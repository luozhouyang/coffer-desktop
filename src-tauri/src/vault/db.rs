//! SQLCipher connection handling and shared data access helpers.

use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, params};
use zeroize::Zeroizing;

use super::migrations::{FtsTokenizer, migrate};
use crate::error::AppError;
use crate::models::{CategorySource, ItemKind, ItemMeta};

/// Current Unix time in milliseconds.
pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Opens a SQLCipher connection with the raw derived key and hardened pragmas.
pub fn open_encrypted(path: &Path, db_key: &Zeroizing<[u8; 32]>) -> Result<Connection, AppError> {
    let conn = Connection::open(path)?;
    let key_hex = hex::encode(db_key.as_ref());
    // Raw key via `x'<hex>'`: SQLCipher skips its own PBKDF2.
    conn.pragma_update(None, "key", format!("x'{key_hex}'"))?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "temp_store", "MEMORY")?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    // Fails here (not later) when the key is wrong or the file is corrupt.
    match conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| {
        r.get::<_, i64>(0)
    }) {
        Ok(_) => {}
        Err(rusqlite::Error::SqliteFailure(code, _))
            if code.code == rusqlite::ErrorCode::NotADatabase =>
        {
            return Err(AppError::internal("vault database could not be decrypted"));
        }
        Err(e) => return Err(e.into()),
    }
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    Ok(conn)
}

/// An unlocked, encrypted vault database.
pub struct VaultDb {
    conn: Arc<Mutex<Connection>>,
    tokenizer: FtsTokenizer,
}

impl VaultDb {
    /// Creates and migrates a new database file.
    pub fn create(path: &Path, db_key: &Zeroizing<[u8; 32]>) -> Result<Self, AppError> {
        let conn = open_encrypted(path, db_key)?;
        let tokenizer = migrate(&conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            tokenizer,
        })
    }

    /// Opens an existing database, verifying and upgrading the schema.
    pub fn open(path: &Path, db_key: &Zeroizing<[u8; 32]>) -> Result<Self, AppError> {
        Self::create(path, db_key)
    }

    pub fn tokenizer(&self) -> FtsTokenizer {
        self.tokenizer
    }

    /// Borrows the connection. A poisoned mutex means another thread panicked
    /// while holding the connection; we surface it as an internal error.
    pub fn with<T>(
        &self,
        f: impl FnOnce(&Connection) -> Result<T, AppError>,
    ) -> Result<T, AppError> {
        let guard: MutexGuard<'_, Connection> = self
            .conn
            .lock()
            .map_err(|_| AppError::internal("database unavailable"))?;
        f(&guard)
    }

    /// Runs `f` inside a write transaction.
    pub fn write<T>(
        &self,
        f: impl FnOnce(&rusqlite::Transaction<'_>) -> Result<T, AppError>,
    ) -> Result<T, AppError> {
        self.with(|conn| {
            let tx = conn.unchecked_transaction()?;
            let out = f(&tx)?;
            tx.commit()?;
            Ok(out)
        })
    }

    /// Checkpoints WAL and closes the connection. Must be called before the
    /// database key is zeroized.
    pub fn close(self) {
        if let Ok(conn) = Arc::try_unwrap(self.conn)
            && let Ok(conn) = conn.into_inner()
        {
            let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
            let _ = conn.close();
        }
    }
}

/// Returns the tags for an item, ordered alphabetically.
pub fn item_tags(conn: &Connection, item_id: &str) -> Result<Vec<String>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT t.name FROM tags t JOIN item_tags it ON it.tag_id = t.id
         WHERE it.item_id = ?1 ORDER BY t.name COLLATE NOCASE",
    )?;
    let rows = stmt
        .query_map(params![item_id], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Returns the tags of many items in one query, keyed by item id.
pub fn tags_for_items(
    conn: &Connection,
    ids: &[String],
) -> Result<std::collections::HashMap<String, Vec<String>>, AppError> {
    use std::collections::HashMap;
    let mut map: HashMap<String, Vec<String>> = HashMap::new();
    if ids.is_empty() {
        return Ok(map);
    }
    // Batch the IN-list to stay well below SQLite's variable limit.
    for chunk in ids.chunks(400) {
        let placeholders = std::iter::repeat_n("?", chunk.len())
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT it.item_id, t.name FROM item_tags it JOIN tags t ON t.id = it.tag_id
             WHERE it.item_id IN ({placeholders}) ORDER BY t.name COLLATE NOCASE"
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(chunk), |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (id, tag) = row?;
            map.entry(id).or_default().push(tag);
        }
    }
    Ok(map)
}

/// Maps a joined `items` row (as selected by [`ITEM_META_COLUMNS`]) to `ItemMeta`.
pub fn row_to_meta(row: &rusqlite::Row<'_>, tags: Vec<String>) -> Result<ItemMeta, AppError> {
    let kind: String = row.get(2)?;
    let source: String = row.get(9)?;
    Ok(ItemMeta {
        id: row.get(0)?,
        kind: ItemKind::parse(&kind).ok_or_else(|| AppError::internal("bad item kind"))?,
        title: row.get(1)?,
        username: row.get(3)?,
        url: row.get(4)?,
        project_id: row.get(5)?,
        category: row.get(8)?,
        category_source: CategorySource::parse(&source),
        favorite: row.get::<_, i64>(10)? != 0,
        tags,
        created_at: row.get(11)?,
        updated_at: row.get(12)?,
    })
}

/// Column list matching [`row_to_meta`] for queries over `items` aliased as
/// `i`. Columns 6 and 7 (notes) are unused by `ItemMeta` and kept as `NULL`
/// placeholders to preserve the fixed index layout.
pub const ITEM_META_COLUMNS: &str = "i.id, i.title, i.kind, i.username, i.url, i.project_id, \
     NULL, NULL, i.category, i.category_source, i.favorite, i.created_at, i.updated_at";

/// Loads a live (non-deleted) item's metadata.
pub fn load_meta(conn: &Connection, id: &str) -> Result<ItemMeta, AppError> {
    let sql =
        format!("SELECT {ITEM_META_COLUMNS} FROM items i WHERE i.id = ?1 AND i.deleted_at IS NULL");
    let meta = conn
        .query_row(&sql, params![id], |r| Ok(row_to_meta(r, Vec::new())?))
        .optional()?
        .ok_or_else(|| AppError::not_found("item"))?;
    let tags = item_tags(conn, id)?;
    Ok(ItemMeta { tags, ..meta })
}

/// Ensures *any* project with this id exists (archived allowed).
pub fn project_exists(conn: &Connection, project_id: &str) -> Result<bool, AppError> {
    let exists: Option<i64> = conn
        .query_row(
            "SELECT 1 FROM projects WHERE id = ?1",
            params![project_id],
            |r| r.get(0),
        )
        .optional()?;
    Ok(exists.is_some())
}

/// Replaces the tag set of an item, creating tags on demand.
pub fn set_item_tags(conn: &Connection, item_id: &str, tags: &[String]) -> Result<(), AppError> {
    conn.execute("DELETE FROM item_tags WHERE item_id = ?1", params![item_id])?;
    for tag in tags {
        conn.execute(
            "INSERT OR IGNORE INTO tags(id, name, color) VALUES (?1, ?2, NULL)",
            params![uuid::Uuid::now_v7().to_string(), tag],
        )?;
        conn.execute(
            "INSERT OR IGNORE INTO item_tags(item_id, tag_id)
             SELECT ?1, id FROM tags WHERE name = ?2",
            params![item_id, tag],
        )?;
    }
    Ok(())
}

/// Normalizes and validates a tag list: trims, drops empties, de-duplicates
/// case-insensitively and enforces limits.
pub fn normalize_tags(input: &[String]) -> Result<Vec<String>, AppError> {
    use crate::models::limits;
    let mut out: Vec<String> = Vec::new();
    for raw in input {
        let tag = raw.trim();
        if tag.is_empty() {
            continue;
        }
        if tag.len() > limits::TAG {
            return Err(AppError::validation("tag is too long"));
        }
        if !out.iter().any(|t| t.eq_ignore_ascii_case(tag)) {
            out.push(tag.to_string());
        }
    }
    if out.len() > limits::TAGS_PER_ITEM {
        return Err(AppError::validation("too many tags"));
    }
    Ok(out)
}

/// Rebuilds the FTS row for a single item from the source tables. Keeps the
/// index consistent without SQL triggers (the tags column is aggregated).
///
/// The FTS rowid mirrors the integer `items.rowid` so results can be joined
/// back to `items` cheaply; `item_id` is therefore not stored in the index.
pub fn reindex_fts(conn: &Connection, item_id: &str) -> Result<(), AppError> {
    conn.execute(
        "DELETE FROM items_fts WHERE rowid IN (SELECT rowid FROM items WHERE id = ?1)",
        params![item_id],
    )?;
    conn.execute(
        "INSERT INTO items_fts(rowid, title, username, url, notes, tags)
         SELECT i.rowid, i.title, coalesce(i.username,''), coalesce(i.url,''),
                coalesce(i.notes,''),
                coalesce((SELECT group_concat(t.name, ' ')
                          FROM item_tags it JOIN tags t ON t.id = it.tag_id
                          WHERE it.item_id = i.id), '')
         FROM items i WHERE i.id = ?1 AND i.deleted_at IS NULL",
        params![item_id],
    )?;
    Ok(())
}

/// Writes an audit row. `meta` must never contain secret material.
pub fn audit(
    conn: &Connection,
    action: &str,
    item_id: Option<&str>,
    meta: Option<&str>,
) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO audit_log(ts, action, item_id, meta) VALUES (?1, ?2, ?3, ?4)",
        params![now_ms(), action, item_id, meta],
    )?;
    Ok(())
}

/// Counts non-deleted items.
pub fn count_all(conn: &Connection) -> Result<i64, AppError> {
    Ok(conn.query_row(
        "SELECT count(*) FROM items WHERE deleted_at IS NULL",
        [],
        |r| r.get(0),
    )?)
}

/// Restricts file permissions to the current user where supported.
pub fn restrict_permissions(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    #[cfg(not(unix))]
    let _ = path;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_tags_dedupes_and_trims() {
        let out =
            normalize_tags(&[" aws ".into(), "AWS".into(), "".into(), "prod".into()]).unwrap();
        assert_eq!(out, vec!["aws".to_string(), "prod".to_string()]);
        assert!(normalize_tags(&["x".repeat(100)]).is_err());
        let many: Vec<String> = (0..100).map(|i| format!("t{i}")).collect();
        assert!(normalize_tags(&many).is_err());
    }

    #[test]
    fn now_ms_is_plausible() {
        assert!(now_ms() > 1_700_000_000_000);
    }

    #[test]
    fn sqlcipher_encrypts_on_disk_and_rejects_wrong_key() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("vault.db");
        let key = Zeroizing::new([5u8; 32]);
        {
            let db = VaultDb::create(&path, &key).unwrap();
            db.with(|c| {
                c.execute(
                    "INSERT INTO items VALUES('i1','note','plaintext-title-marker',NULL,NULL,NULL,\
                     'secret-value-marker',NULL,NULL,'default',0,1,1,NULL)",
                    [],
                )?;
                Ok(())
            })
            .unwrap();
            db.close();
        }
        // Nothing readable on disk.
        let raw = std::fs::read(&path).unwrap();
        assert!(!raw.windows(21).any(|w| w == b"plaintext-title-marker"));
        assert!(!raw.windows(19).any(|w| w == b"secret-value-marker"));
        assert!(!raw.starts_with(b"SQLite format 3"));

        // Wrong key cannot open.
        let wrong = Zeroizing::new([6u8; 32]);
        assert!(VaultDb::open(&path, &wrong).is_err());

        // Right key reopens and sees the row.
        let db = VaultDb::open(&path, &key).unwrap();
        let n: i64 = db
            .with(|c| Ok(c.query_row("SELECT count(*) FROM items", [], |r| r.get(0))?))
            .unwrap();
        assert_eq!(n, 1);
        db.close();
    }

    #[test]
    fn foreign_keys_and_pragmas_enforced() {
        let dir = tempfile::tempdir().unwrap();
        let key = Zeroizing::new([1u8; 32]);
        let db = VaultDb::create(&dir.path().join("vault.db"), &key).unwrap();
        db.with(|c| {
            let fk: i64 = c.query_row("PRAGMA foreign_keys", [], |r| r.get(0))?;
            assert_eq!(fk, 1);
            let ts: i64 = c.query_row("PRAGMA temp_store", [], |r| r.get(0))?;
            assert_eq!(ts, 2); // MEMORY
            let mode: String = c.query_row("PRAGMA journal_mode", [], |r| r.get(0))?;
            assert_eq!(mode, "wal");
            // A dangling project reference is rejected.
            assert!(
                c.execute(
                    "INSERT INTO items VALUES('i1','note','t',NULL,NULL,'missing','s',NULL,NULL,\
                     'default',0,1,1,NULL)",
                    [],
                )
                .is_err()
            );
            Ok(())
        })
        .unwrap();
    }
}

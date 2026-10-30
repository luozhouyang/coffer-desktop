//! Versioned schema migrations, applied inside a transaction.
//!
//! Each migration is a plain SQL file under `src-tauri/migrations/`, embedded
//! at compile time. Files are named `NNNN_<slug>.sql` and applied in version
//! order; `MIGRATIONS` must stay contiguous starting at 1.

use rusqlite::{Connection, OptionalExtension, params};

use crate::error::AppError;

/// Embedded migrations as `(version, sql)`, applied in order.
const MIGRATIONS: &[(i64, &str)] = &[(1, include_str!("../../migrations/0001_init.sql"))];

/// Highest schema version this build knows about (0 when there are none).
pub const SCHEMA_VERSION: i64 = match MIGRATIONS.last() {
    Some((version, _)) => *version,
    None => 0,
};

/// FTS5 tokenizer actually in use for a database. New databases always use
/// `Trigram`; `Unicode61` is only read back from databases created by older
/// builds, because the tokenizer cannot be changed after table creation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FtsTokenizer {
    Trigram,
    Unicode61,
}

/// Returns the FTS tokenizer actually created for this database, read back
/// from sqlite_master so a returned connection always reports the truth.
pub fn detect_tokenizer(conn: &Connection) -> Option<FtsTokenizer> {
    let sql: Option<String> = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type='table' AND name='items_fts'",
            [],
            |r| r.get(0),
        )
        .optional()
        .ok()?;
    let sql = sql?;
    if sql.contains("trigram") {
        Some(FtsTokenizer::Trigram)
    } else if sql.contains("unicode61") {
        Some(FtsTokenizer::Unicode61)
    } else {
        None
    }
}

/// Returns the current `user_version` of the schema.
pub fn current_version(conn: &Connection) -> Result<i64, AppError> {
    Ok(conn.query_row("PRAGMA user_version", [], |r| r.get(0))?)
}

/// Applies all migrations needed to reach [`SCHEMA_VERSION`] in one
/// transaction. Returns the FTS tokenizer in use for this database.
pub fn migrate(conn: &Connection) -> Result<FtsTokenizer, AppError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version    INTEGER PRIMARY KEY,
            applied_at INTEGER NOT NULL
        )",
    )?;
    let current = current_version(conn)?;
    if current > SCHEMA_VERSION {
        return Err(AppError::internal("vault schema is newer than this app"));
    }

    let pending: Vec<(i64, &str)> = MIGRATIONS
        .iter()
        .filter(|(version, _)| *version > current)
        .copied()
        .collect();

    if !pending.is_empty() {
        let tx = conn.unchecked_transaction()?;
        let now = super::db::now_ms();
        for (version, sql) in pending {
            tx.execute_batch(sql)?;
            tx.execute(
                "INSERT INTO schema_migrations(version, applied_at) VALUES (?1, ?2)",
                params![version, now],
            )?;
            tx.execute_batch(&format!("PRAGMA user_version = {version}"))?;
        }
        tx.commit()?;
    }

    if current < SCHEMA_VERSION {
        // Fresh schema: 0001 creates the FTS5 index with the `trigram`
        // tokenizer, which the bundled SQLite build always supports.
        Ok(FtsTokenizer::Trigram)
    } else {
        detect_tokenizer(conn).ok_or_else(|| AppError::internal("missing search index"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mem() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("PRAGMA foreign_keys = ON").unwrap();
        c
    }

    #[test]
    fn migrate_is_idempotent() {
        let c = mem();
        migrate(&c).unwrap();
        let v = current_version(&c).unwrap();
        assert_eq!(v, SCHEMA_VERSION);
        // Second run is a no-op.
        migrate(&c).unwrap();
        assert_eq!(current_version(&c).unwrap(), SCHEMA_VERSION);
        let n: i64 = c
            .query_row("SELECT count(*) FROM schema_migrations", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn project_delete_unlinks_instead_of_cascading() {
        let c = mem();
        migrate(&c).unwrap();
        c.execute_batch(
            "INSERT INTO projects VALUES('p1','P',NULL,NULL,NULL,1,1);
             INSERT INTO items VALUES('i1','note','t',NULL,NULL,'p1','s',NULL,NULL,'default',0,1,1,NULL);",
        )
        .unwrap();
        c.execute("DELETE FROM projects WHERE id='p1'", []).unwrap();
        let pid: Option<String> = c
            .query_row("SELECT project_id FROM items WHERE id='i1'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(pid, None);
    }

    #[test]
    fn project_active_name_is_unique_case_insensitively() {
        let c = mem();
        migrate(&c).unwrap();
        c.execute_batch("INSERT INTO projects VALUES('p1','Prod',NULL,NULL,NULL,1,1)")
            .unwrap();
        assert!(
            c.execute_batch("INSERT INTO projects VALUES('p2','prod',NULL,NULL,NULL,1,1)")
                .is_err()
        );
    }

    #[test]
    fn tokenizer_detection_reads_schema() {
        let c = mem();
        let tok = migrate(&c).unwrap();
        assert_eq!(detect_tokenizer(&c), Some(tok));
    }

    #[test]
    fn migrations_are_contiguous() {
        for (index, (version, _)) in MIGRATIONS.iter().enumerate() {
            assert_eq!(
                *version,
                index as i64 + 1,
                "migrations must start at 1 and increment by exactly 1"
            );
        }
        assert_eq!(SCHEMA_VERSION, MIGRATIONS.len() as i64);
    }

    #[test]
    fn migrations_have_no_placeholders() {
        for (version, sql) in MIGRATIONS {
            assert!(
                !sql.contains("%TOKENIZER%"),
                "migration {version} has %TOKENIZER%"
            );
            assert!(
                !sql.contains("%FTS_OPTIONS%"),
                "migration {version} has %FTS_OPTIONS%"
            );
        }
    }
}

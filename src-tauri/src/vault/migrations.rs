//! Versioned schema migrations, applied inside a transaction.

use rusqlite::{Connection, OptionalExtension, params};

use crate::error::AppError;

/// Highest schema version this build knows about.
pub const SCHEMA_VERSION: i64 = 1;

const V1: &[&str] = &[
    r#"CREATE TABLE projects (
        id          TEXT PRIMARY KEY,
        name        TEXT NOT NULL COLLATE NOCASE,
        description TEXT,
        color       TEXT,
        archived_at INTEGER,
        created_at  INTEGER NOT NULL,
        updated_at  INTEGER NOT NULL
    )"#,
    r#"CREATE UNIQUE INDEX projects_active_name
        ON projects(name) WHERE archived_at IS NULL"#,
    r#"CREATE TABLE items (
        id              TEXT PRIMARY KEY,
        kind            TEXT NOT NULL
                        CHECK (kind IN ('apiKey','env','password','note')),
        title           TEXT NOT NULL,
        username        TEXT,
        url             TEXT,
        project_id      TEXT REFERENCES projects(id) ON DELETE SET NULL,
        secret          TEXT NOT NULL,
        notes           TEXT,
        category        TEXT,
        category_source TEXT NOT NULL DEFAULT 'default'
                        CHECK (category_source IN ('manual','rule','import','default')),
        favorite        INTEGER NOT NULL DEFAULT 0,
        created_at      INTEGER NOT NULL,
        updated_at      INTEGER NOT NULL,
        deleted_at      INTEGER
    )"#,
    r#"CREATE TABLE tags (
        id    TEXT PRIMARY KEY,
        name  TEXT NOT NULL UNIQUE,
        color TEXT
    )"#,
    r#"CREATE TABLE item_tags (
        item_id TEXT NOT NULL REFERENCES items(id) ON DELETE CASCADE,
        tag_id  TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
        PRIMARY KEY (item_id, tag_id)
    )"#,
    "CREATE INDEX item_tags_tag ON item_tags(tag_id)",
    "CREATE INDEX items_project ON items(project_id, updated_at DESC)",
    "CREATE INDEX items_unassigned ON items(updated_at DESC) WHERE project_id IS NULL",
    "CREATE INDEX items_updated ON items(updated_at DESC) WHERE deleted_at IS NULL",
    r#"CREATE TABLE audit_log (
        id      INTEGER PRIMARY KEY AUTOINCREMENT,
        ts      INTEGER NOT NULL,
        action  TEXT NOT NULL,
        item_id TEXT,
        meta    TEXT
    )"#,
    "CREATE INDEX audit_log_item ON audit_log(item_id, ts DESC)",
    // External-content FTS index; rows are maintained by application code
    // (see `db::reindex_fts`) because the tags column aggregates item_tags.
    // `%FTS_OPTIONS%` is resolved by `fts_options`.
    r#"CREATE VIRTUAL TABLE items_fts USING fts5(
        title, username, url, notes, tags,
        content='',
        %FTS_OPTIONS%
        tokenize='%TOKENIZER%'
    )"#,
];

/// Returns the FTS5 options line: `contentless_delete=1` when the build
/// supports it (SQLite >= 3.43), otherwise an empty string.
pub fn fts_options(conn: &Connection) -> &'static str {
    let ok = conn
        .execute_batch(
            "CREATE VIRTUAL TABLE temp.__coffer_cld_probe USING fts5(x, content='', \
             contentless_delete=1)",
        )
        .is_ok()
        && conn
            .execute_batch("DROP TABLE temp.__coffer_cld_probe")
            .is_ok();
    if ok { "contentless_delete=1," } else { "" }
}

/// Tokenizer chosen at migration time after probing the build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FtsTokenizer {
    Trigram,
    Unicode61,
}

impl FtsTokenizer {
    pub fn as_str(self) -> &'static str {
        match self {
            FtsTokenizer::Trigram => "trigram",
            FtsTokenizer::Unicode61 => "unicode61",
        }
    }
}

/// Probes whether the SQLite build supports the FTS5 `trigram` tokenizer.
pub fn detect_trigram(conn: &Connection) -> bool {
    conn.execute_batch("CREATE VIRTUAL TABLE temp.__coffer_probe USING fts5(x, tokenize='trigram')")
        .is_ok()
        && conn.execute_batch("DROP TABLE temp.__coffer_probe").is_ok()
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
/// transaction. Returns the tokenizer chosen for this run.
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

    let tokenizer = if current < 1 {
        if detect_trigram(conn) {
            FtsTokenizer::Trigram
        } else {
            FtsTokenizer::Unicode61
        }
    } else {
        detect_tokenizer(conn).ok_or_else(|| AppError::internal("missing search index"))?
    };

    if current < SCHEMA_VERSION {
        let tx = conn.unchecked_transaction()?;
        let now = super::db::now_ms();
        if current < 1 {
            let options = fts_options(&tx);
            for stmt in V1 {
                let sql = stmt
                    .replace("%TOKENIZER%", tokenizer.as_str())
                    .replace("%FTS_OPTIONS%", options);
                tx.execute_batch(&sql)?;
            }
            tx.execute(
                "INSERT INTO schema_migrations(version, applied_at) VALUES (1, ?1)",
                params![now],
            )?;
            tx.execute_batch(&format!("PRAGMA user_version = {SCHEMA_VERSION}"))?;
        }
        tx.commit()?;
    }
    Ok(tokenizer)
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
    fn trigram_probe_does_not_leave_tables() {
        let c = mem();
        let _ = detect_trigram(&c);
        let n: i64 = c
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE name='__coffer_probe'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 0);
    }
}

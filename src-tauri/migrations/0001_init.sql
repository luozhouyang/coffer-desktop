-- 0001: initial Coffer vault schema.

CREATE TABLE projects (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL COLLATE NOCASE,
    description TEXT,
    color       TEXT,
    archived_at INTEGER,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL
);

CREATE UNIQUE INDEX projects_active_name
    ON projects(name) WHERE archived_at IS NULL;

CREATE TABLE items (
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
);

CREATE TABLE tags (
    id    TEXT PRIMARY KEY,
    name  TEXT NOT NULL UNIQUE,
    color TEXT
);

CREATE TABLE item_tags (
    item_id TEXT NOT NULL REFERENCES items(id) ON DELETE CASCADE,
    tag_id  TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    PRIMARY KEY (item_id, tag_id)
);

CREATE INDEX item_tags_tag ON item_tags(tag_id);

CREATE INDEX items_project ON items(project_id, updated_at DESC);

CREATE INDEX items_unassigned ON items(updated_at DESC) WHERE project_id IS NULL;

CREATE INDEX items_updated ON items(updated_at DESC) WHERE deleted_at IS NULL;

CREATE TABLE audit_log (
    id      INTEGER PRIMARY KEY AUTOINCREMENT,
    ts      INTEGER NOT NULL,
    action  TEXT NOT NULL,
    item_id TEXT,
    meta    TEXT
);

CREATE INDEX audit_log_item ON audit_log(item_id, ts DESC);

-- External-content FTS index; rows are maintained by application code
-- (see `db::reindex_fts`) because the tags column aggregates item_tags.
-- The `trigram` tokenizer and `contentless_delete=1` are guaranteed by the
-- bundled SQLCipher build (SQLite >= 3.53).
CREATE VIRTUAL TABLE items_fts USING fts5(
    title, username, url, notes, tags,
    content='',
    contentless_delete=1,
    tokenize='trigram'
);

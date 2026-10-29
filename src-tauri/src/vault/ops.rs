//! Vault domain operations against an open [`VaultDb`]. Commands are thin
//! wrappers over these; all validation happens here so it is testable
//! independently of Tauri.

use rusqlite::{Connection, OptionalExtension, params, params_from_iter};
use uuid::Uuid;

use crate::classify::{self, ClassifyInput};
use crate::error::AppError;
use crate::models::{
    AuditEntry, CategorySource, ItemDetail, ItemFilter, ItemInput, ItemKind, ItemMeta, ItemUpdate,
    Project, ProjectCount, ProjectInput, ProjectUpdate, SidebarCounts, limits,
};
use crate::vault::db::{
    ITEM_META_COLUMNS, VaultDb, audit, count_all, load_meta, normalize_tags, now_ms,
    project_exists, reindex_fts, row_to_meta, set_item_tags, tags_for_items,
};

// ---------------------------------------------------------------------------
// Validation helpers
// ---------------------------------------------------------------------------

fn required_text(value: &str, field: &str, max: usize) -> Result<String, AppError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(AppError::validation(format!("{field} is required")));
    }
    if trimmed.len() > max {
        return Err(AppError::validation(format!("{field} is too long")));
    }
    Ok(trimmed.to_string())
}

fn optional_text(
    value: Option<String>,
    field: &str,
    max: usize,
) -> Result<Option<String>, AppError> {
    match value {
        None => Ok(None),
        Some(v) => {
            let trimmed = v.trim();
            if trimmed.is_empty() {
                return Ok(None);
            }
            if trimmed.len() > max {
                return Err(AppError::validation(format!("{field} is too long")));
            }
            if trimmed.chars().any(|c| c == '\0') {
                return Err(AppError::validation(format!(
                    "{field} contains invalid characters"
                )));
            }
            Ok(Some(trimmed.to_string()))
        }
    }
}

fn validate_secret(secret: &str) -> Result<(), AppError> {
    if secret.trim().is_empty() {
        return Err(AppError::validation("secret is required"));
    }
    if secret.len() > limits::SECRET {
        return Err(AppError::validation("secret is too long"));
    }
    Ok(())
}

fn validate_color(color: Option<String>) -> Result<Option<String>, AppError> {
    match color {
        None => Ok(None),
        Some(c) => {
            let c = c.trim().to_string();
            if c.is_empty() {
                return Ok(None);
            }
            let ok = c.len() == 7
                && c.starts_with('#')
                && c[1..].chars().all(|ch| ch.is_ascii_hexdigit());
            if !ok {
                return Err(AppError::validation("color must be #RRGGBB"));
            }
            Ok(Some(c.to_ascii_uppercase()))
        }
    }
}

// ---------------------------------------------------------------------------
// Counts
// ---------------------------------------------------------------------------

pub fn sidebar_counts(db: &VaultDb) -> Result<SidebarCounts, AppError> {
    db.with(|conn| {
        let all = count_all(conn)?;
        let unassigned = conn.query_row(
            "SELECT count(*) FROM items WHERE deleted_at IS NULL AND project_id IS NULL",
            [],
            |r| r.get(0),
        )?;
        let mut stmt = conn.prepare(
            "SELECT p.id, count(i.id) FROM projects p
             LEFT JOIN items i ON i.project_id = p.id AND i.deleted_at IS NULL
             WHERE p.archived_at IS NULL GROUP BY p.id ORDER BY p.name COLLATE NOCASE",
        )?;
        let projects = stmt
            .query_map([], |r| {
                Ok(ProjectCount {
                    project_id: r.get(0)?,
                    count: r.get(1)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(SidebarCounts {
            all,
            unassigned,
            projects,
        })
    })
}

// ---------------------------------------------------------------------------
// Projects
// ---------------------------------------------------------------------------

fn row_to_project(row: &rusqlite::Row<'_>) -> Result<Project, AppError> {
    Ok(Project {
        id: row.get(0)?,
        name: row.get(1)?,
        description: row.get(2)?,
        color: row.get(3)?,
        archived_at: row.get(4)?,
        item_count: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

const PROJECT_COLUMNS: &str = "p.id, p.name, p.description, p.color, p.archived_at, \
     (SELECT count(*) FROM items i WHERE i.project_id = p.id AND i.deleted_at IS NULL), \
     p.created_at, p.updated_at";

/// Reads a project by id (any archive state).
pub fn get_project(conn: &Connection, id: &str) -> Result<Project, AppError> {
    let sql = format!("SELECT {PROJECT_COLUMNS} FROM projects p WHERE p.id = ?1");
    conn.query_row(&sql, params![id], |r| Ok(row_to_project(r)?))
        .optional()?
        .ok_or_else(|| AppError::not_found("project"))
}

pub fn list_projects(db: &VaultDb, include_archived: bool) -> Result<Vec<Project>, AppError> {
    db.with(|conn| {
        let sql = format!(
            "SELECT {PROJECT_COLUMNS} FROM projects p {} ORDER BY p.archived_at IS NOT NULL, \
             p.name COLLATE NOCASE",
            if include_archived {
                ""
            } else {
                "WHERE p.archived_at IS NULL"
            }
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map([], |r| Ok(row_to_project(r)?))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    })
}

fn create_project_in(conn: &Connection, input: &ProjectInput) -> Result<String, AppError> {
    let name = required_text(&input.name, "name", limits::NAME)?;
    let description = optional_text(
        input.description.clone(),
        "description",
        limits::DESCRIPTION,
    )?;
    let color = validate_color(input.color.clone())?;
    let active_exists: Option<i64> = conn
        .query_row(
            "SELECT 1 FROM projects WHERE name = ?1 AND archived_at IS NULL",
            params![name],
            |r| r.get(0),
        )
        .optional()?;
    if active_exists.is_some() {
        return Err(AppError::conflict(
            "a project with this name already exists",
        ));
    }
    let id = Uuid::now_v7().to_string();
    let now = now_ms();
    conn.execute(
        "INSERT INTO projects(id, name, description, color, archived_at, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, NULL, ?5, ?5)",
        params![id, name, description, color, now],
    )?;
    Ok(id)
}

pub fn create_project(db: &VaultDb, input: &ProjectInput) -> Result<Project, AppError> {
    db.write(|tx| {
        let id = create_project_in(tx, input)?;
        audit(tx, "projectChange", None, Some("create"))?;
        get_project(tx, &id)
    })
}

pub fn update_project(db: &VaultDb, input: &ProjectUpdate) -> Result<Project, AppError> {
    db.with(|conn| {
        let mut project = get_project(conn, &input.id)?;
        let now = now_ms();
        if let Some(name) = &input.name {
            let name = required_text(name, "name", limits::NAME)?;
            if name != project.name {
                let archived = project.archived_at.is_some();
                if !archived {
                    let clash: Option<i64> = conn
                        .query_row(
                            "SELECT 1 FROM projects WHERE name = ?1 AND archived_at IS NULL \
                             AND id <> ?2",
                            params![name, project.id],
                            |r| r.get(0),
                        )
                        .optional()?;
                    if clash.is_some() {
                        return Err(AppError::conflict(
                            "a project with this name already exists",
                        ));
                    }
                }
                conn.execute(
                    "UPDATE projects SET name = ?1 WHERE id = ?2",
                    params![name, project.id],
                )?;
                project.name = name;
            }
        }
        if let Some(description) = &input.description {
            let v = optional_text(description.clone(), "description", limits::DESCRIPTION)?;
            conn.execute(
                "UPDATE projects SET description = ?1 WHERE id = ?2",
                params![v, project.id],
            )?;
            project.description = v;
        }
        if let Some(color) = &input.color {
            let v = validate_color(color.clone())?;
            conn.execute(
                "UPDATE projects SET color = ?1 WHERE id = ?2",
                params![v, project.id],
            )?;
            project.color = v;
        }
        let mut changed =
            input.name.is_some() || input.description.is_some() || input.color.is_some();
        if let Some(archived) = input.archived {
            let want = if archived { Some(now) } else { None };
            if archived && project.archived_at.is_none() {
                // Re-activating would resurrect a duplicate active name.
                changed = true;
            }
            if !archived && project.archived_at.is_some() {
                let clash: Option<i64> = conn
                    .query_row(
                        "SELECT 1 FROM projects WHERE name = ?1 AND archived_at IS NULL \
                         AND id <> ?2",
                        params![project.name, project.id],
                        |r| r.get(0),
                    )
                    .optional()?;
                if clash.is_some() {
                    return Err(AppError::conflict(
                        "another active project already uses this name",
                    ));
                }
                changed = true;
            }
            if project.archived_at != want {
                conn.execute(
                    "UPDATE projects SET archived_at = ?1 WHERE id = ?2",
                    params![want, project.id],
                )?;
            }
            project.archived_at = want;
        }
        if changed {
            conn.execute(
                "UPDATE projects SET updated_at = ?1 WHERE id = ?2",
                params![now, project.id],
            )?;
        }
        audit(conn, "projectChange", None, Some("update"))?;
        get_project(conn, &project.id)
    })
}

/// Deletes a project. Items are unlinked (project_id = NULL), never deleted.
pub fn delete_project(db: &VaultDb, id: &str) -> Result<(), AppError> {
    db.write(|tx| {
        let project = get_project(tx, id)?;
        // Explicit unlink first (the FK also has ON DELETE SET NULL).
        tx.execute(
            "UPDATE items SET project_id = NULL, updated_at = ?1
             WHERE project_id = ?2 AND deleted_at IS NULL",
            params![now_ms(), id],
        )?;
        tx.execute("DELETE FROM projects WHERE id = ?1", params![project.id])?;
        audit(tx, "projectChange", None, Some("delete"))?;
        Ok(())
    })
}

// ---------------------------------------------------------------------------
// Items
// ---------------------------------------------------------------------------

pub fn list_items(db: &VaultDb, filter: &ItemFilter) -> Result<Vec<ItemMeta>, AppError> {
    db.with(|conn| {
        let mut sql = format!("SELECT {ITEM_META_COLUMNS} FROM items i WHERE i.deleted_at IS NULL");
        let mut args: Vec<rusqlite::types::Value> = Vec::new();
        if filter.unassigned_only {
            sql.push_str(" AND i.project_id IS NULL");
        } else if let Some(pid) = &filter.project_id {
            sql.push_str(" AND i.project_id = ?");
            args.push(pid.clone().into());
        }
        if let Some(kind) = filter.kind {
            sql.push_str(" AND i.kind = ?");
            args.push(kind.as_str().to_string().into());
        }
        if filter.favorite_only.unwrap_or(false) {
            sql.push_str(" AND i.favorite = 1");
        }
        sql.push_str(" ORDER BY i.updated_at DESC, i.id DESC");
        let mut stmt = conn.prepare(&sql)?;
        let metas = stmt
            .query_map(params_from_iter(args), |r| Ok(row_to_meta(r, Vec::new())?))?
            .collect::<Result<Vec<_>, _>>()?;
        let ids: Vec<String> = metas.iter().map(|m| m.id.clone()).collect();
        let mut tags = tags_for_items(conn, &ids)?;
        Ok(metas
            .into_iter()
            .map(|mut m| {
                if let Some(t) = tags.remove(&m.id) {
                    m.tags = t;
                }
                m
            })
            .collect())
    })
}

pub fn get_item(db: &VaultDb, id: &str) -> Result<ItemDetail, AppError> {
    db.with(|conn| {
        let notes: Option<String> = conn
            .query_row(
                "SELECT notes FROM items WHERE id = ?1 AND deleted_at IS NULL",
                params![id],
                |r| r.get(0),
            )
            .optional()?
            .ok_or_else(|| AppError::not_found("item"))?;
        let meta = load_meta(conn, id)?;
        Ok(ItemDetail {
            meta,
            notes,
            has_secret: true,
        })
    })
}

/// Reads the secret value of a live item. Only used for reveal / copy / deep
/// search inside the backend.
pub fn read_secret(conn: &Connection, id: &str) -> Result<zeroize::Zeroizing<String>, AppError> {
    let value: String = conn
        .query_row(
            "SELECT secret FROM items WHERE id = ?1 AND deleted_at IS NULL",
            params![id],
            |r| r.get(0),
        )
        .optional()?
        .ok_or_else(|| AppError::not_found("item"))?;
    Ok(zeroize::Zeroizing::new(value))
}

fn auto_classify(
    kind: ItemKind,
    title: &str,
    secret: &str,
    url: Option<&str>,
    notes: Option<&str>,
) -> (Option<String>, CategorySource, Vec<String>) {
    let _ = kind;
    // Classification inspects title + secret + url; notes are only appended
    // for the keyword layer so a secret value can never be echoed in a reason.
    let combined_title = match notes {
        Some(n) if !n.is_empty() => format!("{title} {n}"),
        _ => title.to_string(),
    };
    let c = classify::classify(ClassifyInput {
        title: &combined_title,
        value: secret,
        url,
    });
    let source = if c.reason.is_some() && c.category != classify::UNCATEGORIZED {
        CategorySource::Rule
    } else {
        CategorySource::Default
    };
    (Some(c.category), source, c.tags)
}

fn write_item_row(conn: &Connection, input: &ItemInput, now: i64) -> Result<String, AppError> {
    let title = required_text(&input.title, "title", limits::TITLE)?;
    validate_secret(&input.secret)?;
    let username = optional_text(input.username.clone(), "username", limits::USERNAME)?;
    let url = optional_text(input.url.clone(), "url", limits::URL)?;
    let notes = optional_text(input.notes.clone(), "notes", limits::NOTES)?;
    let tags = normalize_tags(&input.tags)?;
    if let Some(pid) = &input.project_id
        && !project_exists(conn, pid)?
    {
        return Err(AppError::not_found("project"));
    }

    let (category, source, auto_tags) = auto_classify(
        input.kind,
        &title,
        &input.secret,
        url.as_deref(),
        notes.as_deref(),
    );
    let mut all_tags = tags;
    for t in auto_tags {
        if !all_tags.iter().any(|x| x.eq_ignore_ascii_case(&t)) {
            all_tags.push(t);
        }
    }
    let all_tags = normalize_tags(&all_tags)?;

    let id = Uuid::now_v7().to_string();
    conn.execute(
        "INSERT INTO items(id, kind, title, username, url, project_id, secret, notes, category,
                           category_source, favorite, created_at, updated_at, deleted_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 0, ?11, ?11, NULL)",
        params![
            id,
            input.kind.as_str(),
            title,
            username,
            url,
            input.project_id,
            input.secret,
            notes,
            category,
            source.as_str(),
            now
        ],
    )?;
    set_item_tags(conn, &id, &all_tags)?;
    reindex_fts(conn, &id)?;
    audit(conn, "create", Some(&id), None)?;
    Ok(id)
}

pub fn create_item(db: &VaultDb, input: &ItemInput) -> Result<ItemMeta, AppError> {
    db.write(|tx| {
        let id = write_item_row(tx, input, now_ms())?;
        load_meta(tx, &id)
    })
}

pub fn update_item(db: &VaultDb, input: &ItemUpdate) -> Result<ItemMeta, AppError> {
    db.write(|tx| {
        let current = load_meta(tx, &input.id)?;
        let kind = input.kind.unwrap_or(current.kind);
        let title = match &input.title {
            Some(t) => required_text(t, "title", limits::TITLE)?,
            None => current.title.clone(),
        };
        let username = match &input.username {
            Some(u) => optional_text(u.clone(), "username", limits::USERNAME)?,
            None => current.username.clone(),
        };
        let url = match &input.url {
            Some(u) => optional_text(u.clone(), "url", limits::URL)?,
            None => current.url.clone(),
        };
        let notes = match &input.notes {
            Some(n) => optional_text(n.clone(), "notes", limits::NOTES)?,
            None => get_notes(tx, &input.id)?,
        };
        let project_id = match &input.project_id {
            Some(p) => p.clone(),
            None => current.project_id.clone(),
        };
        if let Some(pid) = &project_id
            && !project_exists(tx, pid)?
        {
            return Err(AppError::not_found("project"));
        }
        if let Some(secret) = &input.secret {
            validate_secret(secret)?;
        }

        let mut category = current.category.clone();
        let mut category_source = current.category_source;
        let mut auto_tags: Vec<String> = Vec::new();
        let content_changed = input.kind.is_some()
            || input.title.is_some()
            || input.url.is_some()
            || input.secret.is_some();
        if content_changed && category_source != CategorySource::Manual {
            let secret_for_rules = match &input.secret {
                Some(s) => s.clone(),
                None => read_secret(tx, &input.id)?.to_string(),
            };
            let (cat, src, tags) = auto_classify(
                kind,
                &title,
                &secret_for_rules,
                url.as_deref(),
                notes.as_deref(),
            );
            category = cat;
            category_source = src;
            auto_tags = tags;
        }

        tx.execute(
            "UPDATE items SET kind = ?1, title = ?2, username = ?3, url = ?4, project_id = ?5,
                    notes = ?6, category = ?7, category_source = ?8, updated_at = ?9
             WHERE id = ?10 AND deleted_at IS NULL",
            params![
                kind.as_str(),
                title,
                username,
                url,
                project_id,
                notes,
                category,
                category_source.as_str(),
                now_ms(),
                input.id
            ],
        )?;
        if let Some(secret) = &input.secret {
            tx.execute(
                "UPDATE items SET secret = ?1 WHERE id = ?2",
                params![secret, input.id],
            )?;
        }
        // Tags the user sent are authoritative; otherwise keep the existing
        // set and merge any new auto tags. User tags are preserved when the
        // editor saves metadata without touching tags.
        let mut all = match &input.tags {
            Some(tags) => normalize_tags(tags)?,
            None => current.tags.clone(),
        };
        for t in auto_tags {
            if !all.iter().any(|x| x.eq_ignore_ascii_case(&t)) {
                all.push(t);
            }
        }
        set_item_tags(tx, &input.id, &normalize_tags(&all)?)?;
        reindex_fts(tx, &input.id)?;
        audit(tx, "update", Some(&input.id), None)?;
        load_meta(tx, &input.id)
    })
}

fn get_notes(conn: &Connection, id: &str) -> Result<Option<String>, AppError> {
    Ok(conn.query_row(
        "SELECT notes FROM items WHERE id = ?1 AND deleted_at IS NULL",
        params![id],
        |r| r.get(0),
    )?)
}

/// Soft-deletes an item.
pub fn delete_item(db: &VaultDb, id: &str) -> Result<(), AppError> {
    db.write(|tx| {
        let now = now_ms();
        let n = tx.execute(
            "UPDATE items SET deleted_at = ?1, updated_at = ?1 WHERE id = ?2 AND deleted_at IS NULL",
            params![now, id],
        )?;
        if n == 0 {
            return Err(AppError::not_found("item"));
        }
        // `reindex_fts` removes the document because the row is now deleted.
        reindex_fts(tx, id)?;
        audit(tx, "delete", Some(id), None)?;
        Ok(())
    })
}

pub fn set_project(
    db: &VaultDb,
    id: &str,
    project_id: Option<String>,
) -> Result<ItemMeta, AppError> {
    db.write(|tx| {
        let _ = load_meta(tx, id)?;
        if let Some(pid) = &project_id
            && !project_exists(tx, pid)? {
                return Err(AppError::not_found("project"));
            }
        tx.execute(
            "UPDATE items SET project_id = ?1, updated_at = ?2 WHERE id = ?3 AND deleted_at IS NULL",
            params![project_id, now_ms(), id],
        )?;
        audit(tx, "projectChange", Some(id), Some("setProject"))?;
        load_meta(tx, id)
    })
}

pub fn set_category(
    db: &VaultDb,
    id: &str,
    category: Option<String>,
) -> Result<ItemMeta, AppError> {
    db.write(|tx| {
        let _ = load_meta(tx, id)?;
        let (category, source) = match category {
            Some(c) => {
                let c = c.trim().to_string();
                if !classify::is_fixed_category(&c) {
                    return Err(AppError::validation("unknown category"));
                }
                (Some(c), CategorySource::Manual)
            }
            None => (None, CategorySource::Default),
        };
        tx.execute(
            "UPDATE items SET category = ?1, category_source = ?2, updated_at = ?3
             WHERE id = ?4 AND deleted_at IS NULL",
            params![category, source.as_str(), now_ms(), id],
        )?;
        audit(tx, "update", Some(id), Some("category"))?;
        load_meta(tx, id)
    })
}

pub fn toggle_favorite(db: &VaultDb, id: &str) -> Result<ItemMeta, AppError> {
    db.write(|tx| {
        let _ = load_meta(tx, id)?;
        tx.execute(
            "UPDATE items SET favorite = 1 - favorite, updated_at = ?1
             WHERE id = ?2 AND deleted_at IS NULL",
            params![now_ms(), id],
        )?;
        audit(tx, "update", Some(id), Some("favorite"))?;
        load_meta(tx, id)
    })
}

/// Records a reveal; the caller returns the secret to the frontend.
pub fn record_reveal(db: &VaultDb, id: &str) -> Result<(), AppError> {
    db.with(|conn| {
        let _ = load_meta(conn, id)?;
        audit(conn, "reveal", Some(id), None)
    })
}

/// Records a copy.
pub fn record_copy(db: &VaultDb, id: &str) -> Result<(), AppError> {
    db.with(|conn| {
        let _ = load_meta(conn, id)?;
        audit(conn, "copy", Some(id), None)
    })
}

pub fn list_audit(db: &VaultDb, item_id: &str) -> Result<Vec<AuditEntry>, AppError> {
    db.with(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, action, item_id, ts FROM audit_log WHERE item_id = ?1
             ORDER BY ts DESC, id DESC LIMIT 200",
        )?;
        let rows = stmt
            .query_map(params![item_id], |r| {
                Ok(AuditEntry {
                    id: r.get(0)?,
                    action: r.get(1)?,
                    item_id: r.get(2)?,
                    created_at: r.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorCode;
    use crate::vault::migrations::FtsTokenizer;
    use tempfile::TempDir;

    pub(crate) struct Fixture {
        _dir: TempDir,
        pub db: VaultDb,
    }

    pub(crate) fn fixture() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let key = zeroize::Zeroizing::new([7u8; 32]);
        let db = VaultDb::create(&dir.path().join("vault.db"), &key).unwrap();
        assert!(matches!(
            db.tokenizer(),
            FtsTokenizer::Trigram | FtsTokenizer::Unicode61
        ));
        Fixture { _dir: dir, db }
    }

    fn item(title: &str, secret: &str) -> ItemInput {
        ItemInput {
            kind: ItemKind::ApiKey,
            title: title.into(),
            secret: secret.into(),
            username: None,
            url: None,
            project_id: None,
            notes: None,
            tags: vec![],
        }
    }

    #[test]
    fn item_crud_lifecycle() {
        let f = fixture();
        let created = create_item(
            &f.db,
            &item("My Anthropic key", "sk-ant-api03-abcdefghijklmnop"),
        )
        .unwrap();
        assert_eq!(created.category.as_deref(), Some(classify::AI));
        assert_eq!(created.category_source, CategorySource::Rule);
        assert!(created.tags.iter().any(|t| t == "anthropic"));

        let detail = get_item(&f.db, &created.id).unwrap();
        assert!(detail.has_secret);
        assert_eq!(detail.meta.title, "My Anthropic key");

        let updated = update_item(
            &f.db,
            &ItemUpdate {
                id: created.id.clone(),
                title: Some("Renamed".into()),
                username: Some(Some("me".into())),
                ..default_update(&created.id)
            },
        )
        .unwrap();
        assert_eq!(updated.title, "Renamed");
        assert_eq!(updated.username.as_deref(), Some("me"));

        delete_item(&f.db, &created.id).unwrap();
        assert!(matches!(get_item(&f.db, &created.id), Err(e) if e.code == ErrorCode::NotFound));
        assert!(matches!(
            delete_item(&f.db, &created.id),
            Err(e) if e.code == ErrorCode::NotFound
        ));
        let counts = sidebar_counts(&f.db).unwrap();
        assert_eq!(counts.all, 0);
    }

    fn default_update(id: &str) -> ItemUpdate {
        ItemUpdate {
            id: id.to_string(),
            kind: None,
            title: None,
            secret: None,
            username: None,
            url: None,
            project_id: None,
            notes: None,
            tags: None,
        }
    }

    #[test]
    fn validation_limits() {
        let f = fixture();
        let mut bad = item("", "x");
        assert!(matches!(create_item(&f.db, &bad), Err(e) if e.code == ErrorCode::Validation));
        bad = item("ok", "   ");
        assert!(matches!(create_item(&f.db, &bad), Err(e) if e.code == ErrorCode::Validation));
        bad = item(&"t".repeat(limits::TITLE + 1), "x");
        assert!(matches!(create_item(&f.db, &bad), Err(e) if e.code == ErrorCode::Validation));
        bad = item("ok", &"x".repeat(limits::SECRET + 1));
        assert!(matches!(create_item(&f.db, &bad), Err(e) if e.code == ErrorCode::Validation));
        let mut notes = item("ok", "x");
        notes.notes = Some("n".repeat(limits::NOTES + 1));
        assert!(matches!(create_item(&f.db, &notes), Err(e) if e.code == ErrorCode::Validation));
        let mut proj = item("ok", "x");
        proj.project_id = Some("missing".into());
        assert!(matches!(create_item(&f.db, &proj), Err(e) if e.code == ErrorCode::NotFound));
    }

    #[test]
    fn project_lifecycle_and_unlink() {
        let f = fixture();
        let p = create_project(
            &f.db,
            &ProjectInput {
                name: "Prod".into(),
                description: None,
                color: Some("#E2A75A".into()),
            },
        )
        .unwrap();
        assert_eq!(p.color.as_deref(), Some("#E2A75A"));
        assert!(matches!(
            create_project(
                &f.db,
                &ProjectInput {
                    name: " prod ".into(),
                    description: None,
                    color: None
                }
            ),
            Err(e) if e.code == ErrorCode::Conflict
        ));
        let mut it = item("key", "secret-value");
        it.project_id = Some(p.id.clone());
        let created = create_item(&f.db, &it).unwrap();
        assert_eq!(created.project_id.as_deref(), Some(p.id.as_str()));

        let counts = sidebar_counts(&f.db).unwrap();
        assert_eq!(counts.all, 1);
        assert_eq!(counts.unassigned, 0);
        assert_eq!(counts.projects[0].count, 1);

        // Archive then reactivate.
        let archived = update_project(
            &f.db,
            &ProjectUpdate {
                id: p.id.clone(),
                name: None,
                description: None,
                color: None,
                archived: Some(true),
            },
        )
        .unwrap();
        assert!(archived.archived_at.is_some());
        assert!(list_projects(&f.db, false).unwrap().is_empty());
        assert_eq!(list_projects(&f.db, true).unwrap().len(), 1);

        delete_project(&f.db, &p.id).unwrap();
        let after = get_item(&f.db, &created.id).unwrap();
        assert!(after.meta.project_id.is_none());
        let counts = sidebar_counts(&f.db).unwrap();
        assert_eq!(counts.unassigned, 1);
        assert!(counts.projects.is_empty());
    }

    #[test]
    fn manual_category_is_never_overwritten() {
        let f = fixture();
        let created = create_item(&f.db, &item("key", "plain-value")).unwrap();
        assert_eq!(created.category.as_deref(), Some(classify::UNCATEGORIZED));
        assert_eq!(created.category_source, CategorySource::Default);

        let manual = set_category(&f.db, &created.id, Some(classify::NOTES.into())).unwrap();
        assert_eq!(manual.category_source, CategorySource::Manual);

        let mut upd = default_update(&created.id);
        upd.secret = Some("sk-ant-api03-zzzzzzzzzzzzzzzz".into());
        let after = update_item(&f.db, &upd).unwrap();
        assert_eq!(after.category.as_deref(), Some(classify::NOTES));
        assert_eq!(after.category_source, CategorySource::Manual);

        assert!(matches!(
            set_category(&f.db, &created.id, Some("Bogus".into())),
            Err(e) if e.code == ErrorCode::Validation
        ));
    }

    #[test]
    fn filters_and_favorites() {
        let f = fixture();
        let a = create_item(&f.db, &item("a", "s1")).unwrap();
        let mut b_in = item("b", "s2");
        b_in.kind = ItemKind::Password;
        let _b = create_item(&f.db, &b_in).unwrap();
        let fav = toggle_favorite(&f.db, &a.id).unwrap();
        assert!(fav.favorite);

        let all = list_items(&f.db, &ItemFilter::default()).unwrap();
        assert_eq!(all.len(), 2);
        // Both items were created within the same millisecond, so ordering
        // by updated_at falls back to id (UUIDv7, ascending = newest first).
        let titles: Vec<&str> = all.iter().map(|m| m.title.as_str()).collect();
        assert!(titles.contains(&"a") && titles.contains(&"b"));

        let pw = list_items(
            &f.db,
            &ItemFilter {
                kind: Some(ItemKind::Password),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(pw.len(), 1);
        let favs = list_items(
            &f.db,
            &ItemFilter {
                favorite_only: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(favs.len(), 1);
    }

    #[test]
    fn set_project_detaches() {
        let f = fixture();
        let p = create_project(
            &f.db,
            &ProjectInput {
                name: "P".into(),
                description: None,
                color: None,
            },
        )
        .unwrap();
        let it = create_item(&f.db, &item("k", "v")).unwrap();
        let attached = set_project(&f.db, &it.id, Some(p.id.clone())).unwrap();
        assert_eq!(attached.project_id.as_deref(), Some(p.id.as_str()));
        let detached = set_project(&f.db, &it.id, None).unwrap();
        assert!(detached.project_id.is_none());
    }

    #[test]
    fn audit_entries_recorded() {
        let f = fixture();
        let it = create_item(&f.db, &item("k", "v")).unwrap();
        let _ = update_item(
            &f.db,
            &ItemUpdate {
                title: Some("k2".into()),
                ..default_update(&it.id)
            },
        )
        .unwrap();
        record_reveal(&f.db, &it.id).unwrap();
        record_copy(&f.db, &it.id).unwrap();
        let entries = list_audit(&f.db, &it.id).unwrap();
        let actions: Vec<&str> = entries.iter().map(|e| e.action.as_str()).collect();
        assert!(actions.contains(&"create"));
        assert!(actions.contains(&"update"));
        assert!(actions.contains(&"reveal"));
        assert!(actions.contains(&"copy"));
    }

    #[test]
    fn env_item_stores_structured_json_and_searches_by_tags() {
        let f = fixture();
        let env = create_item(
            &f.db,
            &ItemInput {
                kind: ItemKind::Env,
                title: "service env".into(),
                secret: "env-secret-json".into(),
                username: None,
                url: None,
                project_id: None,
                notes: Some("deployment notes".into()),
                tags: vec!["Deploy".into(), "deploy".into(), "".into()],
            },
        )
        .unwrap();
        assert_eq!(env.kind, ItemKind::Env);
        // Tags are de-duplicated case-insensitively.
        assert_eq!(
            env.tags
                .iter()
                .filter(|t| t.eq_ignore_ascii_case("deploy"))
                .count(),
            1
        );
        // The secret round-trips verbatim (the UI parses env rows).
        let value = f.db.with(|c| read_secret(c, &env.id)).unwrap();
        assert_eq!(value.as_str(), "env-secret-json");

        // Tags and notes are both searchable; snippets never contain the
        // secret value.
        let by_tag = crate::search::search(
            &f.db,
            "Deploy",
            &crate::search::SearchScope::default(),
            false,
        )
        .unwrap();
        assert_eq!(by_tag.len(), 1);
        assert_eq!(by_tag[0].item.id, env.id);
        let by_notes = crate::search::search(
            &f.db,
            "deployment",
            &crate::search::SearchScope::default(),
            false,
        )
        .unwrap();
        assert_eq!(by_notes[0].matched_field, "notes");
        assert!(
            by_notes[0]
                .snippet
                .as_deref()
                .unwrap_or("")
                .contains("deployment")
        );
        assert!(
            by_notes[0]
                .snippet
                .as_deref()
                .unwrap_or("")
                .find("env-secret")
                .is_none()
        );
    }

    #[test]
    fn project_change_audited() {
        let f = fixture();
        let p = create_project(
            &f.db,
            &ProjectInput {
                name: "Repo".into(),
                description: None,
                color: None,
            },
        )
        .unwrap();
        let it = create_item(&f.db, &item("k", "v")).unwrap();
        set_project(&f.db, &it.id, Some(p.id.clone())).unwrap();
        let entries = list_audit(&f.db, &it.id).unwrap();
        assert!(entries.iter().any(|e| e.action == "projectChange"));
        let project_audit: Vec<String> =
            f.db.with(|c| {
                let mut st = c.prepare("SELECT action FROM audit_log WHERE item_id IS NULL")?;
                let v = st
                    .query_map([], |r| r.get::<_, String>(0))?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(v)
            })
            .unwrap();
        assert!(project_audit.contains(&"projectChange".to_string()));
    }

    #[test]
    fn update_can_clear_fields_and_change_tags() {
        let f = fixture();
        let it = create_item(
            &f.db,
            &ItemInput {
                username: Some("user".into()),
                notes: Some("notes".into()),
                url: Some("https://example.com".into()),
                ..item("k", "v")
            },
        )
        .unwrap();
        let updated = update_item(
            &f.db,
            &ItemUpdate {
                username: Some(None),
                notes: Some(None),
                tags: Some(vec!["new".into()]),
                ..default_update(&it.id)
            },
        )
        .unwrap();
        assert!(updated.username.is_none());
        assert!(updated.tags.iter().any(|t| t == "new"));
        let detail = get_item(&f.db, &it.id).unwrap();
        assert!(detail.notes.is_none());
        assert_eq!(detail.meta.url.as_deref(), Some("https://example.com"));
    }
}

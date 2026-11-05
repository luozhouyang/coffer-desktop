//! `.env` import.
//!
//! The uploaded text is parsed in memory and kept in a short-lived backend
//! session. The preview sent to the renderer only contains names and masked
//! values, so plaintext secrets are never handed to the UI (contract §6).
//! Committing copies from the session straight into SQLCipher.
//!
//! Conflict rule (IMPLEMENTATION_PLAN §4.6): a selected key replaces the item
//! with the same title in the target project; otherwise a new item is created.
//! Manual categories on existing items are preserved.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use rusqlite::{Connection, OptionalExtension, params};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::classify::{ClassifyInput, classify};
use crate::error::AppError;
use crate::models::{
    CategorySource, ImportCommitInput, ImportConflict, ImportEntryPreview, ImportPreview,
    ImportResult, ItemKind, limits,
};
use crate::vault::VaultDb;
use crate::vault::db::{
    audit, item_tags, normalize_tags, now_ms, project_exists, reindex_fts, set_item_tags,
};

/// Import previews expire after ten minutes (contract default).
pub const SESSION_TTL: Duration = Duration::from_secs(10 * 60);
/// Bound on the uploaded file; `.env` files are small.
pub const MAX_TEXT_BYTES: usize = 1024 * 1024;
/// Bound on preview/commit work per file.
pub const MAX_ENTRIES: usize = 500;
/// Only a handful of pending imports can exist at once.
const MAX_SESSIONS: usize = 8;
/// Fixed mask; do not reveal value length or a prefix.
pub const MASKED_VALUE: &str = "••••••";

/// Optional name of the uploaded file (used only to suggest a project).
type FileName<'a> = Option<&'a str>;

struct Entry {
    key: String,
    value: Zeroizing<String>,
    category: String,
    tags: Vec<String>,
    duplicate: bool,
}

struct Session {
    entries: Vec<Entry>,
    created_at: Instant,
}

/// Pending imports. Dropping or clearing the map zeroizes all values held by
/// the `Zeroizing` strings.
pub struct ImportSessions {
    sessions: HashMap<String, Session>,
    ttl: Duration,
}

impl Default for ImportSessions {
    fn default() -> Self {
        Self::new()
    }
}

impl ImportSessions {
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
            ttl: SESSION_TTL,
        }
    }

    #[cfg(test)]
    fn with_ttl(ttl: Duration) -> Self {
        Self {
            sessions: HashMap::new(),
            ttl,
        }
    }

    fn purge(&mut self) {
        let now = Instant::now();
        let ttl = self.ttl;
        self.sessions
            .retain(|_, session| now.duration_since(session.created_at) < ttl);
    }

    /// Drops previews past their TTL. Called by the background tick so
    /// abandoned dialogs do not retain plaintext indefinitely.
    pub fn expire(&mut self) {
        self.purge();
    }

    fn insert(&mut self, id: String, entries: Vec<Entry>) {
        self.purge();
        while self.sessions.len() >= MAX_SESSIONS {
            let oldest = self
                .sessions
                .iter()
                .min_by_key(|(_, session)| session.created_at)
                .map(|(id, _)| id.clone());
            let Some(oldest) = oldest else { break };
            self.sessions.remove(&oldest);
        }
        self.sessions.insert(
            id,
            Session {
                entries,
                created_at: Instant::now(),
            },
        );
    }

    fn take(&mut self, id: &str) -> Option<Session> {
        self.purge();
        self.sessions.remove(id)
    }

    /// Drops every pending preview (lock, restore, shutdown).
    pub fn clear(&mut self) {
        self.sessions.clear();
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.sessions.len()
    }
}

/// Parses one uploaded `.env` file into masked metadata and stores the
/// plaintext in `sessions`.
pub fn begin(
    db: &VaultDb,
    sessions: &mut ImportSessions,
    text: &str,
    file_name: FileName<'_>,
) -> Result<ImportPreview, AppError> {
    if text.len() > MAX_TEXT_BYTES {
        return Err(AppError::validation("the .env file is too large"));
    }
    let (entries, skipped_lines) = parse_env(text);
    if entries.is_empty() {
        return Err(AppError::validation("no variables found in this file"));
    }
    if entries.len() > MAX_ENTRIES {
        return Err(AppError::validation(format!(
            "too many variables (limit {MAX_ENTRIES})"
        )));
    }

    let mut previews = Vec::with_capacity(entries.len());
    db.with(|conn| {
        for (index, entry) in entries.iter().enumerate() {
            previews.push(ImportEntryPreview {
                index: u32::try_from(index).unwrap_or(u32::MAX),
                key: entry.key.clone(),
                masked_value: MASKED_VALUE.to_string(),
                category: Some(entry.category.clone()),
                tags: entry.tags.clone(),
                duplicate: entry.duplicate,
                conflicts: conflicts_for(conn, &entry.key)?,
            });
        }
        Ok(())
    })?;

    let suggested_project_id = suggested_project(db, file_name)?;
    let session_id = Uuid::now_v7().to_string();
    sessions.insert(session_id.clone(), entries);

    Ok(ImportPreview {
        session_id,
        entries: previews,
        skipped_lines,
        suggested_project_id,
    })
}

/// Applies a preview to the vault. The session is consumed even when the
/// commit fails, so a retry starts from a fresh upload.
pub fn commit(
    db: &VaultDb,
    sessions: &mut ImportSessions,
    input: &ImportCommitInput,
) -> Result<ImportResult, AppError> {
    let session = sessions.take(&input.session_id).ok_or_else(|| {
        AppError::validation("this import session has expired; choose the file again")
    })?;

    if input.selected.len() > MAX_ENTRIES {
        return Err(AppError::validation("too many selected variables"));
    }
    let mut seen = HashSet::with_capacity(input.selected.len());
    for index in &input.selected {
        if !seen.insert(*index) {
            return Err(AppError::validation("duplicate import selection"));
        }
        if usize::try_from(*index)
            .ok()
            .is_none_or(|i| i >= session.entries.len())
        {
            return Err(AppError::validation("invalid import selection"));
        }
    }

    let project_id = input.project_id.clone();
    db.write(|tx| {
        if let Some(pid) = &project_id
            && !project_exists(tx, pid)?
        {
            return Err(AppError::not_found("project"));
        }

        let mut created = 0u32;
        let mut updated = 0u32;
        for index in &input.selected {
            let entry = session
                .entries
                .get(*index as usize)
                .ok_or_else(|| AppError::validation("invalid import selection"))?;
            if apply_entry(tx, entry, project_id.as_deref())? {
                updated += 1;
            } else {
                created += 1;
            }
        }
        let skipped =
            u32::try_from(session.entries.len().saturating_sub(input.selected.len())).unwrap_or(0);
        Ok(ImportResult {
            created,
            updated,
            skipped,
        })
    })
}

pub fn cancel(sessions: &mut ImportSessions, session_id: &str) {
    let _ = sessions.take(session_id);
}

/// Inserts or replaces one entry. Returns `true` when an existing item was
/// updated.
fn apply_entry(
    conn: &Connection,
    entry: &Entry,
    project_id: Option<&str>,
) -> Result<bool, AppError> {
    let now = now_ms();
    let existing: Option<(String, CategorySource)> = conn
        .query_row(
            "SELECT id, category_source FROM items
             WHERE deleted_at IS NULL AND title = ?1 COLLATE NOCASE
               AND ((?2 IS NULL AND project_id IS NULL) OR project_id = ?2)
             ORDER BY updated_at DESC LIMIT 1",
            params![entry.key, project_id],
            |r| Ok((r.get(0)?, CategorySource::parse(&r.get::<_, String>(1)?))),
        )
        .optional()?;

    if let Some((id, source)) = existing {
        // Manual categories are never overwritten by automatic rules.
        if source == CategorySource::Manual {
            conn.execute(
                "UPDATE items SET secret = ?1, updated_at = ?2
                 WHERE id = ?3 AND deleted_at IS NULL",
                params![entry.value.as_str(), now, id],
            )?;
        } else {
            conn.execute(
                "UPDATE items SET secret = ?1, category = ?2, category_source = 'import',
                        updated_at = ?3
                 WHERE id = ?4 AND deleted_at IS NULL",
                params![entry.value.as_str(), entry.category, now, id],
            )?;
        }
        merge_tags(conn, &id, &entry.tags)?;
        reindex_fts(conn, &id)?;
        audit(conn, "import", Some(&id), Some("update"))?;
        return Ok(true);
    }

    let id = Uuid::now_v7().to_string();
    // One item per variable keeps each key searchable and copied by value.
    // `env` items stay reserved for grouped KEY=value bundles.
    conn.execute(
        "INSERT INTO items(id, kind, title, username, url, project_id, secret, notes, category,
                           category_source, favorite, created_at, updated_at, deleted_at)
         VALUES (?1, ?2, ?3, NULL, NULL, ?4, ?5, NULL, ?6, 'import', 0, ?7, ?7, NULL)",
        params![
            id,
            ItemKind::ApiKey.as_str(),
            entry.key,
            project_id,
            entry.value.as_str(),
            entry.category,
            now
        ],
    )?;
    set_item_tags(conn, &id, &normalize_tags(&entry.tags)?)?;
    reindex_fts(conn, &id)?;
    audit(conn, "import", Some(&id), Some("create"))?;
    Ok(false)
}

fn merge_tags(conn: &Connection, item_id: &str, incoming: &[String]) -> Result<(), AppError> {
    let mut tags = item_tags(conn, item_id)?;
    for tag in incoming {
        if !tags
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(tag))
        {
            tags.push(tag.clone());
        }
    }
    set_item_tags(conn, item_id, &normalize_tags(&tags)?)
}

fn conflicts_for(conn: &Connection, key: &str) -> Result<Vec<ImportConflict>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, project_id FROM items
         WHERE deleted_at IS NULL AND title = ?1 COLLATE NOCASE
         ORDER BY updated_at DESC, id DESC",
    )?;
    let rows = stmt.query_map(params![key], |r| {
        Ok(ImportConflict {
            item_id: r.get(0)?,
            project_id: r.get(1)?,
        })
    })?;
    // Keep the most recent item per project. The target project is therefore
    // always represented, no matter how many other projects share the title.
    let mut seen: HashSet<Option<String>> = HashSet::new();
    let mut conflicts = Vec::new();
    for row in rows {
        let conflict = row?;
        if seen.insert(conflict.project_id.clone()) {
            conflicts.push(conflict);
        }
    }
    Ok(conflicts)
}

fn suggested_project(db: &VaultDb, file_name: Option<&str>) -> Result<Option<String>, AppError> {
    let Some(hint) = file_name.and_then(project_hint) else {
        return Ok(None);
    };
    db.with(|conn| {
        let id = conn
            .query_row(
                "SELECT id FROM projects WHERE archived_at IS NULL AND name = ?1 COLLATE NOCASE LIMIT 1",
                params![hint],
                |r| r.get::<_, String>(0),
            )
            .optional()?;
        Ok(id)
    })
}

/// `myapp.env` → `myapp`, `prod.env.local` → `prod`, `.env` → none.
fn project_hint(file_name: &str) -> Option<String> {
    let lower = file_name.trim().to_ascii_lowercase();
    let (stem, _) = lower.split_once(".env")?;
    let stem = stem.trim_end_matches(['.', '-', '_', ' ']);
    if stem.is_empty() {
        None
    } else {
        Some(stem.to_string())
    }
}

fn parse_env(text: &str) -> (Vec<Entry>, u32) {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut entries: Vec<Entry> = Vec::new();
    let mut positions: HashMap<String, usize> = HashMap::new();
    let mut skipped = 0u32;

    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line
            .strip_prefix("export ")
            .map(str::trim_start)
            .unwrap_or(line);
        let Some((key, raw_value)) = line.split_once('=') else {
            skipped = skipped.saturating_add(1);
            continue;
        };
        let key = key.trim();
        if !valid_key(key) || key.len() > limits::TITLE {
            skipped = skipped.saturating_add(1);
            continue;
        }
        let Some(value) = parse_value(raw_value) else {
            skipped = skipped.saturating_add(1);
            continue;
        };
        if value.trim().is_empty() || value.len() > limits::SECRET {
            skipped = skipped.saturating_add(1);
            continue;
        }
        let value = Zeroizing::new(value);

        // Titles are compared case-insensitively in the vault, so keys that
        // differ only by case are the same imported entry.
        let normalized = key.to_ascii_uppercase();
        if let Some(position) = positions.get(&normalized).copied() {
            // Contract: duplicate keys are flagged; the last value wins.
            if let Some(entry) = entries.get_mut(position) {
                entry.value = value;
                entry.duplicate = true;
            }
            continue;
        }
        positions.insert(normalized, entries.len());
        entries.push(Entry {
            key: key.to_string(),
            value,
            category: String::new(),
            tags: Vec::new(),
            duplicate: false,
        });
    }

    for entry in &mut entries {
        let classification = classify(ClassifyInput {
            title: &entry.key,
            value: &entry.value,
            url: None,
        });
        entry.category = classification.category;
        entry.tags = classification.tags;
        if !entry.tags.iter().any(|tag| tag == ".env") {
            entry.tags.push(".env".to_string());
        }
        // Defensive: classification tags come from fixed rules, but keep the
        // same limits as a hand-written item.
        entry.tags.truncate(limits::TAGS_PER_ITEM);
    }

    (entries, skipped)
}

fn valid_key(key: &str) -> bool {
    let mut chars = key.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
}

/// Returns `None` for malformed values (e.g. an unterminated quote).
fn parse_value(raw: &str) -> Option<String> {
    let raw = strip_inline_comment(raw).trim();
    if raw.is_empty() {
        return Some(String::new());
    }
    if let Some(inner) = raw.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')) {
        return Some(inner.to_string());
    }
    if let Some(inner) = raw.strip_prefix('"').and_then(|v| v.strip_suffix('"')) {
        return Some(unescape_double_quoted(inner));
    }
    // A quote that is opened but not closed on this line is malformed; we do
    // not support multi-line values.
    if raw.starts_with('"') || raw.starts_with('\'') {
        return None;
    }
    Some(raw.to_string())
}

/// Removes an inline `#` comment that is outside quotes. A `#` inside a quoted
/// value (or immediately after a non-space character) is part of the value.
fn strip_inline_comment(raw: &str) -> &str {
    let bytes = raw.as_bytes();
    let mut quote: Option<u8> = None;
    let mut escaped = false;
    for (i, byte) in bytes.iter().enumerate() {
        let byte = *byte;
        if let Some(active) = quote {
            if escaped {
                escaped = false;
                continue;
            }
            if active == b'"' && byte == b'\\' {
                escaped = true;
                continue;
            }
            if byte == active {
                quote = None;
            }
            continue;
        }
        if byte == b'"' || byte == b'\'' {
            quote = Some(byte);
            continue;
        }
        if byte == b'#' && (i == 0 || bytes[i - 1].is_ascii_whitespace()) {
            return raw[..i].trim_end();
        }
    }
    raw.trim_end()
}

fn unescape_double_quoted(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            Some('$') => out.push('$'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ItemInput, ProjectInput};
    use crate::vault::ops::{create_item, create_project, get_item, list_audit};
    use tempfile::TempDir;

    struct Fixture {
        _dir: TempDir,
        db: VaultDb,
    }

    fn fixture() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let key = Zeroizing::new([5u8; 32]);
        let db = VaultDb::create(&dir.path().join("vault.db"), &key).unwrap();
        Fixture { _dir: dir, db }
    }

    fn project(db: &VaultDb, name: &str) -> String {
        create_project(
            db,
            &ProjectInput {
                name: name.into(),
                description: None,
                color: None,
            },
        )
        .unwrap()
        .id
    }

    fn existing_item(
        db: &VaultDb,
        title: &str,
        secret: &str,
        project_id: Option<String>,
    ) -> String {
        create_item(
            db,
            &ItemInput {
                kind: ItemKind::ApiKey,
                title: title.into(),
                secret: secret.into(),
                username: None,
                url: None,
                project_id,
                notes: None,
                tags: vec![],
            },
        )
        .unwrap()
        .id
    }

    #[test]
    fn parser_handles_dotenv_syntax() {
        let (entries, skipped) = parse_env(
            "\u{feff}# comment\n\
             export OPENAI_API_KEY=\"sk-test\\nvalue\"\n\
             PLAIN=hello # trailing comment\n\
             SINGLE='literal $VALUE'\n\
             QUOTED=\"hi # there\" # trailing\n\
             HASH=abc#def\n\
             COMMENTED= # fill later\n\
             EMPTY=\n\
             BAD LINE\n\
             DUP=one\n\
             DUP=two\n\
             CASE=upper\n\
             case=lower\n\
             1BAD=x\n",
        );
        assert_eq!(skipped, 4, "empty, comment-only, bad line, invalid key");
        assert_eq!(entries.len(), 7);
        let openai = entries.iter().find(|e| e.key == "OPENAI_API_KEY").unwrap();
        assert_eq!(openai.value.as_str(), "sk-test\nvalue");
        assert_eq!(openai.category, "AI / LLM");
        assert!(openai.tags.iter().any(|t| t == ".env"));
        let plain = entries.iter().find(|e| e.key == "PLAIN").unwrap();
        assert_eq!(plain.value.as_str(), "hello");
        let single = entries.iter().find(|e| e.key == "SINGLE").unwrap();
        assert_eq!(single.value.as_str(), "literal $VALUE");
        let quoted = entries.iter().find(|e| e.key == "QUOTED").unwrap();
        assert_eq!(quoted.value.as_str(), "hi # there");
        let hash = entries.iter().find(|e| e.key == "HASH").unwrap();
        assert_eq!(hash.value.as_str(), "abc#def");
        let dup = entries.iter().find(|e| e.key == "DUP").unwrap();
        assert_eq!(dup.value.as_str(), "two");
        assert!(dup.duplicate);
        // Case-insensitive, matching the vault's title comparison.
        let case = entries.iter().find(|e| e.key == "CASE").unwrap();
        assert_eq!(case.value.as_str(), "lower");
        assert!(case.duplicate);
    }

    #[test]
    fn preview_masks_values_and_marks_conflicts() {
        let f = fixture();
        let prod = project(&f.db, "myapp");
        existing_item(&f.db, "OPENAI_API_KEY", "old", Some(prod.clone()));
        let mut sessions = ImportSessions::new();
        let preview = begin(
            &f.db,
            &mut sessions,
            "OPENAI_API_KEY=sk-secret-value\nNEW_KEY=value\n",
            Some("myapp.env"),
        )
        .unwrap();

        assert_eq!(preview.entries.len(), 2);
        assert_eq!(preview.suggested_project_id.as_deref(), Some(prod.as_str()));
        assert_eq!(preview.entries[0].conflicts.len(), 1);
        assert_eq!(
            preview.entries[0].conflicts[0].project_id.as_deref(),
            Some(prod.as_str())
        );
        assert_eq!(preview.entries[0].masked_value, MASKED_VALUE);
        let json = serde_json::to_string(&preview).unwrap();
        assert!(!json.contains("sk-secret-value"));
        assert!(!json.contains("value\""));
        assert_eq!(sessions.len(), 1);
    }

    #[test]
    fn commit_creates_then_replaces_and_preserves_manual_category() {
        let f = fixture();
        let prod = project(&f.db, "Prod");
        let mut sessions = ImportSessions::new();

        let preview = begin(
            &f.db,
            &mut sessions,
            "OPENAI_API_KEY=first\nSTRIPE_SECRET_KEY=sk_live_x\n",
            None,
        )
        .unwrap();
        let result = commit(
            &f.db,
            &mut sessions,
            &ImportCommitInput {
                session_id: preview.session_id.clone(),
                project_id: Some(prod.clone()),
                selected: vec![0, 1],
            },
        )
        .unwrap();
        assert_eq!((result.created, result.updated, result.skipped), (2, 0, 0));
        assert_eq!(sessions.len(), 0, "commit consumes the session");

        let item =
            f.db.with(|c| {
                c.query_row(
                    "SELECT id FROM items WHERE title = 'OPENAI_API_KEY'",
                    [],
                    |r| r.get::<_, String>(0),
                )
                .map_err(AppError::from)
            })
            .unwrap();
        let detail = get_item(&f.db, &item).unwrap();
        assert_eq!(detail.meta.category_source, CategorySource::Import);
        assert_eq!(detail.meta.project_id.as_deref(), Some(prod.as_str()));
        assert!(detail.meta.tags.iter().any(|t| t == ".env"));
        assert!(detail.meta.tags.iter().any(|t| t == "openai"));

        // A manual category survives a re-import.
        f.db.with(|c| {
            c.execute(
                "UPDATE items SET category = 'My category', category_source = 'manual' WHERE id = ?1",
                params![item],
            )
            .map_err(AppError::from)
        })
        .unwrap();

        let preview = begin(&f.db, &mut sessions, "OPENAI_API_KEY=second\n", None).unwrap();
        assert_eq!(preview.entries[0].conflicts.len(), 1);
        let result = commit(
            &f.db,
            &mut sessions,
            &ImportCommitInput {
                session_id: preview.session_id,
                project_id: Some(prod),
                selected: vec![0],
            },
        )
        .unwrap();
        assert_eq!((result.created, result.updated), (0, 1));

        let detail = get_item(&f.db, &item).unwrap();
        assert_eq!(detail.meta.category.as_deref(), Some("My category"));
        assert_eq!(detail.meta.category_source, CategorySource::Manual);
        let secret =
            f.db.with(|c| crate::vault::ops::read_secret(c, &item))
                .unwrap();
        assert_eq!(secret.as_str(), "second");
        let audit = list_audit(&f.db, &item).unwrap();
        assert!(audit.iter().any(|entry| entry.action == "import"));
    }

    #[test]
    fn sessions_expire_without_access() {
        let f = fixture();
        let mut sessions = ImportSessions::with_ttl(Duration::ZERO);
        let _ = begin(&f.db, &mut sessions, "A=1\n", None).unwrap();
        assert_eq!(sessions.len(), 1);
        sessions.expire();
        assert_eq!(
            sessions.len(),
            0,
            "the background tick drops stale plaintext"
        );
    }

    #[test]
    fn conflict_preview_keeps_the_target_project() {
        let f = fixture();
        let target = project(&f.db, "Target");
        existing_item(&f.db, "API_KEY", "old", Some(target.clone()));
        // Many newer items in other projects must not crowd out the target.
        for i in 0..25 {
            let other = project(&f.db, &format!("Other {i}"));
            existing_item(&f.db, "api_key", "old", Some(other));
        }

        let mut sessions = ImportSessions::new();
        let preview = begin(&f.db, &mut sessions, "API_KEY=new\n", None).unwrap();
        let conflicts = &preview.entries[0].conflicts;
        assert!(
            conflicts
                .iter()
                .any(|conflict| conflict.project_id.as_deref() == Some(target.as_str())),
            "preview must represent every project, including the target"
        );
        assert_eq!(conflicts.len(), 26, "one conflict per project");
    }

    #[test]
    fn commit_respects_selection_and_expiry() {
        let f = fixture();
        let mut sessions = ImportSessions::with_ttl(Duration::ZERO);
        let preview = begin(&f.db, &mut sessions, "A=1\nB=2\n", None).unwrap();
        let err = commit(
            &f.db,
            &mut sessions,
            &ImportCommitInput {
                session_id: preview.session_id,
                project_id: None,
                selected: vec![0],
            },
        )
        .unwrap_err();
        assert_eq!(err.code, crate::error::ErrorCode::Validation);

        let mut sessions = ImportSessions::new();
        let preview = begin(&f.db, &mut sessions, "A=1\nB=2\n", None).unwrap();
        let result = commit(
            &f.db,
            &mut sessions,
            &ImportCommitInput {
                session_id: preview.session_id,
                project_id: None,
                selected: vec![1],
            },
        )
        .unwrap();
        assert_eq!((result.created, result.updated, result.skipped), (1, 0, 1));
        let metas = crate::vault::ops::list_items(&f.db, &Default::default()).unwrap();
        assert_eq!(metas.len(), 1);
        assert_eq!(metas[0].title, "B");
    }

    #[test]
    fn commit_rejects_unknown_selection_and_project() {
        let f = fixture();
        let mut sessions = ImportSessions::new();
        let preview = begin(&f.db, &mut sessions, "A=1\n", None).unwrap();
        let err = commit(
            &f.db,
            &mut sessions,
            &ImportCommitInput {
                session_id: preview.session_id.clone(),
                project_id: None,
                selected: vec![9],
            },
        )
        .unwrap_err();
        assert_eq!(err.code, crate::error::ErrorCode::Validation);

        let preview = begin(&f.db, &mut sessions, "A=1\n", None).unwrap();
        let err = commit(
            &f.db,
            &mut sessions,
            &ImportCommitInput {
                session_id: preview.session_id,
                project_id: Some("missing".into()),
                selected: vec![0],
            },
        )
        .unwrap_err();
        assert_eq!(err.code, crate::error::ErrorCode::NotFound);
    }

    #[test]
    fn project_hint_from_file_name() {
        assert_eq!(project_hint("myapp.env").as_deref(), Some("myapp"));
        assert_eq!(project_hint("prod.env.local").as_deref(), Some("prod"));
        assert_eq!(project_hint(".env"), None);
        assert_eq!(project_hint(".env.production"), None);
        assert_eq!(project_hint("secrets.txt"), None);
    }
}

//! Search over item metadata (DESIGN.md §9).
//!
//! - FTS5 over title / username / url / notes / tag names. With the `trigram`
//!   tokenizer every term matches as a substring (tokens ≥ 3 chars).
//! - Secret values are never indexed. `deep` additionally scans decrypted
//!   secrets in memory; such hits carry `matchedField = "secret"` and a `null`
//!   snippet so no secret fragment can reach the frontend.
//! - Empty query returns the 20 most recently updated items.
//! - Project-name queries also surface that project's items.

use std::collections::HashMap;

use rusqlite::{Connection, params, params_from_iter};

use crate::error::AppError;
use crate::models::{ItemMeta, SearchHit, limits};
use crate::vault::db::{ITEM_META_COLUMNS, VaultDb, row_to_meta, tags_for_items};
use crate::vault::migrations::FtsTokenizer;

const RECENT_LIMIT: usize = 20;
const MAX_RESULTS: usize = 100;

/// The scope of a query as chosen by the sidebar.
#[derive(Debug, Clone, Default)]
pub struct SearchScope {
    pub project_id: Option<String>,
    pub unassigned_only: bool,
}

/// Runs a search. `deep` is ignored unless enabled by the caller's setting.
pub fn search(
    db: &VaultDb,
    query: &str,
    scope: &SearchScope,
    deep: bool,
) -> Result<Vec<SearchHit>, AppError> {
    let query = query.trim();
    if query.len() > limits::QUERY {
        return Err(AppError::validation("query is too long"));
    }
    db.with(|conn| {
        if query.is_empty() {
            return recent(conn, scope, RECENT_LIMIT);
        }
        let mut hits = fts_search(conn, query, scope, db.tokenizer())?;
        // Project-name matches widen the result set (DESIGN.md §8.1): the
        // project's items are surfaced, top-ranked, with `matchedField =
        // "project"` unless an earlier (field-level) hit already exists.
        let project_ids: Vec<String> = if scope.unassigned_only {
            Vec::new()
        } else if let Some(pid) = &scope.project_id {
            let name = project_name(conn, pid)?.unwrap_or_default();

            if name.to_lowercase().contains(&query.to_lowercase()) {
                vec![pid.clone()]
            } else {
                vec![]
            }
        } else {
            matching_projects(conn, query)?
        };
        for pid in project_ids.iter().rev() {
            let project_hits = in_project(conn, pid, scope)?;
            for (meta, tags) in project_hits {
                if hits.iter().any(|h| h.item.id == meta.id) {
                    continue;
                }
                hits.push(SearchHit {
                    item: ItemMeta { tags, ..meta },
                    score: 0.5,
                    matched_field: "project".to_string(),
                    snippet: None,
                });
            }
        }
        if deep {
            deep_scan(conn, query, scope, &mut hits)?;
        }
        hits.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(b.item.updated_at.cmp(&a.item.updated_at))
        });
        hits.truncate(MAX_RESULTS);
        Ok(hits)
    })
}

fn scope_clause(scope: &SearchScope) -> (String, Vec<rusqlite::types::Value>) {
    // `items` is always aliased as `i` in every query built here.
    let mut sql = String::from(" AND i.deleted_at IS NULL");
    let mut args = Vec::new();
    if scope.unassigned_only {
        sql.push_str(" AND i.project_id IS NULL");
    } else if let Some(pid) = &scope.project_id {
        sql.push_str(" AND i.project_id = ?");
        args.push(pid.clone().into());
    }
    (sql, args)
}

fn recent(
    conn: &Connection,
    scope: &SearchScope,
    limit: usize,
) -> Result<Vec<SearchHit>, AppError> {
    let (clause, args) = scope_clause(scope);
    let sql = format!(
        "SELECT {ITEM_META_COLUMNS} FROM items i WHERE 1=1 {clause}
         ORDER BY i.updated_at DESC, i.id DESC LIMIT {limit}"
    );
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
            SearchHit {
                item: m,
                score: 0.1,
                matched_field: "title".to_string(),
                snippet: None,
            }
        })
        .collect())
}

/// Builds the FTS MATCH expression: every term quoted (so punctuation and
/// SQL keywords are inert) and AND-ed.
pub fn match_expression(query: &str, tokenizer: FtsTokenizer) -> Option<String> {
    let terms: Vec<&str> = query.split_whitespace().collect();
    if terms.is_empty() {
        return None;
    }
    let mut parts = Vec::new();
    for term in terms {
        if tokenizer == FtsTokenizer::Trigram && term.chars().count() < 3 {
            // trigram cannot index shorter tokens; handled via LIKE instead.
            return None;
        }
        let escaped = term.replace('"', "\"\"");
        if tokenizer == FtsTokenizer::Trigram {
            parts.push(format!("\"{escaped}\""));
        } else {
            parts.push(format!("\"{escaped}\"*"));
        }
    }
    Some(parts.join(" AND "))
}

fn fts_search(
    conn: &Connection,
    query: &str,
    scope: &SearchScope,
    tokenizer: FtsTokenizer,
) -> Result<Vec<SearchHit>, AppError> {
    let (clause, args) = scope_clause(scope);
    let mut hits = Vec::new();
    if let Some(expr) = match_expression(query, tokenizer) {
        let sql = format!(
            "SELECT {ITEM_META_COLUMNS}, bm25(items_fts) AS rank, i.notes
             FROM items_fts JOIN items i ON i.rowid = items_fts.rowid
             WHERE items_fts MATCH ?1 {clause}
             ORDER BY rank, i.updated_at DESC LIMIT {MAX_RESULTS}"
        );
        let mut stmt = conn.prepare(&sql)?;
        let mut params_v: Vec<rusqlite::types::Value> = vec![expr.into()];
        params_v.extend(args.clone());
        let rows = stmt
            .query_map(params_from_iter(params_v), |r| {
                let meta = row_to_meta(r, Vec::new())?;
                let rank: f64 = r.get(13)?;
                let notes: Option<String> = r.get(14)?;
                Ok((meta, rank, notes))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let ids: Vec<String> = rows.iter().map(|(m, _, _)| m.id.clone()).collect();
        let mut tags = tags_for_items(conn, &ids)?;
        for (mut meta, rank, notes) in rows {
            let tag_list = tags.remove(&meta.id).unwrap_or_default();
            let matched = matched_field(&meta, &tag_list, query);
            let snippet = match matched {
                Some(f) => make_snippet(f, &meta, &tag_list, notes.as_deref(), query),
                None => None,
            };
            meta.tags = tag_list;
            hits.push(SearchHit {
                item: meta,
                score: normalize_bm25(rank),
                matched_field: matched.unwrap_or("title").to_string(),
                snippet,
            });
        }
    }
    // Substring fallback for short queries (or when FTS is not usable).
    if hits.len() < MAX_RESULTS {
        let like_hits = like_search(conn, query, scope, MAX_RESULTS - hits.len())?;
        for hit in like_hits {
            if !hits.iter().any(|h| h.item.id == hit.item.id) {
                hits.push(hit);
            }
        }
    }
    Ok(hits)
}

/// Linear LIKE scan for terms the FTS tokenizer cannot index (< 3 chars).
fn like_search(
    conn: &Connection,
    query: &str,
    scope: &SearchScope,
    limit: usize,
) -> Result<Vec<SearchHit>, AppError> {
    let (clause, args) = scope_clause(scope);
    let pattern = format!("%{}%", escape_like(query));
    let sql = format!(
        "SELECT {ITEM_META_COLUMNS}, i.notes FROM items i
         WHERE (i.title LIKE ? ESCAPE '\\' OR coalesce(i.username,'') LIKE ? ESCAPE '\\'
                OR coalesce(i.url,'') LIKE ? ESCAPE '\\' OR coalesce(i.notes,'') LIKE ? ESCAPE '\\'
                OR EXISTS (SELECT 1 FROM item_tags it JOIN tags t ON t.id = it.tag_id
                           WHERE it.item_id = i.id AND t.name LIKE ? ESCAPE '\\'))
         {clause}
         ORDER BY (i.title LIKE ? ESCAPE '\\') DESC, i.updated_at DESC LIMIT {limit}"
    );
    let prefix = format!("{}%", escape_like(query));
    let mut stmt = conn.prepare(&sql)?;
    let mut params_v: Vec<rusqlite::types::Value> = vec![
        pattern.clone().into(),
        pattern.clone().into(),
        pattern.clone().into(),
        pattern.clone().into(),
        pattern.into(),
        prefix.into(),
    ];
    params_v.extend(args);
    let rows = stmt
        .query_map(params_from_iter(params_v), |r| {
            let meta = row_to_meta(r, Vec::new())?;
            let notes: Option<String> = r.get(13)?;
            Ok((meta, notes))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let ids: Vec<String> = rows.iter().map(|(m, _)| m.id.clone()).collect();
    let mut tags = tags_for_items(conn, &ids)?;
    let mut out = Vec::new();
    for (mut meta, notes) in rows {
        let tag_list = tags.remove(&meta.id).unwrap_or_default();
        let matched = matched_field(&meta, &tag_list, query);
        let snippet =
            matched.and_then(|f| make_snippet(f, &meta, &tag_list, notes.as_deref(), query));
        if matched.is_none() {
            continue; // LIKE matched only due to case folding of escapes
        }
        meta.tags = tag_list;
        let prefix_hit = meta.title.to_lowercase().starts_with(&query.to_lowercase());
        out.push(SearchHit {
            item: meta,
            score: if prefix_hit { 0.9 } else { 0.6 },
            matched_field: matched.unwrap_or("title").to_string(),
            snippet,
        });
    }
    Ok(out)
}

fn escape_like(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

/// FTS bm25 is more negative for better matches; map into (0, 1].
fn normalize_bm25(rank: f64) -> f64 {
    let score = 1.0 / (1.0 + rank.abs());
    score.clamp(0.05, 1.0)
}

fn matched_field(meta: &ItemMeta, tags: &[String], query: &str) -> Option<&'static str> {
    let q = query.to_lowercase();
    let contains = |s: &str| s.to_lowercase().contains(&q);
    if contains(&meta.title) {
        return Some("title");
    }
    if meta.username.as_deref().is_some_and(contains) {
        return Some("username");
    }
    if meta.url.as_deref().is_some_and(contains) {
        return Some("url");
    }
    if tags.iter().any(|t| contains(t)) {
        return Some("tags");
    }
    // Notes are checked by the caller via snippet generation; report them
    // only when nothing else matched.
    Some("notes")
}

/// Builds a short snippet around the first match. Only metadata fields are
/// ever shown; secrets are excluded by construction.
fn make_snippet(
    field: &str,
    meta: &ItemMeta,
    tags: &[String],
    notes: Option<&str>,
    query: &str,
) -> Option<String> {
    let text = match field {
        "title" => meta.title.clone(),
        "username" => meta.username.clone()?,
        "url" => meta.url.clone()?,
        "tags" => tags.join(", "),
        "notes" => notes?.to_string(),
        _ => return None,
    };
    Some(truncate_around(&text, query, 60))
}

fn truncate_around(text: &str, query: &str, width: usize) -> String {
    let lower = text.to_lowercase();
    let q = query.to_lowercase();
    let byte_pos = lower.find(&q).unwrap_or(0);
    let char_pos = lower[..byte_pos].chars().count();
    let chars: Vec<char> = text.chars().collect();
    let start = char_pos.saturating_sub(width / 2);
    let end = (start + width).min(chars.len());
    let mut out: String = chars[start..end].iter().collect();
    if start > 0 {
        out.insert(0, '…');
    }
    if end < chars.len() {
        out.push('…');
    }
    out
}

fn project_name(conn: &Connection, id: &str) -> Result<Option<String>, AppError> {
    Ok(conn
        .query_row(
            "SELECT name FROM projects WHERE id = ?1",
            params![id],
            |r| r.get(0),
        )
        .ok())
}

fn matching_projects(conn: &Connection, query: &str) -> Result<Vec<String>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id FROM projects WHERE name LIKE ?1 ESCAPE '\\' AND archived_at IS NULL",
    )?;
    let pattern = format!("%{}%", escape_like(query));
    let ids = stmt
        .query_map(params![pattern], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ids)
}

fn in_project(
    conn: &Connection,
    project_id: &str,
    scope: &SearchScope,
) -> Result<Vec<(ItemMeta, Vec<String>)>, AppError> {
    if scope.unassigned_only {
        return Ok(Vec::new());
    }
    let sql = format!(
        "SELECT {ITEM_META_COLUMNS} FROM items i
         WHERE i.project_id = ?1 AND i.deleted_at IS NULL
         ORDER BY i.updated_at DESC LIMIT {RECENT_LIMIT}"
    );
    let mut stmt = conn.prepare(&sql)?;
    let metas = stmt
        .query_map(params![project_id], |r| Ok(row_to_meta(r, Vec::new())?))?
        .collect::<Result<Vec<_>, _>>()?;
    let ids: Vec<String> = metas.iter().map(|m| m.id.clone()).collect();
    let mut tags = tags_for_items(conn, &ids)?;
    Ok(metas
        .into_iter()
        .map(|m| {
            let t = tags.remove(&m.id).unwrap_or_default();
            (m, t)
        })
        .collect())
}

/// Scans decrypted secrets in memory. Hits are marked `secret` and never get
/// a snippet (contract §6.6).
fn deep_scan(
    conn: &Connection,
    query: &str,
    scope: &SearchScope,
    hits: &mut Vec<SearchHit>,
) -> Result<(), AppError> {
    let (clause, args) = scope_clause(scope);
    let sql = format!(
        "SELECT {ITEM_META_COLUMNS} FROM items i WHERE instr(lower(i.secret), lower(?1)) > 0 {clause}"
    );
    let mut params_v: Vec<rusqlite::types::Value> = vec![query.to_string().into()];
    params_v.extend(args);
    let mut stmt = conn.prepare(&sql)?;
    let metas = stmt
        .query_map(params_from_iter(params_v), |r| {
            Ok(row_to_meta(r, Vec::new())?)
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let ids: Vec<String> = metas.iter().map(|m| m.id.clone()).collect();
    let mut tags = tags_for_items(conn, &ids)?;
    let existing: HashMap<String, usize> = hits
        .iter()
        .enumerate()
        .map(|(i, h)| (h.item.id.clone(), i))
        .collect();
    for mut meta in metas {
        if let Some(&idx) = existing.get(&meta.id)
            && hits[idx].matched_field != "project"
        {
            hits[idx].matched_field = "secret".to_string();
            hits[idx].snippet = None;
            continue;
        }
        let t = tags.remove(&meta.id).unwrap_or_default();
        meta.tags = t;
        hits.push(SearchHit {
            item: meta,
            score: 0.3,
            matched_field: "secret".to_string(),
            snippet: None,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorCode;
    use crate::models::{ItemInput, ItemKind};
    use crate::vault::VaultDb;
    use crate::vault::db::set_item_tags;
    use crate::vault::ops::{create_item, create_project};

    struct Fx {
        _dir: tempfile::TempDir,
        db: VaultDb,
    }

    fn fixture() -> Fx {
        let dir = tempfile::tempdir().unwrap();
        let key = zeroize::Zeroizing::new([3u8; 32]);
        let db = VaultDb::create(&dir.path().join("vault.db"), &key).unwrap();
        Fx { _dir: dir, db }
    }

    fn add(
        db: &VaultDb,
        title: &str,
        secret: &str,
        username: Option<&str>,
        notes: Option<&str>,
    ) -> String {
        let item = create_item(
            db,
            &ItemInput {
                kind: ItemKind::ApiKey,
                title: title.into(),
                secret: secret.into(),
                username: username.map(str::to_string),
                url: None,
                project_id: None,
                notes: notes.map(str::to_string),
                tags: vec![],
            },
        )
        .unwrap();
        item.id
    }

    #[test]
    fn empty_query_returns_recent_20() {
        let f = fixture();
        for i in 0..25 {
            add(&f.db, &format!("item {i}"), "secret", None, None);
        }
        let hits = search(&f.db, "", &SearchScope::default(), false).unwrap();
        assert_eq!(hits.len(), 20);
        assert_eq!(hits[0].item.title, "item 24");
    }

    #[test]
    fn finds_by_title_username_and_tags() {
        let f = fixture();
        let id = add(&f.db, "Production OpenAI", "value", Some("alice"), None);
        f.db.with(|c| set_item_tags(c, &id, &["billing".into()]))
            .unwrap();
        for q in ["production", "openai", "alice", "billing"] {
            let hits = search(&f.db, q, &SearchScope::default(), false).unwrap();
            assert_eq!(hits.len(), 1, "query {q}");
            assert_eq!(hits[0].item.id, id);
        }
    }

    #[test]
    fn does_not_search_secrets_by_default() {
        let f = fixture();
        add(&f.db, "opaque", "supersecretvalue", None, None);
        let hits = search(&f.db, "supersecretvalue", &SearchScope::default(), false).unwrap();
        assert!(hits.is_empty());
    }

    #[test]
    fn deep_search_finds_secret_but_snippet_is_null() {
        let f = fixture();
        let id = add(&f.db, "opaque", "supersecretvalue", None, None);
        let hits = search(&f.db, "supersecret", &SearchScope::default(), true).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].item.id, id);
        assert_eq!(hits[0].matched_field, "secret");
        assert!(hits[0].snippet.is_none());
    }

    #[test]
    fn fts_expression_quotes_terms() {
        let expr = match_expression("foo \"bar", FtsTokenizer::Trigram).unwrap();
        assert_eq!(expr, "\"foo\" AND \"\"\"bar\"");
        assert!(match_expression("ab", FtsTokenizer::Trigram).is_none());
        assert!(match_expression("ab", FtsTokenizer::Unicode61).is_some());
    }

    #[test]
    fn short_query_falls_back_to_like() {
        let f = fixture();
        let id = add(&f.db, "az key", "v", None, None);
        let hits = search(&f.db, "az", &SearchScope::default(), false).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].item.id, id);
    }

    #[test]
    fn sql_metacharacters_are_inert() {
        let f = fixture();
        add(&f.db, "normal", "v", None, None);
        for q in ["'; DROP TABLE items;--", "\"", "*", "NEAR(", "100%"] {
            let hits = search(&f.db, q, &SearchScope::default(), false).unwrap();
            assert!(hits.is_empty(), "query {q}");
        }
        assert_eq!(
            search(&f.db, "", &SearchScope::default(), false)
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn project_filter_and_unassigned() {
        let f = fixture();
        let p = create_project(
            &f.db,
            &crate::models::ProjectInput {
                name: "Prometheus".into(),
                description: None,
                color: None,
            },
        )
        .unwrap();
        let assigned = create_item(
            &f.db,
            &ItemInput {
                kind: ItemKind::Note,
                title: "metrics token".into(),
                secret: "v".into(),
                username: None,
                url: None,
                project_id: Some(p.id.clone()),
                notes: None,
                tags: vec![],
            },
        )
        .unwrap();
        add(&f.db, "loose token", "v", None, None);

        // Filter by project: the query matches both items, but the scope
        // (like the sidebar) restricts results to the selected project.
        let scope = SearchScope {
            project_id: Some(p.id.clone()),
            unassigned_only: false,
        };
        let hits = search(&f.db, "loose", &scope, false).unwrap();
        assert!(hits.is_empty());
        let hits = search(&f.db, "token", &scope, false).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].item.id, assigned.id);

        // Unassigned only.
        let scope = SearchScope {
            project_id: None,
            unassigned_only: true,
        };
        let hits = search(&f.db, "token", &scope, false).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].item.title, "loose token");

        // A query matching only the project name surfaces its items.
        let hits = search(&f.db, "Prometheus", &SearchScope::default(), false).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].item.id, assigned.id);
        assert_eq!(hits[0].matched_field, "project");
        assert!(hits[0].snippet.is_none());

        // Field matches take precedence over the project-name hit.
        let hits = search(&f.db, "Prom", &SearchScope::default(), false).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].matched_field, "project");

        // The project filter still applies to project-name hits.
        let scope = SearchScope {
            project_id: None,
            unassigned_only: true,
        };
        assert!(
            search(&f.db, "Prometheus", &scope, false)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn chinese_substring_with_trigram() {
        let f = fixture();
        if f.db.tokenizer() != FtsTokenizer::Trigram {
            eprintln!("trigram unavailable in this SQLite build; skipping");
            return;
        }
        add(&f.db, "生产环境 OpenAI 密钥", "v", None, None);
        let hits = search(&f.db, "生产环境", &SearchScope::default(), false).unwrap();
        assert_eq!(hits.len(), 1);
        let hits = search(&f.db, "OpenAI 密钥", &SearchScope::default(), false).unwrap();
        assert_eq!(hits.len(), 1);
        let none = search(&f.db, "测试环境", &SearchScope::default(), false).unwrap();
        assert!(none.is_empty());
    }

    #[test]
    fn query_too_long_rejected() {
        let f = fixture();
        let q = "x".repeat(limits::QUERY + 1);
        assert!(matches!(
            search(&f.db, &q, &SearchScope::default(), false),
            Err(e) if e.code == ErrorCode::Validation
        ));
    }
}

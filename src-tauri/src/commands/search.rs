//! Search command. Requires an unlocked vault.

use tauri::State;

use crate::error::AppError;
use crate::models::SearchHit;
use crate::search::{self as engine, SearchScope};
use crate::state::AppState;

/// `search`. `deep` only takes effect when the `deepSearch` setting is enabled.
#[tauri::command]
#[specta::specta]
pub fn search(
    state: State<'_, AppState>,
    query: String,
    project_id: Option<String>,
    unassigned_only: bool,
    deep: bool,
) -> Result<Vec<SearchHit>, AppError> {
    let db = state.require_db()?;
    let deep = deep && state.settings().deep_search;
    let scope = SearchScope {
        project_id,
        unassigned_only,
    };
    engine::search(&db, &query, &scope, deep)
}

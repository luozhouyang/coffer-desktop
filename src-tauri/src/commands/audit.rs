//! Audit command. Requires an unlocked vault.

use tauri::State;

use crate::error::AppError;
use crate::models::AuditEntry;
use crate::state::AppState;
use crate::vault::ops;

/// `audit_list`
#[tauri::command]
#[specta::specta]
pub fn audit_list(
    state: State<'_, AppState>,
    item_id: String,
) -> Result<Vec<AuditEntry>, AppError> {
    let db = state.require_db()?;
    ops::list_audit(&db, &item_id)
}

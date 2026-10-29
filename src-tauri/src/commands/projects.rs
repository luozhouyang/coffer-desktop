//! Project commands. Require an unlocked vault.

use tauri::State;

use crate::error::AppError;
use crate::models::{Project, ProjectInput, ProjectUpdate, SidebarCounts};
use crate::state::AppState;
use crate::vault::ops;

/// `sidebar_counts`
#[tauri::command]
#[specta::specta]
pub fn sidebar_counts(state: State<'_, AppState>) -> Result<SidebarCounts, AppError> {
    let db = state.require_db()?;
    ops::sidebar_counts(&db)
}

/// `projects_list`
#[tauri::command]
#[specta::specta]
pub fn projects_list(
    state: State<'_, AppState>,
    include_archived: bool,
) -> Result<Vec<Project>, AppError> {
    let db = state.require_db()?;
    ops::list_projects(&db, include_archived)
}

/// `project_create`
#[tauri::command]
#[specta::specta]
pub fn project_create(
    state: State<'_, AppState>,
    input: ProjectInput,
) -> Result<Project, AppError> {
    let db = state.require_db()?;
    ops::create_project(&db, &input)
}

/// `project_update`
#[tauri::command]
#[specta::specta]
pub fn project_update(
    state: State<'_, AppState>,
    input: ProjectUpdate,
) -> Result<Project, AppError> {
    let db = state.require_db()?;
    ops::update_project(&db, &input)
}

/// `project_delete`: unlinks items, never deletes them.
#[tauri::command]
#[specta::specta]
pub fn project_delete(state: State<'_, AppState>, id: String) -> Result<(), AppError> {
    let db = state.require_db()?;
    ops::delete_project(&db, &id)
}

//! Settings commands. Require an unlocked vault (contract §6.2).

use tauri::State;

use crate::error::AppError;
use crate::models::Settings;
use crate::state::AppState;

/// `settings_get`
#[tauri::command]
#[specta::specta]
pub fn settings_get(state: State<'_, AppState>) -> Result<Settings, AppError> {
    state.require_db()?;
    Ok(state.settings())
}

/// `settings_set`
#[tauri::command]
#[specta::specta]
pub fn settings_set(state: State<'_, AppState>, settings: Settings) -> Result<Settings, AppError> {
    state.require_db()?;
    state.apply_settings(settings)
}

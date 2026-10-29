//! Item commands. Require an unlocked vault.

use tauri::{AppHandle, Runtime, State};
use tauri_specta::Event;
use zeroize::Zeroizing;

use crate::classify::{ClassifyInput, classify};
use crate::error::AppError;
use crate::events::ClipboardClearedEvent;
use crate::models::{
    Classification, CopyResult, ItemDetail, ItemFilter, ItemInput, ItemMeta, ItemUpdate,
    REVEAL_HIDE_AFTER_SECS, RevealedSecret, limits,
};
use crate::state::AppState;
use crate::vault::ops;

/// `items_list`
#[tauri::command]
#[specta::specta]
pub fn items_list(
    state: State<'_, AppState>,
    filter: ItemFilter,
) -> Result<Vec<ItemMeta>, AppError> {
    let db = state.require_db()?;
    ops::list_items(&db, &filter)
}

/// `item_get`
#[tauri::command]
#[specta::specta]
pub fn item_get(state: State<'_, AppState>, id: String) -> Result<ItemDetail, AppError> {
    let db = state.require_db()?;
    ops::get_item(&db, &id)
}

/// `item_create`
#[tauri::command]
#[specta::specta]
pub fn item_create(state: State<'_, AppState>, input: ItemInput) -> Result<ItemMeta, AppError> {
    let db = state.require_db()?;
    ops::create_item(&db, &input)
}

/// `item_update`
#[tauri::command]
#[specta::specta]
pub fn item_update(state: State<'_, AppState>, input: ItemUpdate) -> Result<ItemMeta, AppError> {
    let db = state.require_db()?;
    ops::update_item(&db, &input)
}

/// `item_delete` (soft delete)
#[tauri::command]
#[specta::specta]
pub fn item_delete(state: State<'_, AppState>, id: String) -> Result<(), AppError> {
    let db = state.require_db()?;
    ops::delete_item(&db, &id)
}

/// `item_set_project` (`projectId: null` detaches)
#[tauri::command]
#[specta::specta]
pub fn item_set_project(
    state: State<'_, AppState>,
    id: String,
    project_id: Option<String>,
) -> Result<ItemMeta, AppError> {
    let db = state.require_db()?;
    ops::set_project(&db, &id, project_id)
}

/// `item_set_category` (`category: null` resets to default)
#[tauri::command]
#[specta::specta]
pub fn item_set_category(
    state: State<'_, AppState>,
    id: String,
    category: Option<String>,
) -> Result<ItemMeta, AppError> {
    let db = state.require_db()?;
    ops::set_category(&db, &id, category)
}

/// `item_toggle_favorite`
#[tauri::command]
#[specta::specta]
pub fn item_toggle_favorite(state: State<'_, AppState>, id: String) -> Result<ItemMeta, AppError> {
    let db = state.require_db()?;
    ops::toggle_favorite(&db, &id)
}

/// `item_reveal_secret`: the only command that returns a secret value.
#[tauri::command]
#[specta::specta]
pub fn item_reveal_secret(
    state: State<'_, AppState>,
    id: String,
) -> Result<RevealedSecret, AppError> {
    let db = state.require_db()?;
    let value = db.with(|conn| ops::read_secret(conn, &id))?;
    ops::record_reveal(&db, &id)?;
    Ok(RevealedSecret {
        value: value.to_string(),
        hide_after_secs: REVEAL_HIDE_AFTER_SECS,
    })
}

/// `copy_secret`: copies inside Rust and returns only the clear delay.
#[tauri::command]
#[specta::specta]
pub fn copy_secret(state: State<'_, AppState>, id: String) -> Result<CopyResult, AppError> {
    let db = state.require_db()?;
    let value = db.with(|conn| ops::read_secret(conn, &id))?;
    let clear_secs = state.settings().clipboard_clear_secs;
    state
        .clipboard
        .copy(&value, std::time::Duration::from_secs(clear_secs.into()))
        .map_err(|_| AppError::internal("clipboard is unavailable"))?;
    ops::record_copy(&db, &id)?;
    Ok(CopyResult {
        clear_after_secs: clear_secs,
    })
}

/// `classify_preview`: pure local classification, never persisted.
#[tauri::command]
#[specta::specta]
pub fn classify_preview(
    state: State<'_, AppState>,
    title: String,
    value: String,
    url: Option<String>,
) -> Result<Classification, AppError> {
    state.require_db()?;
    if title.len() > limits::TITLE {
        return Err(AppError::validation("title is too long"));
    }
    if value.len() > limits::SECRET {
        return Err(AppError::validation("value is too long"));
    }
    if url.as_ref().is_some_and(|u| u.len() > limits::URL) {
        return Err(AppError::validation("url is too long"));
    }
    // Zeroize the input copy; classification only reads it.
    let value = Zeroizing::new(value);
    Ok(classify(ClassifyInput {
        title: &title,
        value: &value,
        url: url.as_deref(),
    }))
}

/// Emits `clipboard://cleared` (used by the clipboard expiry task).
pub fn emit_clipboard_cleared<R: Runtime>(app: &AppHandle<R>) {
    let _ = ClipboardClearedEvent.emit(app);
}

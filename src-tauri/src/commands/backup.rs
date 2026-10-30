//! Encrypted backup / restore commands.
//!
//! Password verification, snapshotting, cipher work and file I/O all run on a
//! blocking thread; only progress events touch the Tauri handle.

use std::path::PathBuf;

use tauri::{AppHandle, State};
use tauri_specta::Event;
use zeroize::Zeroizing;

use crate::backup;
use crate::error::{AppError, ErrorCode};
use crate::events::{BackupExportProgressEvent, BackupRestoreProgressEvent};
use crate::models::{BackupExportResult, BackupPreview, VaultStatus};
use crate::state::AppState;
use crate::vault::VaultDb;

use super::status_of;
use super::vault::blocking;

fn restore_only() -> AppError {
    AppError::new(
        ErrorCode::AlreadyInitialized,
        "Restore is only available before a vault is created.",
    )
}

/// `backup_export`: writes an encrypted `.coffer` of the whole vault.
#[tauri::command]
#[specta::specta]
pub async fn backup_export(
    app: AppHandle<tauri::Wry>,
    state: State<'_, AppState>,
    destination: String,
    password: String,
) -> Result<BackupExportResult, AppError> {
    let state = state.inner().clone();
    let db = state.require_db()?;
    let destination = PathBuf::from(destination);
    blocking(move || {
        let password = Zeroizing::new(password);
        let mut progress = |phase: &str, done: i64, total: i64| {
            let payload = backup::progress_event(phase, done, total);
            let _ = BackupExportProgressEvent(payload).emit(&app);
        };
        backup::export(&state, &db, &destination, &password, &mut progress)
    })
    .await
}

/// `backup_export_keychain`: export authenticated by Touch ID / the cached
/// master key. Requires biometric unlock to be enabled in Settings.
#[tauri::command]
#[specta::specta]
pub async fn backup_export_keychain(
    app: AppHandle<tauri::Wry>,
    state: State<'_, AppState>,
    destination: String,
) -> Result<BackupExportResult, AppError> {
    let state = state.inner().clone();
    let db = state.require_db()?;
    let destination = PathBuf::from(destination);
    blocking(move || {
        let mut progress = |phase: &str, done: i64, total: i64| {
            let payload = backup::progress_event(phase, done, total);
            let _ = BackupExportProgressEvent(payload).emit(&app);
        };
        backup::export_keychain(&state, &db, &destination, &mut progress)
    })
    .await
}

/// `backup_restore_begin`: verifies an archive and returns a commit preview.
#[tauri::command]
#[specta::specta]
pub async fn backup_restore_begin(
    app: AppHandle<tauri::Wry>,
    state: State<'_, AppState>,
    path: String,
    password: String,
) -> Result<BackupPreview, AppError> {
    if state.paths.initialized() {
        return Err(restore_only());
    }
    let source = PathBuf::from(path);
    let state = state.inner().clone();
    blocking(move || {
        let password = Zeroizing::new(password);
        let mut progress = |phase: &str, done: i64, total: i64| {
            let payload = backup::progress_event(phase, done, total);
            let _ = BackupRestoreProgressEvent(payload).emit(&app);
        };
        backup::restore_begin(&state, &password, &source, &mut progress)
    })
    .await
}

/// `backup_restore_commit`: swaps in the verified DB and unlocks it.
#[tauri::command]
#[specta::specta]
pub async fn backup_restore_commit(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<VaultStatus, AppError> {
    if state.paths.initialized() {
        return Err(restore_only());
    }
    let app_state = state.inner().clone();
    let db: VaultDb = blocking(move || {
        // Holding the store lock keeps a concurrent cancel from deleting the
        // temp DB while it is being renamed into place.
        let mut sessions = app_state
            .restore_sessions
            .lock()
            .map_err(|_| AppError::internal("state busy"))?;
        backup::restore_commit(&app_state, &mut sessions, &session_id)
    })
    .await?;
    state.set_unlocked(db)?;
    Ok(status_of(&state))
}

/// `backup_restore_cancel`: discards the preview and its temp DB.
#[tauri::command]
#[specta::specta]
pub fn backup_restore_cancel(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<(), AppError> {
    let mut sessions = state
        .restore_sessions
        .lock()
        .map_err(|_| AppError::internal("state busy"))?;
    backup::restore_cancel(&mut sessions, &session_id)
}

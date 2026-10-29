//! Vault lifecycle commands: status, create, unlock, lock, touch, change
//! password. These are the only commands callable while locked.

use tauri::{AppHandle, State};
use tauri_specta::Event;

use crate::crypto::KdfParams;
use crate::crypto::header::VaultHeader;
use crate::crypto::keys;
use crate::error::AppError;
use crate::events::LockedEvent;
use crate::models::VaultStatus;
use crate::state::AppState;
use crate::vault::{VaultDb, db};

use super::status_of;

/// Runs Argon2 / SQLCipher work off the async runtime.
async fn blocking<F, T>(f: F) -> Result<T, AppError>
where
    F: FnOnce() -> Result<T, AppError> + Send + 'static,
    T: Send + 'static,
{
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|_| AppError::internal("worker task failed"))?
}

/// `vault_status`
#[tauri::command]
#[specta::specta]
pub fn vault_status(state: State<'_, AppState>) -> VaultStatus {
    status_of(&state)
}

/// `vault_create`
#[tauri::command]
#[specta::specta]
pub async fn vault_create(
    state: State<'_, AppState>,
    password: String,
) -> Result<VaultStatus, AppError> {
    let paths = state.paths.clone();
    if paths.initialized() {
        return Err(AppError::already_initialized());
    }
    let db = blocking(move || {
        paths.ensure_dir()?;
        let created = keys::create(&password, KdfParams::default())?;
        let key = keys::db_key(&created.mk)?;
        let db = VaultDb::create(&paths.db(), &key);
        drop(key);
        drop(created.mk);
        if let Err(e) = db {
            // Never leave a half-created vault the user cannot unlock.
            let _ = std::fs::remove_file(paths.db());
            let _ = std::fs::remove_file(paths.db().with_extension("db-wal"));
            let _ = std::fs::remove_file(paths.db().with_extension("db-shm"));
            return Err(e);
        }
        created.header.store(&paths.header()).inspect_err(|_e| {
            let _ = std::fs::remove_file(paths.db());
        })?;
        db.inspect(|_d| {
            db::restrict_permissions(&paths.db());
            db::restrict_permissions(&paths.header());
        })
    })
    .await?;
    state.set_unlocked(db)?;
    Ok(status_of(&state))
}

/// `vault_unlock`
#[tauri::command]
#[specta::specta]
pub async fn vault_unlock(
    state: State<'_, AppState>,
    password: String,
) -> Result<VaultStatus, AppError> {
    let paths = state.paths.clone();
    let header_path = paths.header();
    if !header_path.exists() {
        return Err(AppError::vault_not_initialized());
    }
    let db_path = paths.db();
    let db = blocking(move || {
        let header = VaultHeader::load(&header_path)?;
        let mk = keys::unlock_with_header(&header, &password)?;
        let key = keys::db_key(&mk)?;
        drop(mk);
        if !db_path.exists() {
            return Err(AppError::internal("vault database is missing"));
        }
        let db = VaultDb::open(&db_path, &key);
        drop(key);
        db
    })
    .await?;
    state.set_unlocked(db)?;
    Ok(status_of(&state))
}

/// `vault_lock`
#[tauri::command]
#[specta::specta]
pub fn vault_lock(app: AppHandle<tauri::Wry>, state: State<'_, AppState>) -> Result<(), AppError> {
    if state.lock() {
        let _ = LockedEvent.emit(&app);
    }
    Ok(())
}

/// `vault_touch_activity`
#[tauri::command]
#[specta::specta]
pub fn vault_touch_activity(state: State<'_, AppState>) -> Result<(), AppError> {
    state.require_db()?;
    state.touch_activity();
    Ok(())
}

/// `vault_change_password`: re-wraps only the master key.
#[tauri::command]
#[specta::specta]
pub async fn vault_change_password(
    state: State<'_, AppState>,
    old_password: String,
    new_password: String,
) -> Result<(), AppError> {
    state.require_db()?;
    let header_path = state.paths.header();
    blocking(move || {
        keys::change_password(
            &header_path,
            &old_password,
            &new_password,
            KdfParams::default(),
        )
    })
    .await
}

//! Optional biometric unlock commands.
//!
//! The password flow is unaffected: enabling just copies the master key into
//! the biometric keychain so a later launch can restore it after Touch ID.
//! Clients hide the entry points when `supported` is false.

use tauri::State;

use crate::crypto::header::VaultHeader;
use crate::crypto::keys::{self, MasterKey};
use crate::error::AppError;
use crate::keychain::KeychainStatus;
use crate::models::VaultStatus;
use crate::state::AppState;
use crate::vault::VaultDb;

use super::status_of;
use super::vault::blocking;

/// `vault_keychain_status`: any state.
#[tauri::command]
#[specta::specta]
pub fn vault_keychain_status(state: State<'_, AppState>) -> KeychainStatus {
    state.keychain.status()
}

/// `vault_keychain_enable`: stores the master key behind biometrics. Requires
/// the master password so enabling is never possible from an unlocked session
/// alone.
#[tauri::command]
#[specta::specta]
pub async fn vault_keychain_enable(
    state: State<'_, AppState>,
    password: String,
) -> Result<KeychainStatus, AppError> {
    enable(&state, password).await
}

async fn enable(state: &AppState, password: String) -> Result<KeychainStatus, AppError> {
    state.require_db()?;
    let status = state.keychain.status();
    if !status.supported || !status.available {
        return Err(AppError::keychain_unavailable(
            "biometric unlock is not available on this device",
        ));
    }
    let header_path = state.paths.header();
    let keychain = state.keychain.clone();
    blocking(move || {
        let header = VaultHeader::load(&header_path)?;
        let mk = keys::unlock_with_header(&header, &password)?;
        keychain
            .store(mk.expose())
            .map_err(AppError::keychain_unavailable)
    })
    .await?;
    Ok(state.keychain.status())
}

/// `vault_keychain_disable`: removes the stored key. Safe in any state.
#[tauri::command]
#[specta::specta]
pub fn vault_keychain_disable(state: State<'_, AppState>) -> Result<KeychainStatus, AppError> {
    state
        .keychain
        .delete()
        .map_err(AppError::keychain_unavailable)?;
    Ok(state.keychain.status())
}

/// `vault_unlock_keychain`: restores the master key from the keychain, which
/// makes the system prompt for Touch ID.
#[tauri::command]
#[specta::specta]
pub async fn vault_unlock_keychain(state: State<'_, AppState>) -> Result<VaultStatus, AppError> {
    unlock(&state).await
}

async fn unlock(state: &AppState) -> Result<VaultStatus, AppError> {
    let paths = state.paths.clone();
    let header_path = paths.header();
    if !header_path.exists() {
        return Err(AppError::vault_not_initialized());
    }
    let db_path = paths.db();
    let keychain = state.keychain.clone();
    let db = blocking(move || {
        let stored = keychain.load().map_err(AppError::keychain_unavailable)?;
        let mk = MasterKey::from_bytes(&stored)?;
        let key = keys::db_key(&mk)?;
        drop(mk);
        drop(stored);
        if !db_path.exists() {
            return Err(AppError::internal("vault database is missing"));
        }
        // A key restored from another vault copy (restored backup, another
        // build) will not open this database. Say so instead of surfacing a
        // raw cipher error so the password path looks like the way out.
        let db = VaultDb::open(&db_path, &key).map_err(|_| {
            AppError::keychain_unavailable(
                "the saved biometric key does not open this vault; unlock with your password and enable Touch ID again",
            )
        });
        drop(key);
        db
    })
    .await?;
    state.set_unlocked(db)?;
    Ok(status_of(state))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clipboard::ClipboardManager;
    use crate::clipboard::tests::FakeClipboard;
    use crate::crypto::KdfParams;
    use crate::error::ErrorCode;
    use crate::keychain::tests::FakeKeychain;
    use crate::vault::paths::VaultPaths;
    use std::sync::Arc;

    const PASSWORD: &str = "correct horse battery";

    fn setup(keychain: FakeKeychain) -> (tempfile::TempDir, AppState) {
        let dir = tempfile::tempdir().unwrap();
        let clipboard = Arc::new(ClipboardManager::new(Arc::new(FakeClipboard::default())));
        let state = AppState::new(
            VaultPaths::from_app_data(dir.path()),
            clipboard,
            Arc::new(keychain),
        );
        (dir, state)
    }

    fn create_vault(state: &AppState) {
        state.paths.ensure_dir().unwrap();
        let fast = KdfParams {
            m_cost: 8,
            t_cost: 1,
            p_cost: 1,
        };
        let created = keys::create(PASSWORD, fast).unwrap();
        let key = keys::db_key(&created.mk).unwrap();
        let db = VaultDb::create(&state.paths.db(), &key).unwrap();
        created.header.store(&state.paths.header()).unwrap();
        state.set_unlocked(db).unwrap();
    }

    fn block_on<T>(f: impl std::future::Future<Output = T>) -> T {
        tauri::async_runtime::block_on(f)
    }

    #[test]
    fn enable_then_unlock_opens_the_vault() {
        let (_dir, state) = setup(FakeKeychain::new());
        create_vault(&state);

        let status = block_on(enable(&state, PASSWORD.into())).unwrap();
        assert!(status.enabled);

        assert!(state.lock());
        assert!(!state.is_unlocked());

        let status = block_on(unlock(&state)).unwrap();
        assert!(status.unlocked);
        assert!(state.is_unlocked());
        let db = state.require_db().unwrap();
        assert_eq!(crate::vault::ops::sidebar_counts(&db).unwrap().all, 0);
    }

    #[test]
    fn enable_requires_the_master_password() {
        let (_dir, state) = setup(FakeKeychain::new());
        create_vault(&state);

        let err = block_on(enable(&state, "wrong password".into())).unwrap_err();
        assert_eq!(err.code, ErrorCode::InvalidPassword);
        assert!(!state.keychain.status().enabled);
    }

    #[test]
    fn enable_is_rejected_without_biometrics() {
        let mut keychain = FakeKeychain::new();
        keychain.available = false;
        let (_dir, state) = setup(keychain);
        create_vault(&state);

        let err = block_on(enable(&state, PASSWORD.into())).unwrap_err();
        assert_eq!(err.code, ErrorCode::KeychainUnavailable);
        assert!(!state.keychain.status().enabled);
    }

    #[test]
    fn unlock_fails_when_no_key_is_stored() {
        let (_dir, state) = setup(FakeKeychain::new());
        create_vault(&state);
        assert!(state.lock());

        let err = block_on(unlock(&state)).unwrap_err();
        assert_eq!(err.code, ErrorCode::KeychainUnavailable);
        assert!(!state.is_unlocked());
    }

    #[test]
    fn unlock_rejects_a_key_from_another_vault() {
        let (_dir, state) = setup(FakeKeychain::new());
        create_vault(&state);
        state.keychain.store(&[9u8; 32]).unwrap();
        assert!(state.lock());

        let err = block_on(unlock(&state)).unwrap_err();
        assert_eq!(err.code, ErrorCode::KeychainUnavailable);
        assert!(!state.is_unlocked());
    }

    #[test]
    fn disable_forgets_the_key() {
        let (_dir, state) = setup(FakeKeychain::new());
        create_vault(&state);
        block_on(enable(&state, PASSWORD.into())).unwrap();
        assert!(state.keychain.status().enabled);

        state.keychain.delete().unwrap();
        assert!(!state.keychain.status().enabled);
        assert!(state.keychain.load().is_err());
    }
}

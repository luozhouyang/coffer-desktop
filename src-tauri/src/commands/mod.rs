//! Tauri command surface. Every command is a thin wrapper that acquires the
//! database (enforcing the locked-state contract) and delegates to the domain
//! modules. Errors are always [`AppError`] / `{ code, message }`.

pub mod audit;
pub mod backup;
pub mod items;
pub mod keychain;
pub mod projects;
pub mod search;
pub mod settings;
pub mod vault;

pub use audit::audit_list;
pub use backup::{
    backup_export, backup_export_keychain, backup_restore_begin, backup_restore_cancel,
    backup_restore_commit,
};
pub use items::{
    classify_preview, copy_secret, item_create, item_delete, item_get, item_reveal_secret,
    item_set_category, item_set_project, item_toggle_favorite, item_update, items_list,
};
pub use keychain::{
    vault_keychain_disable, vault_keychain_enable, vault_keychain_status, vault_unlock_keychain,
};
pub use projects::{project_create, project_delete, project_update, projects_list, sidebar_counts};
pub use search::search;
pub use settings::{settings_get, settings_set};
pub use vault::{
    vault_change_password, vault_create, vault_lock, vault_status, vault_touch_activity,
    vault_unlock,
};

use crate::models::VaultStatus;
use crate::state::AppState;

/// `vault_status`: callable in any state.
pub(crate) fn status_of(state: &AppState) -> VaultStatus {
    let initialized = state.paths.initialized();
    let auto_lock_secs = state.settings().auto_lock_secs;
    if !state.is_unlocked() {
        return VaultStatus::locked(initialized, auto_lock_secs);
    }
    let counts = match state.require_db() {
        Ok(db) => crate::vault::ops::sidebar_counts(&db).unwrap_or_default(),
        Err(_) => Default::default(),
    };
    VaultStatus {
        initialized,
        unlocked: true,
        auto_lock_secs,
        counts,
    }
}

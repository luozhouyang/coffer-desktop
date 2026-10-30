//! Coffer desktop backend.
//!
//! Plaintext secrets and key material only ever live in this process: the
//! frontend receives metadata, and single-shot reveals / internal copies.
//! See `docs/IPC_CONTRACT.md` for the authoritative command surface.

pub mod autolock;
pub mod backup;
pub mod classify;
pub mod clipboard;
pub mod commands;
pub mod crypto;
pub mod error;
pub mod events;
pub mod keychain;
pub mod models;
pub mod search;
pub mod settings;
pub mod state;
pub mod vault;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tauri::{AppHandle, Manager, Runtime};
use tauri_specta::{Builder, Event, collect_commands, collect_events};

use crate::commands::search::search;
use crate::commands::{
    audit_list, backup_export, backup_export_keychain, backup_restore_begin, backup_restore_cancel,
    backup_restore_commit, classify_preview, copy_secret, item_create, item_delete, item_get,
    item_reveal_secret, item_set_category, item_set_project, item_toggle_favorite, item_update,
    items_list, project_create, project_delete, project_update, projects_list, settings_get,
    settings_set, sidebar_counts, vault_change_password, vault_create, vault_keychain_disable,
    vault_keychain_enable, vault_keychain_status, vault_lock, vault_status, vault_touch_activity,
    vault_unlock, vault_unlock_keychain,
};

use crate::autolock::Tick;
use crate::clipboard::{ArboardClipboard, ClipboardManager};
use crate::events::{
    BackupExportProgressEvent, BackupRestoreProgressEvent, ClipboardClearedEvent, LockWarningEvent,
    LockedEvent,
};
use crate::keychain::PlatformKeychain;
use crate::models::events::LockWarning;
use crate::state::AppState;
use crate::vault::paths::VaultPaths;

/// Background task cadence. 1s keeps warnings and auto-lock within a second.
const TICK: Duration = Duration::from_secs(1);

/// Type-safe command + event bindings shared with the frontend. TypeScript is
/// exported to `src-tauri/bindings.ts` (see `run`).
pub fn specta_builder() -> Builder<tauri::Wry> {
    Builder::<tauri::Wry>::new()
        .commands(collect_commands![
            vault_status,
            vault_create,
            vault_unlock,
            vault_lock,
            vault_touch_activity,
            vault_change_password,
            vault_keychain_status,
            vault_keychain_enable,
            vault_keychain_disable,
            vault_unlock_keychain,
            settings_get,
            settings_set,
            sidebar_counts,
            projects_list,
            project_create,
            project_update,
            project_delete,
            items_list,
            item_get,
            item_create,
            item_update,
            item_delete,
            item_set_project,
            item_set_category,
            item_toggle_favorite,
            item_reveal_secret,
            copy_secret,
            search,
            classify_preview,
            audit_list,
            backup_export,
            backup_export_keychain,
            backup_restore_begin,
            backup_restore_commit,
            backup_restore_cancel,
        ])
        .events(collect_events![
            LockedEvent,
            LockWarningEvent,
            ClipboardClearedEvent,
            BackupExportProgressEvent,
            BackupRestoreProgressEvent
        ])
}

/// Application data root used for the vault.
fn data_root<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<PathBuf> {
    app.path().app_data_dir()
}

/// Locks the vault and notifies the frontend.
pub fn lock_vault<R: Runtime>(app: &AppHandle<R>) {
    let locked = app
        .try_state::<AppState>()
        .is_some_and(|state| state.lock());
    if locked {
        let _ = LockedEvent.emit(app);
    }
}

fn spawn_background_tasks<R: Runtime>(app: AppHandle<R>) {
    let handle = app.clone();
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(TICK);
            let Some(state) = handle.try_state::<AppState>() else {
                continue;
            };
            if !state.is_unlocked() {
                continue;
            }
            let tick = state
                .lock_autolock()
                .map(|mut a| a.tick(std::time::Instant::now()))
                .unwrap_or(Tick::Idle);
            match tick {
                Tick::Lock => lock_vault(&handle),
                Tick::Warn(seconds) => {
                    let _ = LockWarningEvent(LockWarning {
                        seconds: u32::try_from(seconds.max(0)).unwrap_or(0),
                    })
                    .emit(&handle);
                }
                Tick::Idle => {}
            }
        }
    });

    let clipboard_handle = app;
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(TICK);
            if let Some(state) = clipboard_handle.try_state::<AppState>()
                && state.clipboard.try_expire()
            {
                let _ = ClipboardClearedEvent.emit(&clipboard_handle);
            }
        }
    });
}

/// Builds shared state for the given data directory. Also used by tests.
pub fn build_state(app_data_dir: PathBuf) -> AppState {
    let clipboard = Arc::new(ClipboardManager::new(Arc::new(ArboardClipboard)));
    let keychain = Arc::new(PlatformKeychain);
    AppState::new(
        VaultPaths::from_app_data(&app_data_dir),
        clipboard,
        keychain,
    )
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // `specta_builder` is the single source of truth for typed commands and
    // events. Its TypeScript output (`src-tauri/bindings.ts`) is generated by
    // the dedicated export test, so `run` never writes to the source tree.
    let builder = specta_builder();

    let invoke_handler = builder.invoke_handler();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .invoke_handler(invoke_handler)
        .setup(move |app| {
            builder.mount_events(app);
            let root = data_root(app.handle())?;
            let state = build_state(root);
            // Sessions died with the previous process; their temp files did not.
            if let Err(e) = crate::backup::session::cleanup_on_startup(state.paths.root()) {
                eprintln!("coffer: backup temp cleanup failed ({})", e.code);
            }
            app.manage(state);
            spawn_background_tasks(app.handle().clone());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;
    use specta_typescript::Typescript;

    /// Every command in IPC contract §3, by invoke name.
    const CONTRACT_COMMANDS: &[&str] = &[
        "vault_status",
        "vault_create",
        "vault_unlock",
        "vault_lock",
        "vault_touch_activity",
        "vault_change_password",
        "vault_keychain_status",
        "vault_keychain_enable",
        "vault_keychain_disable",
        "vault_unlock_keychain",
        "settings_get",
        "settings_set",
        "sidebar_counts",
        "projects_list",
        "project_create",
        "project_update",
        "project_delete",
        "items_list",
        "item_get",
        "item_create",
        "item_update",
        "item_delete",
        "item_set_project",
        "item_set_category",
        "item_toggle_favorite",
        "item_reveal_secret",
        "copy_secret",
        "search",
        "classify_preview",
        "audit_list",
        "backup_export",
        "backup_export_keychain",
        "backup_restore_begin",
        "backup_restore_commit",
        "backup_restore_cancel",
    ];

    fn export() -> String {
        let builder = specta_builder();
        let dir = std::env::temp_dir().join(format!(
            "coffer-bindings-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("bindings.ts");
        builder
            .export(Typescript::default(), &path)
            .expect("export bindings");
        let text = std::fs::read_to_string(&path).expect("bindings.ts");
        let _ = std::fs::remove_dir_all(&dir);
        text
    }

    #[test]
    fn bindings_cover_contract_commands_and_events() {
        let text = export();
        for cmd in CONTRACT_COMMANDS {
            assert!(
                text.contains(&format!("\"{cmd}\"")),
                "missing invoke name {cmd}"
            );
        }
        for event in [
            "vault://locked",
            "vault://lock-warning",
            "clipboard://cleared",
            "backup://export-progress",
            "backup://restore-progress",
        ] {
            assert!(text.contains(event), "missing event {event}");
        }
        assert!(text.contains("export type BackupProgress = {"));
        assert!(text.contains("export type BackupExportResult = {"));
        assert!(text.contains("export type BackupPreview = {"));
        assert!(text.contains("export type BackupExportProgressEvent = BackupProgress;"));
        assert!(text.contains("export type BackupRestoreProgressEvent = BackupProgress;"));
        // Events payloads per contract §4.
        assert!(text.contains("export type LockedEvent = null;"));
        assert!(text.contains("export type ClipboardClearedEvent = null;"));
        assert!(text.contains("export type LockWarningEvent = LockWarning;"));
        // Error envelope.
        assert!(text.contains("export type AppError = {"));
    }

    /// Keeps `src-tauri/bindings.ts` up to date. Skipped unless asked, to keep
    /// parallel test runs from interleaving writes.
    #[test]
    fn export_bindings() {
        if std::env::var_os("COFFER_EXPORT_BINDINGS").is_none() {
            return;
        }
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/bindings.ts");
        specta_builder()
            .export(Typescript::default(), path)
            .expect("export bindings");
    }
}

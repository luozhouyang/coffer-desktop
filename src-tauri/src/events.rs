//! Typed events emitted to the frontend (IPC contract §4).
//!
//! Event names are fixed by the contract, so they are set explicitly instead
//! of relying on the derive macro's default kebab-case naming.

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri_specta::Event;

use crate::models::events::{BackupProgress, LockWarning};

/// `vault://locked` — payload is `null`.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[tauri_specta(event_name = "vault://locked")]
pub struct LockedEvent;

/// `vault://lock-warning` — `{ seconds }`.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[tauri_specta(event_name = "vault://lock-warning")]
pub struct LockWarningEvent(pub LockWarning);

/// `clipboard://cleared` — payload is `null`.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[tauri_specta(event_name = "clipboard://cleared")]
pub struct ClipboardClearedEvent;

/// `backup://export-progress` — `{ phase, done, total }`.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[tauri_specta(event_name = "backup://export-progress")]
pub struct BackupExportProgressEvent(pub BackupProgress);

/// `backup://restore-progress` — `{ phase, done, total }`.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[tauri_specta(event_name = "backup://restore-progress")]
pub struct BackupRestoreProgressEvent(pub BackupProgress);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_names_match_contract() {
        assert_eq!(LockedEvent::NAME, "vault://locked");
        assert_eq!(LockWarningEvent::NAME, "vault://lock-warning");
        assert_eq!(ClipboardClearedEvent::NAME, "clipboard://cleared");
        assert_eq!(
            serde_json::to_value(LockWarningEvent(LockWarning { seconds: 12 })).unwrap(),
            serde_json::json!({"seconds": 12})
        );
    }

    #[test]
    fn backup_event_names_and_payloads() {
        assert_eq!(BackupExportProgressEvent::NAME, "backup://export-progress");
        assert_eq!(
            BackupRestoreProgressEvent::NAME,
            "backup://restore-progress"
        );
        let payload = BackupProgress {
            phase: "encrypt".into(),
            done: 131_072,
            total: 262_144,
        };
        let json = serde_json::to_value(BackupExportProgressEvent(payload)).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"phase": "encrypt", "done": 131072, "total": 262144})
        );
    }
}

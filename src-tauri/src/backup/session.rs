//! Restore preview sessions: a verified, decrypted-once database waiting for
//! the user to commit, plus the on-disk temp-file lifecycle.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use uuid::Uuid;
use zeroize::Zeroizing;

use crate::crypto::header::VaultHeader;
use crate::error::AppError;
use crate::models::BackupPreview;
use crate::vault::paths::VaultPaths;

/// How long a preview stays valid.
pub const SESSION_TTL: Duration = Duration::from_secs(10 * 60);

/// Prefix for per-session temp DB files (`vault.db.restore-<uuid>.tmp`).
const TEMP_DB_PREFIX: &str = "vault.db.restore-";
const ARCHIVE_PREFIX: &str = "restore-";
const ARCHIVE_EXT: &str = ".coffer";
const SNAPSHOT_PREFIX: &str = "snapshot-";
const SNAPSHOT_EXT: &str = ".db";

/// A decrypted-and-verified restore candidate. The password is never kept; only
/// the derived DB key (zeroized on drop) and the embedded vault header are.
pub struct RestoreSession {
    pub id: String,
    temp_db_path: PathBuf,
    pub vault_header: VaultHeader,
    pub db_key: Zeroizing<[u8; 32]>,
    pub preview: BackupPreview,
    pub expires_at: Instant,
}

impl RestoreSession {
    pub fn new(
        temp_db_path: PathBuf,
        vault_header: VaultHeader,
        db_key: Zeroizing<[u8; 32]>,
        mut preview: BackupPreview,
    ) -> Self {
        let id = Uuid::now_v7().to_string();
        preview.session_id = id.clone();
        Self {
            id,
            temp_db_path,
            vault_header,
            db_key,
            preview,
            expires_at: Instant::now() + SESSION_TTL,
        }
    }

    pub fn temp_db_path(&self) -> &Path {
        &self.temp_db_path
    }

    pub fn is_expired(&self) -> bool {
        Instant::now() >= self.expires_at
    }
}

impl Drop for RestoreSession {
    fn drop(&mut self) {
        crate::backup::snapshot::remove_db_files(&self.temp_db_path);
    }
}

/// Session store: inserting drops expired previews, and a session's `Drop`
/// deletes its temp DB.
#[derive(Default)]
pub struct RestoreSessions {
    sessions: HashMap<String, RestoreSession>,
}

impl RestoreSessions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, session: RestoreSession) -> String {
        cleanup_expired_in(self);
        let id = session.id.clone();
        // Only one preview can be meaningful: the vault is uninitialized and
        // there is a single temp DB path.
        self.sessions.clear();
        self.sessions.insert(id.clone(), session);
        id
    }

    pub fn get(&self, id: &str) -> Option<&RestoreSession> {
        self.sessions.get(id).filter(|s| !s.is_expired())
    }

    /// Removes and returns a live session; expired ones are dropped (which
    /// deletes their temp DB) and reported as absent.
    pub fn remove(&mut self, id: &str) -> Option<RestoreSession> {
        self.sessions.remove(id).filter(|s| !s.is_expired())
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.sessions.len()
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty()
    }
}

fn cleanup_expired_in(sessions: &mut RestoreSessions) {
    sessions.sessions.retain(|_, s| !s.is_expired());
}

/// Removes temp files left by a previous process:
/// `restore-*.coffer`, `vault.db.restore-*.tmp` and `snapshot-*.db` (with
/// sidecars). A `vault.db` without a header is left for the restore flow to
/// rename.
pub fn cleanup_on_startup(root: &Path) -> Result<usize, AppError> {
    let entries = match std::fs::read_dir(root) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(e) => return Err(AppError::io(e)),
    };
    let mut removed = 0;
    for entry in entries {
        let entry = entry.map_err(AppError::io)?;
        let path = entry.path();
        if !entry.file_type().map_err(AppError::io)?.is_file() {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let ours = (name.starts_with(TEMP_DB_PREFIX)
            && (name.ends_with(".tmp")
                || name.ends_with(".tmp-wal")
                || name.ends_with(".tmp-shm")))
            || (name.starts_with(ARCHIVE_PREFIX) && name.ends_with(ARCHIVE_EXT))
            || (name.starts_with(SNAPSHOT_PREFIX)
                && (name.ends_with(SNAPSHOT_EXT)
                    || name.ends_with(".db-wal")
                    || name.ends_with(".db-shm")
                    || name.ends_with(".db-journal")));
        if ours && std::fs::remove_file(&path).is_ok() {
            removed += 1;
        }
    }
    Ok(removed)
}

/// Copies a user-chosen archive into the vault root under a private name so a
/// later path swap cannot change what gets verified.
pub fn stage_archive(root: &Path, source: &Path) -> Result<PathBuf, AppError> {
    std::fs::create_dir_all(root).map_err(AppError::io)?;
    let dest = root.join(format!("{ARCHIVE_PREFIX}{}{ARCHIVE_EXT}", Uuid::now_v7()));
    let copy = || -> Result<(), std::io::Error> {
        std::fs::copy(source, &dest)?;
        crate::vault::db::restrict_permissions(&dest);
        Ok(())
    };
    copy().map_err(|e| {
        let _ = std::fs::remove_file(&dest);
        AppError::io(e)
    })?;
    Ok(dest)
}

/// Unique temp DB path under the vault root. Per-session names let a new
/// preview replace an old one without deleting the new candidate on drop.
pub fn temp_db_path(paths: &VaultPaths) -> PathBuf {
    paths
        .root()
        .join(format!("{TEMP_DB_PREFIX}{}.tmp", Uuid::now_v7()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn preview() -> BackupPreview {
        BackupPreview {
            session_id: String::new(),
            source_vault_id: "0199aaaa-bbbb-7ccc-8ddd-eeeeffff0000".into(),
            created_at_ms: 1,
            app_version: "0.1.0".into(),
            schema_version: 1,
            item_count: 1,
            project_count: 1,
            tag_count: 1,
        }
    }

    pub(crate) fn session(dir: &Path) -> RestoreSession {
        let paths = VaultPaths::from_app_data(dir);
        std::fs::create_dir_all(paths.root()).unwrap();
        let path = temp_db_path(&paths);
        std::fs::write(&path, b"x").unwrap();
        RestoreSession::new(
            path,
            crate::backup::tests::sample_header(),
            Zeroizing::new([1u8; 32]),
            preview(),
        )
    }

    #[test]
    fn insert_get_remove_and_drop_cleans_temp() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = RestoreSessions::new();
        let s = session(dir.path());
        let temp = s.temp_db_path().to_path_buf();
        assert!(temp.exists());
        let id = store.insert(s);
        assert!(store.get(&id).is_some());
        assert_eq!(store.get(&id).unwrap().preview.session_id, id);
        let taken = store.remove(&id).unwrap();
        assert!(store.get(&id).is_none());
        assert!(temp.exists());
        drop(taken);
        assert!(!temp.exists());
    }

    #[test]
    fn new_preview_replaces_previous_session() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = RestoreSessions::new();
        let first = store.insert(session(dir.path()));
        let second = store.insert(session(dir.path()));
        assert_eq!(store.len(), 1);
        assert!(store.get(&first).is_none());
        assert!(store.get(&second).is_some());
    }

    #[test]
    fn expired_sessions_are_not_returned() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = RestoreSessions::new();
        let mut s = session(dir.path());
        s.expires_at = Instant::now();
        let id = store.insert(s);
        assert!(store.get(&id).is_none());
        // Expired looks the same as absent to remove().
        assert!(store.remove(&id).is_none());
    }

    #[test]
    fn startup_cleanup_removes_only_known_temp_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("coffer");
        std::fs::create_dir_all(&root).unwrap();
        let stale = [
            "restore-0199.coffer",
            "vault.db.restore-0199.tmp",
            "snapshot-0199.db",
            "snapshot-0199.db-wal",
        ];
        let kept = ["vault.db", "vault.meta.json", "settings.json", "notes.txt"];
        for name in stale.iter().chain(kept.iter()) {
            std::fs::write(root.join(name), b"x").unwrap();
        }
        let removed = cleanup_on_startup(&root).unwrap();
        assert_eq!(removed, 4);
        for name in stale {
            assert!(!root.join(name).exists(), "{name} should be removed");
        }
        for name in kept {
            assert!(root.join(name).exists(), "{name} should be kept");
        }
        // A missing root is not an error.
        assert_eq!(cleanup_on_startup(&dir.path().join("absent")).unwrap(), 0);
    }
}

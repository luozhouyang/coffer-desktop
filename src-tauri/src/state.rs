//! Application state: the unlocked vault (if any), settings and paths.
//!
//! Tauri commands hold short locks on the connection; long operations (Argon2,
//! SQLCipher open) run on blocking threads through `spawn_blocking` in the
//! commands module.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use crate::autolock::AutoLock;
use crate::clipboard::ClipboardManager;
use crate::error::AppError;
use crate::keychain::BiometricKeychain;
use crate::models::Settings;
use crate::vault::VaultDb;
use crate::vault::paths::VaultPaths;

/// An unlocked vault: the open encrypted database. Key material lives only in
/// the SQLCipher connection, which zeroizes it on close.
pub struct UnlockedVault {
    pub db: Arc<VaultDb>,
}

/// Shared application state.
pub struct AppState {
    pub paths: VaultPaths,
    vault: Mutex<Option<UnlockedVault>>,
    settings: Mutex<Settings>,
    autolock: Mutex<AutoLock>,
    /// Sequence bumped every time the vault is (un)locked. Lets callers that
    /// grabbed the DB after checking the lock notice a concurrent lock.
    epoch: AtomicU64,
    pub clipboard: Arc<ClipboardManager>,
    pub keychain: Arc<dyn BiometricKeychain>,
}

impl AppState {
    pub fn new(
        paths: VaultPaths,
        clipboard: Arc<ClipboardManager>,
        keychain: Arc<dyn BiometricKeychain>,
    ) -> Self {
        let settings = crate::settings::load(&paths.settings());
        let autolock = AutoLock::new(std::time::Duration::from_secs(
            settings.auto_lock_secs.into(),
        ));
        Self {
            paths,
            vault: Mutex::new(None),
            settings: Mutex::new(settings),
            autolock: Mutex::new(autolock),
            epoch: AtomicU64::new(0),
            clipboard,
            keychain,
        }
    }

    /// Returns the open database or `Locked`.
    pub fn require_db(&self) -> Result<Arc<VaultDb>, AppError> {
        let guard = self.vault.lock().map_err(|_| AppError::locked())?;
        guard
            .as_ref()
            .map(|v| v.db.clone())
            .ok_or_else(AppError::locked)
    }

    pub fn is_unlocked(&self) -> bool {
        self.vault.lock().map(|g| g.is_some()).unwrap_or(false)
    }

    pub fn epoch(&self) -> u64 {
        self.epoch.load(Ordering::SeqCst)
    }

    /// Installs a freshly opened vault and resets the idle timer.
    pub fn set_unlocked(&self, db: VaultDb) -> Result<(), AppError> {
        let previous = {
            let mut guard = self
                .vault
                .lock()
                .map_err(|_| AppError::internal("state busy"))?;
            guard.replace(UnlockedVault { db: Arc::new(db) })
        };
        self.epoch.fetch_add(1, Ordering::SeqCst);
        if let Some(prev) = previous {
            close_unless_borrowed(prev.db);
        }
        self.touch_activity();
        self.lock_autolock()?.touch();
        Ok(())
    }

    /// Closes the database (zeroizing its key material) and returns whether a
    /// vault was open. Never blocks on in-flight readers: if another thread
    /// still holds the `Arc`, its last drop closes the connection.
    pub fn lock(&self) -> bool {
        let previous = self.vault.lock().ok().and_then(|mut g| g.take());
        let Some(prev) = previous else {
            return false;
        };
        self.epoch.fetch_add(1, Ordering::SeqCst);
        close_unless_borrowed(prev.db);
        true
    }

    pub fn settings(&self) -> Settings {
        self.settings.lock().map(|g| g.clone()).unwrap_or_default()
    }

    /// Persists new settings and applies the new auto-lock timeout.
    pub fn apply_settings(&self, settings: Settings) -> Result<Settings, AppError> {
        settings.validate()?;
        let stored = crate::settings::store(&self.paths.settings(), &settings)?;
        {
            let mut guard = self
                .settings
                .lock()
                .map_err(|_| AppError::internal("state busy"))?;
            *guard = stored.clone();
        }
        self.lock_autolock()?
            .set_timeout(std::time::Duration::from_secs(stored.auto_lock_secs.into()));
        // Touch so a shorter timeout takes effect from now, not from the
        // previous activity.
        self.touch_activity();
        Ok(stored)
    }

    pub fn touch_activity(&self) {
        if let Ok(mut g) = self.autolock.lock() {
            g.touch();
        }
    }

    pub fn lock_autolock(&self) -> Result<MutexGuard<'_, AutoLock>, AppError> {
        self.autolock
            .lock()
            .map_err(|_| AppError::internal("state busy"))
    }
}

/// Closes the vault DB when possible without waiting for readers.
fn close_unless_borrowed(db: Arc<VaultDb>) {
    match Arc::try_unwrap(db) {
        Ok(db) => db.close(),
        Err(shared) => {
            // Another command is mid-query; it closes on final drop.
            drop(shared);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clipboard::tests::FakeClipboard;

    fn state() -> (tempfile::TempDir, AppState) {
        let dir = tempfile::tempdir().unwrap();
        let fake = Arc::new(FakeClipboard::default());
        let clipboard = Arc::new(ClipboardManager::new(fake));
        let keychain = Arc::new(crate::keychain::tests::FakeKeychain::new());
        let state = AppState::new(VaultPaths::from_app_data(dir.path()), clipboard, keychain);
        (dir, state)
    }

    fn open_db(state: &AppState, _dir: &tempfile::TempDir) -> Result<(), AppError> {
        state.paths.ensure_dir()?;
        let key = zeroize::Zeroizing::new([9u8; 32]);
        let db = VaultDb::create(&state.paths.db(), &key)?;
        state.set_unlocked(db)
    }

    #[test]
    fn locked_by_default() {
        let (dir, s) = state();
        assert!(!s.is_unlocked());
        assert!(matches!(s.require_db(), Err(e) if e.code == crate::error::ErrorCode::Locked));
        assert!(!dir.path().exists() || !s.paths.initialized());
    }

    #[test]
    fn unlock_then_lock() {
        let (dir, s) = state();
        open_db(&s, &dir).unwrap();
        assert!(s.is_unlocked());
        let db = s.require_db().unwrap();
        assert_eq!(crate::vault::ops::sidebar_counts(&db).unwrap().all, 0);
        drop(db);
        let before = s.epoch();
        assert!(s.lock());
        assert!(!s.is_unlocked());
        assert!(s.epoch() > before);
        assert!(s.require_db().is_err());
        // Locking twice is a no-op.
        assert!(!s.lock());
    }

    #[test]
    fn lock_while_db_borrowed_does_not_block() {
        let (dir, s) = state();
        open_db(&s, &dir).unwrap();
        let db = s.require_db().unwrap();
        assert!(s.lock());
        // The borrowed handle is still usable until dropped, then closes.
        db.with(|c| {
            let n: i64 = c.query_row("SELECT count(*) FROM items", [], |r| r.get(0))?;
            Ok(n)
        })
        .unwrap();
        drop(db);
    }

    #[test]
    fn settings_roundtrip_and_autolock_timeout() {
        let (_dir, s) = state();
        let next = Settings {
            auto_lock_secs: 120,
            clipboard_clear_secs: 10,
            deep_search: true,
        };
        s.apply_settings(next.clone()).unwrap();
        assert_eq!(s.settings(), next);
        assert_eq!(
            s.lock_autolock().unwrap().timeout(),
            std::time::Duration::from_secs(120)
        );
        assert!(s.apply_settings(Settings::default()).is_ok());
    }
}

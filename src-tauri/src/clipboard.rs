//! OS clipboard abstraction and delayed clearing.
//!
//! Copying happens entirely in Rust: the secret never crosses the IPC
//! boundary. After `clipboardClearSecs` the clipboard is cleared only if it
//! still holds exactly the value we wrote.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use zeroize::Zeroizing;

/// Minimal clipboard surface so tests can substitute a fake.
pub trait Clipboard: Send + Sync + 'static {
    fn set_text(&self, value: &str) -> Result<(), String>;
    fn get_text(&self) -> Result<Option<String>, String>;
    fn clear(&self) -> Result<(), String>;
}

/// Real system clipboard backed by `arboard`.
pub struct ArboardClipboard;

impl Clipboard for ArboardClipboard {
    fn set_text(&self, value: &str) -> Result<(), String> {
        let mut cb = arboard::Clipboard::new().map_err(|e| e.to_string())?;
        // TODO(clipboard-markers): on macOS write `org.nspasteboard.ConcealedType`
        // and on Windows set `ExcludeClipboardContentFromMonitorProcessing` /
        // `CanIncludeInClipboardHistory=0` so clipboard managers and cloud
        // history do not persist the secret. `arboard` has no API for extra
        // pasteboard types yet, so the short auto-clear is the only protection.
        cb.set_text(value.to_owned()).map_err(|e| e.to_string())
    }

    fn get_text(&self) -> Result<Option<String>, String> {
        let mut cb = arboard::Clipboard::new().map_err(|e| e.to_string())?;
        match cb.get_text() {
            Ok(t) => Ok(Some(t)),
            Err(arboard::Error::ContentNotAvailable) => Ok(None),
            Err(arboard::Error::ClipboardNotSupported) | Err(arboard::Error::ClipboardOccupied) => {
                Ok(None)
            }
            Err(e) => Err(e.to_string()),
        }
    }

    fn clear(&self) -> Result<(), String> {
        let mut cb = arboard::Clipboard::new().map_err(|e| e.to_string())?;
        cb.clear().map_err(|e| e.to_string())
    }
}

/// Tracks the pending auto-clear for the last copied secret.
pub struct ClipboardManager {
    backend: Arc<dyn Clipboard>,
    /// Fingerprint of the last value we wrote and its scheduled clear time.
    pending: Mutex<Option<PendingClear>>,
}

struct PendingClear {
    fingerprint: u64,
    due: Instant,
}

impl ClipboardManager {
    pub fn new(backend: Arc<dyn Clipboard>) -> Self {
        Self {
            backend,
            pending: Mutex::new(None),
        }
    }

    /// Copies a value and schedules it to be cleared after `clear_after`.
    pub fn copy(&self, value: &Zeroizing<String>, clear_after: Duration) -> Result<(), String> {
        self.backend.set_text(value)?;
        let fp = fingerprint(value);
        let mut guard = self.pending.lock().map_err(|_| "clipboard busy")?;
        *guard = Some(PendingClear {
            fingerprint: fp,
            due: Instant::now() + clear_after,
        });
        Ok(())
    }

    /// Clears the clipboard if the pending clear is due and the current
    /// clipboard content is still the value we copied. Returns `true` when it
    /// actually cleared.
    pub fn try_expire(&self) -> bool {
        let expired = {
            let Ok(mut guard) = self.pending.lock() else {
                return false;
            };
            match guard.as_ref() {
                Some(p) if Instant::now() >= p.due => {
                    let fp = p.fingerprint;
                    *guard = None;
                    Some(fp)
                }
                _ => None,
            }
        };
        let Some(expected) = expired else {
            return false;
        };
        let current = self.backend.get_text().ok().flatten();
        // Clipboard changed (user copied something else): leave it alone.
        if current.as_deref().map(fingerprint) == Some(expected) {
            self.backend.clear().is_ok()
        } else {
            false
        }
    }
}

/// FNV-1a fingerprint used only to compare clipboard contents. Not a
/// cryptographic hash; the value itself is never stored.
fn fingerprint(value: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in value.as_bytes() {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Default)]
    pub struct FakeClipboard {
        pub content: Mutex<Option<String>>,
        pub clears: AtomicUsize,
    }

    impl FakeClipboard {
        pub fn set(&self, v: &str) {
            *self.content.lock().unwrap() = Some(v.to_string());
        }
        pub fn get(&self) -> Option<String> {
            self.content.lock().unwrap().clone()
        }
    }

    impl Clipboard for FakeClipboard {
        fn set_text(&self, value: &str) -> Result<(), String> {
            self.set(value);
            Ok(())
        }
        fn get_text(&self) -> Result<Option<String>, String> {
            Ok(self.get())
        }
        fn clear(&self) -> Result<(), String> {
            self.clears.fetch_add(1, Ordering::SeqCst);
            *self.content.lock().unwrap() = None;
            Ok(())
        }
    }

    fn manager() -> (Arc<FakeClipboard>, ClipboardManager) {
        let fake = Arc::new(FakeClipboard::default());
        let mgr = ClipboardManager::new(fake.clone());
        (fake, mgr)
    }

    #[test]
    fn clears_only_when_content_still_matches() {
        let (fake, mgr) = manager();
        let secret = Zeroizing::new("s3cr3t".to_string());
        mgr.copy(&secret, Duration::ZERO).unwrap();
        assert_eq!(fake.get().as_deref(), Some("s3cr3t"));
        assert!(mgr.try_expire());
        assert_eq!(fake.get(), None);
        assert_eq!(fake.clears.load(Ordering::SeqCst), 1);
        // Nothing pending any more.
        assert!(!mgr.try_expire());
    }

    #[test]
    fn does_not_clear_replaced_content() {
        let (fake, mgr) = manager();
        mgr.copy(&Zeroizing::new("secret".to_string()), Duration::ZERO)
            .unwrap();
        fake.set("user copied something else");
        assert!(!mgr.try_expire());
        assert_eq!(fake.get().as_deref(), Some("user copied something else"));
        assert_eq!(fake.clears.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn waits_until_due() {
        let (fake, mgr) = manager();
        mgr.copy(&Zeroizing::new("x".to_string()), Duration::from_secs(60))
            .unwrap();
        assert!(!mgr.try_expire());
        assert_eq!(fake.get().as_deref(), Some("x"));
    }

    #[test]
    fn fingerprint_distinguishes_values() {
        assert_ne!(fingerprint("a"), fingerprint("b"));
        assert_eq!(fingerprint("abc"), fingerprint("abc"));
    }
}

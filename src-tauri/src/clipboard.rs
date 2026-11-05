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
        set_text_platform(value)
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

#[cfg(target_os = "macos")]
fn set_text_platform(value: &str) -> Result<(), String> {
    use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString};
    use objc2_foundation::NSString;

    // `arboard` cannot write extra pasteboard types, so write through AppKit
    // and mark the item as concealed. Clipboard managers that understand the
    // nspasteboard convention skip it instead of persisting the secret.
    let pasteboard = NSPasteboard::generalPasteboard();
    pasteboard.clearContents();
    let string = NSString::from_str(value);
    // SAFETY: AppKit owns the static pasteboard type string.
    if !pasteboard.setString_forType(&string, unsafe { NSPasteboardTypeString }) {
        return Err("clipboard is unavailable".to_string());
    }
    let concealed = NSString::from_str("org.nspasteboard.ConcealedType");
    let marker = NSString::from_str("");
    pasteboard.setString_forType(&marker, &concealed);
    Ok(())
}

#[cfg(windows)]
fn set_text_platform(value: &str) -> Result<(), String> {
    let result = arboard_set_text(value);
    if result.is_ok() {
        conceal_windows();
    }
    result
}

#[cfg(not(any(target_os = "macos", windows)))]
fn set_text_platform(value: &str) -> Result<(), String> {
    arboard_set_text(value)
}

#[cfg(not(target_os = "macos"))]
fn arboard_set_text(value: &str) -> Result<(), String> {
    let mut cb = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    cb.set_text(value.to_owned()).map_err(|e| e.to_string())
}

/// Adds the Win32 opt-out formats to the current clipboard contents. Failure
/// is not fatal: the text is already copied and the timer still clears it.
#[cfg(windows)]
fn conceal_windows() {
    use windows_sys::Win32::Foundation::GlobalFree;
    use windows_sys::Win32::System::DataExchange::{
        CloseClipboard, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
    };
    use windows_sys::Win32::System::Memory::{
        GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock,
    };

    const FORMATS: [&str; 2] = [
        "ExcludeClipboardContentFromMonitorProcessing",
        "CanIncludeInClipboardHistory",
    ];

    // SAFETY: standard Win32 clipboard sequence; every handle is released on
    // every path and the system takes ownership only after success.
    unsafe {
        if OpenClipboard(std::ptr::null_mut()) == 0 {
            return;
        }
        for name in FORMATS {
            let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
            let format = RegisterClipboardFormatW(wide.as_ptr());
            if format == 0 {
                continue;
            }
            let handle = GlobalAlloc(GMEM_MOVEABLE, std::mem::size_of::<u32>());
            if handle.is_null() {
                continue;
            }
            let data = GlobalLock(handle) as *mut u32;
            if data.is_null() {
                GlobalFree(handle);
                continue;
            }
            *data = 0;
            GlobalUnlock(handle);
            if SetClipboardData(format, handle).is_null() {
                GlobalFree(handle);
            }
        }
        let _ = CloseClipboard();
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

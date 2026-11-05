//! OS session lock detection.
//!
//! `README` promises that locking the OS session locks the vault. macOS and
//! Windows expose this state; Linux has no portable equivalent, so the idle
//! timer remains the only backstop there (documented in the README).
//!
//! The check runs from the existing one-second background tick. It only reads
//! state and never holds a vault handle.

/// Returns `true` while the current OS session is locked.
#[cfg(target_os = "macos")]
pub fn is_locked() -> bool {
    use objc2_core_foundation::{CFBoolean, CFDictionary, CFString, CFType};
    use objc2_core_graphics::CGSessionCopyCurrentDictionary;

    let Some(dictionary) = CGSessionCopyCurrentDictionary() else {
        return false;
    };
    // The returned dictionary is heterogeneous; re-type it as a CF dictionary
    // keyed by strings. The layout does not depend on the generic parameters.
    let dictionary: &CFDictionary<CFString, CFType> =
        unsafe { &*(&*dictionary as *const CFDictionary as *const CFDictionary<CFString, CFType>) };
    let key = CFString::from_str("CGSSessionScreenIsLocked");
    dictionary
        .get(&key)
        .and_then(|value| value.downcast_ref::<CFBoolean>().map(CFBoolean::as_bool))
        .unwrap_or(false)
}

/// Returns `true` while the current OS session is locked.
#[cfg(windows)]
pub fn is_locked() -> bool {
    use core::mem::size_of;
    use windows_sys::Win32::System::RemoteDesktop::{
        WTS_CURRENT_SERVER_HANDLE, WTS_CURRENT_SESSION, WTSFreeMemory, WTSINFOEXW,
        WTSQuerySessionInformationW, WTSSessionInfoEx,
    };

    let mut buffer: *mut u16 = std::ptr::null_mut();
    let mut bytes: u32 = 0;
    // SAFETY: the API fills `buffer`/`bytes` with a WTSINFOEXW owned by WTS;
    // it is freed exactly once below.
    let ok = unsafe {
        WTSQuerySessionInformationW(
            WTS_CURRENT_SERVER_HANDLE,
            WTS_CURRENT_SESSION,
            WTSSessionInfoEx,
            &mut buffer,
            &mut bytes,
        )
    };
    if ok == 0 || buffer.is_null() || bytes < size_of::<WTSINFOEXW>() as u32 {
        if !buffer.is_null() {
            unsafe { WTSFreeMemory(buffer.cast()) };
        }
        return false;
    }
    // SAFETY: the API returned at least `size_of::<WTSINFOEXW>()` bytes.
    let info = unsafe { &*(buffer as *const WTSINFOEXW) };
    let locked = if info.Level == 1 {
        // 0 = locked, 1 = unlocked, -1 = unknown.
        // SAFETY: `Level == 1` selects the level-1 union member.
        unsafe { info.Data.WTSInfoExLevel1.SessionFlags == 0 }
    } else {
        false
    };
    // SAFETY: `buffer` came from WTSQuerySessionInformationW.
    unsafe { WTSFreeMemory(buffer.cast()) };
    locked
}

/// Linux does not expose a portable session-lock signal. The idle timer still
/// locks the vault after the configured timeout.
#[cfg(not(any(target_os = "macos", windows)))]
pub fn is_locked() -> bool {
    false
}

#[cfg(test)]
mod tests {
    /// The call must be safe from the background thread and must not panic on
    /// the host platform.
    #[test]
    fn check_does_not_panic() {
        let _ = super::is_locked();
    }
}

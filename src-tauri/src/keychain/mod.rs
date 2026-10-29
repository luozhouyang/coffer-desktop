//! Optional biometric unlock.
//!
//! Only macOS implements a biometric keychain (Touch ID). Other platforms
//! report `supported: false`, so the unlock button and the settings entry are
//! hidden there. On macOS the master key is kept in the login keychain and
//! every read is gated by a LocalAuthentication policy, so the system asks for
//! Touch ID first; the password remains the recovery path.

use serde::{Deserialize, Serialize};
use specta::Type;
use zeroize::Zeroizing;

pub const SERVICE: &str = "com.luozhouyang.coffer.biometric";
pub const ACCOUNT: &str = "vault-master-key";
pub const KEY_LEN: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct KeychainStatus {
    pub supported: bool,
    pub available: bool,
    pub enabled: bool,
}

/// Storage for the master key guarded by biometrics.
pub trait BiometricKeychain: Send + Sync + 'static {
    fn status(&self) -> KeychainStatus;
    fn store(&self, key: &[u8]) -> Result<(), String>;
    fn load(&self) -> Result<Zeroizing<Vec<u8>>, String>;
    fn delete(&self) -> Result<(), String>;
}

#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(target_os = "macos"))]
mod unsupported;

#[cfg(target_os = "macos")]
pub use macos::PlatformKeychain;
#[cfg(not(target_os = "macos"))]
pub use unsupported::PlatformKeychain;

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    pub struct FakeKeychain {
        key: Mutex<Option<Vec<u8>>>,
        pub supported: bool,
        pub available: bool,
    }

    impl FakeKeychain {
        pub fn new() -> Self {
            Self {
                key: Mutex::new(None),
                supported: true,
                available: true,
            }
        }

        pub fn with_key(key: [u8; KEY_LEN]) -> Self {
            let fake = Self::new();
            fake.store(&key).unwrap();
            fake
        }
    }

    impl BiometricKeychain for FakeKeychain {
        fn status(&self) -> KeychainStatus {
            KeychainStatus {
                supported: self.supported,
                available: self.available,
                enabled: self.key.lock().unwrap().is_some(),
            }
        }

        fn store(&self, key: &[u8]) -> Result<(), String> {
            *self.key.lock().unwrap() = Some(key.to_vec());
            Ok(())
        }

        fn load(&self) -> Result<Zeroizing<Vec<u8>>, String> {
            self.key
                .lock()
                .unwrap()
                .clone()
                .map(Zeroizing::new)
                .ok_or_else(|| "item not found".to_string())
        }

        fn delete(&self) -> Result<(), String> {
            *self.key.lock().unwrap() = None;
            Ok(())
        }
    }
}

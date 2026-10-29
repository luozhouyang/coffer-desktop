use zeroize::Zeroizing;

use super::{BiometricKeychain, KeychainStatus};

pub struct PlatformKeychain;

impl BiometricKeychain for PlatformKeychain {
    fn status(&self) -> KeychainStatus {
        KeychainStatus {
            supported: false,
            available: false,
            enabled: false,
        }
    }

    fn store(&self, _key: &[u8]) -> Result<(), String> {
        Err("Touch ID is only available on macOS".into())
    }

    fn load(&self) -> Result<Zeroizing<Vec<u8>>, String> {
        Err("Touch ID is only available on macOS".into())
    }

    fn delete(&self) -> Result<(), String> {
        Ok(())
    }
}

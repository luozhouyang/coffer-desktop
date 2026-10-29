//! macOS biometric keychain.
//!
//! The master key lives in the login keychain as a generic-password item, so
//! its ACL is tied to Coffer's code signature: another app reading it needs
//! explicit user approval. Before every read Coffer runs a LocalAuthentication
//! policy, which is what puts Touch ID in front of the key.
//!
//! The data-protection keychain with a `SecAccessControl` biometry constraint
//! would be stronger, but on macOS it requires a provisioning profile carrying
//! `keychain-access-groups`; a Developer ID build without one is killed by
//! AMFI. The password always remains the recovery path.

use std::sync::mpsc;
use std::time::Duration;

use block2::RcBlock;
use objc2::runtime::Bool;
use objc2_foundation::{NSError, NSString};
use objc2_local_authentication::{LAContext, LAPolicy};
use security_framework::item::{ItemClass, ItemSearchOptions};
use security_framework::passwords::{
    PasswordOptions, delete_generic_password, generic_password, set_generic_password,
};
use zeroize::Zeroizing;

use super::{ACCOUNT, BiometricKeychain, KEY_LEN, KeychainStatus, SERVICE};

/// `errSecItemNotFound` from Security/SecBase.h.
const ERR_SEC_ITEM_NOT_FOUND: i32 = -25300;
/// Long enough for the user to find the sensor, short enough not to hang.
const AUTH_TIMEOUT: Duration = Duration::from_secs(120);

pub struct PlatformKeychain;

impl PlatformKeychain {
    fn options(&self) -> PasswordOptions {
        PasswordOptions::new_generic_password(SERVICE, ACCOUNT)
    }
}

impl BiometricKeychain for PlatformKeychain {
    fn status(&self) -> KeychainStatus {
        KeychainStatus {
            supported: true,
            available: biometry_available(),
            enabled: item_exists(),
        }
    }

    fn store(&self, key: &[u8]) -> Result<(), String> {
        if key.len() != KEY_LEN {
            return Err("invalid key length".into());
        }
        // Replace rather than update: the existing item may carry an ACL bound
        // to an older signature.
        let _ = self.delete();
        set_generic_password(SERVICE, ACCOUNT, key)
            .map_err(|e| format!("could not store key in keychain: {e}"))
    }

    fn load(&self) -> Result<Zeroizing<Vec<u8>>, String> {
        authenticate()?;
        let bytes = generic_password(self.options()).map_err(|e| {
            if e.code() == ERR_SEC_ITEM_NOT_FOUND {
                "no biometric key is stored".to_string()
            } else {
                format!("could not read key from keychain: {e}")
            }
        })?;
        if bytes.len() != KEY_LEN {
            return Err("stored key has an invalid length".into());
        }
        Ok(Zeroizing::new(bytes))
    }

    fn delete(&self) -> Result<(), String> {
        match delete_generic_password(SERVICE, ACCOUNT) {
            Ok(()) => Ok(()),
            Err(e) if e.code() == ERR_SEC_ITEM_NOT_FOUND => Ok(()),
            Err(e) => Err(format!("could not remove biometric key: {e}")),
        }
    }
}

/// Whether this Mac has biometric hardware with a current enrolment.
fn biometry_available() -> bool {
    // SAFETY: `LAContext` is created on this thread and only queried.
    let context = unsafe { LAContext::new() };
    // SAFETY: `canEvaluatePolicy:error:` only reads from the context.
    unsafe {
        context
            .canEvaluatePolicy_error(LAPolicy::DeviceOwnerAuthenticationWithBiometrics)
            .is_ok()
    }
}

/// Runs the system biometric prompt and waits for its verdict. Blocking, so
/// call it off the main thread.
fn authenticate() -> Result<(), String> {
    // SAFETY: `LAContext` is created on this thread and only queried.
    let context = unsafe { LAContext::new() };
    unsafe {
        context
            .canEvaluatePolicy_error(LAPolicy::DeviceOwnerAuthenticationWithBiometrics)
            .map_err(|_| "Touch ID is not available on this Mac".to_string())?;
    }

    let reason = NSString::from_str("unlock your Coffer vault");
    let (tx, rx) = mpsc::channel();
    let reply = RcBlock::new(move |success: Bool, error: *mut NSError| {
        let result = if success.as_bool() {
            Ok(())
        } else {
            // The error is only valid for the duration of the reply block;
            // keep just the short reason.
            let _ = error;
            Err("Touch ID was not confirmed".to_string())
        };
        let _ = tx.send(result);
    });
    // SAFETY: the reply block is alive until `recv_timeout` returns; the
    // context stays alive until then too, so the evaluation is not cancelled.
    unsafe {
        context.evaluatePolicy_localizedReason_reply(
            LAPolicy::DeviceOwnerAuthenticationWithBiometrics,
            &reason,
            &reply,
        );
    }
    rx.recv_timeout(AUTH_TIMEOUT)
        .map_err(|_| "Touch ID timed out".to_string())?
}

/// Existence check from attributes only; reading the data would prompt.
fn item_exists() -> bool {
    let mut options = ItemSearchOptions::new();
    options.class(ItemClass::generic_password());
    options.service(SERVICE);
    options.account(ACCOUNT);
    options.load_attributes(true);
    options.limit(1i64);
    matches!(options.search(), Ok(items) if !items.is_empty())
}

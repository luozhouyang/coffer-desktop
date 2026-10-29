//! Master key operations: create, unlock and change-password flows.
//!
//! The master key (MK) is always random and only ever exists wrapped at rest.
//! None of the key material types implement `Debug` and all key buffers are
//! zeroized on drop.

use std::path::Path;

use uuid::Uuid;
use zeroize::Zeroizing;

use super::header::{HEADER_VERSION, KDF_ARGON2ID, VaultHeader};
use super::{
    KEY_LEN, KdfParams, NONCE_LEN, SALT_LEN, derive_db_key, derive_kek, random_bytes, unwrap_mk,
    wrap_mk,
};
use crate::error::AppError;

/// Minimum master password length accepted by `vault_create` / change.
pub const MIN_PASSWORD_LEN: usize = 8;
/// Maximum accepted password length (bytes).
pub const MAX_PASSWORD_LEN: usize = 1024;

/// The unwrapped master key. Zeroized on drop.
pub struct MasterKey(Zeroizing<[u8; KEY_LEN]>);

impl MasterKey {
    fn random() -> Self {
        let mut mk = Zeroizing::new([0u8; KEY_LEN]);
        random_bytes(mk.as_mut());
        Self(mk)
    }

    fn as_ref(&self) -> &[u8; KEY_LEN] {
        &self.0
    }

    /// Raw master key bytes. Only used to hand into the blocking unlock path.
    pub fn expose(&self) -> &[u8; KEY_LEN] {
        self.as_ref()
    }
}

/// Result of creating a brand-new vault header.
pub struct NewVault {
    pub header: VaultHeader,
    pub mk: MasterKey,
}

/// Validates a password for create/change. Errors are generic and never echo
/// the password.
pub fn validate_password(password: &str) -> Result<(), AppError> {
    let chars = password.chars().count();
    if chars < MIN_PASSWORD_LEN {
        return Err(AppError::validation(format!(
            "password must be at least {MIN_PASSWORD_LEN} characters"
        )));
    }
    if password.len() > MAX_PASSWORD_LEN {
        return Err(AppError::validation("password is too long"));
    }
    Ok(())
}

fn validate_lookup_password(password: &str) -> Result<(), AppError> {
    if password.is_empty() || password.len() > MAX_PASSWORD_LEN {
        return Err(AppError::invalid_password());
    }
    Ok(())
}

fn build_header(
    vault_id: &str,
    salt: &[u8; SALT_LEN],
    nonce: &[u8; NONCE_LEN],
    wrapped: &[u8],
    params: KdfParams,
) -> VaultHeader {
    VaultHeader {
        version: HEADER_VERSION,
        vault_id: vault_id.to_string(),
        kdf: KDF_ARGON2ID.to_string(),
        m_cost: params.m_cost,
        t_cost: params.t_cost,
        p_cost: params.p_cost,
        salt_hex: hex::encode(salt),
        wrap_nonce_hex: hex::encode(nonce),
        wrapped_mk_hex: hex::encode(wrapped),
    }
}

/// Creates a new vault: random MK, random salt/nonce, Argon2id KEK, wrapped MK.
/// Blocking (Argon2); call from a blocking thread.
pub fn create(password: &str, params: KdfParams) -> Result<NewVault, AppError> {
    validate_password(password)?;
    let mk = MasterKey::random();
    let mut salt = [0u8; SALT_LEN];
    random_bytes(&mut salt);
    let header_stub = build_header(
        &Uuid::now_v7().to_string(),
        &salt,
        &[0; NONCE_LEN],
        &[],
        params,
    );
    let kek = derive_kek(password.as_bytes(), &salt, params)?;
    let mut nonce = [0u8; NONCE_LEN];
    random_bytes(&mut nonce);
    // Rebuild with the real vault id and AAD.
    let header_no_wrap = build_header(&header_stub.vault_id, &salt, &nonce, &[], params);
    let wrapped = wrap_mk(&kek, mk.as_ref(), &nonce, &header_no_wrap.aad())?;
    let header = build_header(&header_stub.vault_id, &salt, &nonce, &wrapped, params);
    Ok(NewVault { header, mk })
}

/// Unlocks an existing header with the master password. Blocking.
pub fn unlock_with_header(header: &VaultHeader, password: &str) -> Result<MasterKey, AppError> {
    validate_lookup_password(password)?;
    let params = KdfParams {
        m_cost: header.m_cost,
        t_cost: header.t_cost,
        p_cost: header.p_cost,
    };
    params.validate()?;
    let salt = header.salt()?;
    let nonce = header.wrap_nonce()?;
    let wrapped = header.wrapped_mk()?;
    let kek = derive_kek(password.as_bytes(), &salt, params)?;
    let mk = unwrap_mk(&kek, &wrapped, &nonce, &header.aad())?;
    Ok(MasterKey(mk))
}

/// Re-wraps the master key with a new password. Only the header changes, and
/// only after the new password has been fully derived and the new wrap built,
/// so a failure to unwrap the old key never touches the on-disk header.
/// Blocking (Argon2).
pub fn change_password(
    header_path: &Path,
    old_password: &str,
    new_password: &str,
    params: KdfParams,
) -> Result<(), AppError> {
    let current = VaultHeader::load(header_path)?;
    let mk = unlock_with_header(&current, old_password)?;
    validate_password(new_password)?;
    if old_password == new_password {
        return Err(AppError::validation(
            "new password must differ from the current password",
        ));
    }

    let mut salt = [0u8; SALT_LEN];
    random_bytes(&mut salt);
    let kek = derive_kek(new_password.as_bytes(), &salt, params)?;
    let mut nonce = [0u8; NONCE_LEN];
    random_bytes(&mut nonce);
    let no_wrap = build_header(&current.vault_id, &salt, &nonce, &[], params);
    let wrapped = wrap_mk(&kek, mk.as_ref(), &nonce, &no_wrap.aad())?;
    let next = build_header(&current.vault_id, &salt, &nonce, &wrapped, params);
    next.store(header_path)?;
    Ok(())
}

/// Derives the SQLCipher key. Blocking (cheap, but kept off the async pool).
pub fn db_key(mk: &MasterKey) -> Result<Zeroizing<[u8; KEY_LEN]>, AppError> {
    derive_db_key(mk.as_ref())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorCode;

    fn fast() -> KdfParams {
        KdfParams {
            m_cost: 8,
            t_cost: 1,
            p_cost: 1,
        }
    }

    fn create_at(dir: &Path, password: &str) -> VaultHeader {
        let nv = create(password, fast()).unwrap();
        let path = dir.join("vault.meta.json");
        nv.header.store(&path).unwrap();
        VaultHeader::load(&path).unwrap()
    }

    #[test]
    fn roundtrip_unlock() {
        let dir = tempfile::tempdir().unwrap();
        let header = create_at(dir.path(), "correct horse");
        let mk = unlock_with_header(&header, "correct horse").unwrap();
        let again = unlock_with_header(&header, "correct horse").unwrap();
        assert_eq!(mk.as_ref(), again.as_ref());
    }

    #[test]
    fn wrong_password_is_invalid_password() {
        let dir = tempfile::tempdir().unwrap();
        let header = create_at(dir.path(), "correct horse");
        assert!(matches!(
            unlock_with_header(&header, "wrong horse!"),
            Err(e) if e.code == ErrorCode::InvalidPassword
        ));
    }

    #[test]
    fn tampered_header_detected() {
        let dir = tempfile::tempdir().unwrap();
        let mut header = create_at(dir.path(), "correct horse");
        // Flip a nibble in the wrapped key: must fail authentication.
        let mut bytes = header.wrapped_mk_hex.clone().into_bytes();
        bytes[3] = if bytes[3] == b'a' { b'b' } else { b'a' };
        header.wrapped_mk_hex = String::from_utf8(bytes).unwrap();
        assert!(matches!(
            unlock_with_header(&header, "correct horse"),
            Err(e) if e.code == ErrorCode::InvalidPassword
        ));

        // Tamper the salt: AAD changes, decryption must fail.
        let mut header = create_at(dir.path(), "correct horse");
        header.salt_hex = hex::encode([0u8; 32]);
        assert!(matches!(
            unlock_with_header(&header, "correct horse"),
            Err(e) if e.code == ErrorCode::InvalidPassword
        ));

        // Tamper the vault id (bound by AAD).
        let mut header = create_at(dir.path(), "correct horse");
        header.vault_id = Uuid::now_v7().to_string();
        assert!(matches!(
            unlock_with_header(&header, "correct horse"),
            Err(e) if e.code == ErrorCode::InvalidPassword
        ));
    }

    #[test]
    fn change_password_rewraps_and_keeps_mk() {
        let dir = tempfile::tempdir().unwrap();
        let header_path = dir.path().join("vault.meta.json");
        let header = create_at(dir.path(), "old password 1");
        let mk_before = unlock_with_header(&header, "old password 1").unwrap();

        change_password(&header_path, "old password 1", "new password 2", fast()).unwrap();

        let header2 = VaultHeader::load(&header_path).unwrap();
        assert_eq!(header2.vault_id, header.vault_id);
        assert_ne!(header2.salt_hex, header.salt_hex);
        assert!(matches!(
            unlock_with_header(&header2, "old password 1"),
            Err(e) if e.code == ErrorCode::InvalidPassword
        ));
        let mk_after = unlock_with_header(&header2, "new password 2").unwrap();
        assert_eq!(mk_before.expose(), mk_after.expose());
    }

    #[test]
    fn change_password_rejects_bad_input() {
        let dir = tempfile::tempdir().unwrap();
        let header_path = dir.path().join("vault.meta.json");
        create_at(dir.path(), "old password 1");
        assert!(matches!(
            change_password(&header_path, "nope", "new password 2", fast()),
            Err(e) if e.code == ErrorCode::InvalidPassword
        ));
        assert!(matches!(
            change_password(&header_path, "old password 1", "short", fast()),
            Err(e) if e.code == ErrorCode::Validation
        ));
        assert!(matches!(
            change_password(&header_path, "old password 1", "old password 1", fast()),
            Err(e) if e.code == ErrorCode::Validation
        ));
        // Still unlockable with the original password.
        let header = VaultHeader::load(&header_path).unwrap();
        assert!(unlock_with_header(&header, "old password 1").is_ok());
    }

    #[test]
    fn password_validation() {
        assert!(validate_password("1234567").is_err());
        assert!(validate_password("12345678").is_ok());
        assert!(validate_password(&"a".repeat(MAX_PASSWORD_LEN + 1)).is_err());
    }

    #[test]
    fn db_key_derivation() {
        let mk = MasterKey::random();
        let a = db_key(&mk).unwrap();
        let b = db_key(&mk).unwrap();
        assert_eq!(*a, *b);
    }
}

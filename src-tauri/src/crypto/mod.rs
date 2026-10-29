//! Key hierarchy, header format and wrapping primitives.
//!
//! ```text
//! password ──Argon2id(salt, m/t/p)──▶ KEK ──XChaCha20-Poly1305(AAD=header)──▶ MK
//! MK ──HKDF-SHA256("coffer/db/v1")──▶ DB key ──▶ SQLCipher
//! ```

pub mod header;
pub mod keys;

use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use hkdf::Hkdf;
use sha2::Sha256;
use zeroize::Zeroizing;

use crate::error::AppError;

/// Argon2id memory cost in KiB (64 MiB).
pub const ARGON2_M_COST: u32 = 65_536;
/// Argon2id time cost.
pub const ARGON2_T_COST: u32 = 3;
/// Argon2id parallelism.
pub const ARGON2_P_COST: u32 = 1;
/// Salt length in bytes.
pub const SALT_LEN: usize = 32;
/// Master key / KEK / DB key length in bytes.
pub const KEY_LEN: usize = 32;
/// XChaCha20-Poly1305 nonce length in bytes.
pub const NONCE_LEN: usize = 24;
/// HKDF info string binding the DB key to a version.
pub const DB_KEY_INFO: &[u8] = b"coffer/db/v1";

/// Argon2id parameters recorded in the header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KdfParams {
    pub m_cost: u32,
    pub t_cost: u32,
    pub p_cost: u32,
}

impl Default for KdfParams {
    fn default() -> Self {
        Self {
            m_cost: ARGON2_M_COST,
            t_cost: ARGON2_T_COST,
            p_cost: ARGON2_P_COST,
        }
    }
}

impl KdfParams {
    /// Rejects parameters read from disk that are outside sane bounds, so a
    /// tampered header cannot make us allocate unbounded memory.
    pub fn validate(&self) -> Result<(), AppError> {
        let ok = (8..=1_048_576).contains(&self.m_cost)
            && (1..=16).contains(&self.t_cost)
            && (1..=8).contains(&self.p_cost);
        if ok {
            Ok(())
        } else {
            Err(AppError::internal("invalid KDF parameters"))
        }
    }
}

/// Derives the KEK from the master password. CPU/memory heavy; call from a
/// blocking thread.
pub fn derive_kek(
    password: &[u8],
    salt: &[u8; SALT_LEN],
    params: KdfParams,
) -> Result<Zeroizing<[u8; KEY_LEN]>, AppError> {
    let p = Params::new(params.m_cost, params.t_cost, params.p_cost, Some(KEY_LEN))
        .map_err(|_| AppError::internal("kdf parameter error"))?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, p);
    let mut out = Zeroizing::new([0u8; KEY_LEN]);
    argon
        .hash_password_into(password, salt, out.as_mut())
        .map_err(|_| AppError::internal("kdf failure"))?;
    Ok(out)
}

/// Wraps the master key with the KEK, binding `aad` into authentication.
pub fn wrap_mk(
    kek: &[u8; KEY_LEN],
    mk: &[u8; KEY_LEN],
    nonce: &[u8; NONCE_LEN],
    aad: &[u8],
) -> Result<Vec<u8>, AppError> {
    let cipher = XChaCha20Poly1305::new(kek.into());
    cipher
        .encrypt(
            &XNonce::try_from(nonce.as_slice()).map_err(|_| AppError::internal("bad nonce"))?,
            Payload { msg: mk, aad },
        )
        .map_err(|_| AppError::internal("wrap failure"))
}

/// Unwraps the master key. Returns `InvalidPassword` for any authentication
/// failure; the reason is never distinguished to the caller.
pub fn unwrap_mk(
    kek: &[u8; KEY_LEN],
    wrapped: &[u8],
    nonce: &[u8; NONCE_LEN],
    aad: &[u8],
) -> Result<Zeroizing<[u8; KEY_LEN]>, AppError> {
    let cipher = XChaCha20Poly1305::new(kek.into());
    let plain = Zeroizing::new(
        cipher
            .decrypt(
                &XNonce::try_from(nonce.as_slice()).map_err(|_| AppError::internal("bad nonce"))?,
                Payload { msg: wrapped, aad },
            )
            .map_err(|_| AppError::invalid_password())?,
    );
    if plain.len() != KEY_LEN {
        return Err(AppError::invalid_password());
    }
    let mut mk = Zeroizing::new([0u8; KEY_LEN]);
    mk.copy_from_slice(&plain);
    Ok(mk)
}

/// Derives the SQLCipher key from the master key via HKDF-SHA256.
pub fn derive_db_key(mk: &[u8; KEY_LEN]) -> Result<Zeroizing<[u8; KEY_LEN]>, AppError> {
    let hk = Hkdf::<Sha256>::new(None, mk);
    let mut out = Zeroizing::new([0u8; KEY_LEN]);
    hk.expand(DB_KEY_INFO, out.as_mut())
        .map_err(|_| AppError::internal("hkdf failure"))?;
    Ok(out)
}

/// Fills `buf` with OS CSPRNG output.
pub fn random_bytes(buf: &mut [u8]) {
    rand::fill(buf);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorCode;

    fn fast_params() -> KdfParams {
        KdfParams {
            m_cost: 8,
            t_cost: 1,
            p_cost: 1,
        }
    }

    #[test]
    fn derive_kek_is_deterministic() {
        let salt = [7u8; SALT_LEN];
        let a = derive_kek(b"hunter2", &salt, fast_params()).unwrap();
        let b = derive_kek(b"hunter2", &salt, fast_params()).unwrap();
        assert_eq!(*a, *b);
        let c = derive_kek(b"hunter3", &salt, fast_params()).unwrap();
        assert_ne!(*a, *c);
    }

    #[test]
    fn wrap_unwrap_roundtrip_and_tamper() {
        let kek = [1u8; KEY_LEN];
        let mk = Zeroizing::new([9u8; KEY_LEN]);
        let nonce = [5u8; NONCE_LEN];
        let aad = b"v1|uuid|argon2id|65536|3|1|salt";
        let wrapped = wrap_mk(&kek, &mk, &nonce, aad).unwrap();
        let back = unwrap_mk(&kek, &wrapped, &nonce, aad).unwrap();
        assert_eq!(*back, *mk);

        // Wrong AAD (tampered header) must fail authentication.
        assert!(matches!(
            unwrap_mk(&kek, &wrapped, &nonce, b"v1|other"),
            Err(e) if e.code == ErrorCode::InvalidPassword
        ));
        // Wrong KEK (wrong password).
        assert!(matches!(
            unwrap_mk(&[2u8; KEY_LEN], &wrapped, &nonce, aad),
            Err(e) if e.code == ErrorCode::InvalidPassword
        ));
        // Flipped ciphertext bit.
        let mut bad = wrapped.clone();
        bad[0] ^= 1;
        assert!(matches!(
            unwrap_mk(&kek, &bad, &nonce, aad),
            Err(e) if e.code == ErrorCode::InvalidPassword
        ));
    }

    #[test]
    fn db_key_is_deterministic_and_distinct() {
        let mk = [3u8; KEY_LEN];
        let a = derive_db_key(&mk).unwrap();
        let b = derive_db_key(&mk).unwrap();
        assert_eq!(*a, *b);
        assert_ne!(*a, mk);
    }

    #[test]
    fn random_bytes_differ() {
        let mut a = [0u8; 32];
        let mut b = [0u8; 32];
        random_bytes(&mut a);
        random_bytes(&mut b);
        assert_ne!(a, b);
        assert_ne!(a, [0u8; 32]);
    }
}

//! On-disk vault header (`vault.meta.json`), see IPC contract §5.

use std::fs;
use std::io::Write;
use std::path::Path;

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::Zeroizing;

use super::{KEY_LEN, NONCE_LEN, SALT_LEN};
use crate::error::AppError;

/// Current header format version.
pub const HEADER_VERSION: u32 = 1;
/// KDF identifier written to the header.
pub const KDF_ARGON2ID: &str = "argon2id";

/// Plaintext vault header. Contains no secret material: only the salt, the
/// nonce and the wrapped master key.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VaultHeader {
    pub version: u32,
    pub vault_id: String,
    pub kdf: String,
    pub m_cost: u32,
    pub t_cost: u32,
    pub p_cost: u32,
    pub salt_hex: String,
    pub wrap_nonce_hex: String,
    pub wrapped_mk_hex: String,
}

impl VaultHeader {
    /// Builds the additional authenticated data for the wrap:
    /// `version|vaultId|kdf|mCost|tCost|pCost|saltHex`.
    pub fn aad(&self) -> Vec<u8> {
        format!(
            "{}|{}|{}|{}|{}|{}|{}",
            self.version,
            self.vault_id,
            self.kdf,
            self.m_cost,
            self.t_cost,
            self.p_cost,
            self.salt_hex
        )
        .into_bytes()
    }

    /// Decodes the salt, validating length and hex.
    pub fn salt(&self) -> Result<Zeroizing<[u8; SALT_LEN]>, AppError> {
        decode_fixed::<SALT_LEN>(&self.salt_hex)
    }

    /// Decodes the wrap nonce, validating length and hex.
    pub fn wrap_nonce(&self) -> Result<[u8; NONCE_LEN], AppError> {
        Ok(*decode_fixed::<NONCE_LEN>(&self.wrap_nonce_hex)?)
    }

    /// Decodes the wrapped master key ciphertext (32 bytes + 16 byte tag).
    pub fn wrapped_mk(&self) -> Result<Zeroizing<Vec<u8>>, AppError> {
        let bytes = Zeroizing::new(hex::decode(&self.wrapped_mk_hex).map_err(header_corrupt)?);
        if bytes.len() != KEY_LEN + 16 {
            return Err(header_corrupt(()));
        }
        Ok(bytes)
    }

    /// Structural validation applied after reading from disk.
    pub fn validate(&self) -> Result<(), AppError> {
        if self.version != HEADER_VERSION {
            return Err(AppError::internal("unsupported vault version"));
        }
        if self.kdf != KDF_ARGON2ID {
            return Err(AppError::internal("unsupported KDF"));
        }
        Uuid::parse_str(&self.vault_id).map_err(header_corrupt)?;
        self.salt()?;
        self.wrap_nonce()?;
        self.wrapped_mk()?;
        Ok(())
    }

    /// Loads and validates a header.
    pub fn load(path: &Path) -> Result<Self, AppError> {
        let bytes = fs::read(path).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => AppError::vault_not_initialized(),
            _ => AppError::io(e),
        })?;
        let header: VaultHeader = serde_json::from_slice(&bytes).map_err(header_corrupt)?;
        header.validate()?;
        Ok(header)
    }

    /// Atomically writes the header (temp file + fsync + rename).
    pub fn store(&self, path: &Path) -> Result<(), AppError> {
        let json = serde_json::to_vec_pretty(self).map_err(|_| AppError::internal("serialize"))?;
        write_atomic(path, &json)
    }
}

fn header_corrupt<E>(_e: E) -> AppError {
    AppError::internal("vault header is corrupt")
}

fn decode_fixed<const N: usize>(s: &str) -> Result<Zeroizing<[u8; N]>, AppError> {
    let v = Zeroizing::new(hex::decode(s).map_err(header_corrupt)?);
    if v.len() != N {
        return Err(header_corrupt(()));
    }
    let mut out = Zeroizing::new([0u8; N]);
    out.copy_from_slice(&v);
    Ok(out)
}

/// Writes `bytes` to `path` atomically with owner-only permissions where the
/// platform supports it.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), AppError> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::internal("invalid path"))?;
    fs::create_dir_all(parent).map_err(AppError::io)?;
    let mut tmp = tempfile::NamedTempFile::new_in(parent).map_err(AppError::io)?;
    tmp.write_all(bytes).map_err(AppError::io)?;
    tmp.as_file().sync_all().map_err(AppError::io)?;
    tmp.persist(path).map_err(|e| AppError::io(e.error))?;
    Ok(())
}

/// Reads a JSON file, returning `None` when it does not exist.
pub fn read_json_opt<T: serde::de::DeserializeOwned>(path: &Path) -> Result<Option<T>, AppError> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|_| AppError::internal("corrupt json file")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(AppError::io(e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorCode;

    fn sample() -> VaultHeader {
        VaultHeader {
            version: 1,
            vault_id: Uuid::now_v7().to_string(),
            kdf: KDF_ARGON2ID.to_string(),
            m_cost: 65_536,
            t_cost: 3,
            p_cost: 1,
            salt_hex: hex::encode([1u8; 32]),
            wrap_nonce_hex: hex::encode([2u8; 24]),
            wrapped_mk_hex: hex::encode([3u8; 48]),
        }
    }

    #[test]
    fn json_shape_matches_contract() {
        let json = serde_json::to_value(sample()).unwrap();
        for key in [
            "version",
            "vaultId",
            "kdf",
            "mCost",
            "tCost",
            "pCost",
            "saltHex",
            "wrapNonceHex",
            "wrappedMkHex",
        ] {
            assert!(json.get(key).is_some(), "missing {key}");
        }
        assert_eq!(json.as_object().unwrap().len(), 9);
    }

    #[test]
    fn aad_is_stable() {
        let h = sample();
        let expected = format!("1|{}|argon2id|65536|3|1|{}", h.vault_id, h.salt_hex);
        assert_eq!(h.aad(), expected.into_bytes());
    }

    #[test]
    fn store_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("vault.meta.json");
        assert!(matches!(
            VaultHeader::load(&path),
            Err(e) if e.code == ErrorCode::VaultNotInitialized
        ));
        let h = sample();
        h.store(&path).unwrap();
        assert_eq!(VaultHeader::load(&path).unwrap(), h);
    }

    #[test]
    fn rejects_bad_headers() {
        let mut h = sample();
        h.version = 2;
        assert!(h.validate().is_err());
        let mut h = sample();
        h.salt_hex = "zz".into();
        assert!(h.validate().is_err());
        let mut h = sample();
        h.wrapped_mk_hex = hex::encode([3u8; 10]);
        assert!(h.validate().is_err());
        let mut h = sample();
        h.vault_id = "not-a-uuid".into();
        assert!(h.validate().is_err());
    }
}

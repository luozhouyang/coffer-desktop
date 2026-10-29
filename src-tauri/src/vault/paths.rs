//! File layout: `app_data_dir()/coffer/vault.meta.json` and `.../vault.db`.

use std::path::{Path, PathBuf};

use crate::error::AppError;

/// Directory name under the app data directory.
pub const VAULT_DIR: &str = "coffer";
pub const HEADER_FILE: &str = "vault.meta.json";
pub const DB_FILE: &str = "vault.db";
pub const SETTINGS_FILE: &str = "settings.json";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultPaths {
    root: PathBuf,
}

impl VaultPaths {
    /// Builds paths rooted at `<app_data_dir>/coffer`.
    pub fn from_app_data(app_data_dir: &Path) -> Self {
        Self {
            root: app_data_dir.join(VAULT_DIR),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn header(&self) -> PathBuf {
        self.root.join(HEADER_FILE)
    }

    pub fn db(&self) -> PathBuf {
        self.root.join(DB_FILE)
    }

    pub fn settings(&self) -> PathBuf {
        self.root.join(SETTINGS_FILE)
    }

    pub fn initialized(&self) -> bool {
        self.header().is_file()
    }

    /// Ensures the vault directory exists.
    pub fn ensure_dir(&self) -> Result<(), AppError> {
        std::fs::create_dir_all(&self.root).map_err(AppError::io)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_matches_contract() {
        let p = VaultPaths::from_app_data(Path::new("/tmp/appdata"));
        assert_eq!(p.header(), Path::new("/tmp/appdata/coffer/vault.meta.json"));
        assert_eq!(p.db(), Path::new("/tmp/appdata/coffer/vault.db"));
        assert!(!p.initialized());
    }
}

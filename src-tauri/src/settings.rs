//! Settings persistence. Stored as JSON in the vault directory; contains no
//! secret material.

use std::path::Path;

use crate::crypto::header::{read_json_opt, write_atomic};
use crate::error::AppError;
use crate::models::Settings;

/// Loads settings, falling back to defaults when absent or out of range.
pub fn load(path: &Path) -> Settings {
    match read_json_opt::<Settings>(path) {
        Ok(Some(s)) if s.validate().is_ok() => s,
        _ => Settings::default(),
    }
}

/// Validates and atomically writes settings.
pub fn store(path: &Path, settings: &Settings) -> Result<Settings, AppError> {
    settings.validate()?;
    let json = serde_json::to_vec_pretty(settings).map_err(|_| AppError::internal("serialize"))?;
    write_atomic(path, &json)?;
    Ok(settings.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        assert_eq!(load(&path), Settings::default());

        let s = Settings {
            auto_lock_secs: 120,
            clipboard_clear_secs: 10,
            deep_search: true,
        };
        store(&path, &s).unwrap();
        assert_eq!(load(&path), s);

        let bad = Settings {
            auto_lock_secs: 1,
            ..Settings::default()
        };
        assert!(store(&path, &bad).is_err());
        std::fs::write(&path, b"{not json").unwrap();
        assert_eq!(load(&path), Settings::default());
    }
}

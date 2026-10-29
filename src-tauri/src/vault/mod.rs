//! Vault storage: file layout, SQLCipher connection and migrations.

pub mod db;
pub mod migrations;
pub mod ops;
pub mod paths;

pub use db::{VaultDb, now_ms};
pub use paths::VaultPaths;

//! IPC error type. Serialized as `{ code, message }` (contract §2 `AppError`)
//! and returned from every command through `Result<T, AppError>`.

use std::fmt;

use serde::{Deserialize, Serialize};
use specta::Type;

/// Error codes from the IPC contract. `#[serde(rename_all = "camelCase")]`
/// produces `invalidPassword`, `vaultNotInitialized`, `alreadyInitialized`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ErrorCode {
    Locked,
    InvalidPassword,
    VaultNotInitialized,
    AlreadyInitialized,
    NotFound,
    Conflict,
    Validation,
    Io,
    KeychainUnavailable,
    Internal,
}

/// A command error. Messages are created by the backend and must never contain
/// secret material; they are short, user-facing reason strings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, thiserror::Error)]
#[serde(rename_all = "camelCase")]
#[error("{message}")]
pub struct AppError {
    pub code: ErrorCode,
    pub message: String,
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ErrorCode::Locked => "locked",
            ErrorCode::InvalidPassword => "invalidPassword",
            ErrorCode::VaultNotInitialized => "vaultNotInitialized",
            ErrorCode::AlreadyInitialized => "alreadyInitialized",
            ErrorCode::NotFound => "notFound",
            ErrorCode::Conflict => "conflict",
            ErrorCode::Validation => "validation",
            ErrorCode::Io => "io",
            ErrorCode::KeychainUnavailable => "keychainUnavailable",
            ErrorCode::Internal => "internal",
        })
    }
}

impl AppError {
    /// Builds an error with an explicit code.
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn invalid_password() -> Self {
        Self::new(ErrorCode::InvalidPassword, "incorrect password")
    }
    pub fn vault_not_initialized() -> Self {
        Self::new(ErrorCode::VaultNotInitialized, "vault is not initialized")
    }
    pub fn locked() -> Self {
        Self::new(ErrorCode::Locked, "vault is locked")
    }
    pub fn not_found(what: &str) -> Self {
        Self::new(ErrorCode::NotFound, format!("{what} not found"))
    }
    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Conflict, message)
    }
    pub fn validation(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Validation, message)
    }
    pub fn io(e: std::io::Error) -> Self {
        // Logged nowhere; the kind is enough context for troubleshooting
        // without leaking paths or content.
        let _ = e;
        Self::new(ErrorCode::Io, "storage I/O error")
    }
    pub fn keychain_unavailable(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::KeychainUnavailable, message)
    }
    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Internal, message)
    }
    pub fn already_initialized() -> Self {
        Self::new(ErrorCode::AlreadyInitialized, "vault already exists")
    }
}

impl Default for AppError {
    fn default() -> Self {
        Self::internal("unknown error")
    }
}

impl From<AppError> for rusqlite::Error {
    fn from(e: AppError) -> Self {
        rusqlite::Error::ToSqlConversionFailure(Box::new(e))
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        match e {
            rusqlite::Error::QueryReturnedNoRows => Self::not_found("record"),
            rusqlite::Error::SqliteFailure(err, _)
                if err.code == rusqlite::ErrorCode::ConstraintViolation =>
            {
                Self::conflict("constraint violation")
            }
            other => Self::internal(format!("database error: {other}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_as_code_message_object() {
        let err = AppError::locked();
        let json = serde_json::to_value(&err).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"code": "locked", "message": "vault is locked"})
        );
        let invalid = AppError::new(ErrorCode::InvalidPassword, "x");
        assert_eq!(
            serde_json::to_value(&invalid).unwrap()["code"],
            "invalidPassword"
        );
        assert_eq!(
            serde_json::to_value(AppError::new(ErrorCode::VaultNotInitialized, "x")).unwrap()["code"],
            "vaultNotInitialized"
        );
    }
}

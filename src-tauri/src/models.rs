//! Data types shared across the IPC boundary, mirroring IPC_CONTRACT.md §2.

use serde::{Deserialize, Serialize};
use specta::Type;
#[allow(unused_imports)]
use specta_typescript::Number;

pub mod events {
    //! Event payloads (contract §4). Event *names* are defined on the event
    //! structs in [`crate::events`].

    use serde::{Deserialize, Serialize};
    use specta::Type;
    #[allow(unused_imports)]
    use specta_typescript::Number;

    /// `vault://lock-warning` payload.
    #[derive(Debug, Clone, Serialize, Deserialize, Type)]
    pub struct LockWarning {
        pub seconds: u32,
    }

    /// Payload of `backup://export-progress` and `backup://restore-progress`.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
    pub struct BackupProgress {
        pub phase: String,
        #[specta(type = Number)]
        pub done: i64,
        #[specta(type = Number)]
        pub total: i64,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type, Hash)]
#[serde(rename_all = "camelCase")]
pub enum ItemKind {
    ApiKey,
    Env,
    Password,
    Note,
}

impl ItemKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ItemKind::ApiKey => "apiKey",
            ItemKind::Env => "env",
            ItemKind::Password => "password",
            ItemKind::Note => "note",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "apiKey" | "api_key" => Some(ItemKind::ApiKey),
            "env" => Some(ItemKind::Env),
            "password" => Some(ItemKind::Password),
            "note" => Some(ItemKind::Note),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum CategorySource {
    Manual,
    Rule,
    Import,
    Default,
}

impl CategorySource {
    pub fn as_str(self) -> &'static str {
        match self {
            CategorySource::Manual => "manual",
            CategorySource::Rule => "rule",
            CategorySource::Import => "import",
            CategorySource::Default => "default",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "manual" => CategorySource::Manual,
            "rule" => CategorySource::Rule,
            "import" => CategorySource::Import,
            _ => CategorySource::Default,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub color: Option<String>,
    #[specta(type = Option<Number>)]
    pub archived_at: Option<i64>,
    #[specta(type = Number)]
    pub item_count: i64,
    #[specta(type = Number)]
    pub created_at: i64,
    #[specta(type = Number)]
    pub updated_at: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProjectInput {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProjectUpdate {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<Option<String>>,
    #[serde(default)]
    pub color: Option<Option<String>>,
    #[serde(default)]
    pub archived: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ItemMeta {
    pub id: String,
    pub kind: ItemKind,
    pub title: String,
    pub username: Option<String>,
    pub url: Option<String>,
    pub project_id: Option<String>,
    pub category: Option<String>,
    pub category_source: CategorySource,
    pub favorite: bool,
    pub tags: Vec<String>,
    #[specta(type = Number)]
    pub created_at: i64,
    #[specta(type = Number)]
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ItemDetail {
    #[serde(flatten)]
    pub meta: ItemMeta,
    pub notes: Option<String>,
    pub has_secret: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ItemInput {
    pub kind: ItemKind,
    pub title: String,
    pub secret: String,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ItemUpdate {
    pub id: String,
    #[serde(default)]
    pub kind: Option<ItemKind>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub secret: Option<String>,
    #[serde(default)]
    pub username: Option<Option<String>>,
    #[serde(default)]
    pub url: Option<Option<String>>,
    #[serde(default)]
    pub project_id: Option<Option<String>>,
    #[serde(default)]
    pub notes: Option<Option<String>>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ItemFilter {
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub unassigned_only: bool,
    #[serde(default)]
    pub kind: Option<ItemKind>,
    #[serde(default)]
    pub favorite_only: Option<bool>,
}

/// Result of `backup_export`.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BackupExportResult {
    pub path: String,
    #[specta(type = Number)]
    pub bytes: i64,
    #[specta(type = Number)]
    pub item_count: i64,
    #[specta(type = Number)]
    pub project_count: i64,
    #[specta(type = Number)]
    pub created_at_ms: i64,
}

/// Result of `backup_restore_begin`; also names the session to commit.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BackupPreview {
    pub session_id: String,
    pub source_vault_id: String,
    #[specta(type = Number)]
    pub created_at_ms: i64,
    pub app_version: String,
    #[specta(type = Number)]
    pub schema_version: i64,
    #[specta(type = Number)]
    pub item_count: i64,
    #[specta(type = Number)]
    pub project_count: i64,
    #[specta(type = Number)]
    pub tag_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProjectCount {
    pub project_id: String,
    #[specta(type = Number)]
    pub count: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SidebarCounts {
    #[specta(type = Number)]
    pub all: i64,
    #[specta(type = Number)]
    pub unassigned: i64,
    pub projects: Vec<ProjectCount>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct VaultStatus {
    pub initialized: bool,
    pub unlocked: bool,
    pub auto_lock_secs: u32,
    pub counts: SidebarCounts,
}

impl VaultStatus {
    pub fn locked(initialized: bool, auto_lock_secs: u32) -> Self {
        Self {
            initialized,
            unlocked: false,
            auto_lock_secs,
            counts: SidebarCounts::default(),
        }
    }
}

pub const DEFAULT_AUTO_LOCK_SECS: u32 = 300;
pub const DEFAULT_CLIPBOARD_CLEAR_SECS: u32 = 30;
pub const AUTO_LOCK_MIN_SECS: u32 = 60;
pub const AUTO_LOCK_MAX_SECS: u32 = 3600;
pub const CLIPBOARD_MIN_SECS: u32 = 5;
pub const CLIPBOARD_MAX_SECS: u32 = 300;
pub const REVEAL_HIDE_AFTER_SECS: u32 = 15;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub auto_lock_secs: u32,
    pub clipboard_clear_secs: u32,
    pub deep_search: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            auto_lock_secs: DEFAULT_AUTO_LOCK_SECS,
            clipboard_clear_secs: DEFAULT_CLIPBOARD_CLEAR_SECS,
            deep_search: false,
        }
    }
}

impl Settings {
    /// Contract §3: `autoLockSecs` 60..=3600, `clipboardClearSecs` 5..=300.
    pub fn validate(&self) -> Result<(), crate::error::AppError> {
        if !(AUTO_LOCK_MIN_SECS..=AUTO_LOCK_MAX_SECS).contains(&self.auto_lock_secs) {
            return Err(crate::error::AppError::validation(format!(
                "autoLockSecs must be between {AUTO_LOCK_MIN_SECS} and {AUTO_LOCK_MAX_SECS}"
            )));
        }
        if !(CLIPBOARD_MIN_SECS..=CLIPBOARD_MAX_SECS).contains(&self.clipboard_clear_secs) {
            return Err(crate::error::AppError::validation(format!(
                "clipboardClearSecs must be between {CLIPBOARD_MIN_SECS} and {CLIPBOARD_MAX_SECS}"
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Classification {
    pub category: String,
    pub tags: Vec<String>,
    pub confidence: f64,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub item: ItemMeta,
    pub score: f64,
    pub matched_field: String,
    pub snippet: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RevealedSecret {
    pub value: String,
    pub hide_after_secs: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CopyResult {
    pub clear_after_secs: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AuditEntry {
    #[specta(type = Number)]
    pub id: i64,
    pub action: String,
    pub item_id: Option<String>,
    #[specta(type = Number)]
    pub created_at: i64,
}

/// Length limits from contract §6. Counted in bytes for UTF-8 payloads.
pub mod limits {
    pub const TITLE: usize = 200;
    pub const SECRET: usize = 64 * 1024;
    pub const NOTES: usize = 32 * 1024;
    pub const NAME: usize = 100;
    pub const DESCRIPTION: usize = 1000;
    pub const URL: usize = 2048;
    pub const USERNAME: usize = 256;
    pub const TAG: usize = 48;
    pub const TAGS_PER_ITEM: usize = 32;
    pub const QUERY: usize = 256;
    pub const COLLECTED_VISIBLE_SECRETS: usize = 64 * 1024;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enums_serialize_as_contract_strings() {
        assert_eq!(
            serde_json::to_string(&ItemKind::ApiKey).unwrap(),
            "\"apiKey\""
        );
        assert_eq!(
            serde_json::to_string(&CategorySource::Default).unwrap(),
            "\"default\""
        );
        assert_eq!(ItemKind::parse("env"), Some(ItemKind::Env));
        assert_eq!(CategorySource::parse("manual"), CategorySource::Manual);
    }

    #[test]
    fn item_filter_default_is_with_permissive_camel_case() {
        let f: ItemFilter = serde_json::from_value(serde_json::json!({})).unwrap();
        assert!(f.project_id.is_none());
        let f: ItemFilter =
            serde_json::from_value(serde_json::json!({"projectId": null, "unassignedOnly": true}))
                .unwrap();
        assert!(f.unassigned_only);
    }

    #[test]
    fn item_detail_flattens_meta() {
        let d = ItemDetail {
            meta: ItemMeta {
                id: "1".into(),
                kind: ItemKind::Note,
                title: "t".into(),
                username: None,
                url: None,
                project_id: None,
                category: None,
                category_source: CategorySource::Default,
                favorite: false,
                tags: vec![],
                created_at: 0,
                updated_at: 0,
            },
            notes: None,
            has_secret: true,
        };
        let v = serde_json::to_value(d).unwrap();
        assert!(v.get("id").is_some());
        assert!(v.get("meta").is_none());
        assert_eq!(v["hasSecret"], true);
    }

    #[test]
    fn settings_validation_bounds() {
        assert!(Settings::default().validate().is_ok());
        let ok = Settings {
            auto_lock_secs: 60,
            ..Settings::default()
        };
        assert!(ok.validate().is_ok());
        assert!(
            Settings {
                auto_lock_secs: 59,
                ..Settings::default()
            }
            .validate()
            .is_err()
        );
        assert!(
            Settings {
                auto_lock_secs: 3601,
                ..Settings::default()
            }
            .validate()
            .is_err()
        );
        assert!(
            Settings {
                clipboard_clear_secs: 4,
                ..Settings::default()
            }
            .validate()
            .is_err()
        );
        assert!(
            Settings {
                clipboard_clear_secs: 301,
                ..Settings::default()
            }
            .validate()
            .is_err()
        );
    }
}

//! Local, explainable classification rules (DESIGN.md §10).
//!
//! Layers, highest confidence first:
//! 1. secret prefix/signature,
//! 2. `.env` key mapping,
//! 3. URL / registered domain,
//! 4. keyword fallback in title/notes/url,
//! 5. default `Uncategorized`.
//!
//! Secrets are only inspected in memory; nothing here logs values.

pub mod env_map;
pub mod keywords;
pub mod signatures;
pub mod url;

use crate::models::Classification;

pub const AI: &str = "AI / LLM";
pub const PAYMENTS: &str = "Payments";
pub const CLOUD: &str = "Cloud";
pub const VCS: &str = "VCS";
pub const ENV: &str = ".env";
pub const MESSAGING: &str = "Messaging";
pub const DATABASE: &str = "Database";
pub const NOTES: &str = "Notes";
pub const UNCATEGORIZED: &str = "Uncategorized";
pub const OTHER: &str = "Other";

/// Fixed display categories per IPC contract §1.
pub const CATEGORIES: &[&str] = &[
    AI,
    PAYMENTS,
    CLOUD,
    VCS,
    ENV,
    MESSAGING,
    DATABASE,
    NOTES,
    UNCATEGORIZED,
];

/// `true` when `category` is one of the fixed display categories.
pub fn is_fixed_category(category: &str) -> bool {
    CATEGORIES.contains(&category)
}

/// Input for classification. `value` may hold multiple lines (`KEY=VALUE`).
#[derive(Debug, Clone, Copy, Default)]
pub struct ClassifyInput<'a> {
    pub title: &'a str,
    pub value: &'a str,
    pub url: Option<&'a str>,
}

fn result(category: &str, tags: Vec<&str>, confidence: f64, reason: String) -> Classification {
    Classification {
        category: category.to_string(),
        tags: tags.into_iter().map(str::to_string).collect(),
        confidence,
        reason: Some(reason),
    }
}

/// Classifies an item. Pure function.
pub fn classify(input: ClassifyInput<'_>) -> Classification {
    let title = input.title.trim();
    let value = input.value;
    let url = input.url.map(str::trim).filter(|u| !u.is_empty());

    // 1. Prefix / signature rules over the first non-empty line.
    let first_line = value.lines().map(str::trim).find(|l| !l.is_empty());
    if let Some(line) = first_line
        && let Some(hit) = signatures::match_signature(line)
    {
        let mut tags = signature_tags(hit.provider);
        tags.push(hit.tag);
        return result(hit.category, tags, 0.98, hit.reason);
    }

    // 2. `.env` key mapping.
    let env_hit = env_map::match_env(value).or_else(|| {
        let upper = title.to_ascii_uppercase();
        let looks_like_key = upper
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
            && upper.len() >= 3;
        if looks_like_key {
            env_map::classify_key(&upper)
        } else {
            None
        }
    });
    if let Some(hit) = env_hit {
        let mut tags = vec![hit.provider];
        let looks_like_env = value.lines().any(|l| l.contains('='));
        if looks_like_env {
            tags.push(ENV);
        }
        let confidence: f64 = if hit.direct { 0.9 } else { 0.8 };
        return result(
            hit.category,
            tags,
            confidence,
            format!("env key {}", hit.key),
        );
    }

    // 3. URL / domain.
    if let Some(u) = url
        && let Some(hit) = url::match_url(u)
    {
        let tags = hit.tag.as_deref().map(|t| vec![t]).unwrap_or_default();
        return result(hit.category, tags, 0.85, format!("domain {}", hit.domain));
    }

    // 4. Keyword fallback.
    let hay = format!("{} {} {}", title, value, url.unwrap_or_default()).to_ascii_lowercase();
    if let Some(hit) = keywords::match_keywords(&hay) {
        return result(
            hit.category,
            hit.tags.to_vec(),
            0.6,
            format!("keyword {}", hit.keyword),
        );
    }

    // Notes are only a heuristic when nothing else matched.
    if title.is_empty() && !value.contains('=') && value.len() > 200 {
        return result(NOTES, vec![], 0.4, "long free-form text".to_string());
    }

    Classification {
        category: UNCATEGORIZED.to_string(),
        tags: vec![],
        confidence: 0.0,
        reason: None,
    }
}

/// Tags emitted alongside a signature match.
fn signature_tags(provider: &'static str) -> Vec<&'static str> {
    // Provider tags stay close to the display categories so the UI can group
    // by tag without extra mapping tables.
    match provider {
        "anthropic" => vec!["anthropic"],
        "openai" => vec!["openai"],
        "github" => vec!["github"],
        "gitlab" => vec!["gitlab"],
        "aws" => vec!["aws"],
        "google" => vec!["google"],
        "slack" => vec!["slack"],
        "stripe" => vec!["stripe"],
        "npm" => vec!["npm"],
        "jwt" => vec!["jwt"],
        _ => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(title: &str, value: &str, url: Option<&str>) -> Classification {
        classify(ClassifyInput { title, value, url })
    }

    #[test]
    fn signatures_take_priority() {
        let r = c("my key", "sk-ant-api03-xY9", None);
        assert_eq!(r.category, AI);
        assert!(r.confidence > 0.9);
        assert!(r.reason.unwrap().contains("sk-ant-"));
        assert_eq!(c("", "sk-proj-Ab12", None).category, AI);
        assert_eq!(c("", "sk-0123456789abcdef", None).category, AI);
        assert_eq!(c("", "ghp_AbCdEf01", None).category, VCS);
        assert_eq!(c("", "github_pat_AbCdEf01", None).category, VCS);
        assert_eq!(c("", "glpat-Ab12", None).category, VCS);
        assert_eq!(c("", "AKIAQ3EGRV2LZ7PM6X4B", None).category, CLOUD);
        assert_eq!(c("", "ASIAQ3EGRV2LZ7PM6X4B", None).category, CLOUD);
        assert_eq!(c("", "AIzaSyAbcdef123", None).category, CLOUD);
        assert_eq!(c("", "xoxb-12-34-AbCd", None).category, MESSAGING);
        assert_eq!(c("", "xoxp-12-34-AbCd", None).category, MESSAGING);
        assert_eq!(c("", "sk_live_Ab12Cd34", None).category, PAYMENTS);
        assert_eq!(c("", "sk_test_Ab12Cd34", None).category, PAYMENTS);
    }

    #[test]
    fn env_keys() {
        let r = c(
            ".env",
            "OPENAI_API_KEY=abc\nDATABASE_URL=postgres://x",
            None,
        );
        assert_eq!(r.category, AI);
        assert!(r.tags.iter().any(|t| t == ".env"));
        assert_eq!(c("", "AWS_SECRET_ACCESS_KEY=abc", None).category, CLOUD);
        assert_eq!(c("", "STRIPE_SECRET_KEY=abc", None).category, PAYMENTS);
        assert_eq!(c("", "OPENAI_API_KEY", None).category, AI);
        assert_eq!(c("", "SLACK_BOT_TOKEN", None).category, MESSAGING);
        assert_eq!(c("", "SOME_RANDOM_VAR=1", None).category, UNCATEGORIZED);
    }

    #[test]
    fn database_urls() {
        assert_eq!(c("", "postgres://u:p@host/db", None).category, DATABASE);
        assert_eq!(c("", "mysql://u:p@h/d", None).category, DATABASE);
        assert_eq!(c("", "redis://:pw@h:6379", None).category, DATABASE);
        assert_eq!(c("", "mongodb+srv://u:p@c", None).category, DATABASE);
        assert_eq!(c("where", "DATABASE_URL", None).category, DATABASE);
    }

    #[test]
    fn domains() {
        assert_eq!(c("login", "pw", Some("https://github.com/x")).category, VCS);
        assert_eq!(c("", "pw", Some("https://aws.amazon.com/")).category, CLOUD);
        assert_eq!(c("", "pw", Some("https://api.openai.com")).category, AI);
        assert_eq!(
            c("", "pw", Some("https://hooks.slack.com")).category,
            MESSAGING
        );
        assert_eq!(
            c("", "pw", Some("https://dashboard.stripe.com")).category,
            PAYMENTS
        );
        assert_eq!(
            c("", "pw", Some("https://console.cloud.google.com")).category,
            CLOUD
        );
        assert_eq!(c("", "pw", Some("https://gitlab.com/u")).category, VCS);
    }

    #[test]
    fn keywords_and_default() {
        assert_eq!(c("OpenAI billing note", "whatever", None).category, AI);
        assert_eq!(c("AWS prod key", "whatever", None).category, CLOUD);
        assert_eq!(c("random", "nothing to see", None).category, UNCATEGORIZED);
        let r = c("random", "nothing to see", None);
        assert!(r.tags.is_empty());
        assert!(r.reason.is_none());
    }

    #[test]
    fn title_only_env_key() {
        let r = c("GITHUB_TOKEN", "", None);
        assert_eq!(r.category, VCS);
    }
}

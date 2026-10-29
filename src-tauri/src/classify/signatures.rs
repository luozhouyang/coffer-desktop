//! Secret prefix / signature detection (DESIGN.md §10.1 layer 1).

use super::{AI, CLOUD, DATABASE, MESSAGING, PAYMENTS, VCS};

/// A signature rule. `tag` is the short provider tag.
pub struct SignatureRule {
    pub prefix: &'static str,
    pub provider: &'static str,
    pub category: &'static str,
    pub tag: &'static str,
}

static RULES: &[SignatureRule] = &[
    SignatureRule {
        prefix: "sk-ant-",
        provider: "anthropic",
        category: AI,
        tag: "anthropic",
    },
    SignatureRule {
        prefix: "sk-proj-",
        provider: "openai",
        category: AI,
        tag: "openai",
    },
    SignatureRule {
        prefix: "sk-",
        provider: "openai",
        category: AI,
        tag: "openai",
    },
    SignatureRule {
        prefix: "github_pat_",
        provider: "github",
        category: VCS,
        tag: "github",
    },
    SignatureRule {
        prefix: "ghp_",
        provider: "github",
        category: VCS,
        tag: "github",
    },
    SignatureRule {
        prefix: "gho_",
        provider: "github",
        category: VCS,
        tag: "github",
    },
    SignatureRule {
        prefix: "ghu_",
        provider: "github",
        category: VCS,
        tag: "github",
    },
    SignatureRule {
        prefix: "ghs_",
        provider: "github",
        category: VCS,
        tag: "github",
    },
    SignatureRule {
        prefix: "ghr_",
        provider: "github",
        category: VCS,
        tag: "github",
    },
    SignatureRule {
        prefix: "glpat-",
        provider: "gitlab",
        category: VCS,
        tag: "gitlab",
    },
    SignatureRule {
        prefix: "AKIA",
        provider: "aws",
        category: CLOUD,
        tag: "aws",
    },
    SignatureRule {
        prefix: "ASIA",
        provider: "aws",
        category: CLOUD,
        tag: "aws",
    },
    SignatureRule {
        prefix: "AIza",
        provider: "google",
        category: CLOUD,
        tag: "google",
    },
    SignatureRule {
        prefix: "xoxb-",
        provider: "slack",
        category: MESSAGING,
        tag: "slack",
    },
    SignatureRule {
        prefix: "xoxp-",
        provider: "slack",
        category: MESSAGING,
        tag: "slack",
    },
    SignatureRule {
        prefix: "xoxa-",
        provider: "slack",
        category: MESSAGING,
        tag: "slack",
    },
    SignatureRule {
        prefix: "sk_live_",
        provider: "stripe",
        category: PAYMENTS,
        tag: "stripe",
    },
    SignatureRule {
        prefix: "sk_test_",
        provider: "stripe",
        category: PAYMENTS,
        tag: "stripe",
    },
    SignatureRule {
        prefix: "rk_live_",
        provider: "stripe",
        category: PAYMENTS,
        tag: "stripe",
    },
    SignatureRule {
        prefix: "npm_",
        provider: "npm",
        category: VCS,
        tag: "npm",
    },
];

/// Outcome of a signature match.
pub struct SignatureHit {
    pub provider: &'static str,
    pub category: &'static str,
    pub tag: &'static str,
    pub reason: String,
}

fn is_alnum(s: &str, extra: &[u8]) -> bool {
    !s.is_empty()
        && s.as_bytes()
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || extra.contains(b))
}

/// Matches a single-line value (already trimmed) against rules.
pub fn match_signature(value: &str) -> Option<SignatureHit> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }

    for rule in RULES {
        if let Some(rest) = value.strip_prefix(rule.prefix) {
            // The remainder must look like a token, not prose.
            let plausible = if rule.prefix == "sk-" {
                is_alnum(rest, b"-_") && rest.len() >= 12
            } else if rule.prefix == "npm_" {
                is_alnum(rest, b"-_") && rest.len() >= 20
            } else {
                is_alnum(rest, b"-_.")
            };
            if plausible {
                return Some(SignatureHit {
                    provider: rule.provider,
                    category: rule.category,
                    tag: rule.tag,
                    reason: format!("prefix {}", rule.prefix),
                });
            }
        }
    }

    if is_jwt(value) {
        return Some(SignatureHit {
            provider: "jwt",
            category: AI,
            tag: "jwt",
            reason: "JWT (three base64url segments)".to_string(),
        });
    }

    if let Some(scheme) = db_url_scheme(value) {
        return Some(SignatureHit {
            provider: scheme,
            category: DATABASE,
            tag: scheme,
            reason: format!("database url {scheme}://"),
        });
    }

    None
}

fn is_jwt(value: &str) -> bool {
    let parts: Vec<&str> = value.split('.').collect();
    parts.len() == 3
        && is_alnum(parts[0], b"-_")
        && parts[0].len() >= 8
        && is_alnum(parts[1], b"-_")
        && !parts[1].is_empty()
        && is_alnum(parts[2], b"-_")
        && !parts[2].is_empty()
}

/// Recognises database connection URLs by scheme (no regex needed; the rule
/// table stays linear-time and dependency-free).
fn db_url_scheme(value: &str) -> Option<&'static str> {
    let lower = value.to_ascii_lowercase();
    for scheme in [
        "postgresql",
        "postgres",
        "mysql",
        "mariadb",
        "redis",
        "rediss",
        "mongodb",
    ] {
        let prefix = format!("{scheme}://");
        let prefix_srv = format!("{scheme}+srv://");
        if lower.starts_with(&prefix) || lower.starts_with(&prefix_srv) {
            return Some(scheme);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefixes() {
        assert_eq!(match_signature("sk-ant-api03-xY9").unwrap().category, AI);
        assert_eq!(match_signature("sk-proj-Ab12").unwrap().tag, "openai");
        assert_eq!(match_signature("ghp_AbCdEf01").unwrap().category, VCS);
        assert_eq!(match_signature("glpat-Ab12").unwrap().category, VCS);
        assert_eq!(
            match_signature("AKIAQ3EGRV2LZ7PM6X4B").unwrap().category,
            CLOUD
        );
        assert_eq!(
            match_signature("ASIAQ3EGRV2LZ7PM6X4B").unwrap().category,
            CLOUD
        );
        assert_eq!(
            match_signature("xoxb-12-34-AbCd").unwrap().category,
            MESSAGING
        );
    }

    #[test]
    fn rejects_prose_and_short_tokens() {
        assert!(match_signature("sk-").is_none());
        assert!(match_signature("sk-abc").is_none());
        assert!(match_signature("sk-this is a sentence about something").is_none());
        assert!(match_signature("AKIA is the thing").is_none());
        assert!(match_signature("not a key").is_none());
    }

    #[test]
    fn jwt() {
        let jwt = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U";
        assert_eq!(match_signature(jwt).unwrap().tag, "jwt");
        assert!(match_signature("a.b").is_none());
        assert!(match_signature("eyJhbGciOiJIUzI1NiJ9.abc").is_none());
    }

    #[test]
    fn database_urls() {
        assert_eq!(
            match_signature("postgres://u:p@h/d").unwrap().category,
            DATABASE
        );
        assert_eq!(match_signature("POSTGRESQL://h").unwrap().tag, "postgresql");
        assert_eq!(
            match_signature("mongodb+srv://x").unwrap().category,
            DATABASE
        );
        assert!(match_signature("http://example.com").is_none());
    }
}

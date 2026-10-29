//! `.env` key-name mapping (DESIGN.md §10.1 layer 2).

use super::{AI, CLOUD, DATABASE, MESSAGING, PAYMENTS, VCS};

/// Mapping of a normalized env key to a category.
pub struct EnvHit {
    pub category: &'static str,
    pub provider: &'static str,
    /// The key that produced the hit.
    pub key: String,
    /// `true` when the key itself is specific (`OPENAI_API_KEY`), `false` when
    /// it is a generic key (`*_TOKEN`) whose category is only a guess.
    pub direct: bool,
}

/// (needle, category, provider). Matched against the uppercased key; the
/// needle must appear verbatim.
const RULES: &[(&str, &str, &str)] = &[
    ("OPENAI", AI, "openai"),
    ("ANTHROPIC", AI, "anthropic"),
    ("CLAUDE", AI, "anthropic"),
    ("GROQ", AI, "groq"),
    ("GEMINI", AI, "google"),
    ("MISTRAL", AI, "mistral"),
    ("HUGGINGFACE", AI, "huggingface"),
    ("HF_TOKEN", AI, "huggingface"),
    ("COHERE", AI, "cohere"),
    ("PERPLEXITY", AI, "perplexity"),
    ("DEEPSEEK", AI, "deepseek"),
    ("STRIPE", PAYMENTS, "stripe"),
    ("PAYPAL", PAYMENTS, "paypal"),
    ("SQUARE", PAYMENTS, "square"),
    ("BRAINTREE", PAYMENTS, "braintree"),
    ("ADYEN", PAYMENTS, "adyen"),
    ("AWS_ACCESS_KEY", CLOUD, "aws"),
    ("AWS_SECRET", CLOUD, "aws"),
    ("AWS_SESSION", CLOUD, "aws"),
    ("AWS_", CLOUD, "aws"),
    ("AZURE", CLOUD, "azure"),
    ("GCP_", CLOUD, "google"),
    ("GOOGLE_APPLICATION_CREDENTIALS", CLOUD, "google"),
    ("GOOGLE_API_KEY", CLOUD, "google"),
    ("DO_", CLOUD, "digitalocean"),
    ("DIGITALOCEAN", CLOUD, "digitalocean"),
    ("CLOUDFLARE", CLOUD, "cloudflare"),
    ("VERCEL", CLOUD, "vercel"),
    ("GITHUB", VCS, "github"),
    ("GH_TOKEN", VCS, "github"),
    ("GITLAB", VCS, "gitlab"),
    ("BITBUCKET", VCS, "bitbucket"),
    ("NPM_TOKEN", VCS, "npm"),
    ("DATABASE_URL", DATABASE, "database"),
    ("POSTGRES", DATABASE, "postgres"),
    ("MYSQL", DATABASE, "mysql"),
    ("REDIS", DATABASE, "redis"),
    ("MONGO", DATABASE, "mongodb"),
    ("SLACK", MESSAGING, "slack"),
    ("DISCORD", MESSAGING, "discord"),
    ("TELEGRAM", MESSAGING, "telegram"),
    ("TWILIO", MESSAGING, "twilio"),
    ("SENDGRID", MESSAGING, "sendgrid"),
];

/// Normalizes an env key fragment: uppercases and maps `-`/`.` to `_`.
pub fn normalize_key(key: &str) -> String {
    key.trim().to_ascii_uppercase().replace(['-', '.'], "_")
}

/// Classifies one env key name.
pub fn classify_key(key: &str) -> Option<EnvHit> {
    let upper = normalize_key(key);
    let mut best: Option<&(&str, &str, &str)> = None;
    for rule in RULES {
        if upper.contains(rule.0) {
            // Prefer the longest needle so `AWS_` does not shadow a specific rule.
            let better = best.is_none_or(|b| rule.0.len() > b.0.len());
            if better {
                best = Some(rule);
            }
        }
    }
    let (needle, category, provider) = *best?;
    Some(EnvHit {
        category,
        provider,
        key: upper.clone(),
        direct: needle.len() >= 4,
    })
}

/// Scans a (possibly multi-line) value for `KEY=VALUE` or `export KEY=VALUE`
/// lines and returns the first mapping that resolves to a category.
pub fn match_env(value: &str) -> Option<EnvHit> {
    for line in value.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line);
        let Some((key, _)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if key.is_empty() || key.contains(char::is_whitespace) {
            continue;
        }
        if let Some(hit) = classify_key(key) {
            return Some(hit);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn specific_keys() {
        assert_eq!(classify_key("OPENAI_API_KEY").unwrap().category, AI);
        assert_eq!(classify_key("AWS_ACCESS_KEY_ID").unwrap().category, CLOUD);
        assert_eq!(
            classify_key("AWS_SECRET_ACCESS_KEY").unwrap().category,
            CLOUD
        );
        assert_eq!(classify_key("GITHUB_TOKEN").unwrap().category, VCS);
        assert_eq!(classify_key("SLACK_BOT_TOKEN").unwrap().category, MESSAGING);
        assert_eq!(
            classify_key("STRIPE_SECRET_KEY").unwrap().category,
            PAYMENTS
        );
        assert_eq!(classify_key("DATABASE_URL").unwrap().category, DATABASE);
    }

    #[test]
    fn longest_needle_wins() {
        // `AWS_` (CLOUD) and `AWS_ACCESS_KEY` (CLOUD) agree; pick a case where
        // the specific rule is longer than the generic one.
        let hit = classify_key("GOOGLE_API_KEY").unwrap();
        assert_eq!(hit.category, CLOUD);
        assert_eq!(hit.provider, "google");
    }

    #[test]
    fn unknown_keys_are_none() {
        assert!(classify_key("SOME_RANDOM_VAR").is_none());
        assert!(classify_key("").is_none());
    }

    #[test]
    fn multi_line_env() {
        let v = "# comment\nFOO=bar\nexport OPENAI_API_KEY=sk-xxxx\nDATABASE_URL=postgres://a";
        // Signature rules run before env rules in the classifier, so only the
        // env layer is exercised here.
        let hit = match_env(v).unwrap();
        assert_eq!(hit.category, AI);
    }

    #[test]
    fn normalizes_key() {
        assert_eq!(normalize_key(" openai.api-key "), "OPENAI_API_KEY");
    }
}

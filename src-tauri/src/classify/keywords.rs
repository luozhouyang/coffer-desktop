//! Keyword fallback over title / notes / url (DESIGN.md §10.1 layer 4).

use super::{AI, CLOUD, DATABASE, MESSAGING, PAYMENTS, VCS};

pub struct KeywordHit {
    pub category: &'static str,
    pub keyword: &'static str,
    pub tags: &'static [&'static str],
}

/// (keyword, category, tags). Checked in order; the first hit wins, so put
/// more specific keywords first.
const KEYWORDS: &[(&str, &str, &[&str])] = &[
    ("anthropic", AI, &["anthropic"]),
    ("claude", AI, &["anthropic"]),
    ("openai", AI, &["openai"]),
    ("chatgpt", AI, &["openai"]),
    ("gpt", AI, &["openai"]),
    ("gemini", AI, &["google"]),
    ("huggingface", AI, &["huggingface"]),
    ("llm", AI, &["ai"]),
    ("inference", AI, &["ai"]),
    ("aws", CLOUD, &["aws"]),
    ("amazon", CLOUD, &["aws"]),
    ("azure", CLOUD, &["azure"]),
    ("gcp", CLOUD, &["google"]),
    ("google cloud", CLOUD, &["google"]),
    ("cloudflare", CLOUD, &["cloudflare"]),
    ("digitalocean", CLOUD, &["digitalocean"]),
    ("vercel", CLOUD, &["vercel"]),
    ("github", VCS, &["github"]),
    ("gitlab", VCS, &["gitlab"]),
    ("bitbucket", VCS, &["bitbucket"]),
    ("npm", VCS, &["npm"]),
    ("stripe", PAYMENTS, &["stripe"]),
    ("paypal", PAYMENTS, &["paypal"]),
    ("billing", PAYMENTS, &["billing"]),
    ("slack", MESSAGING, &["slack"]),
    ("discord", MESSAGING, &["discord"]),
    ("telegram", MESSAGING, &["telegram"]),
    ("twilio", MESSAGING, &["twilio"]),
    ("sendgrid", MESSAGING, &["sendgrid"]),
    ("postgres", DATABASE, &["postgres"]),
    ("postgresql", DATABASE, &["postgres"]),
    ("mysql", DATABASE, &["mysql"]),
    ("mongo", DATABASE, &["mongodb"]),
    ("redis", DATABASE, &["redis"]),
    ("sqlite", DATABASE, &["sqlite"]),
    ("database", DATABASE, &["database"]),
];

/// Matches the lowercased haystack (title + notes + url) against keywords.
pub fn match_keywords(haystack_lower: &str) -> Option<KeywordHit> {
    KEYWORDS
        .iter()
        .find(|(kw, _, _)| haystack_lower.contains(kw))
        .map(|(kw, cat, tags)| KeywordHit {
            category: cat,
            keyword: kw,
            tags,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_expected_categories() {
        assert_eq!(match_keywords("openai billing").unwrap().category, AI);
        assert_eq!(match_keywords("aws prod").unwrap().category, CLOUD);
        assert_eq!(
            match_keywords("postgres replica").unwrap().category,
            DATABASE
        );
        assert!(match_keywords("totally generic").is_none());
    }
}

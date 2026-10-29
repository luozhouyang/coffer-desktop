//! URL / domain based classification (DESIGN.md §10.1 layer 3).

use super::{AI, CLOUD, DATABASE, MESSAGING, PAYMENTS, VCS};

pub struct UrlHit {
    pub category: &'static str,
    /// Registered domain (or a normalized prefix) that matched.
    pub domain: String,
    pub tag: Option<String>,
}

/// (domain suffix, category, provider tag).
const DOMAINS: &[(&str, &str, &str)] = &[
    ("github.com", VCS, "github"),
    ("github.dev", VCS, "github"),
    ("gitlab.com", VCS, "gitlab"),
    ("bitbucket.org", VCS, "bitbucket"),
    ("npmjs.com", VCS, "npm"),
    ("npmjs.org", VCS, "npm"),
    ("openai.com", AI, "openai"),
    ("anthropic.com", AI, "anthropic"),
    ("console.anthropic.com", AI, "anthropic"),
    ("claude.ai", AI, "anthropic"),
    ("huggingface.co", AI, "huggingface"),
    ("groq.com", AI, "groq"),
    ("mistral.ai", AI, "mistral"),
    ("perplexity.ai", AI, "perplexity"),
    ("deepseek.com", AI, "deepseek"),
    ("aws.amazon.com", CLOUD, "aws"),
    ("console.aws.amazon.com", CLOUD, "aws"),
    ("console.cloud.google.com", CLOUD, "google"),
    ("cloud.google.com", CLOUD, "google"),
    ("portal.azure.com", CLOUD, "azure"),
    ("cloud.digitalocean.com", CLOUD, "digitalocean"),
    ("dash.cloudflare.com", CLOUD, "cloudflare"),
    ("vercel.com", CLOUD, "vercel"),
    ("supabase.com", CLOUD, "supabase"),
    ("vultr.com", CLOUD, "vultr"),
    ("linode.com", CLOUD, "linode"),
    ("stripe.com", PAYMENTS, "stripe"),
    ("paypal.com", PAYMENTS, "paypal"),
    ("squareup.com", PAYMENTS, "square"),
    ("braintreepayments.com", PAYMENTS, "braintree"),
    ("slack.com", MESSAGING, "slack"),
    ("discord.com", MESSAGING, "discord"),
    ("telegram.org", MESSAGING, "telegram"),
    ("twilio.com", MESSAGING, "twilio"),
    ("mailchimp.com", MESSAGING, "mailchimp"),
    ("sendgrid.com", MESSAGING, "sendgrid"),
    ("supabase.co", DATABASE, "supabase"),
    ("neon.tech", DATABASE, "neon"),
    ("planetscale.com", DATABASE, "planetscale"),
    ("mongodb.com", DATABASE, "mongodb"),
    ("redis.com", DATABASE, "redis"),
    ("turso.tech", DATABASE, "turso"),
];

/// Extracts the host from a URL or bare host string, lowercasing it and
/// dropping user info, port and trailing dot.
pub fn host_of(url: &str) -> Option<String> {
    let url = url.trim();
    if url.is_empty() {
        return None;
    }
    let without_scheme = match url.find("://") {
        Some(i) => &url[i + 3..],
        None => url,
    };
    let authority = without_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default();
    let authority = match authority.rsplit_once('@') {
        Some((_, host)) => host,
        None => authority,
    };
    // Strip a port unless this looks like an IPv6 literal.
    let host: &str = if authority.starts_with('[') {
        &authority[..authority.find(']')? + 1]
    } else {
        authority.split(':').next()?
    };
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    if host.is_empty() || !host.contains('.') {
        return None;
    }
    Some(host)
}

/// Matches a URL against the domain table. The longest matching suffix wins
/// and must align on a label boundary (`notgithub.com` does not match
/// `github.com`).
pub fn match_url(url: &str) -> Option<UrlHit> {
    let host = host_of(url)?;
    let mut best: Option<&(&str, &str, &str)> = None;
    for rule in DOMAINS {
        let (domain, _, _) = *rule;
        let boundary_ok = host == domain || host.ends_with(&format!(".{domain}"));
        if boundary_ok && best.is_none_or(|b| domain.len() > b.0.len()) {
            best = Some(rule);
        }
    }
    let (domain, category, tag) = *best?;
    Some(UrlHit {
        category,
        domain: domain.to_string(),
        tag: Some(tag.to_string()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_extraction() {
        assert_eq!(
            host_of("https://GitHub.com/a/b?x=1").as_deref(),
            Some("github.com")
        );
        assert_eq!(
            host_of("user:pw@postgres.example.com:5432/db").as_deref(),
            Some("postgres.example.com")
        );
        assert_eq!(
            host_of("https://github.com./x").as_deref(),
            Some("github.com")
        );
        assert_eq!(host_of("localhost"), None);
    }

    #[test]
    fn longest_suffix_wins() {
        let hit = match_url("https://console.aws.amazon.com/console/home").unwrap();
        assert_eq!(hit.domain, "console.aws.amazon.com");
        assert_eq!(hit.category, CLOUD);
    }

    #[test]
    fn requires_label_boundary() {
        assert!(match_url("https://notgithub.com/x").is_none());
        assert!(match_url("https://github.com.evil.io/x").is_none());
        assert!(match_url("https://github.com").is_some());
    }
}

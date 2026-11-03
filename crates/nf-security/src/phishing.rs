//! Local anti-phishing heuristics (no network, no AI).
//!
//! Signals:
//! 1. Homograph / mixed-script hosts (Punycode `xn--` labels mixing Latin with
//!    Cyrillic or Greek).
//! 2. Look-alikes of protected domains: same "skeleton" after mapping
//!    confusable characters (rn→m, 0→o, Cyrillic а→a...), or one edit away.
//! 3. A protected brand used as a sub-domain of an unrelated site
//!    (`paypal.com.secure-login.xyz`).
//! 4. Raw IP address hosts and plain HTTP for login pages.
//!
//! Protected domains = a built-in list of frequently impersonated services +
//! the user's own sites (bookmarks, saved passwords).

use serde::Serialize;
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "detail")]
pub enum Reason {
    MixedScript,
    LookAlike(String),
    DeceptiveSubdomain(String),
    IpAddressHost,
    InsecureLogin,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Verdict {
    pub host: String,
    pub reasons: Vec<Reason>,
}

impl Verdict {
    pub fn is_suspicious(&self) -> bool {
        !self.reasons.is_empty()
    }

    /// French warning text. Google's guidance for Safe Browsing style
    /// warnings is followed: "suspect", "possible", never "certain".
    pub fn message(&self) -> String {
        let mut parts = Vec::new();
        for r in &self.reasons {
            parts.push(match r {
                Reason::MixedScript => "le nom de domaine mélange plusieurs alphabets (attaque par homographe possible)".to_string(),
                Reason::LookAlike(d) => format!("ce domaine ressemble fortement à {d}"),
                Reason::DeceptiveSubdomain(d) => format!("le nom {d} apparaît dans l'adresse, mais le site réel est {}", self.host),
                Reason::IpAddressHost => "le site est désigné par une adresse IP brute".to_string(),
                Reason::InsecureLogin => "la connexion n'est pas chiffrée (HTTP)".to_string(),
            });
        }
        format!("Site suspect : {}.", parts.join(" ; "))
    }
}

/// Frequently impersonated services (registrable domains).
pub const BUILTIN_PROTECTED: &[&str] = &[
    "google.com", "gmail.com", "youtube.com", "apple.com", "icloud.com", "microsoft.com",
    "live.com", "outlook.com", "office.com", "github.com", "paypal.com", "amazon.com",
    "amazon.fr", "facebook.com", "instagram.com", "whatsapp.com", "twitter.com", "x.com",
    "linkedin.com", "netflix.com", "spotify.com", "steampowered.com", "steamcommunity.com",
    "discord.com", "binance.com", "coinbase.com", "dropbox.com", "adobe.com", "ebay.com",
    "booking.com", "laposte.fr", "ameli.fr", "impots.gouv.fr", "service-public.fr",
    "bnpparibas", "credit-agricole.fr", "societegenerale.fr", "boursorama.com", "ing.be",
    "belfius.be", "kbc.be", "bpost.be", "itsme-id.com", "proton.me", "twitch.tv",
];

/// Map confusable characters to a canonical skeleton (subset of Unicode TR39
/// covering Latin look-alikes from Cyrillic, Greek and digits).
pub fn skeleton(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars().flat_map(char::to_lowercase) {
        let m = match c {
            'а' => 'a', 'е' => 'e', 'о' | 'ο' | '0' => 'o', 'р' | 'ρ' => 'p', 'с' | 'ϲ' => 'c',
            'у' => 'y', 'х' | 'χ' => 'x', 'і' | 'ı' | 'ι' | '1' | 'l' | '|' => 'l', 'ј' => 'j',
            'ѕ' => 's', 'ԁ' => 'd', 'ɡ' => 'g', 'ν' => 'v', 'ԝ' | 'ω' => 'w', 'κ' => 'k',
            'τ' => 't', 'η' => 'n', 'μ' => 'u', 'β' => 'b', '5' => 's', '3' => 'e', '@' => 'a',
            other => other,
        };
        out.push(m);
    }
    out.replace("rn", "m").replace("vv", "w").replace("cl", "d")
}

fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0; b.len() + 1];
    for i in 1..=a.len() {
        cur[0] = i;
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Script {
    Latin,
    Cyrillic,
    Greek,
    Other,
}

fn script(c: char) -> Option<Script> {
    match c {
        'a'..='z' | 'A'..='Z' => Some(Script::Latin),
        '\u{00C0}'..='\u{024F}' => Some(Script::Latin),
        '\u{0400}'..='\u{04FF}' => Some(Script::Cyrillic),
        '\u{0370}'..='\u{03FF}' => Some(Script::Greek),
        c if c.is_ascii_digit() || c == '-' || c == '.' => None,
        _ => Some(Script::Other),
    }
}

fn mixed_script(label: &str) -> bool {
    let mut seen: Option<Script> = None;
    for c in label.chars() {
        if let Some(s) = script(c) {
            if s == Script::Other {
                continue;
            }
            match seen {
                None => seen = Some(s),
                Some(prev) if prev != s => return true,
                _ => {}
            }
        }
    }
    false
}

/// Registrable domain ("eTLD+1") of a host, lowercase.
pub fn registrable_domain(host: &str) -> String {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    psl::domain_str(&host).map(str::to_string).unwrap_or(host)
}

fn label_without_suffix(domain: &str) -> &str {
    domain.split('.').next().unwrap_or(domain)
}

pub struct PhishingDetector {
    protected: BTreeSet<String>,
}

impl Default for PhishingDetector {
    fn default() -> Self {
        Self::new(std::iter::empty::<String>())
    }
}

impl PhishingDetector {
    /// `user_domains`: hosts from bookmarks and saved passwords.
    pub fn new(user_domains: impl IntoIterator<Item = String>) -> Self {
        let mut protected: BTreeSet<String> =
            BUILTIN_PROTECTED.iter().map(|d| d.to_string()).collect();
        for d in user_domains {
            let host = url::Url::parse(&d)
                .ok()
                .and_then(|u| u.host_str().map(str::to_string))
                .unwrap_or(d);
            let reg = registrable_domain(&host);
            if reg.contains('.') {
                protected.insert(reg);
            }
        }
        Self { protected }
    }

    /// Analyse a top-level navigation URL.
    pub fn check(&self, raw_url: &str, has_password_field: bool) -> Option<Verdict> {
        let parsed = url::Url::parse(raw_url).ok()?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return None;
        }
        let host_ascii = parsed.host_str()?.to_ascii_lowercase();
        let mut reasons = Vec::new();

        if matches!(parsed.host(), Some(url::Host::Ipv4(_)) | Some(url::Host::Ipv6(_))) {
            if has_password_field {
                reasons.push(Reason::IpAddressHost);
            }
            if parsed.scheme() == "http" && has_password_field {
                reasons.push(Reason::InsecureLogin);
            }
            return Some(Verdict { host: host_ascii, reasons });
        }

        let (unicode_host, _) = idna::domain_to_unicode(&host_ascii);
        let reg_ascii = registrable_domain(&host_ascii);
        let (reg_unicode, _) = idna::domain_to_unicode(&reg_ascii);

        if host_ascii.split('.').any(|l| l.starts_with("xn--")) && unicode_host.split('.').any(mixed_script) {
            reasons.push(Reason::MixedScript);
        }

        if !self.protected.contains(&reg_ascii) {
            let candidate = skeleton(label_without_suffix(&reg_unicode));
            for p in &self.protected {
                let p_label = label_without_suffix(p);
                if p_label.len() < 4 {
                    continue;
                }
                let p_skel = skeleton(p_label);
                let close = candidate == p_skel
                    || (p_label.len() >= 6 && levenshtein(&candidate, &p_skel) == 1);
                if close {
                    reasons.push(Reason::LookAlike(p.clone()));
                    break;
                }
            }
            // Protected brand as a sub-domain of another site.
            let sub = host_ascii.strip_suffix(&reg_ascii).unwrap_or("");
            for p in &self.protected {
                if p.contains('.') && (sub.contains(&format!("{p}.")) || sub.starts_with(&format!("{}.", label_without_suffix(p)))) && label_without_suffix(p).len() >= 4 {
                    reasons.push(Reason::DeceptiveSubdomain(p.clone()));
                    break;
                }
            }
        }

        if parsed.scheme() == "http" && has_password_field {
            reasons.push(Reason::InsecureLogin);
        }
        Some(Verdict { host: host_ascii, reasons })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d() -> PhishingDetector {
        PhishingDetector::new(vec!["https://www.mybank.example".to_string()])
    }

    #[test]
    fn legit_sites_pass() {
        for u in [
            "https://www.google.com/",
            "https://accounts.google.com/signin",
            "https://www.paypal.com/fr/home",
            "https://doc.rust-lang.org/",
            "https://fr.wikipedia.org/",
        ] {
            let v = d().check(u, true).unwrap();
            assert!(!v.is_suspicious(), "{u}: {:?}", v.reasons);
        }
    }

    #[test]
    fn look_alikes_flagged() {
        for (u, target) in [
            ("https://paypa1.com/login", "paypal.com"),
            ("https://rnicrosoft.com/", "microsoft.com"),
            ("https://g00gle.com/", "google.com"),
            ("https://netfIix.com/", "netflix.com"),
            ("https://amazom.com/", "amazon.com"),
        ] {
            let v = d().check(u, false).unwrap();
            assert!(v.reasons.contains(&Reason::LookAlike(target.into())), "{u}: {:?}", v.reasons);
        }
    }

    #[test]
    fn homograph_flagged() {
        // "аpple.com" with a Cyrillic а
        let v = d().check("https://xn--pple-43d.com/", false).unwrap();
        assert!(v.reasons.contains(&Reason::MixedScript), "{:?}", v.reasons);
    }

    #[test]
    fn deceptive_subdomain_flagged() {
        let v = d().check("https://paypal.com.secure-login.xyz/", true).unwrap();
        assert!(v.reasons.contains(&Reason::DeceptiveSubdomain("paypal.com".into())), "{:?}", v.reasons);
    }

    #[test]
    fn user_domains_are_protected() {
        let v = d().check("https://mybamk.example/", false).unwrap();
        assert!(v.is_suspicious(), "{:?}", v.reasons);
    }

    #[test]
    fn ip_and_http_login() {
        let v = d().check("http://192.168.1.20/login", true).unwrap();
        assert!(v.reasons.contains(&Reason::IpAddressHost));
        assert!(v.reasons.contains(&Reason::InsecureLogin));
        assert!(v.message().starts_with("Site suspect"));
    }
}

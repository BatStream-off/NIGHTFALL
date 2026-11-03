//! Log redaction. Every log line written by NIGHTFALL passes through
//! [`sanitize`], which removes credentials, cookies, tokens and URL queries.

/// Strip userinfo, query string and fragment from a URL, keeping
/// scheme, host and path only. Non-URL input is returned unchanged.
pub fn redact_url(url: &str) -> String {
    let Some(scheme_end) = url.find("://") else {
        return url.to_string();
    };
    let (scheme, rest) = url.split_at(scheme_end + 3);
    let rest = rest.split(['?', '#']).next().unwrap_or("");
    let rest = match rest.find('/') {
        Some(slash) => {
            let (authority, path) = rest.split_at(slash);
            format!("{}{}", strip_userinfo(authority), path)
        }
        None => strip_userinfo(rest).to_string(),
    };
    format!("{scheme}{rest}")
}

fn strip_userinfo(authority: &str) -> &str {
    match authority.rfind('@') {
        Some(at) => &authority[at + 1..],
        None => authority,
    }
}

const SENSITIVE_HEADERS: &[&str] = &[
    "cookie",
    "set-cookie",
    "authorization",
    "proxy-authorization",
    "x-api-key",
    "x-auth-token",
    "x-csrf-token",
];

/// Redact a whole log line:
/// - values of sensitive headers (`Cookie: ...`, `Authorization: ...`)
/// - `Bearer`/`Basic` credentials
/// - JWT-looking tokens and long opaque secrets (>= 32 base64/hex chars)
/// - URL queries, fragments and userinfo
pub fn sanitize(line: &str) -> String {
    // 1. Sensitive headers: drop everything after "<header>:" on the line.
    let lower = line.to_ascii_lowercase();
    for header in SENSITIVE_HEADERS {
        let needle = format!("{header}:");
        if let Some(pos) = lower.find(&needle) {
            let cut = pos + needle.len();
            return format!("{} [redacted]", &line[..cut]);
        }
    }

    let mut out = Vec::new();
    let mut redact_next = false;
    for word in line.split(' ') {
        if redact_next {
            out.push("[redacted]".to_string());
            redact_next = false;
            continue;
        }
        let lw = word.to_ascii_lowercase();
        if lw == "bearer" || lw == "basic" {
            out.push(word.to_string());
            redact_next = true;
            continue;
        }
        if word.contains("://") {
            out.push(redact_url(word));
            continue;
        }
        if looks_like_secret(word) {
            out.push("[redacted]".to_string());
            continue;
        }
        out.push(word.to_string());
    }
    out.join(" ")
}

fn looks_like_secret(word: &str) -> bool {
    let w = word.trim_matches(|c: char| c == '"' || c == '\'' || c == ',' || c == ';');
    // key=value pairs whose key hints at a secret
    if let Some((k, _)) = w.split_once('=') {
        let k = k.to_ascii_lowercase();
        if ["token", "session", "sid", "password", "passwd", "secret", "key", "auth"]
            .iter()
            .any(|s| k.contains(s))
        {
            return true;
        }
    }
    // JWT: three base64url segments starting with eyJ
    if w.starts_with("eyJ") && w.matches('.').count() == 2 {
        return true;
    }
    w.len() >= 32
        && w
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '+' || c == '/' || c == '=')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_lose_query_and_userinfo() {
        assert_eq!(
            redact_url("https://user:pw@example.com/a/b?token=abc#frag"),
            "https://example.com/a/b"
        );
        assert_eq!(redact_url("https://example.com"), "https://example.com");
        assert_eq!(redact_url("not a url"), "not a url");
    }

    #[test]
    fn headers_are_redacted() {
        assert_eq!(sanitize("Cookie: SID=abc; HSID=def"), "Cookie: [redacted]");
        assert_eq!(
            sanitize("request header authorization: Bearer xyz"),
            "request header authorization: [redacted]"
        );
    }

    #[test]
    fn tokens_are_redacted() {
        let jwt = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjMifQ.c2lnbmF0dXJl";
        let line = format!("got {jwt} from server");
        assert_eq!(sanitize(&line), "got [redacted] from server");
        assert_eq!(sanitize("use Bearer abc123 now"), "use Bearer [redacted] now");
        assert_eq!(sanitize("session_id=42"), "[redacted]");
        assert_eq!(
            sanitize("key 0123456789abcdef0123456789abcdef ok"),
            "key [redacted] ok"
        );
        assert_eq!(
            sanitize("load https://site.test/login?next=/x done"),
            "load https://site.test/login done"
        );
    }

    #[test]
    fn harmless_text_is_kept() {
        assert_eq!(sanitize("tab 3 loaded in 120 ms"), "tab 3 loaded in 120 ms");
    }
}

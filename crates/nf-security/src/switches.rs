//! Chromium switches that weaken security or enable remote control.
//!
//! Info-stealers relaunch Chromium browsers with `--remote-debugging-port`
//! to read cookies in clear text through DevTools. NIGHTFALL strips these
//! switches before Chromium starts (release builds), and CI fails if any of
//! them is ever added by NIGHTFALL itself.

/// Switch names (without leading dashes) that are always refused.
pub const FORBIDDEN: &[&str] = &[
    "remote-debugging-port",
    "remote-debugging-pipe",
    "remote-debugging-address",
    "remote-allow-origins",
    "remote-debugging-io-pipes",
    "enable-automation",
    "headless",
    "no-sandbox",
    "disable-gpu-sandbox",
    "disable-setuid-sandbox",
    "disable-web-security",
    "disable-site-isolation-trials",
    "disable-features=IsolateOrigins",
    "allow-running-insecure-content",
    "ignore-certificate-errors",
    "ignore-certificate-errors-spki-list",
    "ignore-urlfetcher-cert-requests",
    "allow-insecure-localhost",
    "load-extension",
    "disable-extensions-except",
    "allow-file-access-from-files",
    "single-process",
    "in-process-gpu",
    "user-data-dir",
    "proxy-server",
    "proxy-pac-url",
    "host-resolver-rules",
    "js-flags",
    "renderer-cmd-prefix",
    "gpu-launcher",
    "utility-cmd-prefix",
    "browser-subprocess-path",
];

/// Normalize `--Name=value` / `-name` to `name` (lowercase, no value).
pub fn switch_name(arg: &str) -> Option<String> {
    let trimmed = arg.trim_start_matches('-');
    if trimmed.len() == arg.len() || trimmed.is_empty() {
        return None;
    }
    let name = trimmed.split('=').next().unwrap_or(trimmed);
    Some(name.to_ascii_lowercase())
}

/// Is this command-line argument forbidden?
pub fn is_forbidden(arg: &str) -> bool {
    let Some(name) = switch_name(arg) else {
        return false;
    };
    let lower = arg.trim_start_matches('-').to_ascii_lowercase();
    FORBIDDEN.iter().any(|f| {
        if f.contains('=') {
            lower.starts_with(&f.to_ascii_lowercase())
        } else {
            name == *f
        }
    })
}

/// Names of forbidden switches present in `args`.
pub fn forbidden_in<'a>(args: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    args.into_iter()
        .filter(|a| is_forbidden(a))
        .filter_map(switch_name)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_dangerous_switches() {
        assert!(is_forbidden("--remote-debugging-port=9222"));
        assert!(is_forbidden("--REMOTE-DEBUGGING-PORT=9222"));
        assert!(is_forbidden("--no-sandbox"));
        assert!(is_forbidden("-disable-web-security"));
        assert!(is_forbidden("--disable-features=IsolateOrigins,site-per-process"));
        assert!(is_forbidden("--user-data-dir=C:\\stolen"));
    }

    #[test]
    fn allows_harmless_ones() {
        assert!(!is_forbidden("--lang=fr"));
        assert!(!is_forbidden("--disable-features=Translate"));
        assert!(!is_forbidden("https://example.com"));
        assert!(!is_forbidden("--type=renderer"));
    }

    #[test]
    fn lists_offenders() {
        let args = ["nightfall.exe", "--remote-debugging-port=9222", "--lang=fr", "--headless"];
        assert_eq!(forbidden_in(args), vec!["remote-debugging-port", "headless"]);
    }
}

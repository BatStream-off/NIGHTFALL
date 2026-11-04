//! HTTPS-First: http:// navigations of web tabs are upgraded to https://;
//! when the site has no working HTTPS, a warning page asks before loading
//! the insecure version (implemented here: Chromium's own HTTPS-First mode
//! relies on Chrome tab helpers that Alloy-style browsers do not attach).
//!
//! Not upgraded: local and private network addresses, IP addresses,
//! single-label and local names, explicit ports (they rarely have HTTPS).

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

/// Upgrades made since start (self test).
pub static UPGRADES: AtomicU64 = AtomicU64::new(0);

/// Sites the user chose to load over HTTP (this session only).
static ALLOWED: Mutex<Option<HashSet<String>>> = Mutex::new(None);
/// browser id -> (https URL tried, original http URL).
static PENDING: Mutex<Option<HashMap<i32, (String, String)>>> = Mutex::new(None);

pub const WARNING_PAGE: &str = "nightfall://app/insecure.html";

fn private_or_local(host: &str) -> bool {
    let h = host.trim_end_matches('.').to_ascii_lowercase();
    if !h.contains('.') {
        return true; // "localhost", intranet names
    }
    ["localhost", "local", "lan", "internal", "home", "arpa", "test", "invalid", "example", "onion", "corp", "intranet"]
        .iter()
        .any(|tld| h.ends_with(&format!(".{tld}")))
}

/// The https:// URL to try instead of `url`, if it should be upgraded.
pub fn upgrade(url: &str) -> Option<String> {
    let parsed = url::Url::parse(url).ok()?;
    if parsed.scheme() != "http" || parsed.port().is_some() {
        return None;
    }
    match parsed.host()? {
        url::Host::Domain(d) if !private_or_local(d) => {}
        _ => return None, // IP addresses and local names
    }
    let mut up = parsed.clone();
    up.set_scheme("https").ok()?;
    Some(up.to_string())
}

fn host_of(url: &str) -> String {
    url::Url::parse(url).ok().and_then(|u| u.host_str().map(str::to_ascii_lowercase)).unwrap_or_default()
}

/// Allowed hosts cover their subdomains (http://site.fr allows
/// http://www.site.fr, as the user chose to trust the site).
pub fn is_allowed(url: &str) -> bool {
    let h = host_of(url);
    ALLOWED
        .lock()
        .ok()
        .and_then(|a| a.as_ref().map(|s| s.iter().any(|a| h == *a || h.ends_with(&format!(".{a}")))))
        .unwrap_or(false)
}

/// The user chose "Continuer vers le site" for this http:// address.
pub fn allow(url: &str) {
    if let Ok(mut a) = ALLOWED.lock() {
        a.get_or_insert_with(HashSet::new).insert(host_of(url));
    }
}

/// What to do with a top-level navigation of a web tab.
#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    Continue,
    /// Cancel and load this https:// URL instead.
    Upgrade(String),
    /// Cancel and show the warning page for this http:// URL (the site
    /// sent us back to http:// right after the upgrade).
    Warn(String),
}

pub fn on_navigation(browser_id: i32, url: &str, enabled: bool) -> Action {
    if !enabled || !url.starts_with("http://") || is_allowed(url) {
        return Action::Continue;
    }
    let Some(https) = upgrade(url) else { return Action::Continue };
    let mut p = PENDING.lock().unwrap_or_else(|e| e.into_inner());
    let map = p.get_or_insert_with(HashMap::new);
    // Redirected back to http:// on the same host just after the upgrade.
    if map.get(&browser_id).map(|(tried, _)| host_of(tried) == host_of(url)).unwrap_or(false) {
        map.remove(&browser_id);
        return Action::Warn(url.to_string());
    }
    map.insert(browser_id, (https.clone(), url.to_string()));
    UPGRADES.fetch_add(1, Ordering::Relaxed);
    Action::Upgrade(https)
}

/// A main-frame load failed: the http:// URL to warn about, if it was an
/// upgrade made by NIGHTFALL.
pub fn on_load_error(browser_id: i32, failed_url: &str, error_code: i32) -> Option<String> {
    // -3 = ERR_ABORTED (the user stopped, or another navigation started).
    if error_code == -3 {
        return None;
    }
    let mut p = PENDING.lock().ok()?;
    let map = p.as_mut()?;
    let (tried, original) = map.get(&browser_id)?.clone();
    if tried.trim_end_matches('/') != failed_url.trim_end_matches('/') {
        return None;
    }
    map.remove(&browser_id);
    Some(original)
}

/// The upgraded page loaded (or the tab closed): forget the attempt.
pub fn done(browser_id: i32) {
    if let Ok(mut p) = PENDING.lock() {
        if let Some(m) = p.as_mut() {
            m.remove(&browser_id);
        }
    }
}

/// Address of the warning page for an http:// URL.
pub fn warning_url(http_url: &str) -> String {
    let enc: String = url::form_urlencoded::byte_serialize(http_url.as_bytes()).collect();
    format!("{WARNING_PAGE}#{enc}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upgrade_rules() {
        assert_eq!(upgrade("http://example.com/a?b=1").as_deref(), Some("https://example.com/a?b=1"));
        assert_eq!(upgrade("http://www.lemonde.fr").as_deref(), Some("https://www.lemonde.fr/"));
        for no in [
            "https://example.com/",
            "http://localhost:8080/",
            "http://localhost/",
            "http://192.168.1.1/",
            "http://10.0.0.2/",
            "http://8.8.8.8/",
            "http://[::1]/",
            "http://imprimante/",
            "http://nas.local/",
            "http://box.lan/",
            "http://example.com:8080/",
            "ftp://example.com/",
            "nightfall://app/newtab.html",
        ] {
            assert_eq!(upgrade(no), None, "{no}");
        }
    }

    #[test]
    fn navigation_flow() {
        // Disabled: nothing happens.
        assert_eq!(on_navigation(1, "http://site-a.fr/", false), Action::Continue);
        // Upgrade, then the https page loads.
        assert_eq!(on_navigation(1, "http://site-a.fr/", true), Action::Upgrade("https://site-a.fr/".into()));
        done(1);
        // Upgrade, then the https attempt fails: warn with the original URL.
        assert_eq!(on_navigation(2, "http://site-b.fr/page", true), Action::Upgrade("https://site-b.fr/page".into()));
        assert_eq!(on_load_error(2, "https://site-b.fr/page", -3), None);
        assert_eq!(on_load_error(2, "https://other.fr/", -105), None);
        assert_eq!(on_load_error(2, "https://site-b.fr/page", -105).as_deref(), Some("http://site-b.fr/page"));
        assert_eq!(on_load_error(2, "https://site-b.fr/page", -105), None);
        // The https site redirects back to http: warn instead of looping.
        assert_eq!(on_navigation(3, "http://site-c.fr/", true), Action::Upgrade("https://site-c.fr/".into()));
        assert_eq!(on_navigation(3, "http://site-c.fr/", true), Action::Warn("http://site-c.fr/".into()));
        // "Continuer vers le site": http allowed for the session.
        allow("http://site-c.fr/");
        assert_eq!(on_navigation(3, "http://site-c.fr/autre", true), Action::Continue);
        assert_eq!(on_navigation(3, "http://www.site-c.fr/", true), Action::Continue);
        assert_eq!(on_navigation(4, "http://autre-site-c.fr/", true), Action::Upgrade("https://autre-site-c.fr/".into()));
        assert!(warning_url("http://site-c.fr/?a=1&b").starts_with("nightfall://app/insecure.html#http%3A%2F%2Fsite-c.fr"));
    }
}

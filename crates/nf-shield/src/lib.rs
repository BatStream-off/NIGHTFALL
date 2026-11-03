//! NIGHTFALL Shield.
//!
//! Network and cosmetic filtering with Brave's `adblock` engine (MPL-2.0),
//! which understands EasyList / EasyPrivacy / uBlock Origin syntax.
//! No uBlock Origin code or resource (GPL) is included: filter lists are
//! downloaded by the user's browser from their publishers at runtime.

use adblock::lists::{FilterSet, ParseOptions};
use adblock::request::Request;
use adblock::Engine;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};

#[derive(Debug, thiserror::Error)]
pub enum ShieldError {
    #[error("téléchargement de {0} impossible : {1}")]
    Download(String, String),
    #[error("i/o : {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FilterList {
    pub id: String,
    pub name: String,
    pub url: String,
    pub enabled: bool,
    /// Unix time of the last successful download.
    #[serde(default)]
    pub updated: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShieldConfig {
    pub enabled: bool,
    pub block_popups: bool,
    pub lists: Vec<FilterList>,
    /// User filters in Adblock Plus / uBO syntax, one per line.
    pub custom_filters: String,
    /// Registrable domains on which Shield is turned off.
    pub allowlist: BTreeSet<String>,
}

impl Default for ShieldConfig {
    fn default() -> Self {
        let list = |id: &str, name: &str, url: &str, enabled| FilterList {
            id: id.into(),
            name: name.into(),
            url: url.into(),
            enabled,
            updated: None,
        };
        Self {
            enabled: true,
            block_popups: true,
            lists: vec![
                list("easylist", "EasyList (publicités)", "https://easylist.to/easylist/easylist.txt", true),
                list("easyprivacy", "EasyPrivacy (traqueurs)", "https://easylist.to/easylist/easyprivacy.txt", true),
                list("liste_fr", "Liste FR (sites francophones)", "https://easylist-downloads.adblockplus.org/liste_fr.txt", true),
                list("fanboy_annoyance", "Fanboy Annoyances (bandeaux, pop-ups)", "https://secure.fanboy.co.nz/fanboy-annoyance.txt", false),
            ],
            custom_filters: String::new(),
            allowlist: BTreeSet::new(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ShieldStats {
    pub total_blocked: u64,
    pub popups_blocked: u64,
    pub per_site: BTreeMap<String, u64>,
}

/// CSS and script to inject into a page for cosmetic filtering.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Cosmetic {
    /// `selector,... { display: none !important }` rules.
    pub css: String,
    /// Selectors kept separately for the generic class/id pass.
    pub generichide: bool,
    pub exceptions: Vec<String>,
}

pub struct Shield {
    cache_dir: PathBuf,
    engine: RwLock<Arc<Engine>>,
    config: RwLock<ShieldConfig>,
    stats: Mutex<ShieldStats>,
    rules_loaded: Mutex<usize>,
}

/// Registrable domain of a URL ("" when unparsable).
pub fn site_of(url_str: &str) -> String {
    url::Url::parse(url_str)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_ascii_lowercase()))
        .map(|h| psl::domain_str(&h).map(str::to_string).unwrap_or(h))
        .unwrap_or_default()
}

/// Map a CEF `cef_resource_type_t` value to adblock's request type string.
pub fn request_type_from_cef(resource_type: i32) -> &'static str {
    match resource_type {
        0 => "main_frame",
        1 => "sub_frame",
        2 => "stylesheet",
        3 => "script",
        4 => "image",
        5 => "font",
        6 => "other",      // sub-resource
        7 => "object",
        8 => "media",
        9 | 10 | 11 => "script", // workers
        12 => "other",     // prefetch
        13 => "image",     // favicon
        14 => "xmlhttprequest",
        15 => "ping",
        16 => "script",    // service worker
        17 => "other",     // CSP report
        18 => "other",     // plugin resource
        19 => "main_frame", // navigation preload main frame
        20 => "sub_frame",
        _ => "other",
    }
}

impl Shield {
    pub fn new(cache_dir: impl Into<PathBuf>, config: ShieldConfig) -> Self {
        let s = Self {
            cache_dir: cache_dir.into(),
            engine: RwLock::new(Arc::new(Engine::default())),
            config: RwLock::new(config),
            stats: Mutex::new(ShieldStats::default()),
            rules_loaded: Mutex::new(0),
        };
        s.rebuild();
        s
    }

    fn list_path(&self, id: &str) -> PathBuf {
        let safe: String = id.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
        self.cache_dir.join(format!("{safe}.txt"))
    }

    /// Rebuild the engine from cached lists + custom filters.
    pub fn rebuild(&self) {
        let cfg = self.config();
        let mut set = FilterSet::new(false);
        let mut count = 0;
        for l in cfg.lists.iter().filter(|l| l.enabled) {
            if let Ok(text) = std::fs::read_to_string(self.list_path(&l.id)) {
                count += text.lines().filter(|x| !x.trim().is_empty() && !x.starts_with('!')).count();
                set.add_filter_list(text, ParseOptions::default());
            }
        }
        if !cfg.custom_filters.trim().is_empty() {
            count += cfg.custom_filters.lines().filter(|x| !x.trim().is_empty()).count();
            set.add_filter_list(cfg.custom_filters.clone(), ParseOptions::default());
        }
        let engine = Engine::new_with_filter_set(set);
        *self.engine.write().unwrap() = Arc::new(engine);
        *self.rules_loaded.lock().unwrap() = count;
    }

    pub fn rules_loaded(&self) -> usize {
        *self.rules_loaded.lock().unwrap()
    }

    pub fn config(&self) -> ShieldConfig {
        self.config.read().unwrap().clone()
    }

    pub fn set_config(&self, cfg: ShieldConfig) {
        *self.config.write().unwrap() = cfg;
        self.rebuild();
    }

    pub fn set_enabled(&self, enabled: bool) {
        self.config.write().unwrap().enabled = enabled;
    }

    /// Turn Shield off (`allowed = true`) or on for one site.
    pub fn set_site_allowed(&self, url_or_domain: &str, allowed: bool) {
        let site = if url_or_domain.contains("://") { site_of(url_or_domain) } else { url_or_domain.to_ascii_lowercase() };
        let mut cfg = self.config.write().unwrap();
        if allowed {
            cfg.allowlist.insert(site);
        } else {
            cfg.allowlist.remove(&site);
        }
    }

    pub fn is_active_for(&self, page_url: &str) -> bool {
        let cfg = self.config.read().unwrap();
        cfg.enabled && !cfg.allowlist.contains(&site_of(page_url))
    }

    /// Decide whether to block a sub-resource. `page_url` is the top-level
    /// document URL (used for the allowlist and statistics).
    pub fn should_block(&self, url: &str, source_url: &str, request_type: &str, page_url: &str) -> bool {
        if request_type == "main_frame" || !self.is_active_for(page_url) {
            return false;
        }
        if !(url.starts_with("http://") || url.starts_with("https://") || url.starts_with("ws")) {
            return false;
        }
        let Ok(req) = Request::new(url, source_url, request_type, "GET") else {
            return false;
        };
        let engine = self.engine.read().unwrap().clone();
        let blocked = engine.check_network_request(&req).should_block();
        if blocked {
            let mut st = self.stats.lock().unwrap();
            st.total_blocked += 1;
            *st.per_site.entry(site_of(page_url)).or_default() += 1;
        }
        blocked
    }

    pub fn record_popup_blocked(&self, page_url: &str) {
        let mut st = self.stats.lock().unwrap();
        st.popups_blocked += 1;
        *st.per_site.entry(site_of(page_url)).or_default() += 1;
    }

    pub fn popups_blocked_enabled(&self, page_url: &str) -> bool {
        self.is_active_for(page_url) && self.config.read().unwrap().block_popups
    }

    /// Cosmetic CSS for a page (element hiding).
    pub fn cosmetic(&self, page_url: &str) -> Option<Cosmetic> {
        if !self.is_active_for(page_url) {
            return None;
        }
        let engine = self.engine.read().unwrap().clone();
        let res = engine.url_cosmetic_resources(page_url);
        let mut selectors: Vec<&String> = res.hide_selectors.iter().collect();
        selectors.sort();
        let mut css = String::new();
        for chunk in selectors.chunks(200) {
            let joined: Vec<&str> = chunk.iter().map(|s| s.as_str()).collect();
            css.push_str(&joined.join(","));
            css.push_str("{display:none!important}\n");
        }
        let mut exceptions: Vec<String> = res.exceptions.into_iter().collect();
        exceptions.sort();
        Some(Cosmetic { css, generichide: res.generichide, exceptions })
    }

    /// Generic class/id based hiding for elements seen in the page.
    pub fn class_id_css(&self, classes: &[String], ids: &[String], exceptions: &[String]) -> String {
        let engine = self.engine.read().unwrap().clone();
        let exc: std::collections::HashSet<String> = exceptions.iter().cloned().collect();
        let sels = engine.hidden_class_id_selectors(classes, ids, &exc);
        if sels.is_empty() {
            String::new()
        } else {
            format!("{}{{display:none!important}}", sels.join(","))
        }
    }

    pub fn stats(&self) -> ShieldStats {
        self.stats.lock().unwrap().clone()
    }

    /// Download enabled lists (blocking; call from a background thread),
    /// then rebuild the engine. Returns per-list errors, if any.
    pub fn update_lists(&self, now: i64) -> Vec<String> {
        let mut errors = Vec::new();
        let lists = self.config().lists;
        let mut updated = Vec::new();
        for l in lists.iter().filter(|l| l.enabled) {
            match download(&l.url) {
                Ok(text) => {
                    if let Err(e) = nf_core::paths::write_private(&self.list_path(&l.id), text.as_bytes()) {
                        errors.push(format!("{}: {e}", l.name));
                    } else {
                        updated.push(l.id.clone());
                    }
                }
                Err(e) => errors.push(e.to_string()),
            }
        }
        {
            let mut cfg = self.config.write().unwrap();
            for l in cfg.lists.iter_mut() {
                if updated.contains(&l.id) {
                    l.updated = Some(now);
                }
            }
        }
        self.rebuild();
        errors
    }

    /// True when at least one enabled list is present on disk.
    pub fn has_cached_lists(&self) -> bool {
        self.config().lists.iter().filter(|l| l.enabled).any(|l| self.list_path(&l.id).exists())
    }

    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }
}

fn download(url: &str) -> Result<String, ShieldError> {
    let resp = ureq::get(url)
        .set("User-Agent", "NIGHTFALL-Shield/0.1 (+https://github.com/BatStream-off/NIGHTFALL)")
        .timeout(std::time::Duration::from_secs(60))
        .call()
        .map_err(|e| ShieldError::Download(url.into(), e.to_string()))?;
    let text = resp.into_string().map_err(|e| ShieldError::Download(url.into(), e.to_string()))?;
    if !text.contains('\n') || text.len() < 100 {
        return Err(ShieldError::Download(url.into(), "liste vide ou invalide".into()));
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shield_with(filters: &str) -> (tempfile::TempDir, Shield) {
        let dir = tempfile::tempdir().unwrap();
        let cfg = ShieldConfig { custom_filters: filters.to_string(), ..Default::default() };
        let s = Shield::new(dir.path(), cfg);
        (dir, s)
    }

    #[test]
    fn blocks_network_requests_and_counts() {
        let (_d, s) = shield_with("||ads.example.com^\n||tracker.test^$third-party\n");
        let page = "https://news.site.test/article";
        assert!(s.should_block("https://ads.example.com/banner.js", page, "script", page));
        assert!(s.should_block("https://tracker.test/pixel.gif", page, "image", page));
        assert!(!s.should_block("https://cdn.site.test/app.js", page, "script", page));
        // First-party request to the tracker itself is allowed by $third-party.
        assert!(!s.should_block("https://tracker.test/p.gif", "https://tracker.test/", "image", "https://tracker.test/"));
        // Top-level navigations are never blocked by Shield.
        assert!(!s.should_block("https://ads.example.com/", page, "main_frame", page));
        let st = s.stats();
        assert_eq!(st.total_blocked, 2);
        assert_eq!(st.per_site.get("site.test"), Some(&2));
    }

    #[test]
    fn allowlist_and_global_switch() {
        let (_d, s) = shield_with("||ads.example.com^\n");
        let page = "https://shop.example.org/";
        s.set_site_allowed(page, true);
        assert!(!s.should_block("https://ads.example.com/x.js", page, "script", page));
        s.set_site_allowed("example.org", false);
        assert!(s.should_block("https://ads.example.com/x.js", page, "script", page));
        s.set_enabled(false);
        assert!(!s.should_block("https://ads.example.com/x.js", page, "script", page));
    }

    #[test]
    fn exception_rules() {
        let (_d, s) = shield_with("||cdn.example.com^\n@@||cdn.example.com/jquery.js\n");
        let page = "https://a.test/";
        assert!(s.should_block("https://cdn.example.com/ads.js", page, "script", page));
        assert!(!s.should_block("https://cdn.example.com/jquery.js", page, "script", page));
    }

    #[test]
    fn cosmetic_css() {
        let (_d, s) = shield_with("news.test##.ad-banner\n##.sponsored-box\n");
        let c = s.cosmetic("https://news.test/").unwrap();
        assert!(c.css.contains(".ad-banner"), "{}", c.css);
        assert!(c.css.contains("display:none!important"));
        let generic = s.class_id_css(&["sponsored-box".into(), "content".into()], &[], &c.exceptions);
        assert!(generic.contains(".sponsored-box"), "{generic}");
        assert!(!generic.contains(".content"));
    }

    #[test]
    fn cached_lists_are_loaded() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("easylist.txt"), "! comment\n||doubleclick.test^\n").unwrap();
        let s = Shield::new(dir.path(), ShieldConfig::default());
        assert!(s.has_cached_lists());
        assert_eq!(s.rules_loaded(), 1);
        let page = "https://a.test/";
        assert!(s.should_block("https://ad.doubleclick.test/x", page, "script", page));
    }

    #[test]
    fn resource_type_mapping() {
        assert_eq!(request_type_from_cef(3), "script");
        assert_eq!(request_type_from_cef(14), "xmlhttprequest");
        assert_eq!(request_type_from_cef(999), "other");
    }
}

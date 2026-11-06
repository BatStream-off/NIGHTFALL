//! NIGHTFALL Shield.
//!
//! Network and cosmetic filtering with Brave's `adblock` engine (MPL-2.0),
//! which understands EasyList / EasyPrivacy / uBlock Origin syntax.
//! No uBlock Origin code or resource (GPL) is included: filter lists are
//! downloaded by the user's browser from their publishers at runtime, and
//! the scriptlets used by `##+js(...)` rules are NIGHTFALL's own
//! implementations (see `scriptlets/`).

pub mod redirects;
pub mod scriptlets;

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
    /// Lists maintained by the uBlock Origin project may use "trusted"
    /// scriptlets (uBO grants this to its own lists only).
    #[serde(default)]
    pub trusted: bool,
    /// SHA-256 of the cached list as written by NIGHTFALL (kept in the
    /// encrypted settings): a cached file changed by anything else is
    /// refused instead of being loaded, trusted scriptlets included.
    #[serde(default)]
    pub sha256: Option<String>,
}

/// Saved encrypted in the profile: a field missing from an older (or newer)
/// saved copy takes its default value, the others are kept.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
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
        Self {
            enabled: true,
            block_popups: true,
            lists: default_lists(),
            custom_filters: String::new(),
            allowlist: BTreeSet::new(),
        }
    }
}

fn default_lists() -> Vec<FilterList> {
    let list = |id: &str, name: &str, url: &str, enabled, trusted| FilterList {
        id: id.into(),
        name: name.into(),
        url: url.into(),
        enabled,
        updated: None,
        trusted,
        sha256: None,
    };
    const UBO: &str = "https://ublockorigin.github.io/uAssets/filters/";
    vec![
        list("easylist", "EasyList (publicités)", "https://easylist.to/easylist/easylist.txt", true, false),
        list("easyprivacy", "EasyPrivacy (traqueurs)", "https://easylist.to/easylist/easyprivacy.txt", true, false),
        list("liste_fr", "Liste FR (sites francophones)", "https://easylist-downloads.adblockplus.org/liste_fr.txt", true, false),
        list("ubo_filters", "uBlock filters (publicités, YouTube)", &format!("{UBO}filters.txt"), true, true),
        list("ubo_quick_fixes", "uBlock filters – Correctifs rapides", &format!("{UBO}quick-fixes.txt"), true, true),
        list("ubo_privacy", "uBlock filters – Vie privée", &format!("{UBO}privacy.txt"), true, true),
        list("ubo_unbreak", "uBlock filters – Réparations", &format!("{UBO}unbreak.txt"), true, true),
        list("ubo_badware", "uBlock filters – Sites malveillants", &format!("{UBO}badware.txt"), true, true),
        list("fanboy_annoyance", "Fanboy Annoyances (bandeaux, pop-ups)", "https://secure.fanboy.co.nz/fanboy-annoyance.txt", false, false),
    ]
}

impl ShieldConfig {
    /// Add lists introduced by a newer NIGHTFALL version to a saved
    /// configuration, keeping the user's own choices for existing lists.
    pub fn with_new_default_lists(mut self) -> Self {
        for d in default_lists() {
            match self.lists.iter_mut().find(|l| l.id == d.id) {
                Some(l) => l.trusted = d.trusted,
                None => self.lists.push(d),
            }
        }
        self
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

const COSMETIC_RUNTIME: &str = include_str!("cosmetic.js");

pub struct Shield {
    cache_dir: PathBuf,
    engine: RwLock<Arc<Engine>>,
    config: RwLock<ShieldConfig>,
    stats: Mutex<ShieldStats>,
    rules_loaded: Mutex<usize>,
    /// One engine build at a time, in call order (the last configuration wins).
    build: Mutex<()>,
    /// Cached lists refused at the last build (changed outside NIGHTFALL).
    integrity_errors: Mutex<Vec<String>>,
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
/// What to do with a sub-resource request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Allow,
    Block,
    /// Blocked, and answered with this neutral content instead of an error.
    Redirect { mime: String, body: Vec<u8> },
}

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
        let s = Self::new_unbuilt(cache_dir, config);
        s.rebuild();
        s
    }

    /// Shield with an empty engine: nothing is read from disk. NIGHTFALL
    /// builds the engine after unlock, in the background (no page can load
    /// before unlock), which keeps the start-up fast and never loads the
    /// cached lists before the settings holding their fingerprints are open.
    pub fn new_unbuilt(cache_dir: impl Into<PathBuf>, config: ShieldConfig) -> Self {
        Self {
            cache_dir: cache_dir.into(),
            engine: RwLock::new(Arc::new(Engine::default())),
            config: RwLock::new(config),
            stats: Mutex::new(ShieldStats::default()),
            rules_loaded: Mutex::new(0),
            build: Mutex::new(()),
            integrity_errors: Mutex::new(Vec::new()),
        }
    }

    /// Cached lists refused at the last build because their content no
    /// longer matches the fingerprint recorded at download.
    pub fn integrity_errors(&self) -> Vec<String> {
        self.integrity_errors.lock().map(|e| e.clone()).unwrap_or_default()
    }

    fn list_path(&self, id: &str) -> PathBuf {
        let safe: String = id.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
        self.cache_dir.join(format!("{safe}.txt"))
    }

    /// Rebuild the engine from cached lists + custom filters.
    pub fn rebuild(&self) {
        let _one_at_a_time = self.build.lock().unwrap_or_else(|e| e.into_inner());
        let cfg = self.config();
        let mut set = FilterSet::new(false);
        let mut count = 0;
        let mut refused = Vec::new();
        let mut first_seen: Vec<(String, String)> = Vec::new();
        for l in cfg.lists.iter().filter(|l| l.enabled) {
            if let Ok(text) = std::fs::read_to_string(self.list_path(&l.id)) {
                let digest = sha256_hex(text.as_bytes());
                match &l.sha256 {
                    Some(expected) if *expected != digest => {
                        refused.push(format!("{} : contenu modifié hors de NIGHTFALL, liste ignorée jusqu'au prochain téléchargement", l.name));
                        continue;
                    }
                    // Cached before fingerprints existed: recorded now.
                    None => first_seen.push((l.id.clone(), digest)),
                    _ => {}
                }
                count += text.lines().filter(|x| !x.trim().is_empty() && !x.starts_with('!')).count();
                let mut opts = ParseOptions::default();
                if l.trusted {
                    opts.permissions = scriptlets::TRUSTED;
                }
                set.add_filter_list(text, opts);
            }
        }
        if !cfg.custom_filters.trim().is_empty() {
            count += cfg.custom_filters.lines().filter(|x| !x.trim().is_empty()).count();
            set.add_filter_list(cfg.custom_filters.clone(), ParseOptions::default());
        }
        let mut engine = Engine::new_with_filter_set(set);
        let mut res = scriptlets::resources();
        res.extend(redirects::resources());
        engine.use_resources(res);
        *self.engine.write().unwrap() = Arc::new(engine);
        *self.rules_loaded.lock().unwrap() = count;
        if !first_seen.is_empty() {
            let mut c = self.config.write().unwrap();
            for (id, digest) in first_seen {
                if let Some(l) = c.lists.iter_mut().find(|l| l.id == id && l.sha256.is_none()) {
                    l.sha256 = Some(digest);
                }
            }
        }
        if let Ok(mut e) = self.integrity_errors.lock() {
            *e = refused;
        }
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

    /// Replace the configuration without building the engine; the caller
    /// then calls `rebuild` (from a background thread).
    pub fn set_config_later(&self, cfg: ShieldConfig) {
        *self.config.write().unwrap() = cfg;
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
        !matches!(self.decide(url, source_url, request_type, page_url), Decision::Allow)
    }

    /// Like `should_block`, with the neutral replacement to serve when the
    /// matching filter asks for one (`$redirect`, `$redirect-rule`).
    pub fn decide(&self, url: &str, source_url: &str, request_type: &str, page_url: &str) -> Decision {
        if request_type == "main_frame" || !self.is_active_for(page_url) {
            return Decision::Allow;
        }
        if !(url.starts_with("http://") || url.starts_with("https://") || url.starts_with("ws")) {
            return Decision::Allow;
        }
        let Ok(req) = Request::new(url, source_url, request_type, "GET") else {
            return Decision::Allow;
        };
        let engine = self.engine.read().unwrap().clone();
        let result = engine.check_network_request(&req);
        if !result.should_block() {
            return Decision::Allow;
        }
        {
            let mut st = self.stats.lock().unwrap();
            st.total_blocked += 1;
            *st.per_site.entry(site_of(page_url)).or_default() += 1;
        }
        match result.redirect.as_deref().and_then(redirects::decode_data_url) {
            Some((mime, body)) => Decision::Redirect { mime, body },
            None => Decision::Block,
        }
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
        for chunk in selectors.chunks(20) {
            let joined: Vec<&str> = chunk.iter().map(|s| s.as_str()).collect();
            css.push_str(&joined.join(","));
            css.push_str("{display:none!important}\n");
        }
        let mut exceptions: Vec<String> = res.exceptions.into_iter().collect();
        exceptions.sort();
        Some(Cosmetic { css, generichide: res.generichide, exceptions })
    }

    /// Script injected at document start: scriptlets (`##+js(...)`), CSS
    /// hiding rules, procedural filters, and the generic class/id observer.
    /// `token` prefixes the page's reports of new classes and ids.
    pub fn page_script(&self, page_url: &str, token: &str) -> Option<String> {
        if !self.is_active_for(page_url) {
            return None;
        }
        let engine = self.engine.read().unwrap().clone();
        let res = engine.url_cosmetic_resources(page_url);
        let mut selectors: Vec<&String> = res.hide_selectors.iter().collect();
        selectors.sort();
        let mut css = String::new();
        for chunk in selectors.chunks(20) {
            let joined: Vec<&str> = chunk.iter().map(|s| s.as_str()).collect();
            css.push_str(&joined.join(","));
            css.push_str("{display:none!important}\n");
        }
        let mut procedural: Vec<&String> = res.procedural_actions.iter().collect();
        procedural.sort();
        if css.is_empty() && procedural.is_empty() && res.injected_script.is_empty() && res.generichide {
            return None;
        }
        let data = serde_json::json!({
            "css": css,
            "procedural": procedural,
            "generic": !res.generichide,
            "token": token,
        });
        let mut script = String::new();
        if !res.injected_script.is_empty() {
            // Scriptlets first: they must run before the page's own scripts.
            script.push_str("(function(){\n");
            script.push_str(&res.injected_script);
            script.push_str("\n})();\n");
        }
        script.push_str(COSMETIC_RUNTIME);
        script.push('(');
        script.push_str(&data.to_string());
        script.push_str(");\n");
        Some(script)
    }

    /// CSS for generic class/id rules matching what the page reported.
    pub fn generic_css(&self, page_url: &str, classes: &[String], ids: &[String]) -> String {
        if !self.is_active_for(page_url) {
            return String::new();
        }
        let engine = self.engine.read().unwrap().clone();
        let res = engine.url_cosmetic_resources(page_url);
        if res.generichide {
            return String::new();
        }
        let sels = engine.hidden_class_id_selectors(classes, ids, &res.exceptions);
        if sels.is_empty() {
            String::new()
        } else {
            format!("{}{{display:none!important}}", sels.join(","))
        }
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
            match download(&l.url).map(|text| preprocess(&text, &l.url, &mut |u: &str| download(u))) {
                Ok(text) => {
                    if let Err(e) = nf_core::paths::write_private(&self.list_path(&l.id), text.as_bytes()) {
                        errors.push(format!("{}: {e}", l.name));
                    } else {
                        updated.push((l.id.clone(), sha256_hex(text.as_bytes())));
                    }
                }
                Err(e) => errors.push(e.to_string()),
            }
        }
        {
            let mut cfg = self.config.write().unwrap();
            for l in cfg.lists.iter_mut() {
                if let Some((_, digest)) = updated.iter().find(|(id, _)| *id == l.id) {
                    l.updated = Some(now);
                    l.sha256 = Some(digest.clone());
                }
            }
        }
        self.rebuild();
        errors
    }

    /// True when an enabled list is missing on disk or older than `max_age`
    /// seconds (uBlock's quick fixes change several times a day).
    pub fn needs_update(&self, now: i64, max_age: i64) -> bool {
        self.config().lists.iter().filter(|l| l.enabled).any(|l| {
            !self.list_path(&l.id).exists() || l.updated.map(|u| now - u > max_age).unwrap_or(true)
        })
    }

    /// True when at least one enabled list is present on disk.
    pub fn has_cached_lists(&self) -> bool {
        self.config().lists.iter().filter(|l| l.enabled).any(|l| self.list_path(&l.id).exists())
    }

    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }
}

/// Largest list accepted (the biggest well-known lists are about 14 MB;
/// `ureq`'s own `into_string` stops at 10 MiB).
const MAX_LIST_BYTES: u64 = 32 * 1024 * 1024;

fn download(url: &str) -> Result<String, ShieldError> {
    use std::io::Read;
    let resp = ureq::get(url)
        .set("User-Agent", "NIGHTFALL-Shield/0.1 (+https://github.com/BatStream-off/NIGHTFALL)")
        .timeout(std::time::Duration::from_secs(60))
        .call()
        .map_err(|e| ShieldError::Download(url.into(), e.to_string()))?;
    let mut bytes = Vec::new();
    resp.into_reader()
        .take(MAX_LIST_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| ShieldError::Download(url.into(), e.to_string()))?;
    if bytes.len() as u64 > MAX_LIST_BYTES {
        return Err(ShieldError::Download(url.into(), "liste trop grande (plus de 32 Mio)".into()));
    }
    let text = String::from_utf8(bytes).map_err(|_| ShieldError::Download(url.into(), "liste illisible (pas du texte UTF-8)".into()))?;
    if !looks_like_filter_list(&text) {
        return Err(ShieldError::Download(url.into(), "liste vide ou invalide".into()));
    }
    Ok(text)
}

/// A filter list has at least a few lines that are neither empty nor comments.
fn looks_like_filter_list(text: &str) -> bool {
    text.len() >= 100 && text.lines().filter(|l| { let t = l.trim(); !t.is_empty() && !t.starts_with('!') && !t.starts_with('[') }).take(3).count() >= 3
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

/// uBlock Origin pre-parsing directives, evaluated as uBO does in a
/// Chromium-based browser: `!#if` / `!#else` / `!#endif` blocks, and
/// `!#include file` (same directory as the list, only in active blocks,
/// not nested). Lines of inactive blocks are dropped.
pub fn preprocess(text: &str, list_url: &str, fetch: &mut dyn FnMut(&str) -> Result<String, ShieldError>) -> String {
    preprocess_inner(text, list_url, fetch, 0)
}

fn preprocess_inner(text: &str, list_url: &str, fetch: &mut dyn FnMut(&str) -> Result<String, ShieldError>, depth: u8) -> String {
    let mut out = String::with_capacity(text.len());
    // Each open `!#if`: (active before the block, condition value, in `!#else`).
    let mut stack: Vec<(bool, bool, bool)> = Vec::new();
    let mut active = true;
    for line in text.lines() {
        let t = line.trim_end();
        if let Some(cond) = t.strip_prefix("!#if ") {
            let v = eval_condition(cond.trim());
            stack.push((active, v, false));
            active = active && v;
            continue;
        }
        if t == "!#else" {
            if let Some(top) = stack.last_mut() {
                top.2 = true;
                active = top.0 && !top.1;
            }
            continue;
        }
        if t == "!#endif" {
            if let Some((before, _, _)) = stack.pop() {
                active = before;
            }
            continue;
        }
        if !active {
            continue;
        }
        if let Some(file) = t.strip_prefix("!#include ") {
            let file = file.trim();
            if depth == 0 && is_safe_include(file) {
                if let Some(url) = sibling_url(list_url, file) {
                    if let Ok(sub) = fetch(&url) {
                        let sub = preprocess_inner(&sub, &url, fetch, depth + 1);
                        out.push_str(&sub);
                        if !sub.is_empty() && !sub.ends_with('\n') {
                            out.push('\n');
                        }
                    }
                }
            }
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// Only a plain file name or sub-path in the list's own directory.
fn is_safe_include(file: &str) -> bool {
    !file.is_empty() && !file.contains("..") && !file.contains("://") && !file.starts_with('/') && !file.contains('\\')
}

fn sibling_url(list_url: &str, file: &str) -> Option<String> {
    let base = url::Url::parse(list_url).ok()?;
    let u = base.join(file).ok()?;
    (u.scheme() == "https" && u.host_str() == base.host_str()).then(|| u.to_string())
}

/// Tokens true for NIGHTFALL (Chromium engine, user style sheets injected).
fn condition_token(tok: &str) -> bool {
    matches!(tok, "true" | "env_chromium" | "cap_user_stylesheet")
}

/// `!#if` expression: tokens, `!`, `&&`, `||` and parentheses.
fn eval_condition(expr: &str) -> bool {
    let toks: Vec<String> = {
        let mut v = Vec::new();
        let mut cur = String::new();
        let mut chars = expr.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                '(' | ')' | '!' => {
                    if !cur.is_empty() { v.push(std::mem::take(&mut cur)); }
                    v.push(c.to_string());
                }
                '&' | '|' => {
                    if !cur.is_empty() { v.push(std::mem::take(&mut cur)); }
                    if chars.peek() == Some(&c) { chars.next(); }
                    v.push(format!("{c}{c}"));
                }
                c if c.is_whitespace() => {
                    if !cur.is_empty() { v.push(std::mem::take(&mut cur)); }
                }
                c => cur.push(c),
            }
        }
        if !cur.is_empty() { v.push(cur); }
        v
    };
    let mut pos = 0;
    let r = parse_or(&toks, &mut pos);
    // Malformed expression: inactive (uBO behaves the same way).
    if pos != toks.len() { false } else { r.unwrap_or(false) }
}

fn parse_or(t: &[String], p: &mut usize) -> Option<bool> {
    let mut v = parse_and(t, p)?;
    while t.get(*p).map(String::as_str) == Some("||") {
        *p += 1;
        let r = parse_and(t, p)?;
        v = v || r;
    }
    Some(v)
}

fn parse_and(t: &[String], p: &mut usize) -> Option<bool> {
    let mut v = parse_not(t, p)?;
    while t.get(*p).map(String::as_str) == Some("&&") {
        *p += 1;
        let r = parse_not(t, p)?;
        v = v && r;
    }
    Some(v)
}

fn parse_not(t: &[String], p: &mut usize) -> Option<bool> {
    match t.get(*p).map(String::as_str) {
        Some("!") => {
            *p += 1;
            parse_not(t, p).map(|v| !v)
        }
        Some("(") => {
            *p += 1;
            let v = parse_or(t, p)?;
            if t.get(*p).map(String::as_str) != Some(")") {
                return None;
            }
            *p += 1;
            Some(v)
        }
        Some(tok) if tok != ")" && tok != "&&" && tok != "||" => {
            *p += 1;
            Some(condition_token(tok))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preprocess_if_else_and_include() {
        let text = "a\n!#if env_firefox\nfirefox-only\n!#include ff.txt\n!#else\nnot-firefox\n!#endif\n!#if !env_safari && (env_chromium || env_edge)\nchromium\n!#include sub.txt\n!#endif\n!#include ../evil.txt\nz\n";
        let mut asked = Vec::new();
        let mut fetch = |u: &str| -> Result<String, ShieldError> {
            asked.push(u.to_string());
            Ok("included\n!#if env_mobile\nmobile\n!#endif\n!#include nested.txt\n".to_string())
        };
        let out = preprocess(text, "https://lists.test/filters/main.txt", &mut fetch);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines, vec!["a", "not-firefox", "chromium", "included", "z"]);
        // Only the include of the active block, never `..`, never nested.
        assert_eq!(asked, vec!["https://lists.test/filters/sub.txt".to_string()]);
    }

    #[test]
    fn condition_expressions() {
        assert!(eval_condition("env_chromium"));
        assert!(!eval_condition("env_firefox"));
        assert!(eval_condition("!env_mobile"));
        assert!(eval_condition("env_firefox || env_chromium"));
        assert!(!eval_condition("env_chromium && env_mv3"));
        assert!(eval_condition("!(env_safari || env_mobile) && cap_user_stylesheet"));
        assert!(!eval_condition("(env_chromium"));
        assert!(!eval_condition("unknown_token"));
    }

    #[test]
    fn cached_list_changed_outside_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let lists = vec![FilterList { id: "l1".into(), name: "L1".into(), url: "https://x.test/l1.txt".into(), enabled: true, updated: None, trusted: false, sha256: None }];
        let cfg = ShieldConfig { lists, ..Default::default() };
        std::fs::write(dir.path().join("l1.txt"), "||ads.test^\n").unwrap();
        let s = Shield::new(dir.path(), cfg);
        let page = "https://site.test/";
        assert!(s.should_block("https://ads.test/a.js", page, "script", page));
        // First build records the fingerprint.
        let recorded = s.config().lists[0].sha256.clone();
        assert_eq!(recorded, Some(sha256_hex(b"||ads.test^\n")));
        // Someone rewrites the cached file: refused at the next build.
        std::fs::write(dir.path().join("l1.txt"), "||other.test^\n").unwrap();
        s.rebuild();
        assert!(!s.should_block("https://other.test/a.js", page, "script", page));
        assert!(!s.should_block("https://ads.test/a.js", page, "script", page));
        assert_eq!(s.integrity_errors().len(), 1);
    }

    #[test]
    fn unbuilt_shield_reads_nothing_until_rebuild() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = ShieldConfig { custom_filters: "||ads.test^\n".into(), ..Default::default() };
        let s = Shield::new_unbuilt(dir.path(), cfg);
        let page = "https://site.test/";
        assert!(!s.should_block("https://ads.test/a.js", page, "script", page));
        s.rebuild();
        assert!(s.should_block("https://ads.test/a.js", page, "script", page));
    }

    #[test]
    fn filter_list_validation() {
        assert!(!looks_like_filter_list("<html>\n</html>"));
        assert!(!looks_like_filter_list("! only comments\n! here\n"));
        assert!(looks_like_filter_list(&format!("! Title\n{}", "||a.test^\n||b.test^\n||c.test^\n".repeat(10))));
    }

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
    fn redirect_filters_get_a_neutral_answer() {
        let (_d, s) = shield_with("||ads.example^$script,redirect=noop.js\n||pix.example^$image,redirect=1x1.gif\n||plain.example^\n||rr.example^$redirect-rule=noop.txt\n");
        let page = "https://a.test/";
        match s.decide("https://ads.example/x.js", page, "script", page) {
            Decision::Redirect { mime, body } => {
                assert_eq!(mime, "application/javascript");
                assert_eq!(body, b"(function () {})();\n");
            }
            other => panic!("redirection attendue : {other:?}"),
        }
        assert!(matches!(s.decide("https://pix.example/p.gif", page, "image", page), Decision::Redirect { ref mime, .. } if mime == "image/gif"));
        assert_eq!(s.decide("https://plain.example/a.js", page, "script", page), Decision::Block);
        // redirect-rule alone does not block.
        assert_eq!(s.decide("https://rr.example/a.txt", page, "script", page), Decision::Allow);
        assert_eq!(s.decide("https://other.example/a.js", page, "script", page), Decision::Allow);
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

    #[test]
    fn page_script_with_scriptlets_and_procedural_rules() {
        let (_d, s) = shield_with(
            "example.com##+js(set-constant, nfShieldTest, 42)\nexample.com##+js(json-prune, playerAds adPlacements)\nexample.com##p:has-text(documentation)\nexample.com##.pub\n",
        );
        let js = s.page_script("https://example.com/", "TOKEN:").expect("script");
        assert!(js.contains("function nfSetConstant"), "{js}");
        assert!(js.contains("nfSetConstant(\"nfShieldTest\", \"42\")"), "{js}");
        assert!(js.contains("function nfJsonPrune"));
        assert!(js.contains("function nfUtils"));
        assert!(js.contains("has-text"));
        assert!(js.contains(".pub{display:none!important}"));
        assert!(js.contains("TOKEN:"));
        // Other sites get no site-specific scriptlet.
        let other = s.page_script("https://other.test/", "T").unwrap_or_default();
        assert!(!other.contains("nfShieldTest"));
    }

    #[test]
    fn trusted_scriptlets_need_a_trusted_list() {
        let (_d, s) = shield_with("example.com##+js(trusted-replace-fetch-response, ads, x)\n");
        let js = s.page_script("https://example.com/", "T").unwrap_or_default();
        assert!(!js.contains("nfTrustedReplaceFetchResponse("), "custom filters are not trusted");
    }

    #[test]
    fn generic_class_rules() {
        let (_d, s) = shield_with("##.ad-banner\n###sponsor-box\n");
        let css = s.generic_css("https://site.test/", &["ad-banner".into(), "content".into()], &["sponsor-box".into()]);
        assert!(css.contains(".ad-banner"));
        assert!(css.contains("#sponsor-box"));
        assert!(!css.contains("content"));
    }

    #[test]
    fn saved_configuration_gains_new_lists() {
        let old = ShieldConfig { lists: default_lists().into_iter().take(3).collect(), ..Default::default() };
        let cfg = old.with_new_default_lists();
        assert!(cfg.lists.iter().any(|l| l.id == "ubo_filters" && l.trusted && l.enabled));
    }

    #[test]
    fn old_saved_config_with_missing_fields_keeps_the_others() {
        // Saved by 0.1.4 (complete at the time).
        let json = r#"{"enabled":true,"block_popups":true,"lists":[{"id":"easylist","name":"EasyList","url":"https://easylist.to/easylist/easylist.txt","enabled":false}],"custom_filters":"x","allowlist":["a.fr"]}"#;
        let cfg: ShieldConfig = serde_json::from_str(json).unwrap();
        assert!(cfg.enabled && cfg.block_popups);
        assert_eq!(cfg.custom_filters, "x");
        assert_eq!(cfg.allowlist.iter().collect::<Vec<_>>(), ["a.fr"]);
        assert_eq!(cfg.lists.len(), 1);
        assert!(!cfg.lists[0].enabled && cfg.lists[0].updated.is_none() && !cfg.lists[0].trusted);
        // Fields missing (older or newer copy): defaults for those only.
        let json = r#"{"enabled":false,"custom_filters":"||ads.example^","allowlist":["b.fr"]}"#;
        let cfg: ShieldConfig = serde_json::from_str(json).unwrap();
        assert!(!cfg.enabled);
        assert!(cfg.block_popups);
        assert_eq!(cfg.custom_filters, "||ads.example^");
        assert!(cfg.allowlist.contains("b.fr"));
        assert_eq!(cfg.lists, default_lists());
        // Unknown fields (written by a newer version) are ignored.
        let cfg: ShieldConfig = serde_json::from_str(r#"{"enabled":false,"site_overrides":{}}"#).unwrap();
        assert!(!cfg.enabled && cfg.block_popups);
        assert_eq!(serde_json::from_str::<ShieldConfig>("{}").unwrap(), ShieldConfig::default());
    }
}

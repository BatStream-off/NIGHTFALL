//! Process-wide state of the browser process.
//!
//! - [`Core`] (behind a mutex) holds everything that is `Send`: lock state,
//!   unlocked keys, database, vault, settings. It is reachable from CEF's IO
//!   thread (scheme handler) and UI thread.
//! - CEF view objects live on the UI thread only (see `windows.rs`).

use nf_auth::NightfallLock;
use nf_core::ProfilePaths;
use nf_crypto::SecretKey;
use nf_emergency::{AutoPurgeRules, Signal};
use nf_shield::Shield;
use nf_storage::Storage;
use nf_vault::Vault;
use nf_videoguard::VideoGuardConfig;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

/// Preferences readable before unlock (not sensitive).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UiPrefs {
    pub splash_enabled: bool,
    /// GPU acceleration; read before Chromium starts (applied at next start).
    pub hardware_acceleration: bool,
}

impl Default for UiPrefs {
    fn default() -> Self {
        Self { splash_enabled: true, hardware_acceleration: true }
    }
}

/// User settings, stored in the encrypted database.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// An id of [`SEARCH_ENGINES`] or `custom`.
    pub search_engine: String,
    /// Custom search engine (search_engine = "custom"): name and https URL
    /// where `%s` is replaced by the query.
    pub search_custom_name: String,
    pub search_custom_url: String,
    pub vertical_tabs: bool,
    /// 0 = never.
    pub auto_lock_minutes: u32,
    pub lock_on_sleep: bool,
    pub session_vault: bool,
    pub restore_session: bool,
    pub devtools_enabled: bool,
    pub phishing_warnings: bool,
    pub wallpaper: String,
    pub shortcuts: Vec<Shortcut>,
    pub perf: nf_perf::PerfConfig,
    pub auto_purge: AutoPurgeRules,
    pub sync_folder: Option<String>,

    // ---- start-up
    /// "" (older profiles: follows `restore_session`) | newtab | restore | pages
    pub startup_mode: String,
    pub startup_pages: Vec<String>,

    // ---- appearance
    pub show_home_button: bool,
    /// Default page zoom, in percent (applied by Chromium to every page
    /// without a zoom of its own).
    pub default_zoom: u32,
    /// Default font size in CSS pixels (Chromium: 16).
    pub font_size: u32,

    // ---- privacy
    pub block_third_party_cookies: bool,
    /// Send `Sec-GPC: 1` and expose `navigator.globalPrivacyControl`.
    pub global_privacy_control: bool,
    /// Upgrade http:// navigations to https:// and warn before an
    /// insecure site.
    pub https_first: bool,
    /// WebRTC: only the public interface (no local IP address leak).
    pub webrtc_ip_protection: bool,
    /// system | off | cloudflare | quad9 | mullvad | nextdns | custom
    pub secure_dns: String,
    /// NextDNS configuration id, or custom DoH template URL.
    pub secure_dns_custom: String,

    // ---- site permissions
    /// Permission kind id -> "ask" | "block" (missing = ask).
    pub permission_defaults: BTreeMap<String, String>,
    /// Decisions the user asked NIGHTFALL to remember.
    pub site_permissions: Vec<SitePermission>,
    /// Answers given without "Mémoriser": Chromium keeps them, NIGHTFALL
    /// resets them at the next lock or start.
    pub permission_resets: Vec<SitePermission>,

    // ---- downloads
    /// None = the user's Downloads folder.
    pub download_dir: Option<String>,
    /// Ask where to save each file (system "Save as" dialog).
    pub download_ask: bool,

    // ---- languages
    /// Accept-Language list, preferred first (e.g. "fr-FR,fr,en-US,en").
    pub accept_languages: String,
    /// Spell checking (macOS only: system spell checker).
    pub spellcheck: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SitePermission {
    /// Normalised origin, e.g. "https://example.com".
    pub origin: String,
    /// Permission kind id (see `permissions::KINDS`).
    pub kind: String,
    pub allow: bool,
    pub updated: i64,
}

/// Search engines offered in the settings: id, name, URL with `%s`.
pub const SEARCH_ENGINES: &[(&str, &str, &str)] = &[
    ("duckduckgo", "DuckDuckGo", "https://duckduckgo.com/?q=%s"),
    ("brave", "Brave Search", "https://search.brave.com/search?q=%s"),
    ("qwant", "Qwant", "https://www.qwant.com/?q=%s"),
    ("startpage", "Startpage", "https://www.startpage.com/do/search?q=%s"),
    ("ecosia", "Ecosia", "https://www.ecosia.org/search?q=%s"),
    ("google", "Google", "https://www.google.com/search?q=%s"),
    ("bing", "Bing", "https://www.bing.com/search?q=%s"),
];

pub const DEFAULT_ACCEPT_LANGUAGES: &str = "fr-FR,fr,en-US,en";
pub const ZOOM_LEVELS: &[u32] = &[50, 67, 75, 80, 90, 100, 110, 125, 150, 175, 200, 250, 300];
pub const FONT_SIZES: &[u32] = &[9, 12, 16, 20, 24];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Startup {
    NewTab,
    Restore,
    Pages,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Shortcut {
    pub title: String,
    pub url: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            search_engine: "duckduckgo".into(),
            search_custom_name: String::new(),
            search_custom_url: String::new(),
            vertical_tabs: false,
            auto_lock_minutes: 15,
            lock_on_sleep: true,
            session_vault: true,
            restore_session: true,
            devtools_enabled: false,
            phishing_warnings: true,
            wallpaper: "city".into(),
            shortcuts: vec![
                Shortcut { title: "YouTube".into(), url: "https://www.youtube.com/".into() },
                Shortcut { title: "Wikipédia".into(), url: "https://fr.wikipedia.org/".into() },
                Shortcut { title: "GitHub".into(), url: "https://github.com/".into() },
            ],
            perf: nf_perf::PerfConfig::default(),
            auto_purge: AutoPurgeRules::default(),
            sync_folder: None,
            startup_mode: String::new(),
            startup_pages: Vec::new(),
            show_home_button: true,
            default_zoom: 100,
            font_size: 16,
            block_third_party_cookies: true,
            global_privacy_control: true,
            https_first: true,
            webrtc_ip_protection: true,
            secure_dns: "system".into(),
            secure_dns_custom: String::new(),
            permission_defaults: BTreeMap::new(),
            site_permissions: Vec::new(),
            permission_resets: Vec::new(),
            download_dir: None,
            download_ask: false,
            accept_languages: DEFAULT_ACCEPT_LANGUAGES.into(),
            spellcheck: true,
        }
    }
}

/// A custom search URL is accepted when it is https and contains `%s`.
pub fn valid_search_template(t: &str) -> bool {
    let t = t.trim();
    t.starts_with("https://") && t.contains("%s") && url::Url::parse(&t.replace("%s", "x")).map(|u| u.host_str().is_some()).unwrap_or(false)
}

/// A language tag such as "fr", "fr-FR", "zh-Hans-CN".
fn valid_language_tag(t: &str) -> bool {
    let mut parts = t.split('-');
    let first = parts.next().unwrap_or("");
    (2..=3).contains(&first.len())
        && first.chars().all(|c| c.is_ascii_alphabetic())
        && parts.all(|p| (2..=8).contains(&p.len()) && p.chars().all(|c| c.is_ascii_alphanumeric()))
}

fn http_url(u: &str) -> Option<String> {
    let parsed = url::Url::parse(u.trim()).ok()?;
    matches!(parsed.scheme(), "http" | "https").then(|| parsed.to_string())
}

impl Settings {
    /// Search URL template of the chosen engine (`%s` = query).
    pub fn search_template(&self) -> String {
        if self.search_engine == "custom" && valid_search_template(&self.search_custom_url) {
            return self.search_custom_url.trim().to_string();
        }
        SEARCH_ENGINES.iter().find(|(id, _, _)| *id == self.search_engine).unwrap_or(&SEARCH_ENGINES[0]).2.to_string()
    }

    /// What to name the engine in the interface: the engine id (named by
    /// omnibox::engine_name), or the custom engine's name.
    pub fn search_engine_label(&self) -> String {
        if self.search_engine == "custom" {
            let n = self.search_custom_name.trim();
            return if n.is_empty() { "Moteur personnalisé".into() } else { n.to_string() };
        }
        self.search_engine.clone()
    }

    pub fn search_url(&self, query: &str) -> String {
        let q: String = url::form_urlencoded::byte_serialize(query.as_bytes()).collect();
        self.search_template().replacen("%s", &q, 1)
    }

    /// What to open after unlock at start-up.
    pub fn startup(&self) -> Startup {
        match self.startup_mode.as_str() {
            "newtab" => Startup::NewTab,
            "restore" => Startup::Restore,
            "pages" if !self.startup_pages.is_empty() => Startup::Pages,
            "pages" => Startup::NewTab,
            _ if self.restore_session => Startup::Restore,
            _ => Startup::NewTab,
        }
    }

    /// Tabs to open at start-up for the "specific pages" mode.
    pub fn startup_tabs(&self) -> Vec<nf_storage::SessionTab> {
        self.startup_pages
            .iter()
            .enumerate()
            .map(|(i, url)| nf_storage::SessionTab { window_index: 0, tab_index: i as i64, url: url.clone(), title: String::new(), pinned: false, active: i == 0 })
            .collect()
    }

    /// DNS over HTTPS: (Chromium mode, templates).
    pub fn doh_config(&self) -> (&'static str, String) {
        let custom = self.secure_dns_custom.trim();
        match self.secure_dns.as_str() {
            "off" => ("off", String::new()),
            "cloudflare" => ("secure", "https://cloudflare-dns.com/dns-query".into()),
            "quad9" => ("secure", "https://dns.quad9.net/dns-query".into()),
            "mullvad" => ("secure", "https://dns.mullvad.net/dns-query".into()),
            "nextdns" if !custom.is_empty() => ("secure", format!("https://dns.nextdns.io/{custom}")),
            "custom" if !custom.is_empty() => ("secure", custom.to_string()),
            _ => ("automatic", String::new()),
        }
    }

    /// Bring every value back into its allowed range (values come from an
    /// interface page; nothing invalid is ever stored).
    pub fn sanitize(&mut self) {
        if !(SEARCH_ENGINES.iter().any(|(id, _, _)| *id == self.search_engine) || self.search_engine == "custom") {
            self.search_engine = "duckduckgo".into();
        }
        self.search_custom_name = self.search_custom_name.trim().chars().take(40).collect();
        self.search_custom_url = self.search_custom_url.trim().chars().take(500).collect();
        if self.search_engine == "custom" && !valid_search_template(&self.search_custom_url) {
            self.search_engine = "duckduckgo".into();
        }
        if !["", "newtab", "restore", "pages"].contains(&self.startup_mode.as_str()) {
            self.startup_mode = String::new();
        }
        let mut pages: Vec<String> = self.startup_pages.iter().filter_map(|u| http_url(u)).collect();
        pages.dedup();
        pages.truncate(20);
        self.startup_pages = pages;
        if !self.startup_mode.is_empty() {
            self.restore_session = self.startup_mode == "restore";
        }
        if !ZOOM_LEVELS.contains(&self.default_zoom) {
            self.default_zoom = 100;
        }
        if !FONT_SIZES.contains(&self.font_size) {
            self.font_size = 16;
        }
        if !["system", "off", "cloudflare", "quad9", "mullvad", "nextdns", "custom"].contains(&self.secure_dns.as_str()) {
            self.secure_dns = "system".into();
        }
        let custom = self.secure_dns_custom.trim().to_string();
        self.secure_dns_custom = match self.secure_dns.as_str() {
            "nextdns" if custom.len() <= 32 && custom.chars().all(|c| c.is_ascii_alphanumeric()) => custom,
            "custom" if custom.starts_with("https://") && url::Url::parse(&custom.replace("{?dns}", "")).is_ok() && !custom.contains(char::is_whitespace) => custom,
            _ => String::new(),
        };
        if matches!(self.secure_dns.as_str(), "nextdns" | "custom") && self.secure_dns_custom.is_empty() {
            self.secure_dns = "system".into();
        }
        self.permission_defaults.retain(|k, v| super::permissions::kind_label(k).is_some() && (v == "ask" || v == "block"));
        let mut langs: Vec<String> = Vec::new();
        for t in self.accept_languages.split(',').map(str::trim) {
            if valid_language_tag(t) && !langs.iter().any(|l| l.eq_ignore_ascii_case(t)) && langs.len() < 12 {
                langs.push(t.to_string());
            }
        }
        self.accept_languages = if langs.is_empty() { DEFAULT_ACCEPT_LANGUAGES.into() } else { langs.join(",") };
        if let Some(d) = &self.download_dir {
            if d.trim().is_empty() || !std::path::Path::new(d.trim()).is_absolute() {
                self.download_dir = None;
            }
        }
    }

    /// "Reset settings": defaults everywhere, but the user's data and the
    /// emergency rules are kept (new tab shortcuts, sync folder, auto-purge).
    pub fn reset_keeping_data(&self) -> Settings {
        Settings { shortcuts: self.shortcuts.clone(), sync_folder: self.sync_folder.clone(), auto_purge: self.auto_purge.clone(), ..Settings::default() }
    }
}

/// Keys and stores available only while unlocked.
pub struct Unlocked {
    pub master: SecretKey,
    pub storage: Option<Storage>,
    pub vault: Option<Vault>,
    pub cookies_key: Option<SecretKey>,
}

pub struct Core {
    pub paths: ProfilePaths,
    pub lock: Option<NightfallLock>,
    pub unlocked: Option<Unlocked>,
    /// Setup done but the Recovery Key screen not yet confirmed.
    pub awaiting_recovery_confirmation: bool,
    pub prefs: UiPrefs,
    pub settings: Settings,
    pub videoguard: VideoGuardConfig,
    pub signals: Vec<Signal>,
    pub last_error: Option<String>,
}

impl Core {
    pub fn is_unlocked(&self) -> bool {
        self.unlocked.is_some() && !self.awaiting_recovery_confirmation
    }

    pub fn storage(&self) -> Option<&Storage> {
        self.unlocked.as_ref().and_then(|u| u.storage.as_ref())
    }

    pub fn vault_mut(&mut self) -> Option<&mut Vault> {
        self.unlocked.as_mut().and_then(|u| u.vault.as_mut())
    }

    pub fn vault(&self) -> Option<&Vault> {
        self.unlocked.as_ref().and_then(|u| u.vault.as_ref())
    }

    pub fn save_prefs(&self) {
        if let Ok(json) = serde_json::to_vec(&self.prefs) {
            let _ = nf_core::paths::write_private(&self.paths.ui_prefs(), &json);
        }
    }

    pub fn save_settings(&self) {
        if let Some(s) = self.storage() {
            let _ = s.set_setting("app", &self.settings);
            let _ = s.set_setting("videoguard", &self.videoguard);
        }
        if let Some(sh) = SHIELD.get() {
            if let Some(s) = self.storage() {
                let _ = s.set_setting("shield", &sh.config());
                let _ = s.set_setting("shield_total", &blocked_total());
            }
        }
    }
}

/// Read a saved setting. A copy that cannot be read (damaged, or written by
/// an incompatible version) is never silently replaced: it is logged and
/// kept as is under `<key>.illisible` (or `<key>.illisible.<time>` when
/// that key is taken) before the defaults are used, so the next save cannot
/// lose it. The log names the key only, never the value.
pub fn load_setting<T: serde::de::DeserializeOwned>(st: &Storage, key: &str) -> Option<T> {
    let raw = match st.setting::<serde_json::Value>(key) {
        Ok(Some(v)) => v,
        Ok(None) => return None,
        Err(_) => {
            log(&format!("réglages : « {key} » illisible dans la base (valeurs par défaut utilisées)"));
            return None;
        }
    };
    match serde_json::from_value::<T>(raw.clone()) {
        Ok(v) => Some(v),
        Err(_) => {
            let mut backup = format!("{key}.illisible");
            if matches!(st.setting::<serde_json::Value>(&backup), Ok(Some(_))) {
                backup = format!("{key}.illisible.{}", nf_core::unix_now());
            }
            let kept = st.set_setting(&backup, &raw).is_ok();
            log(&format!(
                "réglages : « {key} » ne peut pas être lu (valeurs par défaut utilisées) ; copie enregistrée {}",
                if kept { format!("conservée sous « {backup} »") } else { "NON conservée (écriture impossible)".to_string() }
            ));
            None
        }
    }
}

/// Items blocked by Shield since the profile was created: the saved total
/// from previous sessions plus this session's counter.
static BLOCKED_BASE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn blocked_total() -> u64 {
    let session = SHIELD.get().map(|s| s.stats().total_blocked).unwrap_or(0);
    BLOCKED_BASE.load(std::sync::atomic::Ordering::Relaxed) + session
}

/// Called when the encrypted database opens with the saved total.
pub fn set_blocked_saved_total(saved: u64) {
    let session = SHIELD.get().map(|s| s.stats().total_blocked).unwrap_or(0);
    BLOCKED_BASE.store(saved.saturating_sub(session), std::sync::atomic::Ordering::Relaxed);
}

static CORE: OnceLock<Mutex<Core>> = OnceLock::new();
pub static SHIELD: OnceLock<Arc<Shield>> = OnceLock::new();

pub fn init_core(core: Core) {
    let _ = CORE.set(Mutex::new(core));
}

/// Lock the core state. Poisoning is recovered (a panic in one handler must
/// not take the whole browser down).
pub fn core() -> MutexGuard<'static, Core> {
    match CORE.get().expect("core initialised").lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    }
}

/// GPU acceleration preference, readable before the core is fully set up
/// (Chromium command line); `true` when unknown.
pub fn hardware_acceleration_pref() -> bool {
    CORE.get().map(|m| match m.lock() {
        Ok(g) => g.prefs.hardware_acceleration,
        Err(p) => p.into_inner().prefs.hardware_acceleration,
    })
    .unwrap_or(true)
}

pub fn shield() -> Arc<Shield> {
    SHIELD.get().expect("shield initialised").clone()
}

/// What a CEF browser is used for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrowserKind {
    /// NIGHTFALL interface view of a window (top bar or vertical side bar).
    Ui { window_id: u32, side: bool },
    /// Web content tab.
    Tab { window_id: u32, tab_id: u32, private: bool },
}

#[derive(Debug, Clone)]
pub struct BrowserInfo {
    pub kind: BrowserKind,
    /// Last committed top-level URL.
    pub url: String,
    /// One navigation to an internal nightfall:// page is allowed (set when
    /// NIGHTFALL itself opens an internal page in this tab).
    pub allow_internal_nav: bool,
}

static BROWSERS: OnceLock<Mutex<HashMap<i32, BrowserInfo>>> = OnceLock::new();

pub fn browsers() -> MutexGuard<'static, HashMap<i32, BrowserInfo>> {
    let m = BROWSERS.get_or_init(|| Mutex::new(HashMap::new()));
    match m.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    }
}

pub fn browser_info(id: i32) -> Option<BrowserInfo> {
    browsers().get(&id).cloned()
}

/// Pending navigations to internal pages, keyed by tab id (the browser id is
/// not known before the browser exists).
static INTERNAL_TABS: OnceLock<Mutex<Vec<u32>>> = OnceLock::new();

pub fn allow_internal_for_tab(tab_id: u32) {
    let m = INTERNAL_TABS.get_or_init(|| Mutex::new(Vec::new()));
    if let Ok(mut v) = m.lock() {
        v.push(tab_id);
    }
}

pub fn take_internal_for_tab(tab_id: u32) -> bool {
    let m = INTERNAL_TABS.get_or_init(|| Mutex::new(Vec::new()));
    if let Ok(mut v) = m.lock() {
        if let Some(i) = v.iter().position(|t| *t == tab_id) {
            v.remove(i);
            return true;
        }
    }
    false
}

/// Drop every pending authorisation of a tab (tab closed, or its browser
/// could not be created): none may be consumed later by another browser.
pub fn forget_internal_for_tab(tab_id: u32) {
    let m = INTERNAL_TABS.get_or_init(|| Mutex::new(Vec::new()));
    if let Ok(mut v) = m.lock() {
        v.retain(|t| *t != tab_id);
    }
}

/// Diagnostic (CI self test): pending authorisations for a tab.
#[cfg(feature = "selftest")]
pub fn internal_pending_for_tab(tab_id: u32) -> usize {
    let m = INTERNAL_TABS.get_or_init(|| Mutex::new(Vec::new()));
    m.lock().map(|v| v.iter().filter(|t| **t == tab_id).count()).unwrap_or(0)
}

static LOG_FILE: OnceLock<std::path::PathBuf> = OnceLock::new();

/// Set the log file (inside the profile) once the profile is known.
pub fn set_log_file(path: std::path::PathBuf) {
    let _ = LOG_FILE.set(path);
}

/// Write a redacted log line to stderr and to `<profile>/logs/nightfall.log`.
pub fn log(line: &str) {
    use std::io::Write;
    let clean = nf_core::redact::sanitize(line);
    eprintln!("[NIGHTFALL] {clean}");
    if let Some(path) = LOG_FILE.get() {
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(f, "{} {clean}", nf_core::unix_now());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_saved_settings_still_load_with_new_defaults() {
        let old = r#"{"search_engine":"qwant","vertical_tabs":true,"restore_session":false}"#;
        let s: Settings = serde_json::from_str(old).unwrap();
        assert_eq!(s.search_engine, "qwant");
        assert!(s.vertical_tabs);
        assert!(s.show_home_button && s.global_privacy_control && s.https_first);
        assert_eq!((s.default_zoom, s.font_size), (100, 16));
        assert_eq!(s.accept_languages, DEFAULT_ACCEPT_LANGUAGES);
        // Older profiles: start-up follows "restore_session".
        assert_eq!(s.startup(), Startup::NewTab);
        let p: UiPrefs = serde_json::from_str(r#"{"splash_enabled":false}"#).unwrap();
        assert!(!p.splash_enabled && p.hardware_acceleration);
    }

    #[test]
    fn settings_round_trip() {
        let mut s = Settings::default();
        s.search_engine = "custom".into();
        s.search_custom_url = "https://search.example/?q=%s".into();
        s.startup_mode = "pages".into();
        s.startup_pages = vec!["https://example.com/".into()];
        s.permission_defaults.insert("geolocation".into(), "block".into());
        s.site_permissions.push(SitePermission { origin: "https://a.example".into(), kind: "camera".into(), allow: true, updated: 5 });
        s.download_dir = Some(if cfg!(windows) { "C:\\Dl".into() } else { "/tmp/dl".into() });
        let json = serde_json::to_string(&s).unwrap();
        let back: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(serde_json::to_value(&back).unwrap(), serde_json::to_value(&s).unwrap());
    }

    #[test]
    fn search_engines_and_custom_template() {
        let mut s = Settings::default();
        assert_eq!(s.search_url("chat noir"), "https://duckduckgo.com/?q=chat+noir");
        for (id, _, tpl) in SEARCH_ENGINES {
            s.search_engine = id.to_string();
            assert_eq!(s.search_url("a&b"), tpl.replace("%s", "a%26b"));
        }
        s.search_engine = "brave".into();
        assert_eq!(s.search_url("x"), "https://search.brave.com/search?q=x");
        s.search_engine = "custom".into();
        s.search_custom_url = "https://recherche.example/s?k=%s&lang=fr".into();
        assert_eq!(s.search_url("é t"), "https://recherche.example/s?k=%C3%A9+t&lang=fr");
        // Invalid custom engines are refused.
        for bad in ["http://insecure.example/?q=%s", "https://no-placeholder.example/", "javascript:alert(%s)"] {
            let mut t = s.clone();
            t.search_custom_url = bad.into();
            t.sanitize();
            assert_eq!(t.search_engine, "duckduckgo", "{bad}");
        }
        let mut t = s.clone();
        t.search_engine = "inconnu".into();
        t.sanitize();
        assert_eq!(t.search_engine, "duckduckgo");
    }

    #[test]
    fn startup_modes() {
        let mut s = Settings::default();
        assert_eq!(s.startup(), Startup::Restore);
        s.startup_mode = "newtab".into();
        s.sanitize();
        assert_eq!(s.startup(), Startup::NewTab);
        assert!(!s.restore_session);
        s.startup_mode = "pages".into();
        s.startup_pages = vec!["https://a.example".into(), "file:///etc/passwd".into(), "javascript:x".into(), "http://b.example/x".into()];
        s.sanitize();
        assert_eq!(s.startup(), Startup::Pages);
        assert_eq!(s.startup_pages, vec!["https://a.example/".to_string(), "http://b.example/x".to_string()]);
        let tabs = s.startup_tabs();
        assert_eq!(tabs.len(), 2);
        assert!(tabs[0].active && !tabs[1].active);
        s.startup_pages.clear();
        assert_eq!(s.startup(), Startup::NewTab);
        s.startup_mode = "restore".into();
        s.sanitize();
        assert!(s.restore_session);
    }

    #[test]
    fn sanitize_ranges() {
        let mut s = Settings::default();
        s.default_zoom = 333;
        s.font_size = 2;
        s.accept_languages = "de-DE, fr ,<script>,fr,en-US".into();
        s.secure_dns = "nextdns".into();
        s.secure_dns_custom = "abc 123".into();
        s.permission_defaults.insert("camera".into(), "block".into());
        s.permission_defaults.insert("camera2".into(), "block".into());
        s.permission_defaults.insert("geolocation".into(), "allow".into());
        s.download_dir = Some("relatif/dossier".into());
        s.sanitize();
        assert_eq!((s.default_zoom, s.font_size), (100, 16));
        assert_eq!(s.accept_languages, "de-DE,fr,en-US");
        assert_eq!(s.secure_dns, "system");
        assert_eq!(s.permission_defaults.len(), 1);
        assert_eq!(s.download_dir, None);
        s.accept_languages = " , ".into();
        s.sanitize();
        assert_eq!(s.accept_languages, DEFAULT_ACCEPT_LANGUAGES);
    }

    #[test]
    fn secure_dns_providers() {
        let mut s = Settings::default();
        assert_eq!(s.doh_config(), ("automatic", String::new()));
        s.secure_dns = "off".into();
        assert_eq!(s.doh_config().0, "off");
        s.secure_dns = "quad9".into();
        assert_eq!(s.doh_config(), ("secure", "https://dns.quad9.net/dns-query".to_string()));
        s.secure_dns = "nextdns".into();
        s.secure_dns_custom = "a1b2c3".into();
        s.sanitize();
        assert_eq!(s.doh_config(), ("secure", "https://dns.nextdns.io/a1b2c3".to_string()));
        s.secure_dns = "custom".into();
        s.secure_dns_custom = "https://doh.example/dns-query{?dns}".into();
        s.sanitize();
        assert_eq!(s.doh_config(), ("secure", "https://doh.example/dns-query{?dns}".to_string()));
        s.secure_dns_custom = "http://doh.example/dns-query".into();
        s.sanitize();
        assert_eq!(s.doh_config().0, "automatic");
    }

    #[test]
    fn nested_settings_with_missing_fields_load_without_loss() {
        // Complete "app" copy as saved today: perf with its 3 fields,
        // auto_purge with its 3 fields.
        let mut s = Settings::default();
        s.search_engine = "qwant".into();
        s.perf.discard_after_minutes = 45;
        s.perf.battery_saver = false;
        s.auto_purge.enabled = true;
        s.auto_purge.min_independent_critical = 3;
        s.site_permissions.push(SitePermission { origin: "https://a.example".into(), kind: "camera".into(), allow: true, updated: 5 });
        let full = serde_json::to_value(&s).unwrap();
        assert_eq!(full["perf"].as_object().unwrap().len(), 3);
        assert_eq!(full["auto_purge"].as_object().unwrap().len(), 3);
        let back: Settings = serde_json::from_value(full.clone()).unwrap();
        assert_eq!(serde_json::to_value(&back).unwrap(), full);
        // The same copy read by a version whose nested structures have more
        // fields = a copy whose nested structures lack one: nothing else is
        // lost (before, the whole of "app" fell back to the defaults).
        let mut partial = full.clone();
        partial["perf"].as_object_mut().unwrap().remove("discard_enabled");
        partial["auto_purge"].as_object_mut().unwrap().remove("categories");
        let back: Settings = serde_json::from_value(partial).unwrap();
        assert_eq!(back.search_engine, "qwant");
        assert_eq!((back.perf.discard_after_minutes, back.perf.battery_saver, back.perf.discard_enabled), (45, false, true));
        assert_eq!((back.auto_purge.enabled, back.auto_purge.min_independent_critical), (true, 3));
        assert_eq!(back.auto_purge.categories, AutoPurgeRules::default().categories);
        assert_eq!(back.site_permissions, s.site_permissions);
        // Unknown fields from a newer version are ignored.
        let mut newer = full.clone();
        newer["perf"]["new_field"] = serde_json::json!(1);
        newer["auto_purge"]["new_field"] = serde_json::json!("x");
        let back: Settings = serde_json::from_value(newer).unwrap();
        assert_eq!(serde_json::to_value(&back).unwrap(), full);
    }

    #[test]
    fn unreadable_saved_settings_are_kept_not_overwritten() {
        let key = nf_crypto::SecretKey::from_bytes([7; 32]);
        let st = Storage::open_in_memory(&key).unwrap();
        assert!(load_setting::<Settings>(&st, "app").is_none());
        st.set_setting("app", &Settings { search_engine: "brave".into(), ..Default::default() }).unwrap();
        assert_eq!(load_setting::<Settings>(&st, "app").unwrap().search_engine, "brave");
        // A copy of an incompatible shape: defaults used, raw copy kept.
        let bad = serde_json::json!({ "search_engine": 42, "vertical_tabs": true });
        st.set_setting("app", &bad).unwrap();
        assert!(load_setting::<Settings>(&st, "app").is_none());
        assert_eq!(st.setting::<serde_json::Value>("app.illisible").unwrap(), Some(bad.clone()));
        // A second failure keeps the first copy and adds another one.
        let bad2 = serde_json::json!({ "perf": "x" });
        st.set_setting("app", &bad2).unwrap();
        assert!(load_setting::<Settings>(&st, "app").is_none());
        assert_eq!(st.setting::<serde_json::Value>("app.illisible").unwrap(), Some(bad));
        // Same rule for the other saved structures.
        st.set_setting("videoguard", &serde_json::json!({ "enabled": "oui" })).unwrap();
        assert!(load_setting::<VideoGuardConfig>(&st, "videoguard").is_none());
        assert!(st.setting::<serde_json::Value>("videoguard.illisible").unwrap().is_some());
        st.set_setting("shield", &serde_json::json!({ "enabled": false })).unwrap();
        assert!(!load_setting::<nf_shield::ShieldConfig>(&st, "shield").unwrap().enabled);
    }

    #[test]
    fn reset_keeps_user_data() {
        let mut s = Settings::default();
        s.shortcuts = vec![Shortcut { title: "X".into(), url: "https://x.example/".into() }];
        s.sync_folder = Some("/sync".into());
        s.search_engine = "bing".into();
        s.font_size = 24;
        s.auto_purge.enabled = true;
        let r = s.reset_keeping_data();
        assert_eq!(r.shortcuts, s.shortcuts);
        assert_eq!(r.sync_folder, s.sync_folder);
        assert!(r.auto_purge.enabled);
        assert_eq!((r.search_engine.as_str(), r.font_size), ("duckduckgo", 16));
    }
}

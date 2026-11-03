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
use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

/// Preferences readable before unlock (not sensitive).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UiPrefs {
    pub splash_enabled: bool,
}

impl Default for UiPrefs {
    fn default() -> Self {
        Self { splash_enabled: true }
    }
}

/// User settings, stored in the encrypted database.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// duckduckgo | google | brave | startpage | qwant | ecosia | bing
    pub search_engine: String,
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
            vertical_tabs: false,
            auto_lock_minutes: 15,
            lock_on_sleep: true,
            session_vault: true,
            restore_session: true,
            devtools_enabled: false,
            phishing_warnings: true,
            wallpaper: "nebula".into(),
            shortcuts: vec![
                Shortcut { title: "YouTube".into(), url: "https://www.youtube.com/".into() },
                Shortcut { title: "Wikipédia".into(), url: "https://fr.wikipedia.org/".into() },
                Shortcut { title: "GitHub".into(), url: "https://github.com/".into() },
            ],
            perf: nf_perf::PerfConfig::default(),
            auto_purge: AutoPurgeRules::default(),
            sync_folder: None,
        }
    }
}

impl Settings {
    pub fn search_url(&self, query: &str) -> String {
        let q: String = url::form_urlencoded::byte_serialize(query.as_bytes()).collect();
        let base = match self.search_engine.as_str() {
            "google" => "https://www.google.com/search?q=",
            "brave" => "https://search.brave.com/search?q=",
            "startpage" => "https://www.startpage.com/do/search?q=",
            "qwant" => "https://www.qwant.com/?q=",
            "ecosia" => "https://www.ecosia.org/search?q=",
            "bing" => "https://www.bing.com/search?q=",
            _ => "https://duckduckgo.com/?q=",
        };
        format!("{base}{q}")
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
            }
        }
    }
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

static BROWSERS: OnceLock<Mutex<HashMap<u64, BrowserInfo>>> = OnceLock::new();

pub fn browsers() -> MutexGuard<'static, HashMap<u64, BrowserInfo>> {
    let m = BROWSERS.get_or_init(|| Mutex::new(HashMap::new()));
    match m.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    }
}

pub fn browser_info(id: u64) -> Option<BrowserInfo> {
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

pub fn log(line: &str) {
    let clean = nf_core::redact::sanitize(line);
    eprintln!("[NIGHTFALL] {clean}");
}

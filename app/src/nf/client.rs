//! CEF client and handlers for interface views and web tabs.

use super::state::{self, core, BrowserInfo, BrowserKind};
use super::ui_thread::{background, on_ui};
use super::windows;
use cef::*;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientKind {
    Ui { window_id: u32, side: bool },
    Tab { window_id: u32, tab_id: u32, private: bool },
}

// --------------------------------------------------------- blocked counters

static BLOCKED: OnceLock<Mutex<HashMap<i32, u64>>> = OnceLock::new();

fn blocked_map() -> std::sync::MutexGuard<'static, HashMap<i32, u64>> {
    let m = BLOCKED.get_or_init(|| Mutex::new(HashMap::new()));
    match m.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    }
}

pub fn blocked_count(browser_id: i32) -> u64 {
    blocked_map().get(&browser_id).copied().unwrap_or(0)
}

fn cef_to_string(s: &CefStringUserfree) -> String {
    CefString::from(s).to_string()
}

fn browser_url(browser: &Browser) -> String {
    browser.main_frame().map(|f| cef_to_string(&f.url())).unwrap_or_default()
}

// ------------------------------------------------------------------- client

wrap_client! {
    pub struct NfClient {
        kind: Arc<ClientKind>,
    }

    impl Client {
        fn display_handler(&self) -> Option<DisplayHandler> {
            Some(NfDisplay::new(self.kind.clone()))
        }

        fn life_span_handler(&self) -> Option<LifeSpanHandler> {
            Some(NfLifeSpan::new(self.kind.clone()))
        }

        fn load_handler(&self) -> Option<LoadHandler> {
            Some(NfLoad::new(self.kind.clone()))
        }

        fn request_handler(&self) -> Option<RequestHandler> {
            Some(NfRequest::new(self.kind.clone()))
        }

        fn keyboard_handler(&self) -> Option<KeyboardHandler> {
            Some(NfKeyboard::new(self.kind.clone()))
        }

        fn permission_handler(&self) -> Option<PermissionHandler> {
            match *self.kind {
                ClientKind::Tab { .. } => Some(super::permissions::NfPermission::new(self.kind.clone())),
                ClientKind::Ui { .. } => None,
            }
        }

        fn download_handler(&self) -> Option<DownloadHandler> {
            match *self.kind {
                ClientKind::Tab { .. } => Some(super::downloads::NfDownload::new(self.kind.clone())),
                ClientKind::Ui { .. } => None,
            }
        }

        fn find_handler(&self) -> Option<FindHandler> {
            match *self.kind {
                ClientKind::Tab { window_id, .. } => Some(super::browsing::NfFind::new(window_id)),
                ClientKind::Ui { .. } => None,
            }
        }

        fn context_menu_handler(&self) -> Option<ContextMenuHandler> {
            Some(super::browsing::NfContextMenu::new(self.kind.clone()))
        }

        // Interface views only: their `app-region: drag` areas move the
        // frameless window (integrated title bar, see titlebar.rs).
        fn drag_handler(&self) -> Option<DragHandler> {
            match *self.kind {
                ClientKind::Ui { window_id, .. } => Some(NfUiDrag::new(window_id)),
                ClientKind::Tab { .. } => None,
            }
        }
    }
}

wrap_drag_handler! {
    struct NfUiDrag {
        window_id: u32,
    }

    impl DragHandler {
        fn on_draggable_regions_changed(&self, browser: Option<&mut Browser>, frame: Option<&mut Frame>, regions: Option<&[DraggableRegion]>) {
            // Only the main frame of an interface page defines the regions.
            if frame.map(|f| f.is_main() == 0).unwrap_or(false) {
                return;
            }
            if let Some(b) = browser {
                super::titlebar::on_regions_changed(self.window_id, b, regions.unwrap_or(&[]));
            }
        }
    }
}

/// Strings of a CEF string list (handed over by CEF for the call only).
fn string_list(list: &mut CefStringList) -> Vec<String> {
    std::mem::take(list).into_iter().collect()
}

/// Build a client for an interface view or a tab.
pub fn make_client(kind: ClientKind) -> Client {
    NfClient::new(Arc::new(kind))
}

// ------------------------------------------------------------------ display

wrap_display_handler! {
    struct NfDisplay {
        kind: Arc<ClientKind>,
    }

    impl DisplayHandler {
        fn on_address_change(&self, browser: Option<&mut Browser>, frame: Option<&mut Frame>, url: Option<&CefString>) {
            let (Some(browser), Some(frame), Some(url)) = (browser, frame, url) else { return };
            if frame.is_main() == 0 {
                return;
            }
            let url = url.to_string();
            if let ClientKind::Tab { window_id, tab_id, private } = *self.kind {
                let id = browser.identifier();
                {
                    let mut map = state::browsers();
                    if let Some(info) = map.get_mut(&id) {
                        info.url = url.clone();
                    }
                }
                blocked_map().insert(id, 0);
                super::browsing::on_main_navigation(window_id, tab_id, id, &url);
                on_navigation(window_id, tab_id, private, &url);
                super::browsing::apply_site_zoom(browser, &url);
            }
        }

        fn on_fullscreen_mode_change(&self, _browser: Option<&mut Browser>, fullscreen: i32) {
            if let ClientKind::Tab { window_id, tab_id, .. } = *self.kind {
                windows::set_content_fullscreen(window_id, tab_id, fullscreen != 0);
            }
        }

        fn on_favicon_urlchange(&self, browser: Option<&mut Browser>, icon_urls: Option<&mut CefStringList>) {
            if let (ClientKind::Tab { window_id, tab_id, .. }, Some(browser), Some(list)) = (&*self.kind, browser, icon_urls) {
                let urls = string_list(list);
                super::browsing::favicon_changed(*window_id, *tab_id, browser, urls);
            }
        }

        fn on_console_message(
            &self,
            browser: Option<&mut Browser>,
            _level: LogSeverity,
            message: Option<&CefString>,
            source: Option<&CefString>,
            line: i32,
        ) -> i32 {
            if let ClientKind::Tab { window_id, tab_id, .. } = *self.kind {
                let msg = message.map(|m| m.to_string()).unwrap_or_default();
                if let Some(payload) = msg.strip_prefix(shield_token().as_str()) {
                    if let Some(b) = browser {
                        shield_generic_report(b, payload);
                    }
                    return 1; // handled, not shown in the page's console
                }
                if msg.starts_with('\u{1}') {
                    let id = browser.map(|b| b.identifier()).unwrap_or(0);
                    if super::browsing::media_report(window_id, tab_id, id, &msg) {
                        return 1;
                    }
                }
            }
            // Only NIGHTFALL's own pages are logged (never web page content).
            let source = source.map(|s| s.to_string()).unwrap_or_default();
            if source.starts_with("nightfall://") {
                let msg = message.map(|m| m.to_string()).unwrap_or_default();
                state::log(&format!("console {source}:{line} {msg}"));
                #[cfg(feature = "selftest")]
                if source.starts_with("nightfall://selftest") {
                    super::selftest::console_result(&msg);
                }
            }
            0
        }

        fn on_title_change(&self, _browser: Option<&mut Browser>, title: Option<&CefString>) {
            let title = title.map(|t| t.to_string()).unwrap_or_default();
            if let ClientKind::Tab { window_id, tab_id, private } = *self.kind {
                let url = windows::tab_url(window_id, tab_id).unwrap_or_default();
                let new_title = title.clone();
                windows::update_tab(window_id, tab_id, move |t| {
                    t.title = new_title;
                });
                if !private && (url.starts_with("http://") || url.starts_with("https://")) {
                    if let Some(s) = core().storage() {
                        let _ = s.set_history_title(&url, &title);
                    }
                }
                if windows::active_tab_id(window_id) == Some(tab_id) {
                    windows::activate_tab_title(window_id);
                }
            }
        }
    }
}

fn on_navigation(window_id: u32, tab_id: u32, private: bool, url: &str) {
    let is_web = url.starts_with("http://") || url.starts_with("https://");
    // Anti-phishing (local heuristics) on top-level navigations.
    let mut warning = None;
    if is_web && core().settings.phishing_warnings {
        if let Some(v) = super::security::phishing_check(url) {
            warning = Some(v);
        }
    }
    let (new_url, new_warning) = (url.to_string(), warning.clone());
    windows::update_tab(window_id, tab_id, move |t| {
        t.url = new_url;
        t.warning = new_warning;
        t.crashed = false;
    });
    if let Some(w) = &warning {
        windows::push_event(window_id, &serde_json::json!({ "type": "warning", "tab": tab_id, "message": w }));
    }
    if is_web && !private {
        if let Some(s) = core().storage() {
            let _ = s.record_visit(url, "", nf_core::unix_now());
        }
    }
    // VideoGuard: fetch SponsorBlock segments for YouTube videos.
    videoguard_on_navigation(window_id, tab_id, url);
}

/// VideoGuard: SponsorBlock segments of a YouTube video. Nothing is sent nor
/// injected while NIGHTFALL is locked: the video is asked again at unlock if
/// the tab still shows it (see lock_guard).
pub fn videoguard_on_navigation(window_id: u32, tab_id: u32, url: &str) {
    let Some(video) = nf_videoguard::video_id(url) else { return };
    let cfg = core().videoguard.clone();
    if !cfg.enabled {
        return;
    }
    if super::lock_guard::locked() {
        super::lock_guard::defer_videoguard(window_id, tab_id, url);
        return;
    }
    super::lock_guard::videoguard_fetch_started();
    let page = url.to_string();
    background(
        move || nf_videoguard::fetch_segments(&video, &cfg).map(|s| (video, s)),
        move |res| match res {
            Ok((video, segments)) => {
                // Locked while the request was running: not injected.
                if super::lock_guard::locked() {
                    super::lock_guard::defer_videoguard(window_id, tab_id, &page);
                    return;
                }
                let script = nf_videoguard::injection_script(&video, &segments);
                windows::exec_in_tab(window_id, tab_id, &script);
            }
            Err(e) => state::log(&format!("VideoGuard: {e}")),
        },
    );
}

// ---------------------------------------------------------------- life span

wrap_life_span_handler! {
    struct NfLifeSpan {
        kind: Arc<ClientKind>,
    }

    impl LifeSpanHandler {
        fn on_after_created(&self, browser: Option<&mut Browser>) {
            let Some(browser) = browser else { return };
            state::log(&format!("navigateur créé id={} type={:?}", browser.identifier(), *self.kind));
            let kind = match *self.kind {
                ClientKind::Ui { window_id, side } => BrowserKind::Ui { window_id, side },
                ClientKind::Tab { window_id, tab_id, private } => BrowserKind::Tab { window_id, tab_id, private },
            };
            let allow_internal = match *self.kind {
                ClientKind::Tab { tab_id, .. } => state::take_internal_for_tab(tab_id),
                ClientKind::Ui { .. } => true,
            };
            state::browsers().insert(
                browser.identifier(),
                BrowserInfo { kind, url: browser_url(browser), allow_internal_nav: allow_internal },
            );
            // Opened by a hidden page while NIGHTFALL is locked: muted.
            if let ClientKind::Tab { .. } = *self.kind {
                super::lock_guard::on_tab_created(browser);
            }
        }

        fn on_before_close(&self, browser: Option<&mut Browser>) {
            if let Some(b) = browser {
                let id = b.identifier();
                state::browsers().remove(&id);
                blocked_map().remove(&id);
                super::browsing::forget_browser(id);
                super::https_first::done(id);
                if let ClientKind::Ui { .. } = *self.kind {
                    windows::forget_popup_browser(id);
                }
                // Not closed by NIGHTFALL: the page closed itself
                // (window.close() at the end of a login popup).
                if let ClientKind::Tab { window_id, tab_id, .. } = *self.kind {
                    if !windows::take_closing(id) {
                        super::ui_thread::on_ui_delayed(0, move || windows::on_tab_self_closed(window_id, tab_id, id));
                    }
                }
            }
        }

        fn do_close(&self, browser: Option<&mut Browser>) -> i32 {
            // Dropdown popups (overlay views): answering 0 would let CEF
            // send the close to the view's widget, or to the NIGHTFALL
            // window. Their browser is destroyed with their view instead
            // (windows::destroy_popup); a close that does not come from
            // NIGHTFALL closes the popup the same way.
            if let (ClientKind::Ui { window_id, .. }, Some(b)) = (&*self.kind, browser.as_deref()) {
                let id = b.identifier();
                if windows::is_popup_browser(id) {
                    if !windows::is_closing(id) {
                        let window_id = *window_id;
                        super::ui_thread::on_ui_delayed(0, move || windows::popup_close_browser(window_id, id));
                    }
                    return 1;
                }
            }
            // With Alloy and Views, letting CEF handle a page's own
            // window.close() (end of a login popup) closes the whole
            // NIGHTFALL window. Only the tab is removed.
            let (ClientKind::Tab { window_id, tab_id, .. }, Some(b)) = (&*self.kind, browser) else { return 0 };
            let id = b.identifier();
            if windows::is_closing(id) {
                return 0; // closed by NIGHTFALL: its view is already detached
            }
            let (window_id, tab_id) = (*window_id, *tab_id);
            super::ui_thread::on_ui_delayed(0, move || windows::on_tab_self_closed(window_id, tab_id, id));
            1
        }

        fn on_before_popup_aborted(&self, _browser: Option<&mut Browser>, popup_id: i32) {
            if let ClientKind::Tab { window_id, .. } = *self.kind {
                windows::popup_aborted(window_id, popup_id);
            }
        }

        fn on_before_popup(
            &self,
            browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            popup_id: i32,
            target_url: Option<&CefString>,
            _target_frame_name: Option<&CefString>,
            target_disposition: WindowOpenDisposition,
            user_gesture: i32,
            _popup_features: Option<&PopupFeatures>,
            _window_info: Option<&mut WindowInfo>,
            client: Option<&mut Option<Client>>,
            _settings: Option<&mut BrowserSettings>,
            _extra_info: Option<&mut Option<DictionaryValue>>,
            _no_javascript_access: Option<&mut i32>,
        ) -> i32 {
            let ClientKind::Tab { window_id, private, tab_id: opener } = *self.kind else {
                // Interface views never open popups.
                return 1;
            };
            let page = browser.map(|b| browser_url(b)).unwrap_or_default();
            let target = target_url.map(|u| u.to_string()).unwrap_or_default();
            if target.starts_with("nightfall://") {
                return 1;
            }
            let shield = state::shield();
            if user_gesture == 0 && shield.popups_blocked_enabled(&page) {
                shield.record_popup_blocked(&page);
                windows::push_event(window_id, &serde_json::json!({ "type": "popup_blocked", "url": nf_core::redact::redact_url(&target) }));
                return 1;
            }
            // Allowed: the popup becomes a new tab in the same window, with
            // its own client so it is tracked as a separate tab. It keeps
            // window.opener (login flows such as "Sign in with Google").
            let background = target_disposition == WindowOpenDisposition::NEW_BACKGROUND_TAB;
            let tab_id = windows::reserve_popup(window_id, popup_id, Some(opener), background);
            if let Some(c) = client {
                *c = Some(make_client(ClientKind::Tab { window_id, tab_id, private }));
            }
            0
        }
    }
}

// --------------------------------------------------------------------- load

wrap_load_handler! {
    struct NfLoad {
        kind: Arc<ClientKind>,
    }

    impl LoadHandler {
        fn on_loading_state_change(&self, _browser: Option<&mut Browser>, is_loading: i32, can_go_back: i32, can_go_forward: i32) {
            if let ClientKind::Tab { window_id, tab_id, .. } = *self.kind {
                #[cfg(feature = "selftest")]
                state::log(&format!("onglet {tab_id} : chargement {}", if is_loading != 0 { "en cours" } else { "terminé" }));
                windows::update_tab(window_id, tab_id, move |t| {
                    t.loading = is_loading != 0;
                    t.can_back = can_go_back != 0;
                    t.can_forward = can_go_forward != 0;
                    if is_loading != 0 {
                        // Reloading a crashed page starts a new renderer.
                        t.crashed = false;
                    }
                });
            }
        }

        fn on_load_error(
            &self,
            browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            error_code: Errorcode,
            error_text: Option<&CefString>,
            failed_url: Option<&CefString>,
        ) {
            let code = { let raw: cef::sys::cef_errorcode_t = error_code.into(); raw as i32 };
            let text = error_text.map(|t| t.to_string()).unwrap_or_default();
            // HTTPS-First: the upgraded https:// address does not work.
            if let (ClientKind::Tab { .. }, Some(b), Some(f)) = (&*self.kind, browser, frame) {
                if f.is_main() != 0 {
                    let failed = failed_url.map(|u| u.to_string()).unwrap_or_default();
                    if let Some(http) = super::https_first::on_load_error(b.identifier(), &failed, code) {
                        load_later(b, super::https_first::warning_url(&http), true);
                    }
                }
            }
            match *self.kind {
                ClientKind::Ui { .. } => {
                    let url = failed_url.map(|u| u.to_string()).unwrap_or_default();
                    state::log(&format!("interface : échec du chargement {url} ({code} {text})"));
                }
                // Web pages: the error only, never the address (privacy).
                ClientKind::Tab { .. } => state::log(&format!("onglet : échec du chargement ({code} {text})")),
            }
        }

        fn on_load_start(&self, _browser: Option<&mut Browser>, frame: Option<&mut Frame>, _transition_type: TransitionType) {
            let Some(frame) = frame else { return };
            if let ClientKind::Tab { .. } = *self.kind {
                // NIGHTFALL locked: media of the new document stay paused.
                super::lock_guard::on_load_start(frame);
                let url = cef_to_string(&frame.url());
                // Default page zoom (Paramètres > Apparence).
                if frame.is_main() != 0 {
                    if let Some(b) = _browser {
                        super::prefs::apply_default_zoom(b, &url);
                    }
                }
                if !(url.starts_with("http://") || url.starts_with("https://")) {
                    return;
                }
                // Shield: scriptlets, hiding rules and procedural filters,
                // injected as early as possible (before the page's scripts).
                if let Some(js) = state::shield().page_script(&url, &shield_token()) {
                    frame.execute_java_script(Some(&CefString::from(js.as_str())), None, 0);
                }
                // Global Privacy Control, also readable by the page's scripts.
                if super::prefs::GPC.load(std::sync::atomic::Ordering::Relaxed) {
                    frame.execute_java_script(Some(&CefString::from(GPC_SCRIPT)), None, 0);
                }
                // VideoGuard on YouTube (main frame and embedded players);
                // not while locked (added again at unlock).
                let vg = core().videoguard.clone();
                if let Some(js) = nf_videoguard::page_script(&url, &vg).filter(|_| !super::lock_guard::locked()) {
                    frame.execute_java_script(Some(&CefString::from(js.as_str())), None, 0);
                }
                // "Playing sound" indicator of the tab.
                frame.execute_java_script(Some(&CefString::from(super::browsing::media_script().as_str())), None, 0);
            }
        }

        fn on_load_end(&self, browser: Option<&mut Browser>, frame: Option<&mut Frame>, http_status_code: i32) {
            let (Some(_browser), Some(frame)) = (browser, frame) else { return };
            if frame.is_main() == 0 {
                return;
            }
            if let ClientKind::Ui { .. } = *self.kind {
                state::log(&format!("interface chargée : {} (HTTP {http_status_code})", cef_to_string(&frame.url())));
            } else if http_status_code != 0 {
                super::https_first::done(_browser.identifier());
            }
        }
    }
}

/// Prefix of the page's reports of classes and ids (generic hiding rules).
/// Random per session; a page that imitates it can only ask for CSS hiding
/// rules computed from the filter lists, which is harmless.
pub fn shield_token() -> String {
    static TOKEN: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    TOKEN
        .get_or_init(|| {
            let r = nf_crypto::random_bytes(8).unwrap_or_else(|_| vec![7; 8]);
            format!("\u{1}nfcos{}:", nf_crypto::hex(&r))
        })
        .clone()
}

/// A report of classes/ids seen in a page: answer with the matching generic
/// hiding rules.
fn shield_generic_report(browser: &Browser, payload: &str) {
    #[derive(serde::Deserialize)]
    struct Report {
        #[serde(default)]
        c: Vec<String>,
        #[serde(default)]
        i: Vec<String>,
    }
    let Ok(r) = serde_json::from_str::<Report>(payload) else { return };
    if r.c.len() + r.i.len() > 5000 {
        return;
    }
    let Some(frame) = browser.main_frame() else { return };
    let url = cef_to_string(&frame.url());
    let css = state::shield().generic_css(&url, &r.c, &r.i);
    if css.is_empty() {
        return;
    }
    let js = format!("window.__nfShieldAddCss && window.__nfShieldAddCss({});", serde_json::to_string(&css).unwrap_or_default());
    frame.execute_java_script(Some(&CefString::from(js.as_str())), None, 0);
}

// ------------------------------------------------------------------ request

/// Requests sent with the Global Privacy Control header (self test).
pub static GPC_SENT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// `navigator.globalPrivacyControl`, as defined by the GPC specification.
const GPC_SCRIPT: &str = "try{Object.defineProperty(Navigator.prototype,'globalPrivacyControl',{get:function(){return true},configurable:true,enumerable:true})}catch(e){}";

/// Header added to a web request when Global Privacy Control is on. Only
/// on document requests (pages and frames): on scripts or fetches, a
/// non-standard header would turn cross-origin requests into CORS
/// preflights and break sites.
pub fn gpc_header(enabled: bool, url: &str, request_type: &str) -> Option<(&'static str, &'static str)> {
    let web = url.starts_with("https://") || url.starts_with("http://");
    (enabled && web && matches!(request_type, "main_frame" | "sub_frame")).then_some(("Sec-GPC", "1"))
}

/// Load `url` in the browser's main frame once the current callback has
/// returned. `internal`: a NIGHTFALL page, allowed once for this tab.
fn load_later(browser: &Browser, url: String, internal: bool) {
    let id = browser.identifier();
    if internal {
        if let Some(info) = state::browsers().get_mut(&id) {
            info.allow_internal_nav = true;
        }
    }
    let b = browser.clone();
    super::ui_thread::on_ui_delayed(0, move || {
        if let Some(f) = b.main_frame() {
            f.load_url(Some(&CefString::from(url.as_str())));
        }
    });
}

wrap_request_handler! {
    struct NfRequest {
        kind: Arc<ClientKind>,
    }

    impl RequestHandler {
        fn on_before_browse(
            &self,
            browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            request: Option<&mut Request>,
            _user_gesture: i32,
            _is_redirect: i32,
        ) -> i32 {
            let (Some(browser), Some(request)) = (browser, request) else { return 0 };
            let target = cef_to_string(&request.url());
            #[cfg(feature = "selftest")]
            if !target.starts_with("nightfall://") {
                state::log(&format!("selftest: on_before_browse {target}"));
            }
            let is_main = frame.map(|f| f.is_main() != 0).unwrap_or(true);
            match *self.kind {
                ClientKind::Ui { .. } => {
                    // Interface views may only show NIGHTFALL pages.
                    if target.starts_with("nightfall://app/") { 0 } else { 1 }
                }
                ClientKind::Tab { .. } => {
                    if !target.starts_with("nightfall://") {
                        if is_main && target.starts_with("http://") {
                            let enabled = super::prefs::HTTPS_FIRST.load(std::sync::atomic::Ordering::Relaxed);
                            match super::https_first::on_navigation(browser.identifier(), &target, enabled) {
                                super::https_first::Action::Continue => {}
                                super::https_first::Action::Upgrade(https) => {
                                    #[cfg(feature = "selftest")]
                                    state::log(&format!("selftest: HTTPS-First {target} -> {https}"));
                                    load_later(browser, https, false);
                                    return 1;
                                }
                                super::https_first::Action::Warn(http) => {
                                    load_later(browser, super::https_first::warning_url(&http), true);
                                    return 1;
                                }
                            }
                        }
                        return 0;
                    }
                    if !is_main {
                        return 1; // never in a sub-frame
                    }
                    let id = browser.identifier();
                    let mut map = state::browsers();
                    let Some(info) = map.get_mut(&id) else { return 0 };
                    if info.allow_internal_nav || info.url.starts_with("nightfall://") {
                        info.allow_internal_nav = false;
                        0
                    } else {
                        // A web page tried to open an internal page: refused.
                        state::log("navigation vers nightfall:// refusée depuis une page web");
                        1
                    }
                }
            }
        }

        fn resource_request_handler(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _request: Option<&mut Request>,
            _is_navigation: i32,
            _is_download: i32,
            _request_initiator: Option<&CefString>,
            _disable_default_handling: Option<&mut i32>,
        ) -> Option<ResourceRequestHandler> {
            match *self.kind {
                ClientKind::Tab { .. } => Some(NfResourceRequest::new(self.kind.clone())),
                ClientKind::Ui { .. } => None,
            }
        }

        fn on_open_urlfrom_tab(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            target_url: Option<&CefString>,
            target_disposition: WindowOpenDisposition,
            _user_gesture: i32,
        ) -> i32 {
            // Middle-click, Ctrl/Cmd+click, Shift+click on links: with Alloy
            // they would otherwise load in the same tab.
            let ClientKind::Tab { window_id, tab_id, private } = *self.kind else { return 0 };
            let url = target_url.map(|u| u.to_string()).unwrap_or_default();
            let d = target_disposition;
            let new_tab = d == WindowOpenDisposition::NEW_BACKGROUND_TAB || d == WindowOpenDisposition::NEW_FOREGROUND_TAB;
            let new_window = d == WindowOpenDisposition::NEW_WINDOW || d == WindowOpenDisposition::NEW_POPUP;
            if !(new_tab || new_window) {
                return 0;
            }
            if url.starts_with("nightfall://") {
                return 1;
            }
            let foreground = d == WindowOpenDisposition::NEW_FOREGROUND_TAB;
            on_ui(move || {
                if new_window {
                    windows::create_window(private, vec![url]);
                } else {
                    windows::open_from_tab(window_id, tab_id, &url, foreground);
                }
            });
            1
        }

        fn on_render_process_unresponsive(&self, browser: Option<&mut Browser>, callback: Option<&mut UnresponsiveProcessCallback>) -> i32 {
            let (ClientKind::Tab { window_id, tab_id, .. }, Some(cb)) = (&*self.kind, callback) else { return 0 };
            let id = browser.map(|b| b.identifier()).unwrap_or(0);
            super::browsing::page_unresponsive(*window_id, *tab_id, id, cb.clone());
            1
        }

        fn on_render_process_responsive(&self, _browser: Option<&mut Browser>) {
            if let ClientKind::Tab { window_id, tab_id, .. } = *self.kind {
                super::browsing::page_responsive(window_id, tab_id);
            }
        }

        fn on_render_process_terminated(
            &self,
            _browser: Option<&mut Browser>,
            _status: TerminationStatus,
            _error_code: i32,
            _error_string: Option<&CefString>,
        ) {
            if let ClientKind::Tab { window_id, tab_id, .. } = *self.kind {
                on_ui(move || {
                    windows::set_content_fullscreen(window_id, tab_id, false);
                    super::browsing::page_responsive(window_id, tab_id);
                    windows::update_tab(window_id, tab_id, |t| {
                        t.crashed = true;
                        t.loading = false;
                        t.audible = false;
                    });
                });
            }
        }
    }
}

wrap_resource_request_handler! {
    struct NfResourceRequest {
        kind: Arc<ClientKind>,
    }

    impl ResourceRequestHandler {
        fn on_before_resource_load(
            &self,
            browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            request: Option<&mut Request>,
            _callback: Option<&mut Callback>,
        ) -> ReturnValue {
            let (Some(browser), Some(request)) = (browser, request) else { return ReturnValue::CONTINUE };
            let url = cef_to_string(&request.url());
            let rtype = nf_shield::request_type_from_cef(request.resource_type().get_raw() as i32);
            if let Some((name, value)) = gpc_header(super::prefs::GPC.load(std::sync::atomic::Ordering::Relaxed), &url, rtype) {
                request.set_header_by_name(Some(&CefString::from(name)), Some(&CefString::from(value)), 1);
                GPC_SENT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                #[cfg(feature = "selftest")]
                if rtype == "main_frame" {
                    let back = cef_to_string(&request.header_by_name(Some(&CefString::from(name))));
                    state::log(&format!("selftest: en-tête {name} = {back:?} ({url})"));
                }
            }
            if rtype == "main_frame" {
                #[cfg(feature = "selftest")]
                state::log(&format!("selftest: on_before_resource_load (main_frame) {url}"));
                return ReturnValue::CONTINUE;
            }
            let id = browser.identifier();
            let page = state::browser_info(id).map(|i| i.url).unwrap_or_default();
            let source = frame.map(|f| cef_to_string(&f.url())).filter(|s| !s.is_empty()).unwrap_or_else(|| page.clone());
            if state::shield().should_block(&url, &source, rtype, &page) {
                *blocked_map().entry(id).or_default() += 1;
                return ReturnValue::CANCEL;
            }
            ReturnValue::CONTINUE
        }
    }
}

// ----------------------------------------------------------------- keyboard

// The native event type differs per platform in the generated bindings.
#[cfg(target_os = "windows")]
type OsEvent<'a> = Option<&'a mut cef::sys::MSG>;
#[cfg(target_os = "macos")]
type OsEvent<'a> = *mut u8;
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
type OsEvent<'a> = Option<&'a mut cef::sys::XEvent>;

const SHIFT: u32 = 1 << 1;
const CONTROL: u32 = 1 << 2;
const ALT: u32 = 1 << 3;
const COMMAND: u32 = 1 << 7;

wrap_keyboard_handler! {
    struct NfKeyboard {
        kind: Arc<ClientKind>,
    }

    impl KeyboardHandler {
        fn on_pre_key_event(
            &self,
            _browser: Option<&mut Browser>,
            event: Option<&KeyEvent>,
            _os_event: OsEvent<'_>,
            _is_keyboard_shortcut: Option<&mut i32>,
        ) -> i32 {
            let Some(e) = event else { return 0 };
            if e.type_ != KeyEventType::RAWKEYDOWN {
                return 0;
            }
            let window_id = match *self.kind {
                ClientKind::Ui { window_id, .. } | ClientKind::Tab { window_id, .. } => window_id,
            };
            super::idle::touch();
            let mac = cfg!(target_os = "macos");
            let primary = if mac { COMMAND } else { CONTROL };
            let m = e.modifiers;
            let has = |f: u32| m & f != 0;
            let key = e.windows_key_code;
            let p = has(primary);
            let shift = has(SHIFT);
            let alt = has(ALT);
            // Escape leaves a page's fullscreen (video).
            if key == 0x1B && !p && windows::content_fullscreen(window_id) {
                on_ui(move || windows::exit_content_fullscreen(window_id));
                return 1;
            }
            let action: Option<String> = match key {
                0x54 if p && shift => Some("tab.reopen".into()),            // T
                0x54 if p => Some("tab.new".into()),
                0x57 if p && shift => Some("window.close".into()),          // W
                0x57 if p => Some("tab.close".into()),
                0x73 if has(CONTROL) && !mac => Some("tab.close".into()),   // Ctrl+F4
                0x4C if p && shift => Some("lock".into()),                  // Shift+L
                0x4C if p => Some("focus.address".into()),                  // L
                0x44 if alt && !mac && !p => Some("focus.address".into()),  // Alt+D
                0x75 if !p && !mac => Some("focus.address".into()),         // F6
                0x4E if p && shift => Some("window.private".into()),        // Shift+N
                0x4E if p => Some("window.new".into()),                     // N
                0x52 if p && shift => Some("tab.reload.hard".into()),       // Shift+R
                0x52 if p => Some("tab.reload".into()),                     // R
                0x74 if shift || has(CONTROL) => Some("tab.reload.hard".into()), // Shift/Ctrl+F5
                0x74 => Some("tab.reload".into()),                          // F5
                0x09 if has(CONTROL) && shift => Some("tab.prev".into()),   // Ctrl+Shift+Tab
                0x09 if has(CONTROL) => Some("tab.next".into()),            // Ctrl+Tab
                0x22 if has(CONTROL) => Some("tab.next".into()),            // Ctrl+PgDown
                0x21 if has(CONTROL) => Some("tab.prev".into()),            // Ctrl+PgUp
                0xDD if p && shift => Some("tab.next".into()),              // Cmd+Shift+]
                0xDB if p && shift => Some("tab.prev".into()),              // Cmd+Shift+[
                0x27 if mac && p && alt => Some("tab.next".into()),         // Cmd+Option+Right
                0x25 if mac && p && alt => Some("tab.prev".into()),         // Cmd+Option+Left
                0x31..=0x38 if p && !shift && !alt => Some(format!("tab.index.{}", key - 0x31)), // 1..8
                0x39 if p && !shift && !alt => Some("tab.index.last".into()),  // 9
                // Alt+Left/Right (Windows only: on macOS Option+arrows move by word).
                0x25 if alt && !p && !mac => Some("tab.back".into()),
                0x27 if alt && !p && !mac => Some("tab.forward".into()),
                0xDB if mac && p => Some("tab.back".into()),                // Cmd+[
                0xDD if mac && p => Some("tab.forward".into()),             // Cmd+]
                0x44 if p => Some("bookmark.toggle".into()),                // D
                0x4F if p && shift => Some("page.bookmarks".into()),        // Shift+O
                0x42 if mac && p && alt => Some("page.bookmarks".into()),   // Cmd+Option+B
                0x48 if p && !mac => Some("page.history".into()),           // Ctrl+H
                0x59 if p && mac => Some("page.history".into()),            // Cmd+Y
                0x4A if p => Some("page.downloads".into()),                 // J
                0x46 if mac && has(COMMAND) && has(CONTROL) => Some("window.fullscreen".into()), // Ctrl+Cmd+F
                0x46 if p => Some("find".into()),                           // F
                0x72 if shift => Some("find.prev".into()),                  // Shift+F3
                0x72 => Some("find.next".into()),                           // F3
                0x47 if p && shift => Some("find.prev".into()),             // Shift+G
                0x47 if p => Some("find.next".into()),                      // G
                0xBB | 0x6B if p => Some("zoom.in".into()),                 // + / =
                0xBD | 0x6D if p => Some("zoom.out".into()),                // -
                0x30 | 0x60 if p => Some("zoom.reset".into()),              // 0
                0x50 if p && !shift => Some("print".into()),                // P
                0x53 if p && !shift => Some("save".into()),                 // S
                0x55 if p => Some("view.source".into()),                    // U (Cmd+Option+U too)
                0x7A if !mac => Some("window.fullscreen".into()),           // F11
                0x7B if p && shift => Some("emergency.lockdown".into()),    // Shift+F12
                0x7B if !p && !shift && !mac => Some("devtools".into()),    // F12
                0x49 if p && shift => Some("devtools".into()),              // Shift+I
                0x49 if mac && p && alt => Some("devtools".into()),         // Cmd+Option+I
                _ => None,
            };
            match action {
                Some(a) => {
                    on_ui(move || super::api::shortcut(window_id, &a));
                    1
                }
                None => 0,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn global_privacy_control_header() {
        assert_eq!(gpc_header(true, "https://example.com/", "main_frame"), Some(("Sec-GPC", "1")));
        assert_eq!(gpc_header(true, "http://example.com/frame", "sub_frame"), Some(("Sec-GPC", "1")));
        assert_eq!(gpc_header(true, "https://cdn.example/x.js", "script"), None);
        assert_eq!(gpc_header(false, "https://example.com/", "main_frame"), None);
        assert_eq!(gpc_header(true, "nightfall://app/settings.html", "main_frame"), None);
        assert_eq!(gpc_header(true, "data:text/html,x", "main_frame"), None);
    }
}

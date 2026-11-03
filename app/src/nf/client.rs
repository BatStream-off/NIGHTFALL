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
    }
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
                on_navigation(window_id, tab_id, private, &url);
            }
        }

        fn on_console_message(
            &self,
            _browser: Option<&mut Browser>,
            _level: LogSeverity,
            message: Option<&CefString>,
            source: Option<&CefString>,
            line: i32,
        ) -> i32 {
            // Only NIGHTFALL's own pages are logged (never web page content).
            let source = source.map(|s| s.to_string()).unwrap_or_default();
            if source.starts_with("nightfall://") {
                let msg = message.map(|m| m.to_string()).unwrap_or_default();
                state::log(&format!("console {source}:{line} {msg}"));
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
    if let Some(video) = nf_videoguard::video_id(url) {
        let cfg = core().videoguard.clone();
        if cfg.enabled {
            background(
                move || nf_videoguard::fetch_segments(&video, &cfg).map(|s| (video, s)),
                move |res| match res {
                    Ok((video, segments)) => {
                        let script = nf_videoguard::injection_script(&video, &segments);
                        windows::exec_in_tab(window_id, tab_id, &script);
                    }
                    Err(e) => state::log(&format!("VideoGuard: {e}")),
                },
            );
        }
    }
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
        }

        fn on_before_close(&self, browser: Option<&mut Browser>) {
            if let Some(b) = browser {
                state::browsers().remove(&b.identifier());
                blocked_map().remove(&b.identifier());
            }
        }

        fn on_before_popup(
            &self,
            browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _popup_id: i32,
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
            let ClientKind::Tab { window_id, private, .. } = *self.kind else {
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
            let _ = target_disposition;
            // Allowed: the popup becomes a new tab in the same window, with
            // its own client so it is tracked as a separate tab.
            let tab_id = windows::reserve_popup(window_id);
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
                });
            }
        }

        fn on_load_error(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            error_code: Errorcode,
            error_text: Option<&CefString>,
            failed_url: Option<&CefString>,
        ) {
            let code = { let raw: cef::sys::cef_errorcode_t = error_code.into(); raw as i32 };
            let text = error_text.map(|t| t.to_string()).unwrap_or_default();
            match *self.kind {
                ClientKind::Ui { .. } => {
                    let url = failed_url.map(|u| u.to_string()).unwrap_or_default();
                    state::log(&format!("interface : échec du chargement {url} ({code} {text})"));
                }
                // Web pages: the error only, never the address (privacy).
                ClientKind::Tab { .. } => state::log(&format!("onglet : échec du chargement ({code} {text})")),
            }
        }

        fn on_load_end(&self, browser: Option<&mut Browser>, frame: Option<&mut Frame>, http_status_code: i32) {
            let (Some(_browser), Some(frame)) = (browser, frame) else { return };
            if frame.is_main() == 0 {
                return;
            }
            if let ClientKind::Ui { .. } = *self.kind {
                state::log(&format!("interface chargée : {} (HTTP {http_status_code})", cef_to_string(&frame.url())));
                // Start-up diagnostic, written to the NIGHTFALL log through
                // on_console_message (only nightfall:// sources are logged).
                const DIAG: &str = "console.log('diag origin=' + location.origin + ' scripts=' + document.scripts.length + ' api=' + (typeof api) + ' styles=' + document.styleSheets.length);";
                frame.execute_java_script(Some(&CefString::from(DIAG)), Some(&CefString::from("nightfall://app/diag")), 0);
            }
            if let ClientKind::Tab { .. } = *self.kind {
                let url = cef_to_string(&frame.url());
                if !(url.starts_with("http://") || url.starts_with("https://")) {
                    return;
                }
                // Cosmetic filtering: hide ad containers.
                if let Some(c) = state::shield().cosmetic(&url) {
                    if !c.css.is_empty() {
                        let css = serde_json::to_string(&c.css).unwrap_or_default();
                        let js = format!(
                            "(function(){{var s=document.getElementById('nf-shield-css');if(!s){{s=document.createElement('style');s.id='nf-shield-css';(document.head||document.documentElement).appendChild(s);}}s.textContent={css};}})();"
                        );
                        frame.execute_java_script(Some(&CefString::from(js.as_str())), None, 0);
                    }
                }
            }
        }
    }
}

// ------------------------------------------------------------------ request

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

        fn on_render_process_terminated(
            &self,
            _browser: Option<&mut Browser>,
            _status: TerminationStatus,
            _error_code: i32,
            _error_string: Option<&CefString>,
        ) {
            if let ClientKind::Tab { window_id, tab_id, .. } = *self.kind {
                on_ui(move || {
                    windows::update_tab(window_id, tab_id, |t| {
                        t.crashed = true;
                        t.loading = false;
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
            let primary = if cfg!(target_os = "macos") { COMMAND } else { CONTROL };
            let m = e.modifiers;
            let has = |f: u32| m & f != 0;
            let key = e.windows_key_code;
            let action: Option<&str> = match key {
                0x54 if has(primary) && has(SHIFT) => Some("tab.reopen"),
                0x54 if has(primary) => Some("tab.new"),                 // T
                0x57 if has(primary) => Some("tab.close"),               // W
                0x4C if has(primary) && has(SHIFT) => Some("lock"),      // Shift+L
                0x4C if has(primary) => Some("focus.address"),           // L
                0x4E if has(primary) && has(SHIFT) => Some("window.private"), // Shift+N
                0x4E if has(primary) => Some("window.new"),              // N
                0x52 if has(primary) => Some("tab.reload"),              // R
                0x74 => Some("tab.reload"),                              // F5
                0x09 if has(CONTROL) && has(SHIFT) => Some("tab.prev"),  // Ctrl+Shift+Tab
                0x09 if has(CONTROL) => Some("tab.next"),                // Ctrl+Tab
                0x25 if has(ALT) => Some("tab.back"),                    // Alt+Left
                0x27 if has(ALT) => Some("tab.forward"),                 // Alt+Right
                0x44 if has(primary) => Some("bookmark.toggle"),         // D
                0x48 if has(primary) => Some("page.history"),            // H
                0x4A if has(primary) => Some("page.downloads"),          // J
                0x7B if has(primary) && has(SHIFT) => Some("emergency.lockdown"), // Shift+F12
                0x49 if has(primary) && has(SHIFT) => Some("devtools"),  // Shift+I
                _ => None,
            };
            match action {
                Some(a) => {
                    let a = a.to_string();
                    on_ui(move || super::api::shortcut(window_id, &a));
                    1
                }
                None => 0,
            }
        }
    }
}

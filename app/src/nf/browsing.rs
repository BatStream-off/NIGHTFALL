//! Everyday browser features of web tabs: find in page, page context menu,
//! zoom remembered per site, site icons, "playing sound" indicator,
//! unresponsive pages.

use super::client::ClientKind;
use super::state::core;
use super::ui_thread::on_ui_delayed;
use super::windows;
use cef::*;
use serde_json::json;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

fn cef_str(s: &CefStringUserfree) -> String {
    CefString::from(s).to_string()
}

fn is_web(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://")
}

// ------------------------------------------------------------- find in page

#[cfg(feature = "selftest")]
pub static LAST_FIND: std::sync::Mutex<Option<(i32, i32, bool)>> = std::sync::Mutex::new(None);

wrap_find_handler! {
    pub struct NfFind {
        window_id: u32,
    }

    impl FindHandler {
        fn on_find_result(
            &self,
            _browser: Option<&mut Browser>,
            _identifier: i32,
            count: i32,
            _selection_rect: Option<&Rect>,
            active_match_ordinal: i32,
            final_update: i32,
        ) {
            #[cfg(feature = "selftest")]
            if let Ok(mut f) = LAST_FIND.lock() {
                *f = Some((count, active_match_ordinal, final_update != 0));
            }
            windows::push_event(self.window_id, &json!({ "type": "find_result", "count": count, "active": active_match_ordinal, "final": final_update != 0 }));
        }
    }
}

/// Search the active tab. `next`: continue the current search.
pub fn find(window_id: u32, text: &str, forward: bool, match_case: bool, next: bool) {
    let Some(h) = windows::active_browser(window_id).and_then(|b| b.host()) else { return };
    if text.is_empty() {
        h.stop_finding(1);
        return;
    }
    h.find(Some(&CefString::from(text)), forward as i32, match_case as i32, next as i32);
}

pub fn stop_find(window_id: u32, clear: bool) {
    if let Some(h) = windows::active_browser(window_id).and_then(|b| b.host()) {
        h.stop_finding(clear as i32);
    }
}

/// Ctrl+F: open the find bar, or give it the focus again.
pub fn open_find_bar(window_id: u32) {
    if windows::content_fullscreen(window_id) {
        windows::exit_content_fullscreen(window_id);
    }
    if windows::popup_panel(window_id).as_deref() == Some("find") {
        windows::popup_focus(window_id);
        windows::push_event(window_id, &json!({ "type": "find_focus" }));
        return;
    }
    windows::popup_open_under_toolbar(window_id, "find", 400, 50);
}

/// F3 / Shift+F3 (Cmd+G / Shift+Cmd+G): next or previous match.
pub fn find_again(window_id: u32, back: bool) {
    if windows::popup_panel(window_id).as_deref() == Some("find") {
        windows::push_event(window_id, &json!({ "type": "find_next", "back": back }));
    } else {
        open_find_bar(window_id);
    }
}

// --------------------------------------------------------------------- zoom

const ZOOM_KEY: &str = "site_zoom";

fn host_of(url: &str) -> Option<String> {
    if !is_web(url) {
        return None;
    }
    url::Url::parse(url).ok()?.host_str().map(str::to_string)
}

fn saved_zooms() -> HashMap<String, f64> {
    core().storage().and_then(|s| s.setting::<HashMap<String, f64>>(ZOOM_KEY).ok().flatten()).unwrap_or_default()
}

/// Remember the zoom of a site (normal windows only; 0 = default, forgotten).
fn save_zoom(url: &str, level: f64) {
    let Some(host) = host_of(url) else { return };
    let mut map = saved_zooms();
    let changed = if level.abs() < 0.001 { map.remove(&host).is_some() } else { map.insert(host, level) != Some(level) };
    if changed {
        if let Some(s) = core().storage() {
            let _ = s.set_setting(ZOOM_KEY, &map);
        }
    }
}

/// Apply the zoom remembered for the site just opened in a tab.
pub fn apply_site_zoom(browser: &Browser, url: &str) {
    let Some(host) = host_of(url) else { return };
    let Some(level) = saved_zooms().get(&host).copied() else { return };
    if let Some(h) = browser.host() {
        if (h.zoom_level() - level).abs() > 0.001 {
            h.set_zoom_level(level);
        }
    }
}

/// Zoom the active tab: "in", "out" or "reset".
pub fn zoom(window_id: u32, command: &str) {
    let Some(h) = windows::active_browser(window_id).and_then(|b| b.host()) else { return };
    // Chromium zooms per host: NIGHTFALL's pages share theirs with the
    // toolbar, which must keep its size.
    if windows::active_url(window_id).unwrap_or_default().starts_with("nightfall://") {
        return;
    }
    match command {
        "in" => h.zoom(ZoomCommand::IN),
        "out" => h.zoom(ZoomCommand::OUT),
        _ => h.set_zoom_level(0.0),
    }
    // The new level may be applied a little later (macOS): look again.
    for delay in [30, 250, 800] {
        on_ui_delayed(delay, move || check_zoom(window_id));
    }
}

/// Notice zoom changes (also Chromium's own Ctrl+wheel zoom): update the
/// indicator and remember the level for the site.
fn check_zoom(window_id: u32) {
    let Some(tab_id) = windows::active_tab_id(window_id) else { return };
    let Some(level) = windows::active_browser(window_id).and_then(|b| b.host()).map(|h| h.zoom_level()) else { return };
    if level.abs() > 0.001 && windows::active_url(window_id).unwrap_or_default().starts_with("nightfall://") {
        // NIGHTFALL's own pages (and so the toolbar) stay at 100 %.
        if let Some(h) = windows::active_browser(window_id).and_then(|b| b.host()) {
            h.set_zoom_level(0.0);
        }
        return;
    }
    let info = windows::with_window(window_id, |ctx| {
        let private = ctx.private;
        let t = ctx.tabs.iter_mut().find(|t| t.id == tab_id)?;
        if (t.zoom_level - level).abs() < 0.001 {
            return None;
        }
        t.zoom_level = level;
        Some((t.url.clone(), private))
    })
    .flatten();
    if let Some((url, private)) = info {
        if !private {
            save_zoom(&url, level);
        }
        windows::push_tabs(window_id);
    }
}

/// Every 1.5 s while unlocked.
pub fn start_zoom_watch() {
    on_ui_delayed(1500, || {
        if core().is_unlocked() {
            for id in windows::window_ids() {
                check_zoom(id);
            }
        }
        start_zoom_watch();
    });
}

// ---------------------------------------------------------------- favicons

wrap_download_image_callback! {
    struct NfFavicon {
        window_id: u32,
        tab_id: u32,
        page_host: String,
    }

    impl DownloadImageCallback {
        fn on_download_image_finished(&self, _image_url: Option<&CefString>, _http_status_code: i32, image: Option<&mut Image>) {
            let Some(image) = image else { return };
            if image.is_empty() != 0 {
                return;
            }
            let (mut w, mut h) = (0, 0);
            let Some(png) = image.as_png(1.0, 1, Some(&mut w), Some(&mut h)) else { return };
            let n = png.size();
            if n == 0 || n > 96 * 1024 {
                return;
            }
            let mut buf = vec![0u8; n];
            png.data(Some(&mut buf), 0);
            use base64::Engine;
            let data = format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(&buf));
            let (window_id, tab_id, page_host) = (self.window_id, self.tab_id, self.page_host.clone());
            windows::update_tab(window_id, tab_id, move |t| {
                // Ignore a late icon of the previous site.
                if host_of(&t.url).unwrap_or_default() == page_host {
                    t.favicon = Some(data);
                }
            });
        }
    }
}

/// The page announced its icons: download the first one through the tab's
/// own browser (same network context and Shield rules, no cookies sent).
pub fn favicon_changed(window_id: u32, tab_id: u32, browser: &Browser, urls: Vec<String>) {
    let page = browser.main_frame().map(|f| cef_str(&f.url())).unwrap_or_default();
    let Some(page_host) = host_of(&page) else { return };
    let Some(url) = urls.into_iter().find(|u| is_web(u) || u.starts_with("data:image/")) else { return };
    let Some(h) = browser.host() else { return };
    let mut cb = NfFavicon::new(window_id, tab_id, page_host);
    h.download_image(Some(&CefString::from(url.as_str())), 1, 32, 0, Some(&mut cb));
}

/// Main-frame navigation: forget the icon when the site changes.
pub fn on_main_navigation(window_id: u32, tab_id: u32, browser_id: i32, url: &str) {
    MEDIA.with(|m| m.borrow_mut().remove(&browser_id));
    let host = host_of(url);
    windows::update_tab(window_id, tab_id, move |t| {
        if t.favicon.is_some() && host_of(&t.url) != host {
            t.favicon = None;
        }
        t.audible = false;
    });
}

// ------------------------------------------------------ sound indicator

/// Prefix of the reports of the media script (random per session).
pub fn media_token() -> String {
    static TOKEN: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    TOKEN
        .get_or_init(|| {
            let r = nf_crypto::random_bytes(8).unwrap_or_else(|_| vec![9; 8]);
            format!("\u{1}nfmed{}:", nf_crypto::hex(&r))
        })
        .clone()
}

/// Script injected in every web frame: reports whether a <video>/<audio>
/// element plays with sound. A page can only fake its own tab's indicator.
pub fn media_script() -> String {
    let token = serde_json::to_string(&media_token()).unwrap_or_default();
    format!(
        "(function(){{if(window.__nfMed)return;window.__nfMed=1;var T={token};var id=Math.random().toString(36).slice(2,10);var last=false;\
function check(){{var on=false;var m=document.querySelectorAll('video,audio');for(var i=0;i<m.length;i++){{var e=m[i];if(!e.paused&&!e.ended&&!e.muted&&e.volume>0){{on=true;break;}}}}\
if(on!==last){{last=on;console.debug(T+id+':'+(on?1:0));}}}}\
['play','playing','pause','ended','volumechange','emptied','abort'].forEach(function(t){{document.addEventListener(t,function(){{setTimeout(check,0);}},true);}});\
window.addEventListener('pagehide',function(){{if(last){{last=false;console.debug(T+id+':0');}}}});}})();"
    )
}

thread_local! {
    /// Browser id → frames currently playing sound.
    static MEDIA: RefCell<HashMap<i32, HashSet<String>>> = RefCell::new(HashMap::new());
}

/// A report of the media script. Returns true if the message was one.
pub fn media_report(window_id: u32, tab_id: u32, browser_id: i32, msg: &str) -> bool {
    let Some(rest) = msg.strip_prefix(media_token().as_str()) else { return false };
    let Some((frame, on)) = rest.rsplit_once(':') else { return true };
    if frame.len() > 16 {
        return true;
    }
    let audible = MEDIA.with(|m| {
        let mut m = m.borrow_mut();
        let set = m.entry(browser_id).or_default();
        if on == "1" {
            set.insert(frame.to_string());
        } else {
            set.remove(frame);
        }
        !set.is_empty()
    });
    windows::update_tab(window_id, tab_id, move |t| t.audible = audible);
    true
}

pub fn forget_browser(browser_id: i32) {
    MEDIA.with(|m| m.borrow_mut().remove(&browser_id));
    UNRESPONSIVE.with(|u| u.borrow_mut().retain(|_, v| v.0 != browser_id));
}

// --------------------------------------------------------- unresponsive

thread_local! {
    static UNRESPONSIVE: RefCell<HashMap<u32, (i32, UnresponsiveProcessCallback)>> = RefCell::new(HashMap::new());
}

pub fn page_unresponsive(window_id: u32, tab_id: u32, browser_id: i32, callback: UnresponsiveProcessCallback) {
    UNRESPONSIVE.with(|u| u.borrow_mut().insert(tab_id, (browser_id, callback)));
    windows::push_event(window_id, &json!({ "type": "unresponsive", "tab": tab_id, "on": true }));
}

pub fn page_responsive(window_id: u32, tab_id: u32) {
    let had = UNRESPONSIVE.with(|u| u.borrow_mut().remove(&tab_id).is_some());
    if had {
        windows::push_event(window_id, &json!({ "type": "unresponsive", "tab": tab_id, "on": false }));
    }
}

/// The user chose to wait for, or to stop, an unresponsive page.
pub fn answer_unresponsive(window_id: u32, tab_id: u32, terminate: bool) {
    if let Some((_, cb)) = UNRESPONSIVE.with(|u| u.borrow_mut().remove(&tab_id)) {
        if terminate {
            cb.terminate();
        } else {
            cb.wait();
        }
    }
    windows::push_event(window_id, &json!({ "type": "unresponsive", "tab": tab_id, "on": false }));
}

// ------------------------------------------------------------ context menu

const CMD_LINK_TAB: i32 = 26500;
const CMD_LINK_PRIVATE: i32 = 26501;
const CMD_LINK_COPY: i32 = 26502;
const CMD_IMG_TAB: i32 = 26503;
const CMD_IMG_SAVE: i32 = 26504;
const CMD_IMG_COPY: i32 = 26505;
const CMD_SEARCH: i32 = 26506;
const CMD_SAVE_PAGE: i32 = 26507;
const CMD_VIEW_SOURCE: i32 = 26508;
const CMD_INSPECT: i32 = 26509;
const CMD_COPY_PAGE_URL: i32 = 26510;

// Flags are u32 on macOS and i32 on Windows in the bindings.
// Chromium's built-in commands (cef_menu_id_t).
const ID_BACK: i32 = 100;
const ID_FORWARD: i32 = 101;
const ID_RELOAD: i32 = 102;
const ID_PRINT: i32 = 131;
const ID_VIEW_SOURCE: i32 = 132;
const ID_FIND: i32 = 130;

const TYPE_LINK: i64 = 4;
const TYPE_MEDIA: i64 = 8;
const TYPE_SELECTION: i64 = 16;
const TYPE_EDITABLE: i64 = 32;

fn label(s: &str) -> CefString {
    CefString::from(s)
}

/// Links a page may open through the menu (never internal pages or scripts).
fn openable(url: &str) -> bool {
    is_web(url) || url.starts_with("ftp://")
}

fn short(text: &str, max: usize) -> String {
    let t: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if t.chars().count() > max {
        t.chars().take(max).collect::<String>() + "…"
    } else {
        t
    }
}

fn strip_separators(model: &MenuModel) {
    // Leading, trailing and doubled separators left by removed items.
    let is_sep = |i: usize| model.type_at(i) == MenuItemType::SEPARATOR;
    while model.count() > 0 && is_sep(0) {
        model.remove_at(0);
    }
    while model.count() > 0 && is_sep(model.count() - 1) {
        model.remove_at(model.count() - 1);
    }
    let mut i = 1;
    while i < model.count() {
        if is_sep(i) && is_sep(i - 1) {
            model.remove_at(i);
        } else {
            i += 1;
        }
    }
}

wrap_context_menu_handler! {
    pub struct NfContextMenu {
        kind: Arc<ClientKind>,
    }

    impl ContextMenuHandler {
        fn on_before_context_menu(
            &self,
            browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            params: Option<&mut ContextMenuParams>,
            model: Option<&mut MenuModel>,
        ) {
            let (Some(params), Some(model)) = (params, model) else { return };
            let flags = { let raw: cef::sys::cef_context_menu_type_flags_t = params.type_flags().into(); raw.0 as i64 };
            let editable = flags & TYPE_EDITABLE != 0;
            match *self.kind {
                ClientKind::Ui { .. } => {
                    // NIGHTFALL's interface: editing commands in text fields
                    // only (no "view source" of the interface).
                    if !editable {
                        model.clear();
                        return;
                    }
                    for id in [ID_BACK, ID_FORWARD, ID_RELOAD, 103, 104, ID_FIND, ID_PRINT, ID_VIEW_SOURCE] {
                        model.remove(id);
                    }
                    strip_separators(model);
                }
                ClientKind::Tab { .. } => {
                    let link = flags & TYPE_LINK != 0 && openable(&cef_str(&params.link_url()));
                    let image = flags & TYPE_MEDIA != 0 && params.media_type() == ContextMenuMediaType::IMAGE && {
                        let src = cef_str(&params.source_url());
                        is_web(&src) || src.starts_with("data:image/")
                    };
                    let selection = cef_str(&params.selection_text());
                    let has_selection = flags & TYPE_SELECTION != 0 && !selection.trim().is_empty();
                    let devtools = core().settings.devtools_enabled;
                    let engine = super::omnibox::engine_name(&core().settings.search_engine);
                    model.remove(ID_VIEW_SOURCE);
                    if !editable && !link && !image && !has_selection {
                        // Page menu.
                        let (back, fwd) = browser.map(|b| (b.can_go_back() != 0, b.can_go_forward() != 0)).unwrap_or((false, false));
                        model.clear();
                        model.add_item(ID_BACK, Some(&label("Précédent")));
                        model.set_enabled(ID_BACK, back as i32);
                        model.add_item(ID_FORWARD, Some(&label("Suivant")));
                        model.set_enabled(ID_FORWARD, fwd as i32);
                        model.add_item(ID_RELOAD, Some(&label("Actualiser")));
                        model.add_separator();
                        model.add_item(CMD_SAVE_PAGE, Some(&label("Enregistrer la page")));
                        model.add_item(ID_PRINT, Some(&label("Imprimer…")));
                        model.add_item(CMD_COPY_PAGE_URL, Some(&label("Copier l'adresse de la page")));
                        model.add_separator();
                        model.add_item(CMD_VIEW_SOURCE, Some(&label("Afficher la source de la page")));
                    } else {
                        let mut i = 0;
                        if link {
                            model.insert_item_at(i, CMD_LINK_TAB, Some(&label("Ouvrir le lien dans un nouvel onglet")));
                            model.insert_item_at(i + 1, CMD_LINK_PRIVATE, Some(&label("Ouvrir le lien dans une fenêtre privée")));
                            model.insert_item_at(i + 2, CMD_LINK_COPY, Some(&label("Copier l'adresse du lien")));
                            i += 3;
                        }
                        if image {
                            if i > 0 {
                                model.insert_separator_at(i);
                                i += 1;
                            }
                            model.insert_item_at(i, CMD_IMG_TAB, Some(&label("Ouvrir l'image dans un nouvel onglet")));
                            model.insert_item_at(i + 1, CMD_IMG_SAVE, Some(&label("Enregistrer l'image (Téléchargements)")));
                            model.insert_item_at(i + 2, CMD_IMG_COPY, Some(&label("Copier l'adresse de l'image")));
                            i += 3;
                        }
                        if i > 0 && model.count() > i {
                            model.insert_separator_at(i);
                        }
                        if has_selection {
                            let text = format!("Rechercher « {} » avec {engine}", short(&selection, 28));
                            let copy = model.index_of(113);
                            if copy >= 0 {
                                model.insert_item_at(copy as usize + 1, CMD_SEARCH, Some(&label(&text)));
                            } else {
                                model.add_separator();
                                model.add_item(CMD_SEARCH, Some(&label(&text)));
                            }
                        }
                    }
                    if devtools {
                        model.add_separator();
                        model.add_item(CMD_INSPECT, Some(&label("Inspecter")));
                    }
                    strip_separators(model);
                }
            }
        }

        fn on_context_menu_command(
            &self,
            browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            params: Option<&mut ContextMenuParams>,
            command_id: i32,
            _event_flags: EventFlags,
        ) -> i32 {
            let ClientKind::Tab { window_id, tab_id, .. } = *self.kind else { return 0 };
            let (Some(params), Some(browser)) = (params, browser) else { return 0 };
            if !(CMD_LINK_TAB..=CMD_COPY_PAGE_URL).contains(&command_id) {
                return 0;
            }
            let link = cef_str(&params.link_url());
            let src = cef_str(&params.source_url());
            let page = browser.main_frame().map(|f| cef_str(&f.url())).unwrap_or_default();
            let selection = cef_str(&params.selection_text());
            let (x, y) = (params.xcoord(), params.ycoord());
            // Run after the menu has closed (no nested work in CEF's callback).
            on_ui_delayed(0, move || run_command(window_id, tab_id, command_id, &link, &src, &page, &selection, x, y));
            1
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn run_command(window_id: u32, tab_id: u32, cmd: i32, link: &str, src: &str, page: &str, selection: &str, x: i32, y: i32) {
    let host = windows::tab_browser(window_id, tab_id).and_then(|b| b.host());
    match cmd {
        CMD_LINK_TAB if openable(link) => {
            windows::open_from_tab(window_id, tab_id, link, false);
        }
        CMD_LINK_PRIVATE if openable(link) => windows::create_window(true, vec![link.to_string()]),
        CMD_LINK_COPY => windows::copy_text(window_id, link),
        CMD_IMG_TAB if openable(src) || src.starts_with("data:image/") => {
            windows::open_from_tab(window_id, tab_id, src, false);
        }
        CMD_IMG_SAVE => {
            if let Some(h) = host {
                h.start_download(Some(&CefString::from(src)));
            }
        }
        CMD_IMG_COPY => windows::copy_text(window_id, src),
        CMD_SEARCH => {
            let url = core().settings.search_url(selection.trim());
            windows::open_from_tab(window_id, tab_id, &url, true);
        }
        CMD_SAVE_PAGE => save_page(window_id, page),
        CMD_COPY_PAGE_URL => windows::copy_text(window_id, page),
        CMD_VIEW_SOURCE => view_source(window_id, tab_id, page),
        CMD_INSPECT => {
            if core().settings.devtools_enabled {
                if let Some(h) = host {
                    let wi = WindowInfo::default();
                    let settings = BrowserSettings::default();
                    h.show_dev_tools(Some(&wi), None, Some(&settings), Some(&Point { x, y }));
                }
            }
        }
        _ => {}
    }
}

/// Ctrl+S: download the page itself into the Downloads folder.
pub fn save_page(window_id: u32, url: &str) {
    if !is_web(url) {
        return;
    }
    if let Some(h) = windows::active_browser(window_id).and_then(|b| b.host()) {
        h.start_download(Some(&CefString::from(url)));
    }
}

/// Ctrl+U: the page's source in a new tab.
pub fn view_source(window_id: u32, tab_id: u32, url: &str) {
    if !is_web(url) {
        return;
    }
    windows::open_from_tab(window_id, tab_id, &format!("view-source:{url}"), true);
}

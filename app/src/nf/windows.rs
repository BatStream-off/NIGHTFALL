//! Windows, tabs and layout (UI thread only).
//!
//! Layout of a NIGHTFALL window (CEF Views):
//! ```text
//! Window (horizontal box)
//! ├─ side  : BrowserView nightfall://app/shell.html?mode=side   (vertical tabs, optional)
//! └─ column: Panel (vertical box)
//!    ├─ top    : BrowserView nightfall://app/shell.html?mode=top (tabs + toolbar, or full-window lock screen)
//!    └─ content: Panel (fill layout) holding one BrowserView per tab, only the active one visible
//! ```

use super::client::{make_client, ClientKind};
use super::state::{self, core};
use super::ui_thread::on_ui;
use cef::*;
use serde::Serialize;
use std::cell::RefCell;
use std::sync::atomic::{AtomicI32, AtomicU32, Ordering};
use std::sync::Arc;

pub const TOP_HEIGHT: i32 = 88;
/// Window and taskbar icon (256 px, transparent background).
const WINDOW_ICON: &[u8] = include_bytes!("../../resources/window-icon.png");
pub const TOP_HEIGHT_VERTICAL: i32 = 52;
/// Height of the tab strip (title bar band of the frameless window).
pub const TABSTRIP_HEIGHT: i32 = 38;
pub const SIDE_WIDTH: i32 = 260;
pub const NEW_TAB_URL: &str = "nightfall://app/newtab.html";

static NEXT_WINDOW: AtomicU32 = AtomicU32::new(1);
static NEXT_TAB: AtomicU32 = AtomicU32::new(1);
/// Instance number of each dropdown popup (see `popup_close_if`).
static NEXT_POPUP: AtomicU32 = AtomicU32::new(1);

pub fn next_tab_id() -> u32 {
    NEXT_TAB.fetch_add(1, Ordering::Relaxed)
}

pub struct Tab {
    pub id: u32,
    pub view: Option<BrowserView>,
    pub url: String,
    pub title: String,
    pub loading: bool,
    pub can_back: bool,
    pub can_forward: bool,
    pub pinned: bool,
    pub last_active: i64,
    pub warning: Option<String>,
    pub discarded: bool,
    pub crashed: bool,
    /// Site icon as a data: URL (downloaded by the tab's own browser).
    pub favicon: Option<String>,
    /// A media element of the page is playing with sound.
    pub audible: bool,
    /// Tab that opened this one (links, popups): reactivated when a popup
    /// closes itself (OAuth login windows).
    pub opener: Option<u32>,
    /// Last zoom level seen (Ctrl+wheel changes are noticed by polling).
    pub zoom_level: f64,
}

impl Tab {
    fn new(id: u32, view: Option<BrowserView>, url: &str, title: &str) -> Tab {
        Tab {
            id,
            view,
            url: url.to_string(),
            title: title.to_string(),
            loading: true,
            can_back: false,
            can_forward: false,
            pinned: false,
            last_active: now(),
            warning: None,
            discarded: false,
            crashed: false,
            favicon: None,
            audible: false,
            opener: None,
            zoom_level: 0.0,
        }
    }
}

/// A tab closed by the user, for "Rouvrir l'onglet fermé" (normal windows only).
pub struct ClosedTab {
    pub url: String,
    pub index: usize,
}

/// A popup announced by on_before_popup, adopted by on_popup_browser_view_created.
pub struct PendingPopup {
    pub popup_id: i32,
    pub tab_id: u32,
    pub opener: Option<u32>,
    pub background: bool,
}

pub struct WinCtx {
    pub id: u32,
    pub private: bool,
    pub window: Option<Window>,
    pub top: BrowserView,
    pub side: BrowserView,
    pub top_height: Arc<AtomicI32>,
    pub side_width: Arc<AtomicI32>,
    pub column: Panel,
    pub content: Panel,
    pub win_layout: Option<BoxLayout>,
    pub col_layout: Option<BoxLayout>,
    pub tabs: Vec<Tab>,
    pub active: Option<u32>,
    pub request_context: Option<RequestContext>,
    /// Tab ids reserved in on_before_popup, consumed by on_popup_browser_view_created.
    pub pending_popups: Vec<PendingPopup>,
    /// URLs to open once the window exists and NIGHTFALL is unlocked.
    pub initial_urls: Vec<String>,
    /// Dropdown popup (menu, Shield, passwords) shown over the page.
    pub popup: Option<Popup>,
    /// Recently closed tabs (most recent last), never for private windows.
    pub closed: Vec<ClosedTab>,
    /// Tab whose page is in HTML5 fullscreen (video...): the interface is hidden.
    pub fullscreen_tab: Option<u32>,
    /// The window was already fullscreen (F11) when the page went fullscreen.
    pub window_was_fullscreen: bool,
}

pub struct Popup {
    pub panel: String,
    /// Instance number, passed to the popup page (`&n=`): a late close
    /// request from a popup never closes the one opened after it.
    pub serial: u32,
    pub view: BrowserView,
    pub overlay: OverlayController,
    /// Horizontal anchor (right edge, or left edge with `align_left`) and
    /// top of the popup, in window coordinates.
    pub right: i32,
    pub top: i32,
    pub align_left: bool,
    pub max_width: i32,
    /// Browser searched by the find bar (highlights cleared on close).
    pub target: Option<Browser>,
}

/// How a dropdown popup behaves.
#[derive(Clone, Copy)]
pub struct PopupOpts {
    /// Takes keyboard focus (false: the omnibox suggestions, which must leave
    /// the focus in the address field).
    pub activate: bool,
    /// Opening the panel already shown closes it.
    pub toggle: bool,
    /// Anchored by its left edge (context menus, suggestions).
    pub align_left: bool,
    /// The anchor is in the side view's coordinates (vertical tabs).
    pub from_side: bool,
    pub max_width: i32,
}

impl Default for PopupOpts {
    fn default() -> Self {
        PopupOpts { activate: true, toggle: true, align_left: false, from_side: false, max_width: 520 }
    }
}

thread_local! {
    static WINDOWS: RefCell<Vec<WinCtx>> = const { RefCell::new(Vec::new()) };
}

/// Access the window list. CEF may call back into NIGHTFALL synchronously
/// while the list is being used (layout, browser creation...): such a nested
/// access is refused (logged with its location) instead of panicking.
#[track_caller]
fn windows_mut<R>(f: impl FnOnce(&mut Vec<WinCtx>) -> R) -> Option<R> {
    let caller = std::panic::Location::caller();
    WINDOWS.with(|w| match w.try_borrow_mut() {
        Ok(mut g) => Some(f(&mut g)),
        Err(_) => {
            state::log(&format!("accès réentrant aux fenêtres refusé ({}:{})", caller.file(), caller.line()));
            None
        }
    })
}

/// True while the window list is in use higher up the stack.
fn windows_busy() -> bool {
    WINDOWS.with(|w| w.try_borrow_mut().is_err())
}

#[track_caller]
pub fn with_window<R>(id: u32, f: impl FnOnce(&mut WinCtx) -> R) -> Option<R> {
    windows_mut(|v| v.iter_mut().find(|c| c.id == id).map(f)).flatten()
}

pub fn window_ids() -> Vec<u32> {
    windows_mut(|v| v.iter().map(|c| c.id).collect()).unwrap_or_default()
}

fn now() -> i64 {
    nf_core::unix_now()
}

// ---------------------------------------------------------------- delegates

wrap_window_delegate! {
    struct NfWindowDelegate {
        window_id: u32,
    }

    impl ViewDelegate {
        fn preferred_size(&self, _view: Option<&mut View>) -> Size {
            Size { width: 1280, height: 820 }
        }

        fn minimum_size(&self, _view: Option<&mut View>) -> Size {
            Size { width: 640, height: 420 }
        }
    }

    impl PanelDelegate {}

    impl WindowDelegate {
        fn on_window_created(&self, window: Option<&mut Window>) {
            if let Some(window) = window {
                on_window_created(self.window_id, window.clone());
            }
        }

        fn on_window_destroyed(&self, _window: Option<&mut Window>) {
            on_window_destroyed(self.window_id);
        }

        fn can_close(&self, _window: Option<&mut Window>) -> i32 {
            prepare_window_close(self.window_id);
            1
        }

        // CEF's Rust bindings answer "no" by default: without these the
        // window cannot be resized, maximised or minimised, and macOS hides
        // its standard buttons.
        fn can_resize(&self, _window: Option<&mut Window>) -> i32 {
            1
        }

        fn can_maximize(&self, _window: Option<&mut Window>) -> i32 {
            1
        }

        fn can_minimize(&self, _window: Option<&mut Window>) -> i32 {
            1
        }

        fn with_standard_window_buttons(&self, _window: Option<&mut Window>) -> i32 {
            1
        }

        // Integrated title bar (prototype M-0005): no system frame; the
        // interface marks the draggable areas (see titlebar.rs).
        fn is_frameless(&self, _window: Option<&mut Window>) -> i32 {
            super::titlebar::FRAMELESS as i32
        }

        // macOS: centre the traffic lights in the tab strip.
        fn titlebar_height(&self, _window: Option<&mut Window>, titlebar_height: Option<&mut f32>) -> i32 {
            if !cfg!(target_os = "macos") || !super::titlebar::FRAMELESS {
                return 0;
            }
            match titlebar_height {
                Some(h) => {
                    *h = super::titlebar::titlebar_height();
                    1
                }
                None => 0,
            }
        }

        fn on_window_bounds_changed(&self, _window: Option<&mut Window>, _new_bounds: Option<&Rect>) {
            super::titlebar::push_state(self.window_id);
        }

        fn on_window_activation_changed(&self, _window: Option<&mut Window>, _active: i32) {
            super::titlebar::push_state(self.window_id);
        }

        fn window_runtime_style(&self) -> RuntimeStyle {
            RuntimeStyle::ALLOY
        }

        fn on_window_fullscreen_transition(&self, window: Option<&mut Window>, is_completed: i32) {
            // Leaving window fullscreen by other means (macOS green button,
            // system shortcut) also ends a page's fullscreen.
            if is_completed == 0 {
                return;
            }
            super::titlebar::push_state(self.window_id);
            let still = window.map(|w| w.is_fullscreen() != 0).unwrap_or(false);
            if !still {
                let id = self.window_id;
                super::ui_thread::on_ui_delayed(0, move || exit_content_fullscreen(id));
            }
        }
    }
}

wrap_browser_view_delegate! {
    struct UiViewDelegate {
        height: Arc<AtomicI32>,
        width: Arc<AtomicI32>,
    }

    impl ViewDelegate {
        fn preferred_size(&self, _view: Option<&mut View>) -> Size {
            // CEF ignores a preferred size with a zero dimension (it is
            // "empty") and falls back to the view's current size, which
            // stopped the web content area from ever getting any room.
            // The cross axis is stretched by the layout anyway.
            Size { width: self.width.load(Ordering::Relaxed).max(1), height: self.height.load(Ordering::Relaxed).max(1) }
        }
    }

    impl BrowserViewDelegate {
        fn chrome_toolbar_type(&self, _browser_view: Option<&mut BrowserView>) -> ChromeToolbarType {
            ChromeToolbarType::NONE
        }

        // CEF allows only one Chrome-style BrowserView per window; NIGHTFALL
        // needs several (interface + one per tab), so all use Alloy style.
        fn browser_runtime_style(&self) -> RuntimeStyle {
            RuntimeStyle::ALLOY
        }
    }
}

wrap_browser_view_delegate! {
    struct TabViewDelegate {
        window_id: u32,
    }

    impl ViewDelegate {}

    impl BrowserViewDelegate {
        fn chrome_toolbar_type(&self, _browser_view: Option<&mut BrowserView>) -> ChromeToolbarType {
            ChromeToolbarType::NONE
        }

        fn browser_runtime_style(&self) -> RuntimeStyle {
            RuntimeStyle::ALLOY
        }

        fn on_popup_browser_view_created(
            &self,
            _browser_view: Option<&mut BrowserView>,
            popup_browser_view: Option<&mut BrowserView>,
            is_devtools: i32,
        ) -> i32 {
            let Some(popup) = popup_browser_view else { return 0 };
            if is_devtools != 0 {
                // DevTools get their own top-level window.
                let mut d = DevToolsWindowDelegate::new(popup.clone());
                window_create_top_level(Some(&mut d));
                return 1;
            }
            adopt_popup(self.window_id, popup.clone());
            1
        }
    }
}

wrap_window_delegate! {
    struct DevToolsWindowDelegate {
        view: BrowserView,
    }

    impl ViewDelegate {
        fn preferred_size(&self, _view: Option<&mut View>) -> Size {
            Size { width: 1000, height: 700 }
        }
    }

    impl PanelDelegate {}

    impl WindowDelegate {
        fn on_window_created(&self, window: Option<&mut Window>) {
            if let Some(window) = window {
                let mut v = View::from(&self.view);
                window.add_child_view(Some(&mut v));
                window.set_title(Some(&CefString::from("NIGHTFALL · Outils de développement")));
                window.show();
            }
        }

        fn window_runtime_style(&self) -> RuntimeStyle {
            RuntimeStyle::ALLOY
        }

        fn can_resize(&self, _window: Option<&mut Window>) -> i32 {
            1
        }

        fn can_maximize(&self, _window: Option<&mut Window>) -> i32 {
            1
        }

        fn can_minimize(&self, _window: Option<&mut Window>) -> i32 {
            1
        }

        fn with_standard_window_buttons(&self, _window: Option<&mut Window>) -> i32 {
            1
        }

        fn can_close(&self, _window: Option<&mut Window>) -> i32 {
            if let Some(b) = self.view.browser() {
                if let Some(h) = b.host() {
                    return h.try_close_browser();
                }
            }
            1
        }
    }
}

// ---------------------------------------------------------------- windows

fn ui_view(window_id: u32, mode: &str, private: bool, height: Arc<AtomicI32>, width: Arc<AtomicI32>) -> BrowserView {
    let url = format!(
        "nightfall://app/shell.html?mode={mode}&w={window_id}&private={}",
        if private { 1 } else { 0 }
    );
    ui_view_url(window_id, mode == "side", &url, height, width)
}

fn ui_view_url(window_id: u32, side: bool, url: &str, height: Arc<AtomicI32>, width: Arc<AtomicI32>) -> BrowserView {
    let mut client = make_client(ClientKind::Ui { window_id, side });
    let mut delegate = UiViewDelegate::new(height, width);
    let settings = BrowserSettings { background_color: 0xFF0A0C10, ..Default::default() };
    browser_view_create(
        Some(&mut client),
        Some(&CefString::from(url)),
        Some(&settings),
        None,
        None,
        Some(&mut delegate),
    )
    .expect("NIGHTFALL UI view")
}

/// Open a new window. `urls` are opened as tabs once unlocked.
pub fn create_window(private: bool, urls: Vec<String>) {
    on_ui(move || {
        let id = NEXT_WINDOW.fetch_add(1, Ordering::Relaxed);
        let top_height = Arc::new(AtomicI32::new(TOP_HEIGHT));
        let side_width = Arc::new(AtomicI32::new(0));
        let top = ui_view(id, "top", private, top_height.clone(), Arc::new(AtomicI32::new(0)));
        let side = ui_view(id, "side", private, Arc::new(AtomicI32::new(0)), side_width.clone());
        let column = panel_create(None).expect("panel");
        let content = panel_create(None).expect("panel");
        let request_context = if private {
            // Empty cache path = in-memory context: cookies, cache and storage
            // are isolated and disappear with the context.
            let settings = RequestContextSettings::default();
            let mut handler = super::prefs::private_context_handler();
            request_context_create_context(Some(&settings), Some(&mut handler))
        } else {
            None
        };
        let ctx = WinCtx {
            id,
            private,
            window: None,
            top,
            side,
            top_height,
            side_width,
            column,
            content,
            win_layout: None,
            col_layout: None,
            tabs: Vec::new(),
            active: None,
            request_context,
            pending_popups: Vec::new(),
            initial_urls: urls,
            popup: None,
            closed: Vec::new(),
            fullscreen_tab: None,
            window_was_fullscreen: false,
        };
        if windows_mut(|v| v.push(ctx)).is_none() {
            return;
        }
        let mut delegate = NfWindowDelegate::new(id);
        window_create_top_level(Some(&mut delegate));
    });
}

fn on_window_created(id: u32, window: Window) {
    let unlocked = core().is_unlocked();
    with_window(id, |ctx| {
        let win_layout = window.set_to_box_layout(Some(&BoxLayoutSettings { horizontal: 1, cross_axis_alignment: AxisAlignment::STRETCH, ..Default::default() }));
        let mut side = View::from(&ctx.side);
        let mut column = View::from(&ctx.column);
        window.add_child_view(Some(&mut side));
        window.add_child_view(Some(&mut column));
        if let Some(l) = &win_layout {
            l.set_flex_for_view(Some(&mut side), 0);
            l.set_flex_for_view(Some(&mut column), 1);
        }
        let col_layout = ctx.column.set_to_box_layout(Some(&BoxLayoutSettings { horizontal: 0, cross_axis_alignment: AxisAlignment::STRETCH, ..Default::default() }));
        let mut top = View::from(&ctx.top);
        let mut content = View::from(&ctx.content);
        ctx.column.add_child_view(Some(&mut top));
        ctx.column.add_child_view(Some(&mut content));
        if let Some(l) = &col_layout {
            l.set_flex_for_view(Some(&mut content), 1);
        }
        ctx.content.set_to_fill_layout();
        ctx.content.set_background_color(0xFF0A0C10);
        ctx.column.set_background_color(0xFF0A0C10);
        ctx.win_layout = win_layout;
        ctx.col_layout = col_layout;
        window.set_title(Some(&CefString::from(if ctx.private { "NIGHTFALL · Navigation privée" } else { "NIGHTFALL" })));
        window.set_background_color(0xFF0A0C10);
        if let Some(mut icon) = image_create() {
            if icon.add_png(1.0, Some(WINDOW_ICON)) != 0 {
                window.set_window_icon(Some(&mut icon));
                window.set_window_app_icon(Some(&mut icon));
            }
        }
        // Fit the screen: 1280x820 at most, 92 % of the work area otherwise.
        let mut size = Size { width: 1280, height: 820 };
        if let Some(d) = display_get_primary() {
            let wa = d.work_area();
            if wa.width > 0 && wa.height > 0 {
                size.width = size.width.min(wa.width * 92 / 100);
                size.height = size.height.min(wa.height * 92 / 100);
            }
        }
        window.center_window(Some(&size));
        ctx.window = Some(window.clone());
    });
    apply_mode(id, !unlocked);
    if let Some(w) = with_window(id, |c| c.window.clone()).flatten() {
        w.show();
    }
    if unlocked {
        open_initial_tabs(id);
    }
}

/// Locked: the top UI view fills the window (splash / lock screen), web
/// content and the side bar are hidden. Unlocked: normal browser layout.
pub fn apply_mode(id: u32, locked: bool) {
    let vertical = core().settings.vertical_tabs;
    with_window(id, |ctx| {
        let mut top = View::from(&ctx.top);
        let mut side = View::from(&ctx.side);
        let mut content = View::from(&ctx.content);
        if let Some(l) = &ctx.col_layout {
            l.set_flex_for_view(Some(&mut top), if locked { 1 } else { 0 });
            l.set_flex_for_view(Some(&mut content), if locked { 0 } else { 1 });
        }
        ctx.top.set_visible(1);
        if locked {
            ctx.content.set_visible(0);
            ctx.side.set_visible(0);
            ctx.side_width.store(0, Ordering::Relaxed);
        } else if ctx.fullscreen_tab.is_some() {
            // A page is fullscreen: only its content is shown.
            ctx.top.set_visible(0);
            ctx.side.set_visible(0);
            ctx.content.set_visible(1);
        } else {
            ctx.top_height.store(if vertical { TOP_HEIGHT_VERTICAL } else { TOP_HEIGHT }, Ordering::Relaxed);
            ctx.content.set_visible(1);
            ctx.side.set_visible(if vertical { 1 } else { 0 });
            ctx.side_width.store(if vertical { SIDE_WIDTH } else { 0 }, Ordering::Relaxed);
            if let Some(l) = &ctx.win_layout {
                l.set_flex_for_view(Some(&mut side), 0);
            }
        }
        // Flex weights changed: the cached box layouts must be recomputed.
        ctx.top.invalidate_layout();
        ctx.side.invalidate_layout();
        ctx.content.invalidate_layout();
        ctx.column.invalidate_layout();
        ctx.column.layout();
        if let Some(w) = &ctx.window {
            w.invalidate_layout();
            w.layout();
        }
    });
    super::titlebar::set_titlebar_height(if vertical && !locked { TOP_HEIGHT_VERTICAL } else { TABSTRIP_HEIGHT });
    super::titlebar::apply(id);
    #[cfg(feature = "selftest")]
    state::log(&format!("selftest: apply_mode(verrouillé={locked}) -> {}", layout_report(id)));
}

/// Resize the top interface view (panels, warning banner).
pub fn set_top_height(id: u32, height: i32) {
    with_window(id, |ctx| {
        ctx.top_height.store(height, Ordering::Relaxed);
        ctx.top.invalidate_layout();
        ctx.column.layout();
    });
}

pub fn apply_mode_all(locked: bool) {
    for id in window_ids() {
        apply_mode(id, locked);
    }
}

fn open_initial_tabs(id: u32) {
    let urls = with_window(id, |c| std::mem::take(&mut c.initial_urls)).unwrap_or_default();
    let has_tabs = with_window(id, |c| !c.tabs.is_empty()).unwrap_or(false);
    if has_tabs {
        return;
    }
    if urls.is_empty() {
        add_tab(id, NEW_TAB_URL, true);
    } else {
        for (i, u) in urls.iter().enumerate() {
            add_tab(id, u, i == 0);
        }
    }
}

/// After unlock: show content, restore the session in the first window.
pub fn after_unlock(session: Vec<nf_storage::SessionTab>) {
    apply_mode_all(false);
    let ids = window_ids();
    let Some(&first) = ids.first() else {
        create_window(false, Vec::new());
        return;
    };
    let has_tabs = with_window(first, |c| !c.tabs.is_empty()).unwrap_or(false);
    if !has_tabs && !session.is_empty() {
        let urls: Vec<String> = session.iter().map(|t| t.url.clone()).collect();
        let active = session.iter().position(|t| t.active).unwrap_or(0);
        for (i, u) in urls.iter().enumerate() {
            add_tab(first, u, i == active);
        }
        for (i, t) in session.iter().enumerate() {
            if t.pinned {
                if let Some(tab_id) = with_window(first, |c| c.tabs.get(i).map(|t| t.id)).flatten() {
                    set_pinned(first, tab_id, true);
                }
            }
        }
    }
    for id in ids {
        open_initial_tabs(id);
    }
    // Links opened from other applications while NIGHTFALL was locked.
    let pending = EXTERNAL_PENDING.with(|p| std::mem::take(&mut *p.borrow_mut()));
    if !pending.is_empty() {
        open_external_urls(pending);
    }
    push_event_all(&serde_json::json!({ "type": "unlocked" }));
    push_tabs_all();
}

thread_local! {
    static EXTERNAL_PENDING: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Web addresses passed by another application (NIGHTFALL is the default
/// browser, or a second launch with links): opened in new tabs of the first
/// normal window, which comes to the front. While locked they wait for the
/// unlock and are never shown before the code.
pub fn open_external_urls(urls: Vec<String>) {
    let urls: Vec<String> = urls.into_iter().filter(|u| u.starts_with("http://") || u.starts_with("https://")).take(20).collect();
    let target = window_ids().into_iter().find(|&id| !is_private_window(id));
    let Some(id) = target else {
        create_window(false, urls);
        return;
    };
    if let Some(w) = with_window(id, |c| c.window.clone()).flatten() {
        w.activate();
    }
    if urls.is_empty() {
        return;
    }
    if !core().is_unlocked() {
        EXTERNAL_PENDING.with(|p| p.borrow_mut().extend(urls));
        return;
    }
    for (i, u) in urls.iter().enumerate() {
        add_tab(id, u, i == 0);
    }
}

/// Lock: hide every window's content (pages keep running but are invisible).
pub fn on_locked() {
    popup_close_all();
    for id in window_ids() {
        exit_content_fullscreen(id);
    }
    apply_mode_all(true);
    push_event_all(&serde_json::json!({ "type": "locked" }));
}

fn prepare_window_close(id: u32) {
    popup_close(id);
    let (is_last_window, private) =
        windows_mut(|w| (w.len() == 1, w.iter().find(|c| c.id == id).map(|c| c.private).unwrap_or(false))).unwrap_or((false, false));
    if is_last_window || !private {
        super::session::save_session_snapshot();
    }
    if is_last_window {
        // Chromium's copy of the remembered permission decisions does not
        // outlive the session (given back at the next unlock).
        super::permissions::forget_remembered_in_chromium();
    }
    // Collect first, close after: closing calls back into NIGHTFALL.
    let hosts: Vec<BrowserHost> = with_window(id, |ctx| {
        let tabs = ctx.tabs.iter().filter_map(|t| t.view.as_ref().and_then(|v| v.browser()));
        let ui = [&ctx.top, &ctx.side].into_iter().filter_map(|v| v.browser());
        tabs.chain(ui).filter_map(|b| b.host()).collect()
    })
    .unwrap_or_default();
    for h in hosts {
        mark_closing(&h);
        h.close_browser(1);
    }
    if is_last_window {
        super::session::seal_cookies_then(|| {});
    }
}

fn on_window_destroyed(id: u32) {
    if windows_busy() {
        super::ui_thread::on_ui_delayed(0, move || on_window_destroyed(id));
        return;
    }
    super::titlebar::forget(id);
    let remaining = windows_mut(|w| {
        w.retain(|c| c.id != id);
        w.len()
    })
    .unwrap_or(1);
    if remaining == 0 {
        super::updater::on_quit();
        // Give the Session Vault time to seal the cookies, then quit.
        super::ui_thread::on_ui_delayed(1200, || {
            quit_message_loop();
        });
    }
}

// ---------------------------------------------------------------- tabs

fn tab_view(window_id: u32, tab_id: u32, private: bool, url: &str, rc: Option<RequestContext>) -> Option<BrowserView> {
    let mut client = make_client(ClientKind::Tab { window_id, tab_id, private });
    let mut delegate = TabViewDelegate::new(window_id);
    let mut rc = rc;
    let settings = BrowserSettings { background_color: 0xFFFFFFFF, ..Default::default() };
    browser_view_create(
        Some(&mut client),
        Some(&CefString::from(url)),
        Some(&settings),
        None,
        rc.as_mut(),
        Some(&mut delegate),
    )
}

/// Open a tab. Internal pages (nightfall://) are allowed only through here.
pub fn add_tab(window_id: u32, url: &str, activate: bool) -> Option<u32> {
    add_tab_at(window_id, url, activate, None)
}

/// Open a tab at `index` (end of the strip when `None`).
pub fn add_tab_at(window_id: u32, url: &str, activate: bool, index: Option<usize>) -> Option<u32> {
    let tab_id = next_tab_id();
    if url.starts_with("nightfall://") {
        state::allow_internal_for_tab(tab_id);
    }
    let created = with_window(window_id, |ctx| {
        let view = tab_view(window_id, tab_id, ctx.private, url, ctx.request_context.clone())?;
        let mut v = View::from(&view);
        ctx.content.add_child_view(Some(&mut v));
        view.set_visible(0);
        let title = if url.starts_with("nightfall://") { "Nouvel onglet" } else { url };
        let tab = Tab::new(tab_id, Some(view), url, title);
        match index {
            Some(i) => {
                // Never in front of pinned tabs.
                let first_free = ctx.tabs.iter().take_while(|t| t.pinned).count();
                let i = i.clamp(first_free, ctx.tabs.len());
                ctx.tabs.insert(i, tab);
            }
            None => ctx.tabs.push(tab),
        }
        Some(())
    })
    .flatten();
    if created.is_none() {
        // No browser was created: no internal authorisation may stay pending.
        state::forget_internal_for_tab(tab_id);
        return None;
    }
    if activate || with_window(window_id, |c| c.active.is_none()).unwrap_or(false) {
        activate_tab(window_id, tab_id);
    } else {
        push_tabs(window_id);
    }
    Some(tab_id)
}

/// Index right after `tab_id` and the tabs it opened just after it.
pub fn index_after(window_id: u32, tab_id: u32) -> Option<usize> {
    with_window(window_id, |ctx| {
        let i = ctx.tabs.iter().position(|t| t.id == tab_id)?;
        let mut j = i + 1;
        while j < ctx.tabs.len() && ctx.tabs[j].opener == Some(tab_id) {
            j += 1;
        }
        Some(j)
    })
    .flatten()
}

/// Open a link from a tab: new tab to its right (background unless
/// `activate`), remembering the opener.
pub fn open_from_tab(window_id: u32, opener: u32, url: &str, activate: bool) -> Option<u32> {
    if url.starts_with("nightfall://") {
        // Web pages never open internal pages.
        state::log("ouverture de nightfall:// refusée depuis une page web");
        return None;
    }
    let index = index_after(window_id, opener);
    let id = add_tab_at(window_id, url, activate, index)?;
    with_window(window_id, |ctx| {
        if let Some(t) = ctx.tabs.iter_mut().find(|t| t.id == id) {
            t.opener = Some(opener);
        }
    });
    Some(id)
}

/// Reserve a tab id for a popup (called from on_before_popup).
pub fn reserve_popup(window_id: u32, popup_id: i32, opener: Option<u32>, background: bool) -> u32 {
    let tab_id = next_tab_id();
    with_window(window_id, |c| c.pending_popups.push(PendingPopup { popup_id, tab_id, opener, background }));
    tab_id
}

/// on_before_popup allowed a popup that Chromium did not create after all.
pub fn popup_aborted(window_id: u32, popup_id: i32) {
    with_window(window_id, |c| c.pending_popups.retain(|p| p.popup_id != popup_id));
}

fn adopt_popup(window_id: u32, view: BrowserView) {
    let pending = with_window(window_id, |c| if c.pending_popups.is_empty() { None } else { Some(c.pending_popups.remove(0)) }).flatten();
    let (tab_id, opener, background) = match pending {
        Some(p) => (p.tab_id, p.opener, p.background),
        None => (next_tab_id(), None, false),
    };
    let index = opener.and_then(|o| index_after(window_id, o));
    with_window(window_id, |ctx| {
        let mut v = View::from(&view);
        ctx.content.add_child_view(Some(&mut v));
        view.set_visible(0);
        let mut tab = Tab::new(tab_id, Some(view), "", "Chargement…");
        tab.opener = opener;
        match index {
            Some(i) => ctx.tabs.insert(i.min(ctx.tabs.len()), tab),
            None => ctx.tabs.push(tab),
        }
    });
    if background {
        push_tabs(window_id);
    } else {
        activate_tab(window_id, tab_id);
    }
}

pub fn activate_tab(window_id: u32, tab_id: u32) {
    popup_close(window_id);
    // Switching tabs ends another tab's page fullscreen.
    let fs = with_window(window_id, |c| c.fullscreen_tab).flatten();
    if fs.is_some() && fs != Some(tab_id) {
        exit_content_fullscreen(window_id);
    }
    let already_active = with_window(window_id, |c| c.active == Some(tab_id)).unwrap_or(false);
    // Restore a discarded tab first.
    let restore = with_window(window_id, |ctx| {
        ctx.tabs.iter().find(|t| t.id == tab_id && t.discarded).map(|t| (t.url.clone(), ctx.private, ctx.request_context.clone()))
    })
    .flatten();
    let restored = restore.is_some();
    if let Some((url, private, rc)) = restore {
        if url.starts_with("nightfall://") {
            state::allow_internal_for_tab(tab_id);
        }
        if let Some(view) = tab_view(window_id, tab_id, private, &url, rc) {
            with_window(window_id, |ctx| {
                let mut v = View::from(&view);
                ctx.content.add_child_view(Some(&mut v));
                if let Some(t) = ctx.tabs.iter_mut().find(|t| t.id == tab_id) {
                    t.view = Some(view);
                    t.discarded = false;
                    t.loading = true;
                }
            });
        }
    }
    with_window(window_id, |ctx| {
        for t in ctx.tabs.iter_mut() {
            let visible = t.id == tab_id;
            if let Some(v) = &t.view {
                v.set_visible(if visible { 1 } else { 0 });
            }
            if visible {
                t.last_active = now();
                if let Some(v) = &t.view {
                    v.request_focus();
                }
            }
        }
        ctx.active = Some(tab_id);
        ctx.content.layout();
        if let Some(w) = &ctx.window {
            let title = ctx.tabs.iter().find(|t| t.id == tab_id).map(|t| t.title.clone()).unwrap_or_default();
            let prefix = if ctx.private { "NIGHTFALL · Privé" } else { "NIGHTFALL" };
            w.set_title(Some(&CefString::from(format!("{title} — {prefix}").as_str())));
        }
    });
    // Pressing the tab already shown changes nothing: no "tabs" event (each
    // one rebuilds the tab strip under the mouse).
    if !already_active || restored {
        push_tabs(window_id);
    }
}

/// Refresh the OS window title from the active tab.
pub fn activate_tab_title(window_id: u32) {
    with_window(window_id, |ctx| {
        let Some(id) = ctx.active else { return };
        let title = ctx.tabs.iter().find(|t| t.id == id).map(|t| t.title.clone()).unwrap_or_default();
        let prefix = if ctx.private { "NIGHTFALL · Privé" } else { "NIGHTFALL" };
        if let Some(w) = &ctx.window {
            w.set_title(Some(&CefString::from(format!("{title} — {prefix}").as_str())));
        }
    });
}

// Browsers closed by NIGHTFALL itself; any other closing browser was closed
// by its page (window.close() in a login popup).
thread_local! {
    static CLOSING: RefCell<std::collections::HashSet<i32>> = RefCell::new(std::collections::HashSet::new());
}

fn mark_closing(host: &BrowserHost) {
    if let Some(b) = host.browser() {
        CLOSING.with(|c| c.borrow_mut().insert(b.identifier()));
    }
}

pub fn is_closing(browser_id: i32) -> bool {
    CLOSING.with(|c| c.borrow().contains(&browser_id))
}

/// True if NIGHTFALL closed this browser (and forgets it).
pub fn take_closing(browser_id: i32) -> bool {
    CLOSING.with(|c| c.borrow_mut().remove(&browser_id))
}

pub fn close_tab(window_id: u32, tab_id: u32) {
    close_tab_inner(window_id, tab_id, false);
}

/// A tab's page closed itself (window.close()): drop the tab, back to its opener.
pub fn on_tab_self_closed(window_id: u32, tab_id: u32, browser_id: i32) {
    // Only if the tab still shows that browser (not a discarded or
    // restored tab whose old browser went away).
    let same = with_window(window_id, |ctx| {
        let v = ctx.tabs.iter().find(|t| t.id == tab_id)?.view.as_ref()?;
        Some(v.browser().map(|b| b.identifier() == browser_id).unwrap_or(true))
    })
    .flatten()
    .unwrap_or(false);
    if same {
        close_tab_inner(window_id, tab_id, true);
    }
}

fn close_tab_inner(window_id: u32, tab_id: u32, self_closed: bool) {
    if with_window(window_id, |c| c.fullscreen_tab == Some(tab_id)).unwrap_or(false) {
        exit_content_fullscreen(window_id);
    }
    let mut closing: Option<BrowserHost> = None;
    let found = with_window(window_id, |ctx| {
        let idx = ctx.tabs.iter().position(|t| t.id == tab_id)?;
        let tab = ctx.tabs.remove(idx);
        if let Some(view) = &tab.view {
            closing = view.browser().and_then(|b| b.host());
            let mut v = View::from(view);
            ctx.content.remove_child_view(Some(&mut v));
        }
        let reopenable = tab.url.starts_with("http://") || tab.url.starts_with("https://") || tab.url.starts_with("view-source:");
        if !ctx.private && !self_closed && reopenable {
            ctx.closed.push(ClosedTab { url: tab.url.clone(), index: idx });
            if ctx.closed.len() > 25 {
                ctx.closed.remove(0);
            }
        }
        let was_active = ctx.active == Some(tab_id);
        let next = if ctx.tabs.is_empty() {
            None
        } else if let Some(o) = tab.opener.filter(|o| ctx.tabs.iter().any(|t| t.id == *o)) {
            // A popup or a link tab: back to the page that opened it.
            Some(o)
        } else {
            Some(ctx.tabs[idx.min(ctx.tabs.len() - 1)].id)
        };
        Some((was_active, next))
    })
    .flatten();
    // Unknown tab (already closed): nothing to do.
    let Some((was_active, next)) = found else { return };
    state::forget_internal_for_tab(tab_id);
    if let Some(h) = closing {
        mark_closing(&h);
        h.close_browser(1);
    }
    match next {
        Some(n) if was_active => activate_tab(window_id, n),
        Some(_) => push_tabs(window_id),
        None => {
            // Last tab closed: the window closes (as in Chrome).
            let had_window = with_window(window_id, |c| {
                c.active = None;
                c.window.is_some()
            })
            .unwrap_or(false);
            if had_window {
                close_window(window_id);
            }
        }
    }
}

/// Reopen the most recently closed tab of the window.
pub fn reopen_closed_tab(window_id: u32) -> Option<u32> {
    let last = with_window(window_id, |c| c.closed.pop()).flatten()?;
    add_tab_at(window_id, &last.url, true, Some(last.index))
}

/// Open the same address in a new tab right after this one.
pub fn duplicate_tab(window_id: u32, tab_id: u32) -> Option<u32> {
    let url = tab_url(window_id, tab_id).filter(|u| !u.is_empty())?;
    let index = with_window(window_id, |c| c.tabs.iter().position(|t| t.id == tab_id)).flatten()?;
    add_tab_at(window_id, &url, true, Some(index + 1))
}

/// New tab page right after `tab_id`.
pub fn new_tab_right(window_id: u32, tab_id: u32) -> Option<u32> {
    let index = with_window(window_id, |c| c.tabs.iter().position(|t| t.id == tab_id)).flatten()?;
    add_tab_at(window_id, NEW_TAB_URL, true, Some(index + 1))
}

/// Close every tab except `tab_id` (pinned tabs are kept, as in Chrome).
pub fn close_other_tabs(window_id: u32, tab_id: u32) {
    let ids: Vec<u32> = with_window(window_id, |c| c.tabs.iter().filter(|t| t.id != tab_id && !t.pinned).map(|t| t.id).collect()).unwrap_or_default();
    activate_tab(window_id, tab_id);
    for id in ids {
        close_tab(window_id, id);
    }
}

/// Close the tabs on the right of `tab_id`.
pub fn close_tabs_right(window_id: u32, tab_id: u32) {
    let ids: Vec<u32> = with_window(window_id, |c| {
        let Some(i) = c.tabs.iter().position(|t| t.id == tab_id) else { return Vec::new() };
        c.tabs[i + 1..].iter().filter(|t| !t.pinned).map(|t| t.id).collect()
    })
    .unwrap_or_default();
    if ids.iter().any(|id| active_tab_id(window_id) == Some(*id)) {
        activate_tab(window_id, tab_id);
    }
    for id in ids {
        close_tab(window_id, id);
    }
}

/// Mute or unmute a tab.
pub fn set_tab_muted(window_id: u32, tab_id: u32, muted: bool) {
    if let Some(h) = tab_browser(window_id, tab_id).and_then(|b| b.host()) {
        h.set_audio_muted(if muted { 1 } else { 0 });
    }
    push_tabs(window_id);
}

pub fn tab_muted(window_id: u32, tab_id: u32) -> bool {
    tab_browser(window_id, tab_id).and_then(|b| b.host()).map(|h| h.is_audio_muted() != 0).unwrap_or(false)
}

/// Activate the n-th tab (0-based); `usize::MAX` = last tab.
pub fn activate_index(window_id: u32, n: usize) {
    let id = with_window(window_id, |c| if n == usize::MAX { c.tabs.last().map(|t| t.id) } else { c.tabs.get(n).map(|t| t.id) }).flatten();
    if let Some(id) = id {
        activate_tab(window_id, id);
    }
}

pub fn set_pinned(window_id: u32, tab_id: u32, pinned: bool) {
    with_window(window_id, |ctx| {
        if let Some(t) = ctx.tabs.iter_mut().find(|t| t.id == tab_id) {
            t.pinned = pinned;
        }
        // Pinned tabs first, keeping relative order.
        ctx.tabs.sort_by_key(|t| !t.pinned);
    });
    push_tabs(window_id);
}

pub fn move_tab(window_id: u32, tab_id: u32, to_index: usize) {
    with_window(window_id, |ctx| {
        if let Some(i) = ctx.tabs.iter().position(|t| t.id == tab_id) {
            let t = ctx.tabs.remove(i);
            let to = to_index.min(ctx.tabs.len());
            ctx.tabs.insert(to, t);
        }
    });
    push_tabs(window_id);
}

/// Unload a tab to free memory; it reloads when activated.
pub fn discard_tab(window_id: u32, tab_id: u32) {
    let closing = with_window(window_id, |ctx| {
        if ctx.active == Some(tab_id) {
            return None;
        }
        let mut closing = None;
        if let Some(t) = ctx.tabs.iter_mut().find(|t| t.id == tab_id) {
            if t.audible {
                return None; // never unload a tab playing sound
            }
            if let Some(view) = t.view.take() {
                closing = view.browser().and_then(|b| b.host());
                let mut v = View::from(&view);
                ctx.content.remove_child_view(Some(&mut v));
            }
            t.discarded = true;
            t.loading = false;
        }
        closing
    })
    .flatten();
    if let Some(h) = closing {
        mark_closing(&h);
        h.close_browser(1);
    }
    push_tabs(window_id);
}

// ------------------------------------------------------------- fullscreen

/// A page entered or left HTML5 fullscreen (DisplayHandler): with Alloy the
/// application makes the window fullscreen and hides its own interface.
pub fn set_content_fullscreen(window_id: u32, tab_id: u32, on: bool) {
    if windows_busy() {
        super::ui_thread::on_ui_delayed(0, move || set_content_fullscreen(window_id, tab_id, on));
        return;
    }
    if !on {
        if with_window(window_id, |c| c.fullscreen_tab == Some(tab_id)).unwrap_or(false) {
            restore_from_fullscreen(window_id);
        }
        return;
    }
    if active_tab_id(window_id) != Some(tab_id) {
        // Only the visible tab may take the whole window.
        if let Some(h) = tab_browser(window_id, tab_id).and_then(|b| b.host()) {
            h.exit_fullscreen(1);
        }
        return;
    }
    popup_close(window_id);
    let window = with_window(window_id, |ctx| {
        ctx.window_was_fullscreen = ctx.window.as_ref().map(|w| w.is_fullscreen() != 0).unwrap_or(false);
        ctx.fullscreen_tab = Some(tab_id);
        ctx.window.clone()
    })
    .flatten();
    let locked = !core().is_unlocked();
    apply_mode(window_id, locked);
    if let Some(w) = window {
        if w.is_fullscreen() == 0 {
            w.set_fullscreen(1);
        }
    }
    if let Some(v) = with_window(window_id, |c| c.tabs.iter().find(|t| t.id == tab_id).and_then(|t| t.view.clone())).flatten() {
        v.request_focus();
    }
    push_event(window_id, &serde_json::json!({ "type": "fullscreen", "on": true }));
}

fn restore_from_fullscreen(window_id: u32) {
    let Some((window, was_fullscreen)) = with_window(window_id, |ctx| {
        ctx.fullscreen_tab = None;
        (ctx.window.clone(), ctx.window_was_fullscreen)
    }) else {
        return;
    };
    let locked = !core().is_unlocked();
    apply_mode(window_id, locked);
    if let Some(w) = window {
        if !was_fullscreen && w.is_fullscreen() != 0 {
            w.set_fullscreen(0);
        }
    }
    push_event(window_id, &serde_json::json!({ "type": "fullscreen", "on": false }));
}

/// Leave a page's fullscreen (Escape, tab switch, lock...).
pub fn exit_content_fullscreen(window_id: u32) {
    let Some(tab) = with_window(window_id, |c| c.fullscreen_tab).flatten() else { return };
    if let Some(h) = tab_browser(window_id, tab).and_then(|b| b.host()) {
        if h.is_fullscreen() != 0 {
            h.exit_fullscreen(1);
        }
    }
    // The page's notification may never come (crashed page): restore now.
    if with_window(window_id, |c| c.fullscreen_tab.is_some()).unwrap_or(false) {
        restore_from_fullscreen(window_id);
    }
}

pub fn content_fullscreen(window_id: u32) -> bool {
    with_window(window_id, |c| c.fullscreen_tab.is_some()).unwrap_or(false)
}

/// F11 / Ctrl+Cmd+F: whole-window fullscreen (the interface stays visible).
pub fn toggle_window_fullscreen(window_id: u32) {
    if content_fullscreen(window_id) {
        exit_content_fullscreen(window_id);
        return;
    }
    if let Some(w) = with_window(window_id, |c| c.window.clone()).flatten() {
        w.set_fullscreen(if w.is_fullscreen() != 0 { 0 } else { 1 });
    }
}

/// Diagnostic (CI self test).
#[cfg(feature = "selftest")]
pub fn fullscreen_report(window_id: u32) -> (bool, bool, bool) {
    with_window(window_id, |c| (c.fullscreen_tab.is_some(), c.top.is_visible() != 0, c.window.as_ref().map(|w| w.is_fullscreen() != 0).unwrap_or(false))).unwrap_or((false, false, false))
}

pub fn active_browser(window_id: u32) -> Option<Browser> {
    with_window(window_id, |ctx| {
        let id = ctx.active?;
        ctx.tabs.iter().find(|t| t.id == id)?.view.as_ref()?.browser()
    })
    .flatten()
}

pub fn tab_browser(window_id: u32, tab_id: u32) -> Option<Browser> {
    with_window(window_id, |ctx| ctx.tabs.iter().find(|t| t.id == tab_id)?.view.as_ref()?.browser()).flatten()
}

pub fn tab_url(window_id: u32, tab_id: u32) -> Option<String> {
    with_window(window_id, |ctx| ctx.tabs.iter().find(|t| t.id == tab_id).map(|t| t.url.clone())).flatten()
}

pub fn active_tab_id(window_id: u32) -> Option<u32> {
    with_window(window_id, |c| c.active).flatten()
}

pub fn active_url(window_id: u32) -> Option<String> {
    with_window(window_id, |ctx| {
        let id = ctx.active?;
        ctx.tabs.iter().find(|t| t.id == id).map(|t| t.url.clone())
    })
    .flatten()
}

/// Load a URL in the active tab (or a new one if none).
pub fn navigate_active(window_id: u32, url: &str) {
    let tab = active_tab_id(window_id);
    match tab {
        Some(tab_id) => {
            let Some(b) = tab_browser(window_id, tab_id) else {
                // No browser (cannot happen for the active tab): new tab.
                add_tab(window_id, url, true);
                return;
            };
            if url.starts_with("nightfall://") {
                // Asked by NIGHTFALL itself (Accueil, address bar): allowed
                // once on this tab's existing browser. The per-tab list
                // (`allow_internal_for_tab`) is only read when a browser is
                // created, so it is not used here.
                if let Some(info) = state::browsers().get_mut(&b.identifier()) {
                    info.allow_internal_nav = true;
                }
            }
            if let Some(f) = b.main_frame() {
                f.load_url(Some(&CefString::from(url)));
            }
        }
        None => {
            add_tab(window_id, url, true);
        }
    }
}

// ---------------------------------------------------------------- tab events

pub fn update_tab(window_id: u32, tab_id: u32, f: impl FnOnce(&mut Tab) + Send + 'static) {
    if windows_busy() {
        // Called back by CEF while the window list is in use: apply later.
        super::ui_thread::on_ui_delayed(0, move || update_tab(window_id, tab_id, f));
        return;
    }
    with_window(window_id, |ctx| {
        if let Some(t) = ctx.tabs.iter_mut().find(|t| t.id == tab_id) {
            f(t);
        }
    });
    push_tabs(window_id);
}

#[derive(Serialize)]
pub struct TabState {
    pub id: u32,
    pub title: String,
    pub url: String,
    pub loading: bool,
    pub pinned: bool,
    pub active: bool,
    pub discarded: bool,
    pub crashed: bool,
    pub can_back: bool,
    pub can_forward: bool,
    pub warning: Option<String>,
    pub blocked: u64,
    pub favicon: Option<String>,
    pub audible: bool,
    pub muted: bool,
    /// Zoom in percent (100 = default).
    pub zoom: i32,
}

/// Chromium zoom level → percent (each level is a factor 1.2).
pub fn zoom_percent(level: f64) -> i32 {
    (100.0 * 1.2f64.powf(level)).round() as i32
}

pub fn tabs_state(window_id: u32) -> serde_json::Value {
    with_window(window_id, |ctx| {
        let tabs: Vec<TabState> = ctx
            .tabs
            .iter()
            .map(|t| {
                let browser = t.view.as_ref().and_then(|v| v.browser());
                let blocked = browser.as_ref().map(|b| super::client::blocked_count(b.identifier())).unwrap_or(0);
                let host = browser.as_ref().and_then(|b| b.host());
                let muted = host.as_ref().map(|h| h.is_audio_muted() != 0).unwrap_or(false);
                let zoom = host.as_ref().map(|h| zoom_percent(h.zoom_level())).unwrap_or(100);
                TabState {
                    id: t.id,
                    title: t.title.clone(),
                    url: t.url.clone(),
                    loading: t.loading,
                    pinned: t.pinned,
                    active: ctx.active == Some(t.id),
                    discarded: t.discarded,
                    crashed: t.crashed,
                    can_back: t.can_back,
                    can_forward: t.can_forward,
                    warning: t.warning.clone(),
                    blocked,
                    favicon: t.favicon.clone(),
                    audible: t.audible,
                    muted,
                    zoom,
                }
            })
            .collect();
        serde_json::json!({ "type": "tabs", "private": ctx.private, "tabs": tabs })
    })
    .unwrap_or(serde_json::Value::Null)
}

fn exec_js(view: &BrowserView, code: &str) {
    if let Some(f) = view.browser().and_then(|b| b.main_frame()) {
        f.execute_java_script(Some(&CefString::from(code)), Some(&CefString::from("nightfall://app/")), 0);
    }
}

/// Send an event to the window's interface views.
/// True for a private window, and for an unknown one (safe side).
pub fn is_private_window(id: u32) -> bool {
    with_window(id, |c| c.private).unwrap_or(true)
}

pub fn push_event(window_id: u32, event: &serde_json::Value) {
    let code = format!("window.nfEvent && window.nfEvent({event});");
    if windows_busy() {
        super::ui_thread::on_ui_delayed(0, move || {
            with_window(window_id, |ctx| send_to_ui_views(ctx, &code));
        });
        return;
    }
    with_window(window_id, |ctx| send_to_ui_views(ctx, &code));
}

fn send_to_ui_views(ctx: &WinCtx, code: &str) {
    exec_js(&ctx.top, code);
    exec_js(&ctx.side, code);
    if let Some(p) = &ctx.popup {
        exec_js(&p.view, code);
    }
}

// ------------------------------------------------------------------ popups

const POPUP_MARGIN: i32 = 8;

/// Open (or toggle) a dropdown popup anchored under a button of the top
/// interface view. `anchor` is the button's rectangle in top-view CSS px.
pub fn popup_open(window_id: u32, panel: &str, anchor: Rect, width: i32, height: i32) {
    popup_open_ex(window_id, panel, anchor, width, height, PopupOpts::default());
}

pub fn popup_panel(window_id: u32) -> Option<String> {
    with_window(window_id, |ctx| ctx.popup.as_ref().map(|p| p.panel.clone())).flatten()
}

pub fn popup_open_ex(window_id: u32, panel: &str, anchor: Rect, width: i32, height: i32, opts: PopupOpts) {
    let panel_id: String = panel_id(panel);
    let current = popup_panel(window_id);
    if current.as_deref() == Some(panel_id.as_str()) && !opts.toggle {
        popup_resize(window_id, width, height);
        return;
    }
    popup_close(window_id);
    if current.as_deref() == Some(panel_id.as_str()) {
        return;
    }
    let Some((window, private, origin, area)) = with_window(window_id, |ctx| {
        let col = View::from(&ctx.column).bounds();
        let top = View::from(&ctx.top).bounds();
        let side = View::from(&ctx.side).bounds();
        let area = Size { width: col.x + col.width, height: col.y + col.height };
        let origin = if opts.from_side { (side.x, side.y) } else { (col.x + top.x, col.y + top.y) };
        ctx.window.clone().map(|w| (w, ctx.private, origin, area))
    })
    .flatten() else {
        return;
    };
    let serial = NEXT_POPUP.fetch_add(1, Ordering::Relaxed);
    let url = format!(
        "nightfall://app/shell.html?mode=popup&w={window_id}&private={}&panel={panel_id}&n={serial}",
        if private { 1 } else { 0 }
    );
    let max_w = opts.max_width.clamp(160, 2400);
    let (w, h) = (width.clamp(160, max_w), height.clamp(40, 760));
    let target = if panel_id == "find" { active_browser(window_id) } else { None };
    let view = ui_view_url(window_id, false, &url, Arc::new(AtomicI32::new(h)), Arc::new(AtomicI32::new(w)));
    let mut v = View::from(&view);
    let Some(overlay) = window.add_overlay_view(Some(&mut v), DockingMode::CUSTOM, if opts.activate { 1 } else { 0 }) else {
        state::log("menu déroulant : superposition refusée par CEF");
        return;
    };
    // The browser is created when the view joins the overlay widget.
    if let Some(b) = view.browser() {
        POPUP_BROWSERS.with(|p| p.borrow_mut().insert(b.identifier()));
    }
    let right = if opts.align_left { origin.0 + anchor.x } else { origin.0 + anchor.x + anchor.width };
    let top = origin.1 + anchor.y + anchor.height + 6;
    place_popup(area, &overlay, right, opts.align_left, top, w, h);
    overlay.set_visible(1);
    if opts.activate {
        view.request_focus();
    }
    with_window(window_id, |ctx| {
        ctx.popup = Some(Popup { panel: panel_id, serial, view, overlay, right, top, align_left: opts.align_left, max_width: max_w, target });
    });
    super::titlebar::apply(window_id);
}

/// Open a panel under the toolbar, at the right edge of the window (find bar).
pub fn popup_open_under_toolbar(window_id: u32, panel: &str, width: i32, height: i32) {
    let Some(top) = with_window(window_id, |ctx| View::from(&ctx.top).bounds()) else { return };
    let anchor = Rect { x: top.width - 12, y: 0, width: 0, height: (top.height - 6).max(0) };
    popup_open_ex(window_id, panel, anchor, width, height, PopupOpts { toggle: false, ..Default::default() });
}

/// Give the keyboard focus to the open popup (find bar shortcut again).
pub fn popup_focus(window_id: u32) {
    if let Some(v) = with_window(window_id, |ctx| ctx.popup.as_ref().map(|p| p.view.clone())).flatten() {
        v.request_focus();
    }
}

/// `area`: size of the window's client area (where overlays live).
fn place_popup(win: Size, overlay: &OverlayController, anchor_x: i32, align_left: bool, top: i32, w: i32, h: i32) {
    let max_h = (win.height - top - POPUP_MARGIN).max(40);
    let h = h.min(max_h);
    let x = if align_left { anchor_x } else { anchor_x - w };
    let x = x.clamp(POPUP_MARGIN, (win.width - w - POPUP_MARGIN).max(POPUP_MARGIN));
    overlay.set_bounds(Some(&Rect { x, y: top, width: w, height: h }));
}

/// The popup asks for a new size (after rendering its content).
pub fn popup_resize(window_id: u32, width: i32, height: i32) {
    with_window(window_id, |ctx| {
        let col = View::from(&ctx.column).bounds();
        let area = Size { width: col.x + col.width, height: col.y + col.height };
        if let Some(p) = &ctx.popup {
            place_popup(area, &p.overlay, p.right, p.align_left, p.top, width.clamp(160, p.max_width), height.clamp(40, 760));
        }
    });
    super::titlebar::apply(window_id);
}

pub fn popup_close(window_id: u32) {
    popup_close_if(window_id, None, None);
}

fn panel_id(panel: &str) -> String {
    panel.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').take(24).collect()
}

/// Close the popup only if it is the one named: `panel` (its panel name)
/// and `serial` (its instance number), each checked when given. A popup
/// asks to close itself when it loses the focus; on macOS that request can
/// arrive after the next popup was opened, which must stay open.
pub fn popup_close_if(window_id: u32, panel: Option<&str>, serial: Option<u32>) -> bool {
    let wanted = panel.map(panel_id);
    let popup = with_window(window_id, |ctx| {
        let same = ctx.popup.as_ref().map(|p| wanted.as_deref().is_none_or(|n| p.panel == n) && serial.is_none_or(|n| p.serial == n)).unwrap_or(false);
        if same { ctx.popup.take() } else { None }
    })
    .flatten();
    match popup {
        Some(p) => {
            destroy_popup(window_id, p);
            true
        }
        None => false,
    }
}

/// Close the popup showing this browser, if it is still the open one.
pub fn popup_close_browser(window_id: u32, browser_id: i32) {
    let serial = with_window(window_id, |ctx| {
        let p = ctx.popup.as_ref()?;
        (p.view.browser().map(|b| b.identifier()) == Some(browser_id)).then_some(p.serial)
    })
    .flatten();
    if let Some(n) = serial {
        popup_close_if(window_id, None, Some(n));
    }
}

// Browsers of the dropdown popups (overlay views): their close is handled
// by NIGHTFALL, never by CEF's default DoClose (see client.rs `do_close`).
thread_local! {
    static POPUP_BROWSERS: RefCell<std::collections::HashSet<i32>> = RefCell::new(std::collections::HashSet::new());
}

pub fn is_popup_browser(browser_id: i32) -> bool {
    POPUP_BROWSERS.with(|p| p.borrow().contains(&browser_id))
}

/// A popup browser was destroyed (on_before_close).
pub fn forget_popup_browser(browser_id: i32) {
    POPUP_BROWSERS.with(|p| p.borrow_mut().remove(&browser_id));
    take_closing(browser_id);
}

/// Free a popup's view and browser.
///
/// Order matters (each closed popup used to keep its browser and renderer
/// process): the view is first taken out of its overlay widget, then our
/// last reference to it is released. CEF then destroys the browser through
/// the view's tear-down (`WindowDestroyed`), which does not call `DoClose`:
/// CEF's default close (asking the parent window to close, i.e. the whole
/// NIGHTFALL window) can never be reached. Calling `close_browser` first,
/// as before, put the browser in the "closing" state with no window to
/// close, and the view tear-down then skipped it: it was never destroyed
/// (CEF sources: `AlloyBrowserHostImpl::CloseContents`,
/// `CefBrowserViewImpl::~CefBrowserViewImpl`).
fn destroy_popup(window_id: u32, p: Popup) {
    let Popup { panel, view, overlay, target, .. } = p;
    let browser = view.browser();
    if let Some(h) = browser.as_ref().and_then(|b| b.host()) {
        mark_closing(&h);
    }
    overlay.destroy();
    drop(overlay);
    drop(view);
    if let Some(b) = browser {
        // Normally already destroyed (synchronously) at this point. If some
        // other reference keeps the view alive, close the browser
        // explicitly: `do_close` answers 1 for popup browsers, so CEF never
        // closes a window for it, and the view's tear-down completes it.
        super::ui_thread::on_ui_delayed(1000, move || {
            let id = b.identifier();
            if state::browsers().contains_key(&id) {
                state::log("menu déroulant : navigateur encore vivant après sa vue, fermeture explicite");
                if let Some(h) = b.host() {
                    h.close_browser(1);
                }
            }
        });
    }
    if let Some(h) = target.and_then(|b| b.host()) {
        // Find bar closed: remove the highlights.
        h.stop_finding(1);
    }
    push_event(window_id, &serde_json::json!({ "type": "popup_closed", "panel": panel }));
    // The window can be dragged again where the menu was.
    super::titlebar::apply(window_id);
}

/// Give the keyboard focus back to the page (after closing the find bar).
pub fn focus_active_tab(window_id: u32) {
    if let Some(v) = with_window(window_id, |ctx| {
        let id = ctx.active?;
        ctx.tabs.iter().find(|t| t.id == id)?.view.clone()
    })
    .flatten()
    {
        v.request_focus();
    }
}

/// Ctrl+L: keyboard focus to the address field.
pub fn focus_address(window_id: u32) {
    if content_fullscreen(window_id) {
        exit_content_fullscreen(window_id);
    }
    if let Some(top) = with_window(window_id, |c| c.top.clone()) {
        top.request_focus();
    }
    push_event(window_id, &serde_json::json!({ "type": "focus_address" }));
}

/// Copy text to the system clipboard: the text is selected in the top
/// interface view and copied by Chromium's own Copy command (no extra
/// platform code, works on Windows and macOS).
pub fn copy_text(window_id: u32, text: &str) {
    let Some(top) = with_window(window_id, |c| c.top.clone()) else { return };
    let Some(frame) = top.browser().and_then(|b| b.main_frame()) else { return };
    let arg = serde_json::to_string(text).unwrap_or_else(|_| "\"\"".into());
    frame.execute_java_script(Some(&CefString::from(format!("window.nfSelectForCopy && window.nfSelectForCopy({arg});").as_str())), Some(&CefString::from("nightfall://app/")), 0);
    // Same message pipe as the script above: runs after it.
    frame.copy();
    frame.execute_java_script(Some(&CefString::from("window.nfCopyDone && window.nfCopyDone();")), Some(&CefString::from("nightfall://app/")), 0);
}

/// Diagnostic (CI self test).
#[cfg(feature = "selftest")]
pub fn popup_report(window_id: u32) -> String {
    with_window(window_id, |ctx| match &ctx.popup {
        Some(p) => {
            let b = p.overlay.bounds();
            format!("ouvert ({}) {}x{} en {},{} visible={}", p.panel, b.width, b.height, b.x, b.y, p.overlay.is_visible())
        }
        None => "fermé".into(),
    })
    .unwrap_or_default()
}

pub fn popup_close_all() {
    for id in window_ids() {
        popup_close(id);
    }
}

pub fn push_event_all(event: &serde_json::Value) {
    for id in window_ids() {
        push_event(id, event);
    }
}

/// Run JavaScript in every tab showing an internal page (UI thread).
pub fn exec_in_internal_tabs(code: &str) {
    let targets: Vec<(u32, u32)> = windows_mut(|w| {
        w.iter()
            .flat_map(|c| c.tabs.iter().filter(|t| t.url.starts_with("nightfall://app/")).map(move |t| (c.id, t.id)))
            .collect()
    })
    .unwrap_or_default();
    for (w, t) in targets {
        exec_in_tab(w, t, code);
    }
}

pub fn push_tabs(window_id: u32) {
    if windows_busy() {
        super::ui_thread::on_ui_delayed(0, move || push_tabs(window_id));
        return;
    }
    let st = tabs_state(window_id);
    remember_active_url(window_id, active_url(window_id));
    push_event(window_id, &st);
}

/// Address of the active tab of each window, readable from any thread
/// without waiting for the UI thread (the internal API runs on CEF's IO
/// thread: waiting there blocked every network request, up to 2 s on macOS).
/// Updated with every "tabs" event, i.e. on each activation and address change.
static ACTIVE_URLS: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<u32, String>>> = std::sync::OnceLock::new();

fn remember_active_url(window_id: u32, url: Option<String>) {
    let m = ACTIVE_URLS.get_or_init(Default::default);
    if let Ok(mut m) = m.lock() {
        match url {
            Some(u) => {
                m.insert(window_id, u);
            }
            None => {
                m.remove(&window_id);
            }
        }
    }
}

/// Last known address of the active tab (any thread).
pub fn cached_active_url(window_id: u32) -> Option<String> {
    ACTIVE_URLS.get()?.lock().ok()?.get(&window_id).cloned()
}

pub fn push_tabs_all() {
    for id in window_ids() {
        push_tabs(id);
    }
}

/// Execute JavaScript in a tab's main frame.
pub fn exec_in_tab(window_id: u32, tab_id: u32, code: &str) {
    if let Some(f) = tab_browser(window_id, tab_id).and_then(|b| b.main_frame()) {
        f.execute_java_script(Some(&CefString::from(code)), None, 0);
    }
}

/// Session snapshot (non-private windows only).
pub fn session_tabs() -> Vec<nf_storage::SessionTab> {
    let mut out = Vec::new();
    windows_mut(|w| {
        for (wi, ctx) in w.iter().filter(|c| !c.private).enumerate() {
            for (ti, t) in ctx.tabs.iter().enumerate() {
                if t.url.is_empty() || t.url.starts_with("nightfall://app/newtab") {
                    continue;
                }
                out.push(nf_storage::SessionTab {
                    window_index: wi as i64,
                    tab_index: ti as i64,
                    url: t.url.clone(),
                    title: t.title.clone(),
                    pinned: t.pinned,
                    active: ctx.active == Some(t.id),
                });
            }
        }
    });
    out
}

/// Inputs for the performance policy.
pub fn tab_activity() -> Vec<(u32, nf_perf::TabActivity)> {
    let mut out = Vec::new();
    windows_mut(|w| {
        for ctx in w.iter() {
            for t in &ctx.tabs {
                out.push((
                    ctx.id,
                    nf_perf::TabActivity {
                        id: t.id,
                        last_active: t.last_active,
                        is_active: ctx.active == Some(t.id),
                        pinned: t.pinned,
                        audible: t.audible,
                        editing_form: false,
                        private: ctx.private,
                        discarded: t.discarded,
                    },
                ));
            }
        }
    });
    out
}

/// Close every private window (used by Emergency Lockdown).
pub fn close_private_windows() {
    let ids: Vec<u32> = windows_mut(|w| w.iter().filter(|c| c.private).map(|c| c.id).collect()).unwrap_or_default();
    for id in ids {
        if let Some(w) = with_window(id, |c| c.window.clone()).flatten() {
            w.close();
        }
    }
}

pub fn close_window(window_id: u32) {
    if let Some(w) = with_window(window_id, |c| c.window.clone()).flatten() {
        w.close();
    }
}

pub fn show_devtools(window_id: u32) {
    if !core().settings.devtools_enabled {
        return;
    }
    if let Some(b) = active_browser(window_id) {
        if let Some(h) = b.host() {
            let wi = WindowInfo::default();
            let settings = BrowserSettings::default();
            h.show_dev_tools(Some(&wi), None, Some(&settings), None);
        }
    }
}

/// Diagnostic (CI self test): interface browsers alive (top, side, popups).
#[cfg(feature = "selftest")]
pub fn ui_browsers_alive() -> usize {
    state::browsers().values().filter(|i| matches!(i.kind, state::BrowserKind::Ui { .. })).count()
}

/// Diagnostic (CI self test): bounds and visibility of a window's views.
#[cfg(feature = "selftest")]
pub fn layout_report(window_id: u32) -> String {
    let fmt = |name: &str, v: &View| {
        let b = v.bounds();
        let p = v.preferred_size();
        format!("{name}=({},{} {}x{} préféré {}x{} visible={} drawn={})", b.x, b.y, b.width, b.height, p.width, p.height, v.is_visible(), v.is_drawn())
    };
    with_window(window_id, |ctx| {
        let mut out = vec![
            fmt("top", &View::from(&ctx.top)),
            fmt("side", &View::from(&ctx.side)),
            fmt("column", &View::from(&ctx.column)),
            fmt("content", &View::from(&ctx.content)),
        ];
        for t in &ctx.tabs {
            if let Some(v) = &t.view {
                out.push(fmt(&format!("onglet{}", t.id), &View::from(v)));
            }
        }
        out.join(" ")
    })
    .unwrap_or_default()
}

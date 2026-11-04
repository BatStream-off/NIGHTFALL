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
pub const SIDE_WIDTH: i32 = 260;
pub const NEW_TAB_URL: &str = "nightfall://app/newtab.html";

static NEXT_WINDOW: AtomicU32 = AtomicU32::new(1);
static NEXT_TAB: AtomicU32 = AtomicU32::new(1);

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
    pub pending_popups: Vec<u32>,
    /// URLs to open once the window exists and NIGHTFALL is unlocked.
    pub initial_urls: Vec<String>,
    /// Dropdown popup (menu, Shield, passwords) shown over the page.
    pub popup: Option<Popup>,
}

pub struct Popup {
    pub panel: String,
    pub view: BrowserView,
    pub overlay: OverlayController,
    /// Right edge and top of the popup, in window coordinates.
    pub right: i32,
    pub top: i32,
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

        fn window_runtime_style(&self) -> RuntimeStyle {
            RuntimeStyle::ALLOY
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
            request_context_create_context(Some(&settings), None)
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
        if locked {
            ctx.content.set_visible(0);
            ctx.side.set_visible(0);
            ctx.side_width.store(0, Ordering::Relaxed);
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
    push_event_all(&serde_json::json!({ "type": "unlocked" }));
    push_tabs_all();
}

/// Lock: hide every window's content (pages keep running but are invisible).
pub fn on_locked() {
    popup_close_all();
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
    // Collect first, close after: closing calls back into NIGHTFALL.
    let hosts: Vec<BrowserHost> = with_window(id, |ctx| {
        let tabs = ctx.tabs.iter().filter_map(|t| t.view.as_ref().and_then(|v| v.browser()));
        let ui = [&ctx.top, &ctx.side].into_iter().filter_map(|v| v.browser());
        tabs.chain(ui).filter_map(|b| b.host()).collect()
    })
    .unwrap_or_default();
    for h in hosts {
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
    let remaining = windows_mut(|w| {
        w.retain(|c| c.id != id);
        w.len()
    })
    .unwrap_or(1);
    if remaining == 0 {
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
    let tab_id = next_tab_id();
    if url.starts_with("nightfall://") {
        state::allow_internal_for_tab(tab_id);
    }
    let created = with_window(window_id, |ctx| {
        let view = tab_view(window_id, tab_id, ctx.private, url, ctx.request_context.clone())?;
        let mut v = View::from(&view);
        ctx.content.add_child_view(Some(&mut v));
        view.set_visible(0);
        ctx.tabs.push(Tab {
            id: tab_id,
            view: Some(view),
            url: url.to_string(),
            title: if url.starts_with("nightfall://") { "Nouvel onglet".into() } else { url.to_string() },
            loading: true,
            can_back: false,
            can_forward: false,
            pinned: false,
            last_active: now(),
            warning: None,
            discarded: false,
            crashed: false,
        });
        Some(())
    })
    .flatten();
    created?;
    if activate || with_window(window_id, |c| c.active.is_none()).unwrap_or(false) {
        activate_tab(window_id, tab_id);
    } else {
        push_tabs(window_id);
    }
    Some(tab_id)
}

/// Reserve a tab id for a popup (called from on_before_popup).
pub fn reserve_popup(window_id: u32) -> u32 {
    let tab_id = next_tab_id();
    with_window(window_id, |c| c.pending_popups.push(tab_id));
    tab_id
}

fn adopt_popup(window_id: u32, view: BrowserView) {
    let tab_id = with_window(window_id, |c| {
        if c.pending_popups.is_empty() {
            None
        } else {
            Some(c.pending_popups.remove(0))
        }
    })
    .flatten()
    .unwrap_or_else(next_tab_id);
    with_window(window_id, |ctx| {
        let mut v = View::from(&view);
        ctx.content.add_child_view(Some(&mut v));
        ctx.tabs.push(Tab {
            id: tab_id,
            view: Some(view),
            url: String::new(),
            title: "Chargement…".into(),
            loading: true,
            can_back: false,
            can_forward: false,
            pinned: false,
            last_active: now(),
            warning: None,
            discarded: false,
            crashed: false,
        });
    });
    activate_tab(window_id, tab_id);
}

pub fn activate_tab(window_id: u32, tab_id: u32) {
    popup_close(window_id);
    // Restore a discarded tab first.
    let restore = with_window(window_id, |ctx| {
        ctx.tabs.iter().find(|t| t.id == tab_id && t.discarded).map(|t| (t.url.clone(), ctx.private, ctx.request_context.clone()))
    })
    .flatten();
    if let Some((url, private, rc)) = restore {
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
    push_tabs(window_id);
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

pub fn close_tab(window_id: u32, tab_id: u32) {
    let mut closing: Option<BrowserHost> = None;
    let (was_active, next) = with_window(window_id, |ctx| {
        let Some(idx) = ctx.tabs.iter().position(|t| t.id == tab_id) else {
            return (false, None);
        };
        let tab = ctx.tabs.remove(idx);
        if let Some(view) = &tab.view {
            closing = view.browser().and_then(|b| b.host());
            let mut v = View::from(view);
            ctx.content.remove_child_view(Some(&mut v));
        }
        let was_active = ctx.active == Some(tab_id);
        let next = if ctx.tabs.is_empty() {
            None
        } else {
            Some(ctx.tabs[idx.min(ctx.tabs.len() - 1)].id)
        };
        (was_active, next)
    })
    .unwrap_or((false, None));
    if let Some(h) = closing {
        h.close_browser(1);
    }
    match next {
        Some(n) if was_active => activate_tab(window_id, n),
        Some(_) => push_tabs(window_id),
        None => {
            with_window(window_id, |c| c.active = None);
            add_tab(window_id, NEW_TAB_URL, true);
        }
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
        h.close_browser(1);
    }
    push_tabs(window_id);
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
            if url.starts_with("nightfall://") {
                state::allow_internal_for_tab(tab_id);
            }
            if let Some(b) = tab_browser(window_id, tab_id) {
                if let Some(f) = b.main_frame() {
                    f.load_url(Some(&CefString::from(url)));
                }
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
}

pub fn tabs_state(window_id: u32) -> serde_json::Value {
    with_window(window_id, |ctx| {
        let tabs: Vec<TabState> = ctx
            .tabs
            .iter()
            .map(|t| {
                let blocked = t
                    .view
                    .as_ref()
                    .and_then(|v| v.browser())
                    .map(|b| super::client::blocked_count(b.identifier()))
                    .unwrap_or(0);
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
    let same = with_window(window_id, |ctx| ctx.popup.as_ref().map(|p| p.panel == panel).unwrap_or(false)).unwrap_or(false);
    popup_close(window_id);
    if same {
        return;
    }
    let Some((window, private, origin, area)) = with_window(window_id, |ctx| {
        let col = View::from(&ctx.column).bounds();
        let top = View::from(&ctx.top).bounds();
        let area = Size { width: col.x + col.width, height: col.y + col.height };
        ctx.window.clone().map(|w| (w, ctx.private, (col.x + top.x, col.y + top.y), area))
    })
    .flatten() else {
        return;
    };
    let panel_id: String = panel.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').take(24).collect();
    let url = format!(
        "nightfall://app/shell.html?mode=popup&w={window_id}&private={}&panel={panel_id}",
        if private { 1 } else { 0 }
    );
    let (w, h) = (width.clamp(160, 520), height.clamp(60, 640));
    let view = ui_view_url(window_id, false, &url, Arc::new(AtomicI32::new(h)), Arc::new(AtomicI32::new(w)));
    let mut v = View::from(&view);
    let Some(overlay) = window.add_overlay_view(Some(&mut v), DockingMode::CUSTOM, 1) else {
        state::log("menu déroulant : superposition refusée par CEF");
        return;
    };
    let right = origin.0 + anchor.x + anchor.width;
    let top = origin.1 + anchor.y + anchor.height + 6;
    place_popup(area, &overlay, right, top, w, h);
    overlay.set_visible(1);
    view.request_focus();
    with_window(window_id, |ctx| {
        ctx.popup = Some(Popup { panel: panel_id, view, overlay, right, top });
    });
}

/// `area`: size of the window's client area (where overlays live).
fn place_popup(win: Size, overlay: &OverlayController, right: i32, top: i32, w: i32, h: i32) {
    let max_h = (win.height - top - POPUP_MARGIN).max(80);
    let h = h.min(max_h);
    let x = (right - w).clamp(POPUP_MARGIN, (win.width - w - POPUP_MARGIN).max(POPUP_MARGIN));
    overlay.set_bounds(Some(&Rect { x, y: top, width: w, height: h }));
}

/// The popup asks for a new size (after rendering its content).
pub fn popup_resize(window_id: u32, width: i32, height: i32) {
    with_window(window_id, |ctx| {
        let col = View::from(&ctx.column).bounds();
        let area = Size { width: col.x + col.width, height: col.y + col.height };
        if let Some(p) = &ctx.popup {
            place_popup(area, &p.overlay, p.right, p.top, width.clamp(160, 520), height.clamp(60, 640));
        }
    });
}

pub fn popup_close(window_id: u32) {
    let popup = with_window(window_id, |ctx| ctx.popup.take()).flatten();
    if let Some(p) = popup {
        let host = p.view.browser().and_then(|b| b.host());
        p.overlay.destroy();
        if let Some(h) = host {
            h.close_browser(1);
        }
        push_event(window_id, &serde_json::json!({ "type": "popup_closed" }));
    }
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

pub fn push_tabs(window_id: u32) {
    if windows_busy() {
        super::ui_thread::on_ui_delayed(0, move || push_tabs(window_id));
        return;
    }
    let st = tabs_state(window_id);
    push_event(window_id, &st);
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
                        audible: false,
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

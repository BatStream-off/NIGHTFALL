//! Integrated title bar (prototype M-0005, branch only).
//!
//! The window is created without a system frame (`is_frameless`). The
//! interface views mark their empty areas with the CSS property
//! `app-region: drag` (buttons, tabs and fields use `no-drag`); CEF reports
//! these regions through `CefDragHandler::OnDraggableRegionsChanged` in the
//! view's own coordinates. They are translated here into window coordinates
//! and handed to `CefWindow::SetDraggableRegions`, which makes the window
//! answer "caption" (HTCAPTION) to the system there. Verified in CI: move
//! by dragging, double-click to maximise / restore (Windows) and zoom
//! (macOS). Windows 11 Snap Layouts: see `snap` (maximise button answers
//! HTMAXBUTTON, checked by the self test).
//!
//! - Windows: the minimise / maximise / close buttons are drawn by the
//!   interface (`win.*` commands); the resize borders are provided by CEF's
//!   frameless frame view.
//! - macOS: the system "traffic lights" stay (`with_standard_window_buttons`)
//!   and are centred in the tab strip with `get_titlebar_height`.
//!
//! An open dropdown (overlay view) is excluded from the draggable area: in
//! the (old) CEF window_view.cc that was read, the draggable region is tested
//! before the views, so a menu over the tab strip would lose its clicks. Not
//! verified for CEF 156; the self test only checks that the region is added.
//!
//! Prototype limits (to fix in the execution mission): one titlebar height
//! for all windows, `push_state` not rate-limited during a resize, `log`
//! unused.

use super::state;
use super::ui_thread::on_ui_delayed;
use super::windows;
use cef::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicI32, Ordering};

/// Frameless windows are used on the two supported platforms.
pub const FRAMELESS: bool = cfg!(any(target_os = "windows", target_os = "macos"));

/// Height of the band holding the macOS traffic lights (tab strip, or the
/// side bar header with vertical tabs), in DIP. Global (all windows), and
/// 40 until the first `apply_mode` sets the real value (38 or 52).
static TITLEBAR_HEIGHT: AtomicI32 = AtomicI32::new(40);

pub fn set_titlebar_height(h: i32) {
    TITLEBAR_HEIGHT.store(h, Ordering::Relaxed);
}

pub fn titlebar_height() -> f32 {
    TITLEBAR_HEIGHT.load(Ordering::Relaxed) as f32
}

pub fn platform() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    }
}

#[derive(Default)]
struct Regions {
    /// Regions of the top interface view, in its own coordinates.
    top: Vec<DraggableRegion>,
    /// Regions of the side view (vertical tabs).
    side: Vec<DraggableRegion>,
    /// Last list given to the window (window coordinates).
    applied: Vec<DraggableRegion>,
}

thread_local! {
    static REGIONS: RefCell<HashMap<u32, Regions>> = RefCell::new(HashMap::new());
}

/// `CefDragHandler::OnDraggableRegionsChanged` of an interface view.
pub fn on_regions_changed(window_id: u32, browser: &Browser, regions: &[DraggableRegion]) {
    let id = browser.identifier();
    let which = windows::with_window(window_id, |ctx| {
        let is = |v: &BrowserView| v.browser().map(|b| b.identifier() == id).unwrap_or(false);
        if is(&ctx.top) {
            Some(false)
        } else if is(&ctx.side) {
            Some(true)
        } else {
            None // dropdown popups never move the window
        }
    })
    .flatten();
    let Some(side) = which else { return };
    REGIONS.with(|r| {
        let mut r = r.borrow_mut();
        let e = r.entry(window_id).or_default();
        if side {
            e.side = regions.to_vec();
        } else {
            e.top = regions.to_vec();
        }
    });
    // Deferred: CEF may report regions while the window list is in use.
    on_ui_delayed(0, move || apply(window_id));
}

/// Recompute the window's draggable regions (after a layout change, a
/// dropdown opening or closing, or new regions from a view).
pub fn apply(window_id: u32) {
    if !FRAMELESS {
        return;
    }
    let origin = |v: &View| {
        let mut p = Point { x: 0, y: 0 };
        v.convert_point_to_window(Some(&mut p));
        p
    };
    let Some((window, top, side, popup)) = windows::with_window(window_id, |ctx| {
        let top = View::from(&ctx.top);
        let side = View::from(&ctx.side);
        let top = (top.is_drawn() != 0).then(|| origin(&top));
        let side = (side.is_drawn() != 0).then(|| origin(&side));
        let popup = ctx.popup.as_ref().map(|p| p.overlay.bounds());
        ctx.window.clone().map(|w| (w, top, side, popup))
    })
    .flatten() else {
        return;
    };
    let shift = |list: &[DraggableRegion], o: &Point| -> Vec<DraggableRegion> {
        list.iter()
            .map(|r| DraggableRegion {
                bounds: Rect { x: r.bounds.x + o.x, y: r.bounds.y + o.y, width: r.bounds.width, height: r.bounds.height },
                draggable: r.draggable,
            })
            .collect()
    };
    let out = REGIONS.with(|r| {
        let mut r = r.borrow_mut();
        let e = r.entry(window_id).or_default();
        let mut out = Vec::new();
        if let Some(o) = &top {
            out.extend(shift(&e.top, o));
        }
        if let Some(o) = &side {
            out.extend(shift(&e.side, o));
        }
        if let Some(b) = popup {
            // Applied last: subtracted from the draggable area.
            out.push(DraggableRegion { bounds: b, draggable: 0 });
        }
        e.applied = out.clone();
        out
    });
    window.set_draggable_regions(Some(&out));
    #[cfg(target_os = "windows")]
    snap::update(window_id, &window);
}

pub fn forget(window_id: u32) {
    REGIONS.with(|r| r.borrow_mut().remove(&window_id));
}

/// Window buttons drawn by the interface (Windows), also used by the
/// keyboard and the window menu.
pub fn command(window_id: u32, cmd: &str) {
    let Some(w) = windows::with_window(window_id, |c| c.window.clone()).flatten() else { return };
    match cmd {
        "win.minimize" => w.minimize(),
        "win.maximize" => {
            if w.is_maximized() != 0 {
                w.restore();
            } else {
                w.maximize();
            }
        }
        "win.close" => windows::close_window(window_id),
        _ => {}
    }
}

/// Tell the interface whether the window is maximised, fullscreen, active
/// (button icons, traffic-light space, dimmed title bar when inactive).
pub fn push_state(window_id: u32) {
    on_ui_delayed(0, move || {
        let Some(w) = windows::with_window(window_id, |c| c.window.clone()).flatten() else { return };
        let ev = serde_json::json!({
            "type": "winstate",
            "maximized": w.is_maximized() != 0,
            "fullscreen": w.is_fullscreen() != 0,
            "active": w.is_active() != 0,
        });
        windows::push_event(window_id, &ev);
        // The views may have moved (fullscreen hides the title bar band).
        apply(window_id);
    });
}

/// Is `(x, y)` (window coordinates) draggable, evaluated like CEF does:
/// regions applied in order, union for draggable ones, difference otherwise.
fn draggable_at(list: &[DraggableRegion], x: i32, y: i32) -> bool {
    let mut inside = false;
    for r in list {
        let b = &r.bounds;
        if x >= b.x && x < b.x + b.width && y >= b.y && y < b.y + b.height {
            inside = r.draggable != 0;
        }
    }
    inside
}

/// Diagnostic for the CI self test: regions given to the window, a free
/// draggable point and a non-draggable point in the tab strip and, on
/// Windows, what the real window procedure answers there (WM_NCHITTEST).
pub fn report(window_id: u32) -> serde_json::Value {
    let applied = REGIONS.with(|r| r.borrow().get(&window_id).map(|e| e.applied.clone()).unwrap_or_default());
    let (window, width) = match windows::with_window(window_id, |c| c.window.clone().map(|w| (w.clone(), w.bounds().width))).flatten() {
        Some(v) => v,
        None => return serde_json::json!({ "ok": false, "error": "pas de fenêtre" }),
    };
    let y = 18;
    // Free point: rightmost draggable point of the strip.
    let mut drag = None;
    let mut x = width - 2;
    while x > 0 {
        if draggable_at(&applied, x, y) {
            drag = Some((x, y));
            break;
        }
        x -= 6;
    }
    // Non-draggable point: centre of the first "no-drag" region of the band.
    let nodrag = applied
        .iter()
        .find(|r| r.draggable == 0 && r.bounds.y < 40 && r.bounds.width > 4)
        .map(|r| (r.bounds.x + r.bounds.width / 2, r.bounds.y + r.bounds.height / 2));
    #[cfg_attr(not(target_os = "windows"), allow(unused_mut))]
    let mut v = serde_json::json!({
        "ok": true,
        "frameless": FRAMELESS,
        "platform": platform(),
        "regions": applied.len(),
        "draggable": applied.iter().filter(|r| r.draggable != 0).count(),
        "dragPoint": drag,
        "noDragPoint": nodrag,
        "maximized": window.is_maximized() != 0,
    });
    #[cfg(target_os = "windows")]
    {
        v["hitTest"] = hit_test(&window, drag, nodrag);
    }
    v
}

#[cfg(target_os = "windows")]
fn hit_test(window: &Window, drag: Option<(i32, i32)>, nodrag: Option<(i32, i32)>) -> serde_json::Value {
    use windows_sys::Win32::UI::WindowsAndMessaging::{SendMessageW, WM_NCHITTEST};
    // cef-sys wraps the handle: HWND(*mut HWND__).
    let hwnd = window.window_handle().0 as *mut core::ffi::c_void;
    let area = window.client_area_bounds_in_screen();
    let scale = window.display().map(|d| d.device_scale_factor()).unwrap_or(1.0);
    let test = |p: Option<(i32, i32)>| -> i64 {
        let Some((x, y)) = p else { return -1 };
        let sx = ((area.x + x) as f32 * scale).round() as i32;
        let sy = ((area.y + y) as f32 * scale).round() as i32;
        let lparam = ((sy as u32 & 0xFFFF) << 16 | (sx as u32 & 0xFFFF)) as isize;
        // SAFETY: the handle is this process's live top-level window, and
        // WM_NCHITTEST only reads the coordinates packed in lParam.
        unsafe { SendMessageW(hwnd, WM_NCHITTEST, 0, lparam) as i64 }
    };
    let h = area.height;
    serde_json::json!({
        // HTCAPTION = 2, HTCLIENT = 1, HTLEFT = 10, HTBOTTOMRIGHT = 17
        "drag": test(drag),
        "noDrag": test(nodrag),
        "content": test(Some((area.width / 2, h - 60))),
        "leftEdge": test(Some((1, h / 2))),
        "bottomRight": test(Some((area.width - 2, h - 2))),
        // Centre of the interface's maximise button (Snap Layouts): HTMAXBUTTON = 9.
        "maxButton": test(Some((area.width - snap::BUTTON_W - snap::BUTTON_W / 2, 19))),
        "scale": scale,
    })
}

/// Windows 11 Snap Layouts: the layout grid appears when the pointer rests
/// on the maximise button, which the system finds by asking the window
/// what is under the pointer (WM_NCHITTEST). The button is drawn by the
/// interface (top right: minimise, maximise, close, 46 DIP each), so the
/// window procedure is subclassed to answer HTMAXBUTTON over it; a click
/// there then arrives as a non-client message and toggles maximise here.
#[cfg(target_os = "windows")]
mod snap {
    use cef::*;
    use std::sync::Mutex;
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{TrackMouseEvent, TME_LEAVE, TME_NONCLIENT, TRACKMOUSEEVENT};
    use windows_sys::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindowRect, HTMAXBUTTON, WM_NCHITTEST, WM_NCLBUTTONDBLCLK, WM_NCLBUTTONDOWN, WM_NCLBUTTONUP, WM_NCMOUSELEAVE, WM_NCMOUSEMOVE};

    /// Width of each window button and height of the band, in DIP (shell.css).
    pub const BUTTON_W: i32 = 46;
    const BAND_H: i32 = 38;

    struct Info {
        hwnd: isize,
        window_id: u32,
        scale: f32,
        enabled: bool,
        hover: bool,
    }

    static WINDOWS: Mutex<Vec<Info>> = Mutex::new(Vec::new());

    /// Install once per window, then keep the scale and state current.
    pub fn update(window_id: u32, window: &Window) {
        let hwnd = window.window_handle().0 as isize;
        if hwnd == 0 {
            return;
        }
        let scale = window.display().map(|d| d.device_scale_factor()).unwrap_or(1.0);
        let enabled = window.is_fullscreen() == 0;
        let ids = super::windows::window_ids();
        let Ok(mut list) = WINDOWS.lock() else { return };
        if !ids.is_empty() {
            list.retain(|i| ids.contains(&i.window_id));
        }
        if let Some(i) = list.iter_mut().find(|i| i.hwnd == hwnd) {
            i.scale = scale;
            i.enabled = enabled;
            return;
        }
        // SAFETY: a live top-level window of this process; the procedure
        // only reads coordinates and calls DefSubclassProc.
        let ok = unsafe { SetWindowSubclass(hwnd as HWND, Some(proc), 0x4E46, 0) } != 0;
        if ok {
            list.push(Info { hwnd, window_id, scale, enabled, hover: false });
        }
    }

    fn over_button(hwnd: HWND, lp: LPARAM) -> Option<u32> {
        let x = (lp & 0xFFFF) as u16 as i16 as i32;
        let y = ((lp >> 16) & 0xFFFF) as u16 as i16 as i32;
        let list = WINDOWS.try_lock().ok()?;
        let info = list.iter().find(|i| i.hwnd == hwnd as isize && i.enabled)?;
        let mut r = RECT { left: 0, top: 0, right: 0, bottom: 0 };
        // SAFETY: valid window handle and RECT out-pointer.
        if unsafe { GetWindowRect(hwnd, &mut r) } == 0 {
            return None;
        }
        let px = |dip: i32| (dip as f32 * info.scale).round() as i32;
        let inside = x >= r.right - px(2 * BUTTON_W) && x < r.right - px(BUTTON_W) && y >= r.top && y < r.top + px(BAND_H);
        inside.then_some(info.window_id)
    }

    fn set_hover(hwnd: HWND, on: bool) {
        let changed = WINDOWS.try_lock().ok().and_then(|mut list| {
            let i = list.iter_mut().find(|i| i.hwnd == hwnd as isize)?;
            (i.hover != on).then(|| {
                i.hover = on;
                i.window_id
            })
        });
        if let Some(id) = changed {
            super::on_ui_delayed(0, move || super::windows::push_event(id, &serde_json::json!({ "type": "winmaxhover", "on": on })));
        }
    }

    unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM, _id: usize, _data: usize) -> LRESULT {
        let max = HTMAXBUTTON as usize;
        match msg {
            WM_NCHITTEST => {
                if over_button(hwnd, lp).is_some() {
                    return HTMAXBUTTON as LRESULT;
                }
            }
            WM_NCLBUTTONDOWN | WM_NCLBUTTONDBLCLK if wp == max => return 0,
            WM_NCLBUTTONUP if wp == max => {
                if let Some(id) = over_button(hwnd, lp) {
                    super::on_ui_delayed(0, move || super::command(id, "win.maximize"));
                }
                return 0;
            }
            WM_NCMOUSEMOVE => {
                let on = wp == max;
                if on {
                    let mut t = TRACKMOUSEEVENT { cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE | TME_NONCLIENT, hwndTrack: hwnd, dwHoverTime: 0 };
                    // SAFETY: valid structure for this window.
                    unsafe { TrackMouseEvent(&mut t) };
                }
                set_hover(hwnd, on);
            }
            WM_NCMOUSELEAVE => set_hover(hwnd, false),
            _ => {}
        }
        // SAFETY: forwarding the unchanged message to the next procedure.
        unsafe { DefSubclassProc(hwnd, msg, wp, lp) }
    }
}

#[allow(dead_code)]
fn log(s: &str) {
    state::log(&format!("barre de titre : {s}"));
}

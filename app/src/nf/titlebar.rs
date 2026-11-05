//! Integrated title bar (prototype M-0005, branch only).
//!
//! The window is created without a system frame (`is_frameless`). The
//! interface views mark their empty areas with the CSS property
//! `app-region: drag` (buttons, tabs and fields use `no-drag`); CEF reports
//! these regions through `CefDragHandler::OnDraggableRegionsChanged` in the
//! view's own coordinates. They are translated here into window coordinates
//! and handed to `CefWindow::SetDraggableRegions`, which makes the window
//! answer "caption" to the system there (move, Windows Snap by dragging,
//! double-click to maximise on Windows).
//!
//! - Windows: the minimise / maximise / close buttons are drawn by the
//!   interface (`win.*` commands); the resize borders are provided by CEF's
//!   frameless frame view.
//! - macOS: the system "traffic lights" stay (`with_standard_window_buttons`)
//!   and are centred in the tab strip with `get_titlebar_height`.
//!
//! An open dropdown (overlay view) is excluded from the draggable area:
//! CEF tests the draggable region before the views, so a menu drawn over the
//! tab strip would otherwise not receive its clicks.

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
/// side bar header with vertical tabs), in DIP.
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
        "scale": scale,
    })
}

#[allow(dead_code)]
fn log(s: &str) {
    state::log(&format!("barre de titre : {s}"));
}

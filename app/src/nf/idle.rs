//! Inactivity tracking for auto-lock, and sleep/resume detection.

use std::sync::atomic::{AtomicI64, Ordering};

static LAST_ACTIVITY: AtomicI64 = AtomicI64::new(0);

/// Record user activity seen by NIGHTFALL (keyboard events, interface clicks).
pub fn touch() {
    LAST_ACTIVITY.store(nf_core::unix_now(), Ordering::Relaxed);
}

/// Seconds since the last user input. Uses the OS idle time when available
/// (covers mouse and keyboard in web pages too), else NIGHTFALL's own events.
pub fn idle_seconds() -> i64 {
    if let Some(s) = os_idle_seconds() {
        return s;
    }
    let last = LAST_ACTIVITY.load(Ordering::Relaxed);
    if last == 0 {
        touch();
        return 0;
    }
    nf_core::unix_now() - last
}

#[cfg(target_os = "windows")]
fn os_idle_seconds() -> Option<i64> {
    use windows_sys::Win32::System::SystemInformation::GetTickCount;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
    let mut info = LASTINPUTINFO { cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
    let ok = unsafe { GetLastInputInfo(&mut info) };
    if ok == 0 {
        return None;
    }
    let now = unsafe { GetTickCount() };
    Some((now.wrapping_sub(info.dwTime) / 1000) as i64)
}

#[cfg(target_os = "macos")]
fn os_idle_seconds() -> Option<i64> {
    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGEventSourceSecondsSinceLastEventType(state_id: i32, event_type: u32) -> f64;
    }
    // kCGEventSourceStateCombinedSessionState = 0, kCGAnyInputEventType = ~0
    let s = unsafe { CGEventSourceSecondsSinceLastEventType(0, u32::MAX) };
    if s.is_finite() && s >= 0.0 {
        Some(s as i64)
    } else {
        None
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn os_idle_seconds() -> Option<i64> {
    None
}

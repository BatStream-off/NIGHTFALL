//! Nothing acts behind the lock screen.
//!
//! Locking hides the tabs but their pages keep running. While NIGHTFALL is
//! locked:
//! - keyboard shortcuts do nothing (see `api::shortcut`), except Emergency
//!   Lockdown;
//! - every tab is muted and its media paused; media that starts again (a
//!   page script, the next video of a playlist) is paused at once;
//! - VideoGuard sends no request and its page script does nothing (no
//!   segment skip, no notice, no ad fast-forward).
//!
//! At unlock the sound comes back (tabs muted by the user stay muted), media
//! stay paused, VideoGuard resumes and asks again for the videos it skipped.
//!
//! "Locked" starts when `api::lock_now` begins, before the keys are
//! forgotten (the Session Vault is sealed first, which takes about a
//! second): see [`locked`].

use super::state::{self, core};
use super::windows;
use cef::*;
use std::cell::RefCell;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

/// Set by `lock_now` until the keys are forgotten.
static LOCKING: AtomicBool = AtomicBool::new(false);

/// NIGHTFALL is locked (or being locked). Takes the core lock: never call it
/// while holding `core()`.
pub fn locked() -> bool {
    LOCKING.load(Ordering::SeqCst) || !core().is_unlocked()
}

pub fn is_locking() -> bool {
    LOCKING.load(Ordering::SeqCst)
}

pub fn begin_locking() {
    LOCKING.store(true, Ordering::SeqCst);
}

pub fn end_locking() {
    LOCKING.store(false, Ordering::SeqCst);
}

/// Injected into every frame of every tab at lock (and into documents that
/// start loading while locked): pause the media, pause any media that starts
/// playing, suspend VideoGuard's page script.
const PAUSE_JS: &str = "(function(){try{var k=Symbol.for('nightfall.locked');if(!window[k]){var f=function(e){var m=e.target;if(m&&typeof m.pause==='function'){try{m.pause();}catch(_){}}};window[k]=f;document.addEventListener('play',f,true);}document.querySelectorAll('video,audio').forEach(function(m){try{m.pause();}catch(_){}});if(window.__nightfallVideoGuardSuspend)window.__nightfallVideoGuardSuspend(true);}catch(_){}})();";

/// At unlock: media may play again (they stay paused), VideoGuard resumes.
const RESUME_JS: &str = "(function(){try{var k=Symbol.for('nightfall.locked');var f=window[k];if(f){document.removeEventListener('play',f,true);delete window[k];}if(window.__nightfallVideoGuardSuspend)window.__nightfallVideoGuardSuspend(false);}catch(_){}})();";

thread_local! {
    /// Browsers muted by the lock (not by the user), unmuted at unlock.
    static MUTED_BY_LOCK: RefCell<HashSet<i32>> = RefCell::new(HashSet::new());
}

/// Videos whose VideoGuard request was not sent because NIGHTFALL was
/// locked: (window, tab, address).
static DEFERRED_VIDEOGUARD: Mutex<Vec<(u32, u32, String)>> = Mutex::new(Vec::new());

fn tab_browsers() -> Vec<Browser> {
    let mut out = Vec::new();
    for id in windows::window_ids() {
        // Collected first; CEF is called after (see windows::with_window).
        let list = windows::with_window(id, |c| c.tabs.iter().filter_map(|t| t.view.as_ref().and_then(|v| v.browser())).collect::<Vec<_>>());
        out.extend(list.unwrap_or_default());
    }
    out
}

fn each_frame(b: &Browser, f: impl Fn(&Frame)) {
    let mut list = CefStringList::new();
    b.frame_identifiers(Some(&mut list));
    let ids: Vec<String> = list.into_iter().collect();
    if ids.is_empty() {
        if let Some(main) = b.main_frame() {
            f(&main);
        }
        return;
    }
    for id in ids {
        if let Some(frame) = b.frame_by_identifier(Some(&CefString::from(id.as_str()))) {
            f(&frame);
        }
    }
}

fn run(frame: &Frame, js: &str) {
    frame.execute_java_script(Some(&CefString::from(js)), None, 0);
}

fn mute_for_lock(b: &Browser) {
    if let Some(h) = b.host() {
        if h.is_audio_muted() == 0 {
            h.set_audio_muted(1);
            let id = b.identifier();
            MUTED_BY_LOCK.with(|m| m.borrow_mut().insert(id));
        }
    }
}

/// At lock (UI thread): mute every tab, pause its media, suspend VideoGuard.
pub fn on_lock() {
    let browsers = tab_browsers();
    for b in &browsers {
        mute_for_lock(b);
        each_frame(b, |f| run(f, PAUSE_JS));
    }
    state::log(&format!("verrouillage : son coupé et médias mis en pause dans {} onglet(s)", browsers.len()));
}

/// At unlock (UI thread), once the tabs are shown again.
pub fn on_unlock() {
    let muted = MUTED_BY_LOCK.with(|m| std::mem::take(&mut *m.borrow_mut()));
    let vg = core().videoguard.clone();
    for b in tab_browsers() {
        if muted.contains(&b.identifier()) {
            if let Some(h) = b.host() {
                h.set_audio_muted(0);
            }
        }
        each_frame(&b, |f| {
            run(f, RESUME_JS);
            // YouTube documents loaded while locked have no VideoGuard yet
            // (the script ignores a second injection, options are updated).
            let url = CefString::from(&f.url()).to_string();
            if let Some(js) = nf_videoguard::page_script(&url, &vg) {
                run(f, &js);
            }
        });
    }
    let deferred = DEFERRED_VIDEOGUARD.lock().map(|mut d| std::mem::take(&mut *d)).unwrap_or_default();
    for (w, t, url) in deferred {
        if windows::tab_url(w, t).as_deref() == Some(url.as_str()) {
            super::client::videoguard_on_navigation(w, t, &url);
        }
    }
    windows::push_tabs_all();
}

/// A tab browser created while locked (a hidden page opened a popup): muted.
pub fn on_tab_created(b: &Browser) {
    if locked() {
        mute_for_lock(b);
    }
}

/// A document starts loading in a tab: paused media while locked.
pub fn on_load_start(frame: &Frame) {
    if locked() {
        run(frame, PAUSE_JS);
    }
}

/// VideoGuard request not sent while locked: asked again at unlock if the
/// tab still shows this address.
pub fn defer_videoguard(window_id: u32, tab_id: u32, url: &str) {
    #[cfg(feature = "selftest")]
    VG_DEFERRED.fetch_add(1, Ordering::SeqCst);
    if let Ok(mut d) = DEFERRED_VIDEOGUARD.lock() {
        d.retain(|(w, t, _)| !(*w == window_id && *t == tab_id));
        d.push((window_id, tab_id, url.to_string()));
    }
}

// ------------------------------------------------- self test (CI only)

/// VideoGuard requests started / not sent because locked.
#[cfg(feature = "selftest")]
pub static VG_FETCHES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
#[cfg(feature = "selftest")]
pub static VG_DEFERRED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
/// Shortcut actions seen by `api::shortcut`: (action, carried out).
#[cfg(feature = "selftest")]
pub static SHORTCUTS: Mutex<Vec<(String, bool)>> = Mutex::new(Vec::new());

pub fn shortcut_seen(_action: &str, _done: bool) {
    #[cfg(feature = "selftest")]
    if let Ok(mut s) = SHORTCUTS.lock() {
        s.push((_action.to_string(), _done));
    }
}

pub fn videoguard_fetch_started() {
    #[cfg(feature = "selftest")]
    VG_FETCHES.fetch_add(1, Ordering::SeqCst);
}

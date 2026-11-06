//! Session Vault (cookies sealed while locked or closed) and tab session restore.

use super::state::{self, core};
use super::ui_thread::on_ui_delayed;
use super::windows;
use cef::*;
use nf_security::session_vault::{self, CookieRecord};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// Chromium base::Time: microseconds since 1601-01-01 UTC.
const WINDOWS_TO_UNIX_SECONDS: i64 = 11_644_473_600;

fn basetime_to_unix(v: i64) -> i64 {
    v / 1_000_000 - WINDOWS_TO_UNIX_SECONDS
}

fn unix_to_basetime(s: i64) -> i64 {
    (s + WINDOWS_TO_UNIX_SECONDS) * 1_000_000
}

/// True once cookies were restored in this session: an empty jar sealed
/// afterwards is a real (empty) state and may overwrite the vault.
static RESTORED: AtomicBool = AtomicBool::new(false);
static SEALING: AtomicBool = AtomicBool::new(false);

fn same_site_to_i32(s: CookieSameSite) -> i32 {
    if s == CookieSameSite::NO_RESTRICTION {
        1
    } else if s == CookieSameSite::LAX_MODE {
        2
    } else if s == CookieSameSite::STRICT_MODE {
        3
    } else {
        0
    }
}

fn same_site_from_i32(v: i32) -> CookieSameSite {
    match v {
        1 => CookieSameSite::NO_RESTRICTION,
        2 => CookieSameSite::LAX_MODE,
        3 => CookieSameSite::STRICT_MODE,
        _ => CookieSameSite::UNSPECIFIED,
    }
}

fn priority_to_i32(p: CookiePriority) -> i32 {
    if p == CookiePriority::LOW {
        0
    } else if p == CookiePriority::HIGH {
        2
    } else {
        1
    }
}

fn priority_from_i32(v: i32) -> CookiePriority {
    match v {
        0 => CookiePriority::LOW,
        2 => CookiePriority::HIGH,
        _ => CookiePriority::MEDIUM,
    }
}

wrap_cookie_visitor! {
    struct VaultVisitor {
        collected: Arc<Mutex<Vec<CookieRecord>>>,
    }

    impl CookieVisitor {
        fn visit(&self, cookie: Option<&Cookie>, _count: i32, _total: i32, delete_cookie: Option<&mut i32>) -> i32 {
            if let Some(c) = cookie {
                let rec = CookieRecord {
                    name: c.name.to_string(),
                    value: c.value.to_string(),
                    domain: c.domain.to_string(),
                    path: c.path.to_string(),
                    secure: c.secure != 0,
                    httponly: c.httponly != 0,
                    expires: if c.has_expires != 0 { Some(basetime_to_unix(c.expires.val)) } else { None },
                    same_site: same_site_to_i32(c.same_site),
                    priority: priority_to_i32(c.priority),
                };
                if let Ok(mut v) = self.collected.lock() {
                    v.push(rec);
                }
            }
            // Remove the cookie from Chromium's store while it is sealed.
            if let Some(d) = delete_cookie {
                *d = 1;
            }
            1
        }
    }
}

/// Seal every cookie into the Session Vault and clear Chromium's cookie
/// store, then call `done` on the UI thread.
pub fn seal_cookies_then(done: impl FnOnce() + Send + 'static) {
    let (enabled, key, path) = {
        let c = core();
        let key = c.unlocked.as_ref().and_then(|u| u.cookies_key.clone());
        (c.settings.session_vault, key, c.paths.session_vault())
    };
    let (true, Some(key)) = (enabled, key) else {
        done();
        return;
    };
    if SEALING.swap(true, Ordering::SeqCst) {
        done();
        return;
    }
    let Some(manager) = cookie_manager_get_global_manager(None) else {
        SEALING.store(false, Ordering::SeqCst);
        done();
        return;
    };
    let collected = Arc::new(Mutex::new(Vec::new()));
    let mut visitor = VaultVisitor::new(collected.clone());
    manager.visit_all_cookies(Some(&mut visitor));
    on_ui_delayed(900, move || {
        let cookies = collected.lock().map(|v| v.clone()).unwrap_or_default();
        if super::browsing_data::CLEAR_COOKIES_ON_CLOSE.swap(false, Ordering::SeqCst) {
            // Confidentialité › effacer les cookies à la fermeture: the
            // cookies just left Chromium and are not kept.
            let _ = std::fs::remove_file(&path);
            state::log("Session Vault : cookies effacés à la fermeture (aucune copie gardée)");
        } else if !cookies.is_empty() || RESTORED.load(Ordering::SeqCst) {
            match session_vault::seal(&key, &cookies, nf_core::unix_now()) {
                Ok(bytes) => {
                    if let Err(e) = nf_core::paths::write_private(&path, &bytes) {
                        state::log(&format!("Session Vault : écriture impossible ({e})"));
                    }
                }
                Err(e) => state::log(&format!("Session Vault : chiffrement impossible ({e})")),
            }
        }
        if let Some(m) = cookie_manager_get_global_manager(None) {
            m.flush_store(None);
        }
        RESTORED.store(false, Ordering::SeqCst);
        SEALING.store(false, Ordering::SeqCst);
        done();
    });
}

/// Restore cookies from the Session Vault after unlock.
pub fn restore_cookies() {
    let (key, path) = {
        let c = core();
        (c.unlocked.as_ref().and_then(|u| u.cookies_key.clone()), c.paths.session_vault())
    };
    let Some(key) = key else { return };
    RESTORED.store(true, Ordering::SeqCst);
    let Ok(bytes) = std::fs::read(&path) else { return };
    let cookies = match session_vault::open(&key, &bytes, nf_core::unix_now()) {
        Ok(c) => c,
        Err(_) => {
            state::log("Session Vault illisible (clé détruite ou fichier altéré) : sessions non restaurées");
            return;
        }
    };
    let Some(manager) = cookie_manager_get_global_manager(None) else { return };
    for r in cookies {
        let cookie = Cookie {
            name: CefString::from(r.name.as_str()),
            value: CefString::from(r.value.as_str()),
            domain: CefString::from(r.domain.as_str()),
            path: CefString::from(r.path.as_str()),
            secure: r.secure as i32,
            httponly: r.httponly as i32,
            has_expires: r.expires.is_some() as i32,
            expires: Basetime { val: r.expires.map(unix_to_basetime).unwrap_or(0) },
            same_site: same_site_from_i32(r.same_site),
            priority: priority_from_i32(r.priority),
            ..Default::default()
        };
        manager.set_cookie(Some(&CefString::from(r.url().as_str())), Some(&cookie), None);
    }
    // The sealed copy stays on disk (still encrypted) and is overwritten at
    // the next lock or exit.
}

/// Persist the open tabs (for "restore session").
pub fn save_session_snapshot() {
    let tabs = windows::session_tabs();
    let c = core();
    if !c.settings.restore_session {
        return;
    }
    if let Some(s) = c.storage() {
        let _ = s.save_session(&tabs);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_conversion_roundtrip() {
        let unix = 1_800_000_000;
        assert_eq!(basetime_to_unix(unix_to_basetime(unix)), unix);
    }
}

//! Internal API used by NIGHTFALL's own pages (`nightfall://app/api/<cmd>`).
//!
//! Called on CEF's IO thread by the scheme handler, only for requests whose
//! frame is a NIGHTFALL page (see scheme.rs). Commands that touch views are
//! forwarded to the UI thread.

use super::state::{self, core, BrowserKind, Core, Unlocked};
use super::ui_thread::{background, on_ui, on_ui_delayed};
use super::{downloads, security, session, windows};
use cef::ImplBrowser;
use nf_auth::{ArgonParams, NightfallLock, UnlockError};
use nf_emergency::{PurgeCategory, PurgePlan};
use serde_json::{json, Value};

pub struct Caller {
    pub kind: BrowserKind,
}

impl Caller {
    fn window_id(&self) -> u32 {
        match self.kind {
            BrowserKind::Ui { window_id, .. } | BrowserKind::Tab { window_id, .. } => window_id,
        }
    }
}

fn ok() -> Value {
    json!({ "ok": true })
}

fn err(msg: impl std::fmt::Display) -> Value {
    json!({ "ok": false, "error": msg.to_string() })
}

fn s<'a>(v: &'a Value, k: &str) -> &'a str {
    v.get(k).and_then(Value::as_str).unwrap_or("")
}

fn now() -> i64 {
    nf_core::unix_now()
}

/// Commands allowed before unlock.
const PRE_UNLOCK: &[&str] = &["state", "setup", "unlock", "unlock.recovery", "recovery.confirm", "ui.activity", "ui.expand"];

pub fn handle(caller: &Caller, cmd: &str, body: &Value) -> Value {
    let unlocked = core().is_unlocked();
    if !unlocked && !PRE_UNLOCK.contains(&cmd) {
        return err("NIGHTFALL est verrouillé");
    }
    let w = caller.window_id();
    match cmd {
        "state" => {
            if matches!(caller.kind, BrowserKind::Ui { side: false, .. }) {
                state::log(&format!("interface prête (fenêtre {w})"));
            }
            state_json(w)
        }
        "setup" => setup(body),
        "recovery.confirm" => recovery_confirm(),
        "unlock" => unlock(body),
        "unlock.recovery" => unlock_recovery(body),
        "lock" => {
            on_ui(lock_now);
            ok()
        }
        "ui.activity" => {
            super::idle::touch();
            ok()
        }
        "ui.expand" => {
            // Only the top interface view of a window may resize itself.
            if !matches!(caller.kind, BrowserKind::Ui { side: false, .. }) || !core().is_unlocked() {
                return err("interdit");
            }
            let h = body.get("height").and_then(Value::as_i64).unwrap_or(88).clamp(40, 560) as i32;
            on_ui(move || windows::set_top_height(w, h));
            ok()
        }

        // ---- tabs & navigation (UI thread)
        "tab.new" => {
            let url = s(body, "url").to_string();
            on_ui(move || {
                windows::add_tab(w, if url.is_empty() { windows::NEW_TAB_URL } else { &url }, true);
            });
            ok()
        }
        "tab.close" | "tab.activate" | "tab.pin" | "tab.unpin" | "tab.discard" | "tab.move" => {
            let id = body.get("id").and_then(Value::as_u64).unwrap_or(0) as u32;
            let index = body.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
            let cmd = cmd.to_string();
            on_ui(move || match cmd.as_str() {
                "tab.close" => windows::close_tab(w, id),
                "tab.activate" => windows::activate_tab(w, id),
                "tab.pin" => windows::set_pinned(w, id, true),
                "tab.unpin" => windows::set_pinned(w, id, false),
                "tab.discard" => windows::discard_tab(w, id),
                "tab.move" => windows::move_tab(w, id, index),
                _ => {}
            });
            ok()
        }
        "tabs" => {
            // Answered from the UI thread on next push; request a push.
            on_ui(move || windows::push_tabs(w));
            ok()
        }
        "navigate" => {
            let input = s(body, "input").trim().to_string();
            let url = resolve_input(&input, &core().settings);
            let target = url.clone();
            on_ui(move || windows::navigate_active(w, &target));
            json!({ "ok": true, "url": url })
        }
        "nav.back" | "nav.forward" | "nav.reload" | "nav.stop" | "nav.home" => {
            let cmd = cmd.to_string();
            on_ui(move || shortcut(w, &cmd));
            ok()
        }
        "window.new" => {
            windows::create_window(false, Vec::new());
            ok()
        }
        "window.private" => {
            windows::create_window(true, Vec::new());
            ok()
        }
        "window.close" => {
            on_ui(move || windows::close_window(w));
            ok()
        }
        "page.open" => {
            let page = s(body, "page").to_string();
            let allowed = ["settings", "security", "history", "bookmarks", "downloads", "passwords", "extensions", "about", "newtab"];
            if !allowed.contains(&page.as_str()) {
                return err("page inconnue");
            }
            on_ui(move || {
                windows::add_tab(w, &format!("nightfall://app/{page}.html"), true);
            });
            ok()
        }
        "page.info" => page_info(w),
        "permission.answer" => {
            let id: u64 = s(body, "id").parse().unwrap_or(0);
            let allow = body.get("allow").and_then(Value::as_bool).unwrap_or(false);
            on_ui(move || super::permissions::answer(id, allow));
            ok()
        }
        "devtools" => {
            on_ui(move || windows::show_devtools(w));
            ok()
        }

        // ---- settings
        "settings.get" => {
            let c = core();
            json!({ "ok": true, "settings": c.settings, "prefs": c.prefs, "engines": ["duckduckgo", "google", "brave", "startpage", "qwant", "ecosia", "bing"] })
        }
        "settings.set" => settings_set(body),

        // ---- bookmarks
        "bookmarks.list" => {
            let c = core();
            let st = c.storage().unwrap();
            json!({ "ok": true, "folders": st.folders().unwrap_or_default(), "bookmarks": st.bookmarks().unwrap_or_default() })
        }
        "bookmarks.search" => {
            let c = core();
            json!({ "ok": true, "bookmarks": c.storage().unwrap().search_bookmarks(s(body, "q")).unwrap_or_default() })
        }
        "bookmarks.add" => {
            let folder = body.get("folder").and_then(Value::as_i64);
            let r = core().storage().unwrap().add_bookmark(folder, s(body, "title"), s(body, "url"), now());
            security::invalidate_detector();
            r.map(|id| json!({ "ok": true, "id": id })).unwrap_or_else(err)
        }
        "bookmarks.update" => {
            let id = body.get("id").and_then(Value::as_i64).unwrap_or(0);
            let folder = body.get("folder").and_then(Value::as_i64);
            core().storage().unwrap().update_bookmark(id, s(body, "title"), s(body, "url"), folder, now()).map(|_| ok()).unwrap_or_else(err)
        }
        "bookmarks.delete" => {
            let id = body.get("id").and_then(Value::as_i64).unwrap_or(0);
            core().storage().unwrap().delete_bookmark(id).map(|_| ok()).unwrap_or_else(err)
        }
        "bookmarks.folder.add" => {
            let parent = body.get("parent").and_then(Value::as_i64);
            core().storage().unwrap().add_folder(parent, s(body, "title"), now()).map(|id| json!({"ok": true, "id": id})).unwrap_or_else(err)
        }
        "bookmarks.folder.rename" => {
            let id = body.get("id").and_then(Value::as_i64).unwrap_or(0);
            core().storage().unwrap().rename_folder(id, s(body, "title"), now()).map(|_| ok()).unwrap_or_else(err)
        }
        "bookmarks.folder.delete" => {
            let id = body.get("id").and_then(Value::as_i64).unwrap_or(0);
            core().storage().unwrap().delete_folder(id).map(|_| ok()).unwrap_or_else(err)
        }
        "bookmark.toggle" => {
            on_ui(move || shortcut(w, "bookmark.toggle"));
            ok()
        }

        // ---- history
        "history.query" => {
            let q: nf_storage::HistoryQuery = serde_json::from_value(body.clone()).unwrap_or_default();
            json!({ "ok": true, "entries": core().storage().unwrap().history(&q).unwrap_or_default() })
        }
        "history.delete" => {
            let id = body.get("id").and_then(Value::as_i64).unwrap_or(0);
            core().storage().unwrap().delete_history_entry(id).map(|_| ok()).unwrap_or_else(err)
        }
        "history.deleteRange" => {
            let from = body.get("from").and_then(Value::as_i64).unwrap_or(0);
            let to = body.get("to").and_then(Value::as_i64).unwrap_or(i64::MAX);
            core().storage().unwrap().delete_history_range(from, to).map(|n| json!({"ok": true, "deleted": n})).unwrap_or_else(err)
        }
        "history.clear" => core().storage().unwrap().clear_history().map(|_| ok()).unwrap_or_else(err),

        // ---- new tab page
        "newtab.data" => {
            let c = core();
            let top = c.storage().and_then(|s| s.top_sites(8).ok()).unwrap_or_default();
            json!({ "ok": true, "shortcuts": c.settings.shortcuts, "wallpaper": c.settings.wallpaper, "topSites": top, "engine": c.settings.search_engine })
        }

        // ---- downloads (UI thread data)
        "downloads.list" => {
            // Download state lives on the UI thread; the page polls this.
            let (tx, rx) = std::sync::mpsc::channel();
            on_ui(move || {
                let _ = tx.send(serde_json::to_value(downloads::list()).unwrap_or(Value::Null));
            });
            let list = rx.recv_timeout(std::time::Duration::from_secs(2)).unwrap_or(Value::Array(vec![]));
            json!({ "ok": true, "downloads": list })
        }
        "downloads.pause" | "downloads.resume" | "downloads.cancel" => {
            let key = s(body, "key").to_string();
            let action = cmd.trim_start_matches("downloads.").to_string();
            on_ui(move || {
                downloads::control(&key, &action);
            });
            ok()
        }
        "downloads.show" => {
            let key = s(body, "key").to_string();
            on_ui(move || {
                if let Some(p) = downloads::path_for(&key) {
                    downloads::show_in_folder(&p);
                }
            });
            ok()
        }
        "downloads.clear" => core().storage().unwrap().clear_download_history().map(|_| ok()).unwrap_or_else(err),

        // ---- password vault
        "vault.list" => {
            let c = core();
            let v = c.vault().unwrap();
            let q = s(body, "q");
            json!({ "ok": true, "entries": if q.is_empty() { v.list() } else { v.search(q) } })
        }
        "vault.forPage" => {
            let url = windows_active_url(w);
            let c = core();
            json!({ "ok": true, "origin": nf_vault::normalize_origin(&url).ok(), "entries": c.vault().unwrap().for_page(&url) })
        }
        "vault.add" => {
            let mut c = core();
            let r = c.vault_mut().unwrap().add(s(body, "origin"), s(body, "username"), s(body, "password"), s(body, "notes"), now());
            drop(c);
            security::invalidate_detector();
            r.map(|id| json!({"ok": true, "id": id})).unwrap_or_else(err)
        }
        "vault.saveForPage" => {
            let url = windows_active_url(w);
            let mut c = core();
            c.vault_mut().unwrap().add(&url, s(body, "username"), s(body, "password"), "", now()).map(|id| json!({"ok": true, "id": id})).unwrap_or_else(err)
        }
        "vault.update" => {
            let mut c = core();
            c.vault_mut().unwrap().update(s(body, "id"), s(body, "username"), s(body, "password"), s(body, "notes"), now()).map(|_| ok()).unwrap_or_else(err)
        }
        "vault.delete" => {
            let mut c = core();
            c.vault_mut().unwrap().delete(s(body, "id")).map(|_| ok()).unwrap_or_else(err)
        }
        "vault.reveal" => {
            // Revealing a password requires the PIN again.
            if let Err(e) = verify_pin(s(body, "pin")) {
                return e;
            }
            let c = core();
            match c.vault().unwrap().get(s(body, "id")) {
                Some(cred) => json!({ "ok": true, "password": cred.password }),
                None => err("entrée introuvable"),
            }
        }
        "vault.fill" => vault_fill(w, s(body, "id")),
        "vault.generate" => {
            let opts: nf_vault::GeneratorOptions = serde_json::from_value(body.clone()).unwrap_or_default();
            nf_vault::generate_password(opts).map(|p| json!({"ok": true, "password": p})).unwrap_or_else(err)
        }

        // ---- Security Center
        "security.status" => json!({ "ok": true, "status": security::status_json() }),
        "shield.set" => shield_set(body),
        "shield.toggleSite" => {
            let url = windows_active_url(w);
            let sh = state::shield();
            let active = sh.is_active_for(&url);
            sh.set_site_allowed(&url, active);
            core().save_settings();
            on_ui(move || shortcut(w, "nav.reload"));
            json!({ "ok": true, "active": !active })
        }
        "shield.update" => {
            background(
                || state::shield().update_lists(nf_core::unix_now()),
                |errors| {
                    core().save_settings();
                    windows::push_event_all(&json!({ "type": "shield_updated", "errors": errors }));
                },
            );
            json!({ "ok": true, "started": true })
        }
        "videoguard.set" => {
            if let Ok(cfg) = serde_json::from_value::<nf_videoguard::VideoGuardConfig>(body.get("config").cloned().unwrap_or(Value::Null)) {
                let mut c = core();
                c.videoguard = cfg;
                c.save_settings();
                ok()
            } else {
                err("configuration invalide")
            }
        }
        "extensions.list" => extensions_list(),
        "extensions.manage" => {
            on_ui(move || {
                windows::add_tab(w, "chrome://extensions/", true);
            });
            ok()
        }

        // ---- lock management
        "lock.changePin" => {
            let mut c = core();
            let Some(lock) = c.lock.as_ref() else { return err("non configuré") };
            match lock.unlock_with_pin(s(body, "current"), now()) {
                Ok(mk) => {
                    let new_pin = s(body, "new").to_string();
                    let r = c.lock.as_mut().unwrap().change_pin(&mk, &new_pin);
                    r.map(|_| ok()).unwrap_or_else(err)
                }
                Err(e) => unlock_error(e),
            }
        }
        "lock.newRecovery" => {
            let mut c = core();
            let Some(lock) = c.lock.as_ref() else { return err("non configuré") };
            match lock.unlock_with_pin(s(body, "pin"), now()) {
                Ok(mk) => c.lock.as_mut().unwrap().regenerate_recovery(&mk).map(|rk| json!({"ok": true, "recoveryKey": rk.display()})).unwrap_or_else(err),
                Err(e) => unlock_error(e),
            }
        }

        // ---- emergency
        "emergency.lockdown" => {
            on_ui(emergency_lockdown);
            ok()
        }
        "emergency.purge" => emergency_purge(body),
        "autopurge.set" => {
            match serde_json::from_value::<nf_emergency::AutoPurgeRules>(body.get("rules").cloned().unwrap_or(Value::Null)) {
                Ok(mut rules) => {
                    rules.min_independent_critical = rules.min_independent_critical.max(2);
                    let mut c = core();
                    c.settings.auto_purge = rules;
                    c.save_settings();
                    ok()
                }
                Err(e) => err(e),
            }
        }

        // ---- optional sync
        "sync.run" => sync_run(body),

        _ => err(format!("commande inconnue : {cmd}")),
    }
}

// ---------------------------------------------------------------- helpers

fn windows_active_url(w: u32) -> String {
    let (tx, rx) = std::sync::mpsc::channel();
    on_ui(move || {
        let _ = tx.send(windows::active_url(w).unwrap_or_default());
    });
    rx.recv_timeout(std::time::Duration::from_secs(2)).unwrap_or_default()
}

/// URL or search: `example.com` → https://example.com, text → search engine.
pub fn resolve_input(input: &str, settings: &state::Settings) -> String {
    let t = input.trim();
    if t.is_empty() {
        return windows::NEW_TAB_URL.into();
    }
    let lower = t.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("nightfall://app/") || lower.starts_with("chrome://extensions") {
        return t.to_string();
    }
    if lower.starts_with("javascript:") || lower.starts_with("data:") || lower.starts_with("file:") {
        return settings.search_url(t);
    }
    let looks_like_host = !t.contains(' ')
        && (t.contains('.') || lower.starts_with("localhost"))
        && url::Url::parse(&format!("https://{t}")).map(|u| u.host_str().map(|h| h.contains('.') || h == "localhost").unwrap_or(false)).unwrap_or(false);
    if looks_like_host {
        if lower.starts_with("localhost") || lower.starts_with("127.") || lower.starts_with("192.168.") {
            format!("http://{t}")
        } else {
            format!("https://{t}")
        }
    } else {
        settings.search_url(t)
    }
}

fn state_json(w: u32) -> Value {
    let c = core();
    let status = c.lock.as_ref().map(|l| l.status(now()));
    let private = windows_private(w);
    json!({
        "ok": true,
        "configured": c.lock.is_some(),
        "unlocked": c.is_unlocked(),
        "awaitingRecovery": c.awaiting_recovery_confirmation,
        "lockStatus": status,
        "splash": c.prefs.splash_enabled,
        "verticalTabs": c.settings.vertical_tabs,
        "windowId": w,
        "private": private,
        "version": nf_core::VERSION,
        "tagline": nf_core::TAGLINE,
        "signals": c.signals.iter().map(|s| s.describe()).collect::<Vec<_>>(),
        "lastError": c.last_error,
    })
}

fn windows_private(w: u32) -> bool {
    state::browsers().values().any(|b| matches!(b.kind, BrowserKind::Tab { window_id, private: true, .. } if window_id == w))
}

fn setup(body: &Value) -> Value {
    let pin = s(body, "pin");
    if pin != s(body, "confirm") {
        return err("les deux codes ne correspondent pas");
    }
    let mut c = core();
    if c.lock.is_some() {
        return err("NIGHTFALL est déjà configuré");
    }
    let params = if cfg!(debug_assertions) && std::env::var_os("NIGHTFALL_FAST_KDF").is_some() { ArgonParams::TESTING } else { ArgonParams::PRODUCTION };
    match NightfallLock::setup(&c.paths, pin, params) {
        Ok((lock, mk, rk)) => {
            c.lock = Some(lock);
            if let Err(e) = open_stores(&mut *c, mk) {
                return err(e);
            }
            c.awaiting_recovery_confirmation = true;
            json!({ "ok": true, "recoveryKey": rk.display() })
        }
        Err(e) => err(e),
    }
}

fn recovery_confirm() -> Value {
    let mut c = core();
    if !c.awaiting_recovery_confirmation {
        return err("rien à confirmer");
    }
    c.awaiting_recovery_confirmation = false;
    drop(c);
    after_unlock_ui();
    ok()
}

fn unlock_error(e: UnlockError) -> Value {
    match e {
        UnlockError::WrongPin { failures, next_wait_seconds } => json!({ "ok": false, "error": "Code incorrect", "failures": failures, "wait": next_wait_seconds }),
        UnlockError::Throttled(s) => json!({ "ok": false, "error": format!("Patientez {s} s avant un nouvel essai"), "wait": s }),
        UnlockError::RecoveryRequired => json!({ "ok": false, "error": "Trop d'échecs : utilisez votre Recovery Key", "recoveryRequired": true }),
        other => err(other),
    }
}

fn verify_pin(pin: &str) -> Result<(), Value> {
    let c = core();
    let Some(lock) = c.lock.as_ref() else { return Err(err("non configuré")) };
    lock.unlock_with_pin(pin, now()).map(|_| ()).map_err(unlock_error)
}

fn unlock(body: &Value) -> Value {
    let mut c = core();
    if c.is_unlocked() {
        return ok();
    }
    let Some(lock) = c.lock.as_ref() else { return err("non configuré") };
    match lock.unlock_with_pin(s(body, "pin"), now()) {
        Ok(mk) => {
            if let Err(e) = open_stores(&mut *c, mk) {
                return err(e);
            }
            drop(c);
            after_unlock_ui();
            ok()
        }
        Err(e) => {
            let failures = c.lock.as_ref().map(|l| l.status(now()).failures).unwrap_or(0);
            if failures >= 10 {
                let signals = vec![nf_emergency::Signal::PinFailures(failures)];
                evaluate_auto_purge(&mut *c, &signals);
            }
            unlock_error(e)
        }
    }
}

fn unlock_recovery(body: &Value) -> Value {
    let new_pin = s(body, "newPin").to_string();
    if let Err(e) = nf_auth::pin::validate(&new_pin) {
        return err(e);
    }
    let mut c = core();
    let Some(lock) = c.lock.as_ref() else { return err("non configuré") };
    match lock.unlock_with_recovery(s(body, "code")) {
        Ok(mk) => {
            if let Err(e) = c.lock.as_mut().unwrap().change_pin(&mk, &new_pin) {
                return err(e);
            }
            if let Err(e) = open_stores(&mut *c, mk) {
                return err(e);
            }
            drop(c);
            after_unlock_ui();
            ok()
        }
        Err(e) => unlock_error(e),
    }
}

/// Open the encrypted stores with the category keys.
fn open_stores(c: &mut Core, mk: nf_crypto::SecretKey) -> Result<(), String> {
    let paths = c.paths.clone();
    let lock = c.lock.as_mut().ok_or("non configuré")?;
    let mut key_for = |cat: &str| -> Result<nf_crypto::SecretKey, String> {
        match lock.category_key(&mk, cat).map_err(|e| e.to_string())? {
            Some(k) => Ok(k),
            None => lock.recreate_category(&mk, cat).map_err(|e| e.to_string()),
        }
    };
    let db_key = key_for("database")?;
    let vault_key = key_for("vault")?;
    let cookies_key = key_for("cookies")?;
    let storage = match nf_storage::Storage::open(&paths.database(), &db_key) {
        Ok(s) => Some(s),
        Err(e) => {
            c.signals.push(nf_emergency::Signal::DatabaseTampered);
            c.last_error = Some(format!("Base de données : {e}"));
            None
        }
    };
    let vault = match nf_vault::Vault::open(paths.vault(), vault_key) {
        Ok(v) => Some(v),
        Err(e) => {
            c.last_error = Some(format!("Coffre : {e}"));
            None
        }
    };
    if let Some(st) = &storage {
        if let Ok(Some(s)) = st.setting::<state::Settings>("app") {
            c.settings = s;
        }
        if let Ok(Some(v)) = st.setting::<nf_videoguard::VideoGuardConfig>("videoguard") {
            c.videoguard = v;
        }
        if let Ok(Some(sh)) = st.setting::<nf_shield::ShieldConfig>("shield") {
            state::shield().set_config(sh.with_new_default_lists());
        }
    }
    c.unlocked = Some(Unlocked { master: mk, storage, vault, cookies_key: Some(cookies_key) });
    Ok(())
}

fn after_unlock_ui() {
    security::invalidate_detector();
    super::idle::touch();
    let session = {
        let c = core();
        if c.settings.restore_session {
            c.storage().and_then(|s| s.load_session().ok()).unwrap_or_default()
        } else {
            Vec::new()
        }
    };
    on_ui(move || {
        session::restore_cookies();
        // Let the cookie store apply the restored cookies before pages load.
        on_ui_delayed(250, move || windows::after_unlock(session));
    });
    refresh_filter_lists_if_needed();
}

static LISTS_UPDATING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Download missing lists, and refresh lists older than 12 hours, in the
/// background (never twice at the same time).
pub fn refresh_filter_lists_if_needed() {
    use std::sync::atomic::Ordering;
    let sh = state::shield();
    if !sh.needs_update(nf_core::unix_now(), 12 * 3600) || LISTS_UPDATING.swap(true, Ordering::SeqCst) {
        return;
    }
    background(
        move || sh.update_lists(nf_core::unix_now()),
        |errors| {
            LISTS_UPDATING.store(false, Ordering::SeqCst);
            for e in &errors {
                state::log(&format!("Shield : {e}"));
            }
            core().save_settings();
        },
    );
}

/// Lock now: hide everything, seal cookies, then forget the keys.
pub fn lock_now() {
    if !core().is_unlocked() {
        return;
    }
    session::save_session_snapshot();
    core().save_settings();
    windows::on_locked();
    session::seal_cookies_then(|| {
        let mut c = core();
        c.unlocked = None; // keys are zeroized on drop
    });
}

pub fn emergency_lockdown() {
    windows::close_private_windows();
    lock_now();
    windows::push_event_all(&json!({ "type": "lockdown" }));
}

fn evaluate_auto_purge(c: &mut Core, extra: &[nf_emergency::Signal]) {
    let mut signals = c.signals.clone();
    signals.extend_from_slice(extra);
    match c.settings.auto_purge.evaluate(&signals) {
        nf_emergency::Decision::Nothing => {}
        nf_emergency::Decision::Lockdown { .. } => on_ui(emergency_lockdown),
        nf_emergency::Decision::LockdownAndPurge { categories, .. } => {
            on_ui(emergency_lockdown);
            if let Some(lock) = c.lock.as_mut() {
                for cat in &categories {
                    for k in cat.key_categories() {
                        let _ = lock.destroy_category(k);
                    }
                }
            }
            if let Ok(plan) = PurgePlan::new(&c.paths, &categories) {
                let _ = plan.schedule(&c.paths);
            }
        }
    }
}

fn emergency_purge(body: &Value) -> Value {
    if let Err(e) = verify_pin(s(body, "pin")) {
        return e;
    }
    if s(body, "confirm") != "PURGER" {
        return err("tapez PURGER pour confirmer");
    }
    let cats: Vec<PurgeCategory> = serde_json::from_value(body.get("categories").cloned().unwrap_or(Value::Null)).unwrap_or_default();
    if cats.is_empty() {
        return err("aucune catégorie choisie");
    }
    let mut c = core();
    let paths = c.paths.clone();
    // 1. Cryptographic erase: destroy the category keys first.
    if cats.contains(&PurgeCategory::Everything) {
        if let Some(lock) = c.lock.take() {
            let _ = lock.destroy_all();
        }
    } else if let Some(lock) = c.lock.as_mut() {
        for cat in &cats {
            for k in cat.key_categories() {
                let _ = lock.destroy_category(k);
            }
        }
    }
    // 2. Close the stores that are about to be deleted.
    if let Some(u) = c.unlocked.as_mut() {
        if cats.iter().any(|x| matches!(x, PurgeCategory::BrowsingData | PurgeCategory::Everything)) {
            u.storage = None;
        }
        if cats.iter().any(|x| matches!(x, PurgeCategory::Passwords | PurgeCategory::Everything)) {
            u.vault = None;
        }
        if cats.iter().any(|x| matches!(x, PurgeCategory::Sessions | PurgeCategory::Everything)) {
            u.cookies_key = None;
        }
    }
    let plan = match PurgePlan::new(&paths, &cats) {
        Ok(p) => p,
        Err(e) => return err(e),
    };
    // 3. Sessions: delete live cookies now.
    if cats.iter().any(|x| matches!(x, PurgeCategory::Sessions | PurgeCategory::Everything)) {
        on_ui(|| {
            if let Some(m) = cef::cookie_manager_get_global_manager(None) {
                use cef::ImplCookieManager;
                m.delete_cookies(None, None, None);
            }
        });
    }
    let restart = plan.needs_engine_stopped();
    if restart {
        // Chromium keeps its files open: finish at next start, before CEF.
        if let Err(e) = plan.schedule(&paths) {
            return err(e);
        }
        c.unlocked = None;
        drop(c);
        on_ui(|| {
            windows::on_locked();
            for id in windows::window_ids() {
                windows::close_window(id);
            }
        });
        json!({ "ok": true, "restart": true })
    } else {
        let report = plan.execute();
        // Recreate empty stores with fresh keys.
        let cc: &mut Core = &mut c;
        if let (Some(lock), Some(u)) = (cc.lock.as_mut(), cc.unlocked.as_mut()) {
            let mk = u.master.clone();
            for cat in &cats {
                for k in cat.key_categories() {
                    let _ = lock.recreate_category(&mk, k);
                }
            }
            if u.storage.is_none() {
                if let Ok(Some(k)) = lock.category_key(&mk, "database") {
                    u.storage = nf_storage::Storage::open(&paths.database(), &k).ok();
                }
            }
            if u.vault.is_none() {
                if let Ok(Some(k)) = lock.category_key(&mk, "vault") {
                    u.vault = nf_vault::Vault::open(paths.vault(), k).ok();
                }
            }
        }
        json!({ "ok": true, "restart": false, "report": report })
    }
}

fn settings_set(body: &Value) -> Value {
    let mut c = core();
    let old_vertical = c.settings.vertical_tabs;
    if let Some(v) = body.get("settings") {
        match serde_json::from_value::<state::Settings>(v.clone()) {
            Ok(mut s) => {
                // Auto-purge is changed only through autopurge.set (min 2 signals).
                s.auto_purge = c.settings.auto_purge.clone();
                c.settings = s;
            }
            Err(e) => return err(e),
        }
    }
    if let Some(p) = body.get("prefs") {
        if let Ok(p) = serde_json::from_value::<state::UiPrefs>(p.clone()) {
            c.prefs = p;
            c.save_prefs();
        }
    }
    c.save_settings();
    let vertical = c.settings.vertical_tabs;
    drop(c);
    if vertical != old_vertical {
        on_ui(|| {
            windows::apply_mode_all(false);
            windows::push_event_all(&json!({ "type": "layout" }));
        });
    }
    ok()
}

fn shield_set(body: &Value) -> Value {
    match serde_json::from_value::<nf_shield::ShieldConfig>(body.get("config").cloned().unwrap_or(Value::Null)) {
        Ok(cfg) => {
            state::shield().set_config(cfg);
            core().save_settings();
            ok()
        }
        Err(e) => err(e),
    }
}

fn page_info(w: u32) -> Value {
    let url = windows_active_url(w);
    let c = core();
    let bookmarked = c.storage().and_then(|s| s.bookmark_for_url(&url).ok().flatten()).is_some();
    let passwords = c.vault().map(|v| v.for_page(&url).len()).unwrap_or(0);
    let shield = state::shield();
    json!({
        "ok": true,
        "url": url,
        "secure": url.starts_with("https://") || url.starts_with("nightfall://"),
        "internal": url.starts_with("nightfall://"),
        "shieldActive": shield.is_active_for(&url),
        "bookmarked": bookmarked,
        "passwords": passwords,
        "site": nf_shield::site_of(&url),
    })
}

fn vault_fill(w: u32, id: &str) -> Value {
    let url = windows_active_url(w);
    let c = core();
    let Some(cred) = c.vault().and_then(|v| v.get(id)) else { return err("entrée introuvable") };
    // Origin check right before filling: never fill on another site.
    match nf_vault::normalize_origin(&url) {
        Ok(o) if o == cred.origin => {}
        _ => return err("cet identifiant n'appartient pas à ce site"),
    }
    let user = serde_json::to_string(&cred.username).unwrap_or_default();
    let pass = serde_json::to_string(&cred.password).unwrap_or_default();
    let origin = serde_json::to_string(&cred.origin).unwrap_or_default();
    drop(c);
    let js = format!(
        "(function(){{if(location.origin!=={origin})return;\
var p=[...document.querySelectorAll('input[type=password]')].find(function(e){{return e.offsetParent!==null}});\
if(!p)return;var f=p.form||document;\
var u=[...f.querySelectorAll('input[type=email],input[type=text],input[autocomplete=username],input:not([type])')].filter(function(e){{return e.offsetParent!==null&&e!==p}});\
function set(el,v){{var d=Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value');d.set.call(el,v);el.dispatchEvent(new Event('input',{{bubbles:true}}));el.dispatchEvent(new Event('change',{{bubbles:true}}));}}\
if(u.length)set(u[u.length-1],{user});set(p,{pass});}})();"
    );
    on_ui(move || {
        if let Some(tab) = windows::active_tab_id(w) {
            windows::exec_in_tab(w, tab, &js);
        }
    });
    ok()
}

fn extensions_list() -> Value {
    let dir = core().paths.chromium_profile().join("Default").join("Extensions");
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for e in entries.flatten() {
            // Extensions/<id>/<version>/manifest.json
            if let Ok(versions) = std::fs::read_dir(e.path()) {
                for v in versions.flatten() {
                    let manifest = v.path().join("manifest.json");
                    if let Ok(text) = std::fs::read_to_string(&manifest) {
                        if let Ok(a) = nf_security::extensions::assess_manifest(&text) {
                            out.push(json!({ "id": e.file_name().to_string_lossy(), "assessment": a }));
                        }
                    }
                }
            }
        }
    }
    json!({ "ok": true, "extensions": out })
}

fn sync_run(body: &Value) -> Value {
    let folder = s(body, "folder").to_string();
    let passphrase = s(body, "passphrase").to_string();
    if folder.is_empty() {
        return err("choisissez un dossier de synchronisation");
    }
    let backend = match nf_sync::FolderSync::new(std::path::Path::new(&folder), &passphrase) {
        Ok(b) => b,
        Err(e) => return err(e),
    };
    let local: Vec<nf_sync::SyncedBookmark> = {
        let c = core();
        let st = c.storage().unwrap();
        st.bookmarks()
            .unwrap_or_default()
            .into_iter()
            .map(|b| nf_sync::SyncedBookmark { url: b.url, title: b.title, folder: vec![], modified: b.modified.max(b.created), deleted: false })
            .collect()
    };
    match nf_sync::synchronize(&backend, &local) {
        Ok(merged) => {
            let mut c = core();
            let st = c.storage().unwrap();
            let mut added = 0;
            for b in merged.iter().filter(|b| !b.deleted) {
                if st.bookmark_for_url(&b.url).ok().flatten().is_none() && st.add_bookmark(None, &b.title, &b.url, b.modified).is_ok() {
                    added += 1;
                }
            }
            for b in merged.iter().filter(|b| b.deleted) {
                if let Ok(Some(existing)) = st.bookmark_for_url(&b.url) {
                    if existing.modified <= b.modified {
                        let _ = st.delete_bookmark(existing.id);
                    }
                }
            }
            c.settings.sync_folder = Some(folder);
            c.save_settings();
            json!({ "ok": true, "added": added, "total": merged.len() })
        }
        Err(e) => err(e),
    }
}

/// Keyboard shortcuts and toolbar actions (UI thread).
pub fn shortcut(w: u32, action: &str) {
    let browser = windows::active_browser(w);
    match action {
        "tab.new" => {
            windows::add_tab(w, windows::NEW_TAB_URL, true);
        }
        "tab.close" => {
            if let Some(t) = windows::active_tab_id(w) {
                windows::close_tab(w, t);
            }
        }
        "tab.next" | "tab.prev" => {
            let ids: Vec<u32> = windows::with_window(w, |c| c.tabs.iter().map(|t| t.id).collect()).unwrap_or_default();
            if let (Some(cur), false) = (windows::active_tab_id(w), ids.is_empty()) {
                let i = ids.iter().position(|x| *x == cur).unwrap_or(0);
                let n = if action == "tab.next" { (i + 1) % ids.len() } else { (i + ids.len() - 1) % ids.len() };
                windows::activate_tab(w, ids[n]);
            }
        }
        "nav.back" | "tab.back" => {
            if let Some(b) = browser {
                b.go_back();
            }
        }
        "nav.forward" | "tab.forward" => {
            if let Some(b) = browser {
                b.go_forward();
            }
        }
        "nav.reload" | "tab.reload" => {
            if let Some(b) = browser {
                b.reload();
            }
        }
        "nav.stop" => {
            if let Some(b) = browser {
                b.stop_load();
            }
        }
        "nav.home" => windows::navigate_active(w, windows::NEW_TAB_URL),
        "lock" => lock_now(),
        "emergency.lockdown" => emergency_lockdown(),
        "window.new" => windows::create_window(false, Vec::new()),
        "window.private" => windows::create_window(true, Vec::new()),
        "focus.address" => windows::push_event(w, &json!({ "type": "focus_address" })),
        "page.history" => {
            windows::add_tab(w, "nightfall://app/history.html", true);
        }
        "page.downloads" => {
            windows::add_tab(w, "nightfall://app/downloads.html", true);
        }
        "devtools" => windows::show_devtools(w),
        "bookmark.toggle" => {
            let url = windows::active_url(w).unwrap_or_default();
            if !(url.starts_with("http://") || url.starts_with("https://")) {
                return;
            }
            let title = windows::with_window(w, |c| c.active.and_then(|id| c.tabs.iter().find(|t| t.id == id).map(|t| t.title.clone()))).flatten().unwrap_or_default();
            let c = core();
            if let Some(st) = c.storage() {
                match st.bookmark_for_url(&url) {
                    Ok(Some(b)) => {
                        let _ = st.delete_bookmark(b.id);
                    }
                    _ => {
                        let _ = st.add_bookmark(None, &title, &url, now());
                    }
                }
            }
            drop(c);
            security::invalidate_detector();
            windows::push_event(w, &json!({ "type": "page_info" }));
        }
        "tab.reopen" => {}
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_resolution() {
        let s = state::Settings::default();
        assert_eq!(resolve_input("example.com", &s), "https://example.com");
        assert_eq!(resolve_input("https://a.test/x", &s), "https://a.test/x");
        assert_eq!(resolve_input("localhost:8080", &s), "http://localhost:8080");
        assert!(resolve_input("rust borrow checker", &s).starts_with("https://duckduckgo.com/?q=rust+borrow+checker"));
        assert!(resolve_input("javascript:alert(1)", &s).starts_with("https://duckduckgo.com/"));
        assert_eq!(resolve_input("", &s), windows::NEW_TAB_URL);
    }
}

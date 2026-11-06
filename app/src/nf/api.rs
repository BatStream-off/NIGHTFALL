//! Internal API used by NIGHTFALL's own pages (`nightfall://app/api/<cmd>`).
//!
//! Called on CEF's IO thread by the scheme handler, only for requests whose
//! frame is a NIGHTFALL page (see scheme.rs). Commands that touch views are
//! forwarded to the UI thread.

use super::state::{self, core, BrowserKind, Core, Unlocked};
use super::ui_thread::{background, on_ui, on_ui_delayed};
use super::{downloads, security, session, windows};
use cef::{ImplBrowser, ImplBrowserHost};
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
const PRE_UNLOCK: &[&str] = &["state", "setup", "unlock", "unlock.recovery", "recovery.confirm", "ui.activity", "ui.expand", "popup.close", "win.minimize", "win.maximize", "win.close"];

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
        // Window buttons of the integrated title bar (prototype M-0005):
        // only the window's own top interface view may use them, also on
        // the lock screen (the frameless window has no other buttons).
        "win.minimize" | "win.maximize" | "win.close" => {
            if !matches!(caller.kind, BrowserKind::Ui { side: false, .. }) {
                return err("interdit");
            }
            let cmd = cmd.to_string();
            on_ui(move || super::titlebar::command(w, &cmd));
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
        "tab.reopen" | "tab.duplicate" | "tab.reload" | "tab.mute" | "tab.closeOthers" | "tab.closeRight" | "tab.newRight" | "tab.unresponsive" => {
            let id = body.get("id").and_then(Value::as_u64).unwrap_or(0) as u32;
            let terminate = s(body, "action") == "terminate";
            let cmd = cmd.to_string();
            on_ui(move || tab_command(w, id, &cmd, terminate));
            ok()
        }
        "find.run" => {
            let text = s(body, "text").to_string();
            let flag = |k: &str, d: bool| body.get(k).and_then(Value::as_bool).unwrap_or(d);
            let (forward, match_case, next) = (flag("forward", true), flag("matchCase", false), flag("next", false));
            on_ui(move || super::browsing::find(w, &text, forward, match_case, next));
            ok()
        }
        "find.stop" => {
            let clear = body.get("clear").and_then(Value::as_bool).unwrap_or(true);
            on_ui(move || super::browsing::stop_find(w, clear));
            ok()
        }
        "find.open" => {
            on_ui(move || super::browsing::open_find_bar(w));
            ok()
        }
        "find.close" => {
            on_ui(move || {
                windows::popup_close_if(w, Some("find"), None);
                windows::focus_active_tab(w);
            });
            ok()
        }
        "zoom.in" | "zoom.out" | "zoom.reset" | "page.print" | "page.save" | "page.viewSource" | "window.fullscreen" => {
            let action = match cmd {
                "zoom.in" => "zoom.in",
                "zoom.out" => "zoom.out",
                "zoom.reset" => "zoom.reset",
                "page.print" => "print",
                "page.save" => "save",
                "page.viewSource" => "view.source",
                _ => "window.fullscreen",
            };
            on_ui(move || shortcut(w, action));
            ok()
        }
        "omnibox.suggest" => omnibox_suggest(w, s(body, "text")),
        "omnibox.render" => {
            // The top view shows its suggestions in the dropdown view.
            let ev = json!({ "type": "suggest", "items": body.get("items").cloned().unwrap_or(json!([])), "selected": body.get("selected").cloned().unwrap_or(json!(-1)) });
            if let Ok(mut m) = OMNIBOX.lock() {
                m.insert(w, ev.clone());
            }
            on_ui(move || windows::push_event(w, &ev));
            ok()
        }
        "omnibox.state" => {
            let ev = OMNIBOX.lock().ok().and_then(|m| m.get(&w).cloned()).unwrap_or(json!({ "items": [], "selected": -1 }));
            json!({ "ok": true, "state": ev })
        }
        "omnibox.pick" => {
            let url = resolve_input(s(body, "url"), &core().settings);
            let target = url.clone();
            on_ui(move || {
                windows::popup_close(w);
                windows::navigate_active(w, &target);
                windows::focus_active_tab(w);
                windows::push_event(w, &json!({ "type": "omnibox_picked", "url": target }));
            });
            json!({ "ok": true, "url": url })
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
            let mut page = s(body, "page").to_string();
            let allowed = ["settings", "security", "history", "bookmarks", "downloads", "passwords", "extensions", "about", "newtab"];
            if !allowed.contains(&page.as_str()) {
                return err("page inconnue");
            }
            // Optional section of the page (settings.html#confidentialite).
            let mut hash: String = s(body, "hash").chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').take(40).collect();
            // The Security Center is a section of the settings.
            if page == "security" {
                page = "settings".into();
                if hash.is_empty() {
                    hash = "security".into();
                }
            }
            let url = if hash.is_empty() { format!("nightfall://app/{page}.html") } else { format!("nightfall://app/{page}.html#{hash}") };
            on_ui(move || {
                windows::add_tab(w, &url, true);
            });
            ok()
        }
        "page.info" => page_info(w),
        "update.status" => super::updater::status(),
        "update.set" => super::updater::set(body),
        "update.check" => {
            super::updater::check(false);
            super::updater::status()
        }
        "update.install" => super::updater::install_now(),
        "popup.open" => {
            let panel = s(body, "panel").to_string();
            let num = |v: &Value, k: &str| v.get(k).and_then(Value::as_f64).unwrap_or(0.0).round() as i32;
            let a = body.get("anchor").cloned().unwrap_or(Value::Null);
            let anchor = cef::Rect { x: num(&a, "x"), y: num(&a, "y"), width: num(&a, "width"), height: num(&a, "height") };
            let (width, height) = (num(body, "width"), num(body, "height"));
            let flag = |k: &str, d: bool| body.get(k).and_then(Value::as_bool).unwrap_or(d);
            let opts = windows::PopupOpts {
                activate: !flag("noActivate", false),
                toggle: flag("toggle", true),
                align_left: flag("alignLeft", false),
                from_side: matches!(caller.kind, BrowserKind::Ui { side: true, .. }),
                max_width: if panel == "suggest" { 2400 } else { 520 },
            };
            on_ui(move || windows::popup_open_ex(w, &panel, anchor, width, height, opts));
            ok()
        }
        "popup.resize" => {
            let num = |k: &str| body.get(k).and_then(Value::as_f64).unwrap_or(0.0).round() as i32;
            let (width, height) = (num("width"), num("height"));
            on_ui(move || windows::popup_resize(w, width, height));
            ok()
        }
        "popup.close" => {
            // A popup names itself (panel and instance number): a late
            // request never closes the popup opened after it.
            let panel = body.get("panel").and_then(Value::as_str).map(str::to_string);
            let serial = body.get("n").and_then(Value::as_u64).map(|n| n as u32);
            on_ui(move || {
                windows::popup_close_if(w, panel.as_deref(), serial);
            });
            ok()
        }
        "permission.answer" => {
            let id: u64 = s(body, "id").parse().unwrap_or(0);
            let allow = body.get("allow").and_then(Value::as_bool).unwrap_or(false);
            let remember = body.get("remember").and_then(Value::as_bool).unwrap_or(false);
            on_ui(move || super::permissions::answer(id, allow, remember));
            ok()
        }
        "permissions.forget" => {
            // One decision ({origin, kind}) or all of them ({all: true}).
            let all = body.get("all").and_then(Value::as_bool).unwrap_or(false);
            let (origin, kind) = (s(body, "origin").to_string(), s(body, "kind").to_string());
            let removed: Vec<state::SitePermission> = {
                let mut c = core();
                let (gone, kept): (Vec<_>, Vec<_>) =
                    std::mem::take(&mut c.settings.site_permissions).into_iter().partition(|p| all || (p.origin == origin && p.kind == kind));
                c.settings.site_permissions = kept;
                c.save_settings();
                gone
            };
            let n = removed.len();
            on_ui(move || {
                for p in removed {
                    super::permissions::reset_chromium(&p.origin, &p.kind);
                }
            });
            json!({ "ok": true, "removed": n })
        }
        "devtools" => {
            on_ui(move || windows::show_devtools(w));
            ok()
        }

        // ---- settings
        "settings.get" => settings_get(),
        "security.events" => {
            let limit = body.get("limit").and_then(|v| v.as_i64()).unwrap_or(200);
            match core().storage() {
                Some(st) => st.security_events(limit, now()).map(|e| json!({ "ok": true, "events": e })).unwrap_or_else(err),
                None => err("NIGHTFALL est verrouillé"),
            }
        }
        "security.events.clear" => match core().storage() {
            Some(st) => st.clear_security_events().map(|n| json!({ "ok": true, "removed": n })).unwrap_or_else(err),
            None => err("NIGHTFALL est verrouillé"),
        },
        "zoom.list" => {
            let sites: Vec<Value> = super::browsing::site_zoom_list().into_iter().map(|(host, percent)| json!({"host": host, "percent": percent})).collect();
            json!({"ok": true, "sites": sites})
        }
        "zoom.forget" => {
            let host = s(body, "host");
            super::browsing::forget_site_zoom(if host.is_empty() { None } else { Some(host) });
            json!({"ok": true})
        }
        "settings.set" => settings_set(body),
        "theme.get" => theme_get(),
        "theme.set" => theme_set(body),
        "theme.save" => theme_save(body),
        "theme.delete" => theme_delete(body),
        "theme.preview" => {
            let t: super::theme::Theme = serde_json::from_value(body.get("theme").cloned().unwrap_or(Value::Null)).unwrap_or_default();
            match super::theme::validate(&t) {
                Ok(()) => json!({ "ok": true, "css": super::theme::css(&t) }),
                Err(e) => err(e),
            }
        }
        "settings.reset" => settings_reset(),
        "browsing.clear" => super::browsing_data::clear(body),
        "downloads.chooseFolder" => {
            // Native folder picker, run from the settings tab; the page is
            // told when the dialog closes (nfFolderChosen).
            let BrowserKind::Tab { window_id, tab_id, .. } = caller.kind else { return err("interdit") };
            let current = super::downloads::download_dir(core().settings.download_dir.as_deref());
            on_ui(move || {
                let Some(b) = windows::tab_browser(window_id, tab_id) else { return };
                super::downloads::choose_folder(&b, &current, move |chosen| {
                    if let Some(dir) = &chosen {
                        let mut c = core();
                        c.settings.download_dir = Some(dir.clone());
                        c.save_settings();
                    }
                    windows::exec_in_tab(window_id, tab_id, &format!("window.nfFolderChosen && window.nfFolderChosen({})", chosen.is_some()));
                });
            });
            ok()
        }
        "downloads.resetFolder" => {
            let mut c = core();
            c.settings.download_dir = None;
            c.save_settings();
            ok()
        }
        "data.import" | "data.export" => data_transfer(caller, cmd, body),
        "system.proxySettings" => open_proxy_settings(),
        "system.defaultBrowser" => open_default_browser_settings(),
        "https.proceed" => {
            // "Continuer vers le site" on the HTTPS-First warning page.
            let BrowserKind::Tab { window_id, tab_id, .. } = caller.kind else { return err("interdit") };
            let url = s(body, "url").to_string();
            if !url.starts_with("http://") || url::Url::parse(&url).is_err() {
                return err("adresse invalide");
            }
            super::https_first::allow(&url);
            on_ui(move || {
                if let Some(f) = windows::tab_browser(window_id, tab_id).and_then(|b| b.main_frame()) {
                    use cef::ImplFrame;
                    f.load_url(Some(&cef::CefString::from(url.as_str())));
                }
            });
            ok()
        }

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
            // Estimates, as other blockers do: ~40 KB and ~50 ms per blocked item.
            let blocked = state::blocked_total();
            let stats = json!({ "blocked": blocked, "bytesSaved": blocked * 40 * 1024, "secondsSaved": blocked / 20, "estimated": true });
            json!({ "ok": true, "shortcuts": c.settings.shortcuts, "wallpaper": c.settings.wallpaper, "topSites": top, "engine": c.settings.search_engine, "stats": stats })
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
    // Cached by the UI thread: no wait on CEF's IO thread.
    if let Some(u) = windows::cached_active_url(w) {
        return u;
    }
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
    if lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("nightfall://app/") || lower.starts_with("chrome://extensions") || lower.starts_with("view-source:http://") || lower.starts_with("view-source:https://") {
        return t.to_string();
    }
    if lower.starts_with("javascript:") || lower.starts_with("data:") || lower.starts_with("file:") {
        return settings.search_url(t);
    }
    if let Some((_, url)) = settings.keyword_search(t) {
        return url;
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
        "showHome": c.settings.show_home_button,
        "frameless": super::titlebar::FRAMELESS,
        "platform": super::titlebar::platform(),
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

/// Import or export of bookmarks (HTML) or passwords (CSV), from the
/// settings tab: a native file dialog, then the page is told the result
/// (nfTransferDone). Exporting passwords asks for the code first.
fn data_transfer(caller: &Caller, cmd: &str, body: &Value) -> Value {
    let BrowserKind::Tab { window_id, tab_id, .. } = caller.kind else { return err("interdit") };
    let what = s(body, "what").to_string();
    if !matches!(what.as_str(), "bookmarks" | "passwords") {
        return err("type inconnu");
    }
    let export = cmd == "data.export";
    {
        let c = core();
        if !c.is_unlocked() {
            return err("NIGHTFALL est verrouillé");
        }
        if export && what == "passwords" {
            let Some(lock) = c.lock.as_ref() else { return err("non configuré") };
            if let Err(e) = lock.unlock_with_pin(s(body, "pin"), now()) {
                return unlock_error(e);
            }
        }
    }
    let dir = super::downloads::download_dir(core().settings.download_dir.as_deref());
    let (title, name, accept): (&'static str, &str, Vec<&'static str>) = match (export, what.as_str()) {
        (false, "bookmarks") => ("Importer des favoris (fichier HTML)", "", vec![".html", ".htm"]),
        (false, _) => ("Importer des mots de passe (fichier CSV)", "", vec![".csv"]),
        (true, "bookmarks") => ("Exporter les favoris", "favoris-nightfall.html", vec![".html"]),
        (true, _) => ("Exporter les mots de passe", "mots-de-passe-nightfall.csv", vec![".csv"]),
    };
    let default = if name.is_empty() { dir.display().to_string() } else { dir.join(name).display().to_string() };
    on_ui(move || {
        let Some(b) = windows::tab_browser(window_id, tab_id) else { return };
        super::downloads::choose_file(&b, export, title, &default, &accept, move |path| {
            let result = match path {
                None => json!({ "ok": false, "cancelled": true }),
                Some(p) => run_transfer(export, &what, std::path::Path::new(&p)),
            };
            let ev = json!({ "what": what, "export": export, "result": result });
            windows::exec_in_tab(window_id, tab_id, &format!("window.nfTransferDone && window.nfTransferDone({ev})"));
        });
    });
    ok()
}

fn read_limited(p: &std::path::Path, max: u64) -> Result<Vec<u8>, String> {
    let meta = std::fs::metadata(p).map_err(|e| format!("fichier illisible : {e}"))?;
    if meta.len() > max {
        return Err(format!("fichier trop grand (plus de {} Mo)", max / (1024 * 1024)));
    }
    std::fs::read(p).map_err(|e| format!("fichier illisible : {e}"))
}

fn run_transfer(export: bool, what: &str, p: &std::path::Path) -> Value {
    use zeroize::Zeroize;
    let mut c = core();
    match (export, what) {
        (false, "bookmarks") => {
            let bytes = match read_limited(p, 20 * 1024 * 1024) {
                Ok(b) => b,
                Err(e) => return err(e),
            };
            let nodes = nf_storage::bookmark_io::parse_netscape(&String::from_utf8_lossy(&bytes));
            if nodes.is_empty() {
                return err("aucun favori trouvé dans ce fichier");
            }
            let Some(st) = c.storage() else { return err("NIGHTFALL est verrouillé") };
            st.import_bookmarks(&nodes, "Favoris importés", now()).map(|s| json!({ "ok": true, "stats": s })).unwrap_or_else(err)
        }
        (false, _) => {
            let mut bytes = match read_limited(p, 10 * 1024 * 1024) {
                Ok(b) => b,
                Err(e) => return err(e),
            };
            let parsed = match std::str::from_utf8(&bytes) {
                Ok(text) => nf_vault::csv_io::parse_csv(text).map_err(|e| e.to_string()),
                Err(_) => Err("le fichier n'est pas en UTF-8".into()),
            };
            bytes.zeroize();
            let (entries, skipped) = match parsed {
                Ok(v) => v,
                Err(e) => return err(e),
            };
            let Some(v) = c.vault_mut() else { return err("NIGHTFALL est verrouillé") };
            v.import_entries(&entries, now())
                .map(|s| json!({ "ok": true, "stats": { "added": s.added, "duplicates": s.duplicates, "skipped": skipped } }))
                .unwrap_or_else(err)
        }
        (true, "bookmarks") => {
            let Some(st) = c.storage() else { return err("NIGHTFALL est verrouillé") };
            let (folders, bookmarks) = match (st.folders(), st.bookmarks()) {
                (Ok(f), Ok(b)) => (f, b),
                _ => return err("lecture des favoris impossible"),
            };
            let n = bookmarks.len();
            std::fs::write(p, nf_storage::bookmark_io::export_netscape(&folders, &bookmarks))
                .map(|_| json!({ "ok": true, "count": n }))
                .unwrap_or_else(err)
        }
        (true, _) => {
            let Some(v) = c.vault() else { return err("NIGHTFALL est verrouillé") };
            let n = v.len();
            let mut csv = v.export_csv();
            let r = nf_core::paths::write_private(p, csv.as_bytes());
            csv.zeroize();
            r.map(|_| json!({ "ok": true, "count": n })).unwrap_or_else(err)
        }
    }
}

fn unlock_error(e: UnlockError) -> Value {
    match e {
        UnlockError::WrongPin { failures, next_wait_seconds } => {
            security::record_event("wrong_code", "", &format!("Code incorrect ({failures} échec(s) de suite)"), false);
            json!({ "ok": false, "error": "Code incorrect", "failures": failures, "wait": next_wait_seconds })
        }
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
        // A copy that cannot be read is logged and kept aside (never
        // silently replaced by the defaults at the next save).
        if let Some(s) = state::load_setting::<state::Settings>(st, "app") {
            c.settings = s;
        }
        if let Some(v) = state::load_setting::<nf_videoguard::VideoGuardConfig>(st, "videoguard") {
            c.videoguard = v;
        }
        if let Some(sh) = state::load_setting::<nf_shield::ShieldConfig>(st, "shield") {
            state::shield().set_config_later(sh.with_new_default_lists());
        }
        rebuild_shield_in_background();
        if let Ok(Some(total)) = st.setting::<u64>("shield_total") {
            state::set_blocked_saved_total(total);
        }
    }
    c.unlocked = Some(Unlocked { master: mk, storage, vault, cookies_key: Some(cookies_key) });
    Ok(())
}

fn after_unlock_ui() {
    super::updater::load_prefs();
    security::invalidate_detector();
    std::thread::spawn(security::flush_pending_events);
    super::idle::touch();
    let session = {
        let c = core();
        match c.settings.startup() {
            state::Startup::Restore => c.storage().and_then(|s| s.load_session().ok()).unwrap_or_default(),
            state::Startup::Pages => c.settings.startup_tabs(),
            state::Startup::NewTab => Vec::new(),
        }
    };
    on_ui(move || {
        // The user's settings are now known (encrypted database).
        super::prefs::apply_all();
        super::permissions::reset_temporary();
        // Remembered decisions are given back to Chromium until the next lock.
        super::permissions::reapply_remembered();
        session::restore_cookies();
        // Let the cookie store apply the restored cookies before pages load.
        on_ui_delayed(250, move || {
            windows::after_unlock(session);
            // Sound back (media stay paused), VideoGuard resumes.
            super::lock_guard::on_unlock();
        });
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
    if !core().is_unlocked() || super::lock_guard::is_locking() {
        return;
    }
    // From here nothing acts behind the lock screen (shortcuts, sound and
    // media, VideoGuard), even before the keys are forgotten below.
    super::lock_guard::begin_locking();
    super::lock_guard::on_lock();
    // Permission answers given without "Mémoriser" end here.
    super::permissions::reset_temporary();
    // Remembered decisions: Chromium's copy (origins in plain text in its
    // profile) is removed until the next unlock.
    super::permissions::forget_remembered_in_chromium();
    session::save_session_snapshot();
    core().save_settings();
    windows::on_locked();
    session::seal_cookies_then(|| {
        let mut c = core();
        c.unlocked = None; // keys are zeroized on drop
        drop(c);
        super::lock_guard::end_locking();
    });
}

pub fn emergency_lockdown() {
    security::record_event("lockdown", "", "Verrouillage d'urgence déclenché", false);
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

fn settings_get() -> Value {
    let c = core();
    let engines: Vec<Value> = state::SEARCH_ENGINES.iter().map(|(id, name, url)| json!({ "id": id, "name": name, "url": url })).collect();
    let kinds: Vec<Value> = super::permissions::KINDS.iter().map(|(id, label, _, _)| json!({ "id": id, "label": label })).collect();
    let platform = if cfg!(target_os = "windows") { "windows" } else if cfg!(target_os = "macos") { "macos" } else { "linux" };
    json!({
        "ok": true,
        "settings": c.settings,
        "prefs": c.prefs,
        "engines": engines,
        "engineKeywords": state::ENGINE_KEYWORDS.iter().map(|(k, n, _)| json!({"keyword": k, "name": n})).collect::<Vec<_>>(),
        "suggestionEngines": state::SEARCH_ENGINES.iter().filter(|(id, _, _)| super::omnibox::suggest_endpoint(id).is_some()).map(|(id, _, _)| *id).collect::<Vec<_>>(),
        "permissionKinds": kinds,
        "platform": platform,
        "version": nf_core::VERSION,
        "defaultDownloadDir": nf_downloads::default_download_dir().display().to_string(),
        "downloadDir": super::downloads::download_dir(c.settings.download_dir.as_deref()).display().to_string(),
        "spellcheckAvailable": super::prefs::SPELLCHECK_AVAILABLE,
        "hardwareAccelerationAtStart": super::prefs::HW_ACCEL_AT_START.load(std::sync::atomic::Ordering::Relaxed),
        "autoplayBlockAtStart": super::prefs::AUTOPLAY_BLOCK_AT_START.load(std::sync::atomic::Ordering::Relaxed),
        "chromium": super::prefs::last_report(),
    })
}

/// After a settings change: fast flags now, Chromium preferences on the UI
/// thread (the report of what Chromium accepted is returned when ready).
fn apply_settings_change(old: &state::Settings) -> Value {
    let new = core().settings.clone();
    super::prefs::update_flags(&new);
    let (tx, rx) = std::sync::mpsc::channel();
    let layout = old.vertical_tabs != new.vertical_tabs || old.show_home_button != new.show_home_button;
    on_ui(move || {
        let report = super::prefs::apply_all();
        let _ = tx.send(report);
        if layout {
            windows::apply_mode_all(false);
            windows::push_event_all(&json!({ "type": "layout" }));
        }
    });
    let report = rx.recv_timeout(std::time::Duration::from_secs(3)).unwrap_or_default();
    json!({ "ok": true, "chromium": report })
}

fn settings_set(body: &Value) -> Value {
    let mut c = core();
    let old = c.settings.clone();
    if let Some(v) = body.get("settings") {
        match serde_json::from_value::<state::Settings>(v.clone()) {
            Ok(mut s) => {
                // Auto-purge is changed only through autopurge.set (min 2
                // signals); site decisions and the download folder through
                // their own commands (a page with an older copy must not
                // undo them).
                s.auto_purge = old.auto_purge.clone();
                s.site_permissions = old.site_permissions.clone();
                s.permission_resets = old.permission_resets.clone();
                s.download_dir = old.download_dir.clone();
                // Themes change only through the theme.* commands.
                s.theme = old.theme.clone();
                s.sanitize();
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
    drop(c);
    apply_settings_change(&old)
}

/// Defaults everywhere; bookmarks, history, passwords, shortcuts, Shield,
/// VideoGuard and the emergency rules are kept.
fn settings_reset() -> Value {
    let mut c = core();
    let old = c.settings.clone();
    c.settings = old.reset_keeping_data();
    c.prefs = state::UiPrefs::default();
    c.save_prefs();
    c.save_settings();
    drop(c);
    let forget: Vec<state::SitePermission> = old.site_permissions.iter().chain(old.permission_resets.iter()).cloned().collect();
    on_ui(move || {
        for p in forget {
            super::permissions::reset_chromium(&p.origin, &p.kind);
        }
    });
    apply_settings_change(&old)
}

/// Open the operating system's proxy settings (Chromium uses them).
/// The system's own page for the default browser: NIGHTFALL never makes
/// itself the default; the user picks it there.
fn open_default_browser_settings() -> Value {
    #[cfg(target_os = "windows")]
    let r = std::process::Command::new("explorer.exe").arg("ms-settings:defaultapps?registeredAppUser=NIGHTFALL").spawn();
    #[cfg(target_os = "macos")]
    let r = std::process::Command::new("/usr/bin/open").arg("x-apple.systempreferences:com.apple.Desktop-Settings.extension").spawn();
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let r: std::io::Result<std::process::Child> = Err(std::io::Error::other("non disponible"));
    r.map(|_| ok()).unwrap_or_else(|e| err(format!("impossible d'ouvrir les réglages du système : {e}")))
}

fn open_proxy_settings() -> Value {
    #[cfg(target_os = "windows")]
    let r = std::process::Command::new("explorer.exe").arg("ms-settings:network-proxy").spawn();
    #[cfg(target_os = "macos")]
    let r = std::process::Command::new("/usr/bin/open").arg("x-apple.systempreferences:com.apple.Network-Settings.extension").spawn();
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let r: std::io::Result<std::process::Child> = Err(std::io::Error::other("non disponible"));
    r.map(|_| ok()).unwrap_or_else(|e| err(format!("impossible d'ouvrir les réglages du système : {e}")))
}

fn theme_get() -> Value {
    let c = core();
    json!({
        "ok": true,
        "active": super::theme::active_theme(&c.settings.theme).id,
        "presets": super::theme::presets(),
        "custom": super::theme::custom_themes(&c.settings.theme),
    })
}

/// Saved, then every interface view and internal page reloads its colours.
fn theme_changed() {
    core().save_settings();
    on_ui(|| {
        windows::push_event_all(&json!({ "type": "theme" }));
        windows::exec_in_internal_tabs("window.nfReloadTheme && window.nfReloadTheme();");
    });
}

fn theme_set(body: &Value) -> Value {
    let id = s(body, "id").to_string();
    {
        let mut c = core();
        let known = super::theme::presets().into_iter().chain(super::theme::custom_themes(&c.settings.theme)).any(|t| t.id == id);
        if !known {
            return err("thème inconnu");
        }
        c.settings.theme.active = id;
    }
    theme_changed();
    ok()
}

/// Create (no id or an unknown one) or update a custom theme, then use it.
fn theme_save(body: &Value) -> Value {
    let mut t: super::theme::Theme = match serde_json::from_value(body.get("theme").cloned().unwrap_or(Value::Null)) {
        Ok(t) => t,
        Err(e) => return err(e),
    };
    t.name = t.name.trim().to_string();
    if let Err(e) = super::theme::validate(&t) {
        return err(e);
    }
    let id = {
        let mut c = core();
        let existing = super::theme::custom_themes(&c.settings.theme);
        if existing.len() >= 30 && !existing.iter().any(|e| e.id == t.id) {
            return err("30 thèmes personnels au plus");
        }
        if !t.id.starts_with("custom-") || !existing.iter().any(|e| e.id == t.id) {
            t.id = format!("custom-{}", nf_core::unix_now());
            while existing.iter().any(|e| e.id == t.id) {
                t.id.push('x');
            }
        }
        let value = serde_json::to_value(&t).unwrap_or(Value::Null);
        let list = &mut c.settings.theme.custom;
        match list.iter().position(|v| v.get("id").and_then(Value::as_str) == Some(t.id.as_str())) {
            Some(i) => list[i] = value,
            None => list.push(value),
        }
        c.settings.theme.active = t.id.clone();
        t.id.clone()
    };
    theme_changed();
    json!({ "ok": true, "id": id })
}

fn theme_delete(body: &Value) -> Value {
    let id = s(body, "id").to_string();
    if !id.starts_with("custom-") {
        return err("les thèmes intégrés ne peuvent pas être supprimés");
    }
    {
        let mut c = core();
        c.settings.theme.custom.retain(|v| v.get("id").and_then(Value::as_str) != Some(id.as_str()));
        if c.settings.theme.active == id {
            c.settings.theme.active = super::theme::presets()[0].id.clone();
        }
    }
    theme_changed();
    ok()
}

/// Build Shield's engine off CEF's threads (80 to 140 ms, more with large
/// lists), then save the settings (fingerprints recorded at the first build).
pub fn rebuild_shield_in_background() {
    background(
        || {
            let sh = state::shield();
            sh.rebuild();
            sh.integrity_errors()
        },
        |refused| {
            for e in &refused {
                state::log(&format!("Shield : {e}"));
            }
            core().save_settings();
        },
    );
}

fn shield_set(body: &Value) -> Value {
    match serde_json::from_value::<nf_shield::ShieldConfig>(body.get("config").cloned().unwrap_or(Value::Null)) {
        Ok(cfg) => {
            state::shield().set_config_later(cfg);
            core().save_settings();
            rebuild_shield_in_background();
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
    // Locked: no shortcut acts behind the lock screen (tabs, windows, zoom,
    // printing...); Emergency Lockdown stays available. Typing the code is
    // not affected (only shortcuts get here).
    if action != "emergency.lockdown" && super::lock_guard::locked() {
        super::lock_guard::shortcut_seen(action, false);
        state::log(&format!("raccourci « {action} » ignoré : NIGHTFALL est verrouillé"));
        return;
    }
    super::lock_guard::shortcut_seen(action, true);
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
        "tab.reload.hard" => {
            if let Some(b) = browser {
                b.reload_ignore_cache();
            }
        }
        "tab.reopen" => {
            windows::reopen_closed_tab(w);
        }
        "window.close" => windows::close_window(w),
        "window.fullscreen" => windows::toggle_window_fullscreen(w),
        "page.bookmarks" => {
            windows::add_tab(w, "nightfall://app/bookmarks.html", true);
        }
        "find" => super::browsing::open_find_bar(w),
        "find.next" => super::browsing::find_again(w, false),
        "find.prev" => super::browsing::find_again(w, true),
        "zoom.in" => super::browsing::zoom(w, "in"),
        "zoom.out" => super::browsing::zoom(w, "out"),
        "zoom.reset" => super::browsing::zoom(w, "reset"),
        "print" => {
            if let Some(h) = browser.and_then(|b| b.host()) {
                h.print();
            }
        }
        "save" => super::browsing::save_page(w, &windows::active_url(w).unwrap_or_default()),
        "view.source" => {
            if let Some(t) = windows::active_tab_id(w) {
                super::browsing::view_source(w, t, &windows::active_url(w).unwrap_or_default());
            }
        }
        a if a.starts_with("tab.index.") => {
            let n = a.trim_start_matches("tab.index.");
            windows::activate_index(w, if n == "last" { usize::MAX } else { n.parse().unwrap_or(0) });
        }
        "lock" => lock_now(),
        "emergency.lockdown" => emergency_lockdown(),
        "window.new" => windows::create_window(false, Vec::new()),
        "window.private" => windows::create_window(true, Vec::new()),
        "focus.address" => windows::focus_address(w),
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
        _ => {}
    }
}

/// Tab context menu and tab strip actions (UI thread).
fn tab_command(w: u32, id: u32, cmd: &str, terminate: bool) {
    match cmd {
        "tab.reopen" => {
            windows::reopen_closed_tab(w);
        }
        "tab.duplicate" => {
            windows::duplicate_tab(w, id);
        }
        "tab.newRight" => {
            windows::new_tab_right(w, id);
        }
        "tab.reload" => {
            if let Some(b) = windows::tab_browser(w, id) {
                b.reload();
            }
        }
        "tab.mute" => {
            let muted = windows::tab_muted(w, id);
            windows::set_tab_muted(w, id, !muted);
        }
        "tab.closeOthers" => windows::close_other_tabs(w, id),
        "tab.closeRight" => windows::close_tabs_right(w, id),
        "tab.unresponsive" => super::browsing::answer_unresponsive(w, id, terminate),
        _ => {}
    }
}

/// Last suggestions shown by each window's address bar (read by the dropdown).
static OMNIBOX: std::sync::Mutex<std::collections::BTreeMap<u32, Value>> = std::sync::Mutex::new(std::collections::BTreeMap::new());

/// Address bar suggestions from the local history and bookmarks only.
fn omnibox_suggest(w: u32, text: &str) -> Value {
    let text = text.trim();
    if text.is_empty() || text.len() > 300 {
        return json!({ "ok": true, "items": [] });
    }
    let c = core();
    let resolved = resolve_input(text, &c.settings);
    let keyword = c.settings.keyword_search(text);
    let (search, engine_label) = match &keyword {
        Some((name, url)) => (url.clone(), name.clone()),
        None => (c.settings.search_url(text), c.settings.search_engine_label()),
    };
    let typed = if resolved != search { Some(resolved) } else { None };
    // The SQL filter uses the longest word; the ranking checks every word.
    let word = text.split_whitespace().max_by_key(|w| w.len()).unwrap_or(text).to_string();
    let (history, bookmarks) = match c.storage() {
        Some(st) => (
            st.history(&nf_storage::HistoryQuery { text: Some(word.clone()), limit: Some(300), ..Default::default() }).unwrap_or_default(),
            st.search_bookmarks(&word).unwrap_or_default(),
        ),
        None => (Vec::new(), Vec::new()),
    };
    let mut candidates: Vec<super::omnibox::Candidate> = bookmarks
        .iter()
        .map(|b| super::omnibox::Candidate { url: &b.url, title: &b.title, visits: 1, last_visit: b.modified.max(b.created), bookmark: true })
        .collect();
    candidates.extend(history.iter().map(|h| super::omnibox::Candidate { url: &h.url, title: &h.title, visits: h.visit_count, last_visit: h.last_visit, bookmark: false }));
    let items = super::omnibox::suggest(text, typed.as_deref(), &search, &engine_label, &candidates, now(), 8);
    // The engine's own suggestions: only when turned on, for a search (not
    // an address or a keyword), never in a private window. They arrive
    // later as an `omnibox_remote` event; the local list is not delayed.
    if c.settings.search_suggestions && keyword.is_none() && typed.is_none() && text.chars().count() <= 100 {
        if let Some(endpoint) = super::omnibox::suggest_endpoint(&c.settings.search_engine) {
            let template = c.settings.search_template();
            request_remote_suggestions(w, text.to_string(), endpoint, template);
        }
    }
    json!({ "ok": true, "items": items })
}

fn request_remote_suggestions(w: u32, text: String, endpoint: &'static str, template: String) {
    on_ui(move || {
        if windows::is_private_window(w) {
            return;
        }
        super::ui_thread::background(
            move || {
                let q: String = url::form_urlencoded::byte_serialize(text.as_bytes()).collect();
                let agent = ureq::AgentBuilder::new().timeout(std::time::Duration::from_millis(1500)).user_agent("NIGHTFALL").build();
                let body = agent.get(&endpoint.replacen("%s", &q, 1)).call().ok().and_then(|r| {
                    let mut buf = String::new();
                    std::io::Read::read_to_string(&mut std::io::Read::take(r.into_reader(), 64 * 1024), &mut buf).ok().map(|_| buf)
                });
                let phrases = body.map(|b| super::omnibox::parse_opensearch(&b, &text, 4)).unwrap_or_default();
                let items: Vec<Value> = phrases
                    .into_iter()
                    .map(|p| {
                        let pq: String = url::form_urlencoded::byte_serialize(p.as_bytes()).collect();
                        json!({ "type": "search", "remote": true, "title": p, "url": template.replacen("%s", &pq, 1) })
                    })
                    .collect();
                (text, items)
            },
            move |(text, items)| {
                if !items.is_empty() {
                    windows::push_event(w, &json!({ "type": "omnibox_remote", "text": text, "items": items }));
                }
            },
        );
    });
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
        assert_eq!(resolve_input("@w chat", &s), "https://fr.wikipedia.org/w/index.php?search=chat");
        assert_eq!(resolve_input("@w", &s), "https://duckduckgo.com/?q=%40w");
    }
}

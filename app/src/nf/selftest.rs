//! End-to-end self test, compiled only with `--features selftest` (CI).
//!
//! Drives the real browser through the internal API: PIN setup, unlock,
//! navigation to a real web site, Shield blocking, history, lock with the
//! Session Vault, vault entries, emergency purge. Writes `SELFTEST OK` or
//! `SELFTEST FAIL: ...` to the log, then quits.

use super::api::{self, Caller};
use super::state::{self, core, BrowserKind};
use super::ui_thread::on_ui_delayed;
use super::windows;
use cef::{ImplBrowser, ImplBrowserHost, ImplFrame};
use serde_json::{json, Value};

const PIN: &str = "739204";

/// Last step reached, reported by the watchdog if the test gets stuck.
static STEP: std::sync::Mutex<&'static str> = std::sync::Mutex::new("début");
static DONE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn step(name: &'static str) {
    if let Ok(mut s) = STEP.lock() {
        *s = name;
    }
    state::log(&format!("selftest: étape {name}"));
}

fn call(cmd: &str, body: Value) -> Value {
    let caller = Caller { kind: BrowserKind::Ui { window_id: 1, side: false } };
    api::handle(&caller, cmd, &body)
}

fn fail(msg: &str) {
    DONE.store(true, std::sync::atomic::Ordering::SeqCst);
    state::log(&format!("SELFTEST FAIL: {msg}"));
    finish();
}

fn finish() {
    // Windows stay open a few seconds so CI can take a screenshot.
    on_ui_delayed(8000, || {
        for id in windows::window_ids() {
            windows::close_window(id);
        }
    });
}

fn expect_ok(step: &str, v: &Value) -> bool {
    if v.get("ok").and_then(Value::as_bool) == Some(true) {
        state::log(&format!("selftest: {step} ok"));
        true
    } else {
        fail(&format!("{step}: {v}"));
        false
    }
}

/// Poll `cond` every 500 ms up to `tries` times, then run `next` or fail.
fn wait_for(what: &'static str, tries: u32, cond: fn() -> bool, next: fn()) {
    on_ui_delayed(500, move || {
        if cond() {
            next();
        } else if tries == 0 {
            fail(&format!("délai dépassé : {what} ; onglets = {}", windows::tabs_state(1)));
        } else {
            wait_for(what, tries - 1, cond, next);
        }
    });
}

fn active_tab_loaded(prefix: &str) -> bool {
    let st = windows::tabs_state(1);
    st["tabs"].as_array().map(|tabs| {
        tabs.iter().any(|t| t["active"].as_bool() == Some(true) && t["loading"].as_bool() == Some(false) && t["url"].as_str().unwrap_or("").starts_with(prefix))
    }).unwrap_or(false)
}

pub fn start() {
    if std::env::var_os("NIGHTFALL_SELFTEST").is_none() {
        return;
    }
    state::log("selftest: début");
    // Watchdog on its own thread: reports where the test is stuck even if
    // the UI thread is blocked.
    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_secs(300));
        if !DONE.load(std::sync::atomic::Ordering::SeqCst) {
            let at = STEP.lock().map(|s| *s).unwrap_or("?");
            state::log(&format!("SELFTEST FAIL: bloqué à l'étape « {at} »"));
        }
    });
    on_ui_delayed(3000, step_setup);
}

fn step_setup() {
    step("création du code (Argon2id + secret de l'appareil)");
    let r = call("setup", json!({ "pin": PIN, "confirm": PIN }));
    if !expect_ok("création du code", &r) {
        return;
    }
    if r["recoveryKey"].as_str().map(|k| k.len()) != Some(39) {
        return fail("Recovery Key absente");
    }
    step("confirmation de la Recovery Key");
    if !expect_ok("confirmation recovery", &call("recovery.confirm", json!({}))) {
        return;
    }
    step("ouverture du premier onglet");
    wait_for("onglet Nouvel onglet", 30, || active_tab_loaded("nightfall://app/newtab"), step_navigate);
}

fn step_navigate() {
    step("navigation vers example.com");
    // Plain HTTP first, then HTTPS: tells a network problem from a TLS one.
    // In a new tab (no navigation away from an internal page).
    if !expect_ok("navigation", &call("tab.new", json!({ "url": "http://example.com/" }))) {
        return;
    }
    // Chromium upgrades plain HTTP navigations to HTTPS when the site
    // supports it (HTTPS-Upgrades): the tab must end on https://.
    wait_for("chargement de http://example.com (passage automatique en HTTPS)", 60, || active_tab_loaded("https://example.com"), step_https);
}

fn step_https() {
    step("navigation HTTPS");
    if !expect_ok("navigation HTTPS", &call("navigate", json!({ "input": "https://example.com/" }))) {
        return;
    }
    wait_for("chargement de https://example.com", 60, || active_tab_loaded("https://example.com"), step_shield);
}

fn step_shield() {
    // History recorded for the real site.
    let has_history = core()
        .storage()
        .and_then(|s| s.history(&nf_storage::HistoryQuery::default()).ok())
        .map(|h| h.iter().any(|e| e.url.starts_with("https://example.com")))
        .unwrap_or(false);
    step("historique et Shield");
    if !has_history {
        return fail("historique non enregistré");
    }
    state::log("selftest: historique ok");
    // Shield: custom filter, then a page that loads a matching script.
    let mut cfg = state::shield().config();
    cfg.custom_filters = "||ads.nightfall-selftest.invalid^\n".into();
    state::shield().set_config(cfg);
    let page = "data:text/html,<title>shield</title><script src='https://ads.nightfall-selftest.invalid/x.js'></script>";
    windows::navigate_active(1, page);
    wait_for("blocage Shield", 40, || state::shield().stats().total_blocked >= 1, step_lock);
}

fn step_lock() {
    step("verrouillage");
    api::lock_now();
    wait_for("verrouillage + Session Vault", 20, || !core().is_unlocked() && core().unlocked.is_none(), step_unlock);
}

fn step_unlock() {
    step("déverrouillage et coffre");
    let vault_file = core().paths.session_vault();
    if !vault_file.exists() {
        return fail("Session Vault non écrit au verrouillage");
    }
    state::log("selftest: Session Vault écrit");
    let wrong = call("unlock", json!({ "pin": "000001" }));
    if wrong["ok"].as_bool() != Some(false) {
        return fail("un mauvais code a été accepté");
    }
    if !expect_ok("déverrouillage", &call("unlock", json!({ "pin": PIN }))) {
        return;
    }
    let add = call("vault.add", json!({ "origin": "https://example.com", "username": "selftest", "password": "S3lf-Test!pw", "notes": "" }));
    if !expect_ok("ajout coffre", &add) {
        return;
    }
    let list = call("vault.list", json!({}));
    if list["entries"].as_array().map(|a| a.len()) != Some(1) {
        return fail("coffre : entrée absente");
    }
    let raw = std::fs::read(core().paths.vault()).unwrap_or_default();
    if raw.windows(12).any(|w| w == b"S3lf-Test!pw") {
        return fail("mot de passe en clair sur le disque");
    }
    let purge = call("emergency.purge", json!({ "pin": PIN, "confirm": "PURGER", "categories": ["passwords"] }));
    if !expect_ok("purge des mots de passe", &purge) {
        return;
    }
    let after = call("vault.list", json!({}));
    if after["entries"].as_array().map(|a| a.len()) != Some(0) {
        return fail("purge : le coffre n'est pas vide");
    }
    let status = call("security.status", json!({}));
    if !expect_ok("Security Center", &status) {
        return;
    }
    state::log(&format!("selftest: protection du code = {}", status["status"]["lock"]["backend"]));
    // End on a real web page so CI can check on screen that web content is
    // actually displayed (example.com: light page with dark text).
    // Shield on a real page: scriptlet, CSS hiding rule, procedural rule.
    step("Shield : scriptlet, masquage et règle procédurale");
    let mut cfg = state::shield().config();
    cfg.custom_filters = "||ads.nightfall-selftest.invalid^\nexample.com##+js(set-constant, nfShieldTest, 42)\nexample.com##.nf-selftest-pub\nexample.com##p:has-text(nightfall-procedural-test)\n##.nf-selftest-generic\n".into();
    state::shield().set_config(cfg);
    windows::navigate_active(1, "https://example.com/");
    wait_for("affichage final de https://example.com", 40, || active_tab_loaded("https://example.com"), || {
        state::log(&format!("selftest: disposition {}", windows::layout_report(1)));
        on_ui_delayed(1500, || {
            if let Some(b) = windows::active_browser(1) {
                if let Some(f) = b.main_frame() {
                    // Elements created now: the hiding rules must catch them.
                    let js = "(function () { \
                        var pub = document.createElement('div'); pub.className = 'nf-selftest-pub'; pub.textContent = 'x'; document.body.appendChild(pub); \
                        var p = document.createElement('p'); p.textContent = 'nightfall-procedural-test'; document.body.appendChild(p); \
                        var g = document.createElement('div'); g.className = 'nf-selftest-generic'; g.textContent = 'x'; document.body.appendChild(g); \
                        setTimeout(function () { console.log('shield-check ' + JSON.stringify({ \
                          scriptlet: window.nfShieldTest === 42, \
                          css: getComputedStyle(pub).display === 'none', \
                          procedural: getComputedStyle(p).display === 'none', \
                          generic: getComputedStyle(g).display === 'none' })); }, 1500); })();";
                    f.execute_java_script(Some(&cef::CefString::from(js)), Some(&cef::CefString::from("nightfall://selftest")), 0);
                }
            }
            wait_for("résultat du contrôle Shield", 20, || CONSOLE.lock().map(|c| c.is_some()).unwrap_or(false), || {
                let res = CONSOLE.lock().ok().and_then(|c| c.clone()).unwrap_or_default();
                state::log(&format!("selftest: {res}"));
                if ["scriptlet", "css", "procedural", "generic"].iter().any(|k| !res.contains(&format!("\"{k}\":true"))) {
                    return fail(&format!("Shield sur example.com : {res}"));
                }
                step_find();
            });
        });
    });
}

static CONSOLE: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);
static COPY_CHECK: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// Console messages from the self test's own scripts.
pub fn console_result(msg: &str) {
    if msg.starts_with("shield-check") {
        if let Ok(mut c) = CONSOLE.lock() {
            *c = Some(msg.to_string());
        }
    }
    if msg.starts_with("copy-check") {
        if let Ok(mut c) = COPY_CHECK.lock() {
            *c = Some(msg.to_string());
        }
    }
}

// ----------------------------------------------------- browser features

/// Tab showing https://example.com when the feature checks start.
static MAIN_TAB: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

fn tab_count() -> usize {
    windows::with_window(1, |c| c.tabs.len()).unwrap_or(0)
}

fn active_zoom() -> f64 {
    windows::active_browser(1).and_then(|b| b.host()).map(|h| h.zoom_level()).unwrap_or(-99.0)
}

fn find_done() -> bool {
    matches!(*super::browsing::LAST_FIND.lock().unwrap(), Some((n, _, true)) if n > 0)
}

fn step_find() {
    step("recherche dans la page");
    MAIN_TAB.store(windows::active_tab_id(1).unwrap_or(0), std::sync::atomic::Ordering::SeqCst);
    super::browsing::open_find_bar(1);
    if windows::popup_panel(1).as_deref() != Some("find") {
        return fail("barre de recherche non ouverte");
    }
    super::browsing::find(1, "example", true, false, false);
    wait_for("résultat de la recherche dans la page", 20, find_done, || {
        let r = *super::browsing::LAST_FIND.lock().unwrap();
        state::log(&format!("selftest: recherche « example » sur example.com : {r:?}"));
        // Next match: the active match moves.
        super::browsing::find(1, "example", true, false, true);
        on_ui_delayed(800, || {
            let r = *super::browsing::LAST_FIND.lock().unwrap();
            state::log(&format!("selftest: résultat suivant : {r:?}"));
            windows::popup_close(1);
            step_zoom();
        });
    });
}

fn step_zoom() {
    step("zoom");
    super::browsing::zoom(1, "in");
    wait_for("zoom avant", 10, || active_zoom() > 0.1, || {
        let st = windows::tabs_state(1);
        let pct = st["tabs"].as_array().and_then(|t| t.iter().find(|t| t["active"] == true)).map(|t| t["zoom"].as_i64().unwrap_or(0)).unwrap_or(0);
        state::log(&format!("selftest: zoom avant -> niveau {:.3} ({pct} %)", active_zoom()));
        if pct != 110 {
            return fail(&format!("zoom avant : {pct} % au lieu de 110 %"));
        }
        on_ui_delayed(400, || {
            let saved = core().storage().and_then(|s| s.setting::<std::collections::HashMap<String, f64>>("site_zoom").ok().flatten()).unwrap_or_default();
            if !saved.contains_key("example.com") {
                return fail(&format!("zoom non mémorisé pour le site : {saved:?}"));
            }
            state::log("selftest: zoom mémorisé pour example.com");
            super::browsing::zoom(1, "reset");
            wait_for("zoom réinitialisé", 10, || active_zoom().abs() < 0.01, || {
                on_ui_delayed(400, || {
                    let saved = core().storage().and_then(|s| s.setting::<std::collections::HashMap<String, f64>>("site_zoom").ok().flatten()).unwrap_or_default();
                    if saved.contains_key("example.com") {
                        return fail("zoom par défaut toujours mémorisé");
                    }
                    state::log("selftest: zoom ok");
                    step_tabs();
                });
            });
        });
    });
}

fn step_tabs() {
    step("onglets : dupliquer, fermer, rouvrir, lien en arrière-plan");
    let main = MAIN_TAB.load(std::sync::atomic::Ordering::SeqCst);
    let before = tab_count();
    let Some(dup) = windows::duplicate_tab(1, main) else { return fail("duplication impossible") };
    if tab_count() != before + 1 || windows::active_tab_id(1) != Some(dup) {
        return fail("onglet dupliqué absent ou inactif");
    }
    windows::close_tab(1, dup);
    if tab_count() != before {
        return fail("onglet dupliqué non fermé");
    }
    let Some(re) = windows::reopen_closed_tab(1) else { return fail("aucun onglet à rouvrir") };
    if tab_count() != before + 1 || !windows::tab_url(1, re).unwrap_or_default().starts_with("https://example.com") {
        return fail("onglet fermé non rouvert");
    }
    windows::close_tab(1, re);
    windows::activate_tab(1, main);
    // Middle-click / Ctrl+click: background tab right after its opener.
    let Some(bg) = windows::open_from_tab(1, main, "https://example.com/?nf-arriere-plan", false) else { return fail("lien en arrière-plan non ouvert") };
    let order: Vec<u32> = windows::with_window(1, |c| c.tabs.iter().map(|t| t.id).collect()).unwrap_or_default();
    let pos = |id: u32| order.iter().position(|x| *x == id);
    if windows::active_tab_id(1) != Some(main) || pos(bg) != pos(main).map(|p| p + 1) {
        return fail(&format!("onglet en arrière-plan mal placé ou activé : {order:?}"));
    }
    if windows::open_from_tab(1, main, "nightfall://app/settings.html", false).is_some() {
        return fail("une page web a pu ouvrir une page interne");
    }
    windows::close_tab(1, bg);
    state::log("selftest: onglets ok");
    let sugg = call("omnibox.suggest", json!({ "text": "example" }));
    let found = sugg["items"].as_array().map(|a| a.iter().any(|i| i["type"] != "search" && i["url"].as_str().unwrap_or("").starts_with("https://example.com"))).unwrap_or(false);
    state::log(&format!("selftest: suggestions « example » : {}", sugg["items"]));
    if !found {
        return fail("suggestions de la barre d'adresse : historique absent");
    }
    step_popup();
}

static TABS_BEFORE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

fn popup_loaded() -> bool {
    let st = windows::tabs_state(1);
    st["tabs"].as_array().map(|t| t.iter().any(|t| t["url"].as_str().unwrap_or("").contains("nf-popup") && t["loading"] == false)).unwrap_or(false)
}

/// "Sign in with..." flows: window.open() with a user gesture opens a tab
/// that keeps window.opener; its window.close() removes only that tab.
fn step_popup() {
    step("fenêtre de connexion : window.open puis window.close");
    TABS_BEFORE.store(tab_count(), std::sync::atomic::Ordering::SeqCst);
    let Some(h) = windows::active_browser(1).and_then(|b| b.host()) else { return fail("pas d'onglet actif") };
    let msg = r#"{"id":4243,"method":"Runtime.evaluate","params":{"expression":"window.nfPopup = window.open('https://example.com/?nf-popup', 'nfpopup', 'width=480,height=600'); 1","userGesture":true}}"#;
    h.send_dev_tools_message(Some(msg.as_bytes()));
    wait_for("ouverture de la fenêtre pop-up", 40, popup_loaded, || {
        let n = TABS_BEFORE.load(std::sync::atomic::Ordering::SeqCst);
        if tab_count() != n + 1 {
            return fail(&format!("pop-up : {} onglets au lieu de {}", tab_count(), n + 1));
        }
        // The popup page checks its opener, then closes itself.
        let Some(h) = windows::active_browser(1).and_then(|b| b.host()) else { return fail("pop-up non active") };
        let msg = r#"{"id":4244,"method":"Runtime.evaluate","params":{"expression":"console.log('opener ' + (window.opener ? 'présent' : 'absent')); window.close(); 1","userGesture":true}}"#;
        h.send_dev_tools_message(Some(msg.as_bytes()));
        wait_for("fermeture de la pop-up par sa page", 20, || tab_count() == TABS_BEFORE.load(std::sync::atomic::Ordering::SeqCst), || {
            if !windows::window_ids().contains(&1) {
                return fail("window.close() d'une pop-up a fermé la fenêtre");
            }
            let main = MAIN_TAB.load(std::sync::atomic::Ordering::SeqCst);
            if windows::active_tab_id(1) != Some(main) {
                return fail("retour à la page d'origine après la pop-up : non");
            }
            state::log("selftest: pop-up ouverte puis fermée par sa page, fenêtre conservée");
            on_ui_delayed(500, step_fullscreen);
        });
    });
}

fn step_fullscreen() {
    step("plein écran HTML5");
    let Some(h) = windows::active_browser(1).and_then(|b| b.host()) else { return fail("pas d'onglet actif") };
    // A user gesture is required by requestFullscreen(): the DevTools
    // protocol (in process, no debugging port) provides one.
    let msg = r#"{"id":4242,"method":"Runtime.evaluate","params":{"expression":"document.documentElement.requestFullscreen()","userGesture":true,"awaitPromise":false}}"#;
    let sent = h.send_dev_tools_message(Some(msg.as_bytes()));
    state::log(&format!("selftest: demande de plein écran envoyée ({sent})"));
    wait_for("plein écran de la page", 20, || { let (on, top, _) = windows::fullscreen_report(1); on && !top }, || {
        let r = windows::fullscreen_report(1);
        state::log(&format!("selftest: plein écran : page={} interface visible={} fenêtre plein écran={}", r.0, r.1, r.2));
        on_ui_delayed(1500, || {
            windows::exit_content_fullscreen(1);
            wait_for("sortie du plein écran", 20, || { let (on, top, _) = windows::fullscreen_report(1); !on && top }, || {
                let r = windows::fullscreen_report(1);
                state::log(&format!("selftest: sortie du plein écran : interface visible={} fenêtre plein écran={}", r.1, r.2));
                on_ui_delayed(2000, step_copy);
            });
        });
    });
}

fn step_copy() {
    step("copie dans le presse-papiers");
    windows::copy_text(1, "nightfall-copy-test");
    on_ui_delayed(500, || {
        if let Some(f) = windows::active_browser(1).and_then(|b| b.main_frame()) {
            let js = "(function(){var t=document.createElement('textarea');t.id='nf-paste';document.body.appendChild(t);t.focus();})();";
            f.execute_java_script(Some(&cef::CefString::from(js)), Some(&cef::CefString::from("nightfall://selftest")), 0);
            f.paste();
            let js = "setTimeout(function(){var t=document.getElementById('nf-paste');console.log('copy-check '+(t?t.value:'?'));if(t)t.remove();},300);";
            f.execute_java_script(Some(&cef::CefString::from(js)), Some(&cef::CefString::from("nightfall://selftest")), 0);
        }
        // Informative only: a build machine may not give the window the
        // focus that pasting needs.
        on_ui_delayed(1500, || {
            let r = COPY_CHECK.lock().ok().and_then(|c| c.clone()).unwrap_or_else(|| "pas de réponse".into());
            state::log(&format!("selftest: presse-papiers : {r}"));
            step_final_menu();
        });
    });
}

fn step_final_menu() {
    step("menu déroulant final");
    // Dropdown menu over the page (visible on the CI screenshot).
    windows::popup_open(1, "menu", cef::Rect { x: 1200, y: 44, width: 34, height: 34 }, 300, 420);
    // Checked at once: the menu closes itself when focus moves
    // elsewhere (as intended), which a build machine may do.
    let ok = windows::popup_report(1);
    on_ui_delayed(1200, move || {
        state::log(&format!("selftest: menu déroulant {ok}"));
        if !ok.starts_with("ouvert") {
            return fail("menu déroulant non ouvert");
        }
        DONE.store(true, std::sync::atomic::Ordering::SeqCst);
        state::log("SELFTEST OK");
        finish();
    });
}

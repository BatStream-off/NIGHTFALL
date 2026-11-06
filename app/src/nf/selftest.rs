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
use cef::{ImplBrowser, ImplBrowserHost, ImplBrowserView, ImplFrame, ImplValue, ImplView};
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

/// Poll `cond` every 500 ms up to `tries` times, then call `next` with the
/// outcome (true = condition met).
fn wait_or(tries: u32, cond: fn() -> bool, next: fn(bool)) {
    on_ui_delayed(500, move || {
        if cond() {
            next(true);
        } else if tries == 0 {
            next(false);
        } else {
            wait_or(tries - 1, cond, next);
        }
    });
}

fn active_tab_url_starts(prefix: &str) -> bool {
    let st = windows::tabs_state(1);
    st["tabs"].as_array().map(|tabs| tabs.iter().any(|t| t["active"].as_bool() == Some(true) && t["url"].as_str().unwrap_or("").starts_with(prefix))).unwrap_or(false)
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
    // No auto-lock during the test: a build machine has had no keyboard or
    // mouse input for a long time (OS idle time), and the lock is tested
    // explicitly below.
    core().settings.auto_lock_minutes = 0;
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
    // The upgrade above was made by NIGHTFALL's HTTPS-First (on by default).
    let upgrades = super::https_first::UPGRADES.load(std::sync::atomic::Ordering::Relaxed);
    state::log(&format!("selftest: HTTPS-First : {upgrades} passage(s) en HTTPS"));
    if upgrades == 0 {
        return fail("HTTPS-First : http://example.com n'a pas été passé en https par NIGHTFALL");
    }
    step("navigation HTTPS");
    if !expect_ok("navigation HTTPS", &call("navigate", json!({ "input": "https://example.com/" }))) {
        return;
    }
    wait_for("chargement de https://example.com", 60, || active_tab_loaded("https://example.com"), step_prefs);
}

/// Settings round trip, search engine, and the settings applied through
/// Chromium (preferences read back, default zoom, DNS over HTTPS).
fn step_prefs() {
    step("paramètres : aller-retour et préférences Chromium");
    let gpc = super::client::GPC_SENT.load(std::sync::atomic::Ordering::Relaxed);
    state::log(&format!("selftest: Global Privacy Control : en-tête ajouté à {gpc} requête(s)"));
    if gpc == 0 {
        return fail("Global Privacy Control : en-tête Sec-GPC jamais ajouté");
    }
    let mut s = core().settings.clone();
    s.font_size = 20;
    s.default_zoom = 125;
    s.search_engine = "qwant".into();
    s.accept_languages = "fr-FR,fr,en".into();
    s.secure_dns = "quad9".into();
    s.startup_mode = "pages".into();
    s.startup_pages = vec!["https://example.com/".into()];
    let r = call("settings.set", json!({ "settings": s }));
    if !expect_ok("settings.set", &r) {
        return;
    }
    for o in r["chromium"].as_array().cloned().unwrap_or_default() {
        state::log(&format!("selftest: préférence {} : {}", o["name"].as_str().unwrap_or("?"), if o["ok"].as_bool() == Some(true) { "acceptée".to_string() } else { format!("REFUSÉE ({})", o["error"].as_str().unwrap_or("")) }));
    }
    let back = call("settings.get", json!({}));
    let bs = &back["settings"];
    if bs["font_size"] != 20 || bs["default_zoom"] != 125 || bs["search_engine"] != "qwant" || bs["startup_pages"][0] != "https://example.com/" || bs["restore_session"] != false {
        return fail(&format!("aller-retour des paramètres : {bs}"));
    }
    let url = api::resolve_input("chat noir", &core().settings);
    if !url.starts_with("https://www.qwant.com/?q=chat+noir") {
        return fail(&format!("moteur de recherche : {url}"));
    }
    state::log("selftest: aller-retour des paramètres et moteur de recherche ok");
    let int = |n: &str| super::prefs::read_back(n, false).map(|v| v.int());
    let text = |n: &str, g: bool| super::prefs::read_back(n, g).map(|v| cef::CefString::from(&v.string()).to_string());
    state::log(&format!(
        "selftest: relu dans Chromium : police={:?} cookies={:?} webrtc={:?} langues={:?} doh={:?} {:?}",
        int("webkit.webprefs.default_font_size"),
        int("profile.cookie_controls_mode"),
        text("webrtc.ip_handling_policy", false),
        text("intl.accept_languages", false),
        text("dns_over_https.mode", true),
        text("dns_over_https.templates", true)
    ));
    on_ui_delayed(500, || {
        // DNS over HTTPS (Quad9, strict mode): a host never resolved before.
        step("DNS sécurisé (Quad9) et zoom par défaut : chargement de https://www.example.org");
        let _ = call("tab.new", json!({ "url": "https://www.example.org/" }));
        wait_or(40, || active_tab_loaded("https://www.example.org"), |loaded| {
            state::log(&format!("selftest: DNS sécurisé : https://www.example.org {}", if loaded { "chargé" } else { "NON chargé" }));
            // Default zoom 125 % applied to the new page.
            let want = super::prefs::zoom_level(125);
            let zoom = windows::active_browser(1).and_then(|b| b.host()).map(|h| h.zoom_level());
            state::log(&format!("selftest: zoom par défaut attendu {want:.4}, lu {zoom:?}"));
            if loaded && zoom.map(|z| (z - want).abs() > 1e-3).unwrap_or(true) {
                return fail("zoom par défaut non appliqué");
            }
            let r = call("settings.set", json!({ "settings": state::Settings { auto_lock_minutes: 0, ..Default::default() } }));
            if !expect_ok("paramètres par défaut", &r) {
                return;
            }
            step_insecure();
        });
    });
}

/// A public-looking name that does not exist: its https:// version always
/// fails, so the warning page must appear (no third-party site involved).
const NO_HTTPS: &str = "http://nightfall-https-first-selftest.example.net/";

/// The http:// address was let through (not upgraded) after "Continuer";
/// it does not exist, so the tab shows a network error for it.
fn no_https_site_opened_over_http() -> bool {
    super::https_first::ALLOWED_NAVIGATIONS.load(std::sync::atomic::Ordering::Relaxed) > 0
}

/// HTTPS-First on a site without HTTPS: warning page, then "Continuer"
/// loads the http:// address itself (not upgraded again).
fn step_insecure() {
    step("HTTPS-First : site sans HTTPS (avertissement puis Continuer)");
    let _ = call("tab.new", json!({ "url": NO_HTTPS }));
    wait_for("page d'avertissement HTTPS-First", 40, || active_tab_url_starts("nightfall://app/insecure.html"), || {
        state::log("selftest: page d'avertissement HTTPS-First affichée");
        let Some(tab_id) = windows::active_tab_id(1) else { return fail("onglet actif introuvable") };
        let upgrades = super::https_first::UPGRADES.load(std::sync::atomic::Ordering::Relaxed);
        let caller = Caller { kind: BrowserKind::Tab { window_id: 1, tab_id, private: false } };
        let r = api::handle(&caller, "https.proceed", &json!({ "url": NO_HTTPS }));
        if !expect_ok("Continuer vers le site", &r) {
            return;
        }
        UPGRADES_BEFORE.store(upgrades, std::sync::atomic::Ordering::Relaxed);
        wait_for("ouverture de l'adresse http:// après « Continuer »", 40, no_https_site_opened_over_http, || {
            if super::https_first::UPGRADES.load(std::sync::atomic::Ordering::Relaxed) != UPGRADES_BEFORE.load(std::sync::atomic::Ordering::Relaxed) {
                return fail("HTTPS-First : l'adresse a de nouveau été passée en https après « Continuer »");
            }
            state::log("selftest: HTTPS-First : adresse http:// ouverte telle quelle après « Continuer »");
            // The warning was recorded as a security event, with the site only.
            let r = call("security.events", json!({ "limit": 20 }));
            let site = url::Url::parse(NO_HTTPS).ok().and_then(|u| u.host_str().map(str::to_string)).unwrap_or_default();
            let found = r["events"].as_array().map(|a| a.iter().any(|e| e["kind"] == "https_warning" && e["site"] == site.as_str())).unwrap_or(false);
            if !found {
                return fail(&format!("événement de sécurité HTTPS-First absent : {r}"));
            }
            state::log("selftest: événement de sécurité HTTPS-First enregistré");
            step_shield();
        });
    });
}

static UPGRADES_BEFORE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

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
    // "Effacer les données de navigation", last hour, every type.
    step("effacement des données de navigation (dernière heure)");
    let cleared = call("browsing.clear", json!({ "range": "hour", "history": true, "downloads": true, "cookies": true, "cache": true }));
    if !expect_ok("effacement des données", &cleared) {
        return;
    }
    let left = core().storage().and_then(|s| s.history(&nf_storage::HistoryQuery::default()).ok()).map(|h| h.len()).unwrap_or(99);
    state::log(&format!("selftest: effacement : {cleared} ; historique restant : {left}"));
    if cleared["history"].as_u64().unwrap_or(0) == 0 || left != 0 {
        return fail("effacement de l'historique de la dernière heure");
    }
    // Cookies, site data and cache are cleared asynchronously.
    on_ui_delayed(4000, step_final);
}

fn step_final() {
    let sites = super::browsing_data::SITES_CLEARED.load(std::sync::atomic::Ordering::Relaxed);
    state::log(&format!("selftest: données de {sites} site(s) effacées"));
    if sites == 0 {
        return fail("effacement des données des sites (DevTools Storage.clearDataForOrigin)");
    }
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
                          generic: getComputedStyle(g).display === 'none', \
                          gpc: navigator.globalPrivacyControl === true })); }, 1500); })();";
                    f.execute_java_script(Some(&cef::CefString::from(js)), Some(&cef::CefString::from("nightfall://selftest")), 0);
                }
            }
            wait_for("résultat du contrôle Shield", 20, || CONSOLE.lock().map(|c| c.is_some()).unwrap_or(false), || {
                let res = CONSOLE.lock().ok().and_then(|c| c.clone()).unwrap_or_default();
                state::log(&format!("selftest: {res}"));
                if ["scriptlet", "css", "procedural", "generic", "gpc"].iter().any(|k| !res.contains(&format!("\"{k}\":true"))) {
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
    if let Some(json) = msg.strip_prefix("m8-rect ") {
        if let Ok(mut r) = RECT.lock() {
            *r = serde_json::from_str(json).ok();
        }
    }
    if msg.starts_with("copy-check") {
        if let Ok(mut c) = COPY_CHECK.lock() {
            *c = Some(msg.to_string());
        }
    }
    if let Some(rest) = msg.strip_prefix("nfcheck-") {
        if let Some((key, payload)) = rest.split_once(' ') {
            if let Ok(mut c) = CHECKS.lock() {
                c.retain(|(k, _)| k != key);
                c.push((key.to_string(), payload.to_string()));
            }
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

fn saved_zoom_for_example() -> bool {
    core().storage().and_then(|s| s.setting::<std::collections::HashMap<String, f64>>("site_zoom").ok().flatten()).map(|m| m.contains_key("example.com")).unwrap_or(false)
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
        wait_for("zoom mémorisé pour le site", 10, || saved_zoom_for_example(), || {
            state::log("selftest: zoom mémorisé pour example.com");
            super::browsing::zoom(1, "reset");
            wait_for("zoom réinitialisé", 10, || active_zoom().abs() < 0.01, || {
                wait_for("zoom par défaut oublié", 10, || !saved_zoom_for_example(), || {
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
            step_home();
        });
    });
}

// ------------------------------------- real clicks in the interface (M-0008)
//
// The interface views receive real mouse events through the DevTools
// protocol (`Input.dispatchMouseEvent`, press and release sent separately),
// processed by Chromium like hardware events: the bugs that depend on the
// order of events (tab strip rebuilt between press and release, a popup's
// late close) only show up this way.

static RECT: std::sync::Mutex<Option<Value>> = std::sync::Mutex::new(None);
static DT_ID_CLICK: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(950_000);
static UI_BASE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static ANCHOR: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
static TARGET: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
static RACE_OK: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
const TRIALS: u32 = 5;
const MENU_CYCLES: u32 = 20;

/// Interface view of window 1: top bar or vertical tab strip.
fn ui_browser(side: bool) -> Option<cef::Browser> {
    windows::with_window(1, |c| if side { c.side.browser() } else { c.top.browser() }).flatten()
}

fn ui_js(side: bool, code: &str) {
    if let Some(f) = ui_browser(side).and_then(|b| b.main_frame()) {
        f.execute_java_script(Some(&cef::CefString::from(code)), Some(&cef::CefString::from("nightfall://selftest")), 0);
    }
}

fn mouse(side: bool, kind: &str, x: f64, y: f64) {
    let (button, buttons) = match kind {
        "mousePressed" => ("left", 1),
        "mouseReleased" => ("left", 0),
        _ => ("none", 0),
    };
    let clicks = if kind == "mouseMoved" { 0 } else { 1 };
    let msg = json!({ "id": DT_ID_CLICK.fetch_add(1, std::sync::atomic::Ordering::Relaxed), "method": "Input.dispatchMouseEvent",
        "params": { "type": kind, "x": x, "y": y, "button": button, "buttons": buttons, "clickCount": clicks } }).to_string();
    if let Some(h) = ui_browser(side).and_then(|b| b.host()) {
        h.send_dev_tools_message(Some(msg.as_bytes()));
    }
}

/// Poll `cond` every 200 ms up to `tries` times, then `next(met)`.
fn poll_ok<C, N>(tries: u32, cond: C, next: N)
where
    C: Fn() -> bool + Send + 'static,
    N: FnOnce(bool) + Send + 'static,
{
    on_ui_delayed(200, move || {
        if cond() {
            next(true);
        } else if tries == 0 {
            next(false);
        } else {
            poll_ok(tries - 1, cond, next);
        }
    });
}

/// Centre of the element matching `sel` in an interface view.
fn with_rect<N: FnOnce(f64, f64) + Send + 'static>(side: bool, sel: String, next: N) {
    if let Ok(mut r) = RECT.lock() {
        *r = None;
    }
    let sel_js = serde_json::to_string(&sel).unwrap_or_else(|_| "\"\"".into());
    ui_js(side, &format!("(function(){{var e=document.querySelector({sel_js});var o={{ok:false}};if(e){{var r=e.getBoundingClientRect();o={{ok:r.width>0&&r.height>0,x:r.left+r.width/2,y:r.top+r.height/2}};}}console.log('m8-rect '+JSON.stringify(o));}})();"));
    poll_ok(25, || RECT.lock().map(|r| r.is_some()).unwrap_or(false), move |_| {
        let r = RECT.lock().ok().and_then(|r| r.clone()).unwrap_or(Value::Null);
        if r["ok"].as_bool() != Some(true) {
            return fail(&format!("élément introuvable ou invisible dans l'interface : {sel} ({r})"));
        }
        next(r["x"].as_f64().unwrap_or(0.0), r["y"].as_f64().unwrap_or(0.0));
    });
}

/// Real left click: move, press, `during` (between press and release), release.
fn real_click<D, N>(side: bool, x: f64, y: f64, during: D, next: N)
where
    D: FnOnce() + Send + 'static,
    N: FnOnce() + Send + 'static,
{
    mouse(side, "mouseMoved", x, y);
    mouse(side, "mousePressed", x, y);
    on_ui_delayed(60, move || {
        during();
        on_ui_delayed(90, move || {
            mouse(side, "mouseReleased", x, y);
            next();
        });
    });
}

// ---- Accueil from a web page

fn step_home() {
    step("bouton Accueil depuis une page web (clic réel)");
    let main = MAIN_TAB.load(std::sync::atomic::Ordering::SeqCst);
    if windows::active_tab_id(1) != Some(main) || !active_tab_url_starts("https://example.com") {
        return fail(&format!("Accueil : l'onglet actif n'est pas la page web attendue ; onglets = {}", windows::tabs_state(1)));
    }
    with_rect(false, "#b-home".into(), |x, y| {
        real_click(false, x, y, || {}, || {
            poll_ok(75, || active_tab_loaded("nightfall://app/newtab"), |ok| {
                let main = MAIN_TAB.load(std::sync::atomic::Ordering::SeqCst);
                if !ok {
                    return fail(&format!("Accueil depuis une page web : page Nouvel onglet non affichée ; onglets = {}", windows::tabs_state(1)));
                }
                if windows::active_tab_id(1) != Some(main) {
                    return fail("Accueil : la page Nouvel onglet ne s'est pas ouverte dans l'onglet actif");
                }
                let pending = state::internal_pending_for_tab(main);
                let flag = windows::tab_browser(1, main).and_then(|b| state::browser_info(b.identifier())).map(|i| i.allow_internal_nav);
                state::log(&format!("selftest: Accueil depuis https://example.com : Nouvel onglet affiché dans le même onglet ; autorisations en attente pour l'onglet = {pending} ; autorisation du navigateur restante = {flag:?}"));
                if pending != 0 || flag != Some(false) {
                    return fail("Accueil : une autorisation de page interne est restée en attente");
                }
                let _ = call("navigate", json!({ "input": "https://example.com/" }));
                wait_for("retour sur https://example.com après Accueil", 60, || active_tab_loaded("https://example.com"), || {
                    // A web page still cannot open an internal page by itself.
                    let main = MAIN_TAB.load(std::sync::atomic::Ordering::SeqCst);
                    windows::exec_in_tab(1, main, "location.href = 'nightfall://app/settings.html';");
                    on_ui_delayed(2000, || {
                        if !active_tab_url_starts("https://example.com") {
                            return fail(&format!("une page web a ouvert une page interne après Accueil ; onglets = {}", windows::tabs_state(1)));
                        }
                        state::log("selftest: après Accueil, une page web ne peut toujours pas ouvrir nightfall:// (refusé)");
                        step_close_buttons();
                    });
                });
            });
        });
    });
}

// ---- tab close button (horizontal and vertical strips)

fn step_close_buttons() {
    step("croix des onglets : clics réels (barre horizontale)");
    // Leftover tabs of the previous steps: only the main tab stays.
    let main = MAIN_TAB.load(std::sync::atomic::Ordering::SeqCst);
    let others: Vec<u32> = windows::with_window(1, |c| c.tabs.iter().map(|t| t.id).filter(|id| *id != main).collect()).unwrap_or_default();
    for id in others {
        windows::close_tab(1, id);
    }
    on_ui_delayed(800, || {
        close_trial(false, false, 0, Box::new(|| close_trial(false, true, 0, Box::new(step_vertical_on))));
    });
}

fn step_vertical_on() {
    step("croix des onglets : clics réels (onglets verticaux)");
    let mut s = core().settings.clone();
    s.vertical_tabs = true;
    if !expect_ok("onglets verticaux activés", &call("settings.set", json!({ "settings": s }))) {
        return;
    }
    // The interface views reload with the new layout.
    on_ui_delayed(3000, || {
        windows::push_tabs(1);
        on_ui_delayed(800, || close_trial(true, false, 0, Box::new(|| close_trial(true, true, 0, Box::new(step_vertical_off)))));
    });
}

fn step_vertical_off() {
    let mut s = core().settings.clone();
    s.vertical_tabs = false;
    if !expect_ok("onglets verticaux désactivés", &call("settings.set", json!({ "settings": s }))) {
        return;
    }
    on_ui_delayed(3000, || {
        windows::push_tabs(1);
        on_ui_delayed(800, step_menus);
    });
}

/// One real click on the close button of a new tab: active (`background`
/// false) or in the background. The strip is rebuilt between the press and
/// the release (as a page load does).
fn close_trial(side: bool, background: bool, i: u32, done: Box<dyn FnOnce() + Send>) {
    if i == TRIALS {
        done();
        return;
    }
    let Some(anchor) = windows::active_tab_id(1) else { return fail("croix : aucun onglet actif") };
    let before = tab_count();
    let Some(target) = windows::add_tab(1, windows::NEW_TAB_URL, !background) else { return fail("croix : onglet de test non créé") };
    ANCHOR.store(anchor, std::sync::atomic::Ordering::SeqCst);
    TARGET.store(target, std::sync::atomic::Ordering::SeqCst);
    let tab_sel = if side { format!(".vtab[data-id=\"{target}\"]") } else { format!("#tabs .tab[data-id=\"{target}\"]") };
    on_ui_delayed(900, move || {
        let close_sel = format!("{tab_sel} .t-close");
        // Hover the tab first: narrow background tabs show their close
        // button on hover only.
        with_rect(side, tab_sel, move |tx, ty| {
            mouse(side, "mouseMoved", tx, ty);
            on_ui_delayed(200, move || {
                with_rect(side, close_sel, move |x, y| {
                    real_click(side, x, y, || windows::push_tabs(1), move || {
                        on_ui_delayed(1200, move || {
                            let target = TARGET.load(std::sync::atomic::Ordering::SeqCst);
                            let anchor = ANCHOR.load(std::sync::atomic::Ordering::SeqCst);
                            let closed = windows::tab_url(1, target).is_none();
                            let active = windows::active_tab_id(1);
                            let active_ok = if background { active == Some(anchor) } else { active.is_some() && active != Some(target) };
                            let count_ok = tab_count() == before;
                            state::log(&format!(
                                "selftest: croix {} onglet {} essai {}/{TRIALS} : fermé={} onglet_actif_conservé={} onglets={} (attendu {before})",
                                if side { "verticale" } else { "horizontale" },
                                if background { "d'arrière-plan" } else { "actif" },
                                i + 1,
                                closed as u8,
                                if background { (active == Some(anchor)) as u8 } else { 1 },
                                tab_count()
                            ));
                            if !closed || !active_ok || !count_ok {
                                return fail(&format!("croix de l'onglet : onglet {target} non fermé ou onglet actif changé ; onglets = {}", windows::tabs_state(1)));
                            }
                            close_trial(side, background, i + 1, done);
                        });
                    });
                });
            });
        });
    });
}

// ---- dropdown popups: no leak, DoClose, race

fn popup_open_cmd(panel: &str) -> Value {
    call("popup.open", json!({ "panel": panel, "anchor": { "x": 600, "y": 40, "width": 30, "height": 30 }, "width": 320, "height": 330 }))
}

fn step_menus() {
    step("menus déroulants : 20 ouvertures-fermetures sans fuite");
    windows::popup_close(1);
    on_ui_delayed(1500, || {
        UI_BASE.store(windows::ui_browsers_alive(), std::sync::atomic::Ordering::SeqCst);
        TABS_BEFORE.store(tab_count(), std::sync::atomic::Ordering::SeqCst);
        menu_cycle(0);
    });
}

fn menu_cycle(i: u32) {
    if i == MENU_CYCLES {
        let base = UI_BASE.load(std::sync::atomic::Ordering::SeqCst);
        poll_ok(50, move || windows::ui_browsers_alive() == base, move |ok| {
            let alive = windows::ui_browsers_alive();
            let window = windows::window_ids().contains(&1);
            let tabs = tab_count();
            state::log(&format!("selftest: menus : {MENU_CYCLES} ouverts puis fermés ; navigateurs d'interface vivants avant={base} après={alive} ; fenêtre ouverte={} ; onglets avant={} après={tabs}", window as u8, TABS_BEFORE.load(std::sync::atomic::Ordering::SeqCst)));
            if !ok || !window || tabs != TABS_BEFORE.load(std::sync::atomic::Ordering::SeqCst) {
                return fail(&format!("menus déroulants : navigateurs non libérés ({base} avant, {alive} après) ou fenêtre/onglets touchés"));
            }
            step_doclose();
        });
        return;
    }
    let panel = if i % 2 == 0 { "menu" } else { "shield" };
    if !expect_ok("ouverture d'un menu", &popup_open_cmd(panel)) {
        return;
    }
    on_ui_delayed(500, move || {
        if windows::popup_panel(1).as_deref() != Some(panel) {
            return fail(&format!("menu {panel} non ouvert (cycle {})", i + 1));
        }
        // Both ways a popup closes: its own request, and NIGHTFALL itself.
        if i % 4 < 2 {
            let _ = call("popup.close", json!({ "panel": panel }));
        } else {
            windows::popup_close(1);
        }
        on_ui_delayed(150, move || menu_cycle(i + 1));
    });
}

/// A close of a popup browser that NIGHTFALL did not start (as CEF would
/// do it): `do_close` must keep the window and only close the popup.
fn step_doclose() {
    step("menu déroulant : fermeture demandée par CEF (DoClose)");
    if !expect_ok("ouverture du menu", &popup_open_cmd("menu")) {
        return;
    }
    on_ui_delayed(700, || {
        let Some(h) = windows::with_window(1, |c| c.popup.as_ref().and_then(|p| p.view.browser())).flatten().and_then(|b| b.host()) else {
            return fail("DoClose : navigateur du menu introuvable");
        };
        h.close_browser(1);
        let base = UI_BASE.load(std::sync::atomic::Ordering::SeqCst);
        poll_ok(40, move || windows::popup_panel(1).is_none() && windows::ui_browsers_alive() == base, move |ok| {
            let window = windows::window_ids().contains(&1);
            let tabs = tab_count();
            state::log(&format!("selftest: DoClose d'un menu : menu fermé={} navigateurs d'interface={} (attendu {base}) fenêtre ouverte={} onglets={tabs}", windows::popup_panel(1).is_none() as u8, windows::ui_browsers_alive(), window as u8));
            if !ok || !window || tabs != TABS_BEFORE.load(std::sync::atomic::Ordering::SeqCst) {
                return fail("DoClose d'un menu : fenêtre fermée, onglets perdus ou navigateur non libéré");
            }
            step_race();
        });
    });
}

fn step_race() {
    step("menus déroulants : un menu puis un autre (clics réels)");
    // Deterministic: the first menu's close request arrives after the
    // second menu was opened (the macOS order).
    windows::popup_close(1);
    let _ = popup_open_cmd("menu");
    let first = windows::with_window(1, |c| c.popup.as_ref().map(|p| p.serial)).flatten().unwrap_or(0);
    let _ = popup_open_cmd("shield");
    let _ = call("popup.close", json!({ "panel": "menu", "n": first }));
    let _ = call("popup.close", json!({ "panel": "menu" }));
    let kept = windows::popup_panel(1);
    state::log(&format!("selftest: fermeture tardive du premier menu (n={first}) : menu affiché = {kept:?}"));
    if kept.as_deref() != Some("shield") {
        return fail("une fermeture tardive du premier menu a fermé le second");
    }
    // Without a name: still closes the open popup (compatibility).
    let _ = call("popup.close", json!({}));
    if windows::popup_panel(1).is_some() {
        return fail("popup.close sans nom n'a pas fermé le menu");
    }
    RACE_OK.store(0, std::sync::atomic::Ordering::SeqCst);
    on_ui_delayed(800, || race_trial(0));
}

/// Real clicks: menu button, then (keyboard focus moved to the toolbar, as
/// a native click does) the Shield button: Shield must stay open.
fn race_trial(i: u32) {
    if i == TRIALS {
        state::log(&format!("selftest: menu puis Shield : Shield resté ouvert {}/{TRIALS}", RACE_OK.load(std::sync::atomic::Ordering::SeqCst)));
        return step_toggle();
    }
    windows::popup_close(1);
    on_ui_delayed(500, move || {
        with_rect(false, "#b-menu".into(), move |x, y| {
            real_click(false, x, y, || {}, move || {
                poll_ok(25, || windows::popup_panel(1).as_deref() == Some("menu"), move |ok| {
                    if !ok {
                        return fail(&format!("clic réel sur le bouton Menu : menu non ouvert (essai {})", i + 1));
                    }
                    on_ui_delayed(700, move || {
                        with_rect(false, "#b-shield".into(), move |x, y| {
                            if let Some(top) = windows::with_window(1, |c| c.top.clone()) {
                                top.request_focus();
                            }
                            real_click(false, x, y, || {}, move || {
                                on_ui_delayed(1500, move || {
                                    let panel = windows::popup_panel(1);
                                    state::log(&format!("selftest: menu puis Shield essai {}/{TRIALS} : menu affiché = {panel:?}", i + 1));
                                    if panel.as_deref() != Some("shield") {
                                        return fail("menu puis Shield : le second menu ne reste pas ouvert");
                                    }
                                    RACE_OK.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                                    race_trial(i + 1);
                                });
                            });
                        });
                    });
                });
            });
        });
    });
}

/// Informative: Shield open, second click on its button closes it.
fn step_toggle() {
    with_rect(false, "#b-shield".into(), |x, y| {
        if let Some(top) = windows::with_window(1, |c| c.top.clone()) {
            top.request_focus();
        }
        real_click(false, x, y, || {}, || {
            on_ui_delayed(1500, || {
                state::log(&format!("selftest: second clic sur Shield (informatif) : menu affiché = {:?}", windows::popup_panel(1)));
                windows::popup_close(1);
                // L0 : then the lock checks (M-0007), which end on the final menu.
                on_ui_delayed(800, step_lock_guard);
            });
        });
    });
}

/// Integrated title bar (prototype M-0005): the frameless window received
/// the draggable regions of the tab strip and, on Windows, the real window
/// procedure answers "caption" there and "client" on a tab, keeps its
/// resize borders, and maximises / restores through the interface buttons.
fn step_titlebar() {
    step("barre de titre intégrée");
    let r = super::titlebar::report(1);
    state::log(&format!("selftest: barre de titre {r}"));
    if !super::titlebar::FRAMELESS {
        return step_measure();
    }
    if r["draggable"].as_u64().unwrap_or(0) == 0 || r["dragPoint"].is_null() || r["noDragPoint"].is_null() {
        return fail(&format!("barre de titre : zones de déplacement absentes {r}"));
    }
    if cfg!(target_os = "windows") {
        let h = &r["hitTest"];
        let ok = h["drag"] == 2 && h["noDrag"] == 1 && h["content"] == 1 && h["leftEdge"] == 10 && h["bottomRight"] == 17 && h["maxButton"] == 9;
        if !ok {
            return fail(&format!("barre de titre : réponses WM_NCHITTEST inattendues {h}"));
        }
    }
    call("win.maximize", json!({}));
    on_ui_delayed(1500, || {
        let r = super::titlebar::report(1);
        state::log(&format!("selftest: barre de titre après agrandissement {r}"));
        if r["maximized"] != true {
            return fail("barre de titre : la fenêtre ne s'est pas agrandie");
        }
        call("win.maximize", json!({}));
        on_ui_delayed(1500, || {
            let r = super::titlebar::report(1);
            state::log(&format!("selftest: barre de titre après restauration {r}"));
            if r["maximized"] != false {
                return fail("barre de titre : la fenêtre n'a pas été restaurée");
            }
            step_measure();
        });
    });
}

/// Measures (L6): start-up, a page load, memory of the whole process tree.
/// Logged as one line « MESURES {json} » read back from the CI logs.
static MEASURE_T0: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);

fn step_measure() {
    step("mesures (démarrage, chargement, mémoire)");
    *MEASURE_T0.lock().unwrap() = Some(std::time::Instant::now());
    let _ = call("tab.new", json!({ "url": "https://example.com/?nf-mesure" }));
    wait_for("chargement mesuré", 60, || active_tab_loaded("https://example.com/?nf-mesure"), || {
        let load_ms = MEASURE_T0.lock().unwrap().map(|t| t.elapsed().as_millis() as u64).unwrap_or(0);
        // Let the renderer settle before reading memory.
        on_ui_delayed(1500, move || {
            let tabs = windows::tabs_state(1)["tabs"].as_array().map(|a| a.len()).unwrap_or(0);
            let (procs, bytes) = nf_perf::process_tree_memory(std::process::id());
            let m = json!({
                "platform": super::titlebar::platform(),
                "firstWindowMs": windows::FIRST_WINDOW_MS.load(std::sync::atomic::Ordering::Relaxed),
                "pageLoadMs": load_ms,
                "tabs": tabs,
                "processes": procs,
                "memoryMiB": bytes / (1024 * 1024),
                "shieldRules": state::shield().rules_loaded(),
            });
            state::log(&format!("MESURES {m}"));
            step_final_menu();
        });
    });
}

fn step_final_menu() {
    step("menu déroulant final");
    // Back to the top of the page (the search scrolled to its match).
    windows::exec_in_tab(1, MAIN_TAB.load(std::sync::atomic::Ordering::SeqCst), "window.scrollTo(0, 0);");
    // Dropdown menu over the page (visible on the CI screenshot).
    windows::popup_open(1, "menu", cef::Rect { x: 1200, y: 44, width: 34, height: 34 }, 300, 420);
    // Checked at once: the menu closes itself when focus moves
    // elsewhere (as intended), which a build machine may do.
    let ok = windows::popup_report(1);
    on_ui_delayed(1200, move || {
        state::log(&format!("selftest: menu déroulant {ok}"));
        // The open menu is cut out of the draggable area (last region).
        state::log(&format!("selftest: barre de titre avec menu ouvert {}", super::titlebar::report(1)));
        if !ok.starts_with("ouvert") {
            return fail("menu déroulant non ouvert");
        }
        DONE.store(true, std::sync::atomic::Ordering::SeqCst);
        state::log("SELFTEST OK");
        finish();
    });
}

// ------------------------------------------- nothing acts while locked

/// Results of `js_check` (key, JSON), filled by `console_result`.
static CHECKS: std::sync::Mutex<Vec<(String, String)>> = std::sync::Mutex::new(Vec::new());
/// Tab playing the test media.
static MEDIA_TAB: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
static DT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(6000);
/// State seen before the shortcuts sent while locked.
static BEFORE: std::sync::Mutex<Option<Snapshot>> = std::sync::Mutex::new(None);
/// Index in lock_guard::SHORTCUTS where the current batch starts.
static SHORTCUTS_MARK: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static VG_FETCHES_BEFORE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Origin and kind of the remembered permission used by the test.
const PERM_ORIGIN: &str = "https://example.com";
const PERM_HOST: &str = "example.com";

#[derive(Clone, Copy, Debug, PartialEq)]
struct Snapshot {
    tabs: usize,
    windows: usize,
    active: Option<u32>,
    zoom: f64,
    fullscreen: bool,
}

fn snapshot() -> Snapshot {
    Snapshot { tabs: tab_count(), windows: windows::window_ids().len(), active: windows::active_tab_id(1), zoom: active_zoom(), fullscreen: windows::fullscreen_report(1).2 }
}

/// Poll `cond` every 500 ms up to `tries` times, then run `next` or fail.
fn poll<C, N>(what: String, tries: u32, cond: C, next: N)
where
    C: Fn() -> bool + Send + 'static,
    N: FnOnce() + Send + 'static,
{
    on_ui_delayed(500, move || {
        if cond() {
            next();
        } else if tries == 0 {
            fail(&format!("délai dépassé : {what}"));
        } else {
            poll(what, tries - 1, cond, next);
        }
    });
}

/// DevTools protocol message to a browser (in process, no debugging port).
fn devtools(b: Option<cef::Browser>, method: &str, params: Value) {
    if let Some(h) = b.and_then(|b| b.host()) {
        let msg = json!({ "id": DT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed), "method": method, "params": params }).to_string();
        h.send_dev_tools_message(Some(msg.as_bytes()));
    }
}

/// Script run with a user gesture (play(), as after a click).
fn eval_with_gesture(tab: u32, expression: &str) {
    devtools(windows::tab_browser(1, tab), "Runtime.evaluate", json!({ "expression": expression, "userGesture": true, "awaitPromise": false }));
}

/// Evaluate `expr` (a value or a promise) in a tab's page and hand the
/// result (JSON) to `next`.
fn js_check(tab: u32, key: &'static str, expr: &str, next: impl FnOnce(Value) + Send + 'static) {
    if let Ok(mut c) = CHECKS.lock() {
        c.retain(|(k, _)| k != key);
    }
    let js = format!(
        "Promise.resolve().then(function(){{ return ({expr}); }}).then(function(r){{ console.log('nfcheck-{key} ' + JSON.stringify(r)); }}, function(e){{ console.log('nfcheck-{key} ' + JSON.stringify({{ error: String(e) }})); }});"
    );
    match windows::tab_browser(1, tab).and_then(|b| b.main_frame()) {
        Some(f) => f.execute_java_script(Some(&cef::CefString::from(js.as_str())), Some(&cef::CefString::from("nightfall://selftest")), 0),
        None => return fail(&format!("contrôle {key} : onglet {tab} introuvable")),
    }
    let found = move || CHECKS.lock().map(|c| c.iter().any(|(k, _)| k == key)).unwrap_or(false);
    poll(format!("réponse de la page au contrôle {key}"), 20, found, move || {
        let raw = CHECKS.lock().ok().and_then(|c| c.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone())).unwrap_or_default();
        state::log(&format!("selftest: contrôle {key} : {raw}"));
        next(serde_json::from_str(&raw).unwrap_or(Value::Null));
    });
}

const K_P: u32 = 1; // Ctrl (Windows) / Cmd (macOS)
const K_SH: u32 = 2;
const K_ALT: u32 = 4;
const K_CTRL: u32 = 8;

/// Key press sent to the top interface view of window 1 (the lock screen
/// while locked), by the DevTools protocol: it goes through
/// `on_pre_key_event` like a key typed by the user.
fn key(code: i32, mods: u32) {
    let mac = cfg!(target_os = "macos");
    let mut m = 0;
    if mods & K_P != 0 {
        m |= if mac { 4 } else { 2 };
    }
    if mods & K_SH != 0 {
        m |= 8;
    }
    if mods & K_ALT != 0 {
        m |= 1;
    }
    if mods & K_CTRL != 0 {
        m |= 2;
    }
    let top = windows::with_window(1, |c| c.top.browser()).flatten();
    for t in ["rawKeyDown", "keyUp"] {
        devtools(top.clone(), "Input.dispatchKeyEvent", json!({ "type": t, "modifiers": m, "windowsVirtualKeyCode": code, "nativeVirtualKeyCode": code }));
    }
}

/// (action expected, key code, modifiers)
fn lock_shortcuts() -> Vec<(&'static str, i32, u32)> {
    let mac = cfg!(target_os = "macos");
    vec![
        ("tab.new", 0x54, K_P),
        ("tab.close", 0x57, K_P),
        ("tab.reopen", 0x54, K_P | K_SH),
        ("window.new", 0x4E, K_P),
        ("zoom.in", 0xBB, K_P),
        ("window.fullscreen", if mac { 0x46 } else { 0x7A }, if mac { K_P | K_CTRL } else { 0 }),
        ("print", 0x50, K_P),
        ("devtools", if mac { 0x49 } else { 0x7B }, if mac { K_P | K_ALT } else { 0 }),
        ("view.source", 0x55, K_P),
        ("page.history", if mac { 0x59 } else { 0x48 }, K_P),
    ]
}

fn mark_shortcuts() {
    let n = super::lock_guard::SHORTCUTS.lock().map(|s| s.len()).unwrap_or(0);
    SHORTCUTS_MARK.store(n, std::sync::atomic::Ordering::SeqCst);
}

fn shortcuts_since_mark() -> Vec<(String, bool)> {
    let from = SHORTCUTS_MARK.load(std::sync::atomic::Ordering::SeqCst);
    super::lock_guard::SHORTCUTS.lock().map(|s| s.iter().skip(from).cloned().collect()).unwrap_or_default()
}

fn shortcut_done(action: &'static str) -> impl Fn() -> bool + Send + 'static {
    move || shortcuts_since_mark().iter().any(|(a, ok)| a == action && *ok)
}

/// Content setting categories of Chromium's `Preferences` file that hold
/// an exception for `host`.
fn chromium_prefs_categories(host: &str) -> (String, Vec<String>) {
    let base = core().paths.chromium_profile();
    let file = [base.join("Default").join("Preferences"), base.join("Preferences")].into_iter().find(|p| p.exists());
    let Some(file) = file else { return ("(fichier Preferences absent)".into(), Vec::new()) };
    let name = file.display().to_string();
    let v: Value = std::fs::read(&file).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or(Value::Null);
    let mut out = Vec::new();
    if let Some(ex) = v["profile"]["content_settings"]["exceptions"].as_object() {
        for (cat, entries) in ex {
            if entries.as_object().map(|o| o.keys().any(|k| k.contains(host))).unwrap_or(false) {
                out.push(cat.clone());
            }
        }
    }
    (name, out)
}

fn prefs_has_decision(host: &'static str) -> bool {
    chromium_prefs_categories(host).1.iter().any(|c| c.starts_with("clipboard"))
}

fn step_lock_guard() {
    step("verrouillage : page qui joue un média");
    let page = "data:text/html,<title>nf-media</title><body>media</body>";
    let Some(tab) = windows::add_tab(1, page, true) else { return fail("onglet média non ouvert") };
    MEDIA_TAB.store(tab, std::sync::atomic::Ordering::SeqCst);
    wait_for("chargement de la page média", 30, || active_tab_loaded("data:text/html"), || {
        let tab = MEDIA_TAB.load(std::sync::atomic::Ordering::SeqCst);
        // A 2 s looping tone (WAV made in the page), a video element inside
        // a YouTube-like player for VideoGuard's video ad fast-forward.
        let setup = "(function(){var sr=8000,n=sr*2,b=new ArrayBuffer(44+n*2),d=new DataView(b);\
            function w(o,s){for(var i=0;i<s.length;i++)d.setUint8(o+i,s.charCodeAt(i));}\
            w(0,'RIFF');d.setUint32(4,36+n*2,true);w(8,'WAVE');w(12,'fmt ');d.setUint32(16,16,true);d.setUint16(20,1,true);d.setUint16(22,1,true);\
            d.setUint32(24,sr,true);d.setUint32(28,sr*2,true);d.setUint16(32,2,true);d.setUint16(34,16,true);w(36,'data');d.setUint32(40,n*2,true);\
            for(var i=0;i<n;i++)d.setInt16(44+i*2,Math.round(Math.sin(i*2*Math.PI*440/sr)*6000),true);\
            var a=document.createElement('audio');a.id='nf-a';a.loop=true;a.src=URL.createObjectURL(new Blob([b],{type:'audio/wav'}));document.body.appendChild(a);\
            var p=document.createElement('div');p.id='movie_player';p.className='html5-video-player';var v=document.createElement('video');v.id='nf-v';p.appendChild(v);document.body.appendChild(p);\
            return 1;})()";
        js_check(tab, "media-setup", setup, move |_| {
            eval_with_gesture(tab, "document.getElementById('nf-a').play()");
            // VideoGuard's page script, as on YouTube.
            if let Some(js) = nf_videoguard::page_script("https://www.youtube.com/", &nf_videoguard::VideoGuardConfig::default()) {
                windows::exec_in_tab(1, tab, &js);
            }
            on_ui_delayed(1500, move || {
                js_check(tab, "media-before", MEDIA_STATE, move |r| {
                    if r["playing"] != true || r["vg"] != true {
                        return fail(&format!("média de test non lancé avant le verrouillage : {r}"));
                    }
                    step_lock_guard_permission();
                });
            });
        });
    });
}

/// State of the test page: audio playing, VideoGuard present, video rate.
const MEDIA_STATE: &str = "(function(){var a=document.getElementById('nf-a'),v=document.getElementById('nf-v');return {playing:!a.paused,time:a.currentTime,rate:v.playbackRate,videoMuted:v.muted,vg:!!window.__nightfallVideoGuardSuspend};})()";

// Clipboard rather than geolocation: geolocation also depends on the
// operating system (denied on the macOS CI runner), the clipboard does not.
const PERM_QUERY: &str = "navigator.permissions.query({name:'clipboard-read'}).then(function(s){return s.state;})";

/// Answer of `navigator.permissions.query` before any decision.
static PERM_INITIAL: std::sync::Mutex<Option<Value>> = std::sync::Mutex::new(None);

fn step_lock_guard_permission() {
    step("verrouillage : décision d'autorisation mémorisée");
    let main = MAIN_TAB.load(std::sync::atomic::Ordering::SeqCst);
    js_check(main, "perm-initial", PERM_QUERY, |r| {
        if r == "granted" {
            return fail(&format!("autorisation déjà accordée avant le test : {r}"));
        }
        if let Ok(mut p) = PERM_INITIAL.lock() {
            *p = Some(r);
        }
        step_lock_guard_remember();
    });
}

fn step_lock_guard_remember() {
    {
        let mut c = core();
        super::permissions::remember(&mut c.settings, PERM_ORIGIN, &["clipboard"], true, nf_core::unix_now());
        c.save_settings();
    }
    // As at unlock: the remembered decision is given to Chromium.
    super::permissions::reapply_remembered();
    let main = MAIN_TAB.load(std::sync::atomic::Ordering::SeqCst);
    js_check(main, "perm-unlocked", PERM_QUERY, |r| {
        if r != "granted" {
            return fail(&format!("autorisation mémorisée non appliquée par Chromium : {r}"));
        }
        // Chromium writes its preferences about 10 s after a change.
        poll("décision écrite par Chromium dans Preferences".into(), 60, || prefs_has_decision(PERM_HOST), || {
            let (file, cats) = chromium_prefs_categories(PERM_HOST);
            state::log(&format!("selftest: Preferences ({file}) avant verrouillage, catégories pour {PERM_HOST} : {cats:?}"));
            step_lock_guard_lock();
        });
    });
}

fn step_lock_guard_lock() {
    step("verrouillage : raccourcis, médias, VideoGuard, autorisations");
    if let Ok(mut b) = BEFORE.lock() {
        *b = Some(snapshot());
    }
    VG_FETCHES_BEFORE.store(super::lock_guard::VG_FETCHES.load(std::sync::atomic::Ordering::SeqCst), std::sync::atomic::Ordering::SeqCst);
    api::lock_now();
    let locked = || {
        let keys_forgotten = {
            let c = core();
            !c.is_unlocked() && c.unlocked.is_none()
        };
        keys_forgotten && !super::lock_guard::is_locking()
    };
    poll("verrouillage complet".into(), 20, locked, || {
        on_ui_delayed(1000, || {
            mark_shortcuts();
            for (_, code, mods) in lock_shortcuts() {
                key(code, mods);
            }
            on_ui_delayed(2500, step_lock_guard_check_shortcuts);
        });
    });
}

fn step_lock_guard_check_shortcuts() {
    let seen = shortcuts_since_mark();
    state::log(&format!("selftest: raccourcis reçus pendant le verrouillage : {seen:?}"));
    for (action, _, _) in lock_shortcuts() {
        if !seen.iter().any(|(a, ok)| a == action && !*ok) {
            return fail(&format!("raccourci {action} : non reçu par NIGHTFALL pendant le verrouillage (test sans valeur)"));
        }
    }
    if seen.iter().any(|(_, ok)| *ok) {
        return fail(&format!("raccourci exécuté pendant le verrouillage : {seen:?}"));
    }
    let before = BEFORE.lock().ok().and_then(|b| *b);
    let after = snapshot();
    state::log(&format!("selftest: état avant {before:?} / après {after:?}"));
    if before != Some(after) {
        return fail(&format!("état changé pendant le verrouillage : avant {before:?}, après {after:?}"));
    }
    state::log("selftest: raccourcis sans effet pendant le verrouillage ok");
    let tab = MEDIA_TAB.load(std::sync::atomic::Ordering::SeqCst);
    if !windows::tab_muted(1, tab) {
        return fail("onglet média non coupé pendant le verrouillage");
    }
    js_check(tab, "media-locked", MEDIA_STATE, move |r| {
        if r["playing"] != false {
            return fail(&format!("média toujours en lecture pendant le verrouillage : {r}"));
        }
        // The page tries to play again (as after a click), and a video ad
        // shows in the player: nothing must happen.
        eval_with_gesture(tab, "document.getElementById('nf-a').play()");
        windows::exec_in_tab(1, tab, "document.getElementById('movie_player').classList.add('ad-showing');");
        // VideoGuard asked for a YouTube video in a hidden tab.
        super::client::videoguard_on_navigation(1, tab, "https://www.youtube.com/watch?v=dQw4w9WgXcQ");
        on_ui_delayed(2000, move || {
            js_check(tab, "media-locked-later", MEDIA_STATE, move |r| {
                if r["playing"] != false {
                    return fail(&format!("média relancé pendant le verrouillage : {r}"));
                }
                if r["rate"] != 1.0 || r["videoMuted"] != false {
                    return fail(&format!("VideoGuard a agi pendant le verrouillage : {r}"));
                }
                let fetches = super::lock_guard::VG_FETCHES.load(std::sync::atomic::Ordering::SeqCst);
                let deferred = super::lock_guard::VG_DEFERRED.load(std::sync::atomic::Ordering::SeqCst);
                state::log(&format!("selftest: VideoGuard pendant le verrouillage : requêtes {fetches} (avant {}), reportées {deferred}", VG_FETCHES_BEFORE.load(std::sync::atomic::Ordering::SeqCst)));
                if fetches != VG_FETCHES_BEFORE.load(std::sync::atomic::Ordering::SeqCst) || deferred == 0 {
                    return fail("VideoGuard : requête envoyée (ou non vue) pendant le verrouillage");
                }
                state::log("selftest: son coupé, médias en pause, VideoGuard inactif pendant le verrouillage ok");
                let main = MAIN_TAB.load(std::sync::atomic::Ordering::SeqCst);
                js_check(main, "perm-locked", PERM_QUERY, |r| {
                    let initial = PERM_INITIAL.lock().ok().and_then(|p| p.clone()).unwrap_or(Value::Null);
                    if r != initial {
                        return fail(&format!("autorisation mémorisée encore connue de Chromium pendant le verrouillage : {r}"));
                    }
                    poll("décision retirée du fichier Preferences".into(), 60, || !prefs_has_decision(PERM_HOST), || {
                        let (file, cats) = chromium_prefs_categories(PERM_HOST);
                        state::log(&format!("selftest: Preferences ({file}) pendant le verrouillage, catégories pour {PERM_HOST} : {cats:?}"));
                        step_lock_guard_unlock();
                    });
                });
            });
        });
    });
}

fn step_lock_guard_unlock() {
    step("déverrouillage : son rétabli, médias en pause, autorisation rendue");
    if !expect_ok("déverrouillage", &call("unlock", json!({ "pin": PIN }))) {
        return;
    }
    let tab = MEDIA_TAB.load(std::sync::atomic::Ordering::SeqCst);
    poll("son rétabli après le déverrouillage".into(), 20, move || !windows::tab_muted(1, tab), move || {
        on_ui_delayed(1500, move || {
            js_check(tab, "media-unlocked", MEDIA_STATE, move |r| {
                if r["playing"] != false {
                    return fail(&format!("média relancé au déverrouillage : {r}"));
                }
                // VideoGuard acts again (the ad shown in the player).
                if r["rate"] != 16.0 {
                    return fail(&format!("VideoGuard non repris après le déverrouillage : {r}"));
                }
                windows::exec_in_tab(1, tab, "document.getElementById('movie_player').classList.remove('ad-showing');");
                let main = MAIN_TAB.load(std::sync::atomic::Ordering::SeqCst);
                js_check(main, "perm-after", PERM_QUERY, |r| {
                    if r != "granted" {
                        return fail(&format!("autorisation mémorisée non rendue à Chromium au déverrouillage : {r}"));
                    }
                    state::log("selftest: déverrouillage : son rétabli, média en pause, VideoGuard repris, autorisation rendue ok");
                    step_unlocked_shortcuts();
                });
            });
        });
    });
}

fn step_unlocked_shortcuts() {
    step("raccourcis après le déverrouillage");
    let before = snapshot();
    mark_shortcuts();
    key(0x54, K_P);
    let n = before.tabs;
    poll("Ctrl/Cmd+T déverrouillé".into(), 20, move || tab_count() == n + 1, move || {
        key(0x57, K_P);
        poll("Ctrl/Cmd+W déverrouillé".into(), 20, move || tab_count() == n, move || {
            windows::activate_tab(1, MAIN_TAB.load(std::sync::atomic::Ordering::SeqCst));
            key(0xBB, K_P);
            poll("Ctrl/Cmd+= déverrouillé".into(), 20, || active_zoom() > 0.1, || {
                key(0x30, K_P);
                poll("Ctrl/Cmd+0 déverrouillé".into(), 20, || active_zoom().abs() < 0.01 && !saved_zoom_for_example(), || {
                    let w = windows::window_ids().len();
                    key(0x4E, K_P);
                    poll("Ctrl/Cmd+N déverrouillé".into(), 20, move || windows::window_ids().len() == w + 1, move || {
                        on_ui_delayed(1500, move || {
                            for id in windows::window_ids() {
                                if id != 1 {
                                    windows::close_window(id);
                                }
                            }
                            poll("fermeture de la fenêtre ouverte par Ctrl/Cmd+N".into(), 20, move || windows::window_ids().len() == w, step_unlocked_fullscreen);
                        });
                    });
                });
            });
        });
    });
}

fn fullscreen_key() {
    if cfg!(target_os = "macos") {
        key(0x46, K_P | K_CTRL);
    } else {
        key(0x7A, 0);
    }
}

fn step_unlocked_fullscreen() {
    fullscreen_key();
    poll("plein écran par raccourci déverrouillé".into(), 20, || windows::fullscreen_report(1).2, || {
        on_ui_delayed(1000, || {
            fullscreen_key();
            poll("sortie du plein écran par raccourci".into(), 20, || !windows::fullscreen_report(1).2, || {
                // Developer tools are off by default: the shortcut reaches
                // NIGHTFALL, which opens nothing. Printing is not sent here
                // (it would open the system print dialog).
                key(if cfg!(target_os = "macos") { 0x49 } else { 0x7B }, if cfg!(target_os = "macos") { K_P | K_ALT } else { 0 });
                poll("raccourci des outils de développement reçu".into(), 10, shortcut_done("devtools"), || {
                    let seen = shortcuts_since_mark();
                    state::log(&format!("selftest: raccourcis après le déverrouillage : {seen:?}"));
                    for a in ["tab.new", "tab.close", "zoom.in", "zoom.reset", "window.new", "window.fullscreen", "devtools"] {
                        if !seen.iter().any(|(x, ok)| x == a && *ok) {
                            return fail(&format!("raccourci {a} non exécuté après le déverrouillage"));
                        }
                    }
                    state::log("selftest: raccourcis actifs après le déverrouillage ok");
                    step_lock_guard_cleanup();
                });
            });
        });
    });
}

fn step_lock_guard_cleanup() {
    let tab = MEDIA_TAB.load(std::sync::atomic::Ordering::SeqCst);
    windows::close_tab(1, tab);
    {
        let mut c = core();
        c.settings.site_permissions.retain(|p| p.origin != PERM_ORIGIN);
        c.save_settings();
    }
    super::permissions::reset_chromium(PERM_ORIGIN, "clipboard");
    windows::activate_tab(1, MAIN_TAB.load(std::sync::atomic::Ordering::SeqCst));
    // Then the integrated title bar, which ends on the final menu.
    on_ui_delayed(1000, step_titlebar);
}

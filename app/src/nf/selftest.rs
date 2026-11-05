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
static DT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(950_000);
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
    let msg = json!({ "id": DT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed), "method": "Input.dispatchMouseEvent",
        "params": { "type": kind, "x": x, "y": y, "button": button, "buttons": buttons, "clickCount": clicks } }).to_string();
    if let Some(h) = ui_browser(side).and_then(|b| b.host()) {
        h.send_dev_tools_message(Some(msg.as_bytes()));
    }
}

/// Poll `cond` every 200 ms up to `tries` times, then `next(met)`.
fn poll<C, N>(tries: u32, cond: C, next: N)
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
            poll(tries - 1, cond, next);
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
    poll(25, || RECT.lock().map(|r| r.is_some()).unwrap_or(false), move |_| {
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
            poll(75, || active_tab_loaded("nightfall://app/newtab"), |ok| {
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
        poll(50, move || windows::ui_browsers_alive() == base, move |ok| {
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
        poll(40, move || windows::popup_panel(1).is_none() && windows::ui_browsers_alive() == base, move |ok| {
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
                poll(25, || windows::popup_panel(1).as_deref() == Some("menu"), move |ok| {
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
                on_ui_delayed(800, step_final_menu);
            });
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
        if !ok.starts_with("ouvert") {
            return fail("menu déroulant non ouvert");
        }
        DONE.store(true, std::sync::atomic::Ordering::SeqCst);
        state::log("SELFTEST OK");
        finish();
    });
}

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
    on_ui_delayed(500, || {
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
        std::thread::sleep(std::time::Duration::from_secs(200));
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
    DONE.store(true, std::sync::atomic::Ordering::SeqCst);
    state::log("SELFTEST OK");
    finish();
}

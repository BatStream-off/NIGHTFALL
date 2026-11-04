//! Settings applied through Chromium itself: profile preferences (fonts,
//! cookies, WebRTC, languages, spell checking), "local state" preferences
//! (DNS over HTTPS), permission defaults, and the default page zoom.
//!
//! Applied on the UI thread at start-up (`on_context_initialized`), after
//! unlock (the settings live in the encrypted database) and whenever the
//! settings change. Every preference Chromium refuses is logged and shown
//! in the settings page: nothing is claimed that Chromium did not accept.

use super::state::{self, core, Settings};
use cef::*;
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

/// Read on the IO thread for every request (no lock of the core state).
pub static GPC: AtomicBool = AtomicBool::new(true);
pub static HTTPS_FIRST: AtomicBool = AtomicBool::new(true);
/// GPU acceleration in effect for this run (read before Chromium starts).
pub static HW_ACCEL_AT_START: AtomicBool = AtomicBool::new(true);

/// Spell checking without any download: macOS uses the system spell
/// checker. On Windows, Chromium downloads Hunspell dictionaries from
/// Google's servers for languages Windows does not cover, so it stays off.
pub const SPELLCHECK_AVAILABLE: bool = cfg!(target_os = "macos");

#[derive(Debug, Clone, PartialEq)]
pub enum PrefValue {
    Bool(bool),
    Int(i32),
    Str(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Pref {
    pub name: &'static str,
    pub label: &'static str,
    pub value: PrefValue,
}

#[derive(Debug, Clone, Serialize)]
pub struct Outcome {
    pub name: String,
    pub label: String,
    pub ok: bool,
    pub error: String,
}

static REPORT: Mutex<Vec<Outcome>> = Mutex::new(Vec::new());

pub fn last_report() -> Vec<Outcome> {
    REPORT.lock().map(|r| r.clone()).unwrap_or_default()
}

/// Chromium zoom level for a percentage (factor = 1.2 ^ level).
pub fn zoom_level(percent: u32) -> f64 {
    let p = percent.clamp(25, 500) as f64 / 100.0;
    if (p - 1.0).abs() < 1e-9 {
        0.0
    } else {
        p.ln() / 1.2f64.ln()
    }
}

/// Profile preferences for these settings.
pub fn profile_prefs(s: &Settings) -> Vec<Pref> {
    let p = |name, label, value| Pref { name, label, value };
    let size = s.font_size.clamp(6, 72) as i32;
    vec![
        p("webkit.webprefs.default_font_size", "Taille de police", PrefValue::Int(size)),
        p("webkit.webprefs.default_fixed_font_size", "Taille de police (chasse fixe)", PrefValue::Int((size - 3).max(6))),
        // 2 = block third-party cookies, 0 = allow them (1 = only in private windows).
        p("profile.cookie_controls_mode", "Cookies tiers", PrefValue::Int(if s.block_third_party_cookies { 2 } else { 0 })),
        p(
            "webrtc.ip_handling_policy",
            "Protection WebRTC",
            PrefValue::Str(if s.webrtc_ip_protection { "default_public_interface_only" } else { "default" }.into()),
        ),
        p("intl.accept_languages", "Langues des pages", PrefValue::Str(s.accept_languages.clone())),
        p("browser.enable_spellchecking", "Correcteur orthographique", PrefValue::Bool(SPELLCHECK_AVAILABLE && s.spellcheck)),
        // Never Google's online spelling service.
        p("spellcheck.use_spelling_service", "Service de correction en ligne", PrefValue::Bool(false)),
    ]
}

/// "Local state" (browser-wide) preferences: DNS over HTTPS.
pub fn local_state_prefs(s: &Settings) -> Vec<Pref> {
    let (mode, templates) = s.doh_config();
    vec![
        Pref { name: "dns_over_https.mode", label: "DNS sécurisé (mode)", value: PrefValue::Str(mode.into()) },
        Pref { name: "dns_over_https.templates", label: "DNS sécurisé (fournisseur)", value: PrefValue::Str(templates) },
    ]
}

fn to_cef(v: &PrefValue) -> Option<Value> {
    let out = value_create()?;
    match v {
        PrefValue::Bool(b) => out.set_bool(*b as i32),
        PrefValue::Int(i) => out.set_int(*i),
        PrefValue::Str(s) => out.set_string(Some(&CefString::from(s.as_str()))),
    };
    Some(out)
}

/// Set one preference; Err(reason) when Chromium refuses it.
fn set_one(m: &impl ImplPreferenceManager, p: &Pref) -> Result<(), String> {
    let name = CefString::from(p.name);
    if m.has_preference(Some(&name)) == 0 {
        return Err("préférence inconnue de cette version de Chromium".into());
    }
    if m.can_set_preference(Some(&name)) == 0 {
        return Err("préférence verrouillée (stratégie ou ligne de commande)".into());
    }
    let mut value = to_cef(&p.value).ok_or("valeur impossible à créer")?;
    // A real (non-null) string: CEF writes the reason of a refusal into it.
    let mut error = CefString::from("-");
    if m.set_preference(Some(&name), Some(&mut value), Some(&mut error)) == 0 {
        let e = error.to_string();
        return Err(if e.is_empty() || e == "-" { "refusée par Chromium".into() } else { e });
    }
    Ok(())
}

fn apply_list(m: &impl ImplPreferenceManager, prefs: &[Pref], scope: &str, out: &mut Vec<Outcome>) {
    for p in prefs {
        let r = set_one(m, p);
        if let Err(e) = &r {
            state::log(&format!("préférence Chromium {} ({scope}) : {e}", p.name));
        }
        out.push(Outcome { name: p.name.into(), label: p.label.into(), ok: r.is_ok(), error: r.err().unwrap_or_default() });
    }
}

/// Default zoom: (level wanted now, level wanted before the last change).
static ZOOM: Mutex<(f64, f64)> = Mutex::new((0.0, 0.0));

/// Fast flags read on the IO thread.
pub fn update_flags(s: &Settings) {
    GPC.store(s.global_privacy_control, Ordering::Relaxed);
    HTTPS_FIRST.store(s.https_first, Ordering::Relaxed);
    let want = zoom_level(s.default_zoom);
    if let Ok(mut z) = ZOOM.lock() {
        if (z.0 - want).abs() > 1e-9 {
            *z = (want, z.0);
        }
    }
}

/// Zoom to give a page whose zoom is `current`: the default zoom when the
/// page has none of its own (Chromium's 100 %, or the previous default),
/// None when the page keeps its zoom (chosen by the user for this site).
pub fn zoom_for(current: f64, want: f64, previous: f64) -> Option<f64> {
    let same = |a: f64, b: f64| (a - b).abs() < 1e-6;
    if same(current, want) {
        return None;
    }
    (same(current, 0.0) || same(current, previous)).then_some(want)
}

/// Default zoom level of the settings (0 = 100 %).
pub fn default_zoom_level() -> f64 {
    ZOOM.lock().map(|z| z.0).unwrap_or(0.0)
}

/// Apply the default zoom to a tab that starts showing a page (UI thread).
/// NIGHTFALL's own pages stay at 100 % (they share the toolbar's zoom).
/// (Chromium's `partition.default_zoom_level` preference is accepted but
/// not applied to Alloy-style browsers, as the CI self test showed.)
pub fn apply_default_zoom(browser: &Browser, url: &str) {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return;
    }
    let (want, previous) = ZOOM.lock().map(|z| *z).unwrap_or((0.0, 0.0));
    let Some(host) = browser.host() else { return };
    if let Some(z) = zoom_for(host.zoom_level(), want, previous) {
        host.set_zoom_level(z);
    }
}

/// Apply every Chromium-side setting (UI thread). Returns the report.
pub fn apply_all() -> Vec<Outcome> {
    let s = core().settings.clone();
    update_flags(&s);
    let mut out = Vec::new();
    match request_context_get_global_context() {
        Some(rc) => apply_list(&rc, &profile_prefs(&s), "profil", &mut out),
        None => state::log("préférences Chromium : contexte global indisponible"),
    }
    match preference_manager_get_global() {
        Some(m) => apply_list(&m, &local_state_prefs(&s), "global", &mut out),
        None => state::log("préférences Chromium : gestionnaire global indisponible"),
    }
    super::permissions::apply_defaults(&s);
    let failed = out.iter().filter(|o| !o.ok).count();
    state::log(&format!("préférences Chromium appliquées ({} acceptées, {failed} refusées)", out.len() - failed));
    if let Ok(mut r) = REPORT.lock() {
        *r = out.clone();
    }
    out
}

wrap_request_context_handler! {
    struct PrivateContext;

    impl RequestContextHandler {
        fn on_request_context_initialized(&self, request_context: Option<&mut RequestContext>) {
            // Profile preferences of a private window's own context.
            if let Some(rc) = request_context {
                let s = core().settings.clone();
                let mut out = Vec::new();
                apply_list(&*rc, &profile_prefs(&s), "privé", &mut out);
            }
        }
    }
}

pub fn private_context_handler() -> RequestContextHandler {
    PrivateContext::new()
}

/// Read a preference back (self test and diagnosis).
#[cfg_attr(not(feature = "selftest"), allow(dead_code))]
pub fn read_back(name: &str, global: bool) -> Option<Value> {
    let n = CefString::from(name);
    if global {
        preference_manager_get_global()?.preference(Some(&n))
    } else {
        request_context_get_global_context()?.preference(Some(&n))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_zoom_respects_site_zoom() {
        let l125 = zoom_level(125);
        let l150 = zoom_level(150);
        // A page at Chromium's 100 % gets the default zoom.
        assert_eq!(zoom_for(0.0, l125, 0.0), Some(l125));
        assert_eq!(zoom_for(l125, l125, 0.0), None);
        // The user zoomed this site: kept.
        assert_eq!(zoom_for(l150, l125, 0.0), None);
        // Default changed from 125 % to 100 %: pages at the old default follow.
        assert_eq!(zoom_for(l125, 0.0, l125), Some(0.0));
        assert_eq!(zoom_for(l150, 0.0, l125), None);
    }

    #[test]
    fn zoom_levels() {
        assert_eq!(zoom_level(100), 0.0);
        assert!((zoom_level(120) - 1.0).abs() < 1e-9);
        assert!((zoom_level(125) - 1.2239).abs() < 1e-3);
        assert!(zoom_level(80) < 0.0);
    }

    #[test]
    fn preferences_follow_settings() {
        let mut s = Settings::default();
        let find = |prefs: &[Pref], n: &str| prefs.iter().find(|p| p.name == n).map(|p| p.value.clone());
        let p = profile_prefs(&s);
        assert_eq!(find(&p, "webkit.webprefs.default_font_size"), Some(PrefValue::Int(16)));
        assert_eq!(find(&p, "webkit.webprefs.default_fixed_font_size"), Some(PrefValue::Int(13)));
        assert_eq!(find(&p, "profile.cookie_controls_mode"), Some(PrefValue::Int(2)));
        assert_eq!(find(&p, "webrtc.ip_handling_policy"), Some(PrefValue::Str("default_public_interface_only".into())));
        assert_eq!(find(&p, "intl.accept_languages"), Some(PrefValue::Str("fr-FR,fr,en-US,en".into())));
        assert_eq!(find(&p, "spellcheck.use_spelling_service"), Some(PrefValue::Bool(false)));
        assert_eq!(find(&p, "browser.enable_spellchecking"), Some(PrefValue::Bool(SPELLCHECK_AVAILABLE)));
        s.font_size = 20;
        s.default_zoom = 125;
        s.block_third_party_cookies = false;
        s.webrtc_ip_protection = false;
        s.spellcheck = false;
        let p = profile_prefs(&s);
        assert_eq!(find(&p, "webkit.webprefs.default_font_size"), Some(PrefValue::Int(20)));
        assert_eq!(find(&p, "profile.cookie_controls_mode"), Some(PrefValue::Int(0)));
        assert_eq!(find(&p, "webrtc.ip_handling_policy"), Some(PrefValue::Str("default".into())));
        assert_eq!(find(&p, "browser.enable_spellchecking"), Some(PrefValue::Bool(false)));
        s.secure_dns = "mullvad".into();
        let l = local_state_prefs(&s);
        assert_eq!(find(&l, "dns_over_https.mode"), Some(PrefValue::Str("secure".into())));
        assert_eq!(find(&l, "dns_over_https.templates"), Some(PrefValue::Str("https://dns.mullvad.net/dns-query".into())));
    }
}

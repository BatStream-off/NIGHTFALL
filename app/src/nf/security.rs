//! Browser-side glue for the Security Core.

use super::state::{self, core};
use nf_security::phishing::PhishingDetector;
use std::sync::{Mutex, OnceLock};

static DETECTOR: OnceLock<Mutex<Option<PhishingDetector>>> = OnceLock::new();

fn slot() -> std::sync::MutexGuard<'static, Option<PhishingDetector>> {
    let m = DETECTOR.get_or_init(|| Mutex::new(None));
    match m.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    }
}

/// Rebuild the protected-domain list (after unlock, bookmark or vault change).
pub fn invalidate_detector() {
    *slot() = None;
}

/// Returns a French warning when the URL looks like phishing.
pub fn phishing_check(url: &str) -> Option<String> {
    let mut guard = slot();
    if guard.is_none() {
        let mut domains: Vec<String> = Vec::new();
        {
            let c = core();
            if let Some(s) = c.storage() {
                if let Ok(bms) = s.bookmarks() {
                    domains.extend(bms.into_iter().map(|b| b.url));
                }
            }
            if let Some(v) = c.vault() {
                domains.extend(v.origins());
            }
        }
        *guard = Some(PhishingDetector::new(domains));
    }
    let verdict = guard.as_ref()?.check(url, false)?;
    verdict.is_suspicious().then(|| verdict.message())
}

/// Security signals detected at startup (forbidden switches...).
pub fn startup_signals(args: &[String]) -> Vec<nf_emergency::Signal> {
    let mut out = Vec::new();
    // Only the browser process is checked: sub-processes receive switches
    // from Chromium itself.
    if args.iter().any(|a| a.starts_with("--type=")) {
        return out;
    }
    for name in nf_security::switches::forbidden_in(args.iter().map(String::as_str)) {
        out.push(nf_emergency::Signal::ForbiddenSwitch(name));
    }
    out
}

/// Status of every protection, for the Security Center.
pub fn status_json() -> serde_json::Value {
    let c = core();
    let shield = state::shield();
    let sh_cfg = shield.config();
    let stats = shield.stats();
    let backend = c.lock.as_ref().map(|l| l.backend().describe()).unwrap_or("non configuré");
    let hardware = c.lock.as_ref().map(|l| l.backend().is_hardware_backed()).unwrap_or(false);
    serde_json::json!({
        "lock": {
            "configured": c.lock.is_some(),
            "backend": backend,
            "hardwareBacked": hardware,
            "autoLockMinutes": c.settings.auto_lock_minutes,
            "lockOnSleep": c.settings.lock_on_sleep,
        },
        "shield": {
            "enabled": sh_cfg.enabled,
            "blockPopups": sh_cfg.block_popups,
            "rules": shield.rules_loaded(),
            "integrityErrors": shield.integrity_errors(),
            "listsCached": shield.has_cached_lists(),
            "lists": sh_cfg.lists,
            "allowlist": sh_cfg.allowlist,
            "customFilters": sh_cfg.custom_filters,
            "stats": stats,
        },
        "videoguard": {
            "config": c.videoguard.with_all_categories(),
            "categories": nf_videoguard::CATEGORIES,
            "attribution": nf_videoguard::ATTRIBUTION,
        },
        "sessionVault": c.settings.session_vault,
        "phishingWarnings": c.settings.phishing_warnings,
        "autoPurge": c.settings.auto_purge,
        "signals": c.signals.iter().map(|s| s.describe()).collect::<Vec<_>>(),
        "chromium": {
            "sandbox": cfg!(feature = "sandbox"),
            "siteIsolation": true,
            "remoteDebugging": false,
        },
    })
}

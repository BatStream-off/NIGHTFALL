//! Auto-Purge decision rules.
//!
//! Guarantees (tested below):
//! - Auto-Purge is disabled by default.
//! - No signal, or only non-critical signals: nothing or a lockdown.
//! - A single critical signal: lockdown only, never destruction.
//! - Destruction needs at least `min_independent_critical` (>= 2) distinct
//!   critical signals AND a rule enabled by the user beforehand.
//! - Web pages and extensions cannot emit signals: signals are produced only
//!   by the browser core.

use crate::purge::PurgeCategory;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "snake_case", tag = "kind", content = "detail")]
pub enum Signal {
    /// NIGHTFALL's own executable failed its signature check.
    BinarySignatureInvalid,
    /// Key store or lock counter failed authentication (tampering).
    KeystoreTampered,
    /// The encrypted database could not be authenticated.
    DatabaseTampered,
    /// Launched with a forbidden switch (remote debugging, no sandbox...).
    ForbiddenSwitch(String),
    /// An unsigned module was found loaded in the process (Windows).
    InjectedModule(String),
    /// Consecutive wrong PIN attempts.
    PinFailures(u32),
}

impl Signal {
    pub fn is_critical(&self) -> bool {
        match self {
            Signal::BinarySignatureInvalid
            | Signal::KeystoreTampered
            | Signal::DatabaseTampered
            | Signal::InjectedModule(_) => true,
            Signal::ForbiddenSwitch(_) => true,
            Signal::PinFailures(n) => *n >= 10,
        }
    }

    /// Signals of the same family count once (independence).
    fn family(&self) -> &'static str {
        match self {
            Signal::BinarySignatureInvalid => "binary",
            Signal::KeystoreTampered | Signal::DatabaseTampered => "storage",
            Signal::ForbiddenSwitch(_) => "launch",
            Signal::InjectedModule(_) => "process",
            Signal::PinFailures(_) => "pin",
        }
    }

    pub fn describe(&self) -> String {
        match self {
            Signal::BinarySignatureInvalid => "signature de NIGHTFALL invalide".into(),
            Signal::KeystoreTampered => "magasin de clés altéré".into(),
            Signal::DatabaseTampered => "base de données altérée".into(),
            Signal::ForbiddenSwitch(s) => format!("lancement avec l'option interdite --{s}"),
            Signal::InjectedModule(m) => format!("module non signé injecté : {m}"),
            Signal::PinFailures(n) => format!("{n} échecs consécutifs du code"),
        }
    }
}

/// Nested in the saved settings: a missing field takes its default value,
/// the others are kept.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AutoPurgeRules {
    pub enabled: bool,
    /// Distinct critical signal families required before any purge (min 2).
    pub min_independent_critical: usize,
    /// What to purge when the rule fires.
    pub categories: Vec<PurgeCategory>,
}

impl Default for AutoPurgeRules {
    fn default() -> Self {
        Self { enabled: false, min_independent_critical: 2, categories: vec![PurgeCategory::Sessions] }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "action")]
pub enum Decision {
    Nothing,
    Lockdown { reasons: Vec<String> },
    LockdownAndPurge { reasons: Vec<String>, categories: Vec<PurgeCategory> },
}

impl AutoPurgeRules {
    pub fn evaluate(&self, signals: &[Signal]) -> Decision {
        let reasons: Vec<String> = signals.iter().map(Signal::describe).collect();
        let mut families: Vec<&str> = signals.iter().filter(|s| s.is_critical()).map(Signal::family).collect();
        families.sort();
        families.dedup();
        if families.is_empty() {
            return if signals.is_empty() { Decision::Nothing } else { Decision::Lockdown { reasons } };
        }
        let threshold = self.min_independent_critical.max(2);
        if self.enabled && families.len() >= threshold && !self.categories.is_empty() {
            Decision::LockdownAndPurge { reasons, categories: self.categories.clone() }
        } else {
            Decision::Lockdown { reasons }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn armed() -> AutoPurgeRules {
        AutoPurgeRules { enabled: true, min_independent_critical: 2, categories: vec![PurgeCategory::Sessions] }
    }

    #[test]
    fn disabled_by_default() {
        let r = AutoPurgeRules::default();
        assert!(!r.enabled);
        let d = r.evaluate(&[Signal::KeystoreTampered, Signal::ForbiddenSwitch("no-sandbox".into())]);
        assert!(matches!(d, Decision::Lockdown { .. }));
    }

    #[test]
    fn isolated_signal_never_purges() {
        assert_eq!(armed().evaluate(&[]), Decision::Nothing);
        assert!(matches!(armed().evaluate(&[Signal::KeystoreTampered]), Decision::Lockdown { .. }));
        assert!(matches!(armed().evaluate(&[Signal::PinFailures(3)]), Decision::Lockdown { .. }));
        // Two signals of the same family are not independent.
        assert!(matches!(
            armed().evaluate(&[Signal::KeystoreTampered, Signal::DatabaseTampered]),
            Decision::Lockdown { .. }
        ));
    }

    #[test]
    fn two_independent_critical_signals_purge_when_armed() {
        let d = armed().evaluate(&[Signal::KeystoreTampered, Signal::ForbiddenSwitch("remote-debugging-port".into())]);
        assert_eq!(
            d,
            Decision::LockdownAndPurge {
                reasons: vec![
                    "magasin de clés altéré".into(),
                    "lancement avec l'option interdite --remote-debugging-port".into()
                ],
                categories: vec![PurgeCategory::Sessions]
            }
        );
    }

    #[test]
    fn threshold_cannot_go_below_two() {
        let mut r = armed();
        r.min_independent_critical = 1;
        assert!(matches!(r.evaluate(&[Signal::BinarySignatureInvalid]), Decision::Lockdown { .. }));
    }

    #[test]
    fn saved_rules_with_missing_fields_keep_the_others() {
        // Saved by 0.1.4 (complete at the time).
        let r: AutoPurgeRules = serde_json::from_str(r#"{"enabled":true,"min_independent_critical":3,"categories":["passwords","browsing_data"]}"#).unwrap();
        assert!(r.enabled);
        assert_eq!(r.min_independent_critical, 3);
        assert_eq!(r.categories, vec![PurgeCategory::Passwords, PurgeCategory::BrowsingData]);
        // A field missing: its default only, the others kept.
        let r: AutoPurgeRules = serde_json::from_str(r#"{"enabled":true,"categories":["passwords"]}"#).unwrap();
        assert!(r.enabled);
        assert_eq!(r.min_independent_critical, 2);
        assert_eq!(r.categories, vec![PurgeCategory::Passwords]);
        assert_eq!(serde_json::from_str::<AutoPurgeRules>("{}").unwrap(), AutoPurgeRules::default());
    }
}

//! NIGHTFALL Emergency System.
//!
//! - Level 1, Security Lockdown: handled by the browser (hide tabs, wipe keys
//!   from memory, require the PIN). This crate only models the decision.
//! - Level 2, Emergency Data Purge: [`PurgePlan`] deletes NIGHTFALL data,
//!   restricted to the profile root. Downloads and any file outside the
//!   profile are never touched.
//! - Level 3, Auto-Purge: [`AutoPurgeRules::evaluate`] turns security signals
//!   into a decision. An isolated signal never destroys data.

pub mod purge;
pub mod rules;

pub use purge::{PurgeCategory, PurgePlan, PurgeReport};
pub use rules::{AutoPurgeRules, Decision, Signal};

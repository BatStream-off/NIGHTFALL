//! Smart Resource Manager policy.
//!
//! Chromium already throttles timers of hidden tabs. NIGHTFALL adds:
//! - memory pressure detection from the OS (available vs total RAM),
//! - unloading ("discarding") of long-inactive tabs under pressure, restored
//!   on click. Never discarded: the active tab, pinned tabs, tabs playing
//!   audio, tabs with a form being edited, private tabs (their state would
//!   be lost forever).
//!
//! The policy is pure and unit-tested; the browser applies it.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Pressure {
    None,
    Moderate,
    Critical,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct MemorySnapshot {
    pub total_bytes: u64,
    pub available_bytes: u64,
}

impl MemorySnapshot {
    pub fn pressure(&self) -> Pressure {
        if self.total_bytes == 0 {
            return Pressure::None;
        }
        let free = self.available_bytes as f64 / self.total_bytes as f64;
        if free < 0.08 || self.available_bytes < 500 * 1024 * 1024 {
            Pressure::Critical
        } else if free < 0.18 {
            Pressure::Moderate
        } else {
            Pressure::None
        }
    }
}

/// Read system memory from the OS.
pub fn memory_snapshot() -> MemorySnapshot {
    let mut sys = sysinfo::System::new();
    sys.refresh_memory();
    MemorySnapshot { total_bytes: sys.total_memory(), available_bytes: sys.available_memory() }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PerfConfig {
    /// Unload inactive tabs when memory is short.
    pub discard_enabled: bool,
    /// Minimum inactivity before a tab may be unloaded (minutes), moderate pressure.
    pub discard_after_minutes: i64,
    /// On battery: reduce animations and defer background work.
    pub battery_saver: bool,
}

impl Default for PerfConfig {
    fn default() -> Self {
        Self { discard_enabled: true, discard_after_minutes: 30, battery_saver: true }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabActivity {
    pub id: u32,
    pub last_active: i64,
    pub is_active: bool,
    pub pinned: bool,
    pub audible: bool,
    pub editing_form: bool,
    pub private: bool,
    pub discarded: bool,
}

/// Tabs to unload now, least recently used first.
pub fn tabs_to_discard(tabs: &[TabActivity], pressure: Pressure, cfg: &PerfConfig, now: i64) -> Vec<u32> {
    if !cfg.discard_enabled || pressure == Pressure::None {
        return Vec::new();
    }
    let min_idle = match pressure {
        Pressure::Critical => 5 * 60,
        _ => cfg.discard_after_minutes.max(5) * 60,
    };
    let mut candidates: Vec<&TabActivity> = tabs
        .iter()
        .filter(|t| !t.is_active && !t.pinned && !t.audible && !t.editing_form && !t.private && !t.discarded)
        .filter(|t| now - t.last_active >= min_idle)
        .collect();
    candidates.sort_by_key(|t| t.last_active);
    let max = match pressure {
        Pressure::Critical => candidates.len(),
        _ => candidates.len().div_ceil(2),
    };
    candidates.into_iter().take(max).map(|t| t.id).collect()
}

/// A wall-clock jump much larger than the timer interval means the computer
/// slept (used to lock NIGHTFALL after sleep/resume).
pub fn resumed_from_sleep(expected_elapsed_s: i64, wall_elapsed_s: i64) -> bool {
    wall_elapsed_s - expected_elapsed_s > 90
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tab(id: u32, idle_min: i64) -> TabActivity {
        TabActivity { id, last_active: 10_000 - idle_min * 60, is_active: false, pinned: false, audible: false, editing_form: false, private: false, discarded: false }
    }

    #[test]
    fn pressure_levels() {
        let gb = 1024 * 1024 * 1024;
        assert_eq!(MemorySnapshot { total_bytes: 16 * gb, available_bytes: 8 * gb }.pressure(), Pressure::None);
        assert_eq!(MemorySnapshot { total_bytes: 16 * gb, available_bytes: 2 * gb }.pressure(), Pressure::Moderate);
        assert_eq!(MemorySnapshot { total_bytes: 16 * gb, available_bytes: gb / 4 }.pressure(), Pressure::Critical);
    }

    #[test]
    fn nothing_without_pressure() {
        let tabs = vec![tab(1, 600)];
        assert!(tabs_to_discard(&tabs, Pressure::None, &PerfConfig::default(), 10_000).is_empty());
    }

    #[test]
    fn protected_tabs_are_never_discarded() {
        let mut tabs = vec![tab(1, 600), tab(2, 600), tab(3, 600), tab(4, 600), tab(5, 600), tab(6, 600)];
        tabs[0].is_active = true;
        tabs[1].pinned = true;
        tabs[2].audible = true;
        tabs[3].editing_form = true;
        tabs[4].private = true;
        let d = tabs_to_discard(&tabs, Pressure::Critical, &PerfConfig::default(), 10_000);
        assert_eq!(d, vec![6]);
    }

    #[test]
    fn moderate_pressure_is_gentle_and_lru() {
        let tabs = vec![tab(1, 40), tab(2, 90), tab(3, 10), tab(4, 60)];
        let d = tabs_to_discard(&tabs, Pressure::Moderate, &PerfConfig::default(), 10_000);
        // idle >= 30 min: 1, 2, 4; half of them (rounded up) = 2, oldest first.
        assert_eq!(d, vec![2, 4]);
        let d = tabs_to_discard(&tabs, Pressure::Critical, &PerfConfig::default(), 10_000);
        assert_eq!(d, vec![2, 4, 1, 3]);
    }

    #[test]
    fn sleep_detection() {
        assert!(!resumed_from_sleep(30, 31));
        assert!(resumed_from_sleep(30, 3600));
    }
}

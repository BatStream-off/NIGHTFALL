//! Housekeeping every 15 seconds on the UI thread: auto-lock after
//! inactivity, lock after sleep, memory pressure, session snapshot.

use super::state::core;
use super::ui_thread::on_ui_delayed;
use super::{api, idle, session, windows};
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};

const TICK_MS: i64 = 15_000;
static LAST_TICK: AtomicI64 = AtomicI64::new(0);
static TICKS: AtomicU64 = AtomicU64::new(0);

pub fn start() {
    LAST_TICK.store(nf_core::unix_now(), Ordering::Relaxed);
    idle::touch();
    schedule();
}

fn schedule() {
    on_ui_delayed(TICK_MS, || {
        tick();
        schedule();
    });
}

fn tick() {
    let now = nf_core::unix_now();
    let last = LAST_TICK.swap(now, Ordering::Relaxed);
    let n = TICKS.fetch_add(1, Ordering::Relaxed);
    let (unlocked, settings) = {
        let c = core();
        (c.is_unlocked(), c.settings.clone())
    };
    if !unlocked {
        return;
    }
    // Lock after sleep / resume (wall clock jumped far beyond the timer).
    if settings.lock_on_sleep && nf_perf::resumed_from_sleep(TICK_MS / 1000, now - last) {
        api::lock_now();
        return;
    }
    // Auto-lock after inactivity.
    if settings.auto_lock_minutes > 0 && idle::idle_seconds() >= settings.auto_lock_minutes as i64 * 60 {
        api::lock_now();
        return;
    }
    // Every minute: memory pressure policy and session snapshot.
    if n % 4 == 3 {
        let pressure = nf_perf::memory_snapshot().pressure();
        let activity = windows::tab_activity();
        let tabs: Vec<nf_perf::TabActivity> = activity.iter().map(|(_, t)| t.clone()).collect();
        for id in nf_perf::tabs_to_discard(&tabs, pressure, &settings.perf, now) {
            if let Some((w, _)) = activity.iter().find(|(_, t)| t.id == id) {
                windows::discard_tab(*w, id);
            }
        }
        session::save_session_snapshot();
        super::updater::tick();
        super::player::tick();
    }
    // Every 30 minutes: refresh Shield's filter lists when they are old.
    if n % 120 == 119 {
        api::refresh_filter_lists_if_needed();
    }
}

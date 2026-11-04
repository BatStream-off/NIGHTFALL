//! "Effacer les données de navigation": history and download list (by
//! date, NIGHTFALL database), cookies (by creation date, Chromium cookie
//! store), data of the sites visited in the period (local storage,
//! IndexedDB, service workers, caches: DevTools `Storage.clearDataForOrigin`)
//! and Chromium's HTTP cache (always emptied entirely: Chromium cannot
//! clear it by date).

use super::state::{self, core};
use super::ui_thread::on_ui;
use super::windows;
use cef::*;
use serde::Deserialize;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct ClearRequest {
    /// hour | day | week | month | all
    pub range: String,
    pub history: bool,
    pub downloads: bool,
    pub cookies: bool,
    pub cache: bool,
}

/// Start of the period (unix seconds) for a range id.
pub fn range_start(range: &str, now: i64) -> Option<i64> {
    Some(match range {
        "hour" => now - 3_600,
        "day" => now - 86_400,
        "week" => now - 7 * 86_400,
        "month" => now - 28 * 86_400,
        "all" => 0,
        _ => return None,
    })
}

/// Distinct web origins of a list of URLs.
pub fn origins_of<'a>(urls: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for u in urls {
        if let Ok(p) = url::Url::parse(u) {
            if matches!(p.scheme(), "http" | "https") {
                let o = p.origin().ascii_serialization();
                if !out.contains(&o) {
                    out.push(o);
                }
            }
        }
    }
    out
}

const WINDOWS_TO_UNIX_SECONDS: i64 = 11_644_473_600;

wrap_cookie_visitor! {
    struct DeleteSince {
        // Chromium base::Time (microseconds since 1601).
        since: i64,
        deleted: Arc<AtomicUsize>,
    }

    impl CookieVisitor {
        fn visit(&self, cookie: Option<&Cookie>, _count: i32, _total: i32, delete_cookie: Option<&mut i32>) -> i32 {
            if let (Some(c), Some(d)) = (cookie, delete_cookie) {
                if c.creation.val >= self.since {
                    *d = 1;
                    self.deleted.fetch_add(1, Ordering::Relaxed);
                }
            }
            1
        }
    }
}

wrap_completion_callback! {
    struct CacheCleared;

    impl CompletionCallback {
        fn on_complete(&self) {
            state::log("données de navigation : cache vidé");
        }
    }
}

wrap_delete_cookies_callback! {
    struct CookiesDeleted;

    impl DeleteCookiesCallback {
        fn on_complete(&self, num_deleted: i32) {
            state::log(&format!("données de navigation : {num_deleted} cookies supprimés"));
        }
    }
}

wrap_dev_tools_message_observer! {
    struct SiteDataCleared {
        ok: Arc<AtomicUsize>,
        failed: Arc<AtomicUsize>,
    }

    impl DevToolsMessageObserver {
        fn on_dev_tools_method_result(&self, _browser: Option<&mut Browser>, _message_id: i32, success: i32, _result: Option<&[u8]>) {
            if success != 0 {
                self.ok.fetch_add(1, Ordering::Relaxed);
            } else {
                self.failed.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}

thread_local! {
    /// Keeps the DevTools observer registered until the results are in.
    static OBSERVER: std::cell::RefCell<Option<Registration>> = const { std::cell::RefCell::new(None) };
}

/// Sites whose data was cleared / could not be cleared (self test).
pub static SITES_CLEARED: AtomicUsize = AtomicUsize::new(0);
pub static SITES_FAILED: AtomicUsize = AtomicUsize::new(0);

/// A browser of a non-private window (its DevTools session reaches the
/// main profile's storage).
fn main_profile_browser() -> Option<Browser> {
    windows::window_ids().into_iter().find_map(|id| windows::with_window(id, |c| if c.private { None } else { c.top.browser() }).flatten())
}

pub fn clear(body: &serde_json::Value) -> serde_json::Value {
    let req: ClearRequest = serde_json::from_value(body.clone()).unwrap_or_default();
    let now = nf_core::unix_now();
    let Some(from) = range_start(&req.range, now) else {
        return serde_json::json!({ "ok": false, "error": "période inconnue" });
    };
    let all = req.range == "all";
    let (mut history, mut downloads, mut origins) = (0usize, 0usize, Vec::new());
    {
        let c = core();
        let Some(st) = c.storage() else {
            return serde_json::json!({ "ok": false, "error": "base de données indisponible" });
        };
        if req.cookies {
            // Before the history goes: the sites visited in the period.
            let q = nf_storage::HistoryQuery { from: Some(from), limit: Some(5_000), ..Default::default() };
            let entries = st.history(&q).unwrap_or_default();
            origins = origins_of(entries.iter().map(|e| e.url.as_str()));
        }
        if req.history {
            history = if all {
                let n = st.history(&nf_storage::HistoryQuery { limit: Some(i64::MAX), ..Default::default() }).map(|h| h.len()).unwrap_or(0);
                st.clear_history().map(|_| n).unwrap_or(0)
            } else {
                st.delete_history_range(from, i64::MAX).unwrap_or(0)
            };
        }
        if req.downloads {
            downloads = st.delete_downloads_range(from, i64::MAX).unwrap_or(0);
        }
    }
    if req.history {
        super::security::invalidate_detector();
    }
    let site_count = origins.len();
    let (cookies, cache) = (req.cookies, req.cache);
    on_ui(move || {
        if cookies {
            if let Some(m) = cookie_manager_get_global_manager(None) {
                if all {
                    let mut cb = CookiesDeleted::new();
                    m.delete_cookies(None, None, Some(&mut cb));
                } else {
                    let deleted = Arc::new(AtomicUsize::new(0));
                    let mut v = DeleteSince::new((from + WINDOWS_TO_UNIX_SECONDS) * 1_000_000, deleted.clone());
                    m.visit_all_cookies(Some(&mut v));
                    super::ui_thread::on_ui_delayed(1500, move || {
                        state::log(&format!("données de navigation : {} cookies supprimés", deleted.load(Ordering::Relaxed)));
                    });
                }
            }
            if let (Some(host), false) = (main_profile_browser().and_then(|b| b.host()), origins.is_empty()) {
                let types = "local_storage,indexeddb,file_systems,websql,service_workers,cache_storage,shared_storage,storage_buckets";
                let (ok, failed) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
                let mut observer = SiteDataCleared::new(ok.clone(), failed.clone());
                let registration = host.add_dev_tools_message_observer(Some(&mut observer));
                OBSERVER.with(|o| *o.borrow_mut() = registration);
                let mut sent = 0;
                for o in &origins {
                    if let Some(mut params) = dictionary_value_create() {
                        params.set_string(Some(&CefString::from("origin")), Some(&CefString::from(o.as_str())));
                        params.set_string(Some(&CefString::from("storageTypes")), Some(&CefString::from(types)));
                        if host.execute_dev_tools_method(0, Some(&CefString::from("Storage.clearDataForOrigin")), Some(&mut params)) > 0 {
                            sent += 1;
                        }
                    }
                }
                let expected = sent;
                super::ui_thread::on_ui_delayed(3000, move || {
                    let (ok, failed) = (ok.load(Ordering::Relaxed), failed.load(Ordering::Relaxed));
                    SITES_CLEARED.store(ok, Ordering::Relaxed);
                    SITES_FAILED.store(failed, Ordering::Relaxed);
                    state::log(&format!("données de navigation : données de {ok} site(s) effacées sur {expected} ({failed} échec(s))"));
                    OBSERVER.with(|o| o.borrow_mut().take());
                });
            }
        }
        if cache {
            if let Some(rc) = request_context_get_global_context() {
                let mut cb = CacheCleared::new();
                rc.clear_http_cache(Some(&mut cb));
            }
        }
    });
    serde_json::json!({ "ok": true, "history": history, "downloads": downloads, "sites": site_count })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges() {
        let now = 1_000_000;
        assert_eq!(range_start("hour", now), Some(now - 3600));
        assert_eq!(range_start("day", now), Some(now - 86_400));
        assert_eq!(range_start("week", now), Some(now - 604_800));
        assert_eq!(range_start("month", now), Some(now - 2_419_200));
        assert_eq!(range_start("all", now), Some(0));
        assert_eq!(range_start("siècle", now), None);
    }

    #[test]
    fn origins_from_history() {
        let o = origins_of(["https://a.example/x", "https://a.example/y?z", "http://b.example:8080/", "nightfall://app/x", "data:text/html,x"]);
        assert_eq!(o, vec!["https://a.example".to_string(), "http://b.example:8080".to_string()]);
    }
}

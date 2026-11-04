//! Mission M-0002: diagnostic of the reported problems and performance
//! measurements in the real browser, without changing its behaviour.
//!
//! CI only: compiled with the `selftest` feature (never in a released build)
//! and run with `NIGHTFALL_SELFTEST=diag` (see `ci/diag_runner.py`, which
//! also samples memory and measures start-up). Results are `DIAG ...` lines
//! of the log; the scenario ends with `DIAG FIN` and closes the browser.
//!
//! The hooks below (`api_called`, `pushed`, `key`...) are called from the
//! browser's handlers under `#[cfg(feature = "selftest")]` and do nothing
//! unless this scenario runs.

use super::api::{self, Caller};
use super::state::{self, core, BrowserKind};
use super::ui_thread::on_ui_delayed;
use super::windows;
use cef::{Browser, CefString, ImplBrowser, ImplBrowserHost, ImplBrowserView, ImplFrame, ImplView};
use serde_json::{json, Value};
use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering::Relaxed};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

const PIN: &str = "739204";
const W: u32 = 1;

// ------------------------------------------------------------------ state

static ACTIVE: AtomicBool = AtomicBool::new(false);
static T0: OnceLock<Instant> = OnceLock::new();

/// `NIGHTFALL_SELFTEST=diag` (scenario) or `diag-demarrage` (start-up
/// measurements only): release-like logging, no network log.
pub fn requested() -> bool {
    std::env::var("NIGHTFALL_SELFTEST").map(|v| v.starts_with("diag")).unwrap_or(false)
}

fn active() -> bool {
    ACTIVE.load(Relaxed)
}

/// Milliseconds since the browser process started (first call).
pub fn now_ms() -> f64 {
    T0.get_or_init(Instant::now).elapsed().as_secs_f64() * 1000.0
}

pub fn mark_start() {
    now_ms();
}

/// Start-up milestones (logged in every selftest build, cheap).
pub fn startup_mark(what: &str) {
    state::log(&format!("DIAG demarrage etape={what} ms={:.1}", now_ms()));
}

static UI_READY: AtomicBool = AtomicBool::new(false);

/// First "state" call of a top interface view: the interface is drawn.
pub fn ui_ready() {
    if !UI_READY.swap(true, Relaxed) {
        startup_mark("interface_prete");
    }
}

fn dlog(line: String) {
    state::log(&format!("DIAG {line}"));
}

/// A value for `key=value` log words: no spaces (the log sanitizer splits
/// words on spaces and redacts long opaque words, hence the dots).
fn clean(s: &str) -> String {
    let c: String = s.chars().take(48).map(|c| if c.is_whitespace() || c == '=' { '.' } else { c }).collect();
    if c.is_empty() {
        "-".into()
    } else {
        c
    }
}

// ----------------------------------------------------------------- events

#[derive(Clone)]
struct Ev {
    t: f64,
    kind: &'static str,
    n: i64,
    s: String,
}

static EVENTS: Mutex<Vec<Ev>> = Mutex::new(Vec::new());

fn ev(kind: &'static str, n: i64, s: String) {
    if !active() {
        return;
    }
    if let Ok(mut v) = EVENTS.lock() {
        if v.len() < 400_000 {
            // Time taken under the lock: the list stays in time order.
            v.push(Ev { t: now_ms(), kind, n, s });
        }
    }
}

fn events_since(t: f64) -> Vec<Ev> {
    let Ok(v) = EVENTS.lock() else { return Vec::new() };
    let mut out: Vec<Ev> = v.iter().rev().take_while(|e| e.t >= t).cloned().collect();
    out.reverse();
    out
}

thread_local! {
    /// The scenario's own API calls are not counted as interface calls.
    static IN_CALL: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Hook: an internal API command was received (`api::handle`).
pub fn api_called(kind: &BrowserKind, cmd: &str) {
    if !active() || IN_CALL.with(|c| c.get()) {
        return;
    }
    let who = match kind {
        BrowserKind::Ui { side: true, .. } => "laterale",
        BrowserKind::Ui { .. } => "ui",
        BrowserKind::Tab { .. } => "onglet",
    };
    ev("api", 0, format!("{who}:{cmd}"));
}

static API_COST: Mutex<BTreeMap<String, (u64, f64, f64)>> = Mutex::new(BTreeMap::new());

/// Hook: time spent answering an API command (CEF IO thread, scheme.rs).
pub fn api_cost(cmd: &str, ms: f64) {
    if !active() {
        return;
    }
    if let Ok(mut m) = API_COST.lock() {
        let e = m.entry(cmd.chars().take(40).collect()).or_insert((0, 0.0, 0.0));
        e.0 += 1;
        e.1 += ms;
        if ms > e.2 {
            e.2 = ms;
        }
    }
}

/// Hook: an event pushed to a window's interface views (windows::push_event).
pub fn pushed(event: &Value, bytes: usize) {
    if !active() {
        return;
    }
    let t = event.get("type").and_then(Value::as_str).unwrap_or("?").to_string();
    ev("push", bytes as i64, t);
}

/// Hook: on_pre_key_event saw a key press (with the action it maps to).
pub fn key(code: i32, modifiers: u32, action: Option<&str>) {
    ev("touche", code as i64, format!("{modifiers}:{}", action.unwrap_or("-")));
}

pub fn loading(tab_id: u32, is_loading: bool) {
    ev(if is_loading { "charge" } else { "charge_fin" }, tab_id as i64, String::new());
}

pub fn load_end(tab_id: u32, url: &str, status: i32) {
    ev("load_end", tab_id as i64, format!("{status} {url}"));
}

pub fn browser_created(id: i32) {
    ev("cree", id as i64, String::new());
}

pub fn browser_closed(id: i32) {
    ev("ferme", id as i64, String::new());
}

pub fn popup_opened(panel: &str) {
    ev("popup_open", 0, panel.to_string());
}

static SH_N: AtomicU64 = AtomicU64::new(0);
static SH_NS: AtomicU64 = AtomicU64::new(0);
static SH_MAX: AtomicU64 = AtomicU64::new(0);
static SH_BLOCKED: AtomicU64 = AtomicU64::new(0);
static PS_N: AtomicU64 = AtomicU64::new(0);
static PS_NS: AtomicU64 = AtomicU64::new(0);
static PS_MAX: AtomicU64 = AtomicU64::new(0);
static PS_BYTES: AtomicU64 = AtomicU64::new(0);

/// Hook: one Shield network check (on_before_resource_load, IO thread).
pub fn shield_check(ns: u64, blocked: bool) {
    if !active() {
        return;
    }
    SH_N.fetch_add(1, Relaxed);
    SH_NS.fetch_add(ns, Relaxed);
    SH_MAX.fetch_max(ns, Relaxed);
    if blocked {
        SH_BLOCKED.fetch_add(1, Relaxed);
    }
}

/// Hook: Shield's page script built for a frame (on_load_start, UI thread).
pub fn page_script(ns: u64, bytes: usize) {
    if !active() {
        return;
    }
    PS_N.fetch_add(1, Relaxed);
    PS_NS.fetch_add(ns, Relaxed);
    PS_MAX.fetch_max(ns, Relaxed);
    PS_BYTES.fetch_add(bytes as u64, Relaxed);
}

fn reset_counters() {
    for a in [&SH_N, &SH_NS, &SH_MAX, &SH_BLOCKED, &PS_N, &PS_NS, &PS_MAX, &PS_BYTES] {
        a.store(0, Relaxed);
    }
}

static CONSOLE: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Hook: console message of a script run by this scenario.
pub fn console(msg: &str) {
    if active() && msg.starts_with("diag-") {
        if let Ok(mut c) = CONSOLE.lock() {
            c.push(msg.to_string());
        }
    }
}

fn console_clear(prefix: &str) {
    if let Ok(mut c) = CONSOLE.lock() {
        c.retain(|m| !m.starts_with(prefix));
    }
}

fn console_has(prefix: &str) -> bool {
    CONSOLE.lock().map(|c| c.iter().any(|m| m.starts_with(prefix))).unwrap_or(false)
}

fn console_take(prefix: &str) -> Value {
    let Ok(mut c) = CONSOLE.lock() else { return Value::Null };
    let Some(i) = c.iter().position(|m| m.starts_with(prefix)) else { return Value::Null };
    let m = c.remove(i);
    serde_json::from_str(m[prefix.len()..].trim()).unwrap_or(Value::Null)
}

// ----------------------------------------------------------------- runner

enum Act {
    Run(Box<dyn FnOnce() + Send>),
    Wait(i64),
    Until(&'static str, f64, Box<dyn Fn() -> bool + Send>),
}

#[derive(Default)]
struct Seq(Vec<Act>);

impl Seq {
    fn run(&mut self, f: impl FnOnce() + Send + 'static) -> &mut Self {
        self.0.push(Act::Run(Box::new(f)));
        self
    }
    fn wait(&mut self, ms: i64) -> &mut Self {
        self.0.push(Act::Wait(ms));
        self
    }
    fn until(&mut self, what: &'static str, timeout_ms: f64, cond: impl Fn() -> bool + Send + 'static) -> &mut Self {
        self.0.push(Act::Until(what, timeout_ms, Box::new(cond)));
        self
    }
    fn then(&mut self, other: Seq) -> &mut Self {
        self.0.extend(other.0);
        self
    }
}

static PLAN: Mutex<VecDeque<Act>> = Mutex::new(VecDeque::new());

fn plan_back(s: Seq) {
    if let Ok(mut p) = PLAN.lock() {
        p.extend(s.0);
    }
}

/// Run `s` next (before what is already planned).
fn plan_front(s: Seq) {
    if let Ok(mut p) = PLAN.lock() {
        for a in s.0.into_iter().rev() {
            p.push_front(a);
        }
    }
}

fn pump() {
    let next = PLAN.lock().ok().and_then(|mut p| p.pop_front());
    match next {
        None => {}
        Some(Act::Run(f)) => {
            if std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).is_err() {
                dlog("erreur etape=panique".into());
            }
            on_ui_delayed(0, pump);
        }
        Some(Act::Wait(ms)) => on_ui_delayed(ms, pump),
        Some(Act::Until(what, timeout, cond)) => poll(what, now_ms(), timeout, cond),
    }
}

fn poll(what: &'static str, start: f64, timeout: f64, cond: Box<dyn Fn() -> bool + Send>) {
    let ok = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| cond())).unwrap_or(false);
    if ok {
        on_ui_delayed(0, pump);
        return;
    }
    if now_ms() - start > timeout {
        dlog(format!("attente quoi={} resultat=delai_depasse ms={timeout:.0}", clean(what)));
        on_ui_delayed(0, pump);
        return;
    }
    on_ui_delayed(20, move || poll(what, start, timeout, cond));
}

static MARKS: Mutex<BTreeMap<&'static str, f64>> = Mutex::new(BTreeMap::new());

fn mark(name: &'static str) -> f64 {
    let t = now_ms();
    if let Ok(mut m) = MARKS.lock() {
        m.insert(name, t);
    }
    t
}

fn marked(name: &'static str) -> f64 {
    MARKS.lock().ok().and_then(|m| m.get(name).copied()).unwrap_or(0.0)
}

// ---------------------------------------------------------------- helpers

fn call(cmd: &str, body: Value) -> Value {
    IN_CALL.with(|c| c.set(true));
    let r = api::handle(&Caller { kind: BrowserKind::Ui { window_id: W, side: false } }, cmd, &body);
    IN_CALL.with(|c| c.set(false));
    r
}

fn unlocked() -> bool {
    let c = core();
    c.is_unlocked()
}

fn fully_locked() -> bool {
    let c = core();
    !c.is_unlocked() && c.unlocked.is_none()
}

fn tab_ids() -> Vec<u32> {
    windows::with_window(W, |c| c.tabs.iter().map(|t| t.id).collect()).unwrap_or_default()
}

fn all_loaded() -> bool {
    windows::with_window(W, |c| c.tabs.iter().all(|t| !t.loading)).unwrap_or(false)
}

fn browsers_alive() -> usize {
    state::browsers().len()
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum View {
    Top,
    Popup,
}

fn top(w: u32) -> Option<Browser> {
    windows::with_window(w, |c| c.top.browser()).flatten()
}

fn popup_browser() -> Option<Browser> {
    windows::with_window(W, |c| c.popup.as_ref().and_then(|p| p.view.browser())).flatten()
}

fn view_browser(v: View) -> Option<Browser> {
    match v {
        View::Top => top(W),
        View::Popup => popup_browser(),
    }
}

/// Native focus to the view, as a real mouse press does.
fn focus_view(v: View) {
    let view = match v {
        View::Top => windows::with_window(W, |c| Some(c.top.clone())).flatten(),
        View::Popup => windows::with_window(W, |c| c.popup.as_ref().map(|p| p.view.clone())).flatten(),
    };
    if let Some(view) = view {
        view.request_focus();
    }
}

fn js_in(b: Option<Browser>, code: &str) {
    if let Some(f) = b.and_then(|b| b.main_frame()) {
        f.execute_java_script(Some(&CefString::from(code)), Some(&CefString::from("nightfall://selftest")), 0);
    }
}

static DT_ID: AtomicU64 = AtomicU64::new(880_000);

/// DevTools protocol message (in process, no debugging port).
fn devtools(b: &Option<Browser>, method: &str, params: Value) {
    if let Some(h) = b.as_ref().and_then(|b| b.host()) {
        let msg = json!({ "id": DT_ID.fetch_add(1, Relaxed), "method": method, "params": params }).to_string();
        let _ = h.send_dev_tools_message(Some(msg.as_bytes()));
    }
}

/// Real input event, processed by the page like a hardware mouse event.
fn mouse(b: &Option<Browser>, kind: &str, x: f64, y: f64, button: &str) {
    let buttons = match (kind, button) {
        ("mousePressed", "left") => 1,
        ("mousePressed", "right") => 2,
        ("mousePressed", "middle") => 4,
        _ => 0,
    };
    let clicks = if kind == "mouseMoved" { 0 } else { 1 };
    devtools(b, "Input.dispatchMouseEvent", json!({ "type": kind, "x": x, "y": y, "button": button, "buttons": buttons, "clickCount": clicks }));
}

const JS_RECT: &str = r#"(function(){var l=document.querySelectorAll(__SEL__);var e=l[__IDX__];var o={ok:false,n:l.length};if(e){var r=e.getBoundingClientRect();var cs=getComputedStyle(e);var lab=(e.querySelector&&e.querySelector('.m-label'))||e;o={ok:r.width>0&&r.height>0,n:l.length,x:r.left+r.width/2,y:r.top+r.height/2,w:r.width,h:r.height,dis:!!e.disabled,hid:!!e.hidden||cs.display==='none'||cs.visibility==='hidden',t:(lab.textContent||'').trim().slice(0,40),title:(e.title||e.getAttribute('aria-label')||'').slice(0,40),tag:e.tagName};}console.log('diag-rect '+JSON.stringify(o));})();"#;

fn query_rect(v: View, sel: &str, idx: usize) {
    console_clear("diag-rect ");
    let s = serde_json::to_string(sel).unwrap_or_else(|_| "\"\"".into());
    js_in(view_browser(v), &JS_RECT.replace("__SEL__", &s).replace("__IDX__", &idx.to_string()));
}

/// Wraps the top view's event entry point and fetch: counts and times the
/// events pushed by the native side, counts API calls per command.
const JS_UI_WRAP: &str = r#"(function(){if(window.__nfDiag)return;var D=window.__nfDiag={ev:{},api:{}};var o=window.nfEvent;window.nfEvent=function(e){var t=performance.now();try{return o(e);}finally{var k=(e&&e.type)||'?';var s=D.ev[k]||(D.ev[k]={n:0,ms:0,max:0});var d=performance.now()-t;s.n++;s.ms+=d;if(d>s.max)s.max=d;}};var f=window.fetch;window.fetch=function(u){try{var c=String(u).split('/api/')[1]||'?';D.api[c]=(D.api[c]||0)+1;}catch(x){}return f.apply(this,arguments);};})();"#;
const JS_UI_READ: &str = r#"(function(){var D=window.__nfDiag||{ev:{},api:{}};console.log('diag-ui '+JSON.stringify(D));if(window.__nfDiag){window.__nfDiag.ev={};window.__nfDiag.api={};}})();"#;

fn ui_wrap() {
    js_in(top(W), JS_UI_WRAP);
}

/// Navigation Timing of the page, read once the page has loaded.
const JS_NAV: &str = r#"(function(){try{var n=performance.getEntriesByType('navigation')[0]||{};var r=performance.getEntriesByType('resource');var fcp=performance.getEntriesByName('first-contentful-paint')[0];var tr=0;for(var i=0;i<r.length;i++){tr+=r[i].transferSize||0;}var lcp=0;try{new PerformanceObserver(function(l){var e=l.getEntries();if(e.length)lcp=e[e.length-1].startTime;}).observe({type:'largest-contentful-paint',buffered:true});}catch(x){}setTimeout(function(){console.log('diag-nav '+JSON.stringify({ttfb:n.responseStart||0,dcl:n.domContentLoadedEventEnd||0,load:n.loadEventEnd||0,fcp:fcp?fcp.startTime:0,lcp:lcp,res:r.length,tr:tr,doc:n.transferSize||0,proto:n.nextHopProtocol||'',host:location.host}));},250);}catch(x){console.log('diag-nav {"err":1}');}})();"#;

#[derive(Clone, Default)]
struct Snap {
    tabs: Vec<(u32, String)>,
    active: Option<u32>,
    windows: Vec<u32>,
    popup: String,
    locked: bool,
    fullscreen: bool,
    zoom: f64,
}

fn snap() -> Snap {
    let tabs: Vec<(u32, String)> = windows::with_window(W, |c| c.tabs.iter().map(|t| (t.id, t.url.clone())).collect()).unwrap_or_default();
    Snap {
        tabs,
        active: windows::active_tab_id(W),
        windows: windows::window_ids(),
        popup: windows::popup_panel(W).unwrap_or_else(|| "-".into()),
        locked: !unlocked(),
        fullscreen: windows::fullscreen_report(W).2,
        zoom: windows::active_browser(W).and_then(|b| b.host()).map(|h| h.zoom_level()).unwrap_or(0.0),
    }
}

/// Short name of an address (never a full URL in the log).
fn kind_of(url: &str) -> String {
    if let Some(r) = url.strip_prefix("nightfall://app/") {
        return r.split(['.', '?', '#']).next().unwrap_or("").to_string();
    }
    if url.starts_with("view-source:") {
        return "source".into();
    }
    if let Some(r) = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://")) {
        return r.split(['/', '?', '#']).next().unwrap_or("").to_string();
    }
    url.split(':').next().unwrap_or("").to_string()
}

fn diff(a: &Snap, b: &Snap) -> String {
    let mut out = Vec::new();
    let dn = b.tabs.len() as i64 - a.tabs.len() as i64;
    if dn != 0 {
        out.push(format!("onglets{dn:+}"));
    }
    let new: Vec<String> = b.tabs.iter().filter(|t| !a.tabs.iter().any(|x| x.0 == t.0)).map(|t| kind_of(&t.1)).collect();
    if !new.is_empty() {
        out.push(format!("nouveaux:{}", new.join("+")));
    }
    let url_of = |s: &Snap| s.active.and_then(|id| s.tabs.iter().find(|t| t.0 == id).map(|t| t.1.clone())).unwrap_or_default();
    if a.active != b.active {
        out.push("onglet_actif_change".into());
    } else if url_of(a) != url_of(b) {
        out.push(format!("navigation:{}", kind_of(&url_of(b))));
    }
    let dw = b.windows.len() as i64 - a.windows.len() as i64;
    if dw != 0 {
        out.push(format!("fenetres{dw:+}"));
    }
    if a.popup != b.popup {
        out.push(format!("menu:{}>{}", a.popup, b.popup));
    }
    if a.locked != b.locked {
        out.push(format!("verrou:{}>{}", a.locked as u8, b.locked as u8));
    }
    if a.fullscreen != b.fullscreen {
        out.push(format!("plein_ecran:{}>{}", a.fullscreen as u8, b.fullscreen as u8));
    }
    if (a.zoom - b.zoom).abs() > 1e-3 {
        out.push(format!("zoom:{:.2}>{:.2}", a.zoom, b.zoom));
    }
    if out.is_empty() {
        "aucun".into()
    } else {
        out.join(",")
    }
}

/// API commands sent by the interface since `t` (housekeeping ones left out).
fn apis_since(t: f64) -> String {
    const NOISE: &[&str] = &["ui.activity", "popup.resize", "tabs", "page.info", "update.status", "state", "omnibox.state", "vault.forPage"];
    let mut v: Vec<String> = Vec::new();
    for e in events_since(t) {
        if e.kind != "api" {
            continue;
        }
        let cmd = e.s.split_once(':').map(|x| x.1.to_string()).unwrap_or_else(|| e.s.clone());
        if !NOISE.contains(&cmd.as_str()) && v.last() != Some(&cmd) {
            v.push(cmd);
        }
    }
    if v.is_empty() {
        "-".into()
    } else {
        v.join(",")
    }
}

static SNAPS: Mutex<BTreeMap<&'static str, Snap>> = Mutex::new(BTreeMap::new());

fn save_snap(name: &'static str) {
    let s = snap();
    if let Ok(mut m) = SNAPS.lock() {
        m.insert(name, s);
    }
}

fn saved_snap(name: &'static str) -> Snap {
    SNAPS.lock().ok().and_then(|m| m.get(name).cloned()).unwrap_or_default()
}

static NEXT_PAGE: AtomicU32 = AtomicU32::new(1);

/// Exactly `n` tabs in the main window (web pages added when missing), the
/// first one active. Never closes the last tab (that would close the window).
fn restore_tabs(n: usize) {
    windows::popup_close(W);
    for id in tab_ids().into_iter().skip(n) {
        if tab_ids().len() > 1 {
            windows::close_tab(W, id);
        }
    }
    for _ in tab_ids().len()..n {
        let url = format!("https://example.com/?nf-diag={}", NEXT_PAGE.fetch_add(1, Relaxed));
        windows::add_tab(W, &url, false);
    }
    // Pinned or muted by a previous test (tab context menu).
    let pinned: Vec<u32> = windows::with_window(W, |c| c.tabs.iter().filter(|t| t.pinned).map(|t| t.id).collect()).unwrap_or_default();
    for id in pinned {
        windows::set_pinned(W, id, false);
    }
    for id in tab_ids() {
        if windows::tab_muted(W, id) {
            windows::set_tab_muted(W, id, false);
        }
    }
    if let Some(first) = tab_ids().first() {
        windows::activate_tab(W, *first);
    }
    // The first tab shows a web page (an internal page cannot be left for
    // the same test conditions everywhere).
    if !windows::active_url(W).unwrap_or_default().starts_with("http") {
        let old = tab_ids().first().copied();
        let url = format!("https://example.com/?nf-diag={}", NEXT_PAGE.fetch_add(1, Relaxed));
        windows::add_tab_at(W, &url, true, Some(0));
        if let Some(id) = old {
            if tab_ids().len() > 1 {
                windows::close_tab(W, id);
            }
        }
    }
}

/// Back to a known state: unlocked, one window, no popup, no fullscreen,
/// `n` loaded tabs.
fn reset_seq(n: usize) -> Seq {
    let mut s = Seq::default();
    s.run(|| {
        windows::popup_close(W);
        for id in windows::window_ids() {
            if id != W {
                windows::close_window(id);
            }
        }
        if !unlocked() {
            let _ = call("unlock", json!({ "pin": PIN }));
        }
        if windows::content_fullscreen(W) {
            windows::exit_content_fullscreen(W);
        }
        if windows::fullscreen_report(W).2 {
            windows::toggle_window_fullscreen(W);
        }
    });
    s.until("état de base", 15000.0, || {
        // A lock may still be completing (cookies sealed asynchronously).
        if fully_locked() {
            let _ = call("unlock", json!({ "pin": PIN }));
        }
        let u = unlocked();
        // Hidden content while "unlocked": a lock is in progress.
        let shown = windows::with_window(W, |c| c.content.is_visible() != 0).unwrap_or(false);
        u && shown && windows::window_ids().len() == 1 && !windows::fullscreen_report(W).2
    });
    s.run(move || {
        restore_tabs(n);
        super::browsing::zoom(W, "reset");
        state::shield().set_site_allowed("example.com", false);
        ui_wrap();
    });
    s.until("onglets chargés", 20000.0, all_loaded);
    s.wait(500);
    s
}

// ------------------------------------------------------------ the scenario

pub fn start() {
    ACTIVE.store(true, Relaxed);
    dlog(format!("debut plateforme={} ms={:.0}", std::env::consts::OS, now_ms()));
    plan_back(setup());
    plan_back(shield_ready());
    plan_back(tab_close());
    plan_back(popups_and_context_menu());
    plan_back(ui_latency());
    plan_back(buttons());
    plan_back(page_loads());
    plan_back(memory());
    plan_back(h1());
    // Last: a native modal dialog could stop the scenario.
    plan_back(dialogs());
    plan_back(finish());
    on_ui_delayed(3000, pump);
}

fn setup() -> Seq {
    let mut s = Seq::default();
    s.run(|| {
        let t = now_ms();
        let r = call("setup", json!({ "pin": PIN, "confirm": PIN }));
        dlog(format!("etape creation_code ok={} ms={:.0}", r["ok"], now_ms() - t));
        mark("confirmation");
        let r = call("recovery.confirm", json!({}));
        dlog(format!("etape confirmation ok={}", r["ok"]));
        core().settings.auto_lock_minutes = 0;
    });
    s.until("premier onglet", 30000.0, || windows::with_window(W, |c| c.tabs.len() == 1 && c.tabs.iter().all(|t| !t.loading)).unwrap_or(false));
    s.run(|| {
        dlog(format!("etape premier_onglet ms_depuis_confirmation={:.0}", now_ms() - marked("confirmation")));
        ui_wrap();
    });
    s
}

/// Shield's filter lists are downloaded after the first unlock: measures
/// need the real engine.
fn shield_ready() -> Seq {
    let mut s = Seq::default();
    s.until("listes Shield", 180000.0, || state::shield().rules_loaded() > 1000);
    s.wait(2000);
    s.run(|| {
        let sh = state::shield();
        let rules = sh.rules_loaded();
        let mut times = Vec::new();
        for _ in 0..3 {
            let t = Instant::now();
            sh.rebuild();
            times.push(t.elapsed().as_secs_f64() * 1000.0);
        }
        dlog(format!("shield regles={rules} reconstruction_ms={:.0},{:.0},{:.0}", times[0], times[1], times[2]));
    });
    s
}

// ------------------------------------------------- tab close (close button)

#[derive(Clone, Default)]
struct Trial {
    target: Option<u32>,
    browser: Option<i32>,
    x: f64,
    y: f64,
    ok: bool,
}

static TRIAL: Mutex<Option<Trial>> = Mutex::new(None);

fn trial() -> Trial {
    TRIAL.lock().ok().and_then(|t| t.clone()).unwrap_or_default()
}

/// One click on a tab's close button: `mode` souris (real press/release
/// `hold` ms apart), milieu (middle button) or clic_js (element.click(),
/// no mousedown).
fn close_trial(rep: u32, hold: i64, target: &'static str, mode: &'static str) -> Seq {
    let idx = if target == "actif" { 0 } else { 1 };
    let mut s = reset_seq(3);
    // Hover the tab first: an inactive narrow tab only shows its button on hover.
    s.run(move || query_rect(View::Top, "#tabs .tab", idx));
    s.until("onglet", 3000.0, || console_has("diag-rect "));
    s.run(|| {
        let r = console_take("diag-rect ");
        if let (Some(x), Some(y)) = (r["x"].as_f64(), r["y"].as_f64()) {
            mouse(&top(W), "mouseMoved", x, y, "none");
        }
    });
    s.wait(250);
    s.run(move || query_rect(View::Top, "#tabs .t-close", idx));
    s.until("bouton de fermeture", 3000.0, || console_has("diag-rect "));
    s.run(move || {
        let r = console_take("diag-rect ");
        let target_id = tab_ids().get(idx).copied();
        let browser = target_id.and_then(|id| windows::tab_browser(W, id)).map(|b| b.identifier());
        let t = Trial { target: target_id, browser, x: r["x"].as_f64().unwrap_or(0.0), y: r["y"].as_f64().unwrap_or(0.0), ok: r["ok"].as_bool() == Some(true) };
        save_snap("fermeture");
        let b = top(W);
        mark("appui");
        match mode {
            "clic_js" => js_in(b, &format!("document.querySelectorAll('#tabs .t-close')[{idx}].click();")),
            "milieu" => mouse(&b, "mousePressed", t.x, t.y, "middle"),
            _ => {
                mouse(&b, "mouseMoved", t.x, t.y, "none");
                mouse(&b, "mousePressed", t.x, t.y, "left");
            }
        }
        if let Ok(mut g) = TRIAL.lock() {
            *g = Some(t);
        }
    });
    s.wait(hold.max(1));
    s.run(move || {
        let t = trial();
        match mode {
            "milieu" => mouse(&top(W), "mouseReleased", t.x, t.y, "middle"),
            "souris" => mouse(&top(W), "mouseReleased", t.x, t.y, "left"),
            _ => {}
        }
    });
    s.wait(1300);
    s.run(move || {
        let t = trial();
        let before = saved_snap("fermeture");
        let after = snap();
        let t0 = marked("appui");
        let evs = events_since(t0);
        let first = |pred: &dyn Fn(&Ev) -> bool| evs.iter().find(|e| pred(*e)).map(|e| format!("{:.0}", e.t - t0)).unwrap_or_else(|| "-".into());
        let closed = t.target.map(|id| !after.tabs.iter().any(|x| x.0 == id)).unwrap_or(false);
        let activate = first(&|e: &Ev| e.kind == "api" && e.s.ends_with(":tab.activate"));
        let close = first(&|e: &Ev| e.kind == "api" && e.s.ends_with(":tab.close"));
        let push = first(&|e: &Ev| e.kind == "push" && e.s == "tabs");
        let destroyed = first(&|e: &Ev| e.kind == "ferme" && Some(e.n as i32) == t.browser);
        // Closing a background tab must leave the active tab as it was.
        let kept = if target == "inactif" { (after.active == before.active) as u8 } else { 2 };
        dlog(format!(
            "fermeture mode={mode} cible={target} appui_ms={hold} essai={rep} bouton_visible={} ferme={} tab_activate_ms={activate} tab_close_ms={close} premier_push_tabs_ms={push} navigateur_detruit_ms={destroyed} actif_conserve={kept} api={}",
            t.ok as u8,
            closed as u8,
            apis_since(t0)
        ));
    });
    s
}

fn tab_close() -> Seq {
    let mut s = Seq::default();
    s.run(|| dlog("etape fermeture_onglets".into()));
    for rep in 0..5 {
        for hold in [40, 100, 160] {
            for target in ["actif", "inactif"] {
                s.then(close_trial(rep, hold, target, "souris"));
            }
        }
    }
    for rep in 0..3 {
        for target in ["actif", "inactif"] {
            s.then(close_trial(rep, 0, target, "clic_js"));
            s.then(close_trial(rep, 80, target, "milieu"));
        }
    }
    // Tab closing by the API: are the closed browsers really destroyed?
    s.then(reset_seq(1));
    s.run(|| {
        mark("rafale");
        save_snap("rafale");
        for i in 0..6 {
            windows::add_tab(W, &format!("https://example.com/?nf-diag-rafale={i}"), false);
        }
    });
    s.until("onglets de la rafale", 30000.0, || tab_ids().len() == 7 && all_loaded());
    s.run(|| {
        let alive = browsers_alive();
        mark("rafale_fermeture");
        for id in tab_ids().into_iter().skip(1) {
            windows::close_tab(W, id);
        }
        dlog(format!("rafale ouverts=6 navigateurs_avant_fermeture={alive} onglets_apres={}", tab_ids().len()));
    });
    s.wait(4000);
    s.run(|| {
        let t = marked("rafale_fermeture");
        let ends: Vec<f64> = events_since(t).iter().filter(|e| e.kind == "ferme").map(|e| e.t - t).collect();
        let last = ends.iter().cloned().fold(0.0, f64::max);
        dlog(format!("rafale navigateurs_detruits={} dernier_ms={last:.0} navigateurs_vivants={} onglets={}", ends.len(), browsers_alive(), tab_ids().len()));
    });
    s
}

// ------------------------------------------------ menus, context menu, race

fn click_at(v: View, button: &'static str, hold: i64) -> Seq {
    let mut s = Seq::default();
    s.run(move || {
        let r = console_take("diag-rect ");
        let (x, y) = (r["x"].as_f64().unwrap_or(-1.0), r["y"].as_f64().unwrap_or(-1.0));
        if let Ok(mut g) = TRIAL.lock() {
            *g = Some(Trial { x, y, ok: r["ok"].as_bool() == Some(true), ..Default::default() });
        }
        if x >= 0.0 {
            focus_view(v);
            let b = view_browser(v);
            mouse(&b, "mouseMoved", x, y, "none");
            mouse(&b, "mousePressed", x, y, button);
        }
    });
    s.wait(hold);
    s.run(move || {
        let t = trial();
        if t.x >= 0.0 {
            mouse(&view_browser(v), "mouseReleased", t.x, t.y, button);
        }
    });
    s
}

fn popups_and_context_menu() -> Seq {
    let mut s = Seq::default();
    s.run(|| dlog("etape menus".into()));
    // Right click on a tab opens its context menu.
    for rep in 0..3 {
        s.then(reset_seq(3));
        s.run(|| query_rect(View::Top, "#tabs .tab", 1));
        s.until("onglet", 3000.0, || console_has("diag-rect "));
        s.run(|| {
            mark("droit");
        });
        s.then(click_at(View::Top, "right", 90));
        s.wait(1200);
        s.run(move || {
            let panel = windows::popup_panel(W).unwrap_or_else(|| "-".into());
            dlog(format!("menu_contextuel essai={rep} menu={panel} api={}", apis_since(marked("droit"))));
        });
    }
    // Menu open, then another toolbar button: the second dropdown must show
    // (the first one closes itself when it loses the focus).
    for rep in 0..5 {
        s.then(reset_seq(2));
        s.run(|| query_rect(View::Top, "#b-menu", 0));
        s.until("bouton menu", 3000.0, || console_has("diag-rect "));
        s.then(click_at(View::Top, "left", 90));
        s.until("menu ouvert", 5000.0, || windows::popup_panel(W).as_deref() == Some("menu"));
        s.wait(700);
        s.run(|| query_rect(View::Top, "#b-shield", 0));
        s.until("bouton Shield", 3000.0, || console_has("diag-rect "));
        s.run(|| {
            mark("enchaine");
        });
        s.then(click_at(View::Top, "left", 90));
        s.wait(1500);
        s.run(move || {
            let panel = windows::popup_panel(W).unwrap_or_else(|| "-".into());
            dlog(format!("menu_puis_shield essai={rep} menu_final={panel} api={}", apis_since(marked("enchaine"))));
        });
        // The same button again closes its dropdown.
        s.run(|| query_rect(View::Top, "#b-shield", 0));
        s.until("bouton Shield", 3000.0, || console_has("diag-rect "));
        s.then(click_at(View::Top, "left", 90));
        s.wait(1200);
        s.run(move || {
            let panel = windows::popup_panel(W).unwrap_or_else(|| "-".into());
            dlog(format!("second_clic_ferme essai={rep} menu_final={panel}"));
        });
    }
    s
}

// ------------------------------------------------------- interface latency

fn popup_ready(panel: &'static str, since: &'static str) -> bool {
    let open = windows::popup_panel(W).map(|p| p.starts_with(panel)).unwrap_or(false);
    open && events_since(marked(since)).iter().any(|e| e.kind == "api" && e.s == "ui:popup.resize")
}

fn ui_latency() -> Seq {
    let mut s = Seq::default();
    s.run(|| dlog("etape latence".into()));
    for rep in 0..10 {
        s.then(reset_seq(2));
        s.run(|| query_rect(View::Top, "#b-menu", 0));
        s.until("bouton menu", 3000.0, || console_has("diag-rect "));
        s.run(|| {
            mark("latence");
        });
        s.then(click_at(View::Top, "left", 80));
        s.until("menu affiché", 5000.0, || popup_ready("menu", "latence"));
        s.run(move || {
            let t0 = marked("latence");
            let evs = events_since(t0);
            let at = |k: &str, v: &str| evs.iter().find(|e| e.kind == k && e.s == v).map(|e| format!("{:.0}", e.t - t0)).unwrap_or_else(|| "-".into());
            dlog(format!("latence quoi=menu essai={rep} api_popup_open_ms={} vue_creee_ms={} contenu_affiche_ms={}", at("api", "ui:popup.open"), at("popup_open", "menu"), at("api", "ui:popup.resize")));
        });
    }
    for rep in 0..10 {
        s.then(reset_seq(2));
        s.run(|| query_rect(View::Top, "#new-tab", 0));
        s.until("bouton nouvel onglet", 3000.0, || console_has("diag-rect "));
        s.run(|| {
            mark("latence");
            save_snap("latence");
        });
        s.then(click_at(View::Top, "left", 80));
        s.until("nouvel onglet chargé", 8000.0, || {
            let before = saved_snap("latence");
            let new: Vec<u32> = tab_ids().into_iter().filter(|id| !before.tabs.iter().any(|t| t.0 == *id)).collect();
            new.len() == 1 && events_since(marked("latence")).iter().any(|e| e.kind == "load_end" && e.n == new[0] as i64)
        });
        s.run(move || {
            let t0 = marked("latence");
            let evs = events_since(t0);
            let api = evs.iter().find(|e| e.kind == "api" && e.s == "ui:tab.new").map(|e| e.t - t0);
            let created = evs.iter().find(|e| e.kind == "cree").map(|e| e.t - t0);
            let loaded = evs.iter().find(|e| e.kind == "load_end").map(|e| e.t - t0);
            let f = |v: Option<f64>| v.map(|v| format!("{v:.0}")).unwrap_or_else(|| "-".into());
            dlog(format!("latence quoi=nouvel_onglet essai={rep} api_ms={} navigateur_cree_ms={} page_chargee_ms={}", f(api), f(created), f(loaded)));
        });
    }
    for rep in 0..10 {
        if rep == 0 {
            s.then(reset_seq(3));
        }
        let idx = if rep % 2 == 0 { 1 } else { 0 };
        s.run(move || query_rect(View::Top, "#tabs .tab", idx));
        s.until("onglet", 3000.0, || console_has("diag-rect "));
        s.run(|| {
            mark("latence");
            save_snap("latence");
        });
        s.then(click_at(View::Top, "left", 80));
        s.wait(900);
        s.run(move || {
            let t0 = marked("latence");
            let evs = events_since(t0);
            let api = evs.iter().find(|e| e.kind == "api" && e.s == "ui:tab.activate").map(|e| e.t - t0);
            let push = evs.iter().find(|e| e.kind == "push" && e.s == "tabs").map(|e| (e.t - t0, e.n));
            let switched = saved_snap("latence").active != windows::active_tab_id(W);
            dlog(format!(
                "latence quoi=changement_onglet essai={rep} change={} api_ms={} push_tabs_ms={} push_tabs_octets={}",
                switched as u8,
                api.map(|v| format!("{v:.0}")).unwrap_or_else(|| "-".into()),
                push.map(|p| format!("{:.0}", p.0)).unwrap_or_else(|| "-".into()),
                push.map(|p| p.1.to_string()).unwrap_or_else(|| "-".into())
            ));
        });
    }
    s
}

// ---------------------------------------------------------------- buttons

const TOOLBAR: &[&str] = &["#b-back", "#b-fwd", "#b-reload", "#b-home", "#b-star", "#b-shield", "#b-key", "#b-menu", "#new-tab", "#b-zoom", "#b-lock"];
const POPUP_ITEMS: &str = "#popup-body button, #popup-body input[type=checkbox]";
/// Not clicked: they open a system dialog nobody can close on a build
/// machine, or end the session (tested elsewhere or by reading the code).
const SKIP: &[&str] = &["Imprimer", "Enregistrer la page", "Verrouillage d'urgence", "Redémarrer pour mettre à jour"];

/// Click one element and log its effect.
fn click_one(zone: &'static str, v: View, sel: String, idx: usize) -> Seq {
    let mut s = Seq::default();
    let sel2 = sel.clone();
    s.run(move || query_rect(v, &sel2, idx));
    s.until("élément", 3000.0, || console_has("diag-rect "));
    s.run(move || {
        let r = console_take("diag-rect ");
        let label = r["t"].as_str().filter(|t| !t.is_empty()).or(r["title"].as_str()).unwrap_or("").to_string();
        let skip = SKIP.iter().any(|k| label.starts_with(k));
        let state = if r.is_null() || r["ok"].as_bool() != Some(true) {
            if r["hid"] == true { "masque" } else { "absent" }
        } else if r["dis"] == true {
            "desactive"
        } else if skip {
            "non_clique"
        } else {
            "clique"
        };
        let (x, y) = (r["x"].as_f64().unwrap_or(-1.0), r["y"].as_f64().unwrap_or(-1.0));
        if let Ok(mut g) = TRIAL.lock() {
            *g = Some(Trial { x: if state == "clique" { x } else { -1.0 }, y, ok: state == "clique", ..Default::default() });
        }
        if let Ok(mut l) = LABEL.lock() {
            *l = (label, state.to_string());
        }
        save_snap("bouton");
        mark("bouton");
        if state == "clique" {
            focus_view(v);
            let b = view_browser(v);
            mouse(&b, "mouseMoved", x, y, "none");
            mouse(&b, "mousePressed", x, y, "left");
        }
    });
    s.wait(90);
    s.run(move || {
        let t = trial();
        if t.ok {
            mouse(&view_browser(v), "mouseReleased", t.x, t.y, "left");
        }
    });
    s.wait(1500);
    s.run(move || {
        let (label, state) = LABEL.lock().map(|l| l.clone()).unwrap_or_default();
        let effect = if state == "clique" { diff(&saved_snap("bouton"), &snap()) } else { "-".into() };
        let api = if state == "clique" { apis_since(marked("bouton")) } else { "-".into() };
        dlog(format!("bouton zone={zone} selecteur={} n={idx} libelle={} etat={state} api={api} effet={effect}", clean(&sel), clean(&label)));
    });
    s
}

static LABEL: Mutex<(String, String)> = Mutex::new((String::new(), String::new()));

/// A dropdown of the window, opened by NIGHTFALL itself (its content is
/// the same as when opened from its button).
fn open_panel(s: &mut Seq, panel: &'static str) {
    s.run(move || {
        mark("panneau");
        let anchor = cef::Rect { x: 700, y: 44, width: 34, height: 34 };
        match panel {
            "find" => super::browsing::open_find_bar(W),
            "tabmenu" => {
                let ids = tab_ids();
                let id = ids.get(1).copied().unwrap_or(0);
                let flags = if ids.len() > 2 { "ro" } else { "o" };
                let opts = windows::PopupOpts { align_left: true, toggle: false, ..Default::default() };
                windows::popup_open_ex(W, &format!("tabmenu-{id}-{flags}"), anchor, 290, 360, opts);
            }
            "menu" => windows::popup_open(W, "menu", anchor, 340, 640),
            "shield" => windows::popup_open(W, "shield", anchor, 320, 330),
            "passwords" => windows::popup_open(W, "passwords", anchor, 340, 380),
            _ => windows::popup_open(W, panel, anchor, 270, 60),
        }
    });
    s.until("panneau affiché", 6000.0, move || popup_ready(panel, "panneau"));
    s.wait(400);
}

/// Every element of a dropdown, each clicked in a freshly opened dropdown.
fn panel_items(panel: &'static str, tabs: usize) -> Seq {
    let mut s = reset_seq(tabs);
    open_panel(&mut s, panel);
    s.run(|| query_rect(View::Popup, POPUP_ITEMS, 0));
    s.until("éléments du panneau", 3000.0, || console_has("diag-rect "));
    s.run(move || {
        let n = console_take("diag-rect ")["n"].as_u64().unwrap_or(0) as usize;
        dlog(format!("panneau nom={panel} elements={n}"));
        let mut more = Seq::default();
        for i in 0..n {
            more.then(reset_seq(tabs));
            open_panel(&mut more, panel);
            more.then(click_one(panel, View::Popup, POPUP_ITEMS.to_string(), i));
        }
        plan_front(more);
    });
    s
}

fn buttons() -> Seq {
    let mut s = Seq::default();
    s.run(|| dlog("etape boutons".into()));
    // "Accueil" (and the menu's internal pages opened in the current tab)
    // go through windows::navigate_active on a tab showing a web page.
    s.then(reset_seq(2));
    s.run(|| windows::navigate_active(W, windows::NEW_TAB_URL));
    s.wait(2500);
    s.run(|| {
        let url = windows::active_url(W).unwrap_or_default();
        dlog(format!("navigation_interne_depuis_page_web cible=newtab page_finale={} reussi={}", clean(&kind_of(&url)), url.starts_with("nightfall://") as u8));
    });
    // An active tab with history, so that "Précédent" can be used.
    s.then(reset_seq(2));
    s.run(|| windows::navigate_active(W, "https://example.com/?nf-diag-historique"));
    s.until("historique", 15000.0, || all_loaded() && windows::active_url(W).unwrap_or_default().contains("historique"));
    for sel in TOOLBAR {
        s.then(reset_seq(2));
        s.then(click_one("barre", View::Top, sel.to_string(), 0));
    }
    s.then(panel_items("menu", 2));
    s.then(panel_items("tabmenu", 3));
    s.then(panel_items("shield", 2));
    s.then(panel_items("passwords", 2));
    s.then(panel_items("zoom", 2));
    s.then(panel_items("find", 2));
    s
}

// -------------------------------------------------------------- page loads

const CONFIGS: &[(&str, bool, bool, &str)] = &[
    ("defaut", true, true, "system"),
    ("sans_shield", false, true, "system"),
    ("sans_https_first", true, false, "system"),
    ("dns_off", true, true, "off"),
    ("doh_quad9", true, true, "quad9"),
    ("tout_off", false, false, "off"),
];

const PAGES: &[(&str, &str)] = &[
    ("example_http", "http://example.com/"),
    ("wikipedia", "https://en.wikipedia.org/wiki/Web_browser"),
    ("lemonde", "https://www.lemonde.fr/"),
    ("bbc", "https://www.bbc.com/news"),
];

static BENCH: AtomicU32 = AtomicU32::new(0);

fn apply_config(shield: bool, https_first: bool, dns: &str) {
    state::shield().set_enabled(shield);
    let mut s = core().settings.clone();
    s.https_first = https_first;
    s.secure_dns = dns.to_string();
    s.auto_lock_minutes = 0;
    let r = call("settings.set", json!({ "settings": s }));
    if r["ok"] != true {
        dlog(format!("erreur configuration={}", clean(&r.to_string())));
    }
}

fn bench_loaded() -> bool {
    let id = BENCH.load(Relaxed) as i64;
    events_since(marked("nav")).iter().any(|e| e.kind == "load_end" && e.n == id && !e.s.starts_with("0 ") && !e.s.contains("about:blank"))
}

fn page_load(rep: u32, cfg: &'static str, page: &'static str, url: &'static str) -> Seq {
    let mut s = Seq::default();
    s.run(|| windows::navigate_active(W, "about:blank"));
    s.wait(300);
    s.until("page vide", 10000.0, all_loaded);
    s.run(move || {
        // Cold HTTP cache for every load (connections and DNS answers may
        // still be reused: Chromium offers no way to drop them here).
        devtools(&windows::active_browser(W), "Network.clearBrowserCache", json!({}));
        js_in(top(W), JS_UI_READ);
    });
    s.wait(400);
    s.run(move || {
        console_clear("diag-ui ");
        console_clear("diag-nav ");
        reset_counters();
        BENCH.store(windows::active_tab_id(W).unwrap_or(0), Relaxed);
        mark("nav");
        windows::navigate_active(W, url);
    });
    s.until("chargement de la page", 45000.0, bench_loaded);
    s.run(|| {
        mark("nav_fin");
    });
    s.wait(1500);
    s.run(|| {
        js_in(windows::active_browser(W), JS_NAV);
        js_in(top(W), JS_UI_READ);
    });
    s.until("mesures de la page", 4000.0, || console_has("diag-nav ") && console_has("diag-ui "));
    s.run(move || {
        let t0 = marked("nav");
        let total = marked("nav_fin") - t0;
        let loaded = bench_loaded();
        let n = console_take("diag-nav ");
        let ui = console_take("diag-ui ");
        let evs = events_since(t0);
        let status = evs.iter().find(|e| e.kind == "load_end" && e.n == BENCH.load(Relaxed) as i64).map(|e| e.s.split(' ').next().unwrap_or("").to_string()).unwrap_or_default();
        let pushes = evs.iter().filter(|e| e.kind == "push" && e.s == "tabs").count();
        let push_bytes: i64 = evs.iter().filter(|e| e.kind == "push" && e.s == "tabs").map(|e| e.n).sum();
        let info = evs.iter().filter(|e| e.kind == "api" && e.s == "ui:page.info").count();
        let tabs_ev = &ui["ev"]["tabs"];
        let f = |v: &Value| v.as_f64().map(|x| format!("{x:.0}")).unwrap_or_else(|| "-".into());
        let ns = |a: &AtomicU64| a.load(Relaxed) as f64 / 1e6;
        dlog(format!(
            "nav config={cfg} page={page} essai={rep} charge={} statut={status} hote={} total_ms={} ttfb_ms={} dcl_ms={} load_ms={} fcp_ms={} lcp_ms={} ressources={} octets={} doc_octets={} protocole={} shield_requetes={} shield_bloquees={} shield_total_ms={:.2} shield_max_ms={:.3} script_page_n={} script_page_ms={:.2} script_page_octets={} push_tabs={pushes} push_tabs_octets={push_bytes} page_info={info} rendu_onglets_n={} rendu_onglets_ms={} rendu_onglets_max_ms={}",
            loaded as u8,
            clean(n["host"].as_str().unwrap_or("-")),
            if loaded { format!("{total:.0}") } else { "-".into() },
            f(&n["ttfb"]),
            f(&n["dcl"]),
            f(&n["load"]),
            f(&n["fcp"]),
            f(&n["lcp"]),
            f(&n["res"]),
            f(&n["tr"]),
            f(&n["doc"]),
            clean(n["proto"].as_str().unwrap_or("-")),
            SH_N.load(Relaxed),
            SH_BLOCKED.load(Relaxed),
            ns(&SH_NS),
            ns(&SH_MAX),
            PS_N.load(Relaxed),
            ns(&PS_NS),
            PS_BYTES.load(Relaxed),
            f(&tabs_ev["n"]),
            tabs_ev["ms"].as_f64().map(|x| format!("{x:.1}")).unwrap_or_else(|| "-".into()),
            tabs_ev["max"].as_f64().map(|x| format!("{x:.1}")).unwrap_or_else(|| "-".into()),
        ));
    });
    s
}

fn page_loads() -> Seq {
    let mut s = reset_seq(1);
    s.run(|| dlog("etape chargements".into()));
    // Configurations interleaved in every round, so that a slow network
    // period does not penalise one configuration only.
    for rep in 0..5 {
        for (cfg, shield, https_first, dns) in CONFIGS {
            let (cfg, shield, https_first, dns) = (*cfg, *shield, *https_first, *dns);
            s.run(move || {
                apply_config(shield, https_first, dns);
                dlog(format!("configuration nom={cfg} shield={} https_first={} dns={dns}", shield as u8, https_first as u8));
            });
            s.wait(1500);
            for (page, url) in PAGES {
                let (page, url) = (*page, *url);
                s.then(page_load(rep, cfg, page, url));
            }
        }
    }
    s.run(|| apply_config(true, true, "system"));
    s
}

// ------------------------------------------------------------------ memory

const ARTICLES: &[&str] = &[
    "Rust_(programming_language)", "Chromium_(web_browser)", "Firefox", "Safari_(web_browser)", "Opera_(web_browser)",
    "Vivaldi_(web_browser)", "Brave_(web_browser)", "Microsoft_Edge", "WebKit", "Blink_(browser_engine)",
    "Gecko_(software)", "JavaScript", "HTML", "CSS", "HTTP", "Transport_Layer_Security", "Domain_Name_System",
    "DNS_over_HTTPS", "Ad_blocking", "Web_tracking",
];

fn mem_point(s: &mut Seq, label: &'static str) {
    s.wait(4000);
    s.run(move || {
        dlog(format!("mem-demande etiquette={label} onglets={} navigateurs_vivants={}", tab_ids().len(), browsers_alive()));
    });
    // ci/diag_runner.py samples the processes during this pause.
    s.wait(9000);
}

fn memory() -> Seq {
    let mut s = reset_seq(1);
    s.run(|| dlog("etape memoire".into()));
    mem_point(&mut s, "1_onglet");
    for (batch, label) in [(0usize, "10_onglets"), (10, "20_onglets")] {
        s.run(move || {
            for a in &ARTICLES[batch..batch + 10] {
                windows::add_tab(W, &format!("https://en.wikipedia.org/wiki/{a}"), false);
            }
        });
        s.until("articles chargés", 90000.0, all_loaded);
        mem_point(&mut s, label);
    }
    s.run(|| {
        for id in tab_ids().into_iter().skip(1) {
            windows::close_tab(W, id);
        }
    });
    mem_point(&mut s, "apres_fermeture");
    s.wait(20000);
    mem_point(&mut s, "apres_fermeture_30s");
    s
}

// --------------------------------------------- H1: shortcuts while locked

const P: u32 = 1; // Ctrl on Windows, Cmd on macOS
const SH: u32 = 2;
const ALT: u32 = 4;
const CTRL: u32 = 8;

fn dt_mods(f: u32) -> u32 {
    let mac = cfg!(target_os = "macos");
    let mut m = 0;
    if f & P != 0 {
        m |= if mac { 4 } else { 2 };
    }
    if f & SH != 0 {
        m |= 8;
    }
    if f & ALT != 0 {
        m |= 1;
    }
    if f & CTRL != 0 {
        m |= 2;
    }
    m
}

fn cef_mods(f: u32) -> u32 {
    let mac = cfg!(target_os = "macos");
    let mut m = 0;
    if f & P != 0 {
        m |= if mac { 1 << 7 } else { 1 << 2 };
    }
    if f & SH != 0 {
        m |= 1 << 1;
    }
    if f & ALT != 0 {
        m |= 1 << 3;
    }
    if f & CTRL != 0 {
        m |= 1 << 2;
    }
    m
}

fn key_devtools(b: &Option<Browser>, code: i32, mods: u32) {
    for t in ["rawKeyDown", "keyUp"] {
        devtools(b, "Input.dispatchKeyEvent", json!({ "type": t, "modifiers": dt_mods(mods), "windowsVirtualKeyCode": code, "nativeVirtualKeyCode": code }));
    }
}

/// The other injection path: CefBrowserHost::SendKeyEvent.
fn key_native(b: &Option<Browser>, code: i32, mods: u32, mac_code: i32, ch: char) {
    let Some(h) = b.as_ref().and_then(|b| b.host()) else { return };
    let mut e = cef::KeyEvent::default();
    e.type_ = cef::KeyEventType::RAWKEYDOWN;
    e.modifiers = cef_mods(mods);
    e.windows_key_code = code;
    e.native_key_code = if cfg!(target_os = "macos") { mac_code } else { 0 };
    e.character = ch as u16;
    e.unmodified_character = ch as u16;
    h.send_key_event(Some(&e));
    e.type_ = cef::KeyEventType::KEYUP;
    h.send_key_event(Some(&e));
}

fn h1_shortcuts() -> Vec<(&'static str, i32, u32)> {
    let mac = cfg!(target_os = "macos");
    vec![
        ("nouvel_onglet", 0x54, P),
        ("fermer_onglet", 0x57, P),
        ("onglet_suivant", 0x09, CTRL),
        ("onglet_2", 0x32, P),
        ("rouvrir_onglet", 0x54, P | SH),
        ("actualiser", 0x52, P),
        ("zoom_avant", 0xBB, P),
        ("code_source", 0x55, P),
        ("rechercher", 0x46, P),
        ("historique", if mac { 0x59 } else { 0x48 }, P),
        ("telechargements", 0x4A, P),
        ("page_favoris", 0x4F, P | SH),
        ("ajout_favori", 0x44, P),
        ("barre_adresse", 0x4C, P),
        ("outils_dev", if mac { 0x49 } else { 0x7B }, if mac { P | ALT } else { 0 }),
        ("plein_ecran", if mac { 0x46 } else { 0x7A }, if mac { P | CTRL } else { 0 }),
        ("nouvelle_fenetre", 0x4E, P),
        ("fenetre_privee", 0x4E, P | SH),
    ]
}

fn h1_report(name: &'static str, method: &'static str) {
    let before = saved_snap("h1");
    let after = snap();
    let keys: Vec<Ev> = events_since(marked("h1")).into_iter().filter(|e| e.kind == "touche").collect();
    let actions: Vec<String> = keys.iter().filter_map(|e| e.s.split_once(':').map(|x| x.1.to_string())).filter(|a| a != "-").collect();
    dlog(format!(
        "h1 raccourci={name} methode={method} verrouille_avant={} verrouille_apres={} gestionnaire_appels={} action={} effet={}",
        before.locked as u8,
        after.locked as u8,
        keys.len(),
        if actions.is_empty() { "-".to_string() } else { actions.join(",") },
        diff(&before, &after)
    ));
}

fn h1() -> Seq {
    let mut s = reset_seq(4);
    s.run(|| {
        dlog("etape h1".into());
        api::lock_now();
    });
    s.until("verrouillage", 15000.0, fully_locked);
    s.wait(2500);
    for (name, code, mods) in h1_shortcuts() {
        s.run(move || {
            save_snap("h1");
            mark("h1");
            key_devtools(&top(W), code, mods);
        });
        s.wait(1500);
        s.run(move || h1_report(name, "devtools"));
    }
    // Same key through CefBrowserHost::SendKeyEvent (kVK_ANSI_T = 0x11).
    s.run(|| {
        save_snap("h1");
        mark("h1");
        key_native(&top(W), 0x54, P, 0x11, 't');
    });
    s.wait(1500);
    s.run(|| h1_report("nouvel_onglet", "send_key_event"));
    // Ctrl/Cmd+Shift+W on the lock screen of the windows opened above.
    s.run(|| {
        save_snap("h1");
        mark("h1");
        for id in windows::window_ids() {
            if id != W {
                key_devtools(&top(id), 0x57, P | SH);
            }
        }
    });
    s.wait(2500);
    s.run(|| h1_report("fermer_fenetre", "devtools"));
    s.run(|| {
        let t = now_ms();
        let r = call("unlock", json!({ "pin": PIN }));
        dlog(format!("deverrouillage ok={} argon2_et_ouverture_ms={:.0}", r["ok"], now_ms() - t));
    });
    s.until("déverrouillage", 15000.0, unlocked);
    s.then(reset_seq(1));
    s
}

// ------------------------------------------------------- JavaScript dialogs

/// No JsDialogHandler in NIGHTFALL: what does confirm() do? (history,
/// bookmarks and passwords pages use confirm() and prompt()).
fn dialogs() -> Seq {
    let mut s = reset_seq(2);
    s.run(|| {
        dlog("etape dialogues".into());
        let b = windows::active_browser(W);
        devtools(&b, "Page.enable", json!({}));
        console_clear("diag-dialog ");
        js_in(b, "setTimeout(function(){var t=performance.now();var r=confirm('NIGHTFALL diagnostic M-0002');console.log('diag-dialog '+JSON.stringify({r:r,ms:performance.now()-t}));},0);");
    });
    s.wait(2500);
    s.run(|| {
        let waiting = !console_has("diag-dialog ");
        dlog(format!("dialogue confirm en_attente_apres_2500ms={}", waiting as u8));
        devtools(&windows::active_browser(W), "Page.handleJavaScriptDialog", json!({ "accept": false }));
    });
    s.until("réponse du dialogue", 5000.0, || console_has("diag-dialog "));
    s.run(|| {
        let r = console_take("diag-dialog ");
        dlog(format!("dialogue confirm resultat={} duree_ms={}", r["r"], r["ms"].as_f64().map(|x| format!("{x:.0}")).unwrap_or_else(|| "-".into())));
    });
    s
}

// ------------------------------------------------------------------ finish

fn finish() -> Seq {
    let mut s = Seq::default();
    s.run(|| {
        if let Ok(m) = API_COST.lock() {
            for (cmd, (n, total, max)) in m.iter() {
                dlog(format!("api_cout commande={} appels={n} total_ms={total:.1} moyenne_ms={:.2} max_ms={max:.1}", clean(cmd), total / *n as f64));
            }
        }
        let mut by_type: BTreeMap<String, (u64, i64)> = BTreeMap::new();
        if let Ok(v) = EVENTS.lock() {
            for e in v.iter().filter(|e| e.kind == "push") {
                let x = by_type.entry(e.s.clone()).or_insert((0, 0));
                x.0 += 1;
                x.1 += e.n;
            }
            let created = v.iter().filter(|e| e.kind == "cree").count();
            let closed = v.iter().filter(|e| e.kind == "ferme").count();
            dlog(format!("navigateurs crees={created} detruits={closed} vivants={}", browsers_alive()));
        }
        for (t, (n, bytes)) in by_type {
            dlog(format!("push type={} nombre={n} octets={bytes}", clean(&t)));
        }
        dlog("FIN".into());
    });
    s.wait(2000);
    s.run(|| {
        for id in windows::window_ids() {
            windows::close_window(id);
        }
    });
    s
}

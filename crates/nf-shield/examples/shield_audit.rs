//! Shield audit (mission M-0003): measurements only, never part of the
//! browser (Cargo examples are not built into NIGHTFALL).
//!
//! Usage: `cargo run --release -p nf-shield --example shield_audit -- INPUTS`
//! where INPUTS was filled by `ci/shield_inputs.py` (lists, `lists.json`,
//! `requests.json`) and optionally by `ci/shield_crawl.js` (`crawl.ndjson`,
//! `pages.json`). Prints a Markdown report between `NFAUDIT-BEGIN` and
//! `NFAUDIT-END`.
//!
//! Every number comes from this run: list texts (with their SHA-256 in the
//! manifest), the engine of the `adblock` crate NIGHTFALL uses, and
//! NIGHTFALL's own `Shield` API for the "current configuration" figures.

use adblock::lists::{parse_filter, FilterFormat, FilterSet, ParseOptions, ParsedLine};
use adblock::request::Request;
use adblock::resources::{MimeType, Resource, ResourceType};
use adblock::Engine;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use nf_shield::{scriptlets, site_of, Shield, ShieldConfig};
use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

// ------------------------------------------------------------ allocations

/// Counts live heap bytes and the peak since the last reset.
struct Counting;
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(l) };
        if !p.is_null() {
            let now = LIVE.fetch_add(l.size(), Ordering::Relaxed) + l.size();
            PEAK.fetch_max(now, Ordering::Relaxed);
        }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) };
        LIVE.fetch_sub(l.size(), Ordering::Relaxed);
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, new: usize) -> *mut u8 {
        let q = unsafe { System.realloc(p, l, new) };
        if !q.is_null() {
            if new >= l.size() {
                let now = LIVE.fetch_add(new - l.size(), Ordering::Relaxed) + new - l.size();
                PEAK.fetch_max(now, Ordering::Relaxed);
            } else {
                LIVE.fetch_sub(l.size() - new, Ordering::Relaxed);
            }
        }
        q
    }
}

#[global_allocator]
static ALLOC: Counting = Counting;

fn live() -> usize {
    LIVE.load(Ordering::Relaxed)
}
fn reset_peak() {
    PEAK.store(live(), Ordering::Relaxed);
}
fn peak() -> usize {
    PEAK.load(Ordering::Relaxed)
}

// ------------------------------------------------------------ helpers

fn mib(b: usize) -> String {
    format!("{:.1} Mio", b as f64 / 1048576.0)
}

fn ms(ns: u128) -> String {
    format!("{:.1} ms", ns as f64 / 1e6)
}

/// (median, min, max) of durations in nanoseconds.
fn stats(v: &[u128]) -> (u128, u128, u128) {
    let mut s = v.to_vec();
    s.sort_unstable();
    (s[s.len() / 2], s[0], s[s.len() - 1])
}

fn pct(sorted: &[u64], p: f64) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let i = ((sorted.len() as f64 - 1.0) * p).round() as usize;
    sorted[i.min(sorted.len() - 1)]
}

fn latency_row(name: &str, v: &mut [u64]) -> String {
    v.sort_unstable();
    let mean = v.iter().map(|x| *x as f64).sum::<f64>() / v.len().max(1) as f64;
    format!(
        "| {name} | {} | {:.2} | {:.2} | {:.2} | {:.2} | {:.2} | {:.1} | {:.2} |",
        v.len(),
        pct(v, 0.5) as f64 / 1000.0,
        pct(v, 0.9) as f64 / 1000.0,
        pct(v, 0.99) as f64 / 1000.0,
        pct(v, 0.999) as f64 / 1000.0,
        mean / 1000.0,
        v.last().copied().unwrap_or(0) as f64 / 1000.0,
        v.iter().sum::<u64>() as f64 / 1e6,
    )
}

const LATENCY_HEADER: &str = "| Chemin | Requêtes | Médiane (µs) | p90 (µs) | p99 (µs) | p99,9 (µs) | Moyenne (µs) | Max (µs) | Total (ms) |\n|---|---|---|---|---|---|---|---|---|";

fn short(s: &str, n: usize) -> String {
    let t: String = s.chars().take(n).collect();
    let t = t.replace('|', "\\|");
    if s.chars().count() > n {
        format!("{t}…")
    } else {
        t
    }
}

fn top(map: &BTreeMap<String, usize>, n: usize) -> Vec<(String, usize)> {
    let mut v: Vec<(String, usize)> = map.iter().map(|(k, v)| (k.clone(), *v)).collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    v.truncate(n);
    v
}

// ------------------------------------------------------------ inputs

#[derive(serde::Deserialize, Clone)]
#[allow(dead_code)]
struct ListEntry {
    id: String,
    url: String,
    trusted: bool,
    group: String,
    format: String,
    enabled_by_default: bool,
    #[serde(default)]
    sha256: Option<String>,
    #[serde(default)]
    bytes: Option<usize>,
    #[serde(default)]
    includes_found: usize,
    #[serde(default)]
    includes_resolved: usize,
    #[serde(default)]
    error: Option<String>,
}

struct Lists {
    dir: PathBuf,
    entries: Vec<ListEntry>,
}

impl Lists {
    fn entry(&self, id: &str) -> Option<&ListEntry> {
        self.entries.iter().find(|e| e.id == id && e.error.is_none())
    }
    /// List text; `resolved` takes the `!#include`-resolved file when present.
    fn text(&self, id: &str, resolved: bool) -> Option<String> {
        self.entry(id)?;
        if resolved {
            if let Ok(t) = std::fs::read_to_string(self.dir.join(format!("{id}.resolved.txt"))) {
                return Some(t);
            }
        }
        std::fs::read_to_string(self.dir.join(format!("{id}.txt"))).ok()
    }
    fn opts(&self, id: &str) -> ParseOptions {
        let e = self.entry(id);
        let mut o = ParseOptions::default();
        if e.map(|e| e.trusted).unwrap_or(false) {
            o.permissions = scriptlets::TRUSTED;
        }
        if e.map(|e| e.format == "hosts").unwrap_or(false) {
            o.format = FilterFormat::Hosts;
        }
        o
    }
}

#[derive(serde::Deserialize)]
struct Req {
    #[serde(rename = "frameUrl")]
    frame_url: String,
    url: String,
    cpt: String,
    #[serde(default)]
    page: Option<String>,
}

/// One request as NIGHTFALL sees it: adblock type string, source, page.
struct R {
    /// Line number in the input file (joins uBO's reference verdicts).
    line: usize,
    url: String,
    source: String,
    page: String,
    rtype: &'static str,
}

fn map_type(cpt: &str) -> &'static str {
    match cpt {
        "document" | "main_frame" => "main_frame",
        "sub_frame" | "subdocument" => "sub_frame",
        "script" => "script",
        "image" | "imageset" => "image",
        "stylesheet" => "stylesheet",
        "font" => "font",
        "media" => "media",
        "xhr" | "fetch" | "xmlhttprequest" => "xmlhttprequest",
        "ping" | "beacon" => "ping",
        "websocket" => "websocket",
        "object" => "object",
        _ => "other",
    }
}

fn load_requests(path: &Path) -> Vec<R> {
    let Ok(text) = std::fs::read_to_string(path) else { return Vec::new() };
    text.split('\n')
        .enumerate()
        .filter_map(|(i, l)| serde_json::from_str::<Req>(l).ok().map(|r| (i, r)))
        .map(|(line, r)| {
            let page = r.page.clone().unwrap_or_else(|| r.frame_url.clone());
            R { line, rtype: map_type(&r.cpt), url: r.url, source: r.frame_url, page }
        })
        .collect()
}

#[derive(serde::Deserialize)]
struct PageRec {
    page: String,
    #[serde(rename = "finalUrl")]
    final_url: String,
    status: u16,
    requests: usize,
    #[serde(default)]
    classes: Vec<String>,
    #[serde(default)]
    ids: Vec<String>,
    #[serde(default)]
    error: String,
}

// ------------------------------------------------------------ list syntax

/// Network options known to adblock-rust 0.13.5
/// (src/filters/abstract_network.rs, `parse_filter_options`).
const KNOWN_OPTIONS: &[&str] = &[
    "domain", "from", "to", "badfilter", "important", "match-case", "third-party", "3p", "first-party", "1p", "tag",
    "redirect", "rewrite", "redirect-rule", "csp", "removeparam", "generichide", "ghide", "document", "doc", "image",
    "media", "object", "object-subrequest", "other", "ping", "beacon", "script", "stylesheet", "css", "subdocument",
    "frame", "xmlhttprequest", "xhr", "websocket", "font", "all", "method",
];

/// Procedural operators and actions adblock-rust parses (and NIGHTFALL's
/// `cosmetic.js` implements), plus standard functional CSS pseudo-classes.
const KNOWN_PSEUDO: &[&str] = &[
    "has-text", "matches-attr", "matches-css", "matches-css-before", "matches-css-after", "matches-path",
    "min-text-length", "upward", "xpath", "remove", "style", "remove-attr", "remove-class", "-abp-has", "-abp-contains",
    "not", "is", "where", "has", "nth-child", "nth-last-child", "nth-of-type", "nth-last-of-type", "lang", "dir",
];

/// Text of the options of a network rule (after the last `$`).
fn options_of(line: &str) -> Vec<(String, String)> {
    let l = line.strip_prefix("@@").unwrap_or(line);
    let Some(i) = l.rfind('$') else { return Vec::new() };
    l[i + 1..]
        .split(',')
        .filter_map(|o| {
            let o = o.trim().trim_start_matches('~');
            let (name, value) = o.split_once('=').unwrap_or((o, ""));
            let name = name.to_ascii_lowercase();
            if !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
                Some((name, value.to_string()))
            } else {
                None
            }
        })
        .collect()
}

fn pseudo_names(sel: &str) -> Vec<String> {
    let b = sel.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b':' {
            let start = i + 1;
            let mut j = start;
            while j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == b'-') {
                j += 1;
            }
            if j < b.len() && b[j] == b'(' && j > start {
                out.push(sel[start..j].to_ascii_lowercase());
            }
            i = j.max(i + 1);
        } else {
            i += 1;
        }
    }
    out
}

/// Separator of a cosmetic rule (`##`, `#@#`, `#?#`, `#$#`, ...) and what follows.
fn cosmetic_parts(line: &str) -> Option<(&str, &str, &str)> {
    let i = line.find('#')?;
    let rest = &line[i + 1..];
    let j = rest.find('#')?;
    if j > 3 {
        return None;
    }
    Some((&line[..i], &line[i..i + j + 2], &rest[j + 1..]))
}

fn error_name(e: &adblock::lists::FilterParseError) -> String {
    let s = format!("{e:?}");
    let parts: Vec<&str> = s.split('(').take(2).map(|p| p.trim_end_matches(')')).collect();
    parts.join("/")
}

/// Evaluation of `!#if` conditions for NIGHTFALL (Chromium desktop, uBO
/// syntax): true, false, or unknown. Own implementation of the documented
/// semantics (uBO wiki "Static filter syntax", `!#if`).
fn token_true(tok: &str) -> Option<bool> {
    let (neg, t) = match tok.strip_prefix('!') {
        Some(t) => (true, t),
        None => (false, tok),
    };
    let v = match t {
        "env_chromium" | "ext_ublock" | "cap_user_stylesheet" | "adguard_ext_chromium" | "adguard_ext_opera" => true,
        "env_firefox" | "env_safari" | "env_mobile" | "env_mv3" | "env_legacy" | "env_edge" | "env_brave" | "ext_ubol"
        | "ext_devbuild" | "ext_abp" | "false" | "adguard" | "cap_html_filtering" | "cap_ipaddress" => false,
        t if t.starts_with("adguard") || t.starts_with("cap_") => false,
        _ => return None,
    };
    Some(v != neg)
}

fn eval_cond(expr: &str) -> Option<bool> {
    let e = expr.trim().trim_start_matches('(').trim_end_matches(')');
    let mut result: Option<bool> = None;
    let mut op = "";
    for tok in e.split_whitespace() {
        match tok {
            "&&" | "||" => op = if tok == "&&" { "&&" } else { "||" },
            _ => {
                let v = token_true(tok.trim_matches(|c: char| c == '(' || c == ')'))?;
                result = Some(match (result, op) {
                    (None, _) => v,
                    (Some(r), "&&") => r && v,
                    (Some(r), _) => r || v,
                });
            }
        }
    }
    result
}

#[derive(Default)]
struct ListStats {
    lines: usize,
    empty: usize,
    comments: usize,
    directives: usize,
    rules: usize,
    network_ok: usize,
    cosmetic_ok: usize,
    errors: BTreeMap<String, usize>,
    error_example: BTreeMap<String, String>,
    unknown_options: BTreeMap<String, usize>,
    /// Parsed, but without (full) effect in NIGHTFALL today.
    ineffective: BTreeMap<String, usize>,
    redirect_names: BTreeMap<String, usize>,
    scriptlets_total: usize,
    scriptlets_supported: usize,
    scriptlets_untrusted_denied: usize,
    scriptlet_unsupported: BTreeMap<String, usize>,
    scriptlet_used: BTreeMap<String, usize>,
    cosmetic_kinds: BTreeMap<String, usize>,
    in_if_by_cond: BTreeMap<String, usize>,
    in_if_false: usize,
    in_if_unknown: usize,
    includes: usize,
    displayed: usize,
}

struct ScriptletSet {
    names: HashSet<String>,
    trusted: HashSet<String>,
}

fn scriptlet_set() -> ScriptletSet {
    let mut names = HashSet::new();
    let mut trusted = HashSet::new();
    for r in scriptlets::resources() {
        let t = r.permission.to_bits() != 0;
        for n in std::iter::once(r.name.clone()).chain(r.aliases.iter().cloned()) {
            if t {
                trusted.insert(n.clone());
            }
            names.insert(n);
        }
    }
    ScriptletSet { names, trusted }
}

fn with_js(n: &str) -> String {
    if n.ends_with(".js") {
        n.to_string()
    } else {
        format!("{n}.js")
    }
}

fn analyse_list(text: &str, opts: ParseOptions, trusted_list: bool, sc: &ScriptletSet) -> ListStats {
    let mut st = ListStats::default();
    let mut if_stack: Vec<String> = Vec::new();
    for raw in text.lines() {
        st.lines += 1;
        let line = raw.trim();
        if line.is_empty() {
            st.empty += 1;
            continue;
        }
        if !line.starts_with('!') && !line.is_empty() {
            st.displayed += 1; // what `Shield::rebuild` counts as "règles"
        }
        if let Some(d) = line.strip_prefix("!#") {
            st.directives += 1;
            if let Some(c) = d.strip_prefix("if ") {
                if_stack.push(c.trim().to_string());
            } else if d.starts_with("else") {
                if let Some(c) = if_stack.pop() {
                    if_stack.push(format!("!({c})"));
                }
            } else if d.starts_with("endif") {
                if_stack.pop();
            } else if d.starts_with("include") {
                st.includes += 1;
            }
            continue;
        }
        if line.starts_with('!') || line.starts_with('[') || (matches!(opts.format, FilterFormat::Hosts) && line.starts_with('#')) {
            st.comments += 1;
            continue;
        }
        st.rules += 1;
        if let Some(c) = if_stack.last() {
            *st.in_if_by_cond.entry(c.clone()).or_default() += 1;
            let all: Vec<Option<bool>> = if_stack
                .iter()
                .map(|c| {
                    if let Some(inner) = c.strip_prefix("!(").and_then(|x| x.strip_suffix(')')) {
                        eval_cond(inner).map(|v| !v)
                    } else {
                        eval_cond(c)
                    }
                })
                .collect();
            if all.iter().any(|v| *v == Some(false)) {
                st.in_if_false += 1;
            } else if all.iter().any(|v| v.is_none()) {
                st.in_if_unknown += 1;
            }
        }
        match parse_filter(line, false, opts) {
            Ok(ParsedLine::Network(_)) => {
                st.network_ok += 1;
                let exception = line.starts_with("@@");
                let o = options_of(line);
                let has = |n: &str| o.iter().any(|(k, _)| k == n);
                if has("redirect") || has("rewrite") {
                    *st.ineffective.entry("$redirect : bloque sans ressource de substitution".into()).or_default() += 1;
                    for (k, v) in &o {
                        if k == "redirect" || k == "rewrite" {
                            let name = v.rsplit_once(':').filter(|(_, p)| p.parse::<i32>().is_ok()).map(|(n, _)| n).unwrap_or(v);
                            *st.redirect_names.entry(name.to_string()).or_default() += 1;
                        }
                    }
                }
                if has("redirect-rule") {
                    *st.ineffective.entry("$redirect-rule : sans effet".into()).or_default() += 1;
                }
                if has("removeparam") {
                    *st.ineffective.entry("$removeparam : URL réécrite ignorée".into()).or_default() += 1;
                }
                if has("csp") {
                    *st.ineffective.entry("$csp : en-tête CSP jamais appliqué".into()).or_default() += 1;
                }
                if !exception && (has("document") || has("doc")) {
                    *st.ineffective.entry("$document (blocage de page) : jamais vérifié".into()).or_default() += 1;
                }
                if !exception && has("all") {
                    *st.ineffective.entry("$all : partie « document » jamais vérifiée".into()).or_default() += 1;
                }
                if let Some((_, v)) = o.iter().find(|(k, _)| k == "method") {
                    if !v.split('|').any(|m| m.eq_ignore_ascii_case("get")) {
                        *st.ineffective.entry("$method sans GET : jamais vrai (méthode toujours GET)".into()).or_default() += 1;
                    }
                }
                if has("to") {
                    *st.ineffective.entry("$to : analysé mais ignoré au filtrage (moteur)".into()).or_default() += 1;
                }
            }
            Ok(ParsedLine::Cosmetic(_)) => {
                st.cosmetic_ok += 1;
                let (before, sep, after) = cosmetic_parts(line).unwrap_or(("", "##", line));
                let generic = before.is_empty();
                let exception = sep.contains('@');
                if let Some(args) = after.strip_prefix("+js(").and_then(|a| a.strip_suffix(')')) {
                    if exception {
                        *st.cosmetic_kinds.entry("exception de scriptlet #@#+js".into()).or_default() += 1;
                        continue;
                    }
                    st.scriptlets_total += 1;
                    let name = args.split(',').next().unwrap_or("").trim().trim_matches(|c: char| c == '\'' || c == '"');
                    let key = with_js(name);
                    *st.scriptlet_used.entry(key.clone()).or_default() += 1;
                    if !sc.names.contains(&key) {
                        *st.scriptlet_unsupported.entry(key).or_default() += 1;
                    } else if sc.trusted.contains(&key) && !trusted_list {
                        st.scriptlets_untrusted_denied += 1;
                    } else {
                        st.scriptlets_supported += 1;
                    }
                    continue;
                }
                let ps = pseudo_names(after);
                let procedural = ps.iter().any(|p| {
                    ["has-text", "matches-attr", "matches-css", "matches-css-before", "matches-css-after", "matches-path", "min-text-length", "upward", "xpath", "-abp-contains"]
                        .contains(&p.as_str())
                });
                let action = ps.iter().any(|p| ["remove", "style", "remove-attr", "remove-class"].contains(&p.as_str()));
                let kind = match (exception, generic, procedural || action) {
                    (true, _, _) => "exception #@#",
                    (false, true, _) => "masquage générique ##",
                    (false, false, true) => "procédural / action (site)",
                    (false, false, false) => "masquage par site ##",
                };
                *st.cosmetic_kinds.entry(kind.into()).or_default() += 1;
            }
            Err(e) => {
                let mut name = error_name(&e);
                if name == "Network/UnrecognisedOption" {
                    for (k, v) in options_of(line) {
                        if !KNOWN_OPTIONS.contains(&k.as_str()) || (k == "method" && v.is_empty()) {
                            *st.unknown_options.entry(format!("${k}")).or_default() += 1;
                        }
                    }
                } else if name.starts_with("Cosmetic") {
                    if let Some((before, sep, after)) = cosmetic_parts(line) {
                        let detail = if sep.contains('$') && sep.contains('?') {
                            Some("AdGuard #$?#".to_string())
                        } else if sep.contains('$') {
                            Some("ABP/AdGuard #$# (snippets, styles)".to_string())
                        } else if sep.contains('%') {
                            Some("AdGuard #%# (JavaScript)".to_string())
                        } else if after.starts_with('^') {
                            Some("filtrage HTML ##^".to_string())
                        } else if before.starts_with('/') {
                            Some("domaine en expression régulière".to_string())
                        } else if name == "Cosmetic/InvalidCssSelector" || name == "Cosmetic/UnsupportedSyntax" {
                            pseudo_names(after).into_iter().find(|p| !KNOWN_PSEUDO.contains(&p.as_str())).map(|p| format!("opérateur :{p}()"))
                        } else {
                            None
                        };
                        if let Some(d) = detail {
                            name = format!("{name} — {d}");
                        }
                    }
                } else if name == "Unsupported" && line.contains("$$") {
                    name = "Unsupported — AdGuard $$ (HTML)".into();
                }
                st.error_example.entry(name.clone()).or_insert_with(|| short(line, 90));
                *st.errors.entry(name).or_default() += 1;
            }
        }
    }
    st
}

fn merge(into: &mut BTreeMap<String, usize>, from: &BTreeMap<String, usize>) {
    for (k, v) in from {
        *into.entry(k.clone()).or_default() += v;
    }
}

// ------------------------------------------------------------ engines

struct Variant {
    name: String,
    ids: Vec<String>,
    resolved: bool,
}

fn build(lists: &Lists, ids: &[String], resolved: bool, debug: bool, resources: Vec<Resource>) -> Engine {
    let mut set = FilterSet::new(debug);
    for id in ids {
        if let Some(t) = lists.text(id, resolved) {
            set.add_filter_list(t, lists.opts(id));
        }
    }
    let mut e = Engine::new_with_filter_set(set);
    e.use_resources(resources);
    e
}

fn check(e: &Engine, r: &R) -> Option<adblock::blocker::BlockerResult> {
    if r.rtype == "main_frame" || !(r.url.starts_with("http://") || r.url.starts_with("https://") || r.url.starts_with("ws")) {
        return None;
    }
    let req = Request::new(&r.url, &r.source, r.rtype, "GET").ok()?;
    Some(e.check_network_request(&req))
}

fn verdicts(e: &Engine, reqs: &[R]) -> Vec<bool> {
    reqs.iter().map(|r| check(e, r).map(|b| b.should_block()).unwrap_or(false)).collect()
}

fn checkable(reqs: &[R]) -> usize {
    reqs.iter()
        .filter(|r| r.rtype != "main_frame" && (r.url.starts_with("http://") || r.url.starts_with("https://") || r.url.starts_with("ws")))
        .count()
}

// ------------------------------------------------------------ main

fn main() {
    let inputs = PathBuf::from(std::env::args().nth(1).expect("usage: shield_audit INPUTS_DIR"));
    let manifest: Vec<ListEntry> =
        serde_json::from_str(&std::fs::read_to_string(inputs.join("lists.json")).expect("lists.json")).expect("manifest");
    let lists = Lists { dir: inputs.join("lists"), entries: manifest };
    let reps: usize = std::env::var("NF_AUDIT_REPS").ok().and_then(|v| v.parse().ok()).unwrap_or(5);
    let sc = scriptlet_set();

    println!("NFAUDIT-BEGIN");
    println!("# Audit Shield (M-0003) — {} / {}", std::env::consts::OS, std::env::consts::ARCH);
    println!(
        "\nProcesseurs logiques : {}. Profil : {}. Répétitions des constructions : {reps}. adblock-rust : version du Cargo.lock (0.13.x).",
        std::thread::available_parallelism().map(|n| n.get()).unwrap_or(0),
        if cfg!(debug_assertions) { "debug" } else { "release" },
    );

    // ---------------------------------------------------------------- 1. lists
    println!("\n## 1. Listes (texte tel que téléchargé)\n");
    println!("| Liste | Groupe | Activée | Octets | SHA-256 (16) | Lignes | Règles | Réseau OK | Cosmétique OK | Rejetées | % rejetées | « règles » affichées | !#include | dans !#if | !#if faux pour NIGHTFALL |");
    println!("|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|");
    let mut per_list: BTreeMap<String, ListStats> = BTreeMap::new();
    let mut default_total = ListStats::default();
    for e in &lists.entries {
        if let Some(err) = &e.error {
            println!("| {} | {} | {} | ÉCHEC : {} | | | | | | | | | | | |", e.id, e.group, e.enabled_by_default, short(err, 60));
            continue;
        }
        let Some(text) = lists.text(&e.id, false) else { continue };
        let st = analyse_list(&text, lists.opts(&e.id), e.trusted, &sc);
        let rejected: usize = st.errors.values().sum();
        println!(
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {:.1} % | {} | {} (résolus {}) | {} | {} |",
            e.id,
            e.group,
            if e.enabled_by_default { "oui" } else { "non" },
            e.bytes.unwrap_or(0),
            e.sha256.as_deref().map(|s| &s[..16]).unwrap_or(""),
            st.lines,
            st.rules,
            st.network_ok,
            st.cosmetic_ok,
            rejected,
            100.0 * rejected as f64 / st.rules.max(1) as f64,
            st.displayed,
            e.includes_found,
            e.includes_resolved,
            st.in_if_by_cond.values().sum::<usize>(),
            st.in_if_false,
        );
        if e.enabled_by_default {
            default_total.rules += st.rules;
            default_total.network_ok += st.network_ok;
            default_total.cosmetic_ok += st.cosmetic_ok;
            default_total.displayed += st.displayed;
            default_total.scriptlets_total += st.scriptlets_total;
            default_total.scriptlets_supported += st.scriptlets_supported;
            default_total.scriptlets_untrusted_denied += st.scriptlets_untrusted_denied;
            default_total.in_if_false += st.in_if_false;
            default_total.in_if_unknown += st.in_if_unknown;
            merge(&mut default_total.errors, &st.errors);
            merge(&mut default_total.unknown_options, &st.unknown_options);
            merge(&mut default_total.ineffective, &st.ineffective);
            merge(&mut default_total.redirect_names, &st.redirect_names);
            merge(&mut default_total.scriptlet_unsupported, &st.scriptlet_unsupported);
            merge(&mut default_total.scriptlet_used, &st.scriptlet_used);
            merge(&mut default_total.cosmetic_kinds, &st.cosmetic_kinds);
            merge(&mut default_total.in_if_by_cond, &st.in_if_by_cond);
            for (k, v) in &st.error_example {
                default_total.error_example.entry(k.clone()).or_insert_with(|| v.clone());
            }
        }
        per_list.insert(e.id.clone(), st);
    }
    // Resolved uBO lists (what uBlock Origin itself loads).
    let mut resolved_extra = 0usize;
    for e in lists.entries.iter().filter(|e| e.includes_found > 0 && e.error.is_none()) {
        if let Ok(t) = std::fs::read_to_string(lists.dir.join(format!("{}.resolved.txt", e.id))) {
            let st = analyse_list(&t, lists.opts(&e.id), e.trusted, &sc);
            let raw = per_list.get(&e.id).map(|s| s.rules).unwrap_or(0);
            println!(
                "| {} (!#include résolus) | {} | — | {} | — | {} | {} | {} | {} | {} | {:.1} % | — | — | {} | {} |",
                e.id,
                e.group,
                t.len(),
                st.lines,
                st.rules,
                st.network_ok,
                st.cosmetic_ok,
                st.errors.values().sum::<usize>(),
                100.0 * st.errors.values().sum::<usize>() as f64 / st.rules.max(1) as f64,
                st.in_if_by_cond.values().sum::<usize>(),
                st.in_if_false,
            );
            if e.enabled_by_default {
                resolved_extra += st.rules.saturating_sub(raw);
            }
        }
    }
    let rej: usize = default_total.errors.values().sum();
    println!(
        "\nConfiguration par défaut (8 listes activées) : {} règles, {} réseau acceptées, {} cosmétiques acceptées, {} rejetées par le moteur ({:.2} %). L'interface affiche « {} règles ». Règles absentes faute de résolution des `!#include` : {}.",
        default_total.rules,
        default_total.network_ok,
        default_total.cosmetic_ok,
        rej,
        100.0 * rej as f64 / default_total.rules.max(1) as f64,
        default_total.displayed,
        resolved_extra
    );

    println!("\n### 1.1 Règles rejetées par adblock-rust (configuration par défaut), par motif\n");
    println!("| Motif | Règles | Exemple |\n|---|---|---|");
    for (k, v) in top(&default_total.errors, 40) {
        println!("| {k} | {v} | `{}` |", default_total.error_example.get(&k).cloned().unwrap_or_default().replace('`', "'"));
    }
    println!("\n### 1.2 Options réseau inconnues du moteur (règles rejetées pour `UnrecognisedOption`)\n");
    println!("| Option | Occurrences |\n|---|---|");
    for (k, v) in top(&default_total.unknown_options, 30) {
        println!("| `{k}` | {v} |");
    }
    println!("\n### 1.3 Règles acceptées par le moteur mais sans effet (complet) dans NIGHTFALL\n");
    println!("| Cas | Règles |\n|---|---|");
    for (k, v) in top(&default_total.ineffective, 20) {
        println!("| {k} | {v} |");
    }
    println!("\nRessources `$redirect` demandées (aucune n'est fournie par NIGHTFALL) :\n");
    println!("| Ressource | Règles |\n|---|---|");
    for (k, v) in top(&default_total.redirect_names, 25) {
        println!("| `{k}` | {v} |");
    }
    println!(
        "\n### 1.4 Scriptlets `##+js(...)`\n\nRègles de scriptlet : {} ; prises en charge par les scriptlets de NIGHTFALL : {} ({:.1} %) ; refusées faute de confiance de la liste : {} ; nom inconnu : {}. Scriptlets distincts utilisés : {} ; fournis par NIGHTFALL : {}.\n",
        default_total.scriptlets_total,
        default_total.scriptlets_supported,
        100.0 * default_total.scriptlets_supported as f64 / default_total.scriptlets_total.max(1) as f64,
        default_total.scriptlets_untrusted_denied,
        default_total.scriptlet_unsupported.values().sum::<usize>(),
        default_total.scriptlet_used.len(),
        default_total.scriptlet_used.keys().filter(|k| sc.names.contains(*k)).count(),
    );
    println!("| Liste | Scriptlets pris en charge / total |\n|---|---|");
    for e in lists.entries.iter().filter(|e| e.enabled_by_default) {
        if let Some(st) = per_list.get(&e.id) {
            println!("| {} | {} / {} |", e.id, st.scriptlets_supported, st.scriptlets_total);
        }
    }
    println!();
    println!("| Scriptlet absent | Règles |\n|---|---|");
    for (k, v) in top(&default_total.scriptlet_unsupported, 40) {
        println!("| `{k}` | {v} |");
    }
    println!("\n### 1.5 Règles cosmétiques acceptées, par nature\n\n| Nature | Règles |\n|---|---|");
    for (k, v) in top(&default_total.cosmetic_kinds, 10) {
        println!("| {k} | {v} |");
    }
    println!(
        "\n### 1.6 Directives `!#if` (ignorées par adblock-rust : toutes les règles sont chargées)\n\nRègles dans un bloc `!#if` dont la condition est fausse pour NIGHTFALL (Chromium de bureau) : {} ; condition inconnue : {}.\n",
        default_total.in_if_false, default_total.in_if_unknown
    );
    println!("| Condition (bloc le plus intérieur) | Règles |\n|---|---|");
    for (k, v) in top(&default_total.in_if_by_cond, 25) {
        println!("| `{k}` | {v} |");
    }
    println!("\n### 1.7 Listes candidates : rejets et cas sans effet\n");
    println!("| Liste | Règles | Rejetées | Principaux motifs de rejet | Sans effet dans NIGHTFALL | Scriptlets (pris en charge / total) | Refusés (liste non de confiance) | Scriptlets absents les plus utilisés |\n|---|---|---|---|---|---|---|---|");
    for e in lists.entries.iter().filter(|e| e.group == "candidate" || !e.enabled_by_default) {
        let Some(st) = per_list.get(&e.id) else { continue };
        let errs: Vec<String> = top(&st.errors, 3).into_iter().map(|(k, v)| format!("{k} ({v})")).collect();
        let ineff: Vec<String> = top(&st.ineffective, 3).into_iter().map(|(k, v)| format!("{k} ({v})")).collect();
        let missing: Vec<String> = top(&st.scriptlet_unsupported, 4).into_iter().map(|(k, v)| format!("{k} ({v})")).collect();
        println!(
            "| {} | {} | {} | {} | {} | {} / {} | {} | {} |",
            e.id,
            st.rules,
            st.errors.values().sum::<usize>(),
            errs.join(" ; "),
            ineff.join(" ; "),
            st.scriptlets_supported,
            st.scriptlets_total,
            st.scriptlets_untrusted_denied,
            missing.join(" ; ")
        );
    }

    // ---------------------------------------------------------------- 2. engines
    let defaults: Vec<String> = lists.entries.iter().filter(|e| e.enabled_by_default).map(|e| e.id.clone()).collect();
    let has = |id: &str| lists.entry(id).is_some();
    let mut variants = vec![
        Variant { name: "V0 actuel (8 listes)".into(), ids: defaults.clone(), resolved: false },
        Variant { name: "V1 actuel, !#include résolus".into(), ids: defaults.clone(), resolved: true },
        Variant { name: "V2 EasyList + EasyPrivacy".into(), ids: vec!["easylist".into(), "easyprivacy".into()], resolved: false },
    ];
    for c in [
        "fanboy_annoyance",
        "adguard_base",
        "adguard_tracking",
        "adguard_french",
        "ubo_url_tracking",
        "ubo_cookies",
        "ubo_annoyances",
        "easylist_cookie",
        "peter_lowe",
        "urlhaus",
    ] {
        if has(c) {
            let mut ids = defaults.clone();
            ids.push(c.into());
            variants.push(Variant { name: format!("V1 + {c}"), ids, resolved: true });
        }
    }
    let prop: Vec<String> = defaults
        .iter()
        .cloned()
        .chain(["ubo_url_tracking", "ubo_cookies", "easylist_cookie", "peter_lowe", "urlhaus"].iter().filter(|c| has(c)).map(|c| c.to_string()))
        .collect();
    variants.push(Variant { name: "VP proposition (V1 + suivi d'URL, cookies, Peter Lowe, URLhaus)".into(), ids: prop, resolved: true });

    println!("\n## 2. Construction du moteur, mémoire, sérialisation\n");
    println!("Construction = `FilterSet::add_filter_list` de chaque liste + `Engine::new_with_filter_set` + `use_resources` (exactement `Shield::rebuild`, lecture disque exclue). Mémoire : octets du tas comptés par un allocateur instrumenté. Répétitions : {reps}.\n");
    println!("| Variante | Listes | Construction médiane | min | max | Tas conservé | Pic pendant la construction (textes compris) | Taille sérialisée | Désérialisation médiane |\n|---|---|---|---|---|---|---|---|---|");
    let mut engines: Vec<(String, Engine)> = Vec::new();
    for v in &variants {
        let texts: Vec<(String, ParseOptions)> = v.ids.iter().filter_map(|id| lists.text(id, v.resolved).map(|t| (t, lists.opts(id)))).collect();
        let mut times = Vec::new();
        let mut retained = 0;
        let mut peak_b = 0;
        let mut kept: Option<Engine> = None;
        for i in 0..reps.max(1) {
            // The base excludes the list texts: the browser also reads them
            // from disk for each build, and the engine frees them.
            let base = live();
            reset_peak();
            let copies = texts.clone();
            let t0 = Instant::now();
            let mut set = FilterSet::new(false);
            for (t, o) in copies {
                set.add_filter_list(t, o);
            }
            let mut e = Engine::new_with_filter_set(set);
            e.use_resources(scriptlets::resources());
            times.push(t0.elapsed().as_nanos());
            peak_b = peak_b.max(peak().saturating_sub(base));
            retained = live().saturating_sub(base);
            if i == 0 {
                kept = Some(e);
            }
        }
        let e = kept.unwrap();
        let ser = e.serialize();
        let mut dts = Vec::new();
        for _ in 0..reps.max(1) {
            let mut fresh = Engine::default();
            let t0 = Instant::now();
            let ok = fresh.deserialize(&ser).is_ok();
            dts.push(t0.elapsed().as_nanos());
            assert!(ok, "désérialisation");
        }
        let (m, lo, hi) = stats(&times);
        let (dm, _, _) = stats(&dts);
        println!(
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} |",
            v.name,
            v.ids.len(),
            ms(m),
            ms(lo),
            ms(hi),
            mib(retained),
            mib(peak_b),
            mib(ser.len()),
            ms(dm)
        );
        engines.push((v.name.clone(), e));
    }

    // Shield itself, as the browser builds it at start-up (disk read included).
    let dir = tempfile::tempdir().expect("tempdir");
    for id in &defaults {
        if let Some(t) = lists.text(id, false) {
            std::fs::write(dir.path().join(format!("{id}.txt")), t).expect("write");
        }
    }
    let mut st_times = Vec::new();
    let mut shield_retained = 0;
    let mut shield: Option<Shield> = None;
    for _ in 0..reps.max(1) {
        drop(shield.take());
        let base = live();
        let t0 = Instant::now();
        let s = Shield::new(dir.path(), ShieldConfig::default());
        st_times.push(t0.elapsed().as_nanos());
        shield_retained = live().saturating_sub(base);
        shield = Some(s);
    }
    let shield = shield.unwrap();
    let (m, lo, hi) = stats(&st_times);
    println!(
        "\n`Shield::new` (démarrage du navigateur : lecture des 8 fichiers + construction) : médiane {}, min {}, max {} ; tas conservé {} ; `rules_loaded()` = {} (valeur affichée « règles »).",
        ms(m),
        ms(lo),
        ms(hi),
        mib(shield_retained),
        shield.rules_loaded()
    );
    let t0 = Instant::now();
    shield.set_config(shield.config());
    println!("`Shield::set_config` (appelé à chaque déverrouillage et à chaque « Enregistrer » du Security Center) : {}.", ms(t0.elapsed().as_nanos()));

    // ---------------------------------------------------------------- 3. requests
    let dataset = load_requests(&inputs.join("requests.json"));
    let crawl = load_requests(&inputs.join("crawl.ndjson"));
    println!("\n## 3. Temps de décision par requête\n");
    println!(
        "Jeu de requêtes : `requests.json` du dépôt adblock-rust (commit 09d297c, {} requêtes dont {} vérifiables par Shield : les navigations principales ne le sont pas). Chaque appel est chronométré individuellement (`Instant`), sur un seul fil.\n",
        dataset.len(),
        checkable(&dataset)
    );
    println!("{LATENCY_HEADER}");
    for pass in ["1re passe (expressions régulières compilées à la demande)", "2e passe"] {
        let mut v = Vec::with_capacity(dataset.len());
        for r in &dataset {
            if r.rtype == "main_frame" {
                continue;
            }
            let t0 = Instant::now();
            let b = shield.should_block(&r.url, &r.source, r.rtype, &r.page);
            v.push(t0.elapsed().as_nanos() as u64);
            std::hint::black_box(b);
        }
        println!("{}", latency_row(&format!("`Shield::should_block` (V0), {pass}"), &mut v));
    }
    for (name, e) in engines.iter().filter(|(n, _)| n.starts_with("V0") || n.starts_with("V1 actuel") || n.starts_with("VP")) {
        let mut v = Vec::new();
        let mut parse = Vec::new();
        for r in &dataset {
            if r.rtype == "main_frame" {
                continue;
            }
            let t0 = Instant::now();
            let req = Request::new(&r.url, &r.source, r.rtype, "GET");
            parse.push(t0.elapsed().as_nanos() as u64);
            if let Ok(req) = req {
                let t1 = Instant::now();
                let b = e.check_network_request(&req);
                v.push(t1.elapsed().as_nanos() as u64);
                std::hint::black_box(b);
            }
        }
        println!("{}", latency_row(&format!("`Engine::check_network_request` seul, {name}"), &mut v));
        if name.starts_with("V0") {
            println!("{}", latency_row("`Request::new` seul (analyse d'URL + suffixes publics)", &mut parse));
        }
    }
    {
        let mut v = Vec::new();
        for r in dataset.iter().filter(|r| r.rtype != "main_frame").take(50000) {
            let t0 = Instant::now();
            let s = site_of(&r.page);
            v.push(t0.elapsed().as_nanos() as u64);
            std::hint::black_box(s);
        }
        println!("{}", latency_row("`site_of` (appelé 1 à 2 fois par requête par `should_block`)", &mut v));
    }

    // ---------------------------------------------------------------- 4. coverage
    let stats_now = shield.stats();
    println!(
        "\nCompteur de Shield après ces passes : {} requêtes bloquées, {} sites dans `per_site`.",
        stats_now.total_blocked,
        stats_now.per_site.len()
    );
    for (label, reqs) in [("jeu adblock-rust (2019-2020)", &dataset), ("exploration réelle (Playwright, sans blocage)", &crawl)] {
        if reqs.is_empty() {
            println!("\n## 4. Couverture — {label}\n\nNON MESURÉ : fichier absent.");
            continue;
        }
        let total = checkable(reqs);
        let third: Vec<bool> = reqs.iter().map(|r| site_of(&r.url) != site_of(&r.page)).collect();
        println!("\n## 4. Couverture réseau — {label}\n");
        println!("{} requêtes, {} vérifiables (hors navigations principales), dont {} tierces.\n", reqs.len(), total, reqs.iter().zip(&third).filter(|(r, t)| **t && r.rtype != "main_frame").count());
        println!("| Variante | Bloquées | % des vérifiables | Tierces bloquées | Premières parties bloquées | Gagnées vs V0 | Perdues vs V0 |\n|---|---|---|---|---|---|---|");
        let base = verdicts(&engines[0].1, reqs);
        for (name, e) in &engines {
            let v = verdicts(e, reqs);
            let blocked = v.iter().filter(|b| **b).count();
            let tp = v.iter().zip(&third).filter(|(b, t)| **b && **t).count();
            let gained = v.iter().zip(&base).filter(|(a, b)| **a && !**b).count();
            let lost = v.iter().zip(&base).filter(|(a, b)| !**a && **b).count();
            println!(
                "| {name} | {blocked} | {:.2} % | {tp} | {} | {gained} | {lost} |",
                100.0 * blocked as f64 / total.max(1) as f64,
                blocked - tp
            );
        }
        // Per type for V0.
        let mut by_type: BTreeMap<String, (usize, usize)> = BTreeMap::new();
        for (r, b) in reqs.iter().zip(&base) {
            if r.rtype == "main_frame" {
                continue;
            }
            let e = by_type.entry(r.rtype.to_string()).or_default();
            e.0 += 1;
            if *b {
                e.1 += 1;
            }
        }
        println!("\nV0 par type de requête :\n\n| Type | Requêtes | Bloquées | % |\n|---|---|---|---|");
        for (t, (n, b)) in &by_type {
            println!("| {t} | {n} | {b} | {:.1} % |", 100.0 * *b as f64 / (*n).max(1) as f64);
        }
    }

    // ---------------------------------------------------------------- 5. attribution / FP proxy
    let debug_engine = build(&lists, &defaults, false, true, scriptlets::resources());

    // ---------------------------------------------------------------- 4.b uBO reference
    for (label, reqs, file) in [("jeu adblock-rust", &dataset, "ubo-requests.ndjson"), ("exploration réelle", &crawl, "ubo-crawl.ndjson")] {
        let Ok(t) = std::fs::read_to_string(inputs.join(file)) else {
            println!("\n## 4.b Accord avec uBlock Origin — {label}\n\nNON MESURÉ : {file} absent.");
            continue;
        };
        #[derive(serde::Deserialize)]
        struct U {
            i: usize,
            v: i32,
            #[serde(default)]
            f: String,
            #[serde(default)]
            q: u8,
        }
        let ubo: std::collections::HashMap<usize, U> = t.lines().filter_map(|l| serde_json::from_str::<U>(l).ok()).map(|u| (u.i, u)).collect();
        println!("\n## 4.b Accord avec uBlock Origin (moteur réseau statique de uBO, mêmes 8 listes, `!#include` résolus et `!#if` évalués pour Chromium) — {label}\n");
        println!("| Comparaison | Les deux bloquent | uBO seul | NIGHTFALL seul | Aucun | Accord |\n|---|---|---|---|---|---|");
        let mut only_ubo: BTreeMap<String, usize> = BTreeMap::new();
        let mut only_nf: BTreeMap<String, usize> = BTreeMap::new();
        let mut ubo_q = 0usize;
        for (k, (name, e)) in engines.iter().take(2).enumerate() {
            let (mut both, mut u_only, mut n_only, mut none) = (0usize, 0usize, 0usize, 0usize);
            for r in reqs.iter() {
                let Some(u) = ubo.get(&r.line) else { continue };
                let Some(b) = check(e, r) else { continue };
                let nb = b.should_block();
                let ub = u.v == 1;
                match (ub, nb) {
                    (true, true) => both += 1,
                    (true, false) => {
                        u_only += 1;
                        if k == 0 {
                            *only_ubo.entry(u.f.clone()).or_default() += 1;
                        }
                    }
                    (false, true) => {
                        n_only += 1;
                        if k == 0 {
                            let f = check(&debug_engine, r).and_then(|d| d.filter.and_then(|f| f.raw_line)).unwrap_or_else(|| "(règles fusionnées)".into());
                            *only_nf.entry(f).or_default() += 1;
                        }
                    }
                    (false, false) => none += 1,
                }
                if k == 0 && u.q == 1 {
                    ubo_q += 1;
                }
            }
            let n = both + u_only + n_only + none;
            println!("| NIGHTFALL {name} | {both} | {u_only} | {n_only} | {none} | {:.2} % |", 100.0 * (both + none) as f64 / n.max(1) as f64);
        }
        println!("\nRequêtes (navigations comprises) dont uBO nettoierait l'URL par `$removeparam` : {ubo_q}.\n");
        println!("| Règle uBO (bloquée par uBO seul, V0) | Requêtes |\n|---|---|");
        for (k, v) in top(&only_ubo, 20) {
            println!("| `{}` | {v} |", short(&k, 100).replace('`', "'"));
        }
        println!("\n| Règle NIGHTFALL (bloquée par NIGHTFALL seul, V0) | Requêtes |\n|---|---|");
        for (k, v) in top(&only_nf, 20) {
            println!("| `{}` | {v} |", short(&k, 100).replace('`', "'"));
        }
    }
    for (label, reqs) in [("exploration réelle", &crawl), ("jeu adblock-rust", &dataset)] {
        if reqs.is_empty() {
            continue;
        }
        let mut rules_all: BTreeMap<String, usize> = BTreeMap::new();
        let mut rules_fp: BTreeMap<String, usize> = BTreeMap::new();
        let mut fp_types: BTreeMap<String, usize> = BTreeMap::new();
        let mut excepted = 0usize;
        let mut excepted_rules: BTreeMap<String, usize> = BTreeMap::new();
        for r in reqs.iter() {
            let Some(res) = check(&debug_engine, r) else { continue };
            if res.filter.is_some() && res.exception.is_some() && !res.important {
                excepted += 1;
                let x = res.exception.as_ref().and_then(|f| f.raw_line.clone()).unwrap_or_else(|| "?".into());
                *excepted_rules.entry(x).or_default() += 1;
            }
            if !res.should_block() {
                continue;
            }
            let rule = res.filter.as_ref().and_then(|f| f.raw_line.clone()).unwrap_or_else(|| "(règles fusionnées)".into());
            *rules_all.entry(rule.clone()).or_default() += 1;
            if site_of(&r.url) == site_of(&r.page) {
                *rules_fp.entry(rule).or_default() += 1;
                *fp_types.entry(r.rtype.to_string()).or_default() += 1;
            }
        }
        println!("\n## 5. Règles déclenchées — {label} (V0, moteur en mode débogage)\n");
        println!("Requêtes sauvées par une exception `@@` : {excepted}. Blocages de premières parties (même domaine enregistrable que la page, indicateur de risque de faux positif) : {} — par type : {:?}.\n", rules_fp.values().sum::<usize>(), fp_types);
        println!("| Règle (blocages de premières parties) | Requêtes |\n|---|---|");
        for (k, v) in top(&rules_fp, 20) {
            println!("| `{}` | {v} |", short(&k, 100).replace('`', "'"));
        }
        println!("\n| Règle (tous blocages) | Requêtes |\n|---|---|");
        for (k, v) in top(&rules_all, 20) {
            println!("| `{}` | {v} |", short(&k, 100).replace('`', "'"));
        }
        println!("\n| Exception appliquée | Requêtes |\n|---|---|");
        for (k, v) in top(&excepted_rules, 10) {
            println!("| `{}` | {v} |", short(&k, 100).replace('`', "'"));
        }
    }

    // Redirects, removeparam, CSP that uBlock Origin would apply.
    {
        let mut res = scriptlets::resources();
        let have: HashSet<String> = res.iter().flat_map(|r| std::iter::once(r.name.clone()).chain(r.aliases.clone())).collect();
        for n in default_total.redirect_names.keys() {
            if !have.contains(n) {
                res.push(Resource {
                    name: n.clone(),
                    aliases: vec![],
                    kind: ResourceType::Mime(MimeType::from_extension(n)),
                    content: STANDARD.encode(""),
                    dependencies: vec![],
                    permission: Default::default(),
                });
            }
        }
        let with_redirects = build(&lists, &defaults, false, false, res);
        let url_tracking: Vec<String> = defaults.iter().cloned().chain(has("ubo_url_tracking").then(|| "ubo_url_tracking".to_string())).collect();
        let e_rp = build(&lists, &url_tracking, true, false, scriptlets::resources());
        for (label, reqs) in [("exploration réelle", &crawl), ("jeu adblock-rust", &dataset)] {
            if reqs.is_empty() {
                continue;
            }
            let mut redirected = 0;
            let mut redirect_types: BTreeMap<String, usize> = BTreeMap::new();
            let mut rp_v0 = 0;
            let mut rp_more = 0;
            let mut rp_doc = 0;
            let mut csp_docs = 0;
            for r in reqs.iter() {
                if let Some(b) = check(&with_redirects, r) {
                    if b.redirect.is_some() {
                        redirected += 1;
                        *redirect_types.entry(r.rtype.to_string()).or_default() += 1;
                    }
                    if b.rewritten_url.is_some() && !b.should_block() {
                        rp_v0 += 1;
                    }
                }
                if let Some(b) = check(&e_rp, r) {
                    if b.rewritten_url.is_some() && !b.should_block() {
                        rp_more += 1;
                    }
                }
                if r.rtype == "main_frame" {
                    if let Ok(q) = Request::new(&r.url, &r.source, "main_frame", "GET") {
                        if e_rp.check_network_request(&q).rewritten_url.is_some() {
                            rp_doc += 1;
                        }
                        if with_redirects.get_csp_directives(&q).is_some() {
                            csp_docs += 1;
                        }
                    }
                }
            }
            println!(
                "\n### 5.x Effets non appliqués par NIGHTFALL — {label}\n\n- requêtes que uBO servirait par une ressource `$redirect` (NIGHTFALL les annule) : {redirected} — par type : {redirect_types:?}\n- sous-requêtes dont l'URL serait nettoyée par `$removeparam` : {rp_v0} avec V0, {rp_more} avec V1 + ubo_url_tracking\n- navigations principales dont l'URL serait nettoyée (V1 + ubo_url_tracking) : {rp_doc}\n- documents principaux qui recevraient une directive `$csp` (V0) : {csp_docs}"
            );
        }
    }

    // ---------------------------------------------------------------- 6. cosmetic
    println!("\n## 6. Cosmétique (V0, API de `Shield`)\n");
    let mut doc_urls: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for r in dataset.iter().filter(|r| r.rtype == "main_frame") {
        let h = url::Url::parse(&r.url).ok().and_then(|u| u.host_str().map(str::to_string)).unwrap_or_default();
        if !h.is_empty() && seen.insert(h) {
            doc_urls.push(r.url.clone());
        }
        if doc_urls.len() >= 2000 {
            break;
        }
    }
    let mut t_ucr = Vec::new();
    let mut t_ps = Vec::new();
    let mut sizes = Vec::new();
    let mut hide_counts = Vec::new();
    let mut proc_counts = Vec::new();
    let mut with_scriptlets = 0;
    for u in &doc_urls {
        let t0 = Instant::now();
        let res = engines[0].1.url_cosmetic_resources(u);
        t_ucr.push(t0.elapsed().as_nanos() as u64);
        hide_counts.push(res.hide_selectors.len() as u64);
        proc_counts.push(res.procedural_actions.len() as u64);
        if !res.injected_script.is_empty() {
            with_scriptlets += 1;
        }
        let t1 = Instant::now();
        let s = shield.page_script(u, "\u{1}nfcos0000000000000000:");
        t_ps.push(t1.elapsed().as_nanos() as u64);
        sizes.push(s.map(|s| s.len() as u64).unwrap_or(0));
    }
    println!("{} adresses de documents (hôtes distincts du jeu adblock-rust). `page_script` est appelé par `on_load_start` pour chaque cadre (principal et iframes).\n", doc_urls.len());
    println!("{LATENCY_HEADER}");
    println!("{}", latency_row("`Engine::url_cosmetic_resources`", &mut t_ucr));
    println!("{}", latency_row("`Shield::page_script` (1 appel à `url_cosmetic_resources`, tri, CSS, JSON, moteur cosmétique)", &mut t_ps));
    sizes.sort_unstable();
    hide_counts.sort_unstable();
    proc_counts.sort_unstable();
    println!(
        "\nTaille du script injecté par cadre : médiane {} octets, p90 {}, p99 {}, max {}. Sélecteurs de masquage par page : médiane {}, max {}. Règles procédurales par page : médiane {}, max {}. Pages avec scriptlets : {}.",
        pct(&sizes, 0.5),
        pct(&sizes, 0.9),
        pct(&sizes, 0.99),
        sizes.last().copied().unwrap_or(0),
        pct(&hide_counts, 0.5),
        hide_counts.last().copied().unwrap_or(0),
        pct(&proc_counts, 0.5),
        proc_counts.last().copied().unwrap_or(0),
        with_scriptlets
    );
    let blank = engines[0].1.url_cosmetic_resources("https://nightfall-audit.invalid/");
    println!(
        "Page sans règle propre (`nightfall-audit.invalid`) : {} sélecteurs génériques « complexes » injectés dans chaque cadre, script de {} octets.",
        blank.hide_selectors.len(),
        shield.page_script("https://nightfall-audit.invalid/", "T").map(|s| s.len()).unwrap_or(0)
    );

    if let Ok(t) = std::fs::read_to_string(inputs.join("pages.json")) {
        if let Ok(pages) = serde_json::from_str::<Vec<PageRec>>(&t) {
            println!("\n### 6.1 Pages explorées (Playwright) : règles génériques par classes et id\n");
            println!("| Page | HTTP | Requêtes | Bloquées (V0) | Classes | Id | `generic_css` (µs, médiane de 20) | Sélecteurs génériques trouvés | Script injecté (octets) | Procédurales | Scriptlets | Erreur |\n|---|---|---|---|---|---|---|---|---|---|---|---|");
            for p in &pages {
                let url = if p.final_url.starts_with("http") { p.final_url.clone() } else { p.page.clone() };
                let mut ts = Vec::new();
                let mut css = String::new();
                for _ in 0..20 {
                    let t0 = Instant::now();
                    css = shield.generic_css(&url, &p.classes, &p.ids);
                    ts.push(t0.elapsed().as_nanos() as u64);
                }
                ts.sort_unstable();
                let nsel = if css.is_empty() { 0 } else { css.split("{display").next().unwrap_or("").split(',').count() };
                let res = engines[0].1.url_cosmetic_resources(&url);
                let blocked = crawl
                    .iter()
                    .filter(|r| r.page == p.page)
                    .filter(|r| check(&engines[0].1, r).map(|b| b.should_block()).unwrap_or(false))
                    .count();
                println!(
                    "| {} | {} | {} | {} | {} | {} | {:.0} | {} | {} | {} | {} | {} |",
                    short(&p.page, 50),
                    p.status,
                    p.requests,
                    blocked,
                    p.classes.len(),
                    p.ids.len(),
                    pct(&ts, 0.5) as f64 / 1000.0,
                    nsel,
                    shield.page_script(&url, "T").map(|s| s.len()).unwrap_or(0),
                    res.procedural_actions.len(),
                    if res.injected_script.is_empty() { "non" } else { "oui" },
                    short(&p.error, 40)
                );
            }
        }
    }

    // ---------------------------------------------------------------- 7. facts
    println!("\n## 7. Constats exécutés (moteur et API de Shield)\n");
    let fact = |name: &str, filters: &str, f: &dyn Fn(&Shield) -> String| {
        let d = tempfile::tempdir().expect("tempdir");
        let s = Shield::new(d.path(), ShieldConfig { custom_filters: filters.into(), lists: vec![], ..Default::default() });
        println!("- {name} — règles `{}` → {}", filters.replace('\n', " ⏎ "), f(&s));
    };
    let page = "https://news.example/";
    fact("Redirection", "||ads.example^$script,redirect=noopjs", &|s| {
        format!("bloquée : {} (annulée, aucune ressource de substitution)", s.should_block("https://ads.example/a.js", page, "script", page))
    });
    fact("removeparam", "$removeparam=utm_source", &|s| {
        format!(
            "bloquée : {} ; `should_block` ne renvoie qu'un booléen, l'URL réécrite est perdue",
            s.should_block("https://cdn.example/x.js?utm_source=a", page, "script", page)
        )
    });
    fact("Directive !#if false", "!#if false\n||iffalse.example^\n!#endif", &|s| {
        format!("bloquée malgré la condition fausse : {}", s.should_block("https://iffalse.example/x.js", page, "script", page))
    });
    fact("$popup", "||popunder.example^$popup", &|s| format!("règle chargée : {} (`rules_loaded` = {})", s.should_block("https://popunder.example/", page, "sub_frame", page), s.rules_loaded()));
    fact("$document", "||badware.example^$document", &|s| {
        let e = Engine::new_with_list_text("||badware.example^$document");
        let q = Request::new("https://badware.example/", "https://badware.example/", "main_frame", "GET").unwrap();
        format!(
            "moteur : bloquer = {} ; Shield (navigation principale) : {}",
            e.check_network_request(&q).should_block(),
            s.should_block("https://badware.example/", page, "main_frame", page)
        )
    });
    fact("$method=post", "||collect.example^$method=post", &|s| {
        format!("bloquée (Shield passe toujours GET) : {}", s.should_block("https://collect.example/c", page, "xmlhttprequest", page))
    });
    fact("Scriptlet inconnu", "news.example##+js(nonexistent-scriptlet)", &|s| {
        format!("script injecté contient un appel : {}", s.page_script(page, "T").map(|j| j.contains("nonexistent")).unwrap_or(false))
    });
    fact("Témoin générique", "##.ad", &|s| format!("CSS générique pour la classe « ad » : {:?}", s.generic_css(page, &["ad".into()], &[])));
    fact("$elemhide", "@@||news.example^$elemhide\n##.ad", &|s| {
        format!("CSS générique pour « ad » malgré l'exception : {:?}", s.generic_css(page, &["ad".into()], &[]))
    });
    fact("$generichide", "@@||news.example^$generichide\n##.ad", &|s| {
        format!("CSS générique pour « ad » : {:?}", s.generic_css(page, &["ad".into()], &[]))
    });
    fact("$specifichide", "@@||news.example^$specifichide\nnews.example##.pub", &|s| {
        format!("masquage propre au site appliqué malgré l'exception : {}", s.page_script(page, "T").map(|j| j.contains(".pub")).unwrap_or(false))
    });
    fact("Compteur « règles »", "[Adblock Plus 2.0]\n||a.example^$popup\nexample.com#$#abort-on-property-read x\n||b.example^", &|s| {
        format!("`rules_loaded` = {} pour 1 règle réellement chargée", s.rules_loaded())
    });
    fact("Cadre tiers (iframe)", "||widget.example^$third-party\nwidget.example##.inner-ad", &|s| {
        format!(
            "script de l'iframe widget.example contient `.inner-ad` : {}",
            s.page_script("https://widget.example/frame", "T").map(|j| j.contains(".inner-ad")).unwrap_or(false)
        )
    });
    println!("\nNFAUDIT-END");
}

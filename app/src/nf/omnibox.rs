//! Address bar suggestions, computed locally from the encrypted history and
//! bookmarks. The search engine's own suggestions are asked for only when
//! the user turned them on (off by default), never in a private window.

use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Suggestion {
    /// `url` (what was typed is an address), `search`, `bookmark`, `history`.
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub title: String,
    pub url: String,
}

/// A history or bookmark entry offered to the ranking.
pub struct Candidate<'a> {
    pub url: &'a str,
    pub title: &'a str,
    pub visits: i64,
    pub last_visit: i64,
    pub bookmark: bool,
}

/// OpenSearch suggestion address (`%s` = query) of a built-in engine;
/// None for engines without a public one (Qwant, Startpage, custom).
pub fn suggest_endpoint(engine: &str) -> Option<&'static str> {
    Some(match engine {
        "duckduckgo" => "https://duckduckgo.com/ac/?q=%s&type=list",
        "brave" => "https://search.brave.com/api/suggest?q=%s",
        "ecosia" => "https://ac.ecosia.org/autocomplete?q=%s&type=list",
        "google" => "https://suggestqueries.google.com/complete/search?client=firefox&q=%s",
        "bing" => "https://api.bing.com/osjson.aspx?query=%s",
        _ => return None,
    })
}

/// Phrases of an OpenSearch answer `["query", ["phrase", …], …]`: at most
/// `max`, each short and without control characters, the query itself left out.
pub fn parse_opensearch(body: &str, query: &str, max: usize) -> Vec<String> {
    let Ok(serde_json::Value::Array(v)) = serde_json::from_str::<serde_json::Value>(body) else { return Vec::new() };
    let Some(serde_json::Value::Array(list)) = v.get(1) else { return Vec::new() };
    let mut out: Vec<String> = Vec::new();
    for p in list.iter().filter_map(|x| x.as_str()) {
        let p = p.trim();
        if p.is_empty() || p.chars().count() > 120 || p.chars().any(char::is_control) || p.eq_ignore_ascii_case(query.trim()) || out.iter().any(|o| o == p) {
            continue;
        }
        out.push(p.to_string());
        if out.len() >= max {
            break;
        }
    }
    out
}

/// Display name of a search engine id.
pub fn engine_name(id: &str) -> String {
    match id {
        "duckduckgo" => "DuckDuckGo".into(),
        "google" => "Google".into(),
        "brave" => "Brave Search".into(),
        "startpage" => "Startpage".into(),
        "qwant" => "Qwant".into(),
        "ecosia" => "Ecosia".into(),
        "bing" => "Bing".into(),
        "wikipedia" => "Wikipédia".into(),
        other => {
            let mut c = other.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => "le moteur de recherche".into(),
            }
        }
    }
}

/// Address without scheme and "www.", lower case (what people type).
fn bare(url: &str) -> String {
    let l = url.to_lowercase();
    let l = l.strip_prefix("https://").or_else(|| l.strip_prefix("http://")).unwrap_or(&l).to_string();
    l.strip_prefix("www.").map(str::to_string).unwrap_or(l)
}

/// Relevance of a candidate for the typed text (0 = no match).
fn score(text: &str, c: &Candidate, now: i64) -> f64 {
    let t = text.to_lowercase();
    let url = bare(c.url);
    let title = c.title.to_lowercase();
    let mut s = if url.starts_with(&t) {
        100.0
    } else if title.starts_with(&t) {
        60.0
    } else if title.split(|ch: char| !ch.is_alphanumeric()).any(|w| !w.is_empty() && w.starts_with(&t)) {
        45.0
    } else if url.split(['.', '/', '-', '?', '=', '&']).any(|w| !w.is_empty() && w.starts_with(&t)) {
        35.0
    } else if url.contains(&t) || title.contains(&t) {
        15.0
    } else {
        return 0.0;
    };
    // Every typed word must appear somewhere.
    let hay = format!("{url} {title}");
    if !t.split_whitespace().all(|w| hay.contains(w)) {
        return 0.0;
    }
    s += (c.visits.max(1) as f64).ln() * 6.0;
    let age_days = ((now - c.last_visit).max(0) as f64) / 86_400.0;
    s += 12.0 / (1.0 + age_days / 7.0);
    if c.bookmark {
        s += 20.0;
    }
    // Short addresses (home pages) before deep links.
    s -= (url.len() as f64 / 40.0).min(8.0);
    s
}

/// Build the suggestion list.
/// - `typed_url`: the address the input resolves to when it is not a search.
/// - `search_url`: the search engine address for the input.
pub fn suggest(text: &str, typed_url: Option<&str>, search_url: &str, engine: &str, candidates: &[Candidate], now: i64, max: usize) -> Vec<Suggestion> {
    let text = text.trim();
    if text.is_empty() {
        return Vec::new();
    }
    let mut out: Vec<Suggestion> = Vec::new();
    let search = Suggestion { kind: "search", title: format!("Rechercher « {text} » avec {}", engine_name(engine)), url: search_url.to_string() };
    match typed_url {
        Some(u) => {
            out.push(Suggestion { kind: "url", title: u.to_string(), url: u.to_string() });
            out.push(search);
        }
        None => out.push(search),
    }
    let mut ranked: Vec<(f64, &Candidate)> = candidates
        .iter()
        .filter(|c| c.url.starts_with("http://") || c.url.starts_with("https://"))
        .map(|c| (score(text, c, now), c))
        .filter(|(s, _)| *s > 0.0)
        .collect();
    ranked.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    for (_, c) in ranked {
        if out.len() >= max {
            break;
        }
        let same = |u: &str| bare(u).trim_end_matches('/') == bare(c.url).trim_end_matches('/');
        if out.iter().any(|s| s.kind != "search" && same(&s.url)) {
            continue;
        }
        let title = if c.title.trim().is_empty() { c.url.to_string() } else { c.title.to_string() };
        out.push(Suggestion { kind: if c.bookmark { "bookmark" } else { "history" }, title, url: c.url.to_string() });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c<'a>(url: &'a str, title: &'a str, visits: i64, bookmark: bool) -> Candidate<'a> {
        Candidate { url, title, visits, last_visit: 1_000_000, bookmark }
    }

    #[test]
    fn opensearch_answers_are_read_safely() {
        let body = r#"["chat",["chat","chat noir","chat\u0007bip","chat roux","chat noir"]]"#;
        assert_eq!(parse_opensearch(body, "chat", 4), vec!["chat noir", "chat roux"]);
        assert!(parse_opensearch("<html>", "x", 4).is_empty());
        assert!(parse_opensearch(r#"{"a":1}"#, "x", 4).is_empty());
        assert_eq!(parse_opensearch(r#"["a",["a1","a2","a3"]]"#, "a", 2).len(), 2);
        assert!(suggest_endpoint("qwant").is_none() && suggest_endpoint("custom").is_none());
        assert!(suggest_endpoint("duckduckgo").unwrap().starts_with("https://"));
    }

    #[test]
    fn empty_input_gives_nothing() {
        assert!(suggest("  ", None, "https://duckduckgo.com/?q=", "duckduckgo", &[], 0, 8).is_empty());
    }

    #[test]
    fn search_first_then_matches() {
        let cands = [c("https://www.youtube.com/", "YouTube", 40, false), c("https://example.com/", "Example", 1, false), c("https://fr.wikipedia.org/wiki/Yoyo", "Yo-yo — Wikipédia", 2, false)];
        let s = suggest("you", None, "https://duckduckgo.com/?q=you", "duckduckgo", &cands, 1_000_000, 8);
        assert_eq!(s[0].kind, "search");
        assert_eq!(s[0].title, "Rechercher « you » avec DuckDuckGo");
        assert_eq!(s[1].url, "https://www.youtube.com/");
        assert!(!s.iter().any(|x| x.url == "https://example.com/"));
    }

    #[test]
    fn typed_address_first_and_no_duplicate() {
        let cands = [c("https://example.com/", "Example", 3, false), c("https://example.com/", "Example", 3, true)];
        let s = suggest("example.com", Some("https://example.com"), "https://duckduckgo.com/?q=example.com", "duckduckgo", &cands, 1_000_000, 8);
        assert_eq!(s[0].kind, "url");
        assert_eq!(s[1].kind, "search");
        assert_eq!(s.len(), 2, "{s:?}");
    }

    #[test]
    fn bookmarks_rank_above_history_and_all_words_needed() {
        let cands = [c("https://a.test/rust", "Rust book", 1, false), c("https://b.test/rust", "Rust book", 1, true), c("https://c.test/go", "Go book", 9, false)];
        let s = suggest("rust book", None, "q", "qwant", &cands, 1_000_000, 8);
        assert_eq!(s[1].kind, "bookmark");
        assert_eq!(s.len(), 3);
        assert!(s[0].title.ends_with("avec Qwant"));
    }

    #[test]
    fn internal_pages_never_suggested_and_limit() {
        let cands: Vec<Candidate> = (0..20).map(|_| c("https://site.test/", "Site", 1, false)).chain([c("nightfall://app/settings.html", "site", 1, true)]).collect();
        let s = suggest("site", None, "q", "duckduckgo", &cands, 1_000_000, 8);
        assert!(s.len() <= 8);
        assert!(s.iter().all(|x| !x.url.starts_with("nightfall://")));
        assert_eq!(engine_name("monmoteur"), "Monmoteur");
    }
}

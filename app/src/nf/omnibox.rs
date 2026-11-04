//! Address bar suggestions, computed locally from the encrypted history and
//! bookmarks. Nothing is ever sent to a remote suggestion service.

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

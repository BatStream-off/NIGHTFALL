//! NIGHTFALL VideoGuard.
//!
//! Uses the public SponsorBlock API (https://sponsor.ajay.app). Data is
//! licensed CC BY-NC-SA 4.0 by its contributors: attribution is shown in the
//! Security Center, and use is non-commercial. No SponsorBlock extension code
//! is used.
//!
//! Privacy: requests use the "hash prefix" endpoint. Only the first 4 hex
//! characters of SHA-256(video id) are sent, so the server cannot tell which
//! video is being watched. Requests are made by the browser core, not by the
//! YouTube page.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const API_BASE: &str = "https://sponsor.ajay.app/api";
pub const ATTRIBUTION: &str =
    "Segments fournis par SponsorBlock (sponsor.ajay.app), licence CC BY-NC-SA 4.0.";

/// SponsorBlock categories handled by VideoGuard.
pub const CATEGORIES: &[(&str, &str)] = &[
    ("sponsor", "Sponsor"),
    ("intro", "Intro / animation d'ouverture"),
    ("outro", "Outro / générique de fin"),
    ("interaction", "Rappel d'abonnement / interaction"),
    ("selfpromo", "Autopromotion"),
    ("preview", "Aperçu / récapitulatif"),
    ("filler", "Hors-sujet / remplissage"),
    ("music_offtopic", "Musique : partie non musicale"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    /// Jump over the segment automatically.
    Skip,
    /// Only show a "skip" button.
    Show,
    /// Do nothing.
    Ignore,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VideoGuardConfig {
    pub enabled: bool,
    pub actions: BTreeMap<String, Action>,
    /// Minimum segment length to auto-skip (seconds).
    pub min_duration: u32,
}

impl Default for VideoGuardConfig {
    fn default() -> Self {
        let mut actions = BTreeMap::new();
        for (c, _) in CATEGORIES {
            let a = match *c {
                "sponsor" | "interaction" | "selfpromo" => Action::Skip,
                "intro" | "outro" | "preview" => Action::Show,
                _ => Action::Ignore,
            };
            actions.insert((*c).to_string(), a);
        }
        Self { enabled: true, actions, min_duration: 1 }
    }
}

impl VideoGuardConfig {
    pub fn active_categories(&self) -> Vec<String> {
        self.actions
            .iter()
            .filter(|(_, a)| **a != Action::Ignore)
            .map(|(c, _)| c.clone())
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Segment {
    pub category: String,
    pub start: f64,
    pub end: f64,
    pub uuid: String,
    pub action: Action,
}

#[derive(Debug, thiserror::Error)]
pub enum VideoGuardError {
    #[error("API SponsorBlock : {0}")]
    Http(String),
    #[error("réponse SponsorBlock invalide : {0}")]
    Parse(String),
}

/// Extract the YouTube video id from a page URL (watch, shorts, embed, youtu.be).
pub fn video_id(page_url: &str) -> Option<String> {
    let u = url::Url::parse(page_url).ok()?;
    let host = u.host_str()?.trim_start_matches("www.").trim_start_matches("m.");
    let id = match host {
        "youtube.com" | "music.youtube.com" | "youtube-nocookie.com" => {
            let path = u.path();
            if path == "/watch" {
                u.query_pairs().find(|(k, _)| k == "v").map(|(_, v)| v.into_owned())
            } else if let Some(rest) = path.strip_prefix("/shorts/").or_else(|| path.strip_prefix("/embed/")).or_else(|| path.strip_prefix("/live/")) {
                Some(rest.split('/').next().unwrap_or("").to_string())
            } else {
                None
            }
        }
        "youtu.be" => Some(u.path().trim_start_matches('/').split('/').next().unwrap_or("").to_string()),
        _ => None,
    }?;
    let valid = id.len() == 11 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    valid.then_some(id)
}

/// First 4 hex chars of SHA-256(video id), as required by the API.
pub fn hash_prefix(video_id: &str) -> String {
    nf_crypto::hex(&nf_crypto::sha256(video_id.as_bytes()))[..4].to_string()
}

#[derive(Deserialize)]
struct ApiVideo {
    #[serde(rename = "videoID")]
    video_id: String,
    segments: Vec<ApiSegment>,
}

#[derive(Deserialize)]
struct ApiSegment {
    category: String,
    #[serde(rename = "actionType", default)]
    action_type: String,
    segment: [f64; 2],
    #[serde(rename = "UUID", default)]
    uuid: String,
}

/// Parse an API response (array of videos sharing the hash prefix) and keep
/// only the requested video's skippable segments, per the configuration.
pub fn parse_response(body: &str, video_id: &str, cfg: &VideoGuardConfig) -> Result<Vec<Segment>, VideoGuardError> {
    let videos: Vec<ApiVideo> = serde_json::from_str(body).map_err(|e| VideoGuardError::Parse(e.to_string()))?;
    let mut out = Vec::new();
    for v in videos.into_iter().filter(|v| v.video_id == video_id) {
        for s in v.segments {
            if !(s.action_type.is_empty() || s.action_type == "skip") {
                continue;
            }
            let action = cfg.actions.get(&s.category).copied().unwrap_or(Action::Ignore);
            let (start, end) = (s.segment[0], s.segment[1]);
            if action == Action::Ignore || !(end > start) || end - start < cfg.min_duration as f64 {
                continue;
            }
            out.push(Segment { category: s.category, start, end, uuid: s.uuid, action });
        }
    }
    out.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap_or(std::cmp::Ordering::Equal));
    Ok(out)
}

/// Fetch segments for a video (blocking; run on a background thread).
pub fn fetch_segments(video_id: &str, cfg: &VideoGuardConfig) -> Result<Vec<Segment>, VideoGuardError> {
    if !cfg.enabled {
        return Ok(Vec::new());
    }
    let cats = serde_json::to_string(&cfg.active_categories()).expect("serializable");
    let url = format!("{API_BASE}/skipSegments/{}", hash_prefix(video_id));
    let resp = ureq::get(&url)
        .query("categories", &cats)
        .query("actionTypes", "[\"skip\"]")
        .set("User-Agent", "NIGHTFALL-VideoGuard/0.1")
        .timeout(std::time::Duration::from_secs(10))
        .call();
    match resp {
        Ok(r) => {
            let body = r.into_string().map_err(|e| VideoGuardError::Http(e.to_string()))?;
            parse_response(&body, video_id, cfg)
        }
        // 404 = no segment for any video with this prefix.
        Err(ureq::Error::Status(404, _)) => Ok(Vec::new()),
        Err(e) => Err(VideoGuardError::Http(e.to_string())),
    }
}

/// JavaScript injected into YouTube pages with the segments of the current
/// video. It skips (or offers to skip) segments and lets the user go back.
pub fn injection_script(video_id: &str, segments: &[Segment]) -> String {
    let data = serde_json::json!({ "videoId": video_id, "segments": segments });
    format!("{}\n;window.__nightfallVideoGuard && window.__nightfallVideoGuard({});", CONTENT_SCRIPT, data)
}

const CONTENT_SCRIPT: &str = include_str!("videoguard.js");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn video_ids() {
        assert_eq!(video_id("https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=10").as_deref(), Some("dQw4w9WgXcQ"));
        assert_eq!(video_id("https://youtu.be/dQw4w9WgXcQ").as_deref(), Some("dQw4w9WgXcQ"));
        assert_eq!(video_id("https://m.youtube.com/shorts/abcdefghijk").as_deref(), Some("abcdefghijk"));
        assert_eq!(video_id("https://www.youtube.com/"), None);
        assert_eq!(video_id("https://evil.test/watch?v=dQw4w9WgXcQ"), None);
        assert_eq!(video_id("https://www.youtube.com/watch?v=short"), None);
    }

    #[test]
    fn prefix_is_four_hex() {
        let p = hash_prefix("dQw4w9WgXcQ");
        assert_eq!(p.len(), 4);
        assert!(p.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn parse_filters_by_video_and_config() {
        let body = r#"[
          {"videoID":"aaaaaaaaaaa","segments":[{"category":"sponsor","actionType":"skip","segment":[1,2],"UUID":"x"}]},
          {"videoID":"dQw4w9WgXcQ","segments":[
             {"category":"intro","actionType":"skip","segment":[0,5],"UUID":"i"},
             {"category":"sponsor","actionType":"skip","segment":[30.5,60],"UUID":"s"},
             {"category":"filler","actionType":"skip","segment":[70,80],"UUID":"f"},
             {"category":"sponsor","actionType":"mute","segment":[90,95],"UUID":"m"},
             {"category":"sponsor","actionType":"skip","segment":[100,100.2],"UUID":"tiny"}
          ]}]"#;
        let segs = parse_response(body, "dQw4w9WgXcQ", &VideoGuardConfig::default()).unwrap();
        let ids: Vec<_> = segs.iter().map(|s| (s.uuid.as_str(), s.action)).collect();
        assert_eq!(ids, [("i", Action::Show), ("s", Action::Skip)]);
    }

    #[test]
    fn default_config_categories() {
        let c = VideoGuardConfig::default();
        let active = c.active_categories();
        assert!(active.contains(&"sponsor".to_string()));
        assert!(!active.contains(&"filler".to_string()));
    }

    #[test]
    fn script_embeds_segments() {
        let s = injection_script("dQw4w9WgXcQ", &[Segment { category: "sponsor".into(), start: 1.0, end: 2.0, uuid: "u".into(), action: Action::Skip }]);
        assert!(s.contains("\"videoId\":\"dQw4w9WgXcQ\""));
        assert!(s.contains("__nightfallVideoGuard"));
    }
}

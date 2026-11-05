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
    ("hook", "Accroche / teaser d'ouverture"),
    ("poi_highlight", "Moment fort (bouton « Aller au moment fort »)"),
    ("exclusive_access", "Accès exclusif (étiquette sur la vidéo entière)"),
];

/// Default user choice for a category (also used for categories added by a
/// newer version and missing from a saved configuration).
pub fn default_action(category: &str) -> Action {
    match category {
        "sponsor" | "interaction" | "selfpromo" => Action::Skip,
        "intro" | "outro" | "preview" | "hook" | "poi_highlight" | "exclusive_access" => Action::Show,
        _ => Action::Ignore,
    }
}

/// Every category and segment type is always requested, so the request
/// does not reveal the user's settings; filtering happens locally.
const ALL_CATEGORIES: &[&str] = &["sponsor", "selfpromo", "interaction", "intro", "outro", "preview", "music_offtopic", "filler", "hook", "poi_highlight", "exclusive_access"];
const ALL_ACTION_TYPES: &str = r#"["skip","mute","full","poi"]"#;

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

/// Saved encrypted in the profile: a missing field takes its default value,
/// the others are kept.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct VideoGuardConfig {
    pub enabled: bool,
    pub actions: BTreeMap<String, Action>,
    /// Minimum segment length to auto-skip (seconds).
    pub min_duration: u32,
    /// Fast-forward YouTube's own video ads that Shield did not block
    /// (muted, then the "skip" button is pressed as soon as it appears).
    #[serde(default = "yes")]
    pub skip_video_ads: bool,
    /// Mute the "mute" segments (sound only, the video goes on).
    pub mute_segments: bool,
    /// Show the "whole video" labels (fully sponsored, exclusive access).
    pub full_video_labels: bool,
}

fn yes() -> bool {
    true
}

impl Default for VideoGuardConfig {
    fn default() -> Self {
        let mut actions = BTreeMap::new();
        for (c, _) in CATEGORIES {
            actions.insert((*c).to_string(), default_action(c));
        }
        Self { enabled: true, actions, min_duration: 1, skip_video_ads: true, mute_segments: true, full_video_labels: true }
    }
}

impl VideoGuardConfig {
    pub fn active_categories(&self) -> Vec<String> {
        CATEGORIES
            .iter()
            .map(|(c, _)| *c)
            .chain(self.actions.keys().map(String::as_str).filter(|k| !CATEGORIES.iter().any(|(c, _)| c == k)))
            .filter(|c| self.action_for(c) != Action::Ignore)
            .map(str::to_string)
            .collect()
    }

    /// User choice for a category, with the default for categories missing
    /// from a configuration saved by an older version.
    pub fn action_for(&self, category: &str) -> Action {
        self.actions.get(category).copied().unwrap_or_else(|| default_action(category))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Segment {
    pub category: String,
    pub start: f64,
    pub end: f64,
    pub uuid: String,
    pub action: Action,
    /// SponsorBlock segment type: "skip", "mute", "full" (whole video) or
    /// "poi" (highlight, a single point).
    #[serde(default = "skip_kind")]
    pub kind: String,
}

fn skip_kind() -> String {
    "skip".into()
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
            let kind = if s.action_type.is_empty() { "skip".to_string() } else { s.action_type.clone() };
            let action = cfg.action_for(&s.category);
            if action == Action::Ignore {
                continue;
            }
            let (start, end) = (s.segment[0], s.segment[1]);
            let keep = match kind.as_str() {
                "skip" => end > start && end - start >= cfg.min_duration as f64,
                "mute" => cfg.mute_segments && end > start,
                "full" => cfg.full_video_labels,
                "poi" => start >= 0.0,
                _ => false,
            };
            if !keep {
                continue;
            }
            out.push(Segment { category: s.category, start, end, uuid: s.uuid, action, kind });
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
    if cfg.active_categories().is_empty() {
        return Ok(Vec::new());
    }
    let cats = serde_json::to_string(ALL_CATEGORIES).expect("serializable");
    let url = format!("{API_BASE}/skipSegments/{}", hash_prefix(video_id));
    let resp = ureq::get(&url)
        .query("categories", &cats)
        .query("actionTypes", ALL_ACTION_TYPES)
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
    let data = serde_json::json!({ "videoId": video_id, "segments": segments, "labels": labels() });
    format!("{}\n;window.__nightfallVideoGuard && window.__nightfallVideoGuard({});", CONTENT_SCRIPT, data)
}

/// Script injected at the start of every YouTube page: the content script
/// with its options (video ad fast-forward), before any segment is known.
pub fn page_script(page_url: &str, cfg: &VideoGuardConfig) -> Option<String> {
    if !cfg.enabled || !is_youtube(page_url) {
        return None;
    }
    let opts = serde_json::json!({ "skipVideoAds": cfg.skip_video_ads, "labels": labels() });
    Some(format!("{}\n;window.__nightfallVideoGuardInit && window.__nightfallVideoGuardInit({});", CONTENT_SCRIPT, opts))
}

pub fn is_youtube(page_url: &str) -> bool {
    url::Url::parse(page_url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_ascii_lowercase()))
        .map(|h| h == "youtube.com" || h.ends_with(".youtube.com") || h == "youtube-nocookie.com" || h.ends_with(".youtube-nocookie.com"))
        .unwrap_or(false)
}

const CONTENT_SCRIPT: &str = include_str!("videoguard.js");

/// Short French labels shown on the video.
fn labels() -> serde_json::Value {
    serde_json::json!({
        "sponsor": "Sponsor", "intro": "Intro", "outro": "Outro", "interaction": "Rappel d'abonnement",
        "selfpromo": "Autopromotion", "preview": "Aperçu", "filler": "Hors-sujet", "music_offtopic": "Partie non musicale",
        "hook": "Accroche", "poi_highlight": "Moment fort", "exclusive_access": "Accès exclusif"
    })
}

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
        let ids: Vec<_> = segs.iter().map(|s| (s.uuid.as_str(), s.action, s.kind.as_str())).collect();
        // The "mute" segment is kept (muted), the too-short skip is not.
        assert_eq!(ids, [("i", Action::Show, "skip"), ("s", Action::Skip, "skip"), ("m", Action::Skip, "mute")]);
    }

    #[test]
    fn mute_full_and_poi_follow_the_settings() {
        let body = r#"[{"videoID":"dQw4w9WgXcQ","segments":[
             {"category":"sponsor","actionType":"mute","segment":[10,20],"UUID":"m"},
             {"category":"sponsor","actionType":"full","segment":[0,0],"UUID":"f"},
             {"category":"poi_highlight","actionType":"poi","segment":[42,42],"UUID":"p"},
             {"category":"hook","actionType":"skip","segment":[0,8],"UUID":"h"}
          ]}]"#;
        let all = parse_response(body, "dQw4w9WgXcQ", &VideoGuardConfig::default()).unwrap();
        let kinds: Vec<_> = all.iter().map(|s| s.kind.as_str()).collect();
        assert_eq!(kinds, ["full", "skip", "mute", "poi"]);
        let cfg = VideoGuardConfig { mute_segments: false, full_video_labels: false, ..Default::default() };
        let some = parse_response(body, "dQw4w9WgXcQ", &cfg).unwrap();
        let kinds: Vec<_> = some.iter().map(|s| s.kind.as_str()).collect();
        assert_eq!(kinds, ["skip", "poi"]);
    }

    #[test]
    fn categories_missing_from_an_old_configuration_get_their_default() {
        let old: VideoGuardConfig = serde_json::from_str(r#"{"enabled":true,"actions":{"sponsor":"skip","intro":"ignore"},"min_duration":1}"#).unwrap();
        assert_eq!(old.action_for("intro"), Action::Ignore);
        assert_eq!(old.action_for("hook"), Action::Show);
        assert!(old.mute_segments && old.full_video_labels && old.skip_video_ads);
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
        let s = injection_script("dQw4w9WgXcQ", &[Segment { category: "sponsor".into(), start: 1.0, end: 2.0, uuid: "u".into(), action: Action::Skip, kind: "skip".into() }]);
        assert!(s.contains("\"videoId\":\"dQw4w9WgXcQ\""));
        assert!(s.contains("__nightfallVideoGuard"));
    }

    #[test]
    fn page_script_only_on_youtube() {
        let cfg = VideoGuardConfig::default();
        assert!(page_script("https://www.youtube.com/watch?v=dQw4w9WgXcQ", &cfg).unwrap().contains("skipVideoAds"));
        assert!(page_script("https://www.youtube-nocookie.com/embed/x", &cfg).is_some());
        assert!(page_script("https://notyoutube.com/", &cfg).is_none());
        let off = VideoGuardConfig { enabled: false, ..cfg };
        assert!(page_script("https://www.youtube.com/", &off).is_none());
    }

    #[test]
    fn old_saved_config_gets_video_ad_skipping() {
        let json = r#"{"enabled":true,"actions":{},"min_duration":1}"#;
        let cfg: VideoGuardConfig = serde_json::from_str(json).unwrap();
        assert!(cfg.skip_video_ads);
    }

    #[test]
    fn saved_config_with_missing_fields_keeps_the_others() {
        let json = r#"{"enabled":false,"min_duration":7}"#;
        let cfg: VideoGuardConfig = serde_json::from_str(json).unwrap();
        assert!(!cfg.enabled);
        assert_eq!(cfg.min_duration, 7);
        assert!(cfg.skip_video_ads);
        assert_eq!(cfg.actions, VideoGuardConfig::default().actions);
        let json = r#"{"actions":{"sponsor":"ignore"},"skip_video_ads":false}"#;
        let cfg: VideoGuardConfig = serde_json::from_str(json).unwrap();
        assert!(cfg.enabled && !cfg.skip_video_ads);
        assert_eq!(cfg.actions.get("sponsor"), Some(&Action::Ignore));
        assert_eq!(cfg.min_duration, 1);
        assert_eq!(serde_json::from_str::<VideoGuardConfig>("{}").unwrap(), VideoGuardConfig::default());
    }

    #[test]
    fn content_script_can_be_suspended() {
        // Suspended while NIGHTFALL is locked: no skip, no ad fast-forward.
        assert!(CONTENT_SCRIPT.contains("window.__nightfallVideoGuardSuspend = function"));
        assert_eq!(CONTENT_SCRIPT.matches("if (state.suspended) return;").count(), 2);
    }
}

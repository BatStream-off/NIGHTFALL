//! Site permission prompts (camera, microphone, location, notifications...).
//!
//! Alloy-style browsers have no built-in prompt, so NIGHTFALL asks in its
//! own interface. Nothing is granted without an explicit click; ignoring the
//! prompt or closing the tab denies.
//!
//! Before asking, the user's choices apply: a decision remembered for the
//! site ("Mémoriser" ticked in the prompt), then the default of the
//! permission ("Bloquer" in the settings). Chromium itself keeps the answers
//! to its permission prompts (location, notifications...) in its profile,
//! origins in plain text: answers given without "Mémoriser" are reset at the
//! next lock or start, and forgetting a decision in the settings resets
//! Chromium's copy too. Remembered decisions live in the encrypted database;
//! Chromium's copy of them exists only while NIGHTFALL is unlocked: removed
//! at lock and when the last window closes, given back at unlock (so that
//! `navigator.permissions.query` keeps answering the same). After a crash
//! the copy stays until the next unlock (the list is encrypted before).

use super::client::ClientKind;
use super::state::{core, Settings, SitePermission};
use super::windows;
use cef::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Permission kinds offered in the settings: id, French label, CEF prompt
/// bits (`cef_permission_request_types_t`), media bits
/// (`cef_media_access_permission_types_t`).
pub const KINDS: &[(&str, &str, u32, u32)] = &[
    ("camera", "la caméra", 4 | 2, 2),
    ("microphone", "le microphone", 4096, 1),
    ("geolocation", "votre position", 256, 0),
    ("notifications", "les notifications", 32768, 0),
    ("clipboard", "le presse-papiers", 16, 0),
    ("downloads", "les téléchargements multiples", 16384, 0),
    ("window_management", "la gestion des fenêtres sur vos écrans", 8388608, 0),
    ("local_fonts", "les polices installées", 128, 0),
];

/// Screen and system audio capture: always asked, never remembered.
const MEDIA_DESKTOP: u32 = 4 | 8;

pub fn kind_label(id: &str) -> Option<&'static str> {
    KINDS.iter().find(|k| k.0 == id).map(|k| k.1)
}

/// Kinds named by a prompt's bits; `true` when other bits remain (a
/// permission NIGHTFALL does not list: always asked).
pub fn kinds_for_prompt(bits: u32) -> (Vec<&'static str>, bool) {
    let mut rest = bits;
    let mut out = Vec::new();
    for (id, _, prompt, _) in KINDS {
        if bits & prompt != 0 {
            out.push(*id);
            rest &= !prompt;
        }
    }
    (out, rest != 0)
}

/// Kinds named by a media request; `true` when screen capture is asked.
pub fn kinds_for_media(bits: u32) -> (Vec<&'static str>, bool) {
    let out = KINDS.iter().filter(|k| k.3 != 0 && bits & k.3 != 0).map(|k| k.0).collect();
    (out, bits & MEDIA_DESKTOP != 0)
}

/// "https://Example.com:443/path" -> "https://example.com".
pub fn normalize_origin(origin: &str) -> String {
    url::Url::parse(origin.trim()).map(|u| u.origin().ascii_serialization()).unwrap_or_else(|_| origin.trim().trim_end_matches('/').to_string())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Allow,
    Deny,
    Ask,
}

/// What to answer without asking. Private windows apply blocks (remembered
/// or default) but never a remembered "allow".
pub fn decide(s: &Settings, origin: &str, kinds: &[&str], unknown: bool, private: bool) -> Decision {
    if kinds.is_empty() {
        return Decision::Ask;
    }
    let origin = normalize_origin(origin);
    let mut all_allowed = !unknown;
    for k in kinds {
        let site = s.site_permissions.iter().find(|p| p.origin == origin && p.kind == *k);
        match site {
            Some(p) if !p.allow => return Decision::Deny,
            Some(_) if !private => {}
            _ => {
                if s.permission_defaults.get(*k).map(String::as_str) == Some("block") {
                    return Decision::Deny;
                }
                all_allowed = false;
            }
        }
    }
    if all_allowed {
        Decision::Allow
    } else {
        Decision::Ask
    }
}

/// Remember (or replace) the decision for each kind.
pub fn remember(s: &mut Settings, origin: &str, kinds: &[&str], allow: bool, now: i64) {
    let origin = normalize_origin(origin);
    for k in kinds {
        s.site_permissions.retain(|p| !(p.origin == origin && p.kind == *k));
        s.permission_resets.retain(|p| !(p.origin == origin && p.kind == *k));
        s.site_permissions.push(SitePermission { origin: origin.clone(), kind: k.to_string(), allow, updated: now });
    }
    s.site_permissions.sort_by(|a, b| a.origin.cmp(&b.origin).then(a.kind.cmp(&b.kind)));
}

/// Chromium content setting types of a kind (for resets and defaults).
fn content_types(kind: &str) -> Vec<ContentSettingTypes> {
    match kind {
        "camera" => vec![ContentSettingTypes::MEDIASTREAM_CAMERA],
        "microphone" => vec![ContentSettingTypes::MEDIASTREAM_MIC],
        "geolocation" => vec![ContentSettingTypes::GEOLOCATION],
        "notifications" => vec![ContentSettingTypes::NOTIFICATIONS],
        "clipboard" => vec![ContentSettingTypes::CLIPBOARD_READ_WRITE],
        "downloads" => vec![ContentSettingTypes::AUTOMATIC_DOWNLOADS],
        "window_management" => vec![ContentSettingTypes::WINDOW_MANAGEMENT],
        "local_fonts" => vec![ContentSettingTypes::LOCAL_FONTS],
        _ => vec![],
    }
}

/// Forget Chromium's own copy of a decision (UI thread).
pub fn reset_chromium(origin: &str, kind: &str) {
    let Some(rc) = request_context_get_global_context() else { return };
    let url = CefString::from(format!("{origin}/").as_str());
    for t in content_types(kind) {
        rc.set_content_setting(Some(&url), Some(&url), t, ContentSettingValues::DEFAULT);
    }
}

/// Remembered decisions: content setting value for Chromium.
fn chromium_value(allow: bool) -> ContentSettingValues {
    if allow {
        ContentSettingValues::ALLOW
    } else {
        ContentSettingValues::BLOCK
    }
}

/// Remove Chromium's copy of every remembered decision (UI thread; at lock
/// and when the last window closes).
pub fn forget_remembered_in_chromium() {
    let list = core().settings.site_permissions.clone();
    for p in &list {
        reset_chromium(&p.origin, &p.kind);
    }
    if !list.is_empty() {
        super::state::log(&format!("autorisations mémorisées : copie de Chromium effacée ({} décision(s))", list.len()));
    }
}

/// Give the remembered decisions back to Chromium (UI thread; at unlock).
pub fn reapply_remembered() {
    let list = core().settings.site_permissions.clone();
    if list.is_empty() {
        return;
    }
    let Some(rc) = request_context_get_global_context() else { return };
    for p in &list {
        let url = CefString::from(format!("{}/", p.origin).as_str());
        for t in content_types(&p.kind) {
            rc.set_content_setting(Some(&url), Some(&url), t, chromium_value(p.allow));
        }
    }
    super::state::log(&format!("autorisations mémorisées : rendues à Chromium ({} décision(s))", list.len()));
}

/// Defaults "Bloquer" / "Demander" as Chromium content settings (UI
/// thread), so Chromium refuses blocked permissions without a prompt.
pub fn apply_defaults(s: &Settings) {
    let Some(rc) = request_context_get_global_context() else { return };
    for (id, _, _, _) in KINDS {
        let block = s.permission_defaults.get(*id).map(String::as_str) == Some("block");
        for t in content_types(id) {
            rc.set_content_setting(None, None, t, if block { ContentSettingValues::BLOCK } else { ContentSettingValues::DEFAULT });
        }
    }
}

/// Reset the answers given without "Mémoriser" (UI thread; at lock and
/// after unlock).
pub fn reset_temporary() {
    let resets = {
        let mut c = core();
        if c.settings.permission_resets.is_empty() {
            return;
        }
        let r = std::mem::take(&mut c.settings.permission_resets);
        c.save_settings();
        r
    };
    for p in resets {
        reset_chromium(&p.origin, &p.kind);
    }
}

enum Answer {
    Prompt(PermissionPromptCallback),
    Media(MediaAccessCallback, u32),
}

struct Pending {
    answer: Answer,
    origin: String,
    kinds: Vec<&'static str>,
    can_remember: bool,
}

thread_local! {
    static PENDING: RefCell<HashMap<u64, Pending>> = RefCell::new(HashMap::new());
}

static NEXT_MEDIA_ID: AtomicU64 = AtomicU64::new(1 << 40);

fn describe(kinds: &[&str], unknown: bool, desktop: bool) -> String {
    let mut parts: Vec<&str> = kinds.iter().filter_map(|k| kind_label(k)).collect();
    if desktop {
        parts.push("le partage d'écran ou le son de l'ordinateur");
    }
    if unknown {
        parts.push("une autre autorisation");
    }
    if parts.is_empty() {
        "demande une autorisation".into()
    } else {
        format!("souhaite utiliser {}", parts.join(", "))
    }
}

fn ask(window_id: u32, id: u64, origin: &str, what: &str, can_remember: bool) {
    windows::push_event(
        window_id,
        &serde_json::json!({ "type": "permission", "id": id.to_string(), "origin": origin, "what": what, "remember": can_remember }),
    );
}

fn give(answer: &Answer, allow: bool) {
    match answer {
        Answer::Prompt(cb) => cb.cont(if allow { PermissionRequestResult::ACCEPT } else { PermissionRequestResult::DENY }),
        Answer::Media(cb, requested) => cb.cont(if allow { *requested } else { 0 }),
    }
}

/// Answer from the interface (UI thread).
pub fn answer(id: u64, allow: bool, remember_it: bool) {
    let Some(p) = PENDING.with(|p| p.borrow_mut().remove(&id)) else { return };
    give(&p.answer, allow);
    if p.can_remember && remember_it {
        let mut c = core();
        remember(&mut c.settings, &p.origin, &p.kinds, allow, nf_core::unix_now());
        c.save_settings();
    } else if matches!(p.answer, Answer::Prompt(_)) && !p.origin.is_empty() {
        // Chromium keeps prompt answers: undo it at the next lock or start.
        let origin = normalize_origin(&p.origin);
        let mut c = core();
        for k in &p.kinds {
            if !c.settings.site_permissions.iter().any(|s| s.origin == origin && s.kind == *k) {
                c.settings.permission_resets.push(SitePermission { origin: origin.clone(), kind: k.to_string(), allow, updated: nf_core::unix_now() });
            }
        }
        c.save_settings();
    }
}

fn private_of(kind: &ClientKind) -> bool {
    matches!(kind, ClientKind::Tab { private: true, .. })
}

wrap_permission_handler! {
    pub struct NfPermission {
        kind: Arc<ClientKind>,
    }

    impl PermissionHandler {
        fn on_request_media_access_permission(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            requesting_origin: Option<&CefString>,
            requested_permissions: u32,
            callback: Option<&mut MediaAccessCallback>,
        ) -> i32 {
            let ClientKind::Tab { window_id, .. } = *self.kind else { return 0 };
            let Some(cb) = callback else { return 0 };
            let origin = requesting_origin.map(|o| o.to_string()).unwrap_or_default();
            let private = private_of(&self.kind);
            let (kinds, desktop) = kinds_for_media(requested_permissions);
            let decision = decide(&core().settings, &origin, &kinds, desktop, private);
            if decision != Decision::Ask {
                cb.cont(if decision == Decision::Allow { requested_permissions } else { 0 });
                return 1;
            }
            let id = NEXT_MEDIA_ID.fetch_add(1, Ordering::Relaxed);
            let can_remember = !private && !desktop && !kinds.is_empty();
            let what = describe(&kinds, false, desktop);
            PENDING.with(|p| p.borrow_mut().insert(id, Pending { answer: Answer::Media(cb.clone(), requested_permissions), origin: origin.clone(), kinds, can_remember }));
            ask(window_id, id, &origin, &what, can_remember);
            1
        }

        fn on_show_permission_prompt(
            &self,
            _browser: Option<&mut Browser>,
            prompt_id: u64,
            requesting_origin: Option<&CefString>,
            requested_permissions: u32,
            callback: Option<&mut PermissionPromptCallback>,
        ) -> i32 {
            let ClientKind::Tab { window_id, .. } = *self.kind else { return 0 };
            let Some(cb) = callback else { return 0 };
            let origin = requesting_origin.map(|o| o.to_string()).unwrap_or_default();
            let private = private_of(&self.kind);
            let (kinds, unknown) = kinds_for_prompt(requested_permissions);
            let decision = decide(&core().settings, &origin, &kinds, unknown, private);
            if decision != Decision::Ask {
                cb.cont(if decision == Decision::Allow { PermissionRequestResult::ACCEPT } else { PermissionRequestResult::DENY });
                return 1;
            }
            let can_remember = !private && !unknown && !kinds.is_empty();
            let what = describe(&kinds, unknown, false);
            PENDING.with(|p| p.borrow_mut().insert(prompt_id, Pending { answer: Answer::Prompt(cb.clone()), origin: origin.clone(), kinds, can_remember }));
            ask(window_id, prompt_id, &origin, &what, can_remember);
            1
        }

        fn on_dismiss_permission_prompt(&self, _browser: Option<&mut Browser>, prompt_id: u64, _result: PermissionRequestResult) {
            PENDING.with(|p| p.borrow_mut().remove(&prompt_id));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bits_to_kinds() {
        assert_eq!(kinds_for_prompt(256), (vec!["geolocation"], false));
        assert_eq!(kinds_for_prompt(4 | 4096), (vec!["camera", "microphone"], false));
        assert_eq!(kinds_for_prompt(256 | 1), (vec!["geolocation"], true));
        assert_eq!(kinds_for_media(1 | 2), (vec!["camera", "microphone"], false));
        assert_eq!(kinds_for_media(8), (vec![], true));
        assert_eq!(normalize_origin("https://Example.com:443/a?b"), "https://example.com");
    }

    #[test]
    fn remembered_decisions_and_defaults() {
        let mut s = Settings::default();
        let o = "https://maps.example/";
        assert_eq!(decide(&s, o, &["geolocation"], false, false), Decision::Ask);
        remember(&mut s, o, &["geolocation"], true, 1);
        assert_eq!(s.site_permissions.len(), 1);
        assert_eq!(s.site_permissions[0].origin, "https://maps.example");
        assert_eq!(decide(&s, "https://maps.example", &["geolocation"], false, false), Decision::Allow);
        // Never auto-allowed in a private window, nor with an unknown bit.
        assert_eq!(decide(&s, o, &["geolocation"], false, true), Decision::Ask);
        assert_eq!(decide(&s, o, &["geolocation"], true, false), Decision::Ask);
        // Another site still asks; a default block refuses without asking.
        assert_eq!(decide(&s, "https://other.example", &["geolocation"], false, false), Decision::Ask);
        s.permission_defaults.insert("geolocation".into(), "block".into());
        assert_eq!(decide(&s, "https://other.example", &["geolocation"], false, false), Decision::Deny);
        // The remembered "allow" wins over the default block.
        assert_eq!(decide(&s, o, &["geolocation"], false, false), Decision::Allow);
        // A remembered block is applied, also in private windows.
        remember(&mut s, o, &["camera"], false, 2);
        assert_eq!(decide(&s, o, &["camera", "microphone"], false, true), Decision::Deny);
        // Replacing a decision keeps one entry per (site, kind).
        remember(&mut s, o, &["camera"], true, 3);
        assert_eq!(s.site_permissions.iter().filter(|p| p.kind == "camera").count(), 1);
        assert_eq!(decide(&s, o, &["camera"], false, false), Decision::Allow);
        // Camera allowed but microphone unknown for the site: ask.
        assert_eq!(decide(&s, o, &["camera", "microphone"], false, false), Decision::Ask);
        assert_eq!(decide(&s, o, &[], false, false), Decision::Ask);
    }
}

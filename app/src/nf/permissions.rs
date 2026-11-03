//! Site permission prompts (camera, microphone, location, notifications...).
//!
//! Alloy-style browsers have no built-in prompt, so NIGHTFALL asks in its
//! own interface. Nothing is granted without an explicit click; ignoring the
//! prompt or closing the tab denies.

use super::client::ClientKind;
use super::windows;
use cef::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

enum Pending {
    Prompt(PermissionPromptCallback),
    Media(MediaAccessCallback, u32),
}

thread_local! {
    static PENDING: RefCell<HashMap<u64, Pending>> = RefCell::new(HashMap::new());
}

static NEXT_MEDIA_ID: AtomicU64 = AtomicU64::new(1 << 40);

fn media_description(bits: u32) -> String {
    let mut parts = Vec::new();
    if bits & 1 != 0 {
        parts.push("le microphone");
    }
    if bits & 2 != 0 {
        parts.push("la caméra");
    }
    if bits & 4 != 0 {
        parts.push("le son de l'ordinateur");
    }
    if bits & 8 != 0 {
        parts.push("le partage d'écran");
    }
    if parts.is_empty() {
        "un périphérique multimédia".into()
    } else {
        parts.join(", ")
    }
}

fn ask(window_id: u32, id: u64, origin: &str, what: &str) {
    windows::push_event(
        window_id,
        &serde_json::json!({ "type": "permission", "id": id.to_string(), "origin": origin, "what": what }),
    );
}

/// Answer from the interface (UI thread).
pub fn answer(id: u64, allow: bool) {
    let pending = PENDING.with(|p| p.borrow_mut().remove(&id));
    match pending {
        Some(Pending::Prompt(cb)) => cb.cont(if allow { PermissionRequestResult::ACCEPT } else { PermissionRequestResult::DENY }),
        Some(Pending::Media(cb, requested)) => cb.cont(if allow { requested } else { 0 }),
        None => {}
    }
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
            let id = NEXT_MEDIA_ID.fetch_add(1, Ordering::Relaxed);
            let origin = requesting_origin.map(|o| o.to_string()).unwrap_or_default();
            PENDING.with(|p| p.borrow_mut().insert(id, Pending::Media(cb.clone(), requested_permissions)));
            ask(window_id, id, &origin, &format!("souhaite utiliser {}", media_description(requested_permissions)));
            1
        }

        fn on_show_permission_prompt(
            &self,
            _browser: Option<&mut Browser>,
            prompt_id: u64,
            requesting_origin: Option<&CefString>,
            _requested_permissions: u32,
            callback: Option<&mut PermissionPromptCallback>,
        ) -> i32 {
            let ClientKind::Tab { window_id, .. } = *self.kind else { return 0 };
            let Some(cb) = callback else { return 0 };
            let origin = requesting_origin.map(|o| o.to_string()).unwrap_or_default();
            PENDING.with(|p| p.borrow_mut().insert(prompt_id, Pending::Prompt(cb.clone())));
            ask(window_id, prompt_id, &origin, "demande une autorisation (position, notifications, presse-papiers ou autre)");
            1
        }

        fn on_dismiss_permission_prompt(&self, _browser: Option<&mut Browser>, prompt_id: u64, _result: PermissionRequestResult) {
            PENDING.with(|p| p.borrow_mut().remove(&prompt_id));
        }
    }
}

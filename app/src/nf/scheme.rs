//! `nightfall://app/` scheme: serves the embedded interface and the internal API.
//!
//! Security rules:
//! - Only NIGHTFALL pages (main frame URL `nightfall://app/...`) may call
//!   `nightfall://app/api/*`, with POST.
//! - Every response forbids framing (`X-Frame-Options: DENY`,
//!   `frame-ancestors 'none'`) and sets a strict Content-Security-Policy.
//! - The scheme is not CORS-enabled, so web pages cannot read it.

use super::api::{self, Caller};
use super::assets;
use super::state;
use cef::wrapper::byte_read_handler::{ByteReadHandler, ByteStream};
use cef::wrapper::stream_resource_handler::StreamResourceHandler;
use cef::*;
use std::sync::{Arc, Mutex};

const CSP: &str = "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: https: http:; connect-src 'self'; font-src 'self' data:; object-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'";

fn to_string(s: &CefStringUserfree) -> String {
    CefString::from(s).to_string()
}

fn respond(status: i32, mime: &str, body: Vec<u8>, csp: bool) -> Option<ResourceHandler> {
    let stream = Arc::new(Mutex::new(ByteStream::new(body)));
    let mut handler = ByteReadHandler::new(stream);
    let reader = stream_reader_create_for_handler(Some(&mut handler))?;
    let mut headers = CefStringMultimap::new();
    headers.append("Cache-Control", "no-store");
    headers.append("X-Frame-Options", "DENY");
    headers.append("X-Content-Type-Options", "nosniff");
    headers.append("Referrer-Policy", "no-referrer");
    if csp {
        headers.append("Content-Security-Policy", CSP);
    }
    let text = if status == 200 { "OK" } else { "Error" };
    // CEF expects a bare MIME type ("text/html"): with parameters
    // ("text/html; charset=utf-8") Chromium does not recognise the type and
    // renders nothing. The pages declare UTF-8 themselves (<meta charset>),
    // and scripts and style sheets inherit the document encoding.
    let bare = mime.split(';').next().unwrap_or(mime).trim();
    Some(StreamResourceHandler::new(status, text.into(), bare.into(), Some(headers), Some(reader)))
}

fn json_response(v: &serde_json::Value) -> Option<ResourceHandler> {
    respond(200, "application/json", serde_json::to_vec(v).unwrap_or_default(), false)
}

fn read_post_body(request: &Request) -> Vec<u8> {
    let Some(post) = request.post_data() else { return Vec::new() };
    let count = post.element_count();
    let mut elements: Vec<Option<PostDataElement>> = vec![None; count];
    post.elements(Some(&mut elements));
    let mut out = Vec::new();
    for e in elements.into_iter().flatten() {
        let n = e.bytes_count();
        if n == 0 || n > 4 * 1024 * 1024 {
            continue;
        }
        let mut buf = vec![0u8; n];
        let read = e.bytes(n, buf.as_mut_ptr());
        buf.truncate(read);
        out.extend_from_slice(&buf);
    }
    out
}

wrap_scheme_handler_factory! {
    pub struct NfSchemeFactory;

    impl SchemeHandlerFactory {
        fn create(
            &self,
            browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            _scheme_name: Option<&CefString>,
            request: Option<&mut Request>,
        ) -> Option<ResourceHandler> {
            let request = request?;
            let url = to_string(&request.url());
            let Some(rest) = url.strip_prefix("nightfall://app/") else {
                return respond(404, "text/plain", b"introuvable".to_vec(), false);
            };
            let path = rest.split(['?', '#']).next().unwrap_or("");
            // The first requests are logged to diagnose start-up problems.
            static SEEN: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
            if SEEN.fetch_add(1, std::sync::atomic::Ordering::Relaxed) < 40 {
                state::log(&format!("schéma : {path}"));
            }

            if let Some(cmd) = path.strip_prefix("api/") {
                let forbidden = || respond(403, "text/plain", b"interdit".to_vec(), false);
                let (Some(browser), Some(frame)) = (browser, frame) else {
                    state::log("appel API refusé : pas de navigateur ou de frame");
                    return forbidden();
                };
                let frame_url = to_string(&frame.url());
                if frame.is_main() == 0 || !frame_url.starts_with("nightfall://app/") {
                    state::log("appel API refusé : origine non NIGHTFALL");
                    return forbidden();
                }
                if to_string(&request.method()).to_ascii_uppercase() != "POST" {
                    state::log("appel API refusé : méthode non POST");
                    return forbidden();
                }
                let Some(info) = state::browser_info(browser.identifier()) else {
                    state::log(&format!("appel API refusé : navigateur inconnu id={}", browser.identifier()));
                    return forbidden();
                };
                let body = read_post_body(request);
                let value: serde_json::Value = if body.is_empty() {
                    serde_json::Value::Object(Default::default())
                } else {
                    match serde_json::from_slice(&body) {
                        Ok(v) => v,
                        Err(_) => return json_response(&serde_json::json!({ "ok": false, "error": "JSON invalide" })),
                    }
                };
                let caller = Caller { kind: info.kind };
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| api::handle(&caller, cmd, &value)))
                    .unwrap_or_else(|_| serde_json::json!({ "ok": false, "error": "erreur interne" }));
                return json_response(&result);
            }

            match assets::get(path) {
                Some((bytes, mime)) => respond(200, mime, bytes.to_vec(), mime.starts_with("text/html")),
                None => respond(404, "text/plain", b"introuvable".to_vec(), false),
            }
        }
    }
}

pub fn register_factory() {
    let mut factory = NfSchemeFactory::new();
    register_scheme_handler_factory(
        Some(&CefString::from(super::scheme_registration::SCHEME)),
        Some(&CefString::from("app")),
        Some(&mut factory),
    );
}

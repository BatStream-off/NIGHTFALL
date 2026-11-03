//! Registration of the internal `nightfall://` scheme. Must run identically in
//! every process (browser, renderer, helpers).

use cef::*;

pub const SCHEME: &str = "nightfall";

// cef_scheme_options_t bits
const STANDARD: i32 = 1 << 0;
const SECURE: i32 = 1 << 3;
const FETCH_ENABLED: i32 = 1 << 6;

pub fn register(registrar: Option<&mut SchemeRegistrar>) {
    if let Some(r) = registrar {
        // Standard + secure so pages get an origin and are treated like HTTPS.
        // Not CORS-enabled: web pages cannot fetch nightfall:// resources.
        r.add_custom_scheme(Some(&CefString::from(SCHEME)), STANDARD | SECURE | FETCH_ENABLED);
    }
}

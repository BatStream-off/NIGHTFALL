//! NIGHTFALL Security Core (engine-independent parts).
//!
//! - [`switches`]: Chromium command-line switches NIGHTFALL refuses
//!   (remote debugging, sandbox off, web security off...).
//! - [`phishing`]: local look-alike / homograph / deceptive-subdomain checks.
//! - [`extensions`]: risk classification of extension permissions.
//! - [`session_vault`]: encrypted cookie snapshot format.

pub mod extensions;
pub mod phishing;
pub mod session_vault;
pub mod switches;

//! NIGHTFALL core: profile layout, restricted file permissions and log redaction.
//!
//! Nothing in this crate touches the network or Chromium.

pub mod paths;
pub mod redact;

pub use paths::ProfilePaths;

/// Product name shown in the UI and used for directories.
pub const PRODUCT_NAME: &str = "NIGHTFALL";
/// Tagline displayed on the splash screen.
pub const TAGLINE: &str = "Browse Beyond the Darkness";
/// Version of the application (workspace version).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Seconds since the Unix epoch (wall clock).
pub fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

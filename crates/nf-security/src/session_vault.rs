//! Session Vault file format.
//!
//! On lock and on exit the browser copies every cookie (including HttpOnly
//! ones, read from the browser process) into [`CookieRecord`]s, seals them
//! with the "cookies" category key and clears Chromium's cookie store. On
//! unlock they are restored. A copied profile therefore contains no usable
//! session cookie while NIGHTFALL is locked or closed.

use nf_crypto::SecretKey;
use serde::{Deserialize, Serialize};

const AAD: &[u8] = b"nightfall/session-vault/v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CookieRecord {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
    pub secure: bool,
    pub httponly: bool,
    /// Expiry in seconds since the Unix epoch; `None` = session cookie.
    pub expires: Option<i64>,
    /// 0 unspecified, 1 none, 2 lax, 3 strict (CEF `cef_cookie_same_site_t`).
    pub same_site: i32,
    /// 0 low, 1 medium, 2 high.
    pub priority: i32,
}

impl CookieRecord {
    /// URL to use with `CefCookieManager::SetCookie`.
    pub fn url(&self) -> String {
        let host = self.domain.trim_start_matches('.');
        let scheme = if self.secure { "https" } else { "http" };
        format!("{scheme}://{host}{}", if self.path.is_empty() { "/" } else { &self.path })
    }

    pub fn expired(&self, now: i64) -> bool {
        matches!(self.expires, Some(e) if e <= now)
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Snapshot {
    created: i64,
    cookies: Vec<CookieRecord>,
}

/// Encrypt a cookie snapshot.
pub fn seal(key: &SecretKey, cookies: &[CookieRecord], now: i64) -> Result<Vec<u8>, nf_crypto::CryptoError> {
    let snap = Snapshot { created: now, cookies: cookies.to_vec() };
    let json = serde_json::to_vec(&snap).expect("serializable");
    let out = nf_crypto::seal(key, &json, AAD);
    // best-effort wipe of the clear JSON
    let mut json = json;
    for b in json.iter_mut() {
        unsafe { std::ptr::write_volatile(b, 0) };
    }
    out
}

/// Decrypt a snapshot, dropping cookies that expired meanwhile.
pub fn open(key: &SecretKey, sealed: &[u8], now: i64) -> Result<Vec<CookieRecord>, nf_crypto::CryptoError> {
    let json = nf_crypto::open(key, sealed, AAD)?;
    let snap: Snapshot = serde_json::from_slice(&json).map_err(|_| nf_crypto::CryptoError::Authentication)?;
    Ok(snap.cookies.into_iter().filter(|c| !c.expired(now)).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cookie(name: &str, expires: Option<i64>) -> CookieRecord {
        CookieRecord {
            name: name.into(),
            value: "s3cr3t-session".into(),
            domain: ".example.com".into(),
            path: "/".into(),
            secure: true,
            httponly: true,
            expires,
            same_site: 2,
            priority: 1,
        }
    }

    #[test]
    fn roundtrip_and_expiry() {
        let k = SecretKey::generate().unwrap();
        let sealed = seal(&k, &[cookie("SID", Some(2000)), cookie("old", Some(500)), cookie("s", None)], 100).unwrap();
        assert!(!sealed.windows(14).any(|w| w == b"s3cr3t-session"));
        let back = open(&k, &sealed, 1000).unwrap();
        let names: Vec<_> = back.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["SID", "s"]);
        assert!(open(&SecretKey::generate().unwrap(), &sealed, 1000).is_err());
    }

    #[test]
    fn url_for_set_cookie() {
        assert_eq!(cookie("a", None).url(), "https://example.com/");
    }
}

//! Security events: what NIGHTFALL protected you from, kept in the
//! encrypted database for a limited time.
//!
//! Only the site (host name) is kept, never the full address with its path
//! and query. At most `MAX_EVENTS` entries; entries older than the
//! retention period are removed when a new one is written and on read.

use crate::{Result, Storage};
use rusqlite::params;
use serde::{Deserialize, Serialize};

pub const MAX_EVENTS: i64 = 2000;
/// Retention chosen by the owner (06/11/2026): 30 days.
pub const RETENTION_SECONDS: i64 = 30 * 24 * 3600;

pub(crate) const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS security_events (
    id INTEGER PRIMARY KEY,
    at INTEGER NOT NULL,
    kind TEXT NOT NULL,
    site TEXT NOT NULL DEFAULT '',
    detail TEXT NOT NULL DEFAULT ''
);
CREATE INDEX IF NOT EXISTS security_events_at ON security_events(at);
"#;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecurityEvent {
    pub id: i64,
    pub at: i64,
    /// https_warning, phishing_warning, dangerous_download, wrong_code,
    /// lockdown, list_refused, critical_signal…
    pub kind: String,
    /// Host name only (`www.example.com`), or empty.
    pub site: String,
    pub detail: String,
}

/// Host name of an address, without scheme, path, query or credentials.
pub fn site_of(url: &str) -> String {
    url::Url::parse(url).ok().and_then(|u| u.host_str().map(str::to_string)).unwrap_or_default()
}

impl Storage {
    pub fn add_security_event(&self, kind: &str, url_or_site: &str, detail: &str, now: i64) -> Result<()> {
        let site = if url_or_site.contains("://") { site_of(url_or_site) } else { url_or_site.chars().take(253).collect() };
        let detail: String = detail.chars().take(300).collect();
        self.conn.execute(
            "INSERT INTO security_events(at, kind, site, detail) VALUES (?1, ?2, ?3, ?4)",
            params![now, kind, site, detail],
        )?;
        self.prune_security_events(now)
    }

    fn prune_security_events(&self, now: i64) -> Result<()> {
        self.conn.execute("DELETE FROM security_events WHERE at < ?1", [now - RETENTION_SECONDS])?;
        self.conn.execute(
            "DELETE FROM security_events WHERE id NOT IN (SELECT id FROM security_events ORDER BY at DESC, id DESC LIMIT ?1)",
            [MAX_EVENTS],
        )?;
        Ok(())
    }

    /// Most recent first.
    pub fn security_events(&self, limit: i64, now: i64) -> Result<Vec<SecurityEvent>> {
        self.prune_security_events(now)?;
        let mut st = self.conn.prepare("SELECT id, at, kind, site, detail FROM security_events ORDER BY at DESC, id DESC LIMIT ?1")?;
        let rows = st.query_map([limit.clamp(1, MAX_EVENTS)], |r| {
            Ok(SecurityEvent { id: r.get(0)?, at: r.get(1)?, kind: r.get(2)?, site: r.get(3)?, detail: r.get(4)? })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn clear_security_events(&self) -> Result<usize> {
        Ok(self.conn.execute("DELETE FROM security_events", [])?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st() -> Storage {
        Storage::open_in_memory(&nf_crypto::SecretKey::from_bytes([7u8; 32])).unwrap()
    }

    #[test]
    fn only_the_site_is_kept() {
        let s = st();
        s.add_security_event("https_warning", "http://user:pw@insecure.example/secret/path?token=abc#x", "", 100).unwrap();
        let e = s.security_events(10, 100).unwrap();
        assert_eq!(e[0].site, "insecure.example");
        let all = format!("{e:?}");
        assert!(!all.contains("secret") && !all.contains("token") && !all.contains("pw"));
    }

    #[test]
    fn retention_and_bound() {
        let s = st();
        s.add_security_event("wrong_code", "", "1 échec", 0).unwrap();
        s.add_security_event("lockdown", "", "", RETENTION_SECONDS + 10).unwrap();
        let e = s.security_events(10, RETENTION_SECONDS + 10).unwrap();
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].kind, "lockdown");
        for i in 0..(MAX_EVENTS + 5) {
            s.add_security_event("phishing_warning", "https://a.example/", "", RETENTION_SECONDS + 20 + i).unwrap();
        }
        assert_eq!(s.security_events(MAX_EVENTS + 100, RETENTION_SECONDS + 20 + MAX_EVENTS + 5).unwrap().len() as i64, MAX_EVENTS);
        assert_eq!(s.clear_security_events().unwrap() as i64, MAX_EVENTS);
        assert!(s.security_events(10, 0).unwrap().is_empty());
    }
}

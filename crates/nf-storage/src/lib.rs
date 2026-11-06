//! Encrypted local database (SQLCipher, AES-256 page encryption).
//!
//! The database key is the "database" category key from NIGHTFALL LOCK, so
//! nothing here is readable before the user unlocks with the PIN.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::path::Path;

pub mod bookmark_io;

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("base de données : {0}")]
    Sql(#[from] rusqlite::Error),
    #[error("clé de base de données incorrecte ou fichier altéré")]
    WrongKey,
    #[error("sérialisation : {0}")]
    Json(#[from] serde_json::Error),
    #[error("introuvable")]
    NotFound,
}

pub type Result<T> = std::result::Result<T, StorageError>;

const SCHEMA: &str = r#"
PRAGMA foreign_keys = ON;
CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS bookmark_folders (
    id INTEGER PRIMARY KEY,
    parent_id INTEGER REFERENCES bookmark_folders(id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    position INTEGER NOT NULL DEFAULT 0,
    modified INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS bookmarks (
    id INTEGER PRIMARY KEY,
    folder_id INTEGER REFERENCES bookmark_folders(id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    url TEXT NOT NULL,
    position INTEGER NOT NULL DEFAULT 0,
    created INTEGER NOT NULL,
    modified INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS bookmarks_url ON bookmarks(url);
CREATE TABLE IF NOT EXISTS history (
    id INTEGER PRIMARY KEY,
    url TEXT NOT NULL UNIQUE,
    title TEXT NOT NULL DEFAULT '',
    last_visit INTEGER NOT NULL,
    visit_count INTEGER NOT NULL DEFAULT 1
);
CREATE INDEX IF NOT EXISTS history_last_visit ON history(last_visit);
CREATE TABLE IF NOT EXISTS downloads (
    id INTEGER PRIMARY KEY,
    url TEXT NOT NULL,
    path TEXT NOT NULL,
    mime TEXT NOT NULL DEFAULT '',
    total_bytes INTEGER NOT NULL DEFAULT -1,
    received_bytes INTEGER NOT NULL DEFAULT 0,
    state TEXT NOT NULL,
    started INTEGER NOT NULL,
    finished INTEGER,
    signature TEXT NOT NULL DEFAULT 'unknown',
    danger TEXT NOT NULL DEFAULT ''
);
CREATE TABLE IF NOT EXISTS session_tabs (
    window_index INTEGER NOT NULL,
    tab_index INTEGER NOT NULL,
    url TEXT NOT NULL,
    title TEXT NOT NULL DEFAULT '',
    pinned INTEGER NOT NULL DEFAULT 0,
    active INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (window_index, tab_index)
);
"#;

pub struct Storage {
    conn: Connection,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Folder {
    pub id: i64,
    pub parent_id: Option<i64>,
    pub title: String,
    pub position: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bookmark {
    pub id: i64,
    pub folder_id: Option<i64>,
    pub title: String,
    pub url: String,
    pub position: i64,
    pub created: i64,
    pub modified: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: i64,
    pub url: String,
    pub title: String,
    pub last_visit: i64,
    pub visit_count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DownloadRecord {
    pub id: i64,
    pub url: String,
    pub path: String,
    pub mime: String,
    pub total_bytes: i64,
    pub received_bytes: i64,
    /// in_progress | paused | complete | cancelled | interrupted
    pub state: String,
    pub started: i64,
    pub finished: Option<i64>,
    /// unknown | signed:<signer> | unsigned | invalid | not_applicable
    pub signature: String,
    /// Empty or a human readable warning.
    pub danger: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionTab {
    pub window_index: i64,
    pub tab_index: i64,
    pub url: String,
    pub title: String,
    pub pinned: bool,
    pub active: bool,
}

/// History search filter.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct HistoryQuery {
    pub text: Option<String>,
    pub from: Option<i64>,
    pub to: Option<i64>,
    pub limit: Option<i64>,
}

fn like_pattern(q: &str) -> String {
    let escaped = q.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
    format!("%{escaped}%")
}

impl Storage {
    /// Open (or create) the encrypted database with a 256-bit raw key.
    pub fn open(path: &Path, key: &nf_crypto::SecretKey) -> Result<Self> {
        let conn = Connection::open(path)?;
        Self::init(conn, key)
    }

    /// In-memory encrypted database (tests).
    pub fn open_in_memory(key: &nf_crypto::SecretKey) -> Result<Self> {
        Self::init(Connection::open_in_memory()?, key)
    }

    fn init(conn: Connection, key: &nf_crypto::SecretKey) -> Result<Self> {
        let mut hex = key.to_hex();
        let pragma = format!("PRAGMA key = \"x'{hex}'\";");
        // Wipe the hex copy of the key as soon as SQLCipher has it.
        let res = conn.execute_batch(&pragma);
        unsafe {
            for b in hex.as_bytes_mut() {
                std::ptr::write_volatile(b, b'0');
            }
        }
        res?;
        // Any read fails with "file is not a database" if the key is wrong.
        conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| r.get::<_, i64>(0))
            .map_err(|_| StorageError::WrongKey)?;
        conn.execute_batch(SCHEMA)?;
        conn.execute(
            "INSERT OR IGNORE INTO meta(key, value) VALUES ('schema_version', '1')",
            [],
        )?;
        Ok(Self { conn })
    }

    // ---------- settings ----------

    pub fn setting<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        let v: Option<String> = self
            .conn
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0))
            .optional()?;
        Ok(match v {
            Some(s) => Some(serde_json::from_str(&s)?),
            None => None,
        })
    }

    pub fn set_setting<T: Serialize>(&self, key: &str, value: &T) -> Result<()> {
        let json = serde_json::to_string(value)?;
        self.conn.execute(
            "INSERT INTO settings(key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, json],
        )?;
        Ok(())
    }

    // ---------- bookmarks ----------

    pub fn add_folder(&self, parent_id: Option<i64>, title: &str, now: i64) -> Result<i64> {
        let pos: i64 = self.conn.query_row(
            "SELECT COALESCE(MAX(position) + 1, 0) FROM bookmark_folders WHERE parent_id IS ?1",
            [parent_id],
            |r| r.get(0),
        )?;
        self.conn.execute(
            "INSERT INTO bookmark_folders(parent_id, title, position, modified) VALUES (?1, ?2, ?3, ?4)",
            params![parent_id, title, pos, now],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn rename_folder(&self, id: i64, title: &str, now: i64) -> Result<()> {
        let n = self.conn.execute(
            "UPDATE bookmark_folders SET title = ?2, modified = ?3 WHERE id = ?1",
            params![id, title, now],
        )?;
        if n == 0 { Err(StorageError::NotFound) } else { Ok(()) }
    }

    /// Deletes the folder, its sub-folders and their bookmarks.
    pub fn delete_folder(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM bookmark_folders WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn folders(&self) -> Result<Vec<Folder>> {
        let mut st = self.conn.prepare(
            "SELECT id, parent_id, title, position FROM bookmark_folders ORDER BY parent_id, position",
        )?;
        let rows = st.query_map([], |r| {
            Ok(Folder { id: r.get(0)?, parent_id: r.get(1)?, title: r.get(2)?, position: r.get(3)? })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn add_bookmark(&self, folder_id: Option<i64>, title: &str, url: &str, now: i64) -> Result<i64> {
        let pos: i64 = self.conn.query_row(
            "SELECT COALESCE(MAX(position) + 1, 0) FROM bookmarks WHERE folder_id IS ?1",
            [folder_id],
            |r| r.get(0),
        )?;
        self.conn.execute(
            "INSERT INTO bookmarks(folder_id, title, url, position, created, modified)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
            params![folder_id, title, url, pos, now],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn update_bookmark(&self, id: i64, title: &str, url: &str, folder_id: Option<i64>, now: i64) -> Result<()> {
        let n = self.conn.execute(
            "UPDATE bookmarks SET title = ?2, url = ?3, folder_id = ?4, modified = ?5 WHERE id = ?1",
            params![id, title, url, folder_id, now],
        )?;
        if n == 0 { Err(StorageError::NotFound) } else { Ok(()) }
    }

    pub fn delete_bookmark(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM bookmarks WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn bookmark_for_url(&self, url: &str) -> Result<Option<Bookmark>> {
        Ok(self
            .conn
            .query_row(
                "SELECT id, folder_id, title, url, position, created, modified FROM bookmarks WHERE url = ?1 LIMIT 1",
                [url],
                map_bookmark,
            )
            .optional()?)
    }

    pub fn bookmarks(&self) -> Result<Vec<Bookmark>> {
        let mut st = self.conn.prepare(
            "SELECT id, folder_id, title, url, position, created, modified FROM bookmarks ORDER BY folder_id, position",
        )?;
        let rows = st.query_map([], map_bookmark)?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn search_bookmarks(&self, q: &str) -> Result<Vec<Bookmark>> {
        let pat = like_pattern(q);
        let mut st = self.conn.prepare(
            "SELECT id, folder_id, title, url, position, created, modified FROM bookmarks
             WHERE title LIKE ?1 ESCAPE '\\' OR url LIKE ?1 ESCAPE '\\' ORDER BY title LIMIT 200",
        )?;
        let rows = st.query_map([pat], map_bookmark)?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    // ---------- history ----------

    pub fn record_visit(&self, url: &str, title: &str, now: i64) -> Result<()> {
        self.conn.execute(
            "INSERT INTO history(url, title, last_visit, visit_count) VALUES (?1, ?2, ?3, 1)
             ON CONFLICT(url) DO UPDATE SET
               last_visit = excluded.last_visit,
               visit_count = visit_count + 1,
               title = CASE WHEN excluded.title = '' THEN title ELSE excluded.title END",
            params![url, title, now],
        )?;
        Ok(())
    }

    pub fn set_history_title(&self, url: &str, title: &str) -> Result<()> {
        self.conn.execute("UPDATE history SET title = ?2 WHERE url = ?1", params![url, title])?;
        Ok(())
    }

    pub fn history(&self, q: &HistoryQuery) -> Result<Vec<HistoryEntry>> {
        let pat = like_pattern(q.text.as_deref().unwrap_or(""));
        let mut st = self.conn.prepare(
            "SELECT id, url, title, last_visit, visit_count FROM history
             WHERE (title LIKE ?1 ESCAPE '\\' OR url LIKE ?1 ESCAPE '\\')
               AND last_visit >= ?2 AND last_visit <= ?3
             ORDER BY last_visit DESC LIMIT ?4",
        )?;
        let rows = st.query_map(
            params![pat, q.from.unwrap_or(0), q.to.unwrap_or(i64::MAX), q.limit.unwrap_or(500)],
            map_history,
        )?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Most visited sites, for the new tab page.
    pub fn top_sites(&self, limit: i64) -> Result<Vec<HistoryEntry>> {
        let mut st = self.conn.prepare(
            "SELECT id, url, title, last_visit, visit_count FROM history ORDER BY visit_count DESC, last_visit DESC LIMIT ?1",
        )?;
        let rows = st.query_map([limit], map_history)?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn delete_history_entry(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM history WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn delete_history_range(&self, from: i64, to: i64) -> Result<usize> {
        Ok(self
            .conn
            .execute("DELETE FROM history WHERE last_visit >= ?1 AND last_visit <= ?2", [from, to])?)
    }

    pub fn clear_history(&self) -> Result<()> {
        self.conn.execute("DELETE FROM history", [])?;
        self.conn.execute_batch("VACUUM;")?;
        Ok(())
    }

    // ---------- downloads ----------

    pub fn insert_download(&self, d: &DownloadRecord) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO downloads(url, path, mime, total_bytes, received_bytes, state, started, finished, signature, danger)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![d.url, d.path, d.mime, d.total_bytes, d.received_bytes, d.state, d.started, d.finished, d.signature, d.danger],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn update_download(&self, d: &DownloadRecord) -> Result<()> {
        self.conn.execute(
            "UPDATE downloads SET path=?2, total_bytes=?3, received_bytes=?4, state=?5, finished=?6, signature=?7, danger=?8 WHERE id=?1",
            params![d.id, d.path, d.total_bytes, d.received_bytes, d.state, d.finished, d.signature, d.danger],
        )?;
        Ok(())
    }

    pub fn downloads(&self) -> Result<Vec<DownloadRecord>> {
        let mut st = self.conn.prepare(
            "SELECT id, url, path, mime, total_bytes, received_bytes, state, started, finished, signature, danger
             FROM downloads ORDER BY started DESC LIMIT 500",
        )?;
        let rows = st.query_map([], |r| {
            Ok(DownloadRecord {
                id: r.get(0)?,
                url: r.get(1)?,
                path: r.get(2)?,
                mime: r.get(3)?,
                total_bytes: r.get(4)?,
                received_bytes: r.get(5)?,
                state: r.get(6)?,
                started: r.get(7)?,
                finished: r.get(8)?,
                signature: r.get(9)?,
                danger: r.get(10)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Forget the download records (never deletes the downloaded files).
    pub fn clear_download_history(&self) -> Result<()> {
        self.conn.execute(
            "DELETE FROM downloads WHERE state NOT IN ('in_progress', 'paused')",
            [],
        )?;
        Ok(())
    }

    /// Forget the download records started between `from` and `to`
    /// (inclusive, unix seconds). Downloads still running are kept and the
    /// downloaded files are never touched.
    pub fn delete_downloads_range(&self, from: i64, to: i64) -> Result<usize> {
        Ok(self.conn.execute(
            "DELETE FROM downloads WHERE started >= ?1 AND started <= ?2 AND state NOT IN ('in_progress', 'paused')",
            [from, to],
        )?)
    }

    // ---------- session restore ----------

    pub fn save_session(&self, tabs: &[SessionTab]) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM session_tabs", [])?;
        for t in tabs {
            tx.execute(
                "INSERT INTO session_tabs(window_index, tab_index, url, title, pinned, active) VALUES (?1,?2,?3,?4,?5,?6)",
                params![t.window_index, t.tab_index, t.url, t.title, t.pinned, t.active],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn load_session(&self) -> Result<Vec<SessionTab>> {
        let mut st = self.conn.prepare(
            "SELECT window_index, tab_index, url, title, pinned, active FROM session_tabs ORDER BY window_index, tab_index",
        )?;
        let rows = st.query_map([], |r| {
            Ok(SessionTab {
                window_index: r.get(0)?,
                tab_index: r.get(1)?,
                url: r.get(2)?,
                title: r.get(3)?,
                pinned: r.get(4)?,
                active: r.get(5)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }
}

fn map_bookmark(r: &rusqlite::Row<'_>) -> rusqlite::Result<Bookmark> {
    Ok(Bookmark {
        id: r.get(0)?,
        folder_id: r.get(1)?,
        title: r.get(2)?,
        url: r.get(3)?,
        position: r.get(4)?,
        created: r.get(5)?,
        modified: r.get(6)?,
    })
}

fn map_history(r: &rusqlite::Row<'_>) -> rusqlite::Result<HistoryEntry> {
    Ok(HistoryEntry {
        id: r.get(0)?,
        url: r.get(1)?,
        title: r.get(2)?,
        last_visit: r.get(3)?,
        visit_count: r.get(4)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use nf_crypto::SecretKey;

    fn key() -> SecretKey {
        SecretKey::from_bytes([42u8; 32])
    }

    #[test]
    fn file_is_encrypted_and_needs_the_key() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nightfall.db");
        {
            let s = Storage::open(&path, &key()).unwrap();
            s.add_bookmark(None, "Secret bank", "https://bank.example/", 1).unwrap();
        }
        let raw = std::fs::read(&path).unwrap();
        assert!(!raw.windows(11).any(|w| w == b"Secret bank"));
        assert!(!raw.starts_with(b"SQLite format 3"));
        assert!(matches!(
            Storage::open(&path, &SecretKey::from_bytes([1u8; 32])),
            Err(StorageError::WrongKey)
        ));
        let s = Storage::open(&path, &key()).unwrap();
        assert_eq!(s.bookmarks().unwrap()[0].title, "Secret bank");
    }

    #[test]
    fn settings_roundtrip() {
        let s = Storage::open_in_memory(&key()).unwrap();
        assert_eq!(s.setting::<bool>("shield").unwrap(), None);
        s.set_setting("shield", &true).unwrap();
        s.set_setting("shield", &false).unwrap();
        assert_eq!(s.setting::<bool>("shield").unwrap(), Some(false));
    }

    #[test]
    fn bookmarks_folders_and_search() {
        let s = Storage::open_in_memory(&key()).unwrap();
        let f = s.add_folder(None, "Travail", 1).unwrap();
        let sub = s.add_folder(Some(f), "Projets", 1).unwrap();
        let b1 = s.add_bookmark(Some(f), "Rust docs", "https://doc.rust-lang.org/", 1).unwrap();
        s.add_bookmark(Some(sub), "CEF", "https://github.com/chromiumembedded/cef", 1).unwrap();
        s.add_bookmark(None, "100%_off", "https://shop.example/", 1).unwrap();
        assert_eq!(s.search_bookmarks("rust").unwrap().len(), 1);
        assert_eq!(s.search_bookmarks("%").unwrap().len(), 1, "LIKE wildcards are escaped");
        assert_eq!(s.bookmark_for_url("https://doc.rust-lang.org/").unwrap().unwrap().id, b1);
        s.delete_folder(f).unwrap();
        // Folder deletion cascades to sub-folders and bookmarks.
        assert_eq!(s.bookmarks().unwrap().len(), 1);
        assert!(s.folders().unwrap().is_empty());
    }

    #[test]
    fn history_visits_filters_and_deletion() {
        let s = Storage::open_in_memory(&key()).unwrap();
        s.record_visit("https://a.example/", "A", 100).unwrap();
        s.record_visit("https://a.example/", "", 200).unwrap();
        s.record_visit("https://b.example/", "Bee", 300).unwrap();
        let all = s.history(&HistoryQuery::default()).unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].url, "https://b.example/");
        let a = all.iter().find(|h| h.url == "https://a.example/").unwrap();
        assert_eq!((a.visit_count, a.title.as_str(), a.last_visit), (2, "A", 200));
        let q = HistoryQuery { from: Some(250), ..Default::default() };
        assert_eq!(s.history(&q).unwrap().len(), 1);
        let q = HistoryQuery { text: Some("bee".into()), ..Default::default() };
        assert_eq!(s.history(&q).unwrap().len(), 1);
        assert_eq!(s.top_sites(1).unwrap()[0].url, "https://a.example/");
        assert_eq!(s.delete_history_range(0, 250).unwrap(), 1);
        s.clear_history().unwrap();
        assert!(s.history(&HistoryQuery::default()).unwrap().is_empty());
    }

    #[test]
    fn clear_browsing_data_by_time_range() {
        let s = Storage::open_in_memory(&key()).unwrap();
        let now = 1_800_000_000;
        s.record_visit("https://old.example/", "Ancien", now - 8 * 86_400).unwrap();
        s.record_visit("https://day.example/", "Hier", now - 3_600 * 5).unwrap();
        s.record_visit("https://recent.example/", "Récent", now - 600).unwrap();
        // "Last hour": only the recent visit goes.
        assert_eq!(s.delete_history_range(now - 3_600, i64::MAX).unwrap(), 1);
        let left: Vec<String> = s.history(&HistoryQuery::default()).unwrap().into_iter().map(|h| h.url).collect();
        assert_eq!(left, vec!["https://day.example/", "https://old.example/"]);
        // "Last 7 days": the visit of 5 hours ago goes, the old one stays.
        assert_eq!(s.delete_history_range(now - 7 * 86_400, i64::MAX).unwrap(), 1);
        assert_eq!(s.history(&HistoryQuery::default()).unwrap().len(), 1);
        // "All time".
        assert_eq!(s.delete_history_range(0, i64::MAX).unwrap(), 1);
        assert!(s.history(&HistoryQuery::default()).unwrap().is_empty());

        let rec = |started: i64, state: &str| DownloadRecord {
            id: 0,
            url: "https://x.example/f.zip".into(),
            path: format!("/tmp/f{started}.zip"),
            mime: String::new(),
            total_bytes: 1,
            received_bytes: 1,
            state: state.into(),
            started,
            finished: Some(started),
            signature: "unknown".into(),
            danger: String::new(),
        };
        s.insert_download(&rec(now - 30 * 86_400, "complete")).unwrap();
        s.insert_download(&rec(now - 60, "complete")).unwrap();
        s.insert_download(&rec(now - 30, "in_progress")).unwrap();
        // Last 24 hours: the finished recent record goes, the running one stays.
        assert_eq!(s.delete_downloads_range(now - 86_400, i64::MAX).unwrap(), 1);
        let states: Vec<(i64, String)> = s.downloads().unwrap().into_iter().map(|d| (d.started, d.state)).collect();
        assert_eq!(states, vec![(now - 30, "in_progress".to_string()), (now - 30 * 86_400, "complete".to_string())]);
        assert_eq!(s.delete_downloads_range(0, i64::MAX).unwrap(), 1);
        assert_eq!(s.downloads().unwrap().len(), 1);
    }

    #[test]
    fn downloads_and_session() {
        let s = Storage::open_in_memory(&key()).unwrap();
        let mut d = DownloadRecord {
            id: 0,
            url: "https://x.example/f.zip".into(),
            path: "/tmp/f.zip".into(),
            mime: "application/zip".into(),
            total_bytes: 10,
            received_bytes: 0,
            state: "in_progress".into(),
            started: 1,
            finished: None,
            signature: "unknown".into(),
            danger: String::new(),
        };
        d.id = s.insert_download(&d).unwrap();
        d.state = "complete".into();
        d.received_bytes = 10;
        s.update_download(&d).unwrap();
        assert_eq!(s.downloads().unwrap()[0], d);
        s.clear_download_history().unwrap();
        assert!(s.downloads().unwrap().is_empty());

        let tabs = vec![
            SessionTab { window_index: 0, tab_index: 0, url: "https://a/".into(), title: "A".into(), pinned: true, active: false },
            SessionTab { window_index: 0, tab_index: 1, url: "https://b/".into(), title: "B".into(), pinned: false, active: true },
        ];
        s.save_session(&tabs).unwrap();
        assert_eq!(s.load_session().unwrap(), tabs);
        s.save_session(&tabs[..1]).unwrap();
        assert_eq!(s.load_session().unwrap().len(), 1);
    }
}

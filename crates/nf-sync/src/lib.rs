//! Optional bookmark synchronisation, without any NIGHTFALL server.
//!
//! The user picks a folder that their own tools already synchronise
//! (OneDrive, iCloud Drive, Dropbox, a USB key, a NAS...). NIGHTFALL writes an
//! end-to-end encrypted file there (`nightfall-bookmarks.nfsync`), sealed with
//! a key derived from a sync passphrase (Argon2id). Each device merges the
//! file with its local bookmarks (last modification wins per URL).
//!
//! Sync is never required: without a folder, nothing happens.

use nf_crypto::SecretKey;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const FILE_NAME: &str = "nightfall-bookmarks.nfsync";
const MAGIC: &[u8; 8] = b"NFSYNC01";
const AAD: &[u8] = b"nightfall/sync/bookmarks/v1";

#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error("phrase de synchronisation incorrecte ou fichier altéré")]
    Decrypt,
    #[error("fichier de synchronisation invalide")]
    Format,
    #[error("i/o : {0}")]
    Io(#[from] std::io::Error),
    #[error("crypto : {0}")]
    Crypto(#[from] nf_crypto::CryptoError),
    #[error("phrase de synchronisation trop courte (12 caractères minimum)")]
    WeakPassphrase,
}

/// A bookmark as exchanged between devices.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncedBookmark {
    pub url: String,
    pub title: String,
    /// Folder path, e.g. ["Travail", "Projets"].
    pub folder: Vec<String>,
    pub modified: i64,
    /// Tombstone: the bookmark was deleted on some device.
    pub deleted: bool,
}

/// Interface for any sync backend (folder today; others may be added later).
pub trait SyncBackend {
    fn pull(&self) -> Result<Vec<SyncedBookmark>, SyncError>;
    fn push(&self, items: &[SyncedBookmark]) -> Result<(), SyncError>;
}

pub struct FolderSync {
    file: PathBuf,
    passphrase: String,
}

impl FolderSync {
    pub fn new(folder: &Path, passphrase: &str) -> Result<Self, SyncError> {
        if passphrase.chars().count() < 12 {
            return Err(SyncError::WeakPassphrase);
        }
        Ok(Self { file: folder.join(FILE_NAME), passphrase: passphrase.to_string() })
    }

    fn key(&self, salt: &[u8]) -> Result<SecretKey, SyncError> {
        use argon2::{Algorithm, Argon2, Params, Version};
        let params = Params::new(32 * 1024, 3, 1, Some(32)).map_err(|_| SyncError::Format)?;
        let mut out = [0u8; 32];
        Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
            .hash_password_into(self.passphrase.as_bytes(), salt, &mut out)
            .map_err(|_| SyncError::Format)?;
        Ok(SecretKey::from_bytes(out))
    }
}

impl SyncBackend for FolderSync {
    fn pull(&self) -> Result<Vec<SyncedBookmark>, SyncError> {
        if !self.file.exists() {
            return Ok(Vec::new());
        }
        let bytes = std::fs::read(&self.file)?;
        if bytes.len() < 8 + 16 || &bytes[..8] != MAGIC {
            return Err(SyncError::Format);
        }
        let (salt, sealed) = bytes[8..].split_at(16);
        let json = nf_crypto::open(&self.key(salt)?, sealed, AAD).map_err(|_| SyncError::Decrypt)?;
        serde_json::from_slice(&json).map_err(|_| SyncError::Format)
    }

    fn push(&self, items: &[SyncedBookmark]) -> Result<(), SyncError> {
        let salt = nf_crypto::random_bytes(16)?;
        let json = serde_json::to_vec(items).expect("serializable");
        let sealed = nf_crypto::seal(&self.key(&salt)?, &json, AAD)?;
        let mut out = Vec::with_capacity(8 + 16 + sealed.len());
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&salt);
        out.extend_from_slice(&sealed);
        nf_core::paths::write_private(&self.file, &out)?;
        Ok(())
    }
}

/// Merge local and remote sets: per URL, the most recent modification wins
/// (deletions included).
pub fn merge(local: &[SyncedBookmark], remote: &[SyncedBookmark]) -> Vec<SyncedBookmark> {
    let mut map: BTreeMap<String, SyncedBookmark> = BTreeMap::new();
    for b in local.iter().chain(remote.iter()) {
        match map.get(&b.url) {
            Some(existing) if existing.modified >= b.modified => {}
            _ => {
                map.insert(b.url.clone(), b.clone());
            }
        }
    }
    map.into_values().collect()
}

/// Pull, merge, push. Returns the merged set for the caller to apply locally.
pub fn synchronize(backend: &dyn SyncBackend, local: &[SyncedBookmark]) -> Result<Vec<SyncedBookmark>, SyncError> {
    let remote = backend.pull()?;
    let merged = merge(local, &remote);
    backend.push(&merged)?;
    Ok(merged)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bm(url: &str, title: &str, modified: i64, deleted: bool) -> SyncedBookmark {
        SyncedBookmark { url: url.into(), title: title.into(), folder: vec![], modified, deleted }
    }

    #[test]
    fn merge_last_write_wins() {
        let local = vec![bm("https://a/", "A local", 5, false), bm("https://b/", "B", 1, false)];
        let remote = vec![bm("https://a/", "A remote", 3, false), bm("https://b/", "B", 9, true), bm("https://c/", "C", 2, false)];
        let m = merge(&local, &remote);
        assert_eq!(m.len(), 3);
        assert_eq!(m[0].title, "A local");
        assert!(m[1].deleted);
        assert_eq!(m[2].url, "https://c/");
    }

    #[test]
    fn folder_roundtrip_two_devices() {
        let dir = tempfile::tempdir().unwrap();
        let dev1 = FolderSync::new(dir.path(), "correct horse battery").unwrap();
        let dev2 = FolderSync::new(dir.path(), "correct horse battery").unwrap();
        synchronize(&dev1, &[bm("https://a/", "A", 1, false)]).unwrap();
        let merged = synchronize(&dev2, &[bm("https://b/", "B", 2, false)]).unwrap();
        assert_eq!(merged.len(), 2);
        let raw = std::fs::read(dir.path().join(FILE_NAME)).unwrap();
        assert!(!raw.windows(9).any(|w| w == b"https://a"));
        let wrong = FolderSync::new(dir.path(), "wrong passphrase!!").unwrap();
        assert!(matches!(wrong.pull(), Err(SyncError::Decrypt)));
        assert!(matches!(FolderSync::new(dir.path(), "short"), Err(SyncError::WeakPassphrase)));
    }
}

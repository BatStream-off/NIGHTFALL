//! NIGHTFALL password vault.
//!
//! - The whole vault is one XChaCha20-Poly1305 blob sealed with the "vault"
//!   category key (from NIGHTFALL LOCK). No password is ever written in clear.
//! - Autofill is bound to the exact origin (scheme + host + port) of the
//!   entry, in the top-level frame, after a user action (enforced by the
//!   browser), so a look-alike site never receives a saved password.

use nf_crypto::SecretKey;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use zeroize::{Zeroize, ZeroizeOnDrop};

const AAD: &[u8] = b"nightfall/vault/v1";

#[derive(Debug, thiserror::Error)]
pub enum VaultError {
    #[error("coffre illisible : mauvaise clé ou fichier altéré")]
    Corrupt,
    #[error("adresse invalide : {0}")]
    BadOrigin(String),
    #[error("entrée introuvable")]
    NotFound,
    #[error("i/o : {0}")]
    Io(#[from] std::io::Error),
    #[error("crypto : {0}")]
    Crypto(#[from] nf_crypto::CryptoError),
}

#[derive(Clone, Serialize, Deserialize, Zeroize, ZeroizeOnDrop, PartialEq, Eq)]
pub struct Credential {
    pub id: String,
    /// Normalized origin, e.g. `https://accounts.example.com`.
    pub origin: String,
    pub username: String,
    pub password: String,
    pub notes: String,
    pub created: i64,
    pub updated: i64,
}

impl std::fmt::Debug for Credential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Credential")
            .field("id", &self.id)
            .field("origin", &self.origin)
            .field("username", &self.username)
            .field("password", &"[redacted]")
            .finish()
    }
}

/// Entry as shown in lists (no password).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CredentialSummary {
    pub id: String,
    pub origin: String,
    pub username: String,
    pub notes: String,
    pub updated: i64,
}

#[derive(Default, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
struct VaultData {
    entries: Vec<Credential>,
}

pub struct Vault {
    path: PathBuf,
    key: SecretKey,
    data: VaultData,
}

/// `https://Example.com:443/login?x` → `https://example.com`
pub fn normalize_origin(input: &str) -> Result<String, VaultError> {
    let u = url::Url::parse(input).map_err(|_| VaultError::BadOrigin(input.into()))?;
    if !matches!(u.scheme(), "https" | "http") {
        return Err(VaultError::BadOrigin(input.into()));
    }
    let origin = u.origin();
    if !origin.is_tuple() {
        return Err(VaultError::BadOrigin(input.into()));
    }
    Ok(origin.ascii_serialization())
}

impl Vault {
    /// Open the vault file, or start an empty one if it does not exist.
    pub fn open(path: impl AsRef<Path>, key: SecretKey) -> Result<Self, VaultError> {
        let path = path.as_ref().to_path_buf();
        let data = if path.exists() {
            let sealed = std::fs::read(&path)?;
            let mut json = nf_crypto::open(&key, &sealed, AAD).map_err(|_| VaultError::Corrupt)?;
            let data = serde_json::from_slice(&json).map_err(|_| VaultError::Corrupt);
            json.zeroize();
            data?
        } else {
            VaultData::default()
        };
        Ok(Self { path, key, data })
    }

    fn save(&self) -> Result<(), VaultError> {
        let mut json = serde_json::to_vec(&self.data).expect("serializable");
        let sealed = nf_crypto::seal(&self.key, &json, AAD);
        json.zeroize();
        nf_core::paths::write_private(&self.path, &sealed?)?;
        Ok(())
    }

    pub fn list(&self) -> Vec<CredentialSummary> {
        let mut v: Vec<_> = self
            .data
            .entries
            .iter()
            .map(|c| CredentialSummary {
                id: c.id.clone(),
                origin: c.origin.clone(),
                username: c.username.clone(),
                notes: c.notes.clone(),
                updated: c.updated,
            })
            .collect();
        v.sort_by(|a, b| a.origin.cmp(&b.origin).then(a.username.cmp(&b.username)));
        v
    }

    pub fn search(&self, q: &str) -> Vec<CredentialSummary> {
        let q = q.to_lowercase();
        self.list()
            .into_iter()
            .filter(|c| c.origin.to_lowercase().contains(&q) || c.username.to_lowercase().contains(&q) || c.notes.to_lowercase().contains(&q))
            .collect()
    }

    /// Entries usable on `page_url` (exact origin match only).
    pub fn for_page(&self, page_url: &str) -> Vec<CredentialSummary> {
        let Ok(origin) = normalize_origin(page_url) else {
            return Vec::new();
        };
        self.list().into_iter().filter(|c| c.origin == origin).collect()
    }

    /// Full credential, for autofill or reveal. The caller must check the
    /// page origin again right before filling.
    pub fn get(&self, id: &str) -> Option<Credential> {
        self.data.entries.iter().find(|c| c.id == id).cloned()
    }

    /// True if a saved password is used on another origin than its own.
    pub fn password_reused_elsewhere(&self, page_url: &str, password: &str) -> Option<String> {
        let origin = normalize_origin(page_url).ok()?;
        self.data
            .entries
            .iter()
            .find(|c| c.password == password && c.origin != origin)
            .map(|c| c.origin.clone())
    }

    pub fn add(&mut self, origin: &str, username: &str, password: &str, notes: &str, now: i64) -> Result<String, VaultError> {
        let origin = normalize_origin(origin)?;
        let id = nf_crypto::hex(&nf_crypto::random_bytes(12)?);
        self.data.entries.push(Credential {
            id: id.clone(),
            origin,
            username: username.into(),
            password: password.into(),
            notes: notes.into(),
            created: now,
            updated: now,
        });
        self.save()?;
        Ok(id)
    }

    pub fn update(&mut self, id: &str, username: &str, password: &str, notes: &str, now: i64) -> Result<(), VaultError> {
        let c = self.data.entries.iter_mut().find(|c| c.id == id).ok_or(VaultError::NotFound)?;
        c.username = username.into();
        c.password = password.into();
        c.notes = notes.into();
        c.updated = now;
        self.save()
    }

    pub fn delete(&mut self, id: &str) -> Result<(), VaultError> {
        let before = self.data.entries.len();
        self.data.entries.retain(|c| c.id != id);
        if self.data.entries.len() == before {
            return Err(VaultError::NotFound);
        }
        self.save()
    }

    pub fn len(&self) -> usize {
        self.data.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.entries.is_empty()
    }

    /// Origins with saved passwords (protected domains for anti-phishing).
    pub fn origins(&self) -> Vec<String> {
        self.data.entries.iter().map(|c| c.origin.clone()).collect()
    }
}

/// Password generator options.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct GeneratorOptions {
    pub length: usize,
    pub lowercase: bool,
    pub uppercase: bool,
    pub digits: bool,
    pub symbols: bool,
}

impl Default for GeneratorOptions {
    fn default() -> Self {
        Self { length: 20, lowercase: true, uppercase: true, digits: true, symbols: true }
    }
}

const LOWER: &[u8] = b"abcdefghijkmnopqrstuvwxyz";
const UPPER: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ";
const DIGITS: &[u8] = b"23456789";
const SYMBOLS: &[u8] = b"!#$%&()*+-./:;<=>?@[]^_{|}~";

fn uniform(n: usize) -> Result<usize, nf_crypto::CryptoError> {
    // Rejection sampling: no modulo bias.
    let limit = u32::MAX - (u32::MAX % n as u32);
    loop {
        let mut b = [0u8; 4];
        nf_crypto::fill_random(&mut b)?;
        let v = u32::from_le_bytes(b);
        if v < limit {
            return Ok((v % n as u32) as usize);
        }
    }
}

/// Generate a random password with at least one character of each class.
pub fn generate_password(opts: GeneratorOptions) -> Result<String, nf_crypto::CryptoError> {
    let mut classes: Vec<&[u8]> = Vec::new();
    if opts.lowercase { classes.push(LOWER); }
    if opts.uppercase { classes.push(UPPER); }
    if opts.digits { classes.push(DIGITS); }
    if opts.symbols { classes.push(SYMBOLS); }
    if classes.is_empty() {
        classes.push(LOWER);
    }
    let length = opts.length.clamp(classes.len().max(8), 128);
    let all: Vec<u8> = classes.iter().flat_map(|c| c.iter().copied()).collect();
    let mut out: Vec<u8> = classes.iter().map(|c| uniform(c.len()).map(|i| c[i])).collect::<Result<_, _>>()?;
    while out.len() < length {
        out.push(all[uniform(all.len())?]);
    }
    // Fisher-Yates shuffle so the guaranteed characters are not at the start.
    for i in (1..out.len()).rev() {
        let j = uniform(i + 1)?;
        out.swap(i, j);
    }
    Ok(String::from_utf8(out).expect("ascii"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vault() -> (tempfile::TempDir, Vault, SecretKey) {
        let dir = tempfile::tempdir().unwrap();
        let key = SecretKey::generate().unwrap();
        let v = Vault::open(dir.path().join("vault.bin"), key.clone()).unwrap();
        (dir, v, key)
    }

    #[test]
    fn add_persist_reopen() {
        let (dir, mut v, key) = vault();
        let id = v.add("https://Accounts.Example.com/login?x=1", "adil", "Tr0ub4dor&3", "", 1).unwrap();
        let raw = std::fs::read(dir.path().join("vault.bin")).unwrap();
        assert!(!raw.windows(11).any(|w| w == b"Tr0ub4dor&3"));
        let v2 = Vault::open(dir.path().join("vault.bin"), key).unwrap();
        let c = v2.get(&id).unwrap();
        assert_eq!(c.origin, "https://accounts.example.com");
        assert_eq!(c.password, "Tr0ub4dor&3");
        assert!(Vault::open(dir.path().join("vault.bin"), SecretKey::generate().unwrap()).is_err());
    }

    #[test]
    fn origin_bound_matching() {
        let (_d, mut v, _k) = vault();
        v.add("https://bank.example", "me", "pw1", "", 1).unwrap();
        assert_eq!(v.for_page("https://bank.example/accounts").len(), 1);
        assert!(v.for_page("http://bank.example/").is_empty(), "scheme must match");
        assert!(v.for_page("https://bank.example.evil.test/").is_empty());
        assert!(v.for_page("https://sub.bank.example/").is_empty());
        assert!(v.for_page("https://bank.example:8443/").is_empty());
        assert_eq!(v.password_reused_elsewhere("https://other.test/", "pw1").as_deref(), Some("https://bank.example"));
        assert_eq!(v.password_reused_elsewhere("https://bank.example/", "pw1"), None);
    }

    #[test]
    fn update_delete_search() {
        let (_d, mut v, _k) = vault();
        let id = v.add("https://a.example", "alice", "x", "perso", 1).unwrap();
        v.add("https://b.example", "bob", "y", "", 1).unwrap();
        assert_eq!(v.search("ALI").len(), 1);
        assert_eq!(v.search("perso").len(), 1);
        v.update(&id, "alice2", "z", "", 2).unwrap();
        assert_eq!(v.get(&id).unwrap().username, "alice2");
        v.delete(&id).unwrap();
        assert!(matches!(v.delete(&id), Err(VaultError::NotFound)));
        assert_eq!(v.len(), 1);
        assert!(v.add("javascript:alert(1)", "x", "y", "", 1).is_err());
    }

    #[test]
    fn generator() {
        for _ in 0..100 {
            let p = generate_password(GeneratorOptions::default()).unwrap();
            assert_eq!(p.len(), 20);
            assert!(p.bytes().any(|c| LOWER.contains(&c)));
            assert!(p.bytes().any(|c| UPPER.contains(&c)));
            assert!(p.bytes().any(|c| DIGITS.contains(&c)));
            assert!(p.bytes().any(|c| SYMBOLS.contains(&c)));
        }
        let p = generate_password(GeneratorOptions { length: 4, symbols: false, ..Default::default() }).unwrap();
        assert_eq!(p.len(), 8, "minimum length is 8");
        assert_ne!(generate_password(GeneratorOptions::default()).unwrap(), generate_password(GeneratorOptions::default()).unwrap());
    }

    #[test]
    fn debug_hides_password() {
        let c = Credential { id: "1".into(), origin: "https://a".into(), username: "u".into(), password: "secret".into(), notes: String::new(), created: 0, updated: 0 };
        assert!(!format!("{c:?}").contains("secret"));
    }
}

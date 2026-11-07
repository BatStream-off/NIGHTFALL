//! NIGHTFALL automatic updates.
//!
//! Each GitHub release published by CI carries `update.json` (version, and
//! the SHA-256 and size of every installer) and `update.json.sig`, an
//! Ed25519 signature of `update.json` made with a key that only exists as a
//! GitHub Actions secret. The public key is compiled into NIGHTFALL.
//!
//! An update is installed only when: the signature is valid, the signed
//! version is strictly newer than the running one (no downgrade), and the
//! downloaded installer matches the signed SHA-256 and size. Releases
//! without a valid signature are reported but never installed automatically.

use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

/// Public key matching the CI secret `NIGHTFALL_UPDATE_KEY` (Ed25519, base64).
pub const PUBLIC_KEY_B64: &str = "WQEDz+sn9HL3DbnpQFvVmy+t8dxCyHC3bpq2k7EYjk4=";
pub const RELEASES_API: &str = "https://api.github.com/repos/BatStream-off/NIGHTFALL/releases?per_page=15";
const USER_AGENT: &str = "NIGHTFALL-Updater (+https://github.com/BatStream-off/NIGHTFALL)";
/// Installers are well under this size; anything larger is refused.
const MAX_DOWNLOAD: u64 = 600 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    #[error("réseau : {0}")]
    Network(String),
    #[error("réponse invalide : {0}")]
    Invalid(String),
    #[error("signature de la mise à jour invalide")]
    BadSignature,
    #[error("la mise à jour n'est pas signée")]
    Unsigned,
    #[error("fichier téléchargé corrompu (empreinte différente)")]
    BadHash,
    #[error("aucun fichier pour cette plateforme")]
    NoAsset,
    #[error("version plus ancienne ou identique refusée")]
    NotNewer,
    #[error("i/o : {0}")]
    Io(#[from] std::io::Error),
}

/// Platform key used in the manifest.
pub fn platform() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows-x64"
    } else if cfg!(target_os = "macos") {
        "macos-arm64"
    } else {
        "linux"
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetInfo {
    pub file: String,
    pub sha256: String,
    pub size: u64,
}

/// Content of `update.json` (signed).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub version: String,
    /// platform key -> installer
    pub assets: BTreeMap<String, AssetInfo>,
}

/// A release found on GitHub.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub version: semver::Version,
    pub tag: String,
    pub page_url: String,
    /// asset name -> download URL
    pub downloads: BTreeMap<String, String>,
}

pub(crate) fn parse_version(s: &str) -> Option<semver::Version> {
    semver::Version::parse(s.trim_start_matches('v')).ok()
}

/// Pick the newest release strictly above `current` from the GitHub API JSON.
pub fn newest_release(api_json: &str, current: &str) -> Result<Option<Candidate>, UpdateError> {
    #[derive(Deserialize)]
    struct Asset {
        name: String,
        browser_download_url: String,
    }
    #[derive(Deserialize)]
    struct Release {
        tag_name: String,
        #[serde(default)]
        draft: bool,
        #[serde(default)]
        html_url: String,
        #[serde(default)]
        assets: Vec<Asset>,
    }
    let releases: Vec<Release> = serde_json::from_str(api_json).map_err(|e| UpdateError::Invalid(e.to_string()))?;
    let current = parse_version(current).ok_or_else(|| UpdateError::Invalid("version courante".into()))?;
    let best = releases
        .into_iter()
        .filter(|r| !r.draft)
        .filter_map(|r| parse_version(&r.tag_name).map(|v| (v, r)))
        .filter(|(v, _)| *v > current)
        .max_by(|a, b| a.0.cmp(&b.0));
    Ok(best.map(|(version, r)| Candidate {
        version,
        tag: r.tag_name,
        page_url: r.html_url,
        downloads: r.assets.into_iter().map(|a| (a.name, a.browser_download_url)).collect(),
    }))
}

/// Verify `update.json` against its signature and return the manifest.
pub fn verify_manifest(manifest_bytes: &[u8], signature_b64: &str, public_key_b64: &str, current: &str) -> Result<Manifest, UpdateError> {
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    let key_bytes: [u8; 32] = STANDARD
        .decode(public_key_b64.trim())
        .ok()
        .and_then(|v| v.try_into().ok())
        .ok_or(UpdateError::BadSignature)?;
    let key = VerifyingKey::from_bytes(&key_bytes).map_err(|_| UpdateError::BadSignature)?;
    let sig_bytes: [u8; 64] = STANDARD
        .decode(signature_b64.trim())
        .ok()
        .and_then(|v| v.try_into().ok())
        .ok_or(UpdateError::BadSignature)?;
    key.verify(manifest_bytes, &Signature::from_bytes(&sig_bytes)).map_err(|_| UpdateError::BadSignature)?;
    let manifest: Manifest = serde_json::from_slice(manifest_bytes).map_err(|e| UpdateError::Invalid(e.to_string()))?;
    let new = parse_version(&manifest.version).ok_or_else(|| UpdateError::Invalid("version".into()))?;
    let cur = parse_version(current).ok_or_else(|| UpdateError::Invalid("version courante".into()))?;
    if new <= cur {
        return Err(UpdateError::NotNewer);
    }
    Ok(manifest)
}

/// Check a downloaded file against the signed size and SHA-256.
pub fn verify_file(path: &Path, expected: &AssetInfo) -> Result<(), UpdateError> {
    let meta = std::fs::metadata(path)?;
    if meta.len() != expected.size {
        return Err(UpdateError::BadHash);
    }
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    let got: String = h.finalize().iter().map(|b| format!("{b:02x}")).collect();
    if !got.eq_ignore_ascii_case(expected.sha256.trim()) {
        return Err(UpdateError::BadHash);
    }
    Ok(())
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(15))
        .timeout_read(std::time::Duration::from_secs(60))
        .redirects(5)
        .build()
}

pub(crate) fn get_text(url: &str) -> Result<String, UpdateError> {
    let r = agent()
        .get(url)
        .set("User-Agent", USER_AGENT)
        .set("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| UpdateError::Network(e.to_string()))?;
    let mut s = String::new();
    r.into_reader().take(4 * 1024 * 1024).read_to_string(&mut s)?;
    Ok(s)
}

pub(crate) fn get_bytes(url: &str, limit: u64) -> Result<Vec<u8>, UpdateError> {
    let r = agent().get(url).set("User-Agent", USER_AGENT).call().map_err(|e| UpdateError::Network(e.to_string()))?;
    let mut v = Vec::new();
    r.into_reader().take(limit).read_to_end(&mut v)?;
    Ok(v)
}

/// Result of a check.
#[derive(Debug, Clone)]
pub struct Available {
    pub candidate: Candidate,
    /// Present only if the manifest signature is valid.
    pub manifest: Option<Manifest>,
    /// Why the release cannot be installed automatically, if so.
    pub not_installable: Option<String>,
}

/// Ask GitHub for a newer release and verify its signed manifest.
pub fn check(current: &str) -> Result<Option<Available>, UpdateError> {
    let json = get_text(RELEASES_API)?;
    let Some(c) = newest_release(&json, current)? else { return Ok(None) };
    let (Some(m_url), Some(s_url)) = (c.downloads.get("update.json"), c.downloads.get("update.json.sig")) else {
        return Ok(Some(Available { candidate: c, manifest: None, not_installable: Some(UpdateError::Unsigned.to_string()) }));
    };
    let m = get_bytes(m_url, 256 * 1024)?;
    let s = String::from_utf8_lossy(&get_bytes(s_url, 4096)?).to_string();
    match verify_manifest(&m, &s, PUBLIC_KEY_B64, current) {
        Ok(manifest) => {
            let ni = if manifest.assets.contains_key(platform()) { None } else { Some(UpdateError::NoAsset.to_string()) };
            Ok(Some(Available { candidate: c, manifest: Some(manifest), not_installable: ni }))
        }
        Err(e) => Ok(Some(Available { candidate: c, manifest: None, not_installable: Some(e.to_string()) })),
    }
}

/// Download this platform's installer into `dir` and verify it. `progress`
/// receives (downloaded, total).
pub fn download(available: &Available, dir: &Path, progress: &dyn Fn(u64, u64)) -> Result<PathBuf, UpdateError> {
    let manifest = available.manifest.as_ref().ok_or(UpdateError::Unsigned)?;
    let asset = manifest.assets.get(platform()).ok_or(UpdateError::NoAsset)?;
    if asset.size > MAX_DOWNLOAD || asset.file.contains(['/', '\\']) || asset.file.starts_with('.') {
        return Err(UpdateError::Invalid("fichier".into()));
    }
    let url = available.candidate.downloads.get(&asset.file).ok_or(UpdateError::NoAsset)?;
    download_asset(url, asset, dir, progress)
}

/// Download one signed asset into `dir`, then check its size and SHA-256
/// against the signed manifest; a mismatching file is deleted.
pub(crate) fn download_asset(url: &str, asset: &AssetInfo, dir: &Path, progress: &dyn Fn(u64, u64)) -> Result<PathBuf, UpdateError> {
    if asset.size > MAX_DOWNLOAD || asset.file.contains(['/', '\\']) || asset.file.starts_with('.') {
        return Err(UpdateError::Invalid("fichier".into()));
    }
    std::fs::create_dir_all(dir)?;
    let dest = dir.join(&asset.file);
    let part = dir.join(format!("{}.part", asset.file));
    {
        let r = agent().get(url).set("User-Agent", USER_AGENT).call().map_err(|e| UpdateError::Network(e.to_string()))?;
        let mut reader = r.into_reader().take(asset.size + 1);
        let mut out = std::fs::File::create(&part)?;
        let mut buf = vec![0u8; 1 << 16];
        let mut done = 0u64;
        loop {
            let n = reader.read(&mut buf)?;
            if n == 0 {
                break;
            }
            std::io::Write::write_all(&mut out, &buf[..n])?;
            done += n as u64;
            progress(done, asset.size);
        }
    }
    if let Err(e) = verify_file(&part, asset) {
        let _ = std::fs::remove_file(&part);
        return Err(e);
    }
    let _ = std::fs::remove_file(&dest);
    std::fs::rename(&part, &dest)?;
    Ok(dest)
}

pub mod player;

/// Remove leftovers of previous updates.
pub fn clean(dir: &Path) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.flatten() {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    use ed25519_dalek::{Signer, SigningKey};

    fn keypair() -> (SigningKey, String) {
        let sk = SigningKey::generate(&mut rand_core::OsRng);
        let pk = STANDARD.encode(sk.verifying_key().to_bytes());
        (sk, pk)
    }

    fn manifest(version: &str) -> Vec<u8> {
        serde_json::to_vec(&Manifest {
            version: version.into(),
            assets: [(platform().to_string(), AssetInfo { file: "x.bin".into(), sha256: "00".into(), size: 1 })].into(),
        })
        .unwrap()
    }

    #[test]
    fn signed_manifest_accepted_tampered_refused() {
        let (sk, pk) = keypair();
        let m = manifest("0.2.0");
        let sig = STANDARD.encode(sk.sign(&m).to_bytes());
        assert_eq!(verify_manifest(&m, &sig, &pk, "0.1.3").unwrap().version, "0.2.0");
        let mut tampered = m.clone();
        tampered[5] ^= 1;
        assert!(matches!(verify_manifest(&tampered, &sig, &pk, "0.1.3"), Err(UpdateError::BadSignature)));
        let (_, other_pk) = keypair();
        assert!(matches!(verify_manifest(&m, &sig, &other_pk, "0.1.3"), Err(UpdateError::BadSignature)));
    }

    #[test]
    fn no_downgrade_even_if_signed() {
        let (sk, pk) = keypair();
        for v in ["0.1.3", "0.1.2"] {
            let m = manifest(v);
            let sig = STANDARD.encode(sk.sign(&m).to_bytes());
            assert!(matches!(verify_manifest(&m, &sig, &pk, "0.1.3"), Err(UpdateError::NotNewer)));
        }
    }

    #[test]
    fn newest_release_is_picked() {
        let json = r#"[
          {"tag_name":"v0.1.2","html_url":"a","assets":[]},
          {"tag_name":"v0.2.0","html_url":"b","assets":[{"name":"update.json","browser_download_url":"u"}]},
          {"tag_name":"v0.3.0","draft":true,"assets":[]},
          {"tag_name":"nightly","assets":[]}
        ]"#;
        let c = newest_release(json, "0.1.3").unwrap().unwrap();
        assert_eq!(c.tag, "v0.2.0");
        assert_eq!(c.downloads.get("update.json").map(String::as_str), Some("u"));
        assert!(newest_release(json, "0.2.0").unwrap().is_none());
    }

    #[test]
    fn file_hash_and_size_checked() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("f");
        std::fs::write(&p, b"hello").unwrap();
        let sha = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";
        assert!(verify_file(&p, &AssetInfo { file: "f".into(), sha256: sha.into(), size: 5 }).is_ok());
        assert!(verify_file(&p, &AssetInfo { file: "f".into(), sha256: sha.into(), size: 6 }).is_err());
        assert!(verify_file(&p, &AssetInfo { file: "f".into(), sha256: "00".into(), size: 5 }).is_err());
    }

    #[test]
    fn embedded_public_key_is_valid() {
        let b = STANDARD.decode(PUBLIC_KEY_B64).unwrap();
        let arr: [u8; 32] = b.try_into().unwrap();
        assert!(VerifyingKey::from_bytes(&arr).is_ok());
    }
}

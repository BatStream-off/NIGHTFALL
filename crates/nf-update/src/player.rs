//! Lecteur NIGHTFALL : application séparée (Electron for Content Security de
//! castLabs) pour les sites vidéo protégés par Widevine. Il n'est pas inclus
//! dans l'installateur : NIGHTFALL le télécharge à la première acceptation.
//!
//! Chaque publication `lecteur-v<version>` porte `player.json` (version, taille et
//! SHA-256 du zip de chaque plate-forme) et `player.json.sig`, signature Ed25519
//! faite avec la clé de mise à jour sur `CONTEXT || player.json`. Le préfixe
//! sépare les usages de la clé : une signature du lecteur ne vaut jamais pour un
//! `update.json` de NIGHTFALL (signé sans préfixe), ni l'inverse.
//!
//! Le lecteur n'est installé ou mis à jour que si : la signature est valide, le
//! manifeste est bien celui du lecteur, la version est strictement plus récente
//! que celle installée, et le zip correspond à la taille et au SHA-256 signés.

use super::{download_asset, get_bytes, get_text, parse_version, platform, AssetInfo, UpdateError, PUBLIC_KEY_B64, RELEASES_API};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Préfixe signé avant le manifeste (identique à player/build/manifest.py).
pub const CONTEXT: &[u8] = b"NIGHTFALL-LECTEUR-MANIFESTE-v1\n";
const KIND: &str = "nightfall-lecteur";
const TAG_PREFIX: &str = "lecteur-v";
/// Nom de l'application (identique à player/build/package.mjs).
pub const APP_NAME: &str = "Lecteur NIGHTFALL";

/// Sites qui exigent une protection vidéo (Widevine) et que le lecteur ouvre.
/// Doit rester identique à `player/services.js` (vérifié par un test).
pub const SERVICES: &[(&str, &[&str])] = &[
    ("Netflix", &["netflix.com"]),
    ("Disney+", &["disneyplus.com", "disney.com", "go.com", "bamgrid.com"]),
    ("Prime Video", &["primevideo.com", "amazon.com", "amazon.fr", "amazon.de", "amazon.co.uk", "amazon.es", "amazon.it", "amazon.ca"]),
    ("Canal+", &["canalplus.com", "canal-plus.com", "mycanal.fr"]),
    ("Max", &["max.com", "hbomax.com"]),
    ("Apple TV+", &["tv.apple.com", "idmsa.apple.com", "appleid.apple.com"]),
    ("Paramount+", &["paramountplus.com"]),
    ("France.tv", &["france.tv"]),
    ("Molotov", &["molotov.tv"]),
    ("Crunchyroll", &["crunchyroll.com"]),
];

/// Domaines qui, ouverts en premier niveau, proposent le lecteur. Les domaines
/// de connexion (amazon.*, go.com, appleid…) sont acceptés par le lecteur mais
/// ne déclenchent pas la proposition à eux seuls.
pub const PROPOSE: &[&str] = &[
    "netflix.com", "disneyplus.com", "primevideo.com", "canalplus.com", "mycanal.fr", "max.com", "hbomax.com",
    "tv.apple.com", "paramountplus.com", "france.tv", "molotov.tv", "crunchyroll.com",
];

/// Hôte d'une adresse https stricte (pas d'identifiants, port 443 seulement),
/// en minuscules et sans point final ; `None` sinon.
fn https_host(url: &str) -> Option<String> {
    let rest = url.get(..8).filter(|p| p.eq_ignore_ascii_case("https://")).map(|_| &url[8..])?;
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if authority.is_empty() || authority.contains('@') || authority.contains('\\') {
        return None;
    }
    let host = match authority.rsplit_once(':') {
        Some((h, "443")) => h,
        Some(_) => return None,
        None => authority,
    };
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    if host.is_empty() || !host.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-') {
        return None;
    }
    Some(host)
}

fn host_matches(host: &str, domain: &str) -> bool {
    host == domain || host.strip_suffix(domain).is_some_and(|p| p.ends_with('.'))
}

/// Le service dont relève cette adresse (accepté par le lecteur), ou `None`.
pub fn service_for(url: &str) -> Option<&'static str> {
    let host = https_host(url)?;
    SERVICES.iter().find(|(_, ds)| ds.iter().any(|d| host_matches(&host, d))).map(|(n, _)| *n)
}

/// Faut-il proposer le lecteur sur cette page ?
pub fn should_propose(url: &str) -> Option<&'static str> {
    let host = https_host(url)?;
    if PROPOSE.iter().any(|d| host_matches(&host, d)) {
        service_for(url)
    } else {
        None
    }
}

/// Contenu de `player.json` (signé).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerManifest {
    pub kind: String,
    pub version: String,
    #[serde(default)]
    pub ecs: String,
    pub assets: BTreeMap<String, AssetInfo>,
}

/// Vérifie `player.json` et sa signature. `installed` : version installée, s'il
/// y en a une (seule une version strictement plus récente est acceptée).
pub fn verify_player_manifest(bytes: &[u8], signature_b64: &str, public_key_b64: &str, installed: Option<&str>) -> Result<PlayerManifest, UpdateError> {
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    let key_bytes: [u8; 32] = STANDARD.decode(public_key_b64.trim()).ok().and_then(|v| v.try_into().ok()).ok_or(UpdateError::BadSignature)?;
    let key = VerifyingKey::from_bytes(&key_bytes).map_err(|_| UpdateError::BadSignature)?;
    let sig: [u8; 64] = STANDARD.decode(signature_b64.trim()).ok().and_then(|v| v.try_into().ok()).ok_or(UpdateError::BadSignature)?;
    let mut signed = Vec::with_capacity(CONTEXT.len() + bytes.len());
    signed.extend_from_slice(CONTEXT);
    signed.extend_from_slice(bytes);
    key.verify(&signed, &Signature::from_bytes(&sig)).map_err(|_| UpdateError::BadSignature)?;
    let m: PlayerManifest = serde_json::from_slice(bytes).map_err(|e| UpdateError::Invalid(e.to_string()))?;
    if m.kind != KIND {
        return Err(UpdateError::Invalid("ce manifeste n'est pas celui du lecteur".into()));
    }
    let new = parse_version(&m.version).ok_or_else(|| UpdateError::Invalid("version".into()))?;
    if let Some(cur) = installed.and_then(parse_version) {
        if new <= cur {
            return Err(UpdateError::NotNewer);
        }
    }
    Ok(m)
}

/// Une publication du lecteur trouvée sur GitHub.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerRelease {
    pub version: semver::Version,
    pub tag: String,
    pub downloads: BTreeMap<String, String>,
}

/// La publication `lecteur-v*` la plus récente (brouillons exclus).
pub fn newest_player_release(api_json: &str) -> Result<Option<PlayerRelease>, UpdateError> {
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
        assets: Vec<Asset>,
    }
    let releases: Vec<Release> = serde_json::from_str(api_json).map_err(|e| UpdateError::Invalid(e.to_string()))?;
    Ok(releases
        .into_iter()
        .filter(|r| !r.draft)
        .filter_map(|r| r.tag_name.strip_prefix(TAG_PREFIX).and_then(|v| semver::Version::parse(v).ok()).map(|v| (v, r)))
        .max_by(|a, b| a.0.cmp(&b.0))
        .map(|(version, r)| PlayerRelease { version, tag: r.tag_name, downloads: r.assets.into_iter().map(|a| (a.name, a.browser_download_url)).collect() }))
}

/// Lecteur disponible au téléchargement, manifeste vérifié.
#[derive(Debug, Clone)]
pub struct PlayerAvailable {
    pub release: PlayerRelease,
    pub manifest: PlayerManifest,
    pub asset: AssetInfo,
}

/// Cherche une version du lecteur plus récente que `installed` (ou n'importe
/// laquelle s'il n'est pas installé). `Ok(None)` : rien de plus récent.
pub fn check(installed: Option<&str>) -> Result<Option<PlayerAvailable>, UpdateError> {
    let json = get_text(RELEASES_API)?;
    let Some(r) = newest_player_release(&json)? else { return Ok(None) };
    if let Some(cur) = installed.and_then(parse_version) {
        if r.version <= cur {
            return Ok(None);
        }
    }
    let (Some(m_url), Some(s_url)) = (r.downloads.get("player.json"), r.downloads.get("player.json.sig")) else {
        return Err(UpdateError::Unsigned);
    };
    let m = get_bytes(m_url, 64 * 1024)?;
    let s = String::from_utf8_lossy(&get_bytes(s_url, 4096)?).to_string();
    let manifest = verify_player_manifest(&m, &s, PUBLIC_KEY_B64, installed)?;
    if manifest.version != r.version.to_string() {
        return Err(UpdateError::Invalid("version du manifeste différente de la publication".into()));
    }
    let asset = manifest.assets.get(platform()).cloned().ok_or(UpdateError::NoAsset)?;
    Ok(Some(PlayerAvailable { release: r, manifest, asset }))
}

/// Télécharge le zip de cette plate-forme dans `dir` et le vérifie.
pub fn download(available: &PlayerAvailable, dir: &Path, progress: &dyn Fn(u64, u64)) -> Result<PathBuf, UpdateError> {
    let url = available.release.downloads.get(&available.asset.file).ok_or(UpdateError::NoAsset)?;
    download_asset(url, &available.asset, dir, progress)
}

/// Exécutable du lecteur dans un dossier de version.
pub fn executable_in(dir: &Path) -> PathBuf {
    if cfg!(target_os = "macos") {
        dir.join(format!("{APP_NAME}.app")).join("Contents").join("MacOS").join(APP_NAME)
    } else {
        dir.join(format!("{APP_NAME}.exe"))
    }
}

/// Version installée (la plus récente présente dans `root` et complète).
pub fn installed(root: &Path) -> Option<(String, PathBuf)> {
    let mut best: Option<(semver::Version, PathBuf)> = None;
    for e in std::fs::read_dir(root).ok()?.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        let Ok(v) = semver::Version::parse(&name) else { continue };
        let exe = executable_in(&e.path());
        if exe.is_file() && best.as_ref().is_none_or(|(b, _)| v > *b) {
            best = Some((v, exe));
        }
    }
    best.map(|(v, p)| (v.to_string(), p))
}

/// Extrait le zip vérifié avec l'outil du système (macOS : ditto, qui garde les
/// liens symboliques des frameworks ; Windows : tar.exe, qui refuse les chemins
/// absolus et « .. »), dans un dossier temporaire, puis le met en place sous
/// `root/<version>` et retire les versions précédentes.
pub fn install(zip: &Path, root: &Path, version: &str) -> Result<PathBuf, UpdateError> {
    let v = semver::Version::parse(version).map_err(|_| UpdateError::Invalid("version".into()))?;
    std::fs::create_dir_all(root)?;
    let staging = root.join(format!(".installation-{v}"));
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging)?;
    let status = extract_command(zip, &staging)?.status()?;
    if !status.success() {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(UpdateError::Invalid("archive du lecteur illisible".into()));
    }
    if !executable_in(&staging).is_file() {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(UpdateError::Invalid("archive du lecteur incomplète".into()));
    }
    let dest = root.join(v.to_string());
    let _ = std::fs::remove_dir_all(&dest);
    std::fs::rename(&staging, &dest)?;
    for e in std::fs::read_dir(root)?.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if name != v.to_string() && (semver::Version::parse(&name).is_ok() || name.starts_with(".installation-")) {
            let _ = std::fs::remove_dir_all(e.path());
        }
    }
    Ok(executable_in(&dest))
}

fn extract_command(zip: &Path, dest: &Path) -> Result<std::process::Command, UpdateError> {
    if cfg!(target_os = "macos") {
        let mut c = std::process::Command::new("/usr/bin/ditto");
        c.arg("-x").arg("-k").arg(zip).arg(dest);
        Ok(c)
    } else if cfg!(target_os = "windows") {
        let root = std::env::var_os("SystemRoot").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
        let mut c = std::process::Command::new(root.join("System32").join("tar.exe"));
        c.arg("-x").arg("-f").arg(zip).arg("-C").arg(dest);
        Ok(c)
    } else {
        Err(UpdateError::Invalid("plateforme non prise en charge par le lecteur".into()))
    }
}

/// Supprime le lecteur (toutes les versions). Ses données de navigation sont
/// dans son propre dossier système et sont retirées par l'appelant.
pub fn remove(root: &Path) -> Result<(), UpdateError> {
    match std::fs::remove_dir_all(root) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
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

    fn manifest(kind: &str, version: &str) -> Vec<u8> {
        serde_json::to_vec(&PlayerManifest {
            kind: kind.into(),
            version: version.into(),
            ecs: "45.2.1+wvcus".into(),
            assets: [(platform().to_string(), AssetInfo { file: "Lecteur.zip".into(), sha256: "00".into(), size: 1 })].into(),
        })
        .unwrap()
    }

    fn sign_player(sk: &SigningKey, m: &[u8]) -> String {
        let mut signed = CONTEXT.to_vec();
        signed.extend_from_slice(m);
        STANDARD.encode(sk.sign(&signed).to_bytes())
    }

    #[test]
    fn player_signature_with_context_accepted_tampered_refused() {
        let (sk, pk) = keypair();
        let m = manifest(KIND, "0.1.0");
        let sig = sign_player(&sk, &m);
        assert_eq!(verify_player_manifest(&m, &sig, &pk, None).unwrap().version, "0.1.0");
        let mut tampered = m.clone();
        tampered[10] ^= 1;
        assert!(matches!(verify_player_manifest(&tampered, &sig, &pk, None), Err(UpdateError::BadSignature)));
        let (_, other) = keypair();
        assert!(matches!(verify_player_manifest(&m, &sig, &other, None), Err(UpdateError::BadSignature)));
    }

    #[test]
    fn signatures_are_not_interchangeable_with_nightfall_updates() {
        let (sk, pk) = keypair();
        // Un manifeste du lecteur se lit aussi comme un update.json (champs version et
        // assets) : sa signature (avec préfixe) ne doit pas valoir pour NIGHTFALL…
        let m = manifest(KIND, "9.0.0");
        let player_sig = sign_player(&sk, &m);
        assert!(matches!(crate::verify_manifest(&m, &player_sig, &pk, "0.3.1"), Err(UpdateError::BadSignature)));
        // …et une signature de mise à jour (sans préfixe) ne vaut pas pour le lecteur.
        let update_sig = STANDARD.encode(sk.sign(&m).to_bytes());
        assert!(matches!(verify_player_manifest(&m, &update_sig, &pk, None), Err(UpdateError::BadSignature)));
    }

    #[test]
    fn other_kind_and_older_versions_refused() {
        let (sk, pk) = keypair();
        let m = manifest("nightfall", "0.2.0");
        assert!(matches!(verify_player_manifest(&m, &sign_player(&sk, &m), &pk, None), Err(UpdateError::Invalid(_))));
        for v in ["0.2.0", "0.1.9"] {
            let m = manifest(KIND, v);
            assert!(matches!(verify_player_manifest(&m, &sign_player(&sk, &m), &pk, Some("0.2.0")), Err(UpdateError::NotNewer)));
        }
        let m = manifest(KIND, "0.3.0");
        assert!(verify_player_manifest(&m, &sign_player(&sk, &m), &pk, Some("0.2.0")).is_ok());
    }

    #[test]
    fn newest_player_release_ignores_nightfall_and_drafts() {
        let json = r#"[
          {"tag_name":"v0.4.0","assets":[]},
          {"tag_name":"lecteur-v0.1.0","assets":[{"name":"player.json","browser_download_url":"a"}]},
          {"tag_name":"lecteur-v0.2.0","assets":[{"name":"player.json","browser_download_url":"b"}]},
          {"tag_name":"lecteur-v0.3.0","draft":true,"assets":[]},
          {"tag_name":"lecteur-vbad","assets":[]}
        ]"#;
        let r = newest_player_release(json).unwrap().unwrap();
        assert_eq!(r.tag, "lecteur-v0.2.0");
        assert_eq!(r.downloads.get("player.json").map(String::as_str), Some("b"));
        // Les publications du lecteur ne sont jamais prises pour des versions de NIGHTFALL.
        assert_eq!(crate::newest_release(json, "0.3.1").unwrap().unwrap().tag, "v0.4.0");
        assert!(newest_player_release(r#"[{"tag_name":"v1.0.0","assets":[]}]"#).unwrap().is_none());
    }

    #[test]
    fn services_accept_only_exact_https_hosts() {
        for ok in ["https://www.netflix.com/browse", "https://netflix.com", "https://WWW.NETFLIX.COM./x", "https://www.netflix.com:443/",
            "https://www.amazon.fr/ap/signin", "https://www.disneyplus.com/fr-fr"] {
            assert!(service_for(ok).is_some(), "{ok}");
        }
        for bad in ["https://evilnetflix.com/", "http://www.netflix.com/", "https://netflix.com.example.org/", "https://user:pw@www.netflix.com/",
            "https://www.netflix.com:8443/", "javascript:alert(1)", "https://", "https://netflix.com\\@evil.org/", "nightfall://app/newtab.html"] {
            assert!(service_for(bad).is_none(), "{bad}");
        }
        assert_eq!(should_propose("https://www.netflix.com/watch/1"), Some("Netflix"));
        assert_eq!(should_propose("https://www.amazon.fr/"), None, "un site de connexion seul ne propose pas le lecteur");
        assert_eq!(should_propose("https://www.primevideo.com/"), Some("Prime Video"));
    }

    #[test]
    fn service_list_matches_the_player() {
        let js = include_str!("../../../player/services.js");
        for (name, domains) in SERVICES {
            assert!(js.contains(&format!("name: '{name}'")), "{name} absent de player/services.js");
            for d in *domains {
                assert!(js.contains(&format!("'{d}'")), "{d} absent de player/services.js");
            }
        }
        assert_eq!(js.matches("{ name: '").count(), SERVICES.len(), "player/services.js a d'autres services");
        let all: Vec<&str> = SERVICES.iter().flat_map(|(_, d)| d.iter().copied()).collect();
        for d in PROPOSE {
            assert!(all.contains(d), "{d} proposé mais non accepté par le lecteur");
        }
    }

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    #[test]
    fn install_extracts_and_keeps_only_the_new_version() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        let exe = executable_in(&src);
        std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
        std::fs::write(&exe, b"lecteur").unwrap();
        let zip = tmp.path().join("p.zip");
        let ok = if cfg!(target_os = "macos") {
            std::process::Command::new("/usr/bin/ditto").args(["-c", "-k", "--keepParent"]).arg(src.join(format!("{APP_NAME}.app"))).arg(&zip).status()
        } else {
            std::process::Command::new("tar").args(["-a", "-c", "-f"]).arg(&zip).arg("-C").arg(&src).arg(format!("{APP_NAME}.exe")).status()
        };
        assert!(ok.unwrap().success());
        let root = tmp.path().join("lecteur");
        std::fs::create_dir_all(root.join("0.0.9")).unwrap();
        let installed_exe = install(&zip, &root, "0.1.0").unwrap();
        assert_eq!(std::fs::read(&installed_exe).unwrap(), b"lecteur");
        assert_eq!(installed(&root).map(|(v, _)| v), Some("0.1.0".to_string()));
        assert!(!root.join("0.0.9").exists());
        // Archive sans l'exécutable attendu : refusée, rien n'est installé.
        std::fs::write(tmp.path().join("vide.txt"), b"x").unwrap();
        let bad = tmp.path().join("bad.zip");
        if cfg!(target_os = "macos") {
            std::process::Command::new("/usr/bin/ditto").args(["-c", "-k"]).arg(tmp.path().join("vide.txt")).arg(&bad).status().unwrap();
        } else {
            std::process::Command::new("tar").args(["-a", "-c", "-f"]).arg(&bad).arg("-C").arg(tmp.path()).arg("vide.txt").status().unwrap();
        }
        assert!(install(&bad, &root, "0.2.0").is_err());
        assert_eq!(installed(&root).map(|(v, _)| v), Some("0.1.0".to_string()));
        remove(&root).unwrap();
        assert!(installed(&root).is_none());
    }
}

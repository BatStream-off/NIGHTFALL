//! Risk classification of Chromium extension permissions (Manifest V3).

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Risk {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PermissionFinding {
    pub permission: String,
    pub risk: Risk,
    pub explanation: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExtensionAssessment {
    pub name: String,
    pub version: String,
    pub manifest_version: u64,
    pub overall: Risk,
    pub findings: Vec<PermissionFinding>,
}

fn classify(permission: &str) -> Option<(Risk, &'static str)> {
    Some(match permission {
        "debugger" => (Risk::Critical, "peut contrôler les pages comme un outil de développement (lecture de tout, y compris les sessions)"),
        "nativeMessaging" => (Risk::Critical, "peut communiquer avec un programme installé sur l'ordinateur"),
        "cookies" => (Risk::High, "peut lire et modifier les cookies, donc les sessions de connexion"),
        "webRequest" | "webRequestBlocking" => (Risk::High, "peut observer le trafic des sites"),
        "declarativeNetRequestWithHostAccess" => (Risk::Medium, "peut modifier les requêtes réseau des sites autorisés"),
        "proxy" => (Risk::High, "peut faire passer tout le trafic par un serveur tiers"),
        "management" => (Risk::High, "peut gérer les autres extensions"),
        "history" => (Risk::Medium, "peut lire l'historique de navigation"),
        "tabs" => (Risk::Medium, "peut voir les adresses et titres de tous les onglets"),
        "scripting" => (Risk::Medium, "peut injecter du code dans les pages autorisées"),
        "clipboardRead" => (Risk::Medium, "peut lire le presse-papiers"),
        "downloads" => (Risk::Medium, "peut lancer et gérer des téléchargements"),
        "privacy" => (Risk::Medium, "peut modifier des réglages de confidentialité"),
        "storage" | "alarms" | "contextMenus" | "notifications" | "activeTab" | "declarativeNetRequest" | "i18n" | "sidePanel" => {
            (Risk::Low, "permission courante à faible impact")
        }
        _ => return None,
    })
}

fn is_broad_host(pattern: &str) -> bool {
    matches!(pattern, "<all_urls>" | "*://*/*" | "http://*/*" | "https://*/*")
}

/// Assess a `manifest.json` (as JSON text).
pub fn assess_manifest(manifest_json: &str) -> Result<ExtensionAssessment, serde_json::Error> {
    let m: serde_json::Value = serde_json::from_str(manifest_json)?;
    let name = m["name"].as_str().unwrap_or("?").to_string();
    let version = m["version"].as_str().unwrap_or("?").to_string();
    let manifest_version = m["manifest_version"].as_u64().unwrap_or(0);
    let mut findings = Vec::new();

    let mut perms: Vec<String> = Vec::new();
    for key in ["permissions", "optional_permissions"] {
        if let Some(arr) = m[key].as_array() {
            perms.extend(arr.iter().filter_map(|v| v.as_str().map(str::to_string)));
        }
    }
    let mut hosts: Vec<String> = Vec::new();
    for key in ["host_permissions", "optional_host_permissions"] {
        if let Some(arr) = m[key].as_array() {
            hosts.extend(arr.iter().filter_map(|v| v.as_str().map(str::to_string)));
        }
    }
    if let Some(cs) = m["content_scripts"].as_array() {
        for s in cs {
            if let Some(arr) = s["matches"].as_array() {
                hosts.extend(arr.iter().filter_map(|v| v.as_str().map(str::to_string)));
            }
        }
    }
    // MV2 mixes host patterns into "permissions".
    let (host_like, api): (Vec<String>, Vec<String>) =
        perms.into_iter().partition(|p| p.contains("://") || p == "<all_urls>");
    hosts.extend(host_like);

    for p in api {
        if let Some((risk, explanation)) = classify(&p) {
            findings.push(PermissionFinding { permission: p, risk, explanation });
        } else {
            findings.push(PermissionFinding { permission: p, risk: Risk::Medium, explanation: "permission non répertoriée par NIGHTFALL" });
        }
    }
    hosts.sort();
    hosts.dedup();
    for h in hosts {
        let broad = is_broad_host(&h);
        findings.push(PermissionFinding {
            permission: h,
            risk: if broad { Risk::High } else { Risk::Medium },
            explanation: if broad { "accès à tous les sites" } else { "accès à certains sites" },
        });
    }
    // Cookies + all sites = can steal every session.
    let has_cookies = findings.iter().any(|f| f.permission == "cookies");
    let has_all_sites = findings.iter().any(|f| is_broad_host(&f.permission));
    let mut overall = findings.iter().map(|f| f.risk).max().unwrap_or(Risk::Low);
    if has_cookies && has_all_sites {
        overall = Risk::Critical;
    }
    if manifest_version < 3 {
        overall = overall.max(Risk::Medium);
    }
    Ok(ExtensionAssessment { name, version, manifest_version, overall, findings })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn benign_extension_is_low() {
        let a = assess_manifest(r#"{"name":"Dark","version":"1","manifest_version":3,"permissions":["storage","activeTab"]}"#).unwrap();
        assert_eq!(a.overall, Risk::Low);
    }

    #[test]
    fn cookie_stealer_is_critical() {
        let a = assess_manifest(r#"{"name":"Coupon","version":"2","manifest_version":3,"permissions":["cookies","tabs"],"host_permissions":["<all_urls>"]}"#).unwrap();
        assert_eq!(a.overall, Risk::Critical);
        assert!(a.findings.iter().any(|f| f.permission == "<all_urls>" && f.risk == Risk::High));
    }

    #[test]
    fn mv2_host_permissions_are_detected() {
        let a = assess_manifest(r#"{"name":"Old","version":"0.1","manifest_version":2,"permissions":["https://*/*","debugger"]}"#).unwrap();
        assert_eq!(a.overall, Risk::Critical);
        assert!(a.findings.iter().any(|f| f.permission == "https://*/*"));
    }
}

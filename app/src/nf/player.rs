//! Lecteur NIGHTFALL pour les sites vidéo protégés (voir crates/nf-update/src/player.rs
//! pour les règles de vérification et player/ pour l'application elle-même).
//!
//! - Proposé par une barre sur les sites de la liste (`nf_update::player::should_propose`).
//! - Téléchargé seulement après l'accord de l'utilisateur, jamais inclus dans
//!   l'installateur ; vérifié (manifeste signé, SHA-256, taille) avant installation
//!   dans le profil (`<profil>/lecteur/<version>`).
//! - Mis à jour par le même chemin, au rythme des vérifications de NIGHTFALL.
//! - Lancé avec l'adresse demandée et le chemin de NIGHTFALL, pour que toute
//!   autre adresse revienne ici.

use super::state::{self, core};
use super::ui_thread::{background, on_ui};
use super::windows;
use nf_update::player as p;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Mutex;

#[derive(Default)]
struct PlayerState {
    busy: bool,
    /// « recherche » | « téléchargement » | « installation »
    phase: &'static str,
    progress: f64,
    size: Option<u64>,
    error: Option<String>,
    last_check: Option<i64>,
}

static STATE: Mutex<Option<PlayerState>> = Mutex::new(None);

fn with<R>(f: impl FnOnce(&mut PlayerState) -> R) -> R {
    let mut g = STATE.lock().unwrap_or_else(|e| e.into_inner());
    f(g.get_or_insert_with(PlayerState::default))
}

fn root() -> PathBuf {
    core().paths.root.join("lecteur")
}

fn download_dir() -> PathBuf {
    core().paths.root.join("lecteur-telechargement")
}

/// Dossier des données du lecteur (cookies des services), propre au lecteur.
fn player_data_dir() -> Option<PathBuf> {
    if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support").join(p::APP_NAME))
    } else if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join(p::APP_NAME))
    } else {
        None
    }
}

pub fn status() -> Value {
    let installed = p::installed(&root());
    let bar = core().settings.player_bar;
    with(|s| {
        json!({
            "ok": true,
            "supported": cfg!(any(target_os = "macos", target_os = "windows")),
            "installed": installed.as_ref().map(|(v, _)| v.clone()),
            "busy": s.busy,
            "phase": s.phase,
            "progress": s.progress,
            "size": s.size,
            "error": s.error,
            "bar": bar,
            "services": p::SERVICES.iter().map(|(n, _)| *n).collect::<Vec<_>>(),
        })
    })
}

fn notify() {
    let st = status();
    on_ui(move || windows::push_event_all(&json!({ "type": "player", "status": st })));
}

/// Faut-il proposer le lecteur pour cette adresse ? (nom du service)
pub fn propose(url: &str) -> Value {
    json!({ "ok": true, "service": p::should_propose(url), "bar": core().settings.player_bar })
}

/// Ouvre l'adresse dans le lecteur s'il est installé.
pub fn open(url: &str) -> Value {
    if p::service_for(url).is_none() {
        return json!({ "ok": false, "error": "Ce site ne s'ouvre pas dans le lecteur NIGHTFALL." });
    }
    match p::installed(&root()) {
        Some((_, exe)) => match launch(&exe, url) {
            Ok(()) => json!({ "ok": true }),
            Err(e) => json!({ "ok": false, "error": format!("Le lecteur n'a pas pu démarrer : {e}") }),
        },
        None => json!({ "ok": false, "notInstalled": true }),
    }
}

fn launch(exe: &std::path::Path, url: &str) -> std::io::Result<()> {
    let me = std::env::current_exe()?;
    std::process::Command::new(exe)
        .arg(format!("--nf-browser={}", me.display()))
        .arg(url)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
}

/// Télécharge (après l'accord de l'utilisateur), vérifie et installe le lecteur,
/// ou sa mise à jour, puis ouvre `then_open` s'il est donné.
pub fn install(then_open: Option<String>) -> Value {
    if !cfg!(any(target_os = "macos", target_os = "windows")) {
        return json!({ "ok": false, "error": "Le lecteur NIGHTFALL existe pour Windows et macOS seulement." });
    }
    if let Some(u) = &then_open {
        if p::service_for(u).is_none() {
            return json!({ "ok": false, "error": "Ce site ne s'ouvre pas dans le lecteur NIGHTFALL." });
        }
    }
    let started = with(|s| {
        if s.busy {
            return false;
        }
        *s = PlayerState { busy: true, phase: "recherche", last_check: s.last_check, ..Default::default() };
        true
    });
    if !started {
        return json!({ "ok": false, "error": "Le lecteur est déjà en cours d'installation." });
    }
    notify();
    let (root, dir) = (root(), download_dir());
    background(
        move || -> Result<Option<(String, PathBuf)>, String> {
            let installed = p::installed(&root);
            let current = installed.as_ref().map(|(v, _)| v.clone());
            let Some(a) = p::check(current.as_deref()).map_err(|e| e.to_string())? else {
                return Ok(installed);
            };
            with(|s| {
                s.phase = "téléchargement";
                s.size = Some(a.asset.size);
            });
            notify();
            let _ = std::fs::remove_dir_all(&dir);
            let zip = p::download(&a, &dir, &|done, total| {
                let first = with(|s| {
                    let before = (s.progress * 50.0) as i64;
                    s.progress = if total > 0 { done as f64 / total as f64 } else { 0.0 };
                    before != (s.progress * 50.0) as i64
                });
                if first {
                    notify();
                }
            })
            .map_err(|e| e.to_string())?;
            with(|s| s.phase = "installation");
            notify();
            let exe = p::install(&zip, &root, &a.manifest.version).map_err(|e| e.to_string());
            let _ = std::fs::remove_dir_all(&dir);
            exe.map(|e| Some((a.manifest.version.clone(), e)))
        },
        move |res| {
            let result = with(|s| {
                s.busy = false;
                s.phase = "";
                s.last_check = Some(nf_core::unix_now());
                match &res {
                    Ok(_) => s.error = None,
                    Err(e) => s.error = Some(e.clone()),
                }
                res
            });
            match result {
                Ok(Some((version, exe))) => {
                    state::log(&format!("lecteur NIGHTFALL {version} prêt"));
                    if let Some(url) = then_open {
                        if let Err(e) = launch(&exe, &url) {
                            with(|s| s.error = Some(format!("Le lecteur n'a pas pu démarrer : {e}")));
                        }
                    }
                }
                Ok(None) => with(|s| s.error = Some("Aucune version du lecteur n'est publiée pour le moment.".into())),
                Err(e) => state::log(&format!("lecteur NIGHTFALL : {e}")),
            }
            notify();
        },
    );
    json!({ "ok": true, "started": true })
}

/// Mise à jour automatique (minuterie des mises à jour) : seulement si le
/// lecteur est déjà installé, et au plus toutes les 6 heures.
pub fn tick() {
    let due = with(|s| !s.busy && s.last_check.map(|t| nf_core::unix_now() - t > 6 * 3600).unwrap_or(true));
    if due && core().is_unlocked() && p::installed(&root()).is_some() {
        with(|s| s.last_check = Some(nf_core::unix_now()));
        install(None);
    }
}

pub fn set_bar(body: &Value) -> Value {
    if let Some(v) = body.get("bar").and_then(Value::as_bool) {
        let mut c = core();
        c.settings.player_bar = v;
        c.save_settings();
    }
    status()
}

/// Supprime le lecteur et, si demandé, ses données (connexions aux services).
pub fn remove(body: &Value) -> Value {
    if with(|s| s.busy) {
        return json!({ "ok": false, "error": "Le lecteur est en cours d'installation." });
    }
    if let Err(e) = p::remove(&root()) {
        return json!({ "ok": false, "error": format!("Suppression impossible (le lecteur est-il ouvert ?) : {e}") });
    }
    if body.get("data").and_then(Value::as_bool).unwrap_or(false) {
        if let Some(d) = player_data_dir() {
            if let Err(e) = std::fs::remove_dir_all(&d) {
                if e.kind() != std::io::ErrorKind::NotFound {
                    return json!({ "ok": false, "error": format!("Lecteur supprimé, mais pas ses données (le lecteur est-il ouvert ?) : {e}") });
                }
            }
        }
    }
    with(|s| s.error = None);
    notify();
    json!({ "ok": true })
}

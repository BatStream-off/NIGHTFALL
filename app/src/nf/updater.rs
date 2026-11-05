//! Automatic updates (see crates/nf-update for the verification rules).
//!
//! - Check: at start-up (after unlock) and every 6 hours, if enabled.
//! - Download: in the background when "install automatically" is on, or
//!   when the user asks for it. The installer is verified (signed manifest,
//!   SHA-256, size) before it is kept.
//! - Install: when NIGHTFALL quits (automatic mode), or immediately with a
//!   restart when the user clicks "Redémarrer pour mettre à jour".

use super::state::{self, core};
use super::ui_thread::{background, on_ui};
use super::windows;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UpdatePrefs {
    pub auto_check: bool,
    pub auto_install: bool,
}

impl Default for UpdatePrefs {
    fn default() -> Self {
        Self { auto_check: true, auto_install: true }
    }
}

#[derive(Default)]
struct UpdState {
    prefs: UpdatePrefs,
    checking: bool,
    downloading: bool,
    progress: f64,
    last_check: Option<i64>,
    available: Option<nf_update::Available>,
    ready: Option<PathBuf>,
    error: Option<String>,
}

static STATE: Mutex<Option<UpdState>> = Mutex::new(None);

fn with<R>(f: impl FnOnce(&mut UpdState) -> R) -> R {
    let mut g = STATE.lock().unwrap_or_else(|p| p.into_inner());
    f(g.get_or_insert_with(UpdState::default))
}

fn dir() -> PathBuf {
    core().paths.root.join("updates")
}

/// Load preferences once the encrypted database is open.
pub fn load_prefs() {
    let prefs = core().storage().and_then(|s| super::state::load_setting::<UpdatePrefs>(s, "updates")).unwrap_or_default();
    with(|s| s.prefs = prefs);
}

fn save_prefs() {
    let prefs = with(|s| s.prefs.clone());
    if let Some(st) = core().storage() {
        let _ = st.set_setting("updates", &prefs);
    }
}

/// Install mode for this copy of NIGHTFALL, or why it cannot update itself.
fn install_target() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    if cfg!(target_os = "macos") {
        // .../NIGHTFALL.app/Contents/MacOS/nightfall
        let app = exe.ancestors().nth(3).map(PathBuf::from).ok_or("emplacement inconnu")?;
        if app.extension().map(|e| e != "app").unwrap_or(true) {
            return Err("copie de développement : mise à jour manuelle".into());
        }
        let parent = app.parent().ok_or("emplacement inconnu")?;
        let probe = parent.join(format!(".nightfall-write-test-{}", std::process::id()));
        std::fs::write(&probe, b"").map_err(|_| "le dossier de NIGHTFALL n'est pas modifiable : installez la mise à jour manuellement".to_string())?;
        let _ = std::fs::remove_file(probe);
        if app.to_string_lossy().contains("/Volumes/") {
            return Err("NIGHTFALL est lancé depuis l'image disque : copiez-le dans Applications".into());
        }
        Ok(app)
    } else if cfg!(target_os = "windows") {
        let installed = exe.with_file_name("unins000.exe").exists();
        if !installed {
            return Err("version portable : téléchargez la nouvelle version manuellement".into());
        }
        Ok(exe)
    } else {
        Err("plateforme non prise en charge".into())
    }
}

pub fn status() -> Value {
    let target = install_target();
    with(|s| {
        let a = s.available.as_ref();
        json!({
            "ok": true,
            "current": nf_core::VERSION,
            "latest": a.map(|a| a.candidate.version.to_string()),
            "available": a.is_some(),
            "releaseUrl": a.map(|a| a.candidate.page_url.clone()),
            "signed": a.map(|a| a.manifest.is_some()).unwrap_or(false),
            "notInstallable": a.and_then(|a| a.not_installable.clone()).or_else(|| target.as_ref().err().cloned()),
            "checking": s.checking,
            "downloading": s.downloading,
            "progress": s.progress,
            "readyToInstall": s.ready.is_some(),
            "lastCheck": s.last_check,
            "error": s.error,
            "autoCheck": s.prefs.auto_check,
            "autoInstall": s.prefs.auto_install,
        })
    })
}

pub fn set(body: &Value) -> Value {
    with(|s| {
        if let Some(v) = body.get("autoCheck").and_then(Value::as_bool) {
            s.prefs.auto_check = v;
        }
        if let Some(v) = body.get("autoInstall").and_then(Value::as_bool) {
            s.prefs.auto_install = v;
        }
    });
    save_prefs();
    status()
}

fn notify() {
    let st = status();
    on_ui(move || windows::push_event_all(&json!({ "type": "update", "status": st })));
}

/// Check now (background). `then_download`: download if installable.
pub fn check(then_download: bool) {
    let busy = with(|s| {
        if s.checking || s.downloading {
            true
        } else {
            s.checking = true;
            s.error = None;
            false
        }
    });
    if busy {
        return;
    }
    notify();
    background(
        || nf_update::check(nf_core::VERSION),
        move |res| {
            let auto = with(|s| {
                s.checking = false;
                s.last_check = Some(nf_core::unix_now());
                match res {
                    Ok(a) => {
                        if a.as_ref().map(|a| a.candidate.version.to_string()) != s.available.as_ref().map(|a| a.candidate.version.to_string()) {
                            s.ready = None;
                        }
                        s.available = a;
                    }
                    Err(e) => s.error = Some(e.to_string()),
                }
                then_download || s.prefs.auto_install
            });
            if let Some(a) = with(|s| s.available.clone()) {
                state::log(&format!("mise à jour disponible : {}", a.candidate.version));
                if auto && a.not_installable.is_none() && install_target().is_ok() {
                    download();
                }
            }
            notify();
        },
    );
}

fn download() {
    let Some(a) = with(|s| {
        if s.downloading || s.ready.is_some() {
            None
        } else {
            s.downloading = true;
            s.progress = 0.0;
            s.available.clone()
        }
    }) else {
        return;
    };
    let d = dir();
    notify();
    background(
        move || {
            nf_update::clean(&d);
            nf_update::download(&a, &d, &|done, total| {
                with(|s| s.progress = if total > 0 { done as f64 / total as f64 } else { 0.0 });
            })
        },
        |res| {
            with(|s| {
                s.downloading = false;
                match res {
                    Ok(p) => s.ready = Some(p),
                    Err(e) => s.error = Some(e.to_string()),
                }
            });
            if let Some(e) = with(|s| s.error.clone()) {
                state::log(&format!("mise à jour : {e}"));
            }
            notify();
        },
    );
}

/// Periodic check (timers): every 6 hours when enabled.
pub fn tick() {
    let due = with(|s| s.prefs.auto_check && !s.checking && s.last_check.map(|t| nf_core::unix_now() - t > 6 * 3600).unwrap_or(true));
    if due && core().is_unlocked() {
        check(false);
    }
}

/// "Redémarrer pour mettre à jour": install now and relaunch.
pub fn install_now() -> Value {
    let ready = with(|s| s.ready.clone());
    let Some(file) = ready else {
        // Not downloaded yet: download, the user clicks again when ready.
        check(true);
        return json!({ "ok": false, "error": "La mise à jour est en cours de téléchargement." });
    };
    match spawn_installer(&file, true) {
        Ok(()) => {
            state::log("mise à jour : installation et redémarrage");
            on_ui(|| {
                for id in windows::window_ids() {
                    windows::close_window(id);
                }
            });
            json!({ "ok": true })
        }
        Err(e) => json!({ "ok": false, "error": e }),
    }
}

/// NIGHTFALL is quitting: install a downloaded update silently (no restart).
pub fn on_quit() {
    let (ready, auto) = with(|s| (s.ready.clone(), s.prefs.auto_install));
    if let (Some(file), true) = (ready, auto) {
        if let Err(e) = spawn_installer(&file, false) {
            state::log(&format!("mise à jour à la fermeture : {e}"));
        }
    }
}

fn spawn_installer(file: &std::path::Path, relaunch: bool) -> Result<(), String> {
    let target = install_target()?;
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        let _ = target;
        let mut cmd = std::process::Command::new(file);
        cmd.args(["/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/CLOSEAPPLICATIONS"]);
        if relaunch {
            cmd.arg("/NFRELAUNCH");
        }
        cmd.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
        cmd.spawn().map_err(|e| e.to_string())?;
        Ok(())
    }
    #[cfg(target_os = "macos")]
    {
        let script = file.with_file_name("install.sh");
        std::fs::write(&script, MAC_INSTALL_SCRIPT).map_err(|e| e.to_string())?;
        std::process::Command::new("/bin/sh")
            .arg(&script)
            .arg(std::process::id().to_string())
            .arg(file)
            .arg(&target)
            .arg(if relaunch { "1" } else { "0" })
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let _ = (file, relaunch, target);
        Err("plateforme non prise en charge".into())
    }
}

/// Waits for NIGHTFALL to exit, replaces the app bundle with the verified
/// one from the disk image, then relaunches it if asked. Arguments: pid,
/// dmg, app path, relaunch (1/0).
#[cfg(target_os = "macos")]
const MAC_INSTALL_SCRIPT: &str = r#"#!/bin/sh
PID="$1"; DMG="$2"; APP="$3"; RELAUNCH="$4"
i=0
while kill -0 "$PID" 2>/dev/null; do sleep 0.5; i=$((i+1)); [ $i -gt 240 ] && exit 1; done
MNT=$(mktemp -d /tmp/nightfall-update.XXXXXX) || exit 1
hdiutil attach -nobrowse -readonly -noautoopen -mountpoint "$MNT" "$DMG" >/dev/null || exit 1
NEW="$MNT/NIGHTFALL.app"
TMP="$(dirname "$APP")/.NIGHTFALL-update-$$.app"
if [ -d "$NEW" ] && ditto "$NEW" "$TMP"; then
  hdiutil detach "$MNT" -quiet
  OLD="$(dirname "$APP")/.NIGHTFALL-old-$$.app"
  mv "$APP" "$OLD" && mv "$TMP" "$APP" && rm -rf "$OLD"
  xattr -dr com.apple.quarantine "$APP" 2>/dev/null
else
  hdiutil detach "$MNT" -quiet
  rm -rf "$TMP"
fi
rm -f "$DMG"
[ "$RELAUNCH" = "1" ] && open "$APP"
exit 0
"#;

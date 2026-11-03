//! `cargo xtask <command>`
//!
//! - `bundle [--release]`: build NIGHTFALL and assemble a runnable app with
//!   the CEF runtime (`dist/NIGHTFALL.app` on macOS, `dist/NIGHTFALL/` on Windows).
//! - `package`: bundle in release mode, then create the distributable
//!   (`.dmg` on macOS; on Windows the folder is zipped and the installer is
//!   built by Inno Setup in CI, see packaging/windows/nightfall.iss).

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

const APP: &str = "nightfall";
const HELPER: &str = "nightfall_helper";
const VERSION: &str = env!("CARGO_PKG_VERSION");

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

fn cargo_build(release: bool) -> Result<()> {
    let mut cmd = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()));
    cmd.current_dir(workspace_root()).args(["build", "-p", APP]);
    if release {
        cmd.arg("--release");
    }
    if let Ok(features) = std::env::var("NIGHTFALL_FEATURES") {
        if !features.is_empty() {
            cmd.args(["--features", &features]);
        }
    }
    let status = cmd.status().context("cargo build")?;
    if !status.success() {
        bail!("la compilation de NIGHTFALL a échoué");
    }
    Ok(())
}

fn target_dir(release: bool) -> PathBuf {
    let base = std::env::var_os("CARGO_TARGET_DIR").map(PathBuf::from).unwrap_or_else(|| workspace_root().join("target"));
    base.join(if release { "release" } else { "debug" })
}

fn dist_dir() -> PathBuf {
    std::env::var_os("NIGHTFALL_DIST_DIR").map(PathBuf::from).unwrap_or_else(|| workspace_root().join("dist"))
}

#[cfg(target_os = "macos")]
fn bundle(release: bool) -> Result<PathBuf> {
    use cef::build_util::mac::{bundle, BundleInfo};
    cargo_build(release)?;
    let out = dist_dir();
    std::fs::create_dir_all(&out)?;
    let final_path = out.join("NIGHTFALL.app");
    if final_path.exists() {
        std::fs::remove_dir_all(&final_path)?;
    }
    let info = BundleInfo::new(APP, "app.nightfall.browser", "NIGHTFALL", "French", semver::Version::parse(VERSION)?);
    let resources = workspace_root().join("app").join("resources");
    let app_path = bundle(&out, &target_dir(release), APP, HELPER, Some(resources), info)
        .map_err(|e| anyhow::anyhow!("bundle macOS : {e}"))?;
    std::fs::rename(&app_path, &final_path)?;
    // Ad-hoc signature: required for the app to start on Apple Silicon. A
    // Developer ID signature + notarisation needs a paid Apple account.
    let status = Command::new("codesign")
        .args(["--force", "--deep", "--sign", "-"])
        .arg(&final_path)
        .status()?;
    if !status.success() {
        bail!("codesign ad hoc a échoué");
    }
    println!("Application : {}", final_path.display());
    Ok(final_path)
}

#[cfg(target_os = "windows")]
fn bundle(release: bool) -> Result<PathBuf> {
    cargo_build(release)?;
    let target = target_dir(release);
    // cef's bundler copies the .pdb next to the DLL; create an empty one if
    // the release profile did not produce it.
    let pdb = target.join(format!("{APP}.pdb"));
    if !pdb.exists() {
        std::fs::write(&pdb, b"")?;
    }
    let out = dist_dir().join("NIGHTFALL");
    if out.exists() {
        std::fs::remove_dir_all(&out)?;
    }
    std::fs::create_dir_all(&out)?;
    let exe = cef::build_util::win::bundle(&out, &target, APP).map_err(|e| anyhow::anyhow!("bundle Windows : {e}"))?;
    let _ = std::fs::remove_file(out.join(format!("{APP}.pdb")));
    println!("Application : {}", exe.display());
    Ok(out)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn bundle(release: bool) -> Result<PathBuf> {
    cargo_build(release)?;
    let out = dist_dir().join("nightfall-linux");
    if out.exists() {
        std::fs::remove_dir_all(&out)?;
    }
    std::fs::create_dir_all(&out)?;
    let exe = cef::build_util::linux::bundle(&out, &target_dir(release), APP).map_err(|e| anyhow::anyhow!("bundle Linux : {e}"))?;
    println!("Application (développement uniquement) : {}", exe.display());
    Ok(out)
}

fn package() -> Result<()> {
    let bundled = bundle(true)?;
    #[cfg(target_os = "macos")]
    {
        let arch = if cfg!(target_arch = "aarch64") { "arm64" } else { "x64" };
        let dmg = dist_dir().join(format!("NIGHTFALL-{VERSION}-macos-{arch}.dmg"));
        let staging = dist_dir().join("dmg");
        if staging.exists() {
            std::fs::remove_dir_all(&staging)?;
        }
        std::fs::create_dir_all(&staging)?;
        let status = Command::new("ditto").arg(&bundled).arg(staging.join("NIGHTFALL.app")).status()?;
        if !status.success() {
            bail!("ditto a échoué");
        }
        #[cfg(unix)]
        std::os::unix::fs::symlink("/Applications", staging.join("Applications"))?;
        let status = Command::new("hdiutil")
            .args(["create", "-volname", "NIGHTFALL", "-format", "UDZO", "-ov", "-srcfolder"])
            .arg(&staging)
            .arg(&dmg)
            .status()?;
        if !status.success() {
            bail!("hdiutil a échoué");
        }
        println!("Image disque : {}", dmg.display());
    }
    #[cfg(not(target_os = "macos"))]
    {
        println!("Dossier prêt à empaqueter : {}", bundled.display());
    }
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let release = args.iter().any(|a| a == "--release");
    match args.first().map(String::as_str) {
        Some("bundle") => bundle(release).map(|_| ()),
        Some("package") => package(),
        _ => {
            eprintln!("usage : cargo xtask bundle [--release] | cargo xtask package");
            std::process::exit(2);
        }
    }
}

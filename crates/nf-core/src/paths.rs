//! On-disk layout of a NIGHTFALL profile.
//!
//! Everything NIGHTFALL writes lives under one root directory, so that the
//! emergency purge can be restricted to that root (see `nf-emergency`).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Paths of every file and directory NIGHTFALL owns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfilePaths {
    /// Root of all NIGHTFALL data.
    pub root: PathBuf,
}

impl ProfilePaths {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Default root for the current OS:
    /// - Windows: `%LOCALAPPDATA%\NIGHTFALL`
    /// - macOS: `~/Library/Application Support/NIGHTFALL`
    /// - Linux (dev only): `$XDG_DATA_HOME/nightfall` or `~/.local/share/nightfall`
    pub fn default_root() -> Option<PathBuf> {
        if let Some(custom) = std::env::var_os("NIGHTFALL_PROFILE_DIR") {
            return Some(PathBuf::from(custom));
        }
        #[cfg(target_os = "windows")]
        {
            std::env::var_os("LOCALAPPDATA").map(|p| PathBuf::from(p).join("NIGHTFALL"))
        }
        #[cfg(target_os = "macos")]
        {
            std::env::var_os("HOME").map(|h| {
                PathBuf::from(h)
                    .join("Library")
                    .join("Application Support")
                    .join("NIGHTFALL")
            })
        }
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        {
            if let Some(x) = std::env::var_os("XDG_DATA_HOME") {
                return Some(PathBuf::from(x).join("nightfall"));
            }
            std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share/nightfall"))
        }
    }

    /// Key store: wrapped master key, Argon2id parameters, recovery envelope.
    pub fn keystore(&self) -> PathBuf {
        self.root.join("keystore.json")
    }
    /// Authenticated counter of failed unlock attempts.
    pub fn lock_state(&self) -> PathBuf {
        self.root.join("lock.state")
    }
    /// Software fallback secret (only when no hardware-backed store exists).
    pub fn software_secret(&self) -> PathBuf {
        self.root.join("device.secret")
    }
    /// SQLCipher database (bookmarks, history, settings, downloads, sessions).
    pub fn database(&self) -> PathBuf {
        self.root.join("nightfall.db")
    }
    /// Encrypted password vault.
    pub fn vault(&self) -> PathBuf {
        self.root.join("vault.bin")
    }
    /// Encrypted cookie snapshot written by the Session Vault on lock/exit.
    pub fn session_vault(&self) -> PathBuf {
        self.root.join("sessions.vault")
    }
    /// Chromium profile directory handed to CEF (`root_cache_path`).
    pub fn chromium_profile(&self) -> PathBuf {
        self.root.join("chromium")
    }
    /// Downloaded filter lists and compiled Shield cache.
    pub fn shield_cache(&self) -> PathBuf {
        self.root.join("shield")
    }
    /// Non-sensitive UI preferences readable before unlock (splash on/off).
    pub fn ui_prefs(&self) -> PathBuf {
        self.root.join("ui-prefs.json")
    }
    /// Marker describing a purge that must run before Chromium starts.
    pub fn pending_purge(&self) -> PathBuf {
        self.root.join("purge.pending")
    }
    /// Local logs (already redacted).
    pub fn logs(&self) -> PathBuf {
        self.root.join("logs")
    }

    /// Create the root and core sub-directories with owner-only permissions.
    pub fn ensure(&self) -> io::Result<()> {
        for dir in [
            self.root.clone(),
            self.chromium_profile(),
            self.shield_cache(),
            self.logs(),
        ] {
            fs::create_dir_all(&dir)?;
            restrict_dir(&dir)?;
        }
        Ok(())
    }
}

/// Restrict a directory to its owner (0700 on Unix). On Windows the profile
/// lives in `%LOCALAPPDATA%`, whose inherited ACL already limits access to the
/// user, SYSTEM and administrators.
pub fn restrict_dir(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}

/// Write a file atomically (temp file + rename) with owner-only permissions.
pub fn write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    use std::io::Write;
    let dir = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no parent directory"))?;
    fs::create_dir_all(dir)?;
    let tmp = dir.join(format!(
        ".{}.tmp",
        path.file_name().and_then(|n| n.to_str()).unwrap_or("nf")
    ));
    {
        let mut f = fs::File::create(&tmp)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            f.set_permissions(fs::Permissions::from_mode(0o600))?;
        }
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_is_under_root() {
        let p = ProfilePaths::new("/tmp/nf-test-root");
        for path in [
            p.keystore(),
            p.lock_state(),
            p.database(),
            p.vault(),
            p.session_vault(),
            p.chromium_profile(),
            p.shield_cache(),
            p.ui_prefs(),
            p.pending_purge(),
        ] {
            assert!(path.starts_with(&p.root), "{path:?}");
        }
    }

    #[test]
    fn ensure_and_write_private() {
        let dir = tempfile::tempdir().unwrap();
        let p = ProfilePaths::new(dir.path().join("profile"));
        p.ensure().unwrap();
        write_private(&p.ui_prefs(), b"{}").unwrap();
        assert_eq!(fs::read(p.ui_prefs()).unwrap(), b"{}");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&p.root).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o700);
            let mode = fs::metadata(p.ui_prefs()).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
    }
}

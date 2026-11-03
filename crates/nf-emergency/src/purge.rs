//! Data purge restricted to the NIGHTFALL profile directory.
//!
//! Key material is destroyed first by the caller (cryptographic erase, see
//! `nf-auth`); this module then deletes the files. Overwriting is best effort:
//! SSD wear levelling, file-system snapshots, backups and cloud sync may keep
//! older copies, which no software can guarantee to erase.

use nf_core::ProfilePaths;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum PurgeCategory {
    /// Cookies, sessions and local tokens: Session Vault + Chromium cookies,
    /// local storage, IndexedDB (whole Chromium profile).
    Sessions,
    /// History, bookmarks, settings, downloads metadata (SQLCipher database).
    BrowsingData,
    /// Password vault.
    Passwords,
    /// Extensions and their data (inside the Chromium profile).
    Extensions,
    /// Filter lists cache.
    ShieldCache,
    /// Everything, including the key store and the device secret.
    Everything,
}

impl PurgeCategory {
    pub fn all() -> &'static [PurgeCategory] {
        &[
            PurgeCategory::Sessions,
            PurgeCategory::BrowsingData,
            PurgeCategory::Passwords,
            PurgeCategory::Extensions,
            PurgeCategory::ShieldCache,
            PurgeCategory::Everything,
        ]
    }

    /// Key-store categories (nf-auth) to destroy for cryptographic erase.
    pub fn key_categories(self) -> &'static [&'static str] {
        match self {
            PurgeCategory::Sessions => &["cookies"],
            PurgeCategory::BrowsingData => &["database", "sync"],
            PurgeCategory::Passwords => &["vault"],
            PurgeCategory::Extensions | PurgeCategory::ShieldCache => &[],
            PurgeCategory::Everything => &["cookies", "database", "sync", "vault"],
        }
    }

    /// Does this purge require Chromium to be stopped first (its files are
    /// open while it runs)? Such purges run at the next start, before CEF.
    pub fn needs_engine_stopped(self) -> bool {
        matches!(self, PurgeCategory::Sessions | PurgeCategory::Extensions | PurgeCategory::Everything)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PurgeError {
    #[error("chemin hors du profil NIGHTFALL refusé : {0}")]
    OutsideProfile(PathBuf),
    #[error("profil introuvable : {0}")]
    NoProfile(PathBuf),
    #[error("i/o : {0}")]
    Io(#[from] io::Error),
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
pub struct PurgeReport {
    pub files_removed: u64,
    pub bytes_overwritten: u64,
    pub errors: Vec<String>,
}

/// The set of paths a purge will delete, validated against the profile root.
#[derive(Debug, Clone)]
pub struct PurgePlan {
    root: PathBuf,
    targets: Vec<PathBuf>,
    pub categories: Vec<PurgeCategory>,
}

impl PurgePlan {
    pub fn new(paths: &ProfilePaths, categories: &[PurgeCategory]) -> Result<Self, PurgeError> {
        let root = fs::canonicalize(&paths.root).map_err(|_| PurgeError::NoProfile(paths.root.clone()))?;
        let rel = |p: PathBuf| -> PathBuf {
            // Paths are built from the canonical root so prefix checks are exact.
            root.join(p.strip_prefix(&paths.root).unwrap_or(&p))
        };
        let mut targets = Vec::new();
        for c in categories {
            match c {
                PurgeCategory::Sessions => {
                    targets.push(rel(paths.session_vault()));
                    targets.push(rel(paths.chromium_profile()));
                }
                PurgeCategory::BrowsingData => {
                    targets.push(rel(paths.database()));
                    for suffix in ["-wal", "-shm", "-journal"] {
                        targets.push(rel(PathBuf::from(format!("{}{suffix}", paths.database().display()))));
                    }
                }
                PurgeCategory::Passwords => targets.push(rel(paths.vault())),
                PurgeCategory::Extensions => {
                    let chromium = rel(paths.chromium_profile());
                    for name in ["Extensions", "Local Extension Settings", "Extension State", "Extension Rules", "Extension Scripts"] {
                        targets.push(chromium.join("Default").join(name));
                    }
                }
                PurgeCategory::ShieldCache => targets.push(rel(paths.shield_cache())),
                PurgeCategory::Everything => targets.push(root.clone()),
            }
        }
        targets.sort();
        targets.dedup();
        for t in &targets {
            if !t.starts_with(&root) {
                return Err(PurgeError::OutsideProfile(t.clone()));
            }
        }
        let mut cats = categories.to_vec();
        cats.sort();
        cats.dedup();
        Ok(Self { root, targets, categories: cats })
    }

    pub fn targets(&self) -> &[PathBuf] {
        &self.targets
    }

    pub fn needs_engine_stopped(&self) -> bool {
        self.categories.iter().any(|c| c.needs_engine_stopped())
    }

    /// Delete every target. Symbolic links are removed, never followed.
    pub fn execute(&self) -> PurgeReport {
        let mut report = PurgeReport::default();
        for t in &self.targets {
            if let Err(e) = remove_within(&self.root, t, &mut report) {
                if e.kind() != io::ErrorKind::NotFound {
                    report.errors.push(format!("{}: {e}", t.display()));
                }
            }
        }
        report
    }

    /// Persist the plan so it runs at next start, before Chromium opens its files.
    pub fn schedule(&self, paths: &ProfilePaths) -> io::Result<()> {
        let json = serde_json::to_vec(&self.categories).expect("serializable");
        nf_core::paths::write_private(&paths.pending_purge(), &json)
    }

    /// Load a scheduled purge, if any (does not execute it).
    pub fn pending(paths: &ProfilePaths) -> Option<Vec<PurgeCategory>> {
        let bytes = fs::read(paths.pending_purge()).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    pub fn clear_pending(paths: &ProfilePaths) {
        let _ = fs::remove_file(paths.pending_purge());
    }
}

fn overwrite(path: &Path, len: u64, report: &mut PurgeReport) {
    use std::io::Write;
    // Best effort: files above 64 MiB are only deleted (cache), their content
    // is already unreadable without the destroyed keys or is not sensitive.
    if len == 0 || len > 64 * 1024 * 1024 {
        return;
    }
    if let Ok(mut f) = fs::OpenOptions::new().write(true).open(path) {
        let zeros = vec![0u8; 64 * 1024];
        let mut left = len;
        while left > 0 {
            let n = left.min(zeros.len() as u64) as usize;
            if f.write_all(&zeros[..n]).is_err() {
                return;
            }
            left -= n as u64;
        }
        let _ = f.sync_all();
        report.bytes_overwritten += len;
    }
}

fn remove_within(root: &Path, target: &Path, report: &mut PurgeReport) -> io::Result<()> {
    if !target.starts_with(root) {
        return Err(io::Error::new(io::ErrorKind::PermissionDenied, "outside profile"));
    }
    let meta = fs::symlink_metadata(target)?;
    let ft = meta.file_type();
    if ft.is_symlink() {
        // Remove the link itself; its destination may be outside the profile.
        fs::remove_file(target).or_else(|_| fs::remove_dir(target))?;
        report.files_removed += 1;
        return Ok(());
    }
    if ft.is_dir() {
        for entry in fs::read_dir(target)? {
            let entry = entry?;
            if let Err(e) = remove_within(root, &entry.path(), report) {
                report.errors.push(format!("{}: {e}", entry.path().display()));
            }
        }
        fs::remove_dir(target)?;
        return Ok(());
    }
    overwrite(target, meta.len(), report);
    fs::remove_file(target)?;
    report.files_removed += 1;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile() -> (tempfile::TempDir, ProfilePaths) {
        let dir = tempfile::tempdir().unwrap();
        let paths = ProfilePaths::new(dir.path().join("NIGHTFALL"));
        paths.ensure().unwrap();
        fs::write(paths.database(), b"db").unwrap();
        fs::write(paths.vault(), b"vault").unwrap();
        fs::write(paths.session_vault(), b"cookies").unwrap();
        fs::create_dir_all(paths.chromium_profile().join("Default/Extensions/abc")).unwrap();
        fs::write(paths.chromium_profile().join("Default/Extensions/abc/manifest.json"), b"{}").unwrap();
        fs::write(paths.chromium_profile().join("Default/Cookies"), b"chromium cookies").unwrap();
        (dir, paths)
    }

    #[test]
    fn purge_selected_categories_only() {
        let (_d, paths) = profile();
        let plan = PurgePlan::new(&paths, &[PurgeCategory::Passwords, PurgeCategory::Extensions]).unwrap();
        let r = plan.execute();
        assert!(r.errors.is_empty(), "{:?}", r.errors);
        assert!(!paths.vault().exists());
        assert!(!paths.chromium_profile().join("Default/Extensions").exists());
        assert!(paths.database().exists());
        assert!(paths.chromium_profile().join("Default/Cookies").exists());
    }

    #[test]
    fn everything_spares_files_outside_profile() {
        let (dir, paths) = profile();
        // Witness files: a "Downloads" folder next to the profile, and a file
        // outside the profile that a symlink inside the profile points to.
        let downloads = dir.path().join("Downloads");
        fs::create_dir_all(&downloads).unwrap();
        fs::write(downloads.join("photo.jpg"), b"precious").unwrap();
        let outside = dir.path().join("outside.txt");
        fs::write(&outside, b"keep me").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, paths.root.join("link-to-outside")).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&downloads, paths.root.join("link-to-downloads")).unwrap();

        let plan = PurgePlan::new(&paths, &[PurgeCategory::Everything]).unwrap();
        assert!(plan.needs_engine_stopped());
        let r = plan.execute();
        assert!(r.errors.is_empty(), "{:?}", r.errors);
        assert!(!paths.root.exists());
        assert_eq!(fs::read(downloads.join("photo.jpg")).unwrap(), b"precious");
        assert_eq!(fs::read(&outside).unwrap(), b"keep me");
    }

    #[test]
    fn schedule_and_reload() {
        let (_d, paths) = profile();
        let plan = PurgePlan::new(&paths, &[PurgeCategory::Sessions, PurgeCategory::Sessions]).unwrap();
        assert_eq!(plan.categories, vec![PurgeCategory::Sessions]);
        plan.schedule(&paths).unwrap();
        assert_eq!(PurgePlan::pending(&paths), Some(vec![PurgeCategory::Sessions]));
        PurgePlan::clear_pending(&paths);
        assert_eq!(PurgePlan::pending(&paths), None);
    }

    #[test]
    fn key_categories_cover_everything() {
        let mut all: Vec<&str> = PurgeCategory::Everything.key_categories().to_vec();
        all.sort();
        assert_eq!(all, ["cookies", "database", "sync", "vault"]);
    }
}

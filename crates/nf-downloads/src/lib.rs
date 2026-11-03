//! Download safety checks.
//!
//! - Dangerous file types get a warning; nothing is ever opened or executed
//!   automatically.
//! - Executables, installers and disk images get a signature check:
//!   Authenticode (`WinVerifyTrust`) on Windows, `codesign`/`pkgutil`/`spctl`
//!   on macOS.
//! - Files get the OS "downloaded from the Internet" marker (Mark-of-the-Web
//!   on Windows, `com.apple.quarantine` on macOS) so SmartScreen and
//!   Gatekeeper check them when opened.

use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Danger {
    None,
    /// Programs, scripts, installers.
    Executable,
    /// Office documents with macros, archives that may hide programs.
    Risky,
}

const EXECUTABLE: &[&str] = &[
    "exe", "msi", "msix", "appx", "bat", "cmd", "com", "scr", "pif", "cpl", "ps1", "psm1", "vbs", "vbe",
    "js", "jse", "wsf", "wsh", "hta", "jar", "lnk", "reg", "dll", "sys", "app", "pkg", "dmg", "command",
    "sh", "bash", "zsh", "py", "pl", "rb", "deb", "rpm", "apk", "iso", "img", "vhd", "vhdx", "scpt",
];
const RISKY: &[&str] = &[
    "docm", "xlsm", "pptm", "dotm", "xlam", "zip", "rar", "7z", "tar", "gz", "cab", "one", "svg", "html", "htm",
];

pub fn classify(file_name: &str) -> Danger {
    let ext = Path::new(file_name)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();
    if EXECUTABLE.contains(&ext.as_str()) {
        Danger::Executable
    } else if RISKY.contains(&ext.as_str()) {
        Danger::Risky
    } else {
        Danger::None
    }
}

pub fn danger_message(d: Danger) -> &'static str {
    match d {
        Danger::None => "",
        Danger::Executable => "Ce fichier peut exécuter du code sur votre ordinateur. Ouvrez-le uniquement si vous faites confiance à sa source.",
        Danger::Risky => "Ce type de fichier peut contenir des macros ou des programmes cachés. Vérifiez sa provenance.",
    }
}

/// Characters forbidden in file names on Windows/macOS, plus control chars
/// and path separators; also blocks "..", hidden dot-files and reserved names.
pub fn sanitize_file_name(name: &str) -> String {
    let mut out: String = name
        .chars()
        .map(|c| if c.is_control() || "<>:\"/\\|?*".contains(c) { '_' } else { c })
        .collect();
    out = out.trim_matches(|c: char| c == '.' || c.is_whitespace()).to_string();
    let stem = out.split('.').next().unwrap_or("").to_ascii_uppercase();
    let reserved = ["CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "LPT1", "LPT2", "LPT3"];
    if out.is_empty() || reserved.contains(&stem.as_str()) {
        out = format!("telechargement_{out}");
    }
    if out.len() > 200 {
        let ext = Path::new(&out).extension().and_then(|e| e.to_str()).unwrap_or("").to_string();
        out.truncate(190);
        if !ext.is_empty() {
            out.push('.');
            out.push_str(&ext);
        }
    }
    out
}

/// A path in `dir` that does not exist yet: `name`, `name (1)`, `name (2)`...
pub fn unique_path(dir: &Path, name: &str) -> PathBuf {
    let name = sanitize_file_name(name);
    let candidate = dir.join(&name);
    if !candidate.exists() {
        return candidate;
    }
    let p = Path::new(&name);
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("fichier");
    let ext = p.extension().and_then(|s| s.to_str());
    for i in 1..10_000 {
        let n = match ext {
            Some(e) => format!("{stem} ({i}).{e}"),
            None => format!("{stem} ({i})"),
        };
        let c = dir.join(n);
        if !c.exists() {
            return c;
        }
    }
    dir.join(format!("{stem}-{}", std::process::id()))
}

/// Default download folder of the user.
pub fn default_download_dir() -> PathBuf {
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    home.join("Downloads")
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "status", content = "detail")]
pub enum Signature {
    /// Valid signature (signer when known).
    Signed(String),
    Unsigned,
    /// Present but invalid or revoked: strongly suspicious.
    Invalid(String),
    /// Not a signable type.
    NotApplicable,
    /// Could not be checked on this OS.
    Unknown,
}

impl Signature {
    pub fn as_db_string(&self) -> String {
        match self {
            Signature::Signed(s) => format!("signed:{s}"),
            Signature::Unsigned => "unsigned".into(),
            Signature::Invalid(r) => format!("invalid:{r}"),
            Signature::NotApplicable => "not_applicable".into(),
            Signature::Unknown => "unknown".into(),
        }
    }
}

/// Verify the digital signature of a downloaded file when it is signable.
pub fn verify_signature(path: &Path) -> Signature {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    #[cfg(target_os = "windows")]
    {
        if ["exe", "msi", "dll", "sys", "msix", "appx", "cab", "ps1", "cat"].contains(&ext.as_str()) {
            return windows_verify(path);
        }
        Signature::NotApplicable
    }
    #[cfg(target_os = "macos")]
    {
        macos_verify(path, &ext)
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let _ = (path, ext);
        Signature::Unknown
    }
}

#[cfg(target_os = "macos")]
fn macos_verify(path: &Path, ext: &str) -> Signature {
    use std::process::Command;
    let run = |cmd: &str, args: &[&str]| -> Option<(bool, String)> {
        let out = Command::new(cmd).args(args).arg(path).output().ok()?;
        let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
        Some((out.status.success(), text))
    };
    match ext {
        "pkg" | "mpkg" => match run("/usr/sbin/pkgutil", &["--check-signature"]) {
            Some((true, t)) if t.contains("signed by a developer certificate") || t.contains("Status: signed") => {
                let signer = t.lines().find(|l| l.contains("1. ")).map(|l| l.trim().trim_start_matches("1. ").to_string()).unwrap_or_default();
                Signature::Signed(signer)
            }
            Some((_, t)) if t.contains("no signature") => Signature::Unsigned,
            Some((_, t)) => Signature::Invalid(t.lines().next().unwrap_or("").to_string()),
            None => Signature::Unknown,
        },
        "dmg" | "app" => match run("/usr/bin/codesign", &["--verify", "--strict", "--verbose=2"]) {
            Some((true, _)) => {
                let signer = Command::new("/usr/bin/codesign")
                    .args(["-dv", "--verbose=2"])
                    .arg(path)
                    .output()
                    .ok()
                    .map(|o| String::from_utf8_lossy(&o.stderr).to_string())
                    .and_then(|t| t.lines().find(|l| l.starts_with("Authority=")).map(|l| l.trim_start_matches("Authority=").to_string()))
                    .unwrap_or_default();
                Signature::Signed(signer)
            }
            Some((false, t)) if t.contains("not signed") => Signature::Unsigned,
            Some((false, t)) => Signature::Invalid(t.lines().next().unwrap_or("").to_string()),
            None => Signature::Unknown,
        },
        _ => Signature::NotApplicable,
    }
}

#[cfg(target_os = "windows")]
fn windows_verify(path: &Path) -> Signature {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::{GUID, PCWSTR};
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Security::WinTrust::*;

    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    let mut file_info = WINTRUST_FILE_INFO {
        cbStruct: std::mem::size_of::<WINTRUST_FILE_INFO>() as u32,
        pcwszFilePath: PCWSTR(wide.as_ptr()),
        ..Default::default()
    };
    let mut data = WINTRUST_DATA {
        cbStruct: std::mem::size_of::<WINTRUST_DATA>() as u32,
        dwUIChoice: WTD_UI_NONE,
        fdwRevocationChecks: WTD_REVOKE_WHOLECHAIN,
        dwUnionChoice: WTD_CHOICE_FILE,
        dwStateAction: WTD_STATEACTION_VERIFY,
        ..Default::default()
    };
    data.Anonymous.pFile = &mut file_info;
    let mut action: GUID = WINTRUST_ACTION_GENERIC_VERIFY_V2;
    let status = unsafe {
        WinVerifyTrust(HWND(std::ptr::null_mut()), &mut action, &mut data as *mut _ as *mut core::ffi::c_void)
    };
    data.dwStateAction = WTD_STATEACTION_CLOSE;
    unsafe {
        WinVerifyTrust(HWND(std::ptr::null_mut()), &mut action, &mut data as *mut _ as *mut core::ffi::c_void);
    }
    const TRUST_E_NOSIGNATURE: i32 = 0x800B0100u32 as i32;
    const TRUST_E_SUBJECT_FORM_UNKNOWN: i32 = 0x800B0003u32 as i32;
    const TRUST_E_PROVIDER_UNKNOWN: i32 = 0x800B0001u32 as i32;
    match status {
        0 => Signature::Signed(String::new()),
        TRUST_E_NOSIGNATURE | TRUST_E_SUBJECT_FORM_UNKNOWN | TRUST_E_PROVIDER_UNKNOWN => Signature::Unsigned,
        other => Signature::Invalid(format!("0x{:08X}", other as u32)),
    }
}

/// Add the OS "downloaded from the Internet" marker if missing.
pub fn mark_downloaded(path: &Path, source_url: &str) -> std::io::Result<()> {
    #[cfg(target_os = "windows")]
    {
        let ads = format!("{}:Zone.Identifier", path.display());
        if std::fs::metadata(&ads).is_err() {
            let referrer = source_url.split(['?', '#']).next().unwrap_or("");
            std::fs::write(ads, format!("[ZoneTransfer]\r\nZoneId=3\r\nHostUrl={referrer}\r\n"))?;
        }
        Ok(())
    }
    #[cfg(target_os = "macos")]
    {
        let _ = source_url;
        let existing = std::process::Command::new("/usr/bin/xattr")
            .args(["-p", "com.apple.quarantine"])
            .arg(path)
            .output()?;
        if !existing.status.success() {
            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
            let value = format!("0081;{now:x};NIGHTFALL;");
            std::process::Command::new("/usr/bin/xattr")
                .args(["-w", "com.apple.quarantine", &value])
                .arg(path)
                .status()?;
        }
        Ok(())
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let _ = (path, source_url);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classification() {
        assert_eq!(classify("setup.EXE"), Danger::Executable);
        assert_eq!(classify("invoice.pdf.js"), Danger::Executable);
        assert_eq!(classify("report.docm"), Danger::Risky);
        assert_eq!(classify("photo.jpg"), Danger::None);
        assert_eq!(classify("README"), Danger::None);
    }

    #[test]
    fn file_names_are_sanitized() {
        assert_eq!(sanitize_file_name("../../etc/passwd"), "_.._etc_passwd");
        assert_eq!(sanitize_file_name("a<b>:c?.txt"), "a_b__c_.txt");
        assert_eq!(sanitize_file_name("CON.txt"), "telechargement_CON.txt");
        assert_eq!(sanitize_file_name(" .hidden "), "hidden");
        assert!(sanitize_file_name(&"x".repeat(300)).len() <= 200);
    }

    #[test]
    fn unique_paths() {
        let dir = tempfile::tempdir().unwrap();
        let a = unique_path(dir.path(), "file.zip");
        std::fs::write(&a, b"1").unwrap();
        let b = unique_path(dir.path(), "file.zip");
        assert_eq!(b.file_name().unwrap(), "file (1).zip");
    }

    #[test]
    fn signature_of_plain_file() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("notes.txt");
        std::fs::write(&p, b"hello").unwrap();
        let s = verify_signature(&p);
        assert!(matches!(s, Signature::NotApplicable | Signature::Unknown));
        mark_downloaded(&p, "https://example.com/notes.txt?token=1").unwrap();
    }
}

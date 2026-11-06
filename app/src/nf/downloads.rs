//! Download manager (UI thread): progress, pause/resume/cancel, history,
//! signature check and "downloaded from the Internet" marking.

use super::client::ClientKind;
use super::state::core;
use super::ui_thread::background;
use super::windows;
use cef::*;
use nf_storage::DownloadRecord;
use serde::Serialize;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

struct Live {
    record: DownloadRecord,
    private: bool,
    speed: i64,
    callback: Option<DownloadItemCallback>,
}

thread_local! {
    static LIVE: RefCell<HashMap<u32, Live>> = RefCell::new(HashMap::new());
}

#[derive(Serialize)]
pub struct DownloadView {
    pub key: String,
    pub url: String,
    pub path: String,
    pub file_name: String,
    pub total_bytes: i64,
    pub received_bytes: i64,
    pub speed: i64,
    pub state: String,
    pub signature: String,
    pub danger: String,
    pub started: i64,
    pub live: bool,
}

fn view(r: &DownloadRecord, speed: i64, live: bool, key: String) -> DownloadView {
    let file_name = std::path::Path::new(&r.path).file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
    DownloadView {
        key,
        url: nf_core::redact::redact_url(&r.url),
        path: r.path.clone(),
        file_name,
        total_bytes: r.total_bytes,
        received_bytes: r.received_bytes,
        speed,
        state: r.state.clone(),
        signature: r.signature.clone(),
        danger: r.danger.clone(),
        started: r.started,
        live,
    }
}

/// All downloads: live ones first, then history from the database.
pub fn list() -> Vec<DownloadView> {
    let mut out = Vec::new();
    let mut live_db_ids = Vec::new();
    LIVE.with(|l| {
        for (id, d) in l.borrow().iter() {
            live_db_ids.push(d.record.id);
            out.push(view(&d.record, d.speed, true, format!("live:{id}")));
        }
    });
    if let Some(s) = core().storage() {
        if let Ok(rows) = s.downloads() {
            for r in rows.iter().filter(|r| r.id == 0 || !live_db_ids.contains(&r.id)) {
                out.push(view(r, 0, false, format!("db:{}", r.id)));
            }
        }
    }
    out
}

pub fn control(key: &str, action: &str) -> bool {
    let Some(id) = key.strip_prefix("live:").and_then(|s| s.parse::<u32>().ok()) else { return false };
    LIVE.with(|l| {
        let l = l.borrow();
        let Some(d) = l.get(&id) else { return false };
        let Some(cb) = &d.callback else { return false };
        match action {
            "pause" => cb.pause(),
            "resume" => cb.resume(),
            "cancel" => cb.cancel(),
            _ => return false,
        }
        true
    })
}

pub fn path_for(key: &str) -> Option<String> {
    if let Some(id) = key.strip_prefix("live:").and_then(|s| s.parse::<u32>().ok()) {
        return LIVE.with(|l| l.borrow().get(&id).map(|d| d.record.path.clone()));
    }
    let id: i64 = key.strip_prefix("db:")?.parse().ok()?;
    core().storage()?.downloads().ok()?.into_iter().find(|r| r.id == id).map(|r| r.path)
}

/// The folder chosen in the settings when it still exists, else the
/// user's Downloads folder.
pub fn download_dir(chosen: Option<&str>) -> std::path::PathBuf {
    match chosen.map(std::path::PathBuf::from) {
        Some(p) if p.is_absolute() && p.is_dir() => p,
        _ => nf_downloads::default_download_dir(),
    }
}

/// Native folder picker for the download folder (UI thread). `done`
/// receives the chosen folder, or None when cancelled.
pub fn choose_folder(browser: &Browser, current: &std::path::Path, done: impl FnOnce(Option<String>) + Send + 'static) {
    let Some(host) = browser.host() else { return };
    let mut cb = FolderPicked::new(Arc::new(std::sync::Mutex::new(Some(Box::new(done) as Box<dyn FnOnce(Option<String>) + Send>))));
    host.run_file_dialog(
        FileDialogMode::OPEN_FOLDER,
        Some(&CefString::from("Dossier de téléchargement")),
        Some(&CefString::from(current.display().to_string().as_str())),
        None,
        Some(&mut cb),
    );
}

/// Native file picker (open or save) for an import or an export run from
/// a settings tab; `accept` are extensions such as ".html".
pub fn choose_file(browser: &Browser, save: bool, title: &str, default_path: &str, accept: &[&str], done: impl FnOnce(Option<String>) + Send + 'static) {
    let Some(host) = browser.host() else { return };
    let mut cb = FolderPicked::new(Arc::new(std::sync::Mutex::new(Some(Box::new(done) as Box<dyn FnOnce(Option<String>) + Send>))));
    let mut filters = CefStringList::new();
    for a in accept {
        filters.append(a);
    }
    host.run_file_dialog(
        if save { FileDialogMode::SAVE } else { FileDialogMode::OPEN },
        Some(&CefString::from(title)),
        Some(&CefString::from(default_path)),
        Some(&mut filters),
        Some(&mut cb),
    );
}

type FolderDone = Arc<std::sync::Mutex<Option<Box<dyn FnOnce(Option<String>) + Send>>>>;

wrap_run_file_dialog_callback! {
    struct FolderPicked {
        done: FolderDone,
    }

    impl RunFileDialogCallback {
        fn on_file_dialog_dismissed(&self, file_paths: Option<&mut CefStringList>) {
            // The list belongs to CEF: take it out of the wrapper to read it.
            let first = file_paths.and_then(|l| std::mem::take(l).into_iter().next()).filter(|p| !p.is_empty());
            if let Some(done) = self.done.lock().ok().and_then(|mut d| d.take()) {
                done(first);
            }
        }
    }
}

fn str_of(s: &CefStringUserfree) -> String {
    CefString::from(s).to_string()
}

wrap_download_handler! {
    pub struct NfDownload {
        kind: Arc<ClientKind>,
    }

    impl DownloadHandler {
        fn on_before_download(
            &self,
            _browser: Option<&mut Browser>,
            download_item: Option<&mut DownloadItem>,
            suggested_name: Option<&CefString>,
            callback: Option<&mut BeforeDownloadCallback>,
        ) -> i32 {
            let (Some(item), Some(callback)) = (download_item, callback) else { return 0 };
            let private = matches!(*self.kind, ClientKind::Tab { private: true, .. });
            let name = suggested_name.map(|s| s.to_string()).unwrap_or_else(|| "telechargement".into());
            let (chosen, ask) = {
                let c = core();
                (c.settings.download_dir.clone(), c.settings.download_ask)
            };
            let dir = download_dir(chosen.as_deref());
            let _ = std::fs::create_dir_all(&dir);
            let path = nf_downloads::unique_path(&dir, &name);
            let danger = nf_downloads::classify(&name);
            let mut record = DownloadRecord {
                id: 0,
                url: str_of(&item.url()),
                path: path.display().to_string(),
                mime: str_of(&item.mime_type()),
                total_bytes: item.total_bytes(),
                received_bytes: 0,
                state: "in_progress".into(),
                started: nf_core::unix_now(),
                finished: None,
                signature: "unknown".into(),
                danger: nf_downloads::danger_message(danger).to_string(),
            };
            if !private {
                if let Some(s) = core().storage() {
                    if let Ok(id) = s.insert_download(&record) {
                        record.id = id;
                    }
                }
            }
            LIVE.with(|l| {
                l.borrow_mut().insert(item.id(), Live { record, private, speed: 0, callback: None });
            });
            // Never opened automatically. Without "ask where to save", the
            // file goes straight to the download folder; otherwise the
            // system "Save as" dialog opens on that path (the final path is
            // read back in on_download_updated).
            callback.cont(Some(&CefString::from(path.display().to_string().as_str())), ask as i32);
            notify();
            1
        }

        fn on_download_updated(
            &self,
            _browser: Option<&mut Browser>,
            download_item: Option<&mut DownloadItem>,
            callback: Option<&mut DownloadItemCallback>,
        ) {
            let Some(item) = download_item else { return };
            let id = item.id();
            let state = if item.is_complete() != 0 {
                "complete"
            } else if item.is_canceled() != 0 {
                "cancelled"
            } else if item.is_interrupted() != 0 {
                "interrupted"
            } else {
                "in_progress"
            };
            let mut finished: Option<(DownloadRecord, bool)> = None;
            LIVE.with(|l| {
                let mut l = l.borrow_mut();
                let Some(d) = l.get_mut(&id) else { return };
                d.record.received_bytes = item.received_bytes();
                d.record.total_bytes = item.total_bytes();
                d.speed = item.current_speed();
                let full = str_of(&item.full_path());
                if !full.is_empty() {
                    d.record.path = full;
                }
                d.record.state = state.into();
                if let Some(cb) = callback {
                    d.callback = Some(cb.clone());
                }
                if state != "in_progress" {
                    d.record.finished = Some(nf_core::unix_now());
                    finished = Some((d.record.clone(), d.private));
                }
            });
            if let Some((record, private)) = finished {
                LIVE.with(|l| l.borrow_mut().remove(&id));
                if !private {
                    if let Some(s) = core().storage() {
                        let _ = s.update_download(&record);
                    }
                }
                if record.state == "complete" {
                    post_process(record, private);
                }
            }
            notify();
        }
    }
}

/// Signature check + OS download marker, off the UI thread.
fn post_process(record: DownloadRecord, private: bool) {
    background(
        move || {
            let path = std::path::PathBuf::from(&record.path);
            let _ = nf_downloads::mark_downloaded(&path, &record.url);
            let sig = nf_downloads::verify_signature(&path);
            let mut r = record;
            r.signature = sig.as_db_string();
            if matches!(sig, nf_downloads::Signature::Invalid(_)) {
                r.danger = "Signature numérique INVALIDE : ce fichier a peut-être été modifié. Ne l'ouvrez pas.".into();
            }
            r
        },
        move |r| {
            if !private {
                if let Some(s) = core().storage() {
                    let _ = s.update_download(&r);
                }
            }
            notify();
        },
    );
}

fn notify() {
    windows::push_event_all(&serde_json::json!({ "type": "downloads" }));
}

/// Reveal a file in the OS file manager (never opens/executes it).
pub fn show_in_folder(path: &str) {
    let p = std::path::Path::new(path);
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("explorer.exe").arg(format!("/select,{}", p.display())).spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("/usr/bin/open").arg("-R").arg(p).spawn();
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        if let Some(dir) = p.parent() {
            let _ = std::process::Command::new("xdg-open").arg(dir).spawn();
        }
    }
}

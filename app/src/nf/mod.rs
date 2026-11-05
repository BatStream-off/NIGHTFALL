//! NIGHTFALL browser process.

pub mod api;
pub mod app;
pub mod assets;
pub mod browsing;
pub mod browsing_data;
pub mod client;
pub mod downloads;
pub mod https_first;
pub mod idle;
pub mod lock_guard;
pub mod omnibox;
#[cfg(target_os = "macos")]
pub mod mac;
pub mod permissions;
pub mod prefs;
pub mod scheme;
pub mod scheme_registration;
pub mod security;
#[cfg(feature = "selftest")]
pub mod selftest;
pub mod session;
pub mod state;
pub mod timers;
pub mod titlebar;
pub mod ui_thread;
pub mod theme;
pub mod updater;
pub mod windows;

use cef::*;
use nf_core::ProfilePaths;
use std::sync::Arc;

#[cfg(target_os = "macos")]
pub type Library = library_loader::LibraryLoader;

#[cfg(not(target_os = "macos"))]
pub struct Library;

/// Load CEF (macOS framework loader) and set the API version.
pub fn load_cef() -> Library {
    #[cfg(target_os = "macos")]
    let library = {
        let loader = library_loader::LibraryLoader::new(&std::env::current_exe().unwrap(), false);
        assert!(loader.load());
        loader
    };
    #[cfg(not(target_os = "macos"))]
    let library = Library;

    let _ = api_hash(sys::CEF_API_VERSION_LAST, 0);

    #[cfg(target_os = "macos")]
    mac::setup_application();

    library
}

fn startup_core(paths: ProfilePaths) -> Result<(), String> {
    paths.ensure().map_err(|e| format!("profil : {e}"))?;
    state::set_log_file(paths.logs().join("nightfall.log"));
    state::log(&format!("démarrage de NIGHTFALL {}", nf_core::VERSION));
    // Internal errors are written to the log (location and message only:
    // NIGHTFALL's panic messages never contain page content or secrets).
    std::panic::set_hook(Box::new(|info| {
        let at = info.location().map(|l| format!("{}:{}", l.file(), l.line())).unwrap_or_default();
        let msg = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_default();
        state::log(&format!("ERREUR INTERNE {at} : {}", nf_core::redact::sanitize(&msg)));
    }));
    let args: Vec<String> = std::env::args().collect();
    let mut signals = security::startup_signals(&args);

    // 1. A purge scheduled before the last exit runs now, before Chromium
    //    opens its files.
    if let Some(cats) = nf_emergency::PurgePlan::pending(&paths) {
        if let Ok(plan) = nf_emergency::PurgePlan::new(&paths, &cats) {
            let report = plan.execute();
            state::log(&format!("purge exécutée : {} fichiers supprimés", report.files_removed));
        }
        nf_emergency::PurgePlan::clear_pending(&paths);
        let _ = paths.ensure();
    }

    // 2. Key store.
    let lock = if nf_auth::NightfallLock::is_configured(&paths) {
        match nf_auth::NightfallLock::load(&paths) {
            Ok(l) => Some(l),
            Err(e) => {
                state::log(&format!("magasin de clés : {e}"));
                signals.push(nf_emergency::Signal::KeystoreTampered);
                None
            }
        }
    } else {
        None
    };

    let prefs: state::UiPrefs = std::fs::read(paths.ui_prefs())
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();

    let shield = nf_shield::Shield::new(paths.shield_cache(), nf_shield::ShieldConfig::default());
    let _ = state::SHIELD.set(Arc::new(shield));

    let last_error = if lock.is_none() && nf_auth::NightfallLock::is_configured(&paths) {
        Some("Le magasin de clés ne peut pas être ouvert sur cet appareil. Utilisez votre Recovery Key ou réinitialisez NIGHTFALL.".to_string())
    } else {
        None
    };

    state::init_core(state::Core {
        paths,
        lock,
        unlocked: None,
        awaiting_recovery_confirmation: false,
        prefs,
        settings: state::Settings::default(),
        videoguard: nf_videoguard::VideoGuardConfig::default(),
        signals,
        last_error,
    });
    Ok(())
}

pub fn run_main(main_args: &MainArgs, cmd_line: &CommandLine, sandbox_info: *mut u8) {
    let switch = CefString::from("type");
    let is_browser_process = cmd_line.has_switch(Some(&switch)) != 1;

    let mut app = app::NfApp::new();
    let ret = execute_process(Some(main_args), Some(&mut app), sandbox_info);
    if !is_browser_process {
        // Renderer, GPU, utility... processes end here.
        let _ = ret;
        return;
    }

    // Refuse to run with switches that enable remote control or disable
    // protections (typical of cookie-stealing malware).
    let args: Vec<String> = std::env::args().collect();
    let forbidden = nf_security::switches::forbidden_in(args.iter().map(String::as_str));
    if !forbidden.is_empty() {
        state::log(&format!("démarrage refusé : options interdites {forbidden:?}"));
        return;
    }

    let Some(root) = ProfilePaths::default_root() else {
        state::log("impossible de déterminer le dossier du profil");
        return;
    };
    let paths = ProfilePaths::new(root);
    if let Err(e) = startup_core(paths.clone()) {
        state::log(&e);
        return;
    }

    let chromium = paths.chromium_profile().display().to_string();
    // Chromium's own log (warnings and errors only) goes next to ours, so a
    // start-up failure can be diagnosed without a debugger.
    let cef_log = paths.logs().join("chromium.log").display().to_string();
    let settings = Settings {
        log_file: CefString::from(cef_log.as_str()),
        // The CI self test records more detail; released builds keep
        // warnings and errors only.
        log_severity: if cfg!(feature = "selftest") { cef::LogSeverity::INFO } else { cef::LogSeverity::WARNING },
        no_sandbox: !cfg!(feature = "sandbox") as _,
        root_cache_path: CefString::from(chromium.as_str()),
        cache_path: CefString::from(chromium.as_str()),
        persist_session_cookies: 0,
        locale: CefString::from("fr"),
        accept_language_list: CefString::from("fr-FR,fr,en-US,en"),
        background_color: 0xFF0A0C10,
        ..Default::default()
    };
    if initialize(Some(main_args), Some(&settings), Some(&mut app), sandbox_info) != 1 {
        state::log("échec de l'initialisation de Chromium (CEF)");
        return;
    }

    #[cfg(target_os = "macos")]
    let _delegate = mac::setup_app_delegate();

    run_message_loop();
    shutdown();
}

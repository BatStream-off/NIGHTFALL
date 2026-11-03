//! CEF application object and browser-process startup hook.

use super::{scheme, scheme_registration, timers, windows};
use cef::*;

wrap_app! {
    pub struct NfApp;

    impl App {
        fn on_before_command_line_processing(&self, process_type: Option<&CefString>, command_line: Option<&mut CommandLine>) {
            let is_browser = process_type.map(|p| p.to_string().is_empty()).unwrap_or(true);
            if !is_browser {
                return;
            }
            if let Some(cl) = command_line {
                // French interface strings for Chromium's own dialogs.
                cl.append_switch_with_value(Some(&CefString::from("lang")), Some(&CefString::from("fr")));
                // CI self test only: record network events for diagnosis.
                #[cfg(feature = "selftest")]
                if let Ok(dir) = std::env::var("NIGHTFALL_PROFILE_DIR") {
                    let path = std::path::Path::new(&dir).join("logs").join("netlog.json");
                    cl.append_switch_with_value(Some(&CefString::from("log-net-log")), Some(&CefString::from(path.display().to_string().as_str())));
                }
            }
        }

        fn on_register_custom_schemes(&self, registrar: Option<&mut SchemeRegistrar>) {
            scheme_registration::register(registrar);
        }

        fn browser_process_handler(&self) -> Option<BrowserProcessHandler> {
            Some(NfBrowserProcess::new())
        }
    }
}

wrap_browser_process_handler! {
    struct NfBrowserProcess;

    impl BrowserProcessHandler {
        fn on_context_initialized(&self) {
            scheme::register_factory();
            let urls: Vec<String> = std::env::args()
                .skip(1)
                .filter(|a| a.starts_with("http://") || a.starts_with("https://"))
                .collect();
            windows::create_window(false, urls);
            timers::start();
            #[cfg(feature = "selftest")]
            super::selftest::start();
        }
    }
}

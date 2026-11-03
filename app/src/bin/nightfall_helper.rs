//! macOS helper executable for Chromium sub-processes (renderer, GPU,
//! utility). It must register the `nightfall://` scheme exactly like the
//! browser process does.

use cef::{args::Args, *};

#[path = "../nf/scheme_registration.rs"]
mod scheme_registration;

wrap_app! {
    struct HelperApp;

    impl App {
        fn on_register_custom_schemes(&self, registrar: Option<&mut SchemeRegistrar>) {
            scheme_registration::register(registrar);
        }
    }
}

fn main() {
    let args = Args::new();

    #[cfg(all(target_os = "macos", feature = "sandbox"))]
    let _sandbox = {
        let mut sandbox = cef::sandbox::Sandbox::new();
        sandbox.initialize(args.as_main_args());
        sandbox
    };

    #[cfg(target_os = "macos")]
    let _loader = {
        let loader = library_loader::LibraryLoader::new(&std::env::current_exe().unwrap(), true);
        assert!(loader.load());
        loader
    };

    let _ = api_hash(sys::CEF_API_VERSION_LAST, 0);
    let mut app = HelperApp::new();
    execute_process(Some(args.as_main_args()), Some(&mut app), std::ptr::null_mut());
}

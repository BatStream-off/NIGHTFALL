//! macOS application object required by CEF (adapted from the cef-rs
//! `cefsimple` example, Apache-2.0 OR MIT).

use cef::application_mac::{CefAppProtocol, CrAppControlProtocol, CrAppProtocol};
use objc2::{
    define_class, extern_methods, msg_send,
    rc::Retained,
    runtime::{AnyObject, Bool, NSObject, NSObjectProtocol, ProtocolObject},
    sel, ClassType, DefinedClass, MainThreadMarker, MainThreadOnly,
};
use objc2_app_kit::{NSApp, NSApplication, NSApplicationDelegate, NSApplicationTerminateReply, NSEvent};
use objc2_foundation::{ns_string, NSBundle, NSObjectNSThreadPerformAdditions};
use std::{cell::Cell, ptr};

define_class! {
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    pub struct NfAppDelegate;

    impl NfAppDelegate {
        #[unsafe(method(createApplication:))]
        unsafe fn create_application(&self, _object: Option<&AnyObject>) {
            let app = NSApp(MainThreadMarker::new().expect("main thread"));
            let main_bundle = NSBundle::mainBundle();
            let _: Bool = msg_send![&main_bundle,
                loadNibNamed: ns_string!("MainMenu"),
                owner: &*app,
                topLevelObjects: ptr::null_mut::<*const AnyObject>()
            ];
        }
    }

    unsafe impl NSObjectProtocol for NfAppDelegate {}

    unsafe impl NSApplicationDelegate for NfAppDelegate {
        #[unsafe(method(applicationShouldTerminate:))]
        unsafe fn application_should_terminate(&self, _sender: &NSApplication) -> NSApplicationTerminateReply {
            NSApplicationTerminateReply::TerminateNow
        }

        #[unsafe(method(applicationShouldHandleReopen:hasVisibleWindows:))]
        unsafe fn application_should_handle_reopen(&self, _sender: &NSApplication, has_visible_windows: Bool) -> Bool {
            if !has_visible_windows.as_bool() && super::windows::window_ids().is_empty() {
                super::windows::create_window(false, Vec::new());
            }
            Bool::NO
        }

        #[unsafe(method(applicationSupportsSecureRestorableState:))]
        unsafe fn application_supports_secure_restorable_state(&self, _sender: &NSApplication) -> Bool {
            Bool::YES
        }
    }
}

impl NfAppDelegate {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = NfAppDelegate::alloc(mtm).set_ivars(());
        unsafe { msg_send![super(this), init] }
    }
}

#[derive(Default)]
pub struct NfApplicationIvars {
    handling_send_event: Cell<Bool>,
}

define_class!(
    #[unsafe(super(NSApplication))]
    #[ivars = NfApplicationIvars]
    pub struct NfApplication;

    impl NfApplication {
        #[unsafe(method(sendEvent:))]
        unsafe fn send_event(&self, event: &NSEvent) {
            let was_sending_event = self.is_handling_send_event();
            if !was_sending_event {
                self.set_handling_send_event(true);
            }
            let _: () = msg_send![super(self), sendEvent:event];
            if !was_sending_event {
                self.set_handling_send_event(false);
            }
        }

        /// Orderly quit: close every NIGHTFALL window (which seals the
        /// Session Vault), then the message loop ends.
        #[unsafe(method(terminate:))]
        unsafe fn terminate(&self, _sender: &AnyObject) {
            for id in super::windows::window_ids() {
                super::windows::close_window(id);
            }
        }
    }

    unsafe impl CrAppControlProtocol for NfApplication {
        #[unsafe(method(setHandlingSendEvent:))]
        unsafe fn _set_handling_send_event(&self, handling_send_event: Bool) {
            self.ivars().handling_send_event.set(handling_send_event);
        }
    }

    unsafe impl CrAppProtocol for NfApplication {
        #[unsafe(method(isHandlingSendEvent))]
        unsafe fn _is_handling_send_event(&self) -> Bool {
            self.ivars().handling_send_event.get()
        }
    }

    unsafe impl CefAppProtocol for NfApplication {}
);

impl NfApplication {
    extern_methods! {
        #[unsafe(method(sharedApplication))]
        fn shared_application() -> Retained<Self>;

        #[unsafe(method(setHandlingSendEvent:))]
        fn set_handling_send_event(&self, handling_send_event: bool);

        #[unsafe(method(isHandlingSendEvent))]
        fn is_handling_send_event(&self) -> bool;
    }
}

pub fn setup_application() {
    let _ = NfApplication::shared_application();
    assert!(NSApp(MainThreadMarker::new().expect("main thread")).isKindOfClass(NfApplication::class()));
}

pub fn setup_app_delegate() -> Retained<NfAppDelegate> {
    let mtm = MainThreadMarker::new().expect("main thread");
    let delegate = NfAppDelegate::new(mtm);
    let proto = ProtocolObject::<dyn NSApplicationDelegate>::from_retained(delegate.clone());
    let app = NSApp(mtm);
    app.setDelegate(Some(&proto));
    unsafe {
        delegate.performSelectorOnMainThread_withObject_waitUntilDone(sel!(createApplication:), None, false);
    }
    delegate
}

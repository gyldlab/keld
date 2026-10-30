//! GH478: the one main-thread-owned, weakly attached `AppKit` Open/Save delegate.
//! Exact filename membership is shared with the existing result-admission rule.
#![allow(unsafe_code)] // GH478 exact methods/NSObject ABI in keld-native/AGENTS.md.
#![deny(unsafe_op_in_unsafe_fn)]

use std::path::Path;

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{NSOpenSavePanelDelegate, NSPopUpButton, NSSavePanel, NSWorkspace};
use objc2_foundation::{NSError, NSObject, NSObjectProtocol, NSString, NSURL};

use super::is_native_directory_selection;
use crate::dialog::file_filter::MacFileFilterModel;

#[derive(Clone, Copy)]
pub(super) enum DelegateMode {
    Open { files: bool, directories: bool },
    Save,
}

pub(super) struct DelegateIvars {
    // Immutable group definitions; the popup supplies the live selected index.
    // Result admission calls the exact same filter predicate after native OK.
    filter: MacFileFilterModel,
    popup: Option<Retained<NSPopUpButton>>,
    mode: DelegateMode,
    treats_packages_as_directories: bool,
}

define_class!(
    // SAFETY: One final, main-thread-only NSObject subclass, with initialized
    // owned Rust ivars. Nothing in the callback outlives its SDK invocation.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "KeldExactFilePanelDelegate"]
    #[ivars = DelegateIvars]
    pub(super) struct ExactFilePanelDelegate;

    // SAFETY: Initialized NSObject subclass; NSObjectProtocol adds no extra
    // callback state or untrusted runtime pointer.
    unsafe impl NSObjectProtocol for ExactFilePanelDelegate {}

    // SAFETY: This exact optional selector and its typed signature are from
    // objc2-app-kit 0.3.2. AppKit calls it on the UI thread with borrowed
    // sender/NSURL values; neither borrowed object escapes the callback.
    unsafe impl NSOpenSavePanelDelegate for ExactFilePanelDelegate {
        #[unsafe(method(panel:shouldEnableURL:))]
        fn should_enable(&self, _sender: &AnyObject, url: &NSURL) -> bool {
            // Directories must remain visible for navigation even when the
            // caller did not receive authority to *select* a directory.
            if let Some(path) = Self::path(url) {
                self.native_directory(&path) || self.accepts(url)
            } else {
                false
            }
        }

        // SAFETY: Pinned objc2-app-kit 0.3.2 exposes the exact selector.
        // The nullable NSError** SDK out parameter is borrowed for one
        // callback, written with one autoreleased error, never adopted,
        // cast, retained across calls, or used outside the main thread.
        #[unsafe(method(panel:validateURL:error:))]
        fn validate_url(
            &self,
            _sender: &AnyObject,
            url: &NSURL,
            out_error: *mut *mut NSError,
        ) -> bool {
            if self.accepts(url) {
                true
            } else {
                if let Some(out_error) = unsafe { out_error.as_mut() } {
                    *out_error = Retained::autorelease_return(NSError::new(
                        16,
                        &NSString::from_str("KELD Native Dialog Filter"),
                    ));
                }
                false
            }
        }
    }
);

impl ExactFilePanelDelegate {
    /// Bind the weak native callback only while the main-thread owner retains
    /// this exact object. No additional authorization is created by the hook.
    pub(super) fn attach_to(&self, panel: &NSSavePanel) {
        // SAFETY: This initialized and main-thread-only NSObject implements
        // the typed delegate protocol; its caller retains it for the sheet.
        unsafe { panel.setDelegate(Some(ProtocolObject::from_ref(self))) };
    }

    /// Release `AppKit`'s weak delegate before the retained Rust owner drops.
    pub(super) fn detach_from(panel: &NSSavePanel) {
        // SAFETY: Clear the typed weak reference on the original main thread,
        // before the referenced Objective-C object can be destroyed.
        unsafe { panel.setDelegate(None) };
    }

    pub(super) fn new(
        main: MainThreadMarker,
        filter: MacFileFilterModel,
        popup: Option<Retained<NSPopUpButton>>,
        mode: DelegateMode,
        treats_packages_as_directories: bool,
    ) -> Option<Retained<Self>> {
        let allocated = Self::alloc(main).set_ivars(DelegateIvars {
            filter,
            popup,
            mode,
            treats_packages_as_directories,
        });
        // SAFETY: Exact NSObject superclass init consumes one allocated owner
        // with fully initialized Rust ivars; a nil return is safely optional.
        unsafe { msg_send![super(allocated), init] }
    }

    fn path(url: &NSURL) -> Option<String> {
        url.isFileURL()
            .then(|| url.path())
            .flatten()
            .map(|p| p.to_string())
    }

    fn native_directory(&self, path: &str) -> bool {
        is_native_directory_selection(
            Path::new(path),
            self.ivars().treats_packages_as_directories,
            &NSWorkspace::sharedWorkspace(),
        )
    }

    fn accepts(&self, url: &NSURL) -> bool {
        let Some(path) = Self::path(url) else {
            return false;
        };
        let (allow_files, allow_directories) = match self.ivars().mode {
            DelegateMode::Open { files, directories } => (files, directories),
            DelegateMode::Save => (true, false),
        };
        let model = &self.ivars().filter;
        let active = if let Some(popup) = &self.ivars().popup {
            usize::try_from(popup.indexOfSelectedItem()).ok()
        } else {
            model.selected_index()
        };
        model.accepts_selected_path_at(
            active,
            Path::new(&path),
            self.native_directory(&path),
            allow_files,
            allow_directories,
        )
    }
}

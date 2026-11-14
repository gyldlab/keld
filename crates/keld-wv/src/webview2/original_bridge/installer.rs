//! One asynchronous document-start registration on its retained creating-STA view.
#![deny(unsafe_op_in_unsafe_fn)]

use std::rc::Rc;
use std::time::Instant;

use tao::event_loop::EventLoopProxy;
use webview2_com::Microsoft::Web::WebView2::Win32::{
    ICoreWebView2, ICoreWebView2AddScriptToExecuteOnDocumentCreatedCompletedHandler,
    ICoreWebView2AddScriptToExecuteOnDocumentCreatedCompletedHandler_Impl,
};
use windows::Win32::Foundation::E_UNEXPECTED;
use windows::core::{HRESULT, PCWSTR, implement};

use super::super::WindowsLoopEvent;
use super::super::original_bootstrap::render_native;
use crate::preload_image::AppPreloadImage;
use crate::renderer_bridge::RendererRoleWitness;
use crate::{WebviewId, WvError};

mod state;
use state::RegistrationState;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::webview2) enum InstallStatus {
    Pending,
    Ready,
    Refused,
}

pub(in crate::webview2) enum InstallStart {
    NotAdmitted,
    Returned(windows::core::Result<()>),
    AlreadyStarted,
    OutcomeUnknown,
}

/// Attach this owner to the complete native View before `begin_on_ui`. The raw
/// callback retains only its state, original locator and notification proxy,
/// never a COM object or View.
pub(in crate::webview2) struct PreloadInstaller {
    view: ICoreWebView2,
    role: RendererRoleWitness,
    state: Rc<RegistrationState>,
    handler: ICoreWebView2AddScriptToExecuteOnDocumentCreatedCompletedHandler,
    proxy: EventLoopProxy<WindowsLoopEvent>,
    _id: WebviewId,
}

impl PreloadInstaller {
    pub(in crate::webview2) fn prepare(
        view: ICoreWebView2,
        role: RendererRoleWitness,
        image: &AppPreloadImage,
        deadline: Instant,
        proxy: EventLoopProxy<WindowsLoopEvent>,
        id: WebviewId,
    ) -> Result<Self, WvError> {
        let state = Rc::new(RegistrationState::new(render_native(image)?, deadline));
        let handler = completion_handler(Rc::clone(&state), proxy.clone(), id);
        Ok(Self {
            view,
            role,
            state,
            handler,
            proxy,
            _id: id,
        })
    }

    /// One exact role fence followed immediately by the retained native call.
    /// No native getter, allocation, pump, wait, or Rust borrow crosses that cut.
    #[allow(unsafe_code)] // Exact reviewed AddScript call with retained counted UTF16.
    #[expect(
        clippy::single_match_else,
        reason = "Preserve the caught panic payload scope through the actual-return or unproved-state transition."
    )]
    pub(in crate::webview2) fn begin_on_ui(&self) -> InstallStart {
        if let Some(previous) = self.state.previous_start() {
            return previous;
        }
        let Some(pointer) = self.state.input_pointer() else {
            self.state.refuse();
            return InstallStart::NotAdmitted;
        };
        let input = PCWSTR(pointer);
        let mut enter = || self.state.begin(Instant::now());
        let admitted = self.role.try_admit_native(&mut enter);
        if let Err(result) = self.state.finish_admission(admitted) {
            return result;
        }
        // SAFETY: this exact retained view and handler stay on the creating STA.
        // Counted immutable UTF16 includes its NUL and is retained by state.
        // The callback cannot release it while Entry::InCall, even on inline
        // completion or same-controller Close. The exact Core fence just
        // marked entry and released all guards; no other work intervenes.
        // https://learn.microsoft.com/microsoft-edge/webview2/reference/win32/icorewebview2#addscripttoexecuteondocumentcreated
        let returned = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
            self.view
                .AddScriptToExecuteOnDocumentCreated(input, &self.handler)
        }));
        let result = match returned {
            Ok(result) => {
                self.state.native_return(result.is_ok(), Instant::now());
                InstallStart::Returned(result)
            }
            Err(_) => {
                self.state.native_unwound();
                InstallStart::OutcomeUnknown
            }
        };
        if self
            .proxy
            .send_event(WindowsLoopEvent::OriginalWake)
            .is_err()
        {
            self.state.refuse();
        }
        result
    }

    /// Ready means both the actual native return and the real completion
    /// callback arrived within the original build deadline. It mints no lease.
    pub(in crate::webview2) fn status(&self, now: Instant) -> InstallStatus {
        self.state.status(now)
    }

    pub(in crate::webview2) fn retire_on_ui(&self) {
        self.state.retire();
    }

    /// Caller must have observed successful Close from this View's shared
    /// `AdoptedControllerClose`; attempted Close or logical retirement is not enough.
    pub(in crate::webview2) fn controller_closed_on_ui(&self) {
        self.state.controller_closed();
    }

    pub(in crate::webview2) fn native_quiesced(&self) -> bool {
        self.state.native_quiesced()
    }
}

// Use the pinned raw COM interface: webview2-com's convenience completion
// wrapper first copies the unbounded script ID into a String. This handler
// does not read or retain that ID; the fixed script lives until controller Close.
#[allow(unsafe_code)] // Exact pinned COM completion vtable, scoped to this constructor.
#[expect(
    clippy::ref_as_ptr,
    clippy::inline_always,
    reason = "These operations are emitted by pinned windows-implement 0.60.2; keep its reviewed COM ABI unchanged."
)]
fn completion_handler(
    state: Rc<RegistrationState>,
    proxy: EventLoopProxy<WindowsLoopEvent>,
    id: WebviewId,
) -> ICoreWebView2AddScriptToExecuteOnDocumentCreatedCompletedHandler {
    #[implement(ICoreWebView2AddScriptToExecuteOnDocumentCreatedCompletedHandler)]
    struct PreloadCompleted {
        state: Rc<RegistrationState>,
        proxy: EventLoopProxy<WindowsLoopEvent>,
        _id: WebviewId,
    }

    impl ICoreWebView2AddScriptToExecuteOnDocumentCreatedCompletedHandler_Impl
        for PreloadCompleted_Impl
    {
        fn Invoke(&self, errorcode: HRESULT, result: &PCWSTR) -> windows::core::Result<()> {
            let handled = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                // No dereference, scan, string conversion, or retained native pointer.
                self.state
                    .complete(errorcode.is_ok() && !result.is_null(), Instant::now());
                if self
                    .proxy
                    .send_event(WindowsLoopEvent::OriginalWake)
                    .is_err()
                {
                    self.state.refuse();
                }
            }));
            if handled.is_err() {
                self.state.refuse();
                return Err(E_UNEXPECTED.into());
            }
            Ok(())
        }
    }
    PreloadCompleted {
        state,
        proxy,
        _id: id,
    }
    .into()
}

#[cfg(test)]
mod tests;

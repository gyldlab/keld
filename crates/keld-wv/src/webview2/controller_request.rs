//! The existing controller request retained before native entry by `NativeBuild`.
//! Actual callback status/interface precede interpretation; one original clock.

use std::rc::{Rc, Weak};
use std::time::Instant;

use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2CreateCoreWebView2ControllerCompletedHandler;
use windows::Win32::Foundation::HWND;

use super::controller_custody::AdoptedControllerClose;
use super::{
    ControllerCreationError, CreateCoreWebView2ControllerCompletedHandler,
    ICoreWebView2Environment, ProfileReleaseFault, ProfileReleaseWait, create_controller_native,
    wait_for_launch_with_message_pump_until,
};

mod state;
use state::{RequestCore, Retirement};

/// Stored in the same Window candidate before begin. Neither a callback nor a
/// timeout transfers this owner out of the builder or releases its parent HWND.
pub(super) struct ControllerRequest {
    core: RequestCore<Rc<AdoptedControllerClose>>,
    callback: ICoreWebView2CreateCoreWebView2ControllerCompletedHandler,
    release: Option<Weak<ProfileReleaseWait>>,
}

impl ControllerRequest {
    pub(super) fn new(release: Option<Weak<ProfileReleaseWait>>, deadline: Instant) -> Self {
        let (core, completion) = RequestCore::new(deadline);
        let callback_release = release.clone();
        // The pinned webview2-com completed handler invokes its FnOnce closure
        // once. Its native argument conversion owns the optional interface.
        let callback = CreateCoreWebView2ControllerCompletedHandler::create(Box::new(
            move |status, controller| {
                let controller = controller.map(|controller| {
                    Rc::new(AdoptedControllerClose::new(controller, callback_release))
                });
                completion.complete(status, controller, |retained| {
                    // The actual interface/status are already stored in the SAME
                    // request before any observer can panic or before a wake.
                    #[cfg(all(feature = "media-acceptance", test))]
                    {
                        super::media_acceptance::observe_com_phase("controller-callback-enter");
                        super::media_acceptance::observe_controller_completion(
                            retained.status(),
                            retained.controller().map(|owner| owner.controller()),
                        );
                    }
                    #[cfg(not(all(feature = "media-acceptance", test)))]
                    let _ = retained;
                })
            },
        ));
        Self {
            core,
            callback,
            release,
        }
    }

    pub(super) fn begin(
        &mut self,
        environment: &ICoreWebView2Environment,
        hwnd: HWND,
    ) -> Result<(), ControllerCreationError> {
        let result = self.core.begin_with(
            || {
                #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
                super::t2_test_observation::controller_requested();
                #[cfg(all(feature = "media-acceptance", test))]
                super::media_acceptance::observe_com_phase("controller-call-enter");
                // The shared helper contains the sole physical Create call.
                create_controller_native(environment, hwnd, &self.callback)
            },
            || {
                #[cfg(all(feature = "media-acceptance", test))]
                super::media_acceptance::observe_com_phase("controller-call-returned");
            },
        );
        if result.is_err() && self.core.entered_native() {
            self.latch_unproved();
        }
        result
    }

    pub(super) fn wait(&mut self) -> Result<(), ControllerCreationError> {
        let result = self.core.wait_with(wait_for_launch_with_message_pump_until);
        if result.is_err() && self.core.unproved_wait() {
            self.latch_unproved();
        }
        result
    }

    pub(super) fn completed_controller(&self) -> Option<Rc<AdoptedControllerClose>> {
        self.core.completed_controller().cloned()
    }

    /// No pump, cancellation API or deadline reset. Only the actual callback and
    /// the same one-shot Close can finish an entered request's physical custody.
    pub(super) fn retire_on_ui(&self) -> bool {
        // Copy the same Rc out before COM; no record/RefCell guard crosses Close.
        let retirement = match self.core.retirement() {
            Retirement::NeverEntered => return true,
            Retirement::Unproved => {
                self.latch_unproved();
                return false;
            }
            Retirement::Resolved(controller) => controller.cloned(),
        };
        let Some(controller) = retirement else {
            return true;
        };
        // A controller still in the request never reached the builder's browser
        // identity/binding step. Preserve the existing unclaimed-controller fault.
        if let Some(release) = self.release.as_ref().and_then(Weak::upgrade) {
            let _ = release.latch_unidentified_controller();
        }
        controller.close_on_ui()
    }

    fn latch_unproved(&self) {
        if let Some(release) = self.release.as_ref().and_then(Weak::upgrade) {
            release.latch_fault(ProfileReleaseFault::RebindUnproven);
        }
    }
}

#[cfg(test)]
mod tests;

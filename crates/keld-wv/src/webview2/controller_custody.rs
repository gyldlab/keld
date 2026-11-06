//! Unclaimed controller custody across completion delivery and launch failure.
//!
//! A successful COM completion does not transfer ownership until the builder
//! adopts it. An abandoned channel value closes on this UI thread. Managed
//! requests report into the existing release owner's first-fault cell only.
#![deny(unsafe_op_in_unsafe_fn)]

use std::rc::Weak;
use std::sync::mpsc::Receiver;

use windows::Win32::Foundation::E_POINTER;

use super::{
    ICoreWebView2Controller, ProfileReleaseFault, ProfileReleaseWait, profile_failure_with,
};
use crate::WvError;
use crate::profile::ProfileErrorKind;

/// The only builder-facing transfer point. Callback/channel owners cannot expose
/// the raw controller before adopting this custody.
pub(super) struct PendingController(Unclaimed<NativeController>);

impl PendingController {
    pub(super) fn adopt(self) -> Result<ICoreWebView2Controller, WvError> {
        self.0.take().map(|owned| owned.controller).ok_or_else(|| {
            profile_failure_with(
                ProfileErrorKind::LifecycleUnproven,
                "a controller completion was adopted without its native custodian",
            )
        })
    }
}

struct NativeController {
    controller: ICoreWebView2Controller,
    release: Option<Weak<ProfileReleaseWait>>,
}

impl NativeController {
    fn retire(&self) {
        // A weak reference neither keeps the loop alive nor interferes with
        // checked sole-owner retirement. An unresolved request was already
        // faulted synchronously if its callback outlives that owner.
        if let Some(release) = self.release.as_ref().and_then(Weak::upgrade) {
            let _ = release.latch_unidentified_controller();
        }
        // SAFETY: the completion and receiver are used only on the controller's
        // creating UI/STA thread. This guard alone owns this unadopted interface.
        // Close runs before the COM reference is released and never a second time.
        // https://learn.microsoft.com/microsoft-edge/webview2/reference/win32/icorewebview2controller#close
        let result = unsafe { self.controller.Close() };
        #[cfg(all(feature = "media-acceptance", test))]
        super::media_acceptance::observe_controller_close_result(&result);
        let _ = result;
    }
}

/// Both adopted-resource custodians use this one Close call before HWND drop.
/// Legacy views pass None and retain their existing Close-result behavior.
pub(super) fn close_adopted(
    controller: &ICoreWebView2Controller,
    release: Option<&Weak<ProfileReleaseWait>>,
) {
    #[cfg(all(feature = "media-acceptance", test))]
    let observed =
        super::media_acceptance::adopted_close_control::before(controller, release.is_some());
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    super::t2_test_observation::native_release_step("adopted-close.enter", None);
    // SAFETY: callers own the adopted controller on its creating UI/STA thread.
    // This runs exactly once from View or standalone failed-build custody.
    // https://learn.microsoft.com/microsoft-edge/webview2/reference/win32/icorewebview2controller#close
    let result = unsafe { controller.Close() };
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    super::t2_test_observation::native_release_step(
        "adopted-close.return",
        Some(result.as_ref().err().map_or(0, |error| error.code().0)),
    );
    #[cfg(all(feature = "media-acceptance", test))]
    let result = super::media_acceptance::adopted_close_control::after(result, observed);
    record_adopted_close_result(result, release);
}

/// A failed Close invalidates even previously credited normal exit. It neither
/// retries Close nor replaces an earlier fault in the existing release owner.
pub(super) fn record_adopted_close_result(
    result: windows::core::Result<()>,
    release: Option<&Weak<ProfileReleaseWait>>,
) {
    if let Err(error) = result
        && let Some(release) = release
    {
        if let Some(release) = release.upgrade() {
            release.latch_fault(ProfileReleaseFault::ControllerCloseFailed);
        }
        eprintln!("KELD_WV_CONTROLLER_CLOSE_FAILED {error}");
    }
}

pub(super) fn completion(
    status: windows::core::Result<()>,
    controller: Option<ICoreWebView2Controller>,
    release: Option<Weak<ProfileReleaseWait>>,
) -> windows::core::Result<PendingController> {
    #[cfg(all(feature = "media-acceptance", test))]
    super::media_acceptance::observe_controller_completion(&status, controller.as_ref());
    wrap_completion(
        status,
        controller.map(|controller| NativeController {
            controller,
            release,
        }),
        NativeController::retire,
        || windows::core::Error::from(E_POINTER),
    )
    .map(PendingController)
}

/// Drop queued completions before recording an unresolved-request fallback.
/// A queued controller then latches its more specific unidentifiable-controller
/// fault first. The original pump/deadline error is returned unchanged.
pub(super) fn finish_wait<T>(
    result: Result<T, WvError>,
    receiver: Receiver<T>,
    unresolved: impl FnOnce(),
) -> Result<T, WvError> {
    match result {
        Ok(value) => Ok(value),
        Err(error) => {
            drop(receiver);
            unresolved();
            Err(error)
        }
    }
}

/// The generic custody is a pure-test seam, not a second lifecycle owner.
struct Unclaimed<T> {
    value: Option<T>,
    retire: fn(&T),
}

impl<T> Unclaimed<T> {
    fn new(value: T, retire: fn(&T)) -> Self {
        Self {
            value: Some(value),
            retire,
        }
    }

    fn take(mut self) -> Option<T> {
        self.value.take()
    }
}

impl<T> Drop for Unclaimed<T> {
    fn drop(&mut self) {
        if let Some(value) = self.value.take() {
            (self.retire)(&value);
        }
    }
}

fn wrap_completion<T, E>(
    status: Result<(), E>,
    controller: Option<T>,
    retire: fn(&T),
    missing: impl FnOnce() -> E,
) -> Result<Unclaimed<T>, E> {
    // Wrap BEFORE combining with status: an error HRESULT can still carry an
    // interface, which must close instead of receiving only an IUnknown release.
    let controller = controller.map(|value| Unclaimed::new(value, retire));
    status.and(controller.ok_or_else(missing))
}

#[cfg(test)]
#[path = "tests/controller_custody.rs"]
mod tests;

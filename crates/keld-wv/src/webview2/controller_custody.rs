//! Unclaimed controller custody across completion delivery and launch failure.
//!
//! A successful COM completion does not transfer ownership until the builder
//! adopts it. An abandoned channel value closes on this UI thread. Managed
//! requests report into the existing release owner's first-fault cell only.
#![deny(unsafe_op_in_unsafe_fn)]

use std::rc::Weak;
use std::sync::mpsc::Receiver;

use windows::Win32::Foundation::E_POINTER;

use super::{ICoreWebView2Controller, ProfileReleaseWait, profile_failure_with};
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
        let _ = unsafe { self.controller.Close() };
    }
}

pub(super) fn completion(
    status: windows::core::Result<()>,
    controller: Option<ICoreWebView2Controller>,
    release: Option<Weak<ProfileReleaseWait>>,
) -> windows::core::Result<PendingController> {
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

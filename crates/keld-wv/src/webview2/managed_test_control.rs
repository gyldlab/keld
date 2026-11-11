//! Approved GH1188 U7/U18 child-only native fault and observation controls.
//!
//! Construct once on the UI thread. The Host launch must scrub these names from
//! Bun. Both the module declaration and every call site are debug/test-feature
//! gated. Failures, bounded holds and receipt withholding expose failure paths;
//! they grant no success credit and do not change product admission or release policy.
#![cfg(all(feature = "profile-test-hooks", debug_assertions))]
#![deny(unsafe_op_in_unsafe_fn)]

use std::time::Instant;

use tao::event_loop::EventLoopProxy;
use tao::window::WindowId;

use super::WindowsLoopEvent;
use crate::WvError;

mod failure;
mod native_hold;
mod page_readback;

pub(super) use failure::{BuildAttempt, BuildPoint};
pub(super) use native_hold::observe_pump_message;
pub(super) use page_readback::PageReadback;

/// One next-build failure: `hwnd|webview|view[:swallow|defer]`.
pub const TEST_WINDOWS_FAIL_BUILD_ENV: &str = "KELD_TEST_WINDOWS_FAIL_BUILD";
/// Native-order hold: `destroyed:<dir>` or `build-pump:<dir>`.
pub const TEST_WINDOWS_NATIVE_HOLD_ENV: &str = "KELD_TEST_WINDOWS_NATIVE_HOLD";
/// Fixed page observation interval in positive whole milliseconds.
pub const TEST_WINDOWS_PAGE_READBACK_ENV: &str = "KELD_TEST_WINDOWS_PAGE_READBACK";

pub(super) struct TestControl {
    failure: failure::FailureControl<WindowId>,
    native_hold: Option<native_hold::NativeHold>,
    withhold_later_generation_receipt: bool,
    deferred_destroyed_proxy: EventLoopProxy<WindowsLoopEvent>,
}

impl TestControl {
    /// The proxy belongs to this hook, not the browser wait or a dying HWND.
    pub(super) fn from_env(
        deferred_destroyed_proxy: EventLoopProxy<WindowsLoopEvent>,
        all_generations: bool,
    ) -> Result<Self, WvError> {
        let withhold_later_generation_receipt =
            later_generation_withholding_requested(all_generations)?;
        let failure = read_env(TEST_WINDOWS_FAIL_BUILD_ENV)?
            .as_deref()
            .map(failure::FailureSpec::parse)
            .transpose()?;
        let native_hold = read_env(TEST_WINDOWS_NATIVE_HOLD_ENV)?
            .as_deref()
            .map(native_hold::NativeHold::parse)
            .transpose()?;
        Ok(Self {
            failure: failure::FailureControl::new(failure),
            native_hold,
            withhold_later_generation_receipt,
            deferred_destroyed_proxy,
        })
    }

    /// Apply the captured receipt test control to the existing release owner.
    pub(super) const fn withholds_later_generation_receipts(&self) -> bool {
        self.withhold_later_generation_receipt
    }

    /// Claim the next-build fault at build entry, even if an earlier real step
    /// fails before the chosen injection point. It can never migrate to a retry.
    pub(super) fn begin_build(&mut self, build: u64) -> BuildAttempt {
        self.failure.begin_build(build)
    }

    /// Invoke before controller creation, in place of `CoreWebView2`, or at the
    /// first fallible post-`View` step, respectively. The caller retains native
    /// custodians and runs the normal D12 failure path after this returned error.
    pub(super) fn fail_at(
        &mut self,
        attempt: &mut BuildAttempt,
        point: BuildPoint,
        key: WindowId,
        hwnd: isize,
    ) -> Result<(), WvError> {
        self.failure.fail_at(attempt, point, key, hwnd)
    }

    /// U7 stage/answer records follow the actually injected build even after
    /// its held native event was consumed; this never enables general D16 logs.
    pub(super) fn observes_failed_build(&self, build: u64) -> bool {
        self.failure.observes_failed_build(build)
    }

    /// False intercepts only this hook's failed HWND. True reaches the common
    /// real-`Destroyed` staging function, which emits its actual stage marker.
    pub(super) fn dispatch_destroyed(&mut self, key: WindowId) -> bool {
        self.failure.dispatch_destroyed(key)
    }

    /// After publishing prior-turn entries at the next callback's top, pass this
    /// key through the real staging function before expiry. Its publication
    /// waits for a subsequent callback, just like a normally delivered event.
    pub(super) fn take_deferred_destroyed(&mut self) -> Option<WindowId> {
        self.failure.take_deferred_destroyed()
    }

    /// Call only at callback end, AFTER that turn's exit check. One failed post
    /// is an owner error; there is no repeated post or synthetic completion.
    pub(super) fn post_deferred_destroyed(
        &mut self,
        terminal_has_run: bool,
        barrier_ready_or_faulted: bool,
    ) -> Result<(), WvError> {
        let proxy = &self.deferred_destroyed_proxy;
        self.failure
            .post_deferred_destroyed(terminal_has_run, barrier_ready_or_faulted, || {
                proxy
                    .send_event(WindowsLoopEvent::ProfileReleaseWake)
                    .is_ok()
            })
    }

    /// Last in an actual `Destroyed` arm after staging. Deliberately no pumping:
    /// the real `WM_DESTROY` stack remains live until the release file arrives.
    pub(super) fn hold_destroyed(&self, hwnd: isize) -> Result<(), WvError> {
        if let Some(hold) = &self.native_hold {
            hold.destroyed(hwnd)?;
        }
        Ok(())
    }

    /// Optional native first-show stimulus, sampled before creating the HWND.
    pub(super) fn initially_focused(&self, build: u64) -> Result<bool, WvError> {
        self.native_hold
            .as_ref()
            .map_or(Ok(true), |hold| hold.initially_focused(build))
    }

    /// Called after the actual controller request returns, outside its callback,
    /// using the ORIGINAL build deadline. The same native pump drains the queue.
    pub(super) fn hold_build_pump(&self, build: u64, deadline: Instant) -> Result<(), WvError> {
        if let Some(hold) = &self.native_hold {
            hold.build_pump(build, deadline)?;
        }
        Ok(())
    }
}

fn read_env(name: &'static str) -> Result<Option<String>, WvError> {
    std::env::var_os(name)
        .map(|value| {
            value
                .into_string()
                .map_err(|_| invalid_hook(name, "expected a Unicode value"))
        })
        .transpose()
}

fn invalid_hook(name: &'static str, reason: &'static str) -> WvError {
    WvError::Webview(format!("invalid {name}: {reason}"))
}

/// Withhold normal receipts from browser generations >= 2 (GH1188 D16).
pub const TEST_WINDOWS_WITHHOLD_LATER_GENERATION_RECEIPT_ENV: &str =
    "KELD_TEST_WINDOWS_WITHHOLD_LATER_GENERATION_RECEIPT";

/// Captured once while constructing the engine, alongside the existing h9 flag.
pub(super) fn later_generation_withholding_requested(
    all_generations: bool,
) -> Result<bool, WvError> {
    let later = std::env::var_os(TEST_WINDOWS_WITHHOLD_LATER_GENERATION_RECEIPT_ENV).as_deref()
        == Some(std::ffi::OsStr::new("1"));
    checked_receipt_controls(later, all_generations)
}

fn checked_receipt_controls(later: bool, all_generations: bool) -> Result<bool, WvError> {
    if later && all_generations {
        return Err(WvError::Webview(format!(
            "invalid {}: cannot combine with {}",
            TEST_WINDOWS_WITHHOLD_LATER_GENERATION_RECEIPT_ENV,
            super::TEST_WINDOWS_BARRIER_GRACE_EXPIRY_ENV,
        )));
    }
    Ok(later)
}

#[cfg(test)]
#[path = "managed_test_control/tests/receipt_controls.rs"]
mod receipt_control_tests;

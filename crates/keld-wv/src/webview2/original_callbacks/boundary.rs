//! Whole adapter callback containment and the existing navigation hold.
#![forbid(unsafe_code)]

use super::super::original_navigation::OriginalNavigation;
use super::super::original_resource::ResourceSession;
use super::{FailureObservation, Observation};
use crate::WebviewId;
use crate::engine::OriginalNavigationFact;
use crate::renderer_contract::NativeAttemptObservation;
use std::cell::RefCell;
use std::rc::{Rc, Weak};
use windows::Win32::Foundation::E_UNEXPECTED;

#[derive(Clone)]
pub(in crate::webview2) struct FailureSink {
    session: Weak<ResourceSession>,
    view: WebviewId,
}

impl FailureSink {
    pub(in crate::webview2) fn new(session: &Rc<ResourceSession>, view: WebviewId) -> Self {
        Self {
            session: Rc::downgrade(session),
            view,
        }
    }

    pub(in crate::webview2) fn latch(&self, detail: &'static str) {
        // Root retains this singleton beyond every View and callback. The same
        // permanent fault is stored before the existing OriginalWake is posted.
        if let Some(session) = self.session.upgrade() {
            session.latch_failure(detail, self.view);
        }
    }
}

/// This includes native argument disposal and the hold's own destruction. The
/// failure sink performs no native operation and owns no separate callback count.
#[expect(
    clippy::single_match_else,
    reason = "Preserve the explicit native-return and unwind arms with their original payload drop scopes"
)]
pub(in crate::webview2) fn boundary(
    failed: &dyn Fn(&'static str),
    run: impl FnOnce() -> windows::core::Result<()>,
) -> windows::core::Result<()> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(run)) {
        Ok(result) => result,
        Err(_) => {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                failed("navigation callback boundary panicked");
            }));
            Err(E_UNEXPECTED.into())
        }
    }
}

pub(super) struct CallbackTurn<'a> {
    state: Rc<RefCell<OriginalNavigation>>,
    failed: &'a dyn Fn(&'static str),
}

impl<'a> CallbackTurn<'a> {
    pub(super) fn enter(
        state: &Rc<RefCell<OriginalNavigation>>,
        failed: &'a dyn Fn(&'static str),
    ) -> windows::core::Result<Self> {
        let Ok(mut navigation) = state.try_borrow_mut() else {
            failed("navigation callback entry met a held document-owner borrow");
            return Err(E_UNEXPECTED.into());
        };
        navigation.enter_navigation_callback();
        drop(navigation);
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        super::super::managed_test_control::navigation_post::navigation(
            state,
            "callback-enter",
            None,
            None,
        );
        Ok(Self {
            state: Rc::clone(state),
            failed,
        })
    }
}

impl Drop for CallbackTurn<'_> {
    fn drop(&mut self) {
        if let Ok(mut navigation) = self.state.try_borrow_mut() {
            navigation.leave_navigation_callback();
        } else {
            // The original count stays nonzero. A refused decrement cannot
            // become quiescence, even after the same controller has closed.
            (self.failed)("navigation callback exit met a held document-owner borrow");
        }
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        super::super::managed_test_control::navigation_post::navigation(
            &self.state,
            "callback-leave",
            None,
            None,
        );
    }
}

/// Caller is already inside `boundary()`. The consumed closure owns all native
/// arguments, so their Release and exact failure reporting precede this hold.
pub(super) fn callback(
    state: &Rc<RefCell<OriginalNavigation>>,
    failed: &dyn Fn(&'static str),
    run: impl FnOnce() -> windows::core::Result<()>,
) -> windows::core::Result<()> {
    let _turn = CallbackTurn::enter(state, failed)?;
    run()
}

pub(super) fn capture_failure(
    state: &Rc<RefCell<OriginalNavigation>>,
) -> windows::core::Result<FailureObservation> {
    let state = state.try_borrow().map_err(|_| E_UNEXPECTED)?;
    Ok(FailureObservation {
        context: state.host_context(),
        attempt: state.active_attempt(),
        previous: state.current_document().map(|lease| lease.stamp()),
    })
}

pub(super) fn fail_observed(
    state: &Rc<RefCell<OriginalNavigation>>,
    observe: &Observation,
    failure: FailureObservation,
    failed: &dyn Fn(&'static str),
) -> windows::core::Result<()> {
    let Ok(mut navigation) = state.try_borrow_mut() else {
        failed("navigation failure settlement met a held document-owner borrow");
        return Err(E_UNEXPECTED.into());
    };
    let previous = failure.previous;
    let fact = if let Some(attempt) = failure.attempt {
        navigation.fail_attempt(attempt);
        OriginalNavigationFact::Failed {
            attempt: NativeAttemptObservation(attempt),
            revoked: previous,
        }
    } else {
        OriginalNavigationFact::Revoked { previous }
    };
    drop(navigation);
    boundary(failed, || {
        observe(failure.context, fact);
        Ok(())
    })
}

#[cfg(test)]
mod tests;

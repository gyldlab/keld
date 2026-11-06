//! A browser failure can synchronously focus a Tao window from inside `WebView2`'s
//! own `CloseInternal`. Return to the caller of the outer native dispatch before
//! the existing stop owner closes any controller. No message or elapsed time is
//! proof that the native call returned.
#![forbid(unsafe_code)]

use std::cell::Cell;

use tao::event::Event;
use tao::event_loop::{ControlFlow, EventLoop, EventLoopWindowTarget};
use tao::platform::run_return::EventLoopExtRunReturn as _;

use super::{ProfileReleaseFault, WindowsLoopEvent};

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Phase {
    #[default]
    Outside,
    Dispatching,
    Returning,
    Returned,
}

#[derive(Default)]
pub(super) struct AbnormalStopBoundary(Cell<Phase>);

impl AbnormalStopBoundary {
    /// The barrier fault itself remains first-wins in `ProfileReleaseWait`.
    pub(super) fn defer(&self, fault: Option<ProfileReleaseFault>) -> bool {
        if fault != Some(ProfileReleaseFault::AbnormalExit) {
            return false;
        }
        match self.0.get() {
            Phase::Dispatching | Phase::Returning => {
                self.0.set(Phase::Returning);
                true
            }
            Phase::Outside | Phase::Returned => false,
        }
    }

    pub(super) fn returning(&self) -> bool {
        self.0.get() == Phase::Returning
    }

    /// Tao 0.35.3 calls `runner.poll` (`Init`) before `GetMessage` on entry. Its
    /// `run_return` does not return until the previous outer `DispatchMessage`
    /// has unwound and its handler is removed. Borrow the same handler across
    /// both calls so all resource, FIFO and retirement custody survives.
    pub(super) fn run(
        &self,
        event_loop: &mut EventLoop<WindowsLoopEvent>,
        mut dispatch: impl FnMut(
            Event<'_, WindowsLoopEvent>,
            &EventLoopWindowTarget<WindowsLoopEvent>,
            &mut ControlFlow,
        ),
    ) -> i32 {
        self.run_passes(|| event_loop.run_return(&mut dispatch))
    }

    fn run_passes(&self, mut run: impl FnMut() -> i32) -> i32 {
        self.0.set(Phase::Dispatching);
        let first = run();
        let result = if self.returning() {
            self.0.set(Phase::Returned);
            let final_code = run();
            if first == 0 { final_code } else { first }
        } else {
            first
        };
        self.0.set(Phase::Outside);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::{AbnormalStopBoundary, ProfileReleaseFault};

    #[test]
    fn abnormal_stop_is_admitted_only_after_the_first_dispatch_pass_returns() {
        let boundary = AbnormalStopBoundary::default();
        let mut passes = 0;
        let result = boundary.run_passes(|| {
            passes += 1;
            if passes == 1 {
                assert!(boundary.defer(Some(ProfileReleaseFault::AbnormalExit)));
                assert!(boundary.defer(Some(ProfileReleaseFault::AbnormalExit)));
                assert!(boundary.returning());
                0
            } else {
                assert!(!boundary.defer(Some(ProfileReleaseFault::AbnormalExit)));
                assert!(!boundary.returning());
                7
            }
        });
        assert_eq!((passes, result), (2, 7));
    }

    #[test]
    fn other_faults_do_not_restart_the_native_loop() {
        let boundary = AbnormalStopBoundary::default();
        let mut passes = 0;
        let result = boundary.run_passes(|| {
            passes += 1;
            assert!(!boundary.defer(Some(ProfileReleaseFault::ReleasingExpired)));
            3
        });
        assert_eq!((passes, result), (1, 3));
    }
}

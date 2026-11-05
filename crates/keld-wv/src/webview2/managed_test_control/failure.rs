//! U7's single consumed build fault and matching native-event interception.
#![forbid(unsafe_code)]

use super::{TEST_WINDOWS_FAIL_BUILD_ENV, invalid_hook};
use crate::WvError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::webview2) enum BuildPoint {
    Hwnd,
    Webview,
    View,
}

impl BuildPoint {
    fn label(self) -> &'static str {
        match self {
            Self::Hwnd => "hwnd",
            Self::Webview => "webview",
            Self::View => "view",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DestroyedMode {
    Deliver,
    Swallow,
    Defer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct FailureSpec {
    point: BuildPoint,
    mode: DestroyedMode,
}

impl FailureSpec {
    pub(super) fn parse(value: &str) -> Result<Self, WvError> {
        let (point, mode) = value
            .split_once(':')
            .map_or((value, None), |(a, b)| (a, Some(b)));
        let point = match point {
            "hwnd" => BuildPoint::Hwnd,
            "webview" => BuildPoint::Webview,
            "view" => BuildPoint::View,
            _ => {
                return Err(invalid_hook(
                    TEST_WINDOWS_FAIL_BUILD_ENV,
                    "expected hwnd, webview or view",
                ));
            }
        };
        let mode = match mode {
            None => DestroyedMode::Deliver,
            Some("swallow") => DestroyedMode::Swallow,
            Some("defer") => DestroyedMode::Defer,
            Some(_) => {
                return Err(invalid_hook(
                    TEST_WINDOWS_FAIL_BUILD_ENV,
                    "expected no suffix, :swallow or :defer",
                ));
            }
        };
        Ok(Self { point, mode })
    }
}

#[derive(Debug)]
pub(in crate::webview2) struct BuildAttempt {
    build: u64,
    fault: Option<FailureSpec>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DeferredState {
    AwaitingDestroyed,
    Held,
    WakePosted,
    WakeFailed,
}

struct FailedWindow<K> {
    key: K,
    hwnd: isize,
    mode: DestroyedMode,
    state: DeferredState,
}

/// Integer keys are used only by pure state tests. Native callers use Tao IDs.
pub(super) struct FailureControl<K> {
    next_build: Option<FailureSpec>,
    failed: Option<FailedWindow<K>>,
    injected_build: Option<u64>,
}

impl<K: Eq + Copy> FailureControl<K> {
    pub(super) fn new(next_build: Option<FailureSpec>) -> Self {
        Self {
            next_build,
            failed: None,
            injected_build: None,
        }
    }

    pub(super) fn begin_build(&mut self, build: u64) -> BuildAttempt {
        BuildAttempt {
            build,
            fault: self.next_build.take(),
        }
    }

    pub(super) fn fail_at(
        &mut self,
        attempt: &mut BuildAttempt,
        point: BuildPoint,
        key: K,
        hwnd: isize,
    ) -> Result<(), WvError> {
        let Some(fault) = attempt.fault.filter(|fault| fault.point == point) else {
            return Ok(());
        };
        attempt.fault = None;
        if self.failed.is_some() {
            return Err(invalid_hook(
                TEST_WINDOWS_FAIL_BUILD_ENV,
                "a failed native event is already retained",
            ));
        }
        self.injected_build = Some(attempt.build);
        self.failed = Some(FailedWindow {
            key,
            hwnd,
            mode: fault.mode,
            state: DeferredState::AwaitingDestroyed,
        });
        eprintln!(
            "FAILED_BUILD_HWND {hwnd} build={} point={}",
            attempt.build,
            point.label()
        );
        Err(WvError::Webview(format!(
            "injected next-build failure at {}",
            point.label()
        )))
    }

    pub(super) fn observes_failed_build(&self, build: u64) -> bool {
        self.injected_build == Some(build)
    }

    pub(super) fn dispatch_destroyed(&mut self, key: K) -> bool {
        let Some(failed) = self.failed.as_mut().filter(|failed| failed.key == key) else {
            return true;
        };
        match failed.mode {
            DestroyedMode::Deliver => {
                self.failed = None;
                true
            }
            DestroyedMode::Swallow => false,
            DestroyedMode::Defer => {
                if failed.state == DeferredState::AwaitingDestroyed {
                    failed.state = DeferredState::Held;
                    eprintln!("DEFERRED_DESTROYED {}", failed.hwnd);
                }
                false
            }
        }
    }

    pub(super) fn post_deferred_destroyed(
        &mut self,
        terminal_has_run: bool,
        barrier_ready_or_faulted: bool,
        post: impl FnOnce() -> bool,
    ) -> Result<(), WvError> {
        let Some(failed) = self.failed.as_mut().filter(|failed| {
            failed.mode == DestroyedMode::Defer
                && failed.state == DeferredState::Held
                && terminal_has_run
                && barrier_ready_or_faulted
        }) else {
            return Ok(());
        };
        // Record the one attempt before calling out. Failure cannot cause a retry.
        failed.state = DeferredState::WakeFailed;
        if !post() {
            return Err(invalid_hook(
                TEST_WINDOWS_FAIL_BUILD_ENV,
                "deferred Destroyed wake post failed",
            ));
        }
        failed.state = DeferredState::WakePosted;
        Ok(())
    }

    pub(super) fn take_deferred_destroyed(&mut self) -> Option<K> {
        if !self
            .failed
            .as_ref()
            .is_some_and(|failed| failed.state == DeferredState::WakePosted)
        {
            return None;
        }
        let failed = self.failed.take()?;
        eprintln!("DEFERRED_CONSUMED {}", failed.hwnd);
        Some(failed.key)
    }
}

#[cfg(test)]
#[path = "tests/failure.rs"]
mod tests;

//! U18 holds reuse the real UI pump and the caller's original launch deadline.
#![forbid(unsafe_code)]

use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use windows::Win32::UI::WindowsAndMessaging::WM_CLOSE;

use super::{TEST_WINDOWS_NATIVE_HOLD_ENV, invalid_hook};
use crate::WvError;
use crate::webview2::{
    PROFILE_LAUNCH_DEADLINE, check_launch_deadline, milliseconds_until, pump_message_or_handle,
};

const FILE_RECHECK: Duration = Duration::from_millis(10);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HoldPoint {
    Destroyed,
    OriginalResourceResult,
    RendererOutcomes,
    RendererAfterEvent,
    RendererFreshOffers,
    OriginalPostSynthetic,
    BuildPump,
    BuildPumpPrefill,
    BuildPumpPrefillConsumed,
}

#[derive(Debug)]
pub(super) struct NativeHold {
    point: Cell<HoldPoint>,
    directory: PathBuf,
}

impl NativeHold {
    pub(super) fn outcome_hold(&self) -> Option<super::outcome_hold::OutcomeHold> {
        use super::outcome_hold::{HoldMode, OutcomeHold};
        let mode = match self.point.get() {
            HoldPoint::RendererOutcomes | HoldPoint::RendererFreshOffers => HoldMode::BeforeIntake,
            HoldPoint::RendererAfterEvent => HoldMode::AfterNextEvent,
            _ => return None,
        };
        Some(OutcomeHold::new(self.directory.clone(), mode))
    }

    pub(super) fn synthetic_post_directory(&self) -> Option<&Path> {
        (self.point.get() == HoldPoint::OriginalPostSynthetic).then_some(&self.directory)
    }

    /// Captured test-control metadata only. The existing Reader owns held work.
    pub(super) fn original_resource_result_directory(&self) -> Option<&Path> {
        (self.point.get() == HoldPoint::OriginalResourceResult).then_some(&self.directory)
    }

    pub(super) fn parse(value: &str) -> Result<Self, WvError> {
        let (point, directory) = value.split_once(':').ok_or_else(|| {
            invalid_hook(
                TEST_WINDOWS_NATIVE_HOLD_ENV,
                "expected <point>:<absolute-directory>",
            )
        })?;
        let point = match point {
            "destroyed" => HoldPoint::Destroyed,
            "original-resource-result" => HoldPoint::OriginalResourceResult,
            "renderer-outcomes" => HoldPoint::RendererOutcomes,
            "renderer-after-event" => HoldPoint::RendererAfterEvent,
            "renderer-fresh-offers" => HoldPoint::RendererFreshOffers,
            "original-post-synthetic" => HoldPoint::OriginalPostSynthetic,
            "build-pump" => HoldPoint::BuildPump,
            "build-pump-prefill" => HoldPoint::BuildPumpPrefill,
            _ => {
                return Err(invalid_hook(
                    TEST_WINDOWS_NATIVE_HOLD_ENV,
                    "expected destroyed, build-pump, build-pump-prefill, original-resource-result, renderer-outcomes, renderer-after-event, renderer-fresh-offers or original-post-synthetic",
                ));
            }
        };
        let directory = PathBuf::from(directory);
        if !directory.is_absolute() {
            return Err(invalid_hook(
                TEST_WINDOWS_NATIVE_HOLD_ENV,
                "expected an absolute directory",
            ));
        }
        Ok(Self {
            point: Cell::new(point),
            directory,
        })
    }

    /// Select only the first actual parked Destroy command in this fixture.
    /// The existing releasing row owns the receipt and all later observations.
    pub(super) fn take_destroyed_prefill(&self) -> bool {
        if self.point.get() != HoldPoint::BuildPumpPrefill {
            return false;
        }
        self.point.set(HoldPoint::BuildPumpPrefillConsumed);
        true
    }

    fn holds_builds(&self) -> bool {
        matches!(
            self.point.get(),
            HoldPoint::BuildPump
                | HoldPoint::BuildPumpPrefill
                | HoldPoint::BuildPumpPrefillConsumed
        )
    }

    /// Select a real native first-show stimulus for one exact held build.
    /// This never supplies or repairs the independent Created snapshot.
    pub(super) fn initially_focused(&self, build: u64) -> Result<bool, WvError> {
        let inactive = self.holds_builds()
            && control_file_exists(&self.directory.join(format!("{build}.initially-inactive")))?;
        if inactive {
            eprintln!("BUILD_INITIAL_INACTIVE {build}");
        }
        Ok(!inactive)
    }

    pub(super) fn destroyed(&self, hwnd: isize) -> Result<(), WvError> {
        if self.point.get() != HoldPoint::Destroyed {
            return Ok(());
        }
        let release = self.directory.join("release");
        let deadline = Instant::now() + PROFILE_LAUNCH_DEADLINE;
        eprintln!("DESTROY_HELD {hwnd}");
        loop {
            check_launch_deadline(deadline)?;
            if control_file_exists(&release)? {
                eprintln!("DESTROY_HOLD_END {hwnd}");
                return Ok(());
            }
            // No nested pump: the actual WM_DESTROY stack remains incomplete.
            std::thread::sleep(
                FILE_RECHECK.min(deadline.saturating_duration_since(Instant::now())),
            );
        }
    }

    pub(super) fn build_pump(&self, build: u64, deadline: Instant) -> Result<(), WvError> {
        if !self.holds_builds() {
            return Ok(());
        }
        check_launch_deadline(deadline)?;
        let _active = PumpHold::begin()?;
        let release = self.directory.join(format!("{build}.release"));
        eprintln!("BUILD_PUMP_HELD {build}");
        loop {
            check_launch_deadline(deadline)?;
            let released = control_file_exists(&release)?;
            PUMP.with(|state| {
                let mut value = state.get();
                value.posted = None;
                state.set(value);
            });
            // Once released, a nonblocking pass still removes/displays every
            // queued message. Only an observed empty pass ends the hold.
            let milliseconds = if released {
                0
            } else {
                milliseconds_until(deadline)?.min(10)
            };
            pump_message_or_handle(None, milliseconds)?;
            let posted = PUMP.with(|state| state.get().posted).ok_or_else(|| {
                invalid_hook(
                    TEST_WINDOWS_NATIVE_HOLD_ENV,
                    "native pump observation is missing",
                )
            })?;
            if released && !posted {
                eprintln!("BUILD_PUMP_EXIT {build}");
                return Ok(());
            }
        }
    }
}

pub(super) fn control_file_exists(path: &Path) -> Result<bool, WvError> {
    match std::fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => Ok(true),
        Ok(_) => Err(invalid_hook(
            TEST_WINDOWS_NATIVE_HOLD_ENV,
            "control path is not a file",
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(_) => Err(invalid_hook(
            TEST_WINDOWS_NATIVE_HOLD_ENV,
            "control file could not be inspected",
        )),
    }
}

#[derive(Clone, Copy, Default)]
struct PumpState {
    active: bool,
    posted: Option<bool>,
}

thread_local! {
    static PUMP: Cell<PumpState> = const {
        Cell::new(PumpState { active: false, posted: None })
    };
}

struct PumpHold;

impl PumpHold {
    fn begin() -> Result<Self, WvError> {
        PUMP.with(|state| {
            if state.get().active {
                return Err(invalid_hook(
                    TEST_WINDOWS_NATIVE_HOLD_ENV,
                    "nested build-pump hold",
                ));
            }
            state.set(PumpState {
                active: true,
                posted: None,
            });
            Ok(Self)
        })
    }
}

impl Drop for PumpHold {
    fn drop(&mut self) {
        PUMP.set(PumpState::default());
    }
}

/// Called immediately after the existing pump's real `PeekMessageW` read and
/// before dispatch, including empty reads. Never infer this from a posted command.
pub(in crate::webview2) fn observe_pump_message(posted: bool, message: u32, hwnd: isize) {
    PUMP.with(|state| {
        let mut value = state.get();
        if value.active {
            value.posted = Some(posted);
            state.set(value);
            if posted && message == WM_CLOSE {
                eprintln!("PUMP_SAW_CLOSE {hwnd}");
            }
        }
    });
}

#[cfg(test)]
#[path = "tests/native_hold.rs"]
mod tests;

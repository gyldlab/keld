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
    BuildPump,
}

#[derive(Debug)]
pub(super) struct NativeHold {
    point: HoldPoint,
    directory: PathBuf,
}

impl NativeHold {
    pub(super) fn parse(value: &str) -> Result<Self, WvError> {
        let (point, directory) = value.split_once(':').ok_or_else(|| {
            invalid_hook(
                TEST_WINDOWS_NATIVE_HOLD_ENV,
                "expected <point>:<absolute-directory>",
            )
        })?;
        let point = match point {
            "destroyed" => HoldPoint::Destroyed,
            "build-pump" => HoldPoint::BuildPump,
            _ => {
                return Err(invalid_hook(
                    TEST_WINDOWS_NATIVE_HOLD_ENV,
                    "expected destroyed or build-pump",
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
        Ok(Self { point, directory })
    }

    pub(super) fn destroyed(&self, hwnd: isize) -> Result<(), WvError> {
        if self.point != HoldPoint::Destroyed {
            return Ok(());
        }
        let release = self.directory.join("release");
        let deadline = Instant::now() + PROFILE_LAUNCH_DEADLINE;
        eprintln!("DESTROY_HELD {hwnd}");
        loop {
            check_launch_deadline(deadline)?;
            if release_exists(&release)? {
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
        if self.point != HoldPoint::BuildPump {
            return Ok(());
        }
        check_launch_deadline(deadline)?;
        let _active = PumpHold::begin()?;
        let release = self.directory.join(format!("{build}.release"));
        eprintln!("BUILD_PUMP_HELD {build}");
        loop {
            check_launch_deadline(deadline)?;
            let released = release_exists(&release)?;
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

fn release_exists(path: &Path) -> Result<bool, WvError> {
    match std::fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => Ok(true),
        Ok(_) => Err(invalid_hook(
            TEST_WINDOWS_NATIVE_HOLD_ENV,
            "release path is not a file",
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(_) => Err(invalid_hook(
            TEST_WINDOWS_NATIVE_HOLD_ENV,
            "release file could not be inspected",
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

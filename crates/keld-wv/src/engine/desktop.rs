//! Owned Windows desktop observations passed from the native loop to Core.
//! No process handle, window handle or app-provided identity crosses this boundary.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Identity of one primary reader. A copy of the value never grants the ability
/// to stop a reader or access its link. This identity stays inside the Host.
#[derive(Clone, Debug)]
pub struct AppDesktopCapture {
    attempt: u32,
    reader: Arc<AtomicBool>,
}

impl AppDesktopCapture {
    /// Freeze the active reader identity before requesting native observation.
    #[must_use]
    pub fn from_current(attempt: u32, reader: Arc<AtomicBool>) -> Self {
        Self { attempt, reader }
    }

    /// A queued capture must never replace the current producer after retirement.
    pub(crate) fn is_live(&self) -> bool {
        !self.reader.load(Ordering::Acquire)
    }

    /// Accept the sample only while the same authenticated reader remains live.
    #[must_use]
    pub fn matches(&self, attempt: u32, reader: &Arc<AtomicBool>) -> bool {
        self.attempt == attempt
            && Arc::ptr_eq(&self.reader, reader)
            && !reader.load(Ordering::Acquire)
    }
}

impl PartialEq for AppDesktopCapture {
    fn eq(&self, other: &Self) -> bool {
        self.attempt == other.attempt && Arc::ptr_eq(&self.reader, &other.reader)
    }
}

impl Eq for AppDesktopCapture {}

/// Checked rectangle in logical desktop units relative to the primary display.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppDesktopRect {
    /// Left edge, possibly negative for another display.
    pub x: i32,
    /// Top edge, possibly negative for another display.
    pub y: i32,
    /// Width in logical units.
    pub width: u32,
    /// Height in logical units.
    pub height: u32,
}

/// One display read on the Windows UI thread.
#[derive(Debug, Clone, PartialEq)]
pub struct AppDesktopDisplay {
    /// Nonzero display identity in this Host session; not an authorization token.
    pub id: u32,
    /// Full logical display bounds.
    pub bounds: AppDesktopRect,
    /// Usable logical display bounds.
    pub work_area: AppDesktopRect,
    /// Positive finite native display scale.
    pub scale_factor: f64,
}

/// A complete native observation. Errors are carried separately as unavailable.
#[derive(Debug, Clone, PartialEq)]
pub struct AppDesktopFacts {
    /// Identity of the primary display in this observation.
    pub primary_id: u32,
    /// The bounded set of displays captured in this observation.
    pub displays: Vec<AppDesktopDisplay>,
    /// Effective native application appearance.
    pub should_use_dark_colors: bool,
}

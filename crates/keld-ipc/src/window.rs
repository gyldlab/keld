//! Host-owned window payloads (GH-531 §4.a–b).
//!
//! These values use the shared [`crate::codec`] postcard encoder and strict
//! decoder. Variant and field order are wire contracts pinned by the shared
//! `tests/fixtures/window-v0.tsv` vectors. Payload decoding does not authenticate
//! a caller or authorize a window operation: the host registry validates every
//! supplied identity against its current owner and state.

use serde::{Deserialize, Serialize};

/// App-visible, host-minted window number, never reused within an app session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowId(
    /// The host registry's nonzero window number.
    pub u32,
);

/// Opaque host-minted window-incarnation identity, compared only for equality.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowGeneration(
    /// The host registry's nonzero incarnation value, never reused in a session.
    pub u64,
);

/// Both identity halves required by every window-addressed message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowRef {
    /// App-visible window number.
    pub id: WindowId,
    /// Opaque identity of this window incarnation.
    pub incarnation: WindowGeneration,
}

/// Outer-frame bounds in logical points, from the primary display's top-left.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rect {
    /// Horizontal coordinate in logical points.
    pub x: i32,
    /// Vertical coordinate in logical points.
    pub y: i32,
    /// Outer-frame width in logical points.
    pub width: u32,
    /// Outer-frame height in logical points.
    pub height: u32,
}

/// Host-confirmed state applied to the app's local window mirror.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowState {
    /// Current outer-frame bounds.
    pub bounds: Rect,
    /// Whether this application window has focus.
    pub focused: bool,
    /// Whether the native window is maximized.
    pub maximized: bool,
    /// Whether the native window is in full-screen mode.
    pub full_screen: bool,
}

/// One actual host-sampled desktop display. All sizes are positive logical points.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DesktopFact {
    /// Native nonzero display identifier (not a principal or OS grant).
    pub id: u32,
    /// Full display frame relative to the primary display's top-left.
    pub bounds: Rect,
    /// Native usable display frame, in the same coordinate space.
    pub work_area: Rect,
    /// Actual positive finite native-to-backing scale factor.
    pub scale_factor: f64,
}

/// Complete native desktop and appearance capture for the admitted primary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DesktopFacts {
    /// Native display identifier matching exactly one entry in `displays`.
    pub primary_id: u32,
    /// One to 32 complete displays, never a synthetic or truncated subset.
    pub displays: Vec<DesktopFact>,
    /// Observed current effective application appearance.
    pub should_use_dark_colors: bool,
}

/// Maximum postcard storage for a qualified desktop event with at most 32 displays.
///
/// Event tag (1), epoch (10), Option (1), primary id (5), Vec length (1),
/// 32 records of id (5), two four-field rectangles (40), f64 (8), and bool (1).
/// This structural storage bound is not the separately enforced 32-KiB wire cap.
pub const DESKTOP_EVENT_STORAGE_BYTES: usize = 1 + 10 + 1 + 5 + 1 + 32 * (5 + 40 + 8) + 1;

/// Host-to-app window facts; discriminants are append-only, starting at zero.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum WindowEvent {
    /// First fact for a window, or its complete state in a recovery replay.
    Created {
        /// Host-minted identity of the window.
        window: WindowRef,
        /// Host-confirmed initial or replayed state.
        state: WindowState,
        /// Recovery-gap close sequence; absent on every ordinary creation.
        pending_close: Option<u64>,
    },
    /// Changed outer-frame position or size.
    Bounds {
        /// Window whose bounds changed.
        window: WindowRef,
        /// Confirmed outer-frame bounds.
        bounds: Rect,
    },
    /// Changed native focus.
    Focus {
        /// Window whose focus changed.
        window: WindowRef,
        /// Confirmed focus state.
        focused: bool,
    },
    /// Changed native maximization state.
    Maximized {
        /// Window whose maximization state changed.
        window: WindowRef,
        /// Confirmed maximization state.
        maximized: bool,
    },
    /// Changed native full-screen state.
    FullScreen {
        /// Window whose full-screen state changed.
        window: WindowRef,
        /// Confirmed full-screen state.
        full_screen: bool,
    },
    /// A pending close attempt requiring a matching application verdict.
    CloseRequested {
        /// Window whose close is pending.
        window: WindowRef,
        /// Host-minted sequence identifying this close attempt.
        close_seq: u64,
    },
    /// Final fact for a window, after native release.
    Destroyed {
        /// Identity that the app must tombstone before notifying listeners.
        window: WindowRef,
    },
    /// GH481 S1: appended event tag 7. Global passive fact without a `WindowRef`.
    DesktopSnapshot {
        /// Monotonically increasing host-session observation generation, never zero.
        epoch: u64,
        /// Complete current observation or explicit invalidation.
        snapshot: Option<DesktopFacts>,
    },
}

/// App-to-host window calls; discriminants are append-only, starting at zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WindowRequest {
    /// Create a native window, or adopt an eligible recovered survivor.
    Create {
        /// Requested outer-frame width in logical points.
        width: u32,
        /// Requested outer-frame height in logical points.
        height: u32,
        /// Requested horizontal position, or the platform default.
        x: Option<i32>,
        /// Requested vertical position, or the platform default.
        y: Option<i32>,
    },
    /// Request a maximization change, confirmed by a later host fact.
    SetMaximized {
        /// Window to change.
        window: WindowRef,
        /// Desired maximization state.
        maximized: bool,
    },
    /// Request a full-screen change, confirmed by a later host fact.
    SetFullScreen {
        /// Window to change.
        window: WindowRef,
        /// Desired full-screen state.
        full_screen: bool,
    },
    /// Begin or merge a cancelable close attempt; this call never blocks.
    RequestClose {
        /// Window to close.
        window: WindowRef,
    },
    /// Reply to one host-minted pending close sequence.
    CloseReply {
        /// Window whose close is pending.
        window: WindowRef,
        /// Sequence from the matching close request.
        close_seq: u64,
        /// Application decision for that attempt.
        verdict: CloseVerdict,
    },
    /// Destroy a window without a cancelable close event.
    Destroy {
        /// Window to destroy.
        window: WindowRef,
    },
}

/// Application verdict for a pending cancelable close; `Allow = 0`, `Veto = 1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CloseVerdict {
    /// Continue the accepted close through native release.
    Allow,
    /// Retain the native window and its document.
    Veto,
}

/// Host-to-app replies; errors use the existing [`crate::CallError`] contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WindowResponse {
    /// A new native window was created; its `Created` fact precedes this reply.
    Created {
        /// Host-minted identity of the new window.
        window: WindowRef,
    },
    /// A setter was accepted; confirmed state arrives as a host fact.
    Accepted,
    /// A new cancelable close attempt was started.
    CloseRequested {
        /// Host-minted pending close sequence.
        close_seq: u64,
    },
    /// A request merged into an already pending close attempt.
    Merged {
        /// Existing pending close sequence.
        close_seq: u64,
    },
    /// The window is already closing.
    AlreadyClosing,
    /// The matching close verdict was applied.
    ReplyApplied,
    /// The close verdict was stale; the host also records one diagnostic.
    ReplyStale,
    /// Native release completed; the `Destroyed` fact precedes this reply.
    Destroyed,
    /// A successor adopted an already replayed window, without a new native build.
    Adopted {
        /// Existing host identity retained by the adopted window.
        window: WindowRef,
    },
}

#[cfg(test)]
mod tests;

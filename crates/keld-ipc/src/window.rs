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

/// Host-to-app window facts; discriminants are append-only, starting at zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

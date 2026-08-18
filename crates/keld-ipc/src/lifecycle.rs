//! Host-lifecycle kipc channel (KEL-72).
//!
//! Generic host session control — not Electron-named (`keld-compat` /
//! `@keld/electron` map these onto `app.whenReady` / `app.quit` /
//! `window-all-closed`). Channel `3` is a v0 hardcoded handle, same as
//! echo (`1`) and fs (`2`); handshake channel-table exchange is later work.
//!
//! This channel is **not** routed through [`crate::guard_dispatch`]: ready /
//! last-window-closed / quit are session control on the app-link the host
//! already minted, not an OS-authority operation (`fs.read` / media).

use serde::{Deserialize, Serialize};

use crate::frame::ChannelId;

/// Channel handle for host lifecycle `Event` / `Call` frames.
pub const LIFECYCLE_CHANNEL: ChannelId = ChannelId(3);

/// Host → app-process notifications on [`LIFECYCLE_CHANNEL`].
///
/// Encoded as a postcard unit enum (varint discriminant). Pinned in tests:
/// `Ready` is `0x00`, `LastWindowClosed` is `0x01`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LifecycleEvent {
    /// The host is ready for the app process to create windows / run.
    Ready,
    /// The last host-owned window closed.
    LastWindowClosed,
}

/// App-process → host requests on [`LIFECYCLE_CHANNEL`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LifecycleRequest {
    /// End this app-link session. The host replies, then the serve loop
    /// returns — that return is the process-lifecycle oracle for quit.
    Quit,
}

/// Host → app-process reply to [`LifecycleRequest::Quit`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LifecycleResponse {
    /// The host accepted quit and will end the session.
    Quit,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::{decode, encode};

    #[test]
    fn lifecycle_event_postcard_bytes_are_pinned() {
        assert_eq!(encode(&LifecycleEvent::Ready).expect("enc"), [0x00]);
        assert_eq!(
            encode(&LifecycleEvent::LastWindowClosed).expect("enc"),
            [0x01]
        );
        assert_eq!(
            decode::<LifecycleEvent>(&[0x00]).expect("dec"),
            LifecycleEvent::Ready
        );
        assert_eq!(
            decode::<LifecycleEvent>(&[0x01]).expect("dec"),
            LifecycleEvent::LastWindowClosed
        );
    }

    #[test]
    fn lifecycle_quit_postcard_bytes_are_pinned() {
        assert_eq!(encode(&LifecycleRequest::Quit).expect("enc"), [0x00]);
        assert_eq!(encode(&LifecycleResponse::Quit).expect("enc"), [0x00]);
        assert_eq!(
            decode::<LifecycleRequest>(&[0x00]).expect("dec"),
            LifecycleRequest::Quit
        );
    }

    #[test]
    fn lifecycle_channel_is_three() {
        assert_eq!(LIFECYCLE_CHANNEL, ChannelId(3));
        assert_ne!(LIFECYCLE_CHANNEL, crate::ECHO_CHANNEL);
    }
}

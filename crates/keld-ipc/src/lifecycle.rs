//! Host-lifecycle kipc channel (KEL-72).
//!
//! Generic host session control — not Electron-named (`keld-compat` /
//! `@keld/electron` map these onto `app.whenReady` / `app.quit` /
//! `window-all-closed`). Its id comes from the [`crate::channel_table`] entry
//! `lifecycle`, like echo and fs; handshake channel-table exchange is later work.
//!
//! This channel is **not** routed through [`crate::guard_dispatch`]: ready /
//! last-window-closed / quit are session control on the app-link the host
//! already minted, not an OS-authority operation (`fs.read` / media).

use serde::{Deserialize, Serialize};

use crate::frame::ChannelId;

/// Channel handle for host lifecycle `Event` / `Call` frames: the
/// [`crate::channel_table::LIFECYCLE`] entry.
pub const LIFECYCLE_CHANNEL: ChannelId = crate::channel_table::LIFECYCLE.id();

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
    /// Subscribe this authenticated link to an admitted host-event channel.
    /// The payload never selects a principal (GH-508 criterion 16).
    Subscribe {
        /// Generated table id of the requested event-bearing channel.
        channel: u16,
    },
}

/// Closed subscription refusal vocabulary (GH-508 §4.4); order is wire data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SubscribeRefusal {
    /// The host build has no entry with this id.
    UnknownChannel,
    /// This entry does not support explicit host-event subscription.
    NotSubscribable,
    /// Guarded event subscription requires a future guard-owned contract.
    GuardedNotSupported,
    /// The host's authenticated role declaration does not admit this channel.
    RoleNotAdmitted,
}

/// Host → app-process reply to [`LifecycleRequest::Quit`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LifecycleResponse {
    /// The host accepted quit and will end the session.
    Quit,
    /// Subscription and any replay have completed on this same link.
    Subscribed,
    /// No subscription state changed.
    SubscribeRefused {
        /// Typed reason for refusal; the peer matches this enum, not prose.
        reason: SubscribeRefusal,
    },
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
    fn subscription_payloads_pin_append_only_discriminants_and_strict_bounds() {
        let request = LifecycleRequest::Subscribe { channel: 5 };
        assert_eq!(encode(&request).expect("encode Subscribe"), [1, 5]);
        assert_eq!(
            decode::<LifecycleRequest>(&[1, 5]).expect("decode"),
            request
        );
        assert_eq!(
            encode(&LifecycleRequest::Subscribe { channel: u16::MAX }).expect("encode max"),
            [1, 255, 255, 3]
        );
        assert_eq!(encode(&LifecycleResponse::Subscribed).expect("encode"), [1]);
        for (reason, byte) in [
            (SubscribeRefusal::UnknownChannel, 0),
            (SubscribeRefusal::NotSubscribable, 1),
            (SubscribeRefusal::GuardedNotSupported, 2),
            (SubscribeRefusal::RoleNotAdmitted, 3),
        ] {
            let response = LifecycleResponse::SubscribeRefused { reason };
            assert_eq!(encode(&response).expect("encode refusal"), [2, byte]);
            assert_eq!(
                decode::<LifecycleResponse>(&[2, byte]).expect("decode refusal"),
                response
            );
        }
        for bad in [&[1][..], &[1, 128], &[1, 255, 255, 7], &[1, 5, 0], &[2]] {
            assert!(
                decode::<LifecycleRequest>(bad).is_err(),
                "malformed request {bad:?}"
            );
        }
        for bad in [&[1, 0][..], &[2], &[2, 4], &[2, 0, 0], &[3]] {
            assert!(
                decode::<LifecycleResponse>(bad).is_err(),
                "malformed response {bad:?}"
            );
        }
    }

    #[test]
    fn lifecycle_channel_is_three() {
        assert_eq!(LIFECYCLE_CHANNEL, ChannelId(3));
        assert_ne!(LIFECYCLE_CHANNEL, crate::ECHO_CHANNEL);
    }
}

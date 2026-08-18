//! App-process lifecycle wire types (KEL-72).
//!
//! `app.whenReady()`/`app.quit()` are the first Electron-compat primitives:
//! basic process lifecycle, not a `keld-guard`-checked capability (an app's
//! own main process can always ask to quit, the same way Node's own
//! `process.exit()` needs no permission grant). Normative spec:
//! `docs/architecture/04-electron-compat.md` §3.

use serde::{Deserialize, Serialize};

use crate::frame::ChannelId;

/// Channel handle for app-lifecycle `Call`/`Reply` pairs.
pub const APP_CHANNEL: ChannelId = ChannelId(3);

/// App-lifecycle request payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AppRequest {
    /// Blocks (on the host) until the app's window subsystem is ready.
    ///
    /// v0 "ready" is an interim, narrower signal than Electron's own
    /// semantic (which fires before any window exists): here it fires once
    /// the host's window has been *created* — the only real, host-backed
    /// milestone available without `BrowserWindow` (KEL-72 out of scope).
    WhenReady,
    /// Asks the host to end this app-process session.
    Quit,
}

/// App-lifecycle response payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AppResponse {
    /// Reply to [`AppRequest::WhenReady`] once the host signals readiness.
    Ready,
    /// Reply to [`AppRequest::Quit`] acknowledging the session is ending.
    Quitting,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Wire fact: postcard encodes a fieldless enum as just the 0-based
    /// variant-index varint (no field bytes, no tag string) — the TS client
    /// (`kipc.ts`) must match this exactly, not guess. A variant reorder
    /// must fail this test.
    #[test]
    fn fieldless_enum_variants_are_pinned_single_byte_indices() {
        assert_eq!(
            postcard::to_allocvec(&AppRequest::WhenReady).unwrap(),
            [0x00]
        );
        assert_eq!(postcard::to_allocvec(&AppRequest::Quit).unwrap(), [0x01]);
        assert_eq!(postcard::to_allocvec(&AppResponse::Ready).unwrap(), [0x00]);
        assert_eq!(
            postcard::to_allocvec(&AppResponse::Quitting).unwrap(),
            [0x01]
        );
    }
}

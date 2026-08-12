//! keld-guard — the capability engine.
//!
//! Every privileged operation in Keld passes through this crate's
//! `(principal, capability, args) -> Decision` check. Normative spec:
//! `docs/architecture/03-security.md`.
//!
//! v0 scope: principal identity and decision types, so other crates can wire
//! guard checks from day one. Manifest parsing and scope matching land next.

use core::fmt;

/// An unforgeable identity minted by the host for each peer.
///
/// Peers never self-identify; the host assigns ids at link/webview creation
/// and rotates webview principals on navigation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Principal {
    /// The supervised app process (developer's Bun main).
    AppProcess,
    /// A webview, identified by a host-assigned generation-tagged id.
    Webview {
        /// Host-assigned webview identifier.
        id: u32,
        /// Bumped on navigation so stale grants cannot carry over.
        generation: u32,
    },
    /// A native plugin registered at startup.
    Plugin {
        /// Registration index in load order.
        id: u16,
    },
}

/// The outcome of a guard check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// Operation may proceed.
    Allow,
    /// Operation is denied; the reason is safe to surface to developers.
    Deny(DenyReason),
}

/// Why an operation was denied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DenyReason {
    /// No grant covers this capability at all.
    NotGranted {
        /// The capability that was required, e.g. `fs.read`.
        capability: String,
    },
    /// A grant exists but the arguments fall outside its scope.
    OutOfScope {
        /// The capability that was checked.
        capability: String,
        /// Human-readable description of the failing scope.
        scope: String,
    },
    /// The principal is not allowed to use this channel.
    ChannelForbidden {
        /// The kipc channel name.
        channel: String,
    },
}

impl fmt::Display for DenyReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotGranted { capability } => write!(
                f,
                "KELD-GUARD001: capability `{capability}` is not granted. \
                 Add a grant for `{capability}` in keld.permissions.jsonc."
            ),
            Self::OutOfScope { capability, scope } => write!(
                f,
                "KELD-GUARD002: capability `{capability}` denied by scope `{scope}`. \
                 Widen that grant's scope in keld.permissions.jsonc so it includes the requested path."
            ),
            Self::ChannelForbidden { channel } => write!(
                f,
                "KELD-GUARD003: channel `{channel}` is not granted to this principal. \
                 Add `{channel}` to this principal's channels list in keld.permissions.jsonc."
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deny_reasons_render_actionable_text() {
        let not_granted = DenyReason::NotGranted {
            capability: "fs.read".to_owned(),
        };
        let not_granted_msg = not_granted.to_string();
        assert!(
            not_granted_msg.contains("KELD-GUARD001"),
            "{not_granted_msg}"
        );
        assert!(
            not_granted_msg.contains("keld.permissions.jsonc"),
            "{not_granted_msg}"
        );

        let reason = DenyReason::OutOfScope {
            capability: "fs.read".to_owned(),
            scope: "$APPDATA/**".to_owned(),
        };
        assert_eq!(
            reason.to_string(),
            "KELD-GUARD002: capability `fs.read` denied by scope `$APPDATA/**`. \
             Widen that grant's scope in keld.permissions.jsonc so it includes the requested path."
        );

        let channel = DenyReason::ChannelForbidden {
            channel: "fs.readScoped".to_owned(),
        };
        let channel_msg = channel.to_string();
        assert!(channel_msg.contains("KELD-GUARD003"), "{channel_msg}");
        assert!(
            channel_msg.contains("keld.permissions.jsonc"),
            "{channel_msg}"
        );
    }

    #[test]
    fn webview_principals_distinguish_generations() {
        let before = Principal::Webview {
            id: 7,
            generation: 1,
        };
        let after = Principal::Webview {
            id: 7,
            generation: 2,
        };
        assert_ne!(before, after);
    }
}

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
            Self::NotGranted { capability } => {
                write!(f, "capability `{capability}` is not granted")
            }
            Self::OutOfScope { capability, scope } => {
                write!(f, "capability `{capability}` denied by scope `{scope}`")
            }
            Self::ChannelForbidden { channel } => {
                write!(f, "channel `{channel}` is not granted to this principal")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deny_reasons_render_actionable_text() {
        let reason = DenyReason::OutOfScope {
            capability: "fs.read".to_owned(),
            scope: "$APPDATA/**".to_owned(),
        };
        assert_eq!(
            reason.to_string(),
            "capability `fs.read` denied by scope `$APPDATA/**`"
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

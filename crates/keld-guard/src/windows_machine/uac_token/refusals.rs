//! Stable Windows token refusal codes and remediation text.

use std::fmt;
use std::io;

const FIX_TOKEN_OS: &str = "Retry from a fresh launch of the installed host; Keld never \
     substitutes another token, changes a token's owner or DACL, or enables a privilege \
     to get past this.";

/// A refusal by a `keld-guard` Windows token wrapper.
///
/// [`code`](Self::code) is its stable `KELD-GUARD*` code; the `Display` text names the
/// refusal and ends with its fix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WindowsTokenError {
    /// A token open, query, privilege lookup, duplication or impersonation call failed,
    /// or returned data outside its bounds (`KELD-GUARD018`).
    Os {
        /// The refused Windows operation.
        operation: &'static str,
        /// What Windows reported, or which bound the returned data broke.
        detail: String,
    },
    /// The current process token runs in another session than the initiating token
    /// (`KELD-GUARD019`).
    SessionMismatch {
        /// The initiating token's session.
        expected: u32,
        /// The current process token's session.
        actual: u32,
    },
    /// The current process token does not hold `SeImpersonatePrivilege` enabled
    /// (`KELD-GUARD020`).
    ImpersonatePrivilegeUnavailable {
        /// `absent`, `disabled` or `removed`.
        state: &'static str,
    },
    /// The calling thread already impersonates a token (`KELD-GUARD021`).
    ThreadImpersonating,
    /// The initiating token is elevated or not at Medium integrity (`KELD-GUARD025`).
    InitiatingTokenProfile {
        /// Whether the token is elevated.
        elevated: bool,
        /// The token's mandatory integrity RID.
        integrity_rid: u32,
    },
}

impl WindowsTokenError {
    /// Stable `KELD-GUARD*` code for this refusal.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Os { .. } => "KELD-GUARD018",
            Self::SessionMismatch { .. } => "KELD-GUARD019",
            Self::ImpersonatePrivilegeUnavailable { .. } => "KELD-GUARD020",
            Self::ThreadImpersonating => "KELD-GUARD021",
            Self::InitiatingTokenProfile { .. } => "KELD-GUARD025",
        }
    }

    pub(in super::super) fn os(operation: &'static str, source: &io::Error) -> Self {
        Self::Os {
            operation,
            detail: source.to_string(),
        }
    }
}

impl fmt::Display for WindowsTokenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Os { operation, detail } => write!(
                f,
                "KELD-GUARD018: Windows token operation `{operation}` failed: {detail}. {FIX_TOKEN_OS}"
            ),
            Self::SessionMismatch { expected, actual } => write!(
                f,
                "KELD-GUARD019: this process runs in session {actual}, not the initiating \
                 session {expected}. Approve the elevation prompt in the initiating user's \
                 interactive session; a process in another session cannot launch the \
                 candidate there."
            ),
            Self::ImpersonatePrivilegeUnavailable { state } => write!(
                f,
                "KELD-GUARD020: SeImpersonatePrivilege is {state} in this process token. \
                 Approve the full elevated administrator prompt; Keld never enables a \
                 privilege as a fallback."
            ),
            Self::ThreadImpersonating => write!(
                f,
                "KELD-GUARD021: the calling thread is already impersonating. Revert the \
                 existing impersonation first; Keld refuses nested impersonation."
            ),
            Self::InitiatingTokenProfile {
                elevated,
                integrity_rid,
            } => write!(
                f,
                "KELD-GUARD025: the initiating token is {} at integrity level \
                 0x{integrity_rid:04X}; only a non-elevated Medium (0x2000) token may start \
                 the candidate. Start the update from the installed host as the signed-in \
                 user without elevation; Keld never substitutes another token.",
                if *elevated {
                    "elevated"
                } else {
                    "not elevated"
                }
            ),
        }
    }
}

impl std::error::Error for WindowsTokenError {}

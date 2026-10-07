//! The helper's typed refusals.

use std::fmt;

/// Why the helper refused. Every refusal precedes the writer lease and any write.
#[derive(Debug)]
pub(crate) enum HelperError {
    /// `KELD-HELPER-001`: the helper was not started as `keld-host.exe` or an
    /// administrator starts it, decided before any open or write.
    Invocation {
        /// What was refused. It never echoes the argument.
        detail: String,
    },
    /// `KELD-HELPER-002`: the helper could not verify its own image through the single
    /// `keld-guard` Authenticode owner, so it could not anchor itself.
    #[cfg(windows)]
    OwnImage {
        /// What was refused.
        detail: String,
        /// How to correct the image.
        fix: &'static str,
    },
    /// The self-anchor refused, under `keld-update`'s own `KELD-UPDATE-*` code.
    #[cfg(windows)]
    Anchor(keld_update::UpdateError),
    /// `KELD-HELPER-003`: the activation role, which refuses after a passing self-anchor
    /// until KEL-270 T4d slice S11 lands (KEL-53 §4 *Interim*).
    #[cfg(windows)]
    ActivationDisabled,
    /// `KELD-HELPER-004`: the recovery-only role, which refuses after a passing
    /// self-anchor until KEL-270 T4d slice S10 lands (KEL-53 §4 *Interim*).
    #[cfg(windows)]
    RecoveryDisabled,
}

impl fmt::Display for HelperError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invocation { detail } => write!(
                f,
                "KELD-HELPER-001: keld-updater-helper.exe refused how it was started ({detail}). \
                 Only keld-host.exe starts it, on Windows, with exactly one argument: the \
                 activation rendezvous `\\\\.\\pipe\\keld-attempt-<64 lowercase hex>`."
            ),
            #[cfg(windows)]
            Self::OwnImage { detail, fix } => write!(
                f,
                "KELD-HELPER-002: keld-updater-helper.exe could not verify its own image ({detail}). {fix}"
            ),
            #[cfg(windows)]
            Self::Anchor(source) => write!(f, "{source}"),
            #[cfg(windows)]
            Self::ActivationDisabled => f.write_str(
                "KELD-HELPER-003: the updater helper's activation role is not available in this \
                 release: it anchored itself and then refused, before the writer lease and any \
                 write. Nothing changed; keep running the current version.",
            ),
            #[cfg(windows)]
            Self::RecoveryDisabled => f.write_str(
                "KELD-HELPER-004: the updater helper's recovery-only role is disabled in this \
                 release (RecoveryDisabled): it anchored itself and then refused, before the \
                 writer lease and any write. No supported resolution exists other than \
                 administrator action; any activation journal, the pointers and the versions are \
                 preserved.",
            ),
        }
    }
}

impl std::error::Error for HelperError {
    #[cfg(windows)]
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        if let Self::Anchor(source) = self {
            Some(source)
        } else {
            None
        }
    }
}

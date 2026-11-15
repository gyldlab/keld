//! The helper's typed refusals.

use std::fmt;

#[cfg(windows)]
use keld_runtime::windows_job::{WINDOWS_UPDATER_HELPER_RECOVERY_SELECTOR, WindowsDllSearchError};

/// Why the helper refused. Every refusal precedes the writer lease and any write.
#[derive(Debug)]
pub(crate) enum HelperError {
    /// `keld-runtime`'s `KELD-RUNTIME-018`, passed through: `main`'s first statement
    /// could not restrict this process's DLL search to System32, so the helper exits
    /// before anything else runs.
    #[cfg(windows)]
    DllSearch(WindowsDllSearchError),
    /// `KELD-HELPER-001`: the helper was not started the way `keld-host.exe` or an
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
    ///
    /// Its text is not `keld-update`'s `MachineRecoveryGuidance::RecoveryDisabled`
    /// guidance, on purpose. That guidance tells an ordinary launch that *this
    /// installation needs* recovery the release cannot perform. The helper refuses the
    /// role right after its self-anchor, without deciding whether any journal phase or
    /// `current` needs recovery, so it states only that the role is disabled and that the
    /// protected state is preserved.
    #[cfg(windows)]
    RecoveryDisabled,
}

impl fmt::Display for HelperError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            #[cfg(windows)]
            Self::DllSearch(source) => write!(f, "{source}"),
            Self::Invocation { detail } => {
                write!(
                    f,
                    "KELD-HELPER-001: keld-updater-helper.exe refused how it was started \
                     ({detail}). "
                )?;
                fmt_accepted_starts(f)
            }
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

/// The `KELD-HELPER-001` correction: the only starts the helper accepts (KEL-53 §4
/// "Helper launch and self-anchor": the host starts it, and an administrator may start
/// the same file directly).
#[cfg(windows)]
fn fmt_accepted_starts(f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(
        f,
        "It takes exactly one argument: keld-host.exe passes the activation rendezvous \
         `\\\\.\\pipe\\keld-attempt-<64 lowercase hex>`, and keld-host.exe or an administrator \
         passes `{WINDOWS_UPDATER_HELPER_RECOVERY_SELECTOR}`."
    )
}

#[cfg(not(windows))]
fn fmt_accepted_starts(f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str("It runs only on Windows, where keld-host.exe or an administrator starts it.")
}

impl std::error::Error for HelperError {
    #[cfg(windows)]
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::DllSearch(source) => Some(source),
            Self::Anchor(source) => Some(source),
            _ => None,
        }
    }
}

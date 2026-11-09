//! Typed Windows verified-signature refusals and bounded signed app-ID carrier.
//!
//! `WinVerifyTrust` state, `Crypt32` and image-handle ownership remain in the parent.

use super::{WINDOWS_SIGNED_APP_ID_PREFIX, WINDOWS_SIGNED_PROGRAM_NAME_MAX_UNITS};
use std::fmt;

/// A refusal by the KEL-135 Windows Authenticode verifier, with its fix guidance.
///
/// The caller maps it to its own diagnostic code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsAuthenticodeError {
    detail: String,
    fix: &'static str,
    close_failure: Option<Box<WindowsAuthenticodeError>>,
}

impl WindowsAuthenticodeError {
    /// What was refused.
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }

    /// How to correct the image or its signature.
    #[must_use]
    pub const fn fix(&self) -> &'static str {
        self.fix
    }

    /// A failure to close the `WinTrust` state that followed this refusal. A close
    /// failure after an otherwise accepted image is the refusal itself, not this.
    #[must_use]
    pub fn close_failure(&self) -> Option<&Self> {
        self.close_failure.as_deref()
    }

    pub(super) fn with_close_failure(mut self, close_failure: Self) -> Self {
        self.close_failure = Some(Box::new(close_failure));
        self
    }
}

impl fmt::Display for WindowsAuthenticodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}. {}", self.detail, self.fix)?;
        if let Some(close_failure) = &self.close_failure {
            write!(f, "; cleanup: {close_failure}")?;
        }
        Ok(())
    }
}

impl std::error::Error for WindowsAuthenticodeError {}

pub(super) fn windows_identity_error(
    detail: impl Into<String>,
    fix: &'static str,
) -> WindowsAuthenticodeError {
    WindowsAuthenticodeError {
        detail: detail.into(),
        fix,
        close_failure: None,
    }
}

pub(super) fn parse_windows_signed_program_name(
    value: &str,
) -> Result<&str, WindowsAuthenticodeError> {
    let Some(app_id) = value.strip_prefix(WINDOWS_SIGNED_APP_ID_PREFIX) else {
        return Err(windows_identity_error(
            "the verified Authenticode signature has no Keld app-id description",
            "Sign the package with `/d \"keld.app-id/v1:<canonical-app.id>\"`, then rebuild it.",
        ));
    };
    if app_id.is_empty()
        || app_id.len() > 255
        || value.len() > WINDOWS_SIGNED_PROGRAM_NAME_MAX_UNITS
    {
        return Err(windows_identity_error(
            "the verified Authenticode app-id description is empty or exceeds 255 app-id bytes",
            "Use 1-255 lowercase ASCII bytes after the exact `keld.app-id/v1:` prefix.",
        ));
    }
    Ok(app_id)
}

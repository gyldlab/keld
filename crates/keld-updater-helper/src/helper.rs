//! The Windows helper: argument check, self-anchor and role dispatch (KEL-53 §4
//! "Helper launch and self-anchor").

use std::ffi::OsString;

use keld_guard::{WindowsAuthenticodeError, WindowsAuthenticodeImage};
use keld_ipc::WindowsNamedPipeBootstrapStream;
use keld_runtime::windows_job::WINDOWS_UPDATER_HELPER_RECOVERY_SELECTOR;
use keld_update::{UpdaterHelperAnchor, UpdaterHelperRole, anchor_updater_helper};

use crate::error::HelperError;

/// The correction for an executable path the helper cannot resolve or verify.
const OWN_IMAGE_FIX: &str = "Run only the signed keld-updater-helper.exe from its \
     installation's protected version tree; repair or reinstall through the trusted \
     installer if it is damaged.";

/// Checks the argument, anchors the helper to its installation, then runs the role.
pub(crate) fn run(args: impl IntoIterator<Item = OsString>) -> Result<(), HelperError> {
    let role = parse_argument(args)?;
    let anchor = anchor_self(role)?;
    dispatch(role, &anchor)
}

/// Decides the role from the arguments alone, before any open or write.
///
/// There must be exactly one argument (KEL-53 §7 "17 (argument shape)"). It conveys no
/// authority and is one of exactly two shapes:
/// - the exact local `\\.\pipe\keld-attempt-<64 lowercase hex>` activation rendezvous,
///   as `keld-ipc`'s [`WindowsNamedPipeBootstrapStream::is_attempt_endpoint`], the one
///   owner of that shape, accepts it; or
/// - the recovery-role selector, compared exactly with `keld-runtime`'s
///   [`WINDOWS_UPDATER_HELPER_RECOVERY_SELECTOR`], the one owner of that literal.
fn parse_argument(
    args: impl IntoIterator<Item = OsString>,
) -> Result<UpdaterHelperRole, HelperError> {
    let args: Vec<OsString> = args.into_iter().collect();
    let [argument] = args.as_slice() else {
        return Err(invocation(format!(
            "it takes exactly one argument, not {}",
            args.len()
        )));
    };
    let Some(argument) = argument.to_str() else {
        return Err(invocation("its argument is not valid Unicode".to_owned()));
    };
    if WindowsNamedPipeBootstrapStream::is_attempt_endpoint(argument) {
        return Ok(UpdaterHelperRole::Activation);
    }
    if argument == WINDOWS_UPDATER_HELPER_RECOVERY_SELECTOR {
        return Ok(UpdaterHelperRole::Recovery);
    }
    Err(invocation(format!(
        "its argument is neither an exact local keld-attempt rendezvous nor \
         `{WINDOWS_UPDATER_HELPER_RECOVERY_SELECTOR}`"
    )))
}

fn invocation(detail: String) -> HelperError {
    HelperError::Invocation { detail }
}

/// Anchors the running image to the installation that holds it, before any lease or
/// write.
///
/// The image is verified once through the single `keld-guard` Authenticode owner, and
/// that one verification supplies both the handle and the identity that
/// `keld-update`'s self-anchor binds: the handle that `WinVerifyTrust` pinned, never a
/// reopened path. The canonical path only locates the installation.
fn anchor_self(role: UpdaterHelperRole) -> Result<UpdaterHelperAnchor, HelperError> {
    let executable = std::env::current_exe().map_err(|source| HelperError::OwnImage {
        detail: format!("its executable path is unavailable: {source}"),
        fix: OWN_IMAGE_FIX,
    })?;
    let verified = WindowsAuthenticodeImage::open(&executable)
        .and_then(WindowsAuthenticodeImage::verify)
        .map_err(|error| own_image_refusal(&error))?;
    let locator = executable
        .canonicalize()
        .map_err(|source| HelperError::OwnImage {
            detail: format!("its executable path cannot be made canonical: {source}"),
            fix: OWN_IMAGE_FIX,
        })?;
    let identity = verified.identity();
    anchor_updater_helper(
        role,
        &locator,
        verified.file(),
        identity.publisher_scope(),
        identity.app_id(),
    )
    .map_err(HelperError::Anchor)
}

/// Carries a `keld-guard` refusal, and any failure to close its `WinTrust` state, with
/// `keld-guard`'s own correction.
fn own_image_refusal(error: &WindowsAuthenticodeError) -> HelperError {
    let detail = match error.close_failure() {
        None => error.detail().to_owned(),
        Some(close_failure) => format!("{}; cleanup: {close_failure}", error.detail()),
    };
    HelperError::OwnImage {
        detail,
        fix: error.fix(),
    }
}

/// Runs `role` for an anchored helper. Until their slices land, both roles refuse here,
/// right after the passing self-anchor and before the writer lease and any write
/// (KEL-53 §4 *Interim*): activation until S11, recovery until S10.
fn dispatch(role: UpdaterHelperRole, _anchor: &UpdaterHelperAnchor) -> Result<(), HelperError> {
    Err(interim_refusal(role))
}

fn interim_refusal(role: UpdaterHelperRole) -> HelperError {
    match role {
        UpdaterHelperRole::Activation => HelperError::ActivationDisabled,
        UpdaterHelperRole::Recovery => HelperError::RecoveryDisabled,
    }
}

#[cfg(test)]
mod tests;

//! Windows initial-install entry is deliberately unprivileged and fail-closed.
//!
//! A release-signature check is NOT production catalog authorization. The
//! existing KEL-53 `BaselineVerifier` and `initialize_windows_per_user_baseline`
//! APIs require trusted installer configuration that does not yet have an
//! approved producer/consumer contract in this integration. In particular,
//! argv and an unverified catalog cannot create `WindowsBaselineTrust`.
//!
//! Until the release owner approves an independently authenticated catalog
//! and binds it to the exact signed Host and `PerUserDirect` installation, this
//! role must make no filesystem reads, writes, process launches or receipts.
//! This is a shipping refusal, not a test lease or a temporary override.

use std::ffi::{OsStr, OsString};
use std::fmt;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum InitialInstallRefusal {
    MalformedInvocation,
    CatalogAuthorityUndefined,
}

impl fmt::Display for InitialInstallRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MalformedInvocation => write!(
                f,
                "KELD-HOST-INITIAL-001: initial-install role requires exactly one absolute request-file locator"
            ),
            Self::CatalogAuthorityUndefined => write!(
                f,
                "KELD-HOST-INITIAL-002: initial installation refused: production catalog authority and installer trust binding are not approved"
            ),
        }
    }
}

/// Grammar-only observation. An absolute path is never an authority token,
/// and no read of the named file occurs at this boundary.
fn request_locator(args: &[OsString]) -> Result<&Path, InitialInstallRefusal> {
    if args.len() != 3 || args[1] != OsStr::new(super::WINDOWS_INITIAL_INSTALL_ARG) {
        return Err(InitialInstallRefusal::MalformedInvocation);
    }
    let path = Path::new(&args[2]);
    if !path.is_absolute() || path.file_name().is_none() {
        return Err(InitialInstallRefusal::MalformedInvocation);
    }
    Ok(path)
}

/// Rejects before deriving package identity, opening caller paths, creating a
/// scaffold or calling KEL-53's protected publication API.
///
/// A future implementation may replace this refusal only when a separate
/// approved catalog authority constructs verified, pinned installer inputs.
pub(super) fn run(args: &[OsString]) -> Result<(), InitialInstallRefusal> {
    let _untrusted_request = request_locator(args)?;
    Err(InitialInstallRefusal::CatalogAuthorityUndefined)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn argv(locator: &OsStr) -> Vec<OsString> {
        vec![
            OsString::from("keld-host.exe"),
            OsString::from(super::super::WINDOWS_INITIAL_INSTALL_ARG),
            locator.to_os_string(),
        ]
    }

    #[test]
    fn exact_private_role_shape_is_observable_but_never_authority() {
        let args = argv(OsStr::new(r"C:\operator\initial-install-request.json"));
        assert_eq!(
            request_locator(&args).expect("the private role has a bounded grammar"),
            Path::new(r"C:\operator\initial-install-request.json")
        );
        assert_eq!(
            run(&args),
            Err(InitialInstallRefusal::CatalogAuthorityUndefined)
        );
        assert!(
            run(&args)
                .unwrap_err()
                .to_string()
                .contains("KELD-HOST-INITIAL-002")
        );
    }

    #[test]
    fn malformed_argv_relative_alias_or_missing_request_is_denied() {
        let valid = argv(OsStr::new(r"C:\operator\initial-install-request.json"));
        let mut extra = valid.clone();
        extra.push(OsString::from("--allow-unsigned"));
        let mut wrong_flag = valid.clone();
        wrong_flag[1] = OsString::from("--hello");
        for args in [
            Vec::<OsString>::new(),
            vec![OsString::from("keld-host.exe")],
            extra,
            wrong_flag,
            argv(OsStr::new("relative.json")),
            argv(OsStr::new("")),
            argv(OsStr::new(r"C:\")),
        ] {
            assert_eq!(run(&args), Err(InitialInstallRefusal::MalformedInvocation));
        }
    }

    #[test]
    fn forged_signer_wrong_package_tampering_and_path_substitution_never_publish() {
        let temp = tempfile::tempdir().expect("private negative-control fixture");
        let request = temp.path().join("initial-install-request.json");
        let protected_marker = temp.path().join("protected-state-must-not-appear");
        // These are deliberately UNAUTHENTICATED claims, not proof that their
        // signatures or product identities were cryptographically checked.
        // Every variant must be refused before even reading the caller file.
        for bytes in [
            br#"{"catalog":"unsigned","catalog_signature":"attacker"}"#.as_slice(),
            br#"{"publisher_scope":"wrong-signer","app_id":"wrong.app"}"#,
            br#"{"package":"other-app","version":"99.0.0"}"#,
            br#"{"manifest":"tampered-manifest","canonical_content":"tampered"}"#,
            br#"{"install_root":"C:/substituted-location","update_root":"C:/other"}"#,
        ] {
            fs::write(&request, bytes).expect("negative-control request bytes");
            let before = fs::read(&request).expect("original untrusted bytes");
            assert_eq!(
                run(&argv(request.as_os_str())),
                Err(InitialInstallRefusal::CatalogAuthorityUndefined),
                "untrusted request has no production installation authority"
            );
            assert_eq!(fs::read(&request).unwrap(), before);
            assert!(!protected_marker.exists());
            assert_eq!(
                fs::read_dir(temp.path()).unwrap().count(),
                1,
                "failed initial installation must create no artifacts"
            );
        }
        // Even an absent request must be refused *without opening that path*.
        fs::remove_file(&request).unwrap();
        assert_eq!(
            run(&argv(request.as_os_str())),
            Err(InitialInstallRefusal::CatalogAuthorityUndefined)
        );
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 0);
    }
}

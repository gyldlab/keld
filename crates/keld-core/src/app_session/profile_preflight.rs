//! Explicit-profile eligibility before application resources are acquired.

use keld_guard::PermissionsManifest;
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
use keld_guard::ProfileState;

use super::HostAppError;

/// Direct diagnostics have no compiled digest. Their caller uses the bounded
/// project loader and this same refusal, without inventing verified provenance.
pub(crate) fn reject_diagnostic_profile(
    manifest: &PermissionsManifest,
) -> Result<(), HostAppError> {
    if manifest.declared_profile().is_some() {
        return Err(HostAppError::new(
            "KELD-CORE-035",
            "diagnostic profile preflight",
            "an explicit security profile cannot run through the unprivileged diagnostic entry",
            "Use the supported compiled no-flag primary host; do not remove a profile to bypass admission.",
        ));
    }
    Ok(())
}

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
pub(super) fn validate_profile(
    schema: u8,
    verified: &keld_guard::verified_manifest::VerifiedManifest,
    diagnostic: bool,
) -> Result<ProfileState, HostAppError> {
    let manifest = verified.manifest();
    let facade = match schema {
        1 | 3 => false,
        2 | 4 => true,
        _ => {
            return Err(super::boot_error(
                "unsupported compiled boot schema in profile preflight",
                "Recompile with a supporting exact compiler and host pair.",
            ));
        }
    };
    if schema != super::boot_schema(facade, manifest.declared_profile()) {
        return Err(super::boot_error(
            "boot schema and verified /profile presence disagree",
            "Recompile the same policy and window mode with a matching host; never strip the declaration or downgrade the schema.",
        ));
    }
    if diagnostic {
        reject_diagnostic_profile(manifest)?;
        if facade {
            return Err(super::boot_error(
                "the renderer-only diagnostic entry cannot execute a facade boot",
                "Use the supported guarded no-flag primary host; do not change window ownership to bypass admission.",
            ));
        }
        return Ok(ProfileState::Unverified);
    }
    // Called only by the validated no-flag primary host, never by an addon or
    // secondary-role launcher. State values are not launch capabilities.
    #[cfg(not(target_os = "macos"))]
    {
        if facade || manifest.declared_profile().is_some() {
            return Err(HostAppError::new(
                "KELD-CORE-034",
                "explicit profile availability",
                "explicit security profiles are not qualified for this platform's primary launch",
                "Use a qualified macOS primary host for this profile; do not discard the requested policy.",
            ));
        }
        // Preserve existing Windows/Linux containment; do not infer a newly
        // declared Strict receipt from platform choice.
        Ok(ProfileState::Unverified)
    }
    #[cfg(target_os = "macos")]
    {
        let requested = manifest
            .declared_profile()
            .map_or(ProfileState::Unverified, ProfileState::from);
        keld_guard::admit_uncontained(requested).map_err(HostAppError::admission)
    }
}

#[cfg(target_os = "macos")]
pub(super) fn disclose_profile(profile: ProfileState, bound: bool) -> Result<(), HostAppError> {
    use std::io::Write as _;

    let phase = if bound { "bound primary" } else { "requested" };
    let mut output = std::io::stderr().lock();
    writeln!(
        output,
        "Keld security profile ({phase}; source keld.permissions.jsonc /profile): {}",
        profile.as_report_str()
    )
    .and_then(|()| {
        if profile == ProfileState::Legacy && !bound {
            writeln!(output, "{}", keld_guard::LEGACY_PROFILE_FORFEIT)
        } else {
            Ok(())
        }
    })
    .map_err(|source| {
        HostAppError::io(
            "KELD-CORE-037",
            "security profile disclosure",
            &source,
            "Restore the host diagnostic sink before launching; the Legacy forfeit must be visible.",
        )
    })
}

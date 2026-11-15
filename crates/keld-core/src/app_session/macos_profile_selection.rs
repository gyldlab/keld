//! macOS signed identity to persistent/ephemeral `WebView` profile selection.
use super::{DevHostLease, HostAppError};
use crate::macos_profile_identity::verified_current_process_signing_info;
use getrandom::fill as fill_os_random;
use keld_wv::profile::EphemeralProfile;
use keld_wv::{ProfileIdentity, WebProfileSelection};
use sha2::{Digest as _, Sha256};

#[cfg(target_os = "macos")]
const MACOS_PUBLISHER_SCOPE_DOMAIN: &[u8] = b"keld.publisher.macos/v1\0";

#[cfg(target_os = "macos")]
#[derive(Debug)]
pub(super) struct MacValidatedAppIdentity {
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    pub(super) team_identifier: Box<str>,
    pub(super) publisher_scope: [u8; 32],
    pub(super) signing_identifier: Box<str>,
    pub(super) profile_identity: ProfileIdentity,
}

#[cfg(target_os = "macos")]
impl MacValidatedAppIdentity {
    pub(super) fn from_verified_parts(
        team_identifier: &str,
        signing_identifier: &str,
    ) -> Result<Self, HostAppError> {
        if team_identifier.len() != 10
            || !team_identifier
                .bytes()
                .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
        {
            return Err(macos_identity_error(
                "the validated code signature has no canonical 10-character Team Identifier",
            ));
        }
        let mut publisher_hasher = Sha256::new();
        publisher_hasher.update(MACOS_PUBLISHER_SCOPE_DOMAIN);
        publisher_hasher.update(team_identifier.as_bytes());
        let publisher_digest = publisher_hasher.finalize();
        let mut publisher_scope = [0_u8; 32];
        publisher_scope.copy_from_slice(&publisher_digest);
        let profile_identity = ProfileIdentity::from_host_verified_parts(
            publisher_scope,
            signing_identifier,
        )
        .map_err(|source| {
            macos_identity_error(format!(
                "the validated code-signing identifier is not a canonical Keld app id: {source}"
            ))
        })?;
        Ok(Self {
            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
            team_identifier: team_identifier.into(),
            publisher_scope,
            signing_identifier: signing_identifier.into(),
            profile_identity,
        })
    }

    fn into_profile_selection(self) -> Result<WebProfileSelection, HostAppError> {
        debug_assert_eq!(
            ProfileIdentity::from_host_verified_parts(
                self.publisher_scope,
                &self.signing_identifier,
            ),
            Ok(self.profile_identity)
        );
        WebProfileSelection::persistent(Some(self.profile_identity))
            .map_err(|source| macos_identity_error(source.to_string()))
    }
}

#[cfg(target_os = "macos")]
pub(super) fn select_macos_profile_mode_with(
    has_authenticated_dev_lease: bool,
    release_identity: impl FnOnce() -> Result<MacValidatedAppIdentity, HostAppError>,
) -> Result<WebProfileSelection, HostAppError> {
    if has_authenticated_dev_lease {
        let mut launch_nonce = [0_u8; 32];
        fill_os_random(&mut launch_nonce)
            .map_err(|source| macos_identity_error(format!("host randomness failed: {source}")))?;
        let profile = EphemeralProfile::from_host_random(launch_nonce)
            .map_err(|source| macos_identity_error(source.to_string()))?;
        return Ok(WebProfileSelection::ephemeral_dev(profile));
    }
    release_identity()?.into_profile_selection()
}

#[cfg(target_os = "macos")]
pub(super) fn macos_profile_selection(
    dev_lease: Option<&DevHostLease>,
) -> Result<WebProfileSelection, HostAppError> {
    select_macos_profile_mode_with(dev_lease.is_some(), verified_macos_app_identity)
}

#[cfg(target_os = "macos")]
pub(super) fn verified_macos_app_identity() -> Result<MacValidatedAppIdentity, HostAppError> {
    let signing_info = verified_current_process_signing_info().map_err(macos_identity_error)?;
    MacValidatedAppIdentity::from_verified_parts(
        &signing_info.team_identifier,
        &signing_info.signing_identifier,
    )
}

#[cfg(target_os = "macos")]
pub(super) fn macos_identity_error(detail: impl Into<String>) -> HostAppError {
    HostAppError::new(
        "KELD-WV-009",
        "macOS authenticated app identity",
        detail,
        "Launch a validly signed macOS app with a canonical signed identifier, or use `keld dev` for a fresh nonpersistent profile.",
    )
}

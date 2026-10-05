use std::cmp::Ordering;
use std::path::{Path, PathBuf};

use ed25519_dalek::VerifyingKey;
use keld_guard::ProfileDigest;
use semver::Version;

use crate::Channel;
use crate::error::{ProvenanceField, ProvenanceUnavailable, UpdateError, hex_digest};

/// Stable identity of the compiled-in Ed25519 release key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SigningKeyId([u8; 32]);

impl SigningKeyId {
    #[cfg(any(windows, test, feature = "fuzzing"))]
    pub(crate) const fn from_digest(digest: [u8; 32]) -> Self {
        Self(digest)
    }

    /// Derives the v0 key identity from the exact 32-byte Ed25519 public key.
    #[must_use]
    pub fn from_public_key(public_key: &[u8; 32]) -> Self {
        Self(*blake3::hash(public_key).as_bytes())
    }

    /// Returns the 32-byte BLAKE3 key identity stored in provenance.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Exact release artifact identity used by provenance and later activation state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactIdentity {
    /// Canonical application id.
    pub app_id: String,
    /// Release channel.
    pub channel: Channel,
    /// Compiled target string, such as `windows-x64`.
    pub target: String,
    /// Complete strict-SemVer string, including build metadata.
    pub version: String,
    /// BLAKE3 of the decompressed canonical package bytes.
    pub content_blake3: [u8; 32],
}

/// Explicit Windows direct-install mode recorded by the trusted installer.
///
/// This selects which OS writer profile may obtain an activation lease. It does not
/// change package verification, the activation journal, candidate health or recovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DirectInstallMode {
    /// Default installation beneath the installing user's `LocalAppData` tree.
    PerUserDirect,
    /// Program Files installation whose updates request explicit UAC.
    MachineUacDirect,
    /// Opt-in Program Files mode with a still-separately-gated privileged writer.
    MachineSeamlessDirect,
}

impl DirectInstallMode {
    /// Stable local-record spelling; it is not a manifest field.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PerUserDirect => "per-user-direct",
            Self::MachineUacDirect => "machine-uac-direct",
            Self::MachineSeamlessDirect => "machine-seamless-direct",
        }
    }

    #[cfg(any(windows, test, feature = "fuzzing"))]
    pub(crate) const fn protection_profile(self) -> keld_guard::WindowsInstallProtectionProfile {
        match self {
            Self::PerUserDirect => keld_guard::WindowsInstallProtectionProfile::PerUserOwnerPrivate,
            Self::MachineUacDirect => keld_guard::WindowsInstallProtectionProfile::MachineUac,
            Self::MachineSeamlessDirect => {
                keld_guard::WindowsInstallProtectionProfile::MachineSystem
            }
        }
    }

    #[cfg(any(windows, test, feature = "fuzzing"))]
    pub(crate) fn parse(value: &str) -> Result<Self, UpdateError> {
        match value {
            "per-user-direct" => Ok(Self::PerUserDirect),
            "machine-uac-direct" => Ok(Self::MachineUacDirect),
            "machine-seamless-direct" => Ok(Self::MachineSeamlessDirect),
            _ => Err(UpdateError::LocalRecordInvalid {
                detail: "unsupported direct installation mode".to_owned(),
            }),
        }
    }
}

/// Security-principal model recorded by the installer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrincipalModel {
    /// Host and hostile application roles use distinct OS principals.
    StrictDistinctOsPrincipals,
    /// Host and application roles share the per-user OS principal.
    LegacySameUser,
    /// Package profile has not been independently admitted.
    Unverified,
}

impl PrincipalModel {
    fn as_str(self) -> &'static str {
        match self {
            Self::StrictDistinctOsPrincipals => "strict-distinct-os-principals",
            Self::LegacySameUser => "legacy-same-user",
            Self::Unverified => "unverified",
        }
    }
}

/// Exact direct-install facts expected by the running host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectInstallationIdentity {
    /// Explicit installer-selected Windows installation mode.
    pub install_mode: DirectInstallMode,
    /// Canonical application id compiled into the host.
    pub app_id: String,
    /// Channel requested by this host.
    pub channel: Channel,
    /// Target compiled into this host.
    pub target: String,
    /// Exact direct installation root.
    pub install_root: PathBuf,
    /// Exact protected update-state root.
    pub update_root: PathBuf,
    /// Identity of the compiled-in Ed25519 release key.
    pub signing_key_id: SigningKeyId,
    /// Immutable artifact installed as the direct package baseline.
    pub baseline: ArtifactIdentity,
    /// Strict-profile digest admitted for the installed package.
    pub profile_digest: ProfileDigest,
    /// Required OS-principal separation model.
    pub principal_model: PrincipalModel,
}

/// Owner recorded by the installer for this installation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallOwner {
    /// Keld's direct installer owns updates.
    Direct,
    /// A package manager or store owns updates.
    Managed {
        /// Human-readable stable mechanism name, such as `msix-store`.
        mechanism: String,
    },
}

/// Installer-created provenance record after a trusted platform loader reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallProvenance {
    /// Exact installation identity recorded by the installer.
    pub identity: DirectInstallationIdentity,
    /// Update-channel owner.
    pub owner: InstallOwner,
}

/// Result of loading provenance and its required version floor.
///
/// The platform/installer adapter owns whether `Protected` is true. Constructing a
/// test value is only state-machine evidence; it does not prove a real ACL, package
/// signature, or hostile-role denial.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProvenanceObservation {
    /// The installer commit record is absent.
    Missing,
    /// A record exists but the loader could not prove it protected.
    Unprotected {
        /// Untrusted record retained only for diagnostics.
        record: InstallProvenance,
    },
    /// The platform loader proved the record and floor came from protected state.
    Protected {
        /// Protected installer record.
        record: InstallProvenance,
        /// Protected semantic-version floor; `None` is a fail-closed corrupt state.
        version_floor: Option<String>,
    },
}

/// Host-owned verifier configuration before installation provenance is admitted.
#[derive(Debug, Clone)]
pub struct UpdateVerifier {
    pub(crate) expected: DirectInstallationIdentity,
    pub(crate) verifying_key: VerifyingKey,
}

impl UpdateVerifier {
    /// Creates a verifier from host-owned identity and compiled-in key bytes.
    ///
    /// # Errors
    ///
    /// Returns [`UpdateError::ManifestAuthentication`] for an invalid or weak
    /// Ed25519 key, or [`UpdateError::ProvenanceMismatch`] when the configured key
    /// identity or baseline is internally inconsistent.
    pub fn new(
        expected: DirectInstallationIdentity,
        public_key: [u8; 32],
    ) -> Result<Self, UpdateError> {
        validate_expected(&expected)?;
        let verifying_key = release_verifying_key(&public_key).map_err(|detail| {
            UpdateError::ManifestAuthentication {
                detail: format!("compiled-in {detail}"),
            }
        })?;
        let actual_key_id = SigningKeyId::from_public_key(&public_key);
        if expected.signing_key_id != actual_key_id {
            return Err(mismatch(
                ProvenanceField::SigningKey,
                &hex_digest(expected.signing_key_id.as_bytes()),
                &hex_digest(actual_key_id.as_bytes()),
            ));
        }
        Ok(Self {
            expected,
            verifying_key,
        })
    }

    /// Admits only an exact protected direct-install record and valid protected floor.
    ///
    /// # Errors
    ///
    /// Returns a typed refusal before a feed-verification capability is created when
    /// provenance is missing, unprotected, managed, legacy, mismatched, or paired with
    /// a missing/corrupt floor.
    pub fn admit(
        &self,
        observed: &ProvenanceObservation,
    ) -> Result<AdmittedInstallation, UpdateError> {
        let (record, floor_text) = match observed {
            ProvenanceObservation::Missing => {
                return Err(UpdateError::ProvenanceUnavailable {
                    reason: ProvenanceUnavailable::Missing,
                });
            }
            ProvenanceObservation::Unprotected { .. } => {
                return Err(UpdateError::ProvenanceUnavailable {
                    reason: ProvenanceUnavailable::Unprotected,
                });
            }
            ProvenanceObservation::Protected {
                record,
                version_floor,
            } => (record, version_floor.as_deref()),
        };

        if let InstallOwner::Managed { mechanism } = &record.owner {
            return Err(UpdateError::ManagedInstall {
                mechanism: mechanism.clone(),
            });
        }
        match_identity(&self.expected, &record.identity)?;

        let floor_text = floor_text.ok_or_else(|| UpdateError::VersionFloorInvalid {
            detail: "record is present but version-floor is missing".to_owned(),
        })?;
        let floor = validate_version_floor(&record.identity, floor_text)?;

        Ok(AdmittedInstallation {
            identity: self.expected.clone(),
            verifying_key: self.verifying_key,
            floor_text: floor_text.to_owned(),
            floor,
        })
    }
}

/// Opaque capability proving one protected direct installation was admitted.
#[derive(Debug, Clone)]
pub struct AdmittedInstallation {
    pub(crate) identity: DirectInstallationIdentity,
    pub(crate) verifying_key: VerifyingKey,
    pub(crate) floor_text: String,
    pub(crate) floor: Version,
}

impl AdmittedInstallation {
    /// Returns the exact direct-install identity bound to this capability.
    #[must_use]
    pub const fn identity(&self) -> &DirectInstallationIdentity {
        &self.identity
    }

    /// Returns the complete protected floor string used for selection.
    #[must_use]
    pub fn version_floor(&self) -> &str {
        &self.floor_text
    }
}

pub(crate) fn validate_version_floor(
    identity: &DirectInstallationIdentity,
    floor_text: &str,
) -> Result<Version, UpdateError> {
    let floor = Version::parse(floor_text).map_err(|error| UpdateError::VersionFloorInvalid {
        detail: format!("`{floor_text}` is not strict SemVer: {error}"),
    })?;
    let baseline = Version::parse(&identity.baseline.version).map_err(|error| {
        UpdateError::ProvenanceMismatch {
            field: ProvenanceField::Baseline,
            expected: "a strict-SemVer baseline".to_owned(),
            found: format!("{} ({error})", identity.baseline.version),
        }
    })?;
    match floor.cmp_precedence(&baseline) {
        Ordering::Less => {
            return Err(mismatch(
                ProvenanceField::VersionFloor,
                &format!("{} or a higher precedence", identity.baseline.version),
                floor_text,
            ));
        }
        Ordering::Equal if floor_text != identity.baseline.version => {
            return Err(mismatch(
                ProvenanceField::VersionFloor,
                &identity.baseline.version,
                floor_text,
            ));
        }
        Ordering::Equal | Ordering::Greater => {}
    }
    Ok(floor)
}

pub(crate) fn validate_expected(expected: &DirectInstallationIdentity) -> Result<(), UpdateError> {
    if expected.app_id.is_empty() || expected.app_id != expected.baseline.app_id {
        return Err(mismatch(
            ProvenanceField::AppId,
            &expected.app_id,
            &expected.baseline.app_id,
        ));
    }
    if expected.target.is_empty() || expected.target != expected.baseline.target {
        return Err(mismatch(
            ProvenanceField::Target,
            &expected.target,
            &expected.baseline.target,
        ));
    }
    if expected.channel != expected.baseline.channel {
        return Err(mismatch(
            ProvenanceField::Channel,
            expected.channel.as_str(),
            expected.baseline.channel.as_str(),
        ));
    }
    if expected.install_root.as_os_str().is_empty() {
        return Err(mismatch(
            ProvenanceField::InstallRoot,
            "a non-empty exact install root",
            "",
        ));
    }
    if expected.update_root.as_os_str().is_empty() {
        return Err(mismatch(
            ProvenanceField::UpdateRoot,
            "a non-empty exact update root",
            "",
        ));
    }
    if expected.principal_model != PrincipalModel::StrictDistinctOsPrincipals {
        return Err(mismatch(
            ProvenanceField::PrincipalModel,
            PrincipalModel::StrictDistinctOsPrincipals.as_str(),
            expected.principal_model.as_str(),
        ));
    }
    Version::parse(&expected.baseline.version).map_err(|error| {
        UpdateError::ProvenanceMismatch {
            field: ProvenanceField::Baseline,
            expected: "a strict-SemVer baseline".to_owned(),
            found: format!("{} ({error})", expected.baseline.version),
        }
    })?;
    Ok(())
}

/// The single admission rule for a release Ed25519 public key: it must decode and must
/// not be weak. The error is a detail that each caller wraps in its own typed refusal.
fn release_verifying_key(public_key: &[u8; 32]) -> Result<VerifyingKey, String> {
    let key = VerifyingKey::from_bytes(public_key)
        .map_err(|error| format!("Ed25519 public key is invalid: {error}"))?;
    if key.is_weak() {
        return Err("Ed25519 public key is weak".to_owned());
    }
    Ok(key)
}

/// Build-time expected app identity that a signed host carries (KEL-254 A3 §4).
///
/// It is decoded only from keld-pack's canonical payload; runtime code never
/// hand-writes it. It anchors executable-located selection together with the located
/// roots' file identities and the recorded mode's protection profile. Windows only:
/// A3 changes Windows boot alone.
#[cfg(any(windows, test))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpectedAppIdentity {
    pub(crate) app_id: String,
    pub(crate) channel: Channel,
    pub(crate) target: String,
    pub(crate) signing_key_id: SigningKeyId,
}

#[cfg(any(windows, test))]
impl ExpectedAppIdentity {
    /// Decodes keld-pack payload bytes (not a whole executable image).
    ///
    /// # Errors
    ///
    /// Returns [`UpdateError::ExpectedIdentityInvalid`] when keld-pack refuses the
    /// payload bytes, the channel is not a supported channel, or the update-signing
    /// public key is not a valid, non-weak Ed25519 key.
    pub fn decode(payload: &[u8]) -> Result<Self, UpdateError> {
        let payload = keld_pack::ExpectedAppIdentityPayload::decode(payload).map_err(|error| {
            let detail = match error {
                keld_pack::PackError::ExpectedIdentityInvalid { detail } => detail,
                _ => "payload",
            };
            invalid_expected(format!("payload {detail}"))
        })?;
        let channel = Channel::parse(payload.channel())
            .ok_or_else(|| invalid_expected("unsupported channel".to_owned()))?;
        release_verifying_key(payload.update_public_key()).map_err(invalid_expected)?;
        Ok(Self {
            app_id: payload.app_id().to_owned(),
            channel,
            target: payload.target().to_owned(),
            signing_key_id: SigningKeyId::from_public_key(payload.update_public_key()),
        })
    }

    /// Requires a protected record to carry exactly this app id, channel, target and
    /// signing key. Roots and mode are anchored by the located roots' file identity and
    /// the recorded mode's protection profile; baseline and profile digest are accepted
    /// from the protected record only after those anchors match (KEL-254 A3 §4).
    pub(crate) fn require_matches(
        &self,
        record: &DirectInstallationIdentity,
    ) -> Result<(), UpdateError> {
        compare(ProvenanceField::AppId, &self.app_id, &record.app_id)?;
        compare(
            ProvenanceField::Channel,
            self.channel.as_str(),
            record.channel.as_str(),
        )?;
        compare(ProvenanceField::Target, &self.target, &record.target)?;
        compare(
            ProvenanceField::SigningKey,
            &hex_digest(self.signing_key_id.as_bytes()),
            &hex_digest(record.signing_key_id.as_bytes()),
        )
    }
}

#[cfg(any(windows, test))]
fn invalid_expected(detail: String) -> UpdateError {
    UpdateError::ExpectedIdentityInvalid { detail }
}

pub(crate) fn match_identity(
    expected: &DirectInstallationIdentity,
    observed: &DirectInstallationIdentity,
) -> Result<(), UpdateError> {
    compare(
        ProvenanceField::InstallMode,
        expected.install_mode.as_str(),
        observed.install_mode.as_str(),
    )?;
    compare(ProvenanceField::AppId, &expected.app_id, &observed.app_id)?;
    compare(
        ProvenanceField::Channel,
        expected.channel.as_str(),
        observed.channel.as_str(),
    )?;
    compare(ProvenanceField::Target, &expected.target, &observed.target)?;
    compare_path(
        ProvenanceField::InstallRoot,
        &expected.install_root,
        &observed.install_root,
    )?;
    compare_path(
        ProvenanceField::UpdateRoot,
        &expected.update_root,
        &observed.update_root,
    )?;
    compare(
        ProvenanceField::SigningKey,
        &hex_digest(expected.signing_key_id.as_bytes()),
        &hex_digest(observed.signing_key_id.as_bytes()),
    )?;
    if expected.baseline != observed.baseline {
        return Err(mismatch(
            ProvenanceField::Baseline,
            &artifact_summary(&expected.baseline),
            &artifact_summary(&observed.baseline),
        ));
    }
    compare(
        ProvenanceField::Profile,
        &hex_digest(&expected.profile_digest.0),
        &hex_digest(&observed.profile_digest.0),
    )?;
    compare(
        ProvenanceField::PrincipalModel,
        expected.principal_model.as_str(),
        observed.principal_model.as_str(),
    )
}

fn compare(field: ProvenanceField, expected: &str, observed: &str) -> Result<(), UpdateError> {
    if expected == observed {
        Ok(())
    } else {
        Err(mismatch(field, expected, observed))
    }
}

fn compare_path(
    field: ProvenanceField,
    expected: &Path,
    observed: &Path,
) -> Result<(), UpdateError> {
    if expected == observed {
        Ok(())
    } else {
        Err(mismatch(
            field,
            &expected.display().to_string(),
            &observed.display().to_string(),
        ))
    }
}

fn mismatch(field: ProvenanceField, expected: &str, found: &str) -> UpdateError {
    UpdateError::ProvenanceMismatch {
        field,
        expected: expected.to_owned(),
        found: found.to_owned(),
    }
}

fn artifact_summary(identity: &ArtifactIdentity) -> String {
    format!(
        "{}/{}/{}/{}#{}",
        identity.app_id,
        identity.channel.as_str(),
        identity.target,
        identity.version,
        hex_digest(&identity.content_blake3)
    )
}

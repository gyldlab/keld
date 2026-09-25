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
    expected: DirectInstallationIdentity,
    verifying_key: VerifyingKey,
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
        let verifying_key = VerifyingKey::from_bytes(&public_key).map_err(|error| {
            UpdateError::ManifestAuthentication {
                detail: format!("compiled-in Ed25519 public key is invalid: {error}"),
            }
        })?;
        if verifying_key.is_weak() {
            return Err(UpdateError::ManifestAuthentication {
                detail: "compiled-in Ed25519 public key is weak".to_owned(),
            });
        }
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
        let floor =
            Version::parse(floor_text).map_err(|error| UpdateError::VersionFloorInvalid {
                detail: format!("`{floor_text}` is not strict SemVer: {error}"),
            })?;
        let baseline = Version::parse(&record.identity.baseline.version).map_err(|error| {
            UpdateError::ProvenanceMismatch {
                field: ProvenanceField::Baseline,
                expected: "a strict-SemVer baseline".to_owned(),
                found: format!("{} ({error})", record.identity.baseline.version),
            }
        })?;
        match floor.cmp_precedence(&baseline) {
            Ordering::Less => {
                return Err(mismatch(
                    ProvenanceField::VersionFloor,
                    &format!(
                        "{} or a higher precedence",
                        record.identity.baseline.version
                    ),
                    floor_text,
                ));
            }
            Ordering::Equal if floor_text != record.identity.baseline.version => {
                return Err(mismatch(
                    ProvenanceField::VersionFloor,
                    &record.identity.baseline.version,
                    floor_text,
                ));
            }
            Ordering::Equal | Ordering::Greater => {}
        }

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

fn validate_expected(expected: &DirectInstallationIdentity) -> Result<(), UpdateError> {
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

fn match_identity(
    expected: &DirectInstallationIdentity,
    observed: &DirectInstallationIdentity,
) -> Result<(), UpdateError> {
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

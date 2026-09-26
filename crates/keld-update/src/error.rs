use std::fmt;

/// Why protected installation provenance was unavailable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProvenanceUnavailable {
    /// The installer commit record is absent.
    Missing,
    /// The platform loader could read the record but could not prove it protected.
    Unprotected,
}

impl ProvenanceUnavailable {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::Unprotected => "not OS-protected",
        }
    }
}

/// Provenance identity field that disagreed with the running host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProvenanceField {
    /// Canonical application id.
    AppId,
    /// Requested update channel.
    Channel,
    /// Compiled target triple.
    Target,
    /// Direct installation root.
    InstallRoot,
    /// Protected update-state root.
    UpdateRoot,
    /// Identity of the compiled-in signing key.
    SigningKey,
    /// Installer baseline artifact.
    Baseline,
    /// Strict-profile identity.
    Profile,
    /// Distinct-OS-principal model.
    PrincipalModel,
    /// Persisted semantic-version floor relative to the installer baseline.
    VersionFloor,
}

impl ProvenanceField {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::AppId => "app.id",
            Self::Channel => "channel",
            Self::Target => "target",
            Self::InstallRoot => "installRoot",
            Self::UpdateRoot => "updateRoot",
            Self::SigningKey => "signingKey",
            Self::Baseline => "baseline",
            Self::Profile => "securityProfile",
            Self::PrincipalModel => "principalModel",
            Self::VersionFloor => "versionFloor",
        }
    }
}

/// Signed-manifest identity field that disagreed with the admitted installation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManifestIdentityField {
    /// Canonical application id.
    AppId,
    /// Requested update channel.
    Channel,
    /// Compiled target triple.
    Target,
}

impl ManifestIdentityField {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::AppId => "app.id",
            Self::Channel => "channel",
            Self::Target => "target",
        }
    }
}

/// Byte domain whose declared size or digest failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactDomain {
    /// Downloaded zstd-compressed artifact bytes.
    Compressed,
    /// Decompressed canonical package bytes.
    Content,
}

impl ArtifactDomain {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Compressed => "compressed artifact",
            Self::Content => "decompressed content",
        }
    }
}

/// Typed updater refusal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateError {
    /// Protected provenance is absent or its protection could not be established.
    ProvenanceUnavailable {
        /// The unavailable state observed by the trusted platform loader.
        reason: ProvenanceUnavailable,
    },
    /// Another package/store mechanism owns this installation.
    ManagedInstall {
        /// Owning mechanism named by protected provenance.
        mechanism: String,
    },
    /// Protected provenance disagrees with the host or baseline contract.
    ProvenanceMismatch {
        /// Field that failed exact comparison.
        field: ProvenanceField,
        /// Expected value safe for diagnostics.
        expected: String,
        /// Observed value safe for diagnostics.
        found: String,
    },
    /// The compiled key or detached signature cannot authenticate the manifest bytes.
    ManifestAuthentication {
        /// Non-secret reason for the authentication refusal.
        detail: String,
    },
    /// The authenticated JSON is not one unambiguous v0 manifest.
    ManifestInvalid {
        /// Parser or closed-schema failure detail.
        detail: String,
    },
    /// The authenticated manifest is for another app, channel, or target.
    ManifestIdentityMismatch {
        /// Field that failed exact comparison.
        field: ManifestIdentityField,
        /// Admitted host value.
        expected: String,
        /// Signed manifest value.
        found: String,
    },
    /// The protected semantic-version floor is missing or malformed.
    VersionFloorInvalid {
        /// Failure detail without protected path contents.
        detail: String,
    },
    /// A compressed or decompressed byte count did not equal its signed bound.
    ArtifactSizeMismatch {
        /// Byte domain whose count failed.
        domain: ArtifactDomain,
        /// Signed exact count.
        expected: u64,
        /// Exact count or a lower-bound description when the stream was too long.
        observed: String,
    },
    /// Transport or canonical-content BLAKE3 did not match the signed digest.
    ArtifactDigestMismatch {
        /// Byte domain whose digest failed.
        domain: ArtifactDomain,
        /// Signed lowercase hexadecimal digest.
        expected: String,
        /// Computed lowercase hexadecimal digest.
        actual: String,
    },
    /// Stream seek/read/write or zstd decoding failed.
    ArtifactProcessing {
        /// Processing stage safe for diagnostics.
        stage: &'static str,
        /// Underlying failure detail.
        detail: String,
    },
    /// Decompressed package bytes do not match the canonical Windows v0 archive profile.
    ArchiveInvalid {
        /// Stable parser reason that does not include untrusted path bytes.
        detail: &'static str,
    },
    /// Windows root admission or unpublished extraction failed.
    Extraction {
        /// Diagnostic stage name after creation was attempted; not proof of ownership or existence.
        incomplete_stage: Option<String>,
        /// Failed boundary or I/O operation.
        step: &'static str,
        /// Underlying refusal, without archive member contents.
        detail: String,
    },
}

impl UpdateError {
    /// Stable `KELD-UPDATE-*` code for this refusal.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::ProvenanceUnavailable { .. } => "KELD-UPDATE-001",
            Self::ManagedInstall { .. } => "KELD-UPDATE-002",
            Self::ProvenanceMismatch { .. } => "KELD-UPDATE-003",
            Self::ManifestAuthentication { .. } => "KELD-UPDATE-004",
            Self::ManifestInvalid { .. } => "KELD-UPDATE-005",
            Self::ManifestIdentityMismatch { .. } => "KELD-UPDATE-006",
            Self::VersionFloorInvalid { .. } => "KELD-UPDATE-007",
            Self::ArtifactSizeMismatch { .. } => "KELD-UPDATE-008",
            Self::ArtifactDigestMismatch { .. } => "KELD-UPDATE-009",
            Self::ArtifactProcessing { .. } => "KELD-UPDATE-010",
            Self::ArchiveInvalid { .. } => "KELD-UPDATE-011",
            Self::Extraction { .. } => "KELD-UPDATE-012",
        }
    }
}

impl fmt::Display for UpdateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ProvenanceUnavailable { reason } => write!(
                f,
                "KELD-UPDATE-001: direct-update provenance is {}. Reinstall with a supported direct installer and restore its OS-protected commit record before checking for updates.",
                reason.as_str()
            ),
            Self::ManagedInstall { mechanism } => write!(
                f,
                "KELD-UPDATE-002: `{mechanism}` owns this installation. Use that package/store mechanism to update it; Keld will not mutate its files directly."
            ),
            Self::ProvenanceMismatch {
                field,
                expected,
                found,
            } => write!(
                f,
                "KELD-UPDATE-003: protected provenance `{}` mismatch: expected `{expected}`, found `{found}`. Stop before feed access and repair or reinstall the direct package.",
                field.as_str()
            ),
            Self::ManifestAuthentication { detail } => write!(
                f,
                "KELD-UPDATE-004: detached manifest authentication failed ({detail}). Do not parse or activate the feed; publish bytes signed by the compiled-in release key."
            ),
            Self::ManifestInvalid { detail } => write!(
                f,
                "KELD-UPDATE-005: authenticated update manifest is not valid v0 ({detail}). Publish one closed, duplicate-free v0 manifest with canonical fields."
            ),
            Self::ManifestIdentityMismatch {
                field,
                expected,
                found,
            } => write!(
                f,
                "KELD-UPDATE-006: signed manifest `{}` mismatch: expected `{expected}`, found `{found}`. Fetch the feed for the admitted app, channel, and target.",
                field.as_str()
            ),
            Self::VersionFloorInvalid { detail } => write!(
                f,
                "KELD-UPDATE-007: protected semantic-version floor is unavailable or invalid ({detail}). Restore the protected updater state from the installer/known-good record before polling."
            ),
            Self::ArtifactSizeMismatch {
                domain,
                expected,
                observed,
            } => write!(
                f,
                "KELD-UPDATE-008: {} length mismatch: expected exactly {expected} bytes, observed {observed}. Discard the candidate and fetch the signed full artifact again.",
                domain.as_str()
            ),
            Self::ArtifactDigestMismatch {
                domain,
                expected,
                actual,
            } => write!(
                f,
                "KELD-UPDATE-009: {} BLAKE3 mismatch: expected `{expected}`, computed `{actual}`. Discard the candidate and fetch the signed full artifact again.",
                domain.as_str()
            ),
            Self::ArtifactProcessing { stage, detail } => write!(
                f,
                "KELD-UPDATE-010: full-artifact {stage} failed ({detail}). Preserve the current installation, repair the stream or staging sink, and retry."
            ),
            Self::ArchiveInvalid { detail } => write!(
                f,
                "KELD-UPDATE-011: verified full-package bytes are not a canonical Windows v0 archive ({detail}). Discard the candidate and publish a canonical package signed by the release key."
            ),
            Self::Extraction {
                incomplete_stage,
                step,
                detail,
            } => {
                write!(
                    f,
                    "KELD-UPDATE-012: Windows extraction {step} failed ({detail}). "
                )?;
                if let Some(name) = incomplete_stage {
                    write!(
                        f,
                        "Preserve any incomplete stage at `{name}` for diagnosis; it is not a runnable version. "
                    )?;
                }
                f.write_str("Keep the current installation and repair the protected staging root or artifact before retrying.")
            }
        }
    }
}

impl std::error::Error for UpdateError {}

pub(crate) fn hex_digest(digest: &[u8; 32]) -> String {
    let mut output = String::with_capacity(64);
    for byte in digest {
        let _ = fmt::Write::write_fmt(&mut output, format_args!("{byte:02x}"));
    }
    output
}

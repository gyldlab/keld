//! keld-update — signed v0 update verification.
//!
//! [`UpdateVerifier`] admits an installer-observed protected direct installation before
//! exposing [`AdmittedInstallation::verify_manifest`]. The admitted capability verifies
//! literal manifest bytes with the compiled-in Ed25519 key, validates the complete v0
//! shape, applies the protected semantic-version floor, and selects only [`SelectedFull`].
//! [`SelectedFull::verify_full`] then enforces both signed byte counts and BLAKE3 domains
//! while streaming zstd content.
//!
//! Native Windows archive preflight also checks canonical metadata, namespace and exact
//! no-migration policy. Windows extraction binds verified bytes to real protected,
//! fixed-NTFS staging handles and returns only an unpublished incomplete stage.
//! On Windows, a separate one-shot SYSTEM baseline initializer publishes protected
//! installer provenance last. Its read-only loader retains protected identity/floor
//! handles. Under the exclusive writer lease, one common journaled transaction
//! activates a published version, durably binds an exact attempt health receipt before
//! it commits, or rolls back, and resumes from every persisted cut. Production admission of that
//! transaction is currently `PerUserDirect` only; candidate launch, health observation,
//! process-family evidence and current-image signer verification remain host-owned. A
//! launched candidate locates its installation from its own image and immutable
//! provenance before its connect-back claim, and once its owner accepted it reads its
//! boot selection and health-receipt digest without the writer lease (KEL-53 criterion
//! 20's one reader exception).
//! A [`ProvenanceObservation::Protected`] test value
//! proves policy logic only; it is not real package-signature or ACL evidence. The full
//! lifecycle contract remains in `docs/architecture/06-runtime-and-tooling.md` §4 and
//! `docs/specs/kel53-full-package-activation.md`.

#[cfg(any(windows, test))]
mod activation;
#[cfg(any(windows, test, feature = "fuzzing"))]
mod archive;
mod baseline;
mod error;
mod full;
mod manifest;
mod provenance;
#[cfg(any(windows, test, feature = "fuzzing"))]
mod records;
#[cfg(windows)]
mod windows_baseline;
#[cfg(windows)]
mod windows_extraction;
#[cfg(windows)]
mod windows_fs;

#[cfg(test)]
mod tests;

#[cfg(any(windows, test, feature = "fuzzing"))]
pub use archive::{ArchiveEntry, ArchiveEntryKind, ValidatedArchive};
pub use baseline::{BaselineVerifier, SelectedBaseline, VerifiedBaseline};
pub use error::{
    ActivationEffect, ArtifactDomain, MachineRecoveryGuidance, ManifestIdentityField,
    ProvenanceField, ProvenanceUnavailable, UpdateError, VersionPublicationOutcome,
    WindowsLocatedImage,
};
pub use full::VerifiedFull;
#[cfg(feature = "fuzzing")]
#[doc(hidden)]
pub use full::fuzz_canonical_archive;
pub use manifest::{ManifestDecision, SelectedFull};
#[cfg(any(windows, test))]
pub use provenance::ExpectedAppIdentity;
pub use provenance::{
    AdmittedInstallation, ArtifactIdentity, DirectInstallMode, DirectInstallationIdentity,
    InstallOwner, InstallProvenance, PrincipalModel, ProvenanceObservation, SigningKeyId,
    UpdateVerifier,
};
#[cfg(feature = "fuzzing")]
#[doc(hidden)]
pub use records::fuzz_activation_journal;
#[cfg(windows)]
pub use records::{ActivationFailureClass, AttemptOwner, InitiatingLogon};
#[cfg(windows)]
pub use windows_baseline::{
    ActivationHealthReceipt, ActivePackageSelection, LoadedWindowsBaseline,
    ProcessFamilyRetirement, UpdaterHelperAnchor, UpdaterHelperImage, UpdaterHelperRole,
    WindowsActivationAttempt, WindowsActivationOutcome, WindowsActivationResolution,
    WindowsActivationWriteSnapshot, WindowsBaselineReceipt, WindowsBaselineTrust,
    WindowsCandidateBoot, WindowsCandidateClaimant, WindowsHealthAcceptedAttempt,
    WindowsJournaledAttempt, WindowsMintedAttempt, WindowsRecoveryInspection,
    WindowsRecoveryOutcome, anchor_updater_helper, initialize_windows_baseline,
    initialize_windows_machine_uac_baseline, initialize_windows_per_user_baseline,
    load_windows_activation_write_snapshot, load_windows_baseline,
    load_windows_recovery_inspection, locate_candidate_claimant,
    repair_windows_unjournaled_versions, select_active_package_for_executable,
    select_windows_active_package,
};
#[cfg(windows)]
pub use windows_extraction::{CompletedWindowsStage, ExtractedWindowsStage, WindowsExtractionRoot};

/// Release channels supported by update feeds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Channel {
    /// Production releases.
    #[default]
    Stable,
    /// Pre-release testing.
    Beta,
    /// Continuous builds.
    Canary,
}

impl Channel {
    /// Parses an exact v0 wire spelling; any other text is not a channel.
    #[cfg(any(windows, test, feature = "fuzzing"))]
    pub(crate) fn parse(text: &str) -> Option<Self> {
        match text {
            "stable" => Some(Self::Stable),
            "beta" => Some(Self::Beta),
            "canary" => Some(Self::Canary),
            _ => None,
        }
    }

    /// Stable v0 wire spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Beta => "beta",
            Self::Canary => "canary",
        }
    }
}

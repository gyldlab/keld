//! keld-update — signed v0 update verification.
//!
//! [`UpdateVerifier`] admits an installer-observed protected direct installation before
//! exposing [`AdmittedInstallation::verify_manifest`]. The admitted capability verifies
//! literal manifest bytes with the compiled-in Ed25519 key, validates the complete v0
//! shape, applies the protected semantic-version floor, and selects only [`SelectedFull`].
//! [`SelectedFull::verify_full`] then enforces both signed byte counts and BLAKE3 domains
//! while streaming zstd content.
//!
//! Archive validation/extraction, activation, health, and OS protection production are
//! deliberately outside this T2 slice. A [`ProvenanceObservation::Protected`] test value
//! proves policy logic only; it is not real package-signature or ACL evidence. The full
//! lifecycle contract remains in `docs/architecture/06-runtime-and-tooling.md` §4 and
//! `docs/specs/kel53-full-package-activation.md`.

mod archive;
mod error;
mod full;
mod manifest;
mod provenance;

#[cfg(test)]
mod tests;

pub use archive::{ArchiveEntry, ArchiveEntryKind, ValidatedArchive};
pub use error::{
    ArtifactDomain, ManifestIdentityField, ProvenanceField, ProvenanceUnavailable, UpdateError,
};
pub use full::VerifiedFull;
#[cfg(feature = "fuzzing")]
#[doc(hidden)]
pub use full::fuzz_canonical_archive;
pub use manifest::{ManifestDecision, SelectedFull};
pub use provenance::{
    AdmittedInstallation, ArtifactIdentity, DirectInstallationIdentity, InstallOwner,
    InstallProvenance, PrincipalModel, ProvenanceObservation, SigningKeyId, UpdateVerifier,
};

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

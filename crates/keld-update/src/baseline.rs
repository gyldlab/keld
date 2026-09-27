use std::io::{Read, Seek, Write};

use crate::full::ContentIdentity;
use crate::manifest::{SignedFullArtifact, authenticate_manifest, select_baseline};
use crate::{ArtifactIdentity, DirectInstallationIdentity, UpdateError, UpdateVerifier};

/// Trusted installer configuration for authenticating an exact initial artifact.
///
/// This verifier creates no protected-provenance observation or update admission.
/// Configuration must come from the trusted installer, not a feed or untrusted caller.
#[derive(Debug, Clone)]
pub struct BaselineVerifier {
    configuration: UpdateVerifier,
}

impl BaselineVerifier {
    /// Binds the exact baseline identity to the installer's trusted release key.
    ///
    /// # Errors
    /// Refuses inconsistent identity, a non-strict principal model or an invalid key.
    pub fn new(
        expected: DirectInstallationIdentity,
        public_key: [u8; 32],
    ) -> Result<Self, UpdateError> {
        Ok(Self {
            configuration: UpdateVerifier::new(expected, public_key)?,
        })
    }

    /// Authenticates the entire v0 manifest and selects the exact configured baseline.
    ///
    /// All releases are validated before selection. The complete version string and
    /// content digest must match; a newer release cannot substitute for the baseline.
    /// No semantic-version floor is invented, read or lowered by this operation.
    ///
    /// # Errors
    /// Refuses invalid authentication/schema/identity or an absent/mismatched baseline.
    pub fn verify_manifest(
        &self,
        manifest_bytes: &[u8],
        signature_bytes: &[u8],
    ) -> Result<SelectedBaseline, UpdateError> {
        let installation = &self.configuration.expected;
        let releases = authenticate_manifest(
            installation,
            &self.configuration.verifying_key,
            manifest_bytes,
            signature_bytes,
        )?;
        Ok(SelectedBaseline {
            installation: installation.clone(),
            artifact: select_baseline(installation, releases)?,
        })
    }
}

/// Opaque signed full-artifact metadata for one exact installer baseline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectedBaseline {
    installation: DirectInstallationIdentity,
    artifact: SignedFullArtifact,
}

impl SelectedBaseline {
    /// Exact configured and signed baseline identity.
    #[must_use]
    pub const fn identity(&self) -> &ArtifactIdentity {
        &self.artifact.identity
    }

    /// Signed full-package location.
    #[must_use]
    pub fn url(&self) -> &str {
        &self.artifact.url
    }

    /// Exact signed transport byte count.
    #[must_use]
    pub const fn compressed_size(&self) -> u64 {
        self.artifact.compressed_size
    }

    /// Exact signed decompressed byte count.
    #[must_use]
    pub const fn content_size(&self) -> u64 {
        self.artifact.content_size
    }

    /// Verifies both signed byte domains using the updater's shared streaming decoder.
    ///
    /// The caller must discard output after any error. Success authenticates bytes;
    /// archive preflight and native protection remain separate requirements.
    ///
    /// # Errors
    /// Refuses count/digest mismatch, source changes and I/O or decompression failure.
    pub fn verify_full<R: Read + Seek, W: Write>(
        &self,
        compressed: &mut R,
        output: &mut W,
    ) -> Result<VerifiedBaseline, UpdateError> {
        Ok(VerifiedBaseline {
            installation: self.installation.clone(),
            content: self.artifact.verify_content(compressed, output)?,
        })
    }
}

/// Opaque authentication receipt for baseline bytes, without update or OS authority.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedBaseline {
    installation: DirectInstallationIdentity,
    content: ContentIdentity,
}

impl VerifiedBaseline {
    /// Exact baseline whose transport and content bytes passed verification.
    #[must_use]
    pub const fn identity(&self) -> &ArtifactIdentity {
        &self.content.identity
    }

    /// Exact authenticated canonical-content byte count.
    #[must_use]
    pub const fn content_size(&self) -> u64 {
        self.content.content_size
    }

    /// Authenticated canonical-content digest.
    #[must_use]
    pub const fn content_blake3(&self) -> &[u8; 32] {
        &self.content.content_blake3
    }

    #[cfg(windows)]
    pub(crate) const fn installation(&self) -> &DirectInstallationIdentity {
        &self.installation
    }

    /// Performs full canonical Windows archive and exact no-migration policy preflight.
    ///
    /// # Errors
    /// Refuses changed bytes, invalid archives, names, policy, or read/seek failures.
    #[cfg(windows)]
    pub fn validate_windows_archive<R: Read + Seek>(
        &self,
        archive: &mut R,
    ) -> Result<crate::ValidatedArchive, UpdateError> {
        crate::archive::parse_content_archive(
            &self.content,
            archive,
            keld_guard::validate_windows_package_paths,
        )
    }
}

#[cfg(test)]
mod tests;

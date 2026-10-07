//! The connect-back claimant's two reads (KEL-53 §4 "Candidate connect-back", *Claim*
//! and *After acceptance*; criterion 20; KEL-270 T4d slice S6b).
//!
//! A launched candidate `keld-host.exe` first learns its install mode and installation
//! ID from its own image and immutable provenance alone, through the executable-located
//! anchor, before its claim. Only after the attempt owner accepted that claim does it
//! read the mutable records, without the writer lease that the owner holds: criterion
//! 20's authenticated candidate-boot read, the one reader exception. Every refusal of
//! that read is the typed [`ActivationEffect::WriterActive`]: the candidate starts no
//! application code.

use std::path::Path;

use super::ActivePackageSelection;
use super::load::{AcceptedClaim, read_candidate_boot};
use super::locate::{LocatedInstallation, locate};
use crate::{
    ActivationEffect, AttemptOwner, DirectInstallMode, ExpectedAppIdentity, UpdateError,
    WindowsLocatedImage,
};

/// A launched candidate's installation, located from its own image before its claim.
///
/// Only [`locate_candidate_claimant`] constructs it. It holds the located directories, the
/// running image and the immutable `install-provenance` record open without delete
/// sharing, so none can be renamed or replaced while it lives. It holds no lease and has
/// read no mutable record.
#[derive(Debug)]
pub struct WindowsCandidateClaimant {
    installation: LocatedInstallation,
    installation_id: [u8; 32],
}

/// Reads the connect-back claimant's provenance before its claim (KEL-53 §4 "Candidate
/// connect-back", *Claim*).
///
/// `executable` is the open handle whose image the single `keld-guard` Authenticode owner
/// verified, and `publisher_scope` and `app_id` are what that verification proved;
/// `locator` is the image's canonical path and conveys no authority. The candidate is
/// located as `keld-host.exe` by the executable-located rule of
/// [`super::select_active_package_for_executable`]: its record is matched against
/// `expected` and admitted against the recorded mode's protection profile, it must name
/// the verified publisher scope and app id, and every root it names must be the located
/// root.
///
/// It takes no snapshot lease and reads no journal, pointer or floor, so it succeeds while
/// the attempt owner holds the share-zero writer lease. The installation ID it derives is
/// the lifecycle installation ID of that immutable record, which the claimant sends in
/// `KELD-AH1`.
///
/// # Errors
/// As the executable-located selection types them, before any mutable record is read:
/// [`UpdateError::ExecutableBinding`], [`UpdateError::ProvenanceMismatch`],
/// [`UpdateError::ManagedInstall`] and [`UpdateError::Baseline`] for the located layout,
/// the record and the recorded roots, and [`UpdateError::RecordedSignerMismatch`] when
/// the record names another publisher or app.
pub fn locate_candidate_claimant(
    locator: &Path,
    executable: &std::fs::File,
    expected: &ExpectedAppIdentity,
    publisher_scope: &[u8; 32],
    app_id: &str,
) -> Result<WindowsCandidateClaimant, UpdateError> {
    let installation = locate(WindowsLocatedImage::Host, locator, executable, expected)?;
    installation
        .trust
        .require_verified_signer(publisher_scope, app_id)?;
    installation.require_recorded_roots()?;
    let installation_id = installation.trust.lifecycle_installation_id()?;
    Ok(WindowsCandidateClaimant {
        installation,
        installation_id,
    })
}

impl WindowsCandidateClaimant {
    /// The install mode that the immutable provenance record names.
    #[must_use]
    pub const fn install_mode(&self) -> DirectInstallMode {
        self.installation.trust.installation.install_mode
    }

    /// The lifecycle installation ID of the immutable provenance record: the value of
    /// `KELD-AH1`'s installation field.
    #[must_use]
    pub const fn lifecycle_installation_id(&self) -> &[u8; 32] {
        &self.installation_id
    }

    /// Criterion 20's authenticated candidate-boot read, after the attempt owner accepted
    /// this claimant (KEL-53 §4 "Candidate connect-back", *After acceptance*).
    ///
    /// `attempt_id` and `health_channel_id` are the values that the owner's `KELD-AC1`
    /// offered and the claim matched against the rendezvous name. `owner` is the process
    /// ID and creation time of the endpoint's server process, and `owner_image` that
    /// process's executable image, all as the claimant observed them on that process. In
    /// `PerUserDirect`, the only mode this release admits, the claimant runs as its
    /// owner's user and can open a live owner, so all three owner facts are required;
    /// `MachineUacDirect` claimants, which may bind on the process ID alone, arrive with
    /// KEL-53 T4d S11.
    ///
    /// It takes no lease and writes nothing. It reads the mutable records once, closing
    /// each before it returns, and requires an `AwaitingHealth` journal whose attempt and
    /// health-channel IDs equal the offered ones, whose v2 owner equals `owner`, whose
    /// `helper_image_blake3` is the BLAKE3 of `owner_image`'s bytes, and whose candidate is
    /// the version tree holding the running executable; then the exact `current`, floor
    /// and known-good state of that attempt, the version census, and the candidate's
    /// completion record and package policy. The result pins only the candidate's
    /// immutable version and tree with the protected ancestry.
    ///
    /// # Errors
    /// Every refusal is [`UpdateError::Activation`] with
    /// [`ActivationEffect::WriterActive`] and the failing step: a mode other than
    /// `PerUserDirect`, no journal, another phase, attempt, health channel or owner, a v1
    /// journal, another owner image, a candidate in another version tree, and any
    /// failure to read, decode or admit the records, the roots or the candidate's
    /// version. The candidate then starts no application code.
    pub fn read_candidate_boot(
        self,
        attempt_id: &[u8; 32],
        health_channel_id: &[u8; 32],
        owner: AttemptOwner,
        owner_image: &std::fs::File,
    ) -> Result<WindowsCandidateBoot, UpdateError> {
        let Self { installation, .. } = self;
        let result = (|| {
            let mode = installation.trust.installation.install_mode;
            if mode != DirectInstallMode::PerUserDirect {
                return Err(super::error(
                    "candidate install mode",
                    format!(
                        "the installation records `{mode:?}`, and only PerUserDirect admits a candidate-boot read"
                    ),
                ));
            }
            let roots = installation.require_recorded_roots()?;
            let claim = AcceptedClaim {
                attempt_id,
                health_channel_id,
                owner,
                owner_image,
                located_version: installation.located().version,
            };
            let (selection, health_receipt_digest) = read_candidate_boot(roots, &claim)?;
            installation.require_selected_image(&selection)?;
            Ok(WindowsCandidateBoot {
                selection,
                health_receipt_digest,
            })
        })();
        // The located pins and the provenance record are held until the read is proven.
        drop(installation);
        result.map_err(|cause| {
            let (step, detail) = cause.step_and_detail();
            UpdateError::activation(step, ActivationEffect::WriterActive, detail)
        })
    }
}

#[cfg(test)]
impl WindowsCandidateClaimant {
    /// Seam for the mode gate's negative control: this claimant with its recorded mode
    /// replaced. A real record in another mode refuses earlier, on its protection profile.
    pub(super) fn with_install_mode(mut self, mode: DirectInstallMode) -> Self {
        self.installation.trust.installation.install_mode = mode;
        self
    }
}

/// The accepted candidate's boot selection and its health-receipt digest: the result of
/// [`WindowsCandidateClaimant::read_candidate_boot`].
#[derive(Debug)]
pub struct WindowsCandidateBoot {
    selection: ActivePackageSelection,
    health_receipt_digest: [u8; 32],
}

impl WindowsCandidateBoot {
    /// The health-receipt digest that the candidate sends as the last field of
    /// `KELD-AB1` (KEL-53 §4 *Health records*): the §4 receipt digest over the attempt ID,
    /// the health-channel ID and the candidate artifact identity that this read matched
    /// to the running executable's version tree. Only this accessor and the attempt
    /// owner's expose the digest; its derivation stays private to keld-update.
    #[must_use]
    pub const fn health_receipt_digest(&self) -> &[u8; 32] {
        &self.health_receipt_digest
    }

    /// The attempt's candidate, selected from its own pinned tree.
    #[must_use]
    pub const fn selection(&self) -> &ActivePackageSelection {
        &self.selection
    }

    /// The candidate's selection, which keeps its immutable version and tree pinned.
    #[must_use]
    pub fn into_selection(self) -> ActivePackageSelection {
        self.selection
    }
}

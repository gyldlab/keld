//! The updater helper's self-anchor and the activation role's helper image (KEL-53
//! "Helper launch and self-anchor", criterion 17; KEL-270 T4d slice S9a).
//!
//! `keld-updater-helper.exe` locates its own installation through the same
//! executable-located rule as the host, with its own image name, and refuses every role
//! before the writer lease or any write unless the recorded mode is `MachineUacDirect`,
//! its verified signer is the recorded publisher and app, and its own image is the
//! expected one. The host derives the activation role's helper only from its own
//! selection: the selected tree's file, never a path it was given or searched for.

use std::path::{Path, PathBuf};

use super::locate::{LocatedInstallation, locate};
use super::{ActivePackageSelection, WindowsBaselineTrust, load};
use crate::{DirectInstallMode, ExpectedAppIdentity, UpdateError, WindowsLocatedImage};

/// The role `keld-updater-helper.exe` was started for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdaterHelperRole {
    /// Activation of a verified candidate, which the host starts from its own selected
    /// version tree.
    Activation,
    /// The recovery-only role.
    Recovery,
}

/// Proof that the running updater helper anchored itself to its installation.
///
/// Only [`anchor_updater_helper`] constructs it. It holds no lease and no handle: every
/// role takes the exclusive writer lease itself and revalidates the protected state.
#[derive(Debug)]
pub struct UpdaterHelperAnchor {
    trust: WindowsBaselineTrust,
}

impl UpdaterHelperAnchor {
    /// The anchored installation: the claims of the protected record that the helper's
    /// embedded expectation, the located roots' file identities and the recorded mode's
    /// protection profile admitted.
    #[must_use]
    pub const fn trust(&self) -> &WindowsBaselineTrust {
        &self.trust
    }
}

/// Anchors the running `keld-updater-helper.exe` to the installation that holds it,
/// before any lease or write (KEL-53 "Helper launch and self-anchor").
///
/// `executable` is the open handle whose image the single `keld-guard` Authenticode owner
/// verified, and `publisher_scope` and `app_id` are what that verification proved;
/// `locator` is the image's canonical path and conveys no authority. In order:
///
/// 1. the helper's own `ExpectedAppIdentity` is read from `executable` with
///    [`ExpectedAppIdentity::from_signed_image`], before any installation is located;
/// 2. the installation is located from the helper's own image by the executable-located
///    rule that [`super::select_active_package_for_executable`] applies, with
///    `keld-updater-helper.exe` in place of `keld-host.exe`, and its record is admitted
///    against that expectation and the recorded mode's protection profile;
/// 3. the recorded mode must be `MachineUacDirect`;
/// 4. the record must name the verified publisher scope and app id;
/// 5. every root the record names must be the located root; and
/// 6. under the shared snapshot lease, the helper's own image must be the expected one:
///    with an activation journal, the BLAKE3 of the verified handle's bytes must equal the
///    journaled `helper_image_blake3`; without one, the located version must be the
///    version that `current` names for [`UpdaterHelperRole::Activation`] and the
///    last-known-good version for [`UpdaterHelperRole::Recovery`].
///
/// It reads but never writes, and it holds nothing when it returns.
///
/// # Errors
/// [`UpdateError::ExpectedIdentityContainer`] or [`UpdateError::ExpectedIdentityInvalid`]
/// for a missing, duplicated or invalid embedded payload;
/// [`UpdateError::ExecutableBinding`], [`UpdateError::ProvenanceMismatch`],
/// [`UpdateError::ManagedInstall`] and [`UpdateError::Baseline`] as the executable-located
/// selection types them; [`UpdateError::UpdaterHelper`] for any mode but
/// `MachineUacDirect`, a journaled image digest that differs, or a located version that
/// is not the role's version; [`UpdateError::RecordedSignerMismatch`] when the record
/// names another publisher or app; and [`crate::ActivationEffect::WriterActive`] when a
/// conflicting handle holds the activation lock.
pub fn anchor_updater_helper(
    role: UpdaterHelperRole,
    locator: &Path,
    executable: &std::fs::File,
    publisher_scope: &[u8; 32],
    app_id: &str,
) -> Result<UpdaterHelperAnchor, UpdateError> {
    let expected = ExpectedAppIdentity::from_signed_image(executable)?;
    let installation = locate(
        WindowsLocatedImage::UpdaterHelper,
        locator,
        executable,
        &expected,
    )?;
    require_machine_uac(installation.trust.installation.install_mode, "install mode")?;
    anchor_located(role, installation, executable, publisher_scope, app_id)
}

/// Steps 4 to 6 of [`anchor_updater_helper`], after the mode gate.
pub(super) fn anchor_located(
    role: UpdaterHelperRole,
    installation: LocatedInstallation,
    executable: &std::fs::File,
    publisher_scope: &[u8; 32],
    app_id: &str,
) -> Result<UpdaterHelperAnchor, UpdateError> {
    installation
        .trust
        .require_verified_signer(publisher_scope, app_id)?;
    installation.require_recorded_roots()?;
    let records = load::read_anchor_records(&installation.trust)?;
    let located = installation.located().version;
    match records.journaled_helper_image {
        Some(journaled) => {
            let actual = image_blake3(executable)?;
            if actual != journaled {
                return Err(refusal(
                    "journaled image",
                    format!(
                        "the helper image's BLAKE3 `{}` is not the journaled `{}`",
                        crate::error::hex_digest(&actual),
                        crate::error::hex_digest(&journaled)
                    ),
                ));
            }
        }
        None => match role {
            UpdaterHelperRole::Activation => {
                let current = records.current.map_err(|cause| {
                    refusal(
                        "activation version",
                        format!(
                            "current is invalid ({}), so no version is selected",
                            super::activate::refusal_detail(&cause)
                        ),
                    )
                })?;
                if current.version != located {
                    return Err(refusal(
                        "activation version",
                        format!(
                            "the helper is in version `{located}`, not the selected current version `{}`",
                            current.version
                        ),
                    ));
                }
            }
            UpdaterHelperRole::Recovery => {
                if records.last_known_good.version != located {
                    return Err(refusal(
                        "recovery version",
                        format!(
                            "the helper is in version `{located}`, not last-known-good `{}`",
                            records.last_known_good.version
                        ),
                    ));
                }
            }
        },
    }
    Ok(UpdaterHelperAnchor {
        trust: installation.trust,
    })
}

/// The activation role's updater helper, opened beneath the selected version tree.
///
/// Only [`ActivePackageSelection::open_activation_helper`] constructs it. Its handle
/// shares only reads, so the file cannot be written, renamed or deleted while this value
/// lives.
#[derive(Debug)]
pub struct UpdaterHelperImage {
    path: PathBuf,
    _image: cap_std::fs::File,
}

impl UpdaterHelperImage {
    /// The exact absolute path of the opened file, the selected version tree's
    /// `keld-updater-helper.exe`, derived only from the protected record and the
    /// selected version: the one path a launcher may start.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl ActivePackageSelection {
    /// Derives the activation role's updater helper from this selection (KEL-53 "Helper
    /// launch and self-anchor"): `keld-updater-helper.exe` beneath this selection's own
    /// pinned version tree, opened without following a reparse point and sharing only
    /// reads, and admitted as a regular file on the tree's volume under the recorded
    /// mode's protection profile. Nothing is searched for and no given path is used.
    ///
    /// # Errors
    /// [`UpdateError::UpdaterHelper`] when the installation's mode is not
    /// `MachineUacDirect`, before anything is opened, so no helper is offered; and when
    /// the tree's helper is absent, not a regular file, or not protected as the recorded
    /// mode requires.
    pub fn open_activation_helper(&self) -> Result<UpdaterHelperImage, UpdateError> {
        require_machine_uac(self.identity.install_mode, "install mode")?;
        self.open_tree_helper()
    }

    /// The derivation after the mode gate.
    pub(super) fn open_tree_helper(&self) -> Result<UpdaterHelperImage, UpdateError> {
        let name = WindowsLocatedImage::UpdaterHelper.file_name();
        let image = super::open_machine_file(
            &self.tree,
            name,
            self.identity.install_mode.protection_profile(),
        )
        .map_err(|cause| refusal("activation image", cause))?;
        Ok(UpdaterHelperImage {
            path: self.tree_root.join(name),
            _image: image,
        })
    }
}

/// The one rule that only a `MachineUacDirect` installation runs the updater helper,
/// shared by the helper's self-anchor and the host's derivation of its image.
pub(super) fn require_machine_uac(
    mode: DirectInstallMode,
    step: &'static str,
) -> Result<(), UpdateError> {
    if mode == DirectInstallMode::MachineUacDirect {
        Ok(())
    } else {
        Err(refusal(
            step,
            format!(
                "the installation records `{mode:?}`, and only `MachineUacDirect` runs keld-updater-helper.exe"
            ),
        ))
    }
}

/// BLAKE3 of every byte of the verified image, read with positioned reads through the
/// verified handle and never by path. The handle shares no write access, so its length
/// is fixed while it is held.
fn image_blake3(image: &std::fs::File) -> Result<[u8; 32], UpdateError> {
    use std::os::windows::fs::FileExt as _;

    let length = image
        .metadata()
        .map_err(|cause| refusal("image digest", cause))?
        .len();
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0_u8; 16 * 1024];
    let mut offset = 0_u64;
    loop {
        let read = image
            .seek_read(&mut buffer, offset)
            .map_err(|cause| refusal("image digest", cause))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        offset = offset
            .checked_add(read as u64)
            .ok_or_else(|| refusal("image digest", "image length overflows"))?;
    }
    if offset != length {
        return Err(refusal(
            "image digest",
            format!("read {offset} bytes of a {length}-byte image"),
        ));
    }
    Ok(*hasher.finalize().as_bytes())
}

fn refusal(step: &'static str, detail: impl std::fmt::Display) -> UpdateError {
    UpdateError::UpdaterHelper {
        step,
        detail: detail.to_string(),
    }
}

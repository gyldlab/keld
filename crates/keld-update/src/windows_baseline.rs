//! One-shot machine baseline initialization and read-only provenance ownership.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Read, Write};

use cap_fs_ext::{DirExt as _, FollowSymlinks, MetadataExt as _, OpenOptionsFollowExt as _};
use cap_std::fs::{Dir, File, File as CapFile, OpenOptions, OpenOptionsExt as _};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_FLAG_BACKUP_SEMANTICS, FILE_GENERIC_READ, FILE_GENERIC_WRITE, FILE_SHARE_READ,
    FILE_SHARE_WRITE, READ_CONTROL, WRITE_DAC,
};

use crate::windows_extraction::{ensure_directory, ensure_regular, open_ancestors};
use crate::windows_fs::{
    create_file_relative_with_profile, publish_new, qualified_volume_root,
    require_volume_root_handle, validate_volume_locator,
};
use crate::{AdmittedInstallation, ArtifactIdentity};
use crate::{
    DirectInstallMode, DirectInstallationIdentity, InstallOwner, ProvenanceObservation, UpdateError,
};

mod activate;
mod initialize;
mod load;

#[cfg(test)]
pub(crate) use activate::CRASH_CUT_HOOK;
pub use activate::{
    ActivationHealthReceipt, ProcessFamilyRetirement, WindowsActivationAttempt,
    WindowsActivationOutcome, WindowsActivationResolution, WindowsRecoveryOutcome,
};
pub use initialize::{
    initialize_windows_baseline, initialize_windows_machine_uac_baseline,
    initialize_windows_per_user_baseline,
};
pub use load::{
    load_windows_activation_write_snapshot, load_windows_baseline, load_windows_recovery_inspection,
};

/// Trusted deployment/host inputs, independent of the record being authenticated.
///
/// These values must come from trusted packaging/host configuration. An installer
/// must never accept lower-trust command-line, environment or feed assertions here.
#[derive(Debug, Clone)]
pub struct WindowsBaselineTrust {
    /// Complete identity expected by the host and authenticated baseline verifier.
    pub installation: DirectInstallationIdentity,
    /// Trusted package/update owner observed by the deployment adapter before any feed access.
    ///
    /// Derive this only from trusted installer or package-manager state, never from a
    /// feed, command line, environment variable or inferred install path. Managed owners
    /// fail before Keld reads or mutates a direct-update installation tree.
    pub owner: InstallOwner,
    /// Installer-asserted publisher digest, not independent Authenticode evidence.
    pub publisher_scope: [u8; 32],
    /// Expected canonical volume-GUID root, such as `\\?\Volume{GUID}\`.
    pub volume_guid: String,
}

impl WindowsBaselineTrust {
    pub(crate) fn require_direct_owner(&self) -> Result<(), UpdateError> {
        match &self.owner {
            InstallOwner::Direct => Ok(()),
            InstallOwner::Managed { mechanism } => Err(UpdateError::ManagedInstall {
                mechanism: mechanism.clone(),
            }),
        }
    }

    /// Derives the lifecycle installation ID expected from this trusted install choice.
    ///
    /// The ID is a domain-separated digest of canonical v2 protected provenance, including
    /// install mode, app/channel/target, both roots, signing key, baseline, profile, principal
    /// model, owner, publisher scope and volume. The host supplies this value to lifecycle
    /// authentication; recovery inspection independently re-derives it from protected state.
    /// It is stable for the same provenance and changes on relocation or provenance change.
    ///
    /// # Errors
    /// Refuses managed ownership or invalid/noncanonical trusted provenance inputs.
    pub fn lifecycle_installation_id(&self) -> Result<[u8; 32], UpdateError> {
        self.require_direct_owner()?;
        crate::records::lifecycle_installation_id(
            &crate::InstallProvenance {
                identity: self.installation.clone(),
                owner: self.owner.clone(),
            },
            &self.publisher_scope,
            &self.volume_guid,
        )
    }
}

/// Protected installed identity/floor with retained read handles and namespace pins.
///
/// This is not active-package selection, mutation authority or strict-role admission.
#[derive(Debug)]
pub struct LoadedWindowsBaseline {
    observation: ProvenanceObservation,
    floor: String,
    publisher_scope: [u8; 32],
    roots: Roots,
    _activation_lease: File,
    _records: Vec<File>,
    _baseline_version: Dir,
    _baseline_tree: Dir,
}

impl LoadedWindowsBaseline {
    /// Protected identity and floor for ordinary update admission.
    #[must_use]
    pub const fn observation(&self) -> &ProvenanceObservation {
        &self.observation
    }

    /// Trusted expected installation, checked against the protected record.
    #[must_use]
    pub fn identity(&self) -> &DirectInstallationIdentity {
        &self.roots.trust.installation
    }

    /// Installer-asserted publisher digest; boot must independently verify its signer.
    #[must_use]
    pub const fn publisher_scope(&self) -> &[u8; 32] {
        &self.publisher_scope
    }

    /// Exact protected semantic-version floor, without activation selection.
    #[must_use]
    pub fn version_floor(&self) -> &str {
        &self.floor
    }

    /// Consumes this real loader owner to retain a SYSTEM-only staging authority.
    ///
    /// The returned root derives its identity, floor and versions handle from these
    /// protected observations. It can produce only owner-private incomplete stages,
    /// and retains this owner's metadata and ancestry handles for its lifetime.
    ///
    /// # Errors
    /// Refuses a non-SYSTEM caller or changed machine versions protection. It never
    /// creates scaffolding, changes the floor or publishes an artifact.
    pub fn into_windows_extraction_root(self) -> Result<crate::WindowsExtractionRoot, UpdateError> {
        crate::windows_extraction::WindowsExtractionRoot::from_loaded(self)
    }

    pub(crate) fn retained_versions(&self) -> io::Result<Dir> {
        self.roots.versions.try_clone()
    }
}

/// Successful baseline initialization after production reload and exact seed checks.
#[derive(Debug)]
pub struct WindowsBaselineReceipt {
    loaded: LoadedWindowsBaseline,
    _version: VersionPins,
}

/// Coherent protected activation snapshot held under the exclusive installation lease.
///
/// It exposes only validated state and an extraction root. Journal, floor and pointer
/// writes happen only through [`crate::WindowsExtractionRoot::begin_activation`] after
/// a complete version is published under this same lease. The production loader
/// currently admits only `PerUserDirect`; machine-UAC and machine-seamless authority
/// remain separate gates.
#[derive(Debug)]
pub struct WindowsActivationWriteSnapshot {
    roots: Roots,
    lease: File,
    admitted: AdmittedInstallation,
    version_floor: String,
    current: ArtifactIdentity,
    last_known_good: ArtifactIdentity,
    previous_known_good: Option<ArtifactIdentity>,
    version_pins: BTreeMap<String, VersionPins>,
}

/// Read-only protected recovery observations held under the exclusive installation lease.
///
/// This owner can inspect one canonical activation journal and its protected pointer/floor
/// context. It exposes no active-package selection or extraction root. Its only mutation
/// paths are [`Self::recover`], which requires an exact process-family retirement binding
/// for the inspected journal before the common transaction may write, and
/// [`Self::resume_unlaunched`], which admits only a never-launched `PublishPending` attempt.
#[derive(Debug)]
pub struct WindowsRecoveryInspection {
    roots: Roots,
    lease: File,
    admitted: AdmittedInstallation,
    lifecycle_installation_id: [u8; 32],
    journal: crate::records::ActivationJournal,
    version_floor: String,
    current: ArtifactIdentity,
    last_known_good: ArtifactIdentity,
    previous_known_good: Option<ArtifactIdentity>,
    version_pins: BTreeMap<String, VersionPins>,
}

impl WindowsActivationWriteSnapshot {
    /// Creates a non-writable, non-inheritable reference to this exact
    /// share-zero activation lease for the bounded lifecycle keeper.
    ///
    /// The returned handle retains only `FILE_READ_ATTRIBUTES | SYNCHRONIZE`;
    /// it cannot mutate the lock file or updater records. The transaction owner
    /// retains all write authority.
    ///
    /// # Errors
    ///
    /// Returns an error if the lease handle is inheritable or Windows cannot
    /// create/read back the reduced-rights duplicate.
    #[cfg(windows)]
    pub fn duplicate_lifecycle_lease_retention(
        &self,
    ) -> Result<std::os::windows::io::OwnedHandle, UpdateError> {
        duplicate_lifecycle_lease_retention(&self.lease)
    }

    /// Exact anti-downgrade floor observed under the exclusive lease.
    #[must_use]
    pub fn version_floor(&self) -> &str {
        &self.version_floor
    }

    /// Current selected artifact observed under the exclusive lease.
    #[must_use]
    pub const fn current(&self) -> &ArtifactIdentity {
        &self.current
    }

    /// Health-confirmed last-known-good artifact observed under the exclusive lease.
    #[must_use]
    pub const fn last_known_good(&self) -> &ArtifactIdentity {
        &self.last_known_good
    }

    /// Older health-confirmed rollback artifact, if present.
    #[must_use]
    pub const fn previous_known_good(&self) -> Option<&ArtifactIdentity> {
        self.previous_known_good.as_ref()
    }

    /// Opens the unpublished staging root while retaining this exact writer lease.
    ///
    /// # Errors
    /// Refuses a profile/root mismatch or a direct mode without production admission.
    pub fn open_extraction_root(self) -> Result<crate::WindowsExtractionRoot, UpdateError> {
        crate::windows_extraction::WindowsExtractionRoot::from_activation_write_snapshot(self)
    }

    pub(crate) const fn identity(&self) -> &DirectInstallationIdentity {
        &self.admitted.identity
    }

    pub(crate) fn floor(&self) -> &semver::Version {
        &self.admitted.floor
    }

    pub(crate) fn retained_versions(&self) -> io::Result<Dir> {
        self.roots.versions.try_clone()
    }

    pub(crate) const fn profile(&self) -> keld_guard::WindowsInstallProtectionProfile {
        self.roots.profile()
    }

    pub(crate) fn verify_published_version(
        &self,
        expected: &ArtifactIdentity,
        content_size: u64,
    ) -> Result<(), UpdateError> {
        load::verify_completed_version(&self.roots, expected, content_size)
    }
}

impl WindowsRecoveryInspection {
    /// Lifecycle installation ID independently re-derived from admitted protected provenance.
    #[must_use]
    pub const fn lifecycle_installation_id(&self) -> &[u8; 32] {
        &self.lifecycle_installation_id
    }

    /// Trusted installation identity checked against protected provenance.
    #[must_use]
    pub const fn identity(&self) -> &DirectInstallationIdentity {
        &self.admitted.identity
    }

    /// Attempt ID read from the protected activation journal under the writer lease.
    #[must_use]
    pub const fn attempt_id(&self) -> &[u8; 32] {
        &self.journal.attempt_id
    }

    /// Lifecycle channel ID read from the protected activation journal under the writer lease.
    #[must_use]
    pub const fn lifecycle_channel_id(&self) -> &[u8; 32] {
        &self.journal.lifecycle_channel_id
    }

    /// Protected semantic-version floor observed with the journal.
    #[must_use]
    pub fn version_floor(&self) -> &str {
        &self.version_floor
    }

    /// Current pointer observed with the journal.
    #[must_use]
    pub const fn current(&self) -> &ArtifactIdentity {
        &self.current
    }

    /// Last-known-good pointer observed with the journal.
    #[must_use]
    pub const fn last_known_good(&self) -> &ArtifactIdentity {
        &self.last_known_good
    }

    /// Previous-known-good pointer observed with the journal, if present.
    #[must_use]
    pub const fn previous_known_good(&self) -> Option<&ArtifactIdentity> {
        self.previous_known_good.as_ref()
    }

    /// Duplicates this exact inspection's share-zero lease with only keeper-retention rights.
    ///
    /// # Errors
    ///
    /// Returns an error if Windows cannot duplicate and attenuate the retained lease handle.
    #[cfg(windows)]
    pub fn duplicate_lifecycle_lease_retention(
        &self,
    ) -> Result<std::os::windows::io::OwnedHandle, UpdateError> {
        duplicate_lifecycle_lease_retention(&self.lease)
    }
}

#[cfg(windows)]
fn duplicate_lifecycle_lease_retention(
    lease: &File,
) -> Result<std::os::windows::io::OwnedHandle, UpdateError> {
    let lease = lease
        .try_clone()
        .map_err(|cause| error("activation lease keeper clone", cause))?
        .into_std();
    crate::windows_fs::duplicate_activation_lease_for_keeper(&lease)
        .map_err(|cause| error("activation lease keeper retention", cause))
}

impl WindowsBaselineReceipt {
    /// Read-only protected provenance retained by this initialization receipt.
    #[must_use]
    pub const fn loaded(&self) -> &LoadedWindowsBaseline {
        &self.loaded
    }
}

#[derive(Debug)]
struct Roots {
    trust: WindowsBaselineTrust,
    ancestors: Vec<Dir>,
    update: Dir,
    versions: Dir,
}

impl Roots {
    fn install(&self) -> io::Result<&Dir> {
        self.ancestors
            .last()
            .ok_or_else(|| io::Error::other("install root absent"))
    }

    const fn profile(&self) -> keld_guard::WindowsInstallProtectionProfile {
        self.trust.installation.install_mode.protection_profile()
    }
}

#[derive(Debug)]
struct VersionPins {
    _directories: Vec<Dir>,
    files: Vec<File>,
}

fn error(step: &'static str, detail: impl std::fmt::Display) -> UpdateError {
    UpdateError::Baseline {
        step,
        detail: detail.to_string(),
    }
}

fn open_roots(trust: &WindowsBaselineTrust, private: bool) -> io::Result<Roots> {
    let identity = &trust.installation;
    if identity.target != "windows-x64" {
        return Err(io::Error::other("baseline requires windows-x64"));
    }
    if private
        && !matches!(
            identity.install_mode,
            DirectInstallMode::PerUserDirect
                | DirectInstallMode::MachineSeamlessDirect
                | DirectInstallMode::MachineUacDirect
        )
    {
        return Err(io::Error::other(
            "baseline initializer does not admit this installation mode",
        ));
    }
    crate::records::path_text(&identity.install_root).map_err(io::Error::other)?;
    crate::records::path_text(&identity.update_root).map_err(io::Error::other)?;
    validate_volume_locator(&trust.volume_guid)?;
    if identity.update_root.parent() != Some(identity.install_root.as_path()) {
        return Err(io::Error::other(
            "update root must be one direct child of install root",
        ));
    }
    let update_name = identity
        .update_root
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| io::Error::other("update component absent"))?;
    if update_name.eq_ignore_ascii_case("install-provenance") {
        return Err(io::Error::other(
            "update component conflicts with provenance",
        ));
    }
    let ancestors = open_ancestors(&identity.install_root)?;
    let last = ancestors
        .len()
        .checked_sub(1)
        .ok_or_else(|| io::Error::other("install root absent"))?;
    let volume = ancestors[0].dir_metadata()?.dev();
    for (index, directory) in ancestors.iter().enumerate() {
        if directory.dir_metadata()?.dev() != volume {
            return Err(io::Error::other("ancestor crosses the expected volume"));
        }
        let file = directory.try_clone()?.into_std_file();
        if index == 0 {
            require_volume_root_handle(&file)?;
            keld_guard::validate_windows_machine_volume_anchor(&file)?;
        } else if index == last {
            keld_guard::validate_windows_install_directory(
                &file,
                identity.install_mode.protection_profile(),
            )?;
        } else {
            match identity.install_mode {
                DirectInstallMode::PerUserDirect => ensure_directory(&directory.dir_metadata()?)?,
                DirectInstallMode::MachineUacDirect => {
                    keld_guard::validate_windows_machine_ancestor_directory(&file)?;
                }
                DirectInstallMode::MachineSeamlessDirect => {
                    keld_guard::validate_windows_machine_directory(&file)?;
                }
            }
        }
        let actual = qualified_volume_root(&file)?;
        if !actual.eq_ignore_ascii_case(&trust.volume_guid) {
            return Err(io::Error::other(
                "actual volume differs from trusted expected volume",
            ));
        }
    }
    let update = ancestors[last].open_dir_nofollow(update_name)?;
    let versions = update.open_dir_nofollow("versions")?;
    for directory in [&update, &versions] {
        ensure_directory(&directory.dir_metadata()?)?;
        if directory.dir_metadata()?.dev() != volume {
            return Err(io::Error::other(
                "baseline scaffold crosses the expected volume",
            ));
        }
        let file = directory.try_clone()?.into_std_file();
        keld_guard::validate_windows_install_directory(
            &file,
            identity.install_mode.protection_profile(),
        )?;
    }
    Ok(Roots {
        trust: trust.clone(),
        ancestors,
        update,
        versions,
    })
}

pub(crate) fn exact_entries(directory: &Dir, expected: &[&str]) -> io::Result<()> {
    let mut names = BTreeSet::new();
    for entry in directory.entries()? {
        let name = entry?
            .file_name()
            .into_string()
            .map_err(|_| io::Error::other("non-UTF-8 directory entry"))?;
        names.insert(name);
    }
    let wanted: BTreeSet<String> = expected.iter().map(|name| (*name).to_owned()).collect();
    if names != wanted {
        return Err(io::Error::other(format!(
            "unexpected state: wanted {wanted:?}, found {names:?}"
        )));
    }
    Ok(())
}

fn open_machine_file(
    parent: &Dir,
    leaf: &str,
    profile: keld_guard::WindowsInstallProtectionProfile,
) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .follow(FollowSymlinks::No);
    let file = parent.open_with(leaf, &options)?;
    ensure_regular(&file.metadata()?)?;
    if file.metadata()?.dev() != parent.dir_metadata()?.dev() {
        return Err(io::Error::other("record crosses parent volume"));
    }
    keld_guard::validate_windows_install_file(&file.try_clone()?.into_std(), profile)?;
    Ok(file)
}

/// Opens the persistent installation-wide activation lease without creating it.
///
/// Snapshot readers share only with other readers; the writer shares with nobody.
/// The handle itself, never the lock-file contents or its existence, represents the
/// live lease. The trusted initializer seeds the regular file before provenance commit.
fn open_activation_lease(
    update: &Dir,
    profile: keld_guard::WindowsInstallProtectionProfile,
    exclusive: bool,
) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true).follow(FollowSymlinks::No);
    if exclusive {
        options
            .write(true)
            .access_mode(FILE_GENERIC_READ | FILE_GENERIC_WRITE)
            .share_mode(0);
    } else {
        options.share_mode(FILE_SHARE_READ);
    }
    let file = update.open_with("activation.lock", &options)?;
    ensure_regular(&file.metadata()?)?;
    if file.metadata()?.len() != 0 {
        return Err(io::Error::other("activation lease file is not empty"));
    }
    if file.metadata()?.dev() != update.dir_metadata()?.dev() {
        return Err(io::Error::other("activation lease crosses parent volume"));
    }
    keld_guard::validate_windows_install_file(&file.try_clone()?.into_std(), profile)?;
    Ok(file)
}

fn read_record(
    parent: &Dir,
    leaf: &str,
    profile: keld_guard::WindowsInstallProtectionProfile,
) -> Result<(File, Vec<u8>), UpdateError> {
    let mut file = open_machine_file(parent, leaf, profile)
        .map_err(|cause| error("protected record open", cause))?;
    let length = file
        .metadata()
        .map_err(|cause| error("protected record size", cause))?
        .len();
    if length > crate::records::MAX_LOCAL_RECORD_BYTES as u64 {
        return Err(error("protected record size", "record exceeds 64 KiB"));
    }
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|cause| error("protected record read", cause))?;
    Ok((file, bytes))
}

/// Prepares one protected record sibling, then publishes it only at an absent target.
///
/// The same owner handles installer-baseline records and completed package markers;
/// the caller selects only the already-admitted directory profile and fixed leaf.
pub(crate) fn publish_new_record(
    parent: &Dir,
    leaf: &str,
    bytes: &[u8],
    profile: keld_guard::WindowsInstallProtectionProfile,
) -> Result<(), UpdateError> {
    let temporary = prepare_record(parent, bytes, profile)?;
    publish_prepared_record(
        parent,
        &temporary,
        RecordTarget::Absent(leaf),
        bytes,
        profile,
    )
}

/// Where a prepared protected record sibling is published.
#[derive(Debug, Clone, Copy)]
pub(crate) enum RecordTarget<'leaf> {
    /// A fixed leaf that must not exist yet.
    Absent(&'leaf str),
    /// One existing activation record slot, replaced under the writer lease.
    Replace(crate::windows_fs::RecordSlot),
}

fn prepare_record(
    parent: &Dir,
    bytes: &[u8],
    profile: keld_guard::WindowsInstallProtectionProfile,
) -> Result<String, UpdateError> {
    if bytes.len() > crate::records::MAX_LOCAL_RECORD_BYTES {
        return Err(error("record creation", "record exceeds 64 KiB"));
    }
    let temporary = random_leaf_name("pending").map_err(|cause| error("record identity", cause))?;
    let retained_parent = parent
        .try_clone()
        .map_err(|cause| error("record parent", cause))?
        .into_std_file();
    let mut output = CapFile::from_std(
        create_file_relative_with_profile(&retained_parent, &temporary, profile)
            .map_err(|cause| error("record creation", cause))?,
    );
    ensure_regular(
        &output
            .metadata()
            .map_err(|cause| error("record metadata", cause))?,
    )
    .map_err(|cause| error("record kind", cause))?;
    keld_guard::validate_windows_install_file(
        &output
            .try_clone()
            .map_err(|cause| error("record handle", cause))?
            .into_std(),
        profile,
    )
    .map_err(|cause| error("record protection", cause))?;
    output
        .write_all(bytes)
        .map_err(|cause| error("record write", cause))?;
    let output = output.into_std();
    output
        .sync_all()
        .map_err(|cause| error("record flush", cause))?;
    drop(output);
    let (_, observed) = read_record(parent, &temporary, profile)?;
    if observed != bytes {
        return Err(error("record readback", "flushed bytes differ"));
    }
    Ok(temporary)
}

fn publish_prepared_record(
    parent: &Dir,
    temporary: &str,
    target: RecordTarget<'_>,
    bytes: &[u8],
    profile: keld_guard::WindowsInstallProtectionProfile,
) -> Result<(), UpdateError> {
    let retained_parent = parent
        .try_clone()
        .map_err(|cause| error("record parent", cause))?
        .into_std_file();
    let leaf = match target {
        RecordTarget::Absent(leaf) => {
            publish_new(&retained_parent, temporary, leaf)
                .map_err(|cause| error("record publication", cause))?;
            leaf
        }
        RecordTarget::Replace(slot) => {
            crate::windows_fs::replace_record_slot(&retained_parent, temporary, slot)
                .map_err(|cause| error("record replacement", cause))?;
            slot.leaf()
        }
    };
    let (_, observed) = read_record(parent, leaf, profile)?;
    if observed != bytes {
        return Err(error("published record readback", "published bytes differ"));
    }
    Ok(())
}

pub(crate) fn random_leaf_name(prefix: &str) -> io::Result<String> {
    let mut random = [0_u8; 32];
    getrandom::fill(&mut random).map_err(io::Error::other)?;
    Ok(format!("{prefix}-{}", crate::error::hex_digest(&random)))
}

/// Whether `name` is exactly `<prefix>-<64 lowercase hex>`, as minted by [`random_leaf_name`].
pub(crate) fn is_generated_leaf(name: &str, prefix: &str) -> bool {
    name.strip_prefix(prefix)
        .and_then(|rest| rest.strip_prefix('-'))
        .is_some_and(|suffix| {
            suffix.len() == 64
                && suffix
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        })
}

fn seal_child(parent: &Dir, leaf: &str, directory: bool) -> io::Result<()> {
    let mut options = OpenOptions::new();
    options
        .access_mode(READ_CONTROL | WRITE_DAC)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .follow(FollowSymlinks::No);
    if directory {
        options.custom_flags(FILE_FLAG_BACKUP_SEMANTICS);
    }
    let object = parent.open_with(leaf, &options)?;
    if directory {
        ensure_directory(&object.metadata()?)?;
    } else {
        ensure_regular(&object.metadata()?)?;
    }
    let mut object = object.into_std();
    if directory {
        keld_guard::seal_windows_machine_directory(&mut object)
    } else {
        keld_guard::seal_windows_machine_file(&mut object)
    }
}

#[cfg(test)]
mod tests;

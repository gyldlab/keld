//! Executable-located active-package selection (KEL-254 A3 §4, task T2b) over real
//! per-user Windows state.
//!
//! Oracles are the selected version and tree, typed refusals naming the failing step,
//! and the exact bytes of every protected record (including `install-provenance`)
//! before and after each refusal: no negative case may write.

use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::os::windows::fs::OpenOptionsExt as _;
use std::path::{Path, PathBuf};

use windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ;

use super::selection::record_bytes;
use super::support::host_package_content;
use super::transaction::{begin_with, commit_with};
use super::writer::seed_per_user_baseline_with;
use crate::records::PointerKind;
use crate::windows_baseline::{WindowsBaselineTrust, select_active_package_for_executable};
use crate::{ActivationEffect, DirectInstallMode, ExpectedAppIdentity, UpdateError};

struct Install {
    _fixture: tempfile::TempDir,
    trust: WindowsBaselineTrust,
    content: Vec<u8>,
}

fn installed() -> Install {
    let fixture = tempfile::tempdir().expect("executable-located selection fixture");
    let content = host_package_content();
    let trust = seed_per_user_baseline_with(fixture.path(), &content);
    Install {
        _fixture: fixture,
        trust,
        content,
    }
}

pub(super) fn host_path(trust: &WindowsBaselineTrust, version: &str) -> PathBuf {
    trust
        .installation
        .update_root
        .join("versions")
        .join(version)
        .join("tree")
        .join("keld-host.exe")
}

/// Opens an image the way the verified-image owner does: read access, no write or
/// delete sharing, so the identity stays bound for the handle's lifetime.
pub(super) fn open_image(path: &Path) -> std::fs::File {
    OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .open(path)
        .expect("open the executable image")
}

pub(super) fn expected_for(trust: &WindowsBaselineTrust) -> ExpectedAppIdentity {
    expected_for_identity(&trust.installation)
}

/// The build-time expectation a host built for `identity` embeds.
pub(super) fn expected_for_identity(
    identity: &crate::DirectInstallationIdentity,
) -> ExpectedAppIdentity {
    let payload = keld_pack::ExpectedAppIdentityPayload::new(
        &identity.app_id,
        identity.channel.as_str(),
        &identity.target,
        crate::tests::signing_key().verifying_key().to_bytes(),
    )
    .expect("fixture identity fits the canonical payload");
    ExpectedAppIdentity::decode(&payload.encode()).expect("canonical expectation decodes")
}

/// Every protected record of the installation, including `install-provenance`.
pub(super) fn state(trust: &WindowsBaselineTrust) -> BTreeMap<String, Vec<u8>> {
    let mut records = record_bytes(trust);
    let provenance = trust.installation.install_root.join("install-provenance");
    records.insert(
        "install-provenance".to_owned(),
        std::fs::read(provenance).expect("provenance bytes"),
    );
    records
}

pub(super) fn select_from(
    trust: &WindowsBaselineTrust,
    version: &str,
) -> Result<crate::ActivePackageSelection, UpdateError> {
    let path = host_path(trust, version);
    let executable = open_image(&path);
    select_active_package_for_executable(&path, &executable, &expected_for(trust))
}

pub(super) fn refuses(
    trust: &WindowsBaselineTrust,
    locator: &Path,
    executable: &std::fs::File,
    expected: &ExpectedAppIdentity,
    why: &str,
) -> UpdateError {
    let before = state(trust);
    let error = select_active_package_for_executable(locator, executable, expected).expect_err(why);
    assert_eq!(state(trust), before, "{why}: the refusal writes nothing");
    error
}

pub(super) fn binding_step(error: &UpdateError) -> &'static str {
    match error {
        UpdateError::ExecutableBinding { step, .. } => step,
        other => panic!("expected KELD-UPDATE-018, got {other:?}"),
    }
}

pub(super) fn baseline_step(error: &UpdateError) -> &'static str {
    match error {
        UpdateError::Baseline { step, .. } => step,
        other => panic!("expected KELD-UPDATE-013, got {other:?}"),
    }
}

fn rewrite_provenance(trust: &WindowsBaselineTrust, edit: impl FnOnce(&mut WindowsBaselineTrust)) {
    let mut recorded = trust.clone();
    edit(&mut recorded);
    let bytes = crate::records::encode_provenance(
        &crate::InstallProvenance {
            identity: recorded.installation,
            owner: recorded.owner,
        },
        &recorded.publisher_scope,
        &recorded.volume_guid,
    )
    .expect("canonical substituted provenance");
    // Overwriting keeps the record's protected descriptor; only its claims change.
    std::fs::write(
        trust.installation.install_root.join("install-provenance"),
        bytes,
    )
    .expect("substitute the recorded claims");
}

#[test]
fn the_located_baseline_is_selected() {
    let install = installed();
    let selection = select_from(&install.trust, "1.0.0").expect("the located baseline selects");
    assert_eq!(selection.artifact().version, "1.0.0");
    assert_eq!(
        selection.tree_root(),
        host_path(&install.trust, "1.0.0")
            .parent()
            .expect("tree root")
    );
    assert_eq!(selection.install_identity(), &install.trust.installation);
    assert_eq!(selection.publisher_scope(), &install.trust.publisher_scope);
}

#[test]
fn a_committed_update_is_selected_from_its_own_tree() {
    let install = installed();
    commit_with(&install.trust, "2.0.0", &install.content);
    let before = state(&install.trust);
    let selection = select_from(&install.trust, "2.0.0").expect("the committed update selects");
    assert_eq!(selection.artifact().version, "2.0.0");
    assert_eq!(state(&install.trust), before, "selection writes nothing");
}

#[test]
fn a_retained_previous_version_host_refuses() {
    let install = installed();
    commit_with(&install.trust, "2.0.0", &install.content);
    let locator = host_path(&install.trust, "1.0.0");
    let executable = open_image(&locator);
    let error = refuses(
        &install.trust,
        &locator,
        &executable,
        &expected_for(&install.trust),
        "the previous-known-good host is not the selected version",
    );
    assert_eq!(binding_step(&error), "selected version");
    assert_eq!(error.code(), "KELD-UPDATE-018");
}

#[test]
fn an_invalid_current_is_repaired_only_for_the_last_known_good_host() {
    let install = installed();
    commit_with(&install.trust, "2.0.0", &install.content);
    let current = install.trust.installation.update_root.join("current");
    std::fs::write(&current, b"not a pointer").expect("invalidate current");

    let stale = host_path(&install.trust, "1.0.0");
    let executable = open_image(&stale);
    let error = refuses(
        &install.trust,
        &stale,
        &executable,
        &expected_for(&install.trust),
        "a non-last-known-good host must not repair current",
    );
    assert_eq!(binding_step(&error), "current pointer repair");
    assert_eq!(
        std::fs::read(&current).expect("current bytes"),
        b"not a pointer"
    );

    let selection =
        select_from(&install.trust, "2.0.0").expect("the last-known-good host repairs and selects");
    assert_eq!(selection.artifact().version, "2.0.0");
    let repaired = crate::records::decode_pointer(
        PointerKind::Current,
        &std::fs::read(&current).expect("repaired current"),
    )
    .expect("current is canonical again");
    assert_eq!(repaired.version, "2.0.0");
}

#[test]
fn locator_shape_refuses_before_any_installation_read() {
    let install = installed();
    let real = host_path(&install.trust, "1.0.0");
    let executable = open_image(&real);
    let expected = expected_for(&install.trust);
    let tree = real.parent().expect("tree");
    let version = tree.parent().expect("version");
    let versions = version.parent().expect("versions");
    let update = versions.parent().expect("update");
    let cases: Vec<(&str, PathBuf)> = vec![
        ("case-variant host", tree.join("KELD-HOST.EXE")),
        (
            "case-variant tree",
            version.join("Tree").join("keld-host.exe"),
        ),
        (
            "case-variant versions",
            update
                .join("Versions")
                .join("1.0.0")
                .join("tree")
                .join("keld-host.exe"),
        ),
        (
            "prefixed version",
            versions.join("v1.0.0").join("tree").join("keld-host.exe"),
        ),
        (
            "leading-zero version",
            versions.join("01.0.0").join("tree").join("keld-host.exe"),
        ),
        (
            "relative locator",
            PathBuf::from(r"updates\versions\1.0.0\tree\keld-host.exe"),
        ),
        (
            "UNC locator",
            PathBuf::from(r"\\server\share\app\updates\versions\1.0.0\tree\keld-host.exe"),
        ),
        (
            "volume-root install",
            PathBuf::from(r"C:\updates\versions\1.0.0\tree\keld-host.exe"),
        ),
    ];
    for (why, locator) in cases {
        let error = refuses(&install.trust, &locator, &executable, &expected, why);
        assert_eq!(binding_step(&error), "locator", "{why}: {error}");
    }
}

#[test]
fn an_extra_install_root_entry_refuses() {
    let install = installed();
    std::fs::write(install.trust.installation.install_root.join("stray"), b"x")
        .expect("plant a stray install-root entry");
    let locator = host_path(&install.trust, "1.0.0");
    let executable = open_image(&locator);
    let error = refuses(
        &install.trust,
        &locator,
        &executable,
        &expected_for(&install.trust),
        "the install root must hold exactly the update root and provenance",
    );
    assert_eq!(binding_step(&error), "located install root");
}

#[test]
fn a_byte_identical_copy_outside_the_tree_refuses() {
    let install = installed();
    let locator = host_path(&install.trust, "1.0.0");
    let copy = install
        .trust
        .installation
        .install_root
        .parent()
        .expect("fixture root")
        .join("copied-host.exe");
    std::fs::copy(&locator, &copy).expect("copy the host image");
    let executable = open_image(&copy);
    let error = refuses(
        &install.trust,
        &locator,
        &executable,
        &expected_for(&install.trust),
        "identical bytes are not the located file",
    );
    assert_eq!(binding_step(&error), "executable identity");
}

#[test]
fn a_record_naming_another_installation_refuses() {
    let install = installed();
    let other = installed();
    let other_record = std::fs::read(
        other
            .trust
            .installation
            .install_root
            .join("install-provenance"),
    )
    .expect("the other installation's record");
    std::fs::write(
        install
            .trust
            .installation
            .install_root
            .join("install-provenance"),
        other_record,
    )
    .expect("substitute a valid record that names another installation");
    // The named installation's last-known-good is the located version and its `current`
    // is invalid, so only the root-identity refusal stands between this record and a
    // repair write into the installation it names.
    std::fs::write(
        other.trust.installation.update_root.join("current"),
        b"not a pointer",
    )
    .expect("invalidate the named installation's current");
    let named_before = state(&other.trust);
    let locator = host_path(&install.trust, "1.0.0");
    let executable = open_image(&locator);
    let error = refuses(
        &install.trust,
        &locator,
        &executable,
        &expected_for(&install.trust),
        "a record must name the located roots",
    );
    assert_eq!(binding_step(&error), "install root identity");
    assert_eq!(
        state(&other.trust),
        named_before,
        "the installation the record names is never written"
    );
}

#[test]
fn a_record_on_another_volume_refuses() {
    let install = installed();
    rewrite_provenance(&install.trust, |recorded| {
        recorded.volume_guid = r"\\?\Volume{00000000-0000-0000-0000-000000000000}\".to_owned();
    });
    let locator = host_path(&install.trust, "1.0.0");
    let executable = open_image(&locator);
    let error = refuses(
        &install.trust,
        &locator,
        &executable,
        &expected_for(&install.trust),
        "the record's volume must be the executable's volume",
    );
    assert_eq!(binding_step(&error), "record volume");
}

#[test]
fn a_rewritten_install_mode_refuses_on_its_protection_profile() {
    let install = installed();
    rewrite_provenance(&install.trust, |recorded| {
        recorded.installation.install_mode = DirectInstallMode::MachineUacDirect;
    });
    let locator = host_path(&install.trust, "1.0.0");
    let executable = open_image(&locator);
    let error = refuses(
        &install.trust,
        &locator,
        &executable,
        &expected_for(&install.trust),
        "an owner-private record cannot claim the machine-UAC profile",
    );
    // A protection-profile refusal is typed as the installed-root selector types it.
    assert_eq!(baseline_step(&error), "protected record profile");
    assert_eq!(error.code(), "KELD-UPDATE-013");
}

#[test]
fn the_record_must_carry_the_expected_identity() {
    let install = installed();
    let locator = host_path(&install.trust, "1.0.0");
    let executable = open_image(&locator);
    let mut other = install.trust.clone();
    other.installation.app_id = "dev.keld.other".to_owned();
    let error = refuses(
        &install.trust,
        &locator,
        &executable,
        &expected_for(&other),
        "a different expected app id refuses",
    );
    assert!(
        matches!(
            error,
            UpdateError::ProvenanceMismatch {
                field: crate::error::ProvenanceField::AppId,
                ..
            }
        ),
        "{error:?}"
    );
}

#[test]
fn a_pending_journal_selects_nothing() {
    let install = installed();
    drop(begin_with(&install.trust, "2.0.0", &install.content));
    let locator = host_path(&install.trust, "2.0.0");
    let executable = open_image(&locator);
    match refuses(
        &install.trust,
        &locator,
        &executable,
        &expected_for(&install.trust),
        "a pending journal must not select a tree",
    ) {
        UpdateError::Activation { effect, .. } => {
            assert_eq!(effect, ActivationEffect::JournalBoundRecoveryRequired);
        }
        other => panic!("a pending journal must refuse as journal-bound: {other:?}"),
    }
}

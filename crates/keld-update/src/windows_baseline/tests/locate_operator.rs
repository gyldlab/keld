//! Executable-located selection over real machine installations (KEL-254 A3 §4, task
//! T2b slice 3): operator acceptance for the `MachineUac` and `MachineSystem` cells and
//! the non-NTFS executable-volume refusal.
//!
//! Each privileged seeding selector runs once in its own principal and writes its
//! completion marker last; the ordinary-user selectors then observe the located selection
//! or its typed refusal, and the exact bytes of every protected record before and after:
//! no selector here may write installation state.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use keld_guard::WindowsInstallProtectionProfile;
use windows_permissions::constants::AceType;
use windows_permissions::{LocalBox, SecurityDescriptor, Sid};

use super::locate::{
    baseline_step, binding_step, expected_for, expected_for_identity, host_path, open_image,
    refuses, select_from, state,
};
use super::machine_uac::{operator_root, operator_root_path};
use super::substitutions::set_dacl;
use super::support::{
    self, NON_NTFS_ROOT_ENV, baseline_with, host_package_content, trust_for_with,
};
use crate::windows_baseline::{
    WindowsBaselineTrust, initialize_windows_baseline, initialize_windows_machine_uac_baseline,
    select_active_package_for_executable,
};
use crate::{DirectInstallMode, UpdateError};

pub(super) const LABEL: &str = "application";
const WEAKENED: &str = "ancestor-weakened";
const SEEDED: &str = "kel254-located-seeded.txt";
/// The exact `MachineUac` directory profile plus one non-inheritable Users `FILE_ADD_FILE`
/// (0x2) grant. The ancestor rule admits `FILE_ADD_SUBDIRECTORY` (0x4), not 0x2.
const WEAKENED_UAC_ANCESTOR: &str =
    "O:S-1-5-32-544D:P(A;OICI;FA;;;BA)(A;OICI;FA;;;SY)(A;OICI;0x1200a9;;;BU)(A;;0x2;;;BU)";
/// The exact `MachineSystem` directory profile plus the same one extra Users grant.
const WEAKENED_SYSTEM_ANCESTOR: &str = "O:SYD:P(A;OICI;FA;;;SY)(A;OICI;0x1200a9;;;BU)(A;;0x2;;;BU)";

pub(super) fn machine_uac_trust(parent: &Path, content: &[u8]) -> WindowsBaselineTrust {
    let mut trust = trust_for_with(&parent.join(LABEL), content);
    trust.installation.install_mode = DirectInstallMode::MachineUacDirect;
    trust
}

/// The fixture root, once the privileged seeding selector that writes `marker` last has
/// finished.
pub(super) fn seeded(root: PathBuf, marker: &str) -> PathBuf {
    assert!(
        root.join(marker).is_file(),
        "run the privileged seeding selector that writes `{marker}` first: {}",
        root.display()
    );
    root
}

pub(super) fn leaf(root: &Path) -> &str {
    root.file_name()
        .and_then(|value| value.to_str())
        .expect("UTF-8 fixture leaf")
}

/// Every entry under `root`: directories map to `None`, files to their exact bytes.
pub(super) fn census(root: &Path) -> BTreeMap<PathBuf, Option<Vec<u8>>> {
    let mut entries = BTreeMap::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory).expect("census directory") {
            let path = entry.expect("census entry").path();
            if fs::symlink_metadata(&path)
                .expect("census entry kind")
                .is_dir()
            {
                pending.push(path.clone());
                entries.insert(path, None);
            } else {
                let bytes = fs::read(&path).expect("census bytes");
                entries.insert(path, Some(bytes));
            }
        }
    }
    entries
}

#[test]
fn weakened_ancestor_descriptors_add_one_users_write_grant_to_the_exact_profiles() {
    let users: LocalBox<Sid> = "S-1-5-32-545".parse().expect("BUILTIN\\Users SID");
    for (weakened, profile) in [
        (
            WEAKENED_UAC_ANCESTOR,
            WindowsInstallProtectionProfile::MachineUac,
        ),
        (
            WEAKENED_SYSTEM_ANCESTOR,
            WindowsInstallProtectionProfile::MachineSystem,
        ),
    ] {
        let exact = keld_guard::windows_install_directory_security(profile)
            .expect("guard-owned directory profile");
        let weakened: LocalBox<SecurityDescriptor> =
            weakened.parse().expect("weakened fixture SDDL");
        assert_eq!(weakened.owner(), exact.owner(), "{profile:?}");
        let exact_acl = exact.dacl().expect("profile DACL");
        let weakened_acl = weakened.dacl().expect("weakened DACL");
        assert_eq!(weakened_acl.len(), exact_acl.len() + 1, "{profile:?}");
        for index in 0..exact_acl.len() {
            let actual = weakened_acl.get_ace(index).expect("weakened ACE");
            let expected = exact_acl.get_ace(index).expect("profile ACE");
            assert!(
                actual.ace_type() == expected.ace_type()
                    && actual.flags() == expected.flags()
                    && actual.mask() == expected.mask()
                    && actual.sid() == expected.sid(),
                "{profile:?}: ACE {index} is the profile's own"
            );
        }
        let extra = weakened_acl
            .get_ace(exact_acl.len())
            .expect("the one extra ACE");
        assert_eq!(extra.ace_type(), AceType::ACCESS_ALLOWED_ACE_TYPE);
        assert!(extra.flags().is_empty(), "{profile:?}: not inheritable");
        assert_eq!(extra.mask().bits(), 0x2, "{profile:?}: FILE_ADD_FILE only");
        assert_eq!(extra.sid(), Some(&*users), "{profile:?}: granted to Users");
    }
}

#[test]
#[ignore = "operator acceptance: run this exact selector in an explicitly elevated administrator process before the ordinary-user located-selection selectors"]
fn machine_uac_elevated_installer_seeds_located_host_baselines() {
    let root = operator_root();
    let anchor = support::directory(Path::new(r"C:\"));
    keld_guard::validate_windows_machine_volume_anchor(&anchor)
        .expect("C: retains its protected volume-anchor ACL");
    let profile = WindowsInstallProtectionProfile::MachineUac;
    let created =
        crate::windows_fs::create_directory_relative_with_profile(&anchor, leaf(&root), profile)
            .expect("elevated owner-capable Administrators token assigns the UAC ancestor profile");
    drop(
        crate::windows_fs::create_directory_relative_with_profile(&created, WEAKENED, profile)
            .expect("the ancestor to weaken starts with the exact UAC profile"),
    );
    drop(created);
    let content = host_package_content();
    let source = root.join("host-baseline.tar");
    fs::write(&source, &content).expect("write the signed host baseline fixture");
    for parent in [root.clone(), root.join(WEAKENED)] {
        let trust = support::provision_machine_uac_as_administrator_with(&parent, LABEL, &content);
        drop(
            initialize_windows_machine_uac_baseline(
                &baseline_with(&trust, &content),
                &source,
                &trust,
            )
            .expect("explicit-UAC installer seeds the located host baseline"),
        );
    }
    let weakened = root.join(WEAKENED);
    set_dacl(&weakened, WEAKENED_UAC_ANCESTOR);
    keld_guard::validate_windows_machine_ancestor_directory(&support::directory(&root))
        .expect("the exact UAC ancestor stays admitted");
    assert!(
        keld_guard::validate_windows_machine_ancestor_directory(&support::directory(&weakened))
            .is_err(),
        "one Users FILE_ADD_FILE grant fails the machine ancestor rule"
    );
    fs::write(root.join(SEEDED), b"seeded").expect("seeding completion marker, written last");
    println!(
        "KELD_KEL254_MACHINE_UAC_LOCATED_SEED_PASS root={}",
        root.display()
    );
}

#[test]
#[ignore = "operator acceptance: run this exact selector from an ordinary unelevated user token after the elevated located-host seeding selector"]
fn machine_uac_ordinary_user_selects_the_located_baseline() {
    support::assert_ordinary_token();
    let root = seeded(operator_root_path(), SEEDED);
    let trust = machine_uac_trust(&root, &host_package_content());
    let before = state(&trust);
    let selection =
        select_from(&trust, "1.0.0").expect("an ordinary user selects the located UAC baseline");
    assert_eq!(selection.artifact().version, "1.0.0");
    assert_eq!(
        selection.tree_root(),
        host_path(&trust, "1.0.0").parent().expect("tree root")
    );
    assert_eq!(selection.install_identity(), &trust.installation);
    assert_eq!(
        selection.install_identity().install_mode,
        DirectInstallMode::MachineUacDirect
    );
    assert_eq!(selection.publisher_scope(), &trust.publisher_scope);
    drop(selection);
    assert_eq!(state(&trust), before, "selection writes nothing");
    println!("KELD_KEL254_MACHINE_UAC_LOCATED_SELECT_PASS version=1.0.0");
}

#[test]
#[ignore = "operator acceptance: run this exact selector from an ordinary unelevated user token after the elevated located-host seeding selector"]
fn machine_uac_ordinary_user_refuses_a_weakened_ancestor() {
    support::assert_ordinary_token();
    let root = seeded(operator_root_path(), SEEDED);
    let trust = machine_uac_trust(&root.join(WEAKENED), &host_package_content());
    let locator = host_path(&trust, "1.0.0");
    let executable = open_image(&locator);
    let error = refuses(
        &trust,
        &locator,
        &executable,
        &expected_for(&trust),
        "a weakened MachineUac ancestor refuses before selection",
    );
    assert_eq!(baseline_step(&error), "recorded roots", "{error}");
    assert_eq!(error.code(), "KELD-UPDATE-013");
    assert!(
        matches!(&error, UpdateError::Baseline { detail, .. } if detail.contains("grants mutation")),
        "{error}"
    );
    println!(
        "KELD_KEL254_MACHINE_UAC_WEAKENED_ANCESTOR_REFUSED step=recorded-roots code=KELD-UPDATE-013"
    );
}

#[test]
#[ignore = "operator acceptance: requires the reviewed operator helper running this exact selector as LocalSystem"]
fn machine_system_installer_seeds_located_host_baselines() {
    let root = support::create_suite();
    let content = host_package_content();
    let source = root.join("host-source.tar");
    fs::write(&source, &content).expect("independently pinned host package");
    drop(
        crate::windows_fs::create_directory_relative_with_profile(
            &support::directory(&root),
            WEAKENED,
            WindowsInstallProtectionProfile::MachineSystem,
        )
        .expect("the ancestor to weaken starts with the exact SYSTEM profile"),
    );
    for parent in [root.clone(), root.join(WEAKENED)] {
        let trust = support::provision_with(&parent, LABEL, &content);
        drop(
            initialize_windows_baseline(&baseline_with(&trust, &content), &source, &trust)
                .expect("SYSTEM installer seeds the located host baseline"),
        );
    }
    let weakened = root.join(WEAKENED);
    set_dacl(&weakened, WEAKENED_SYSTEM_ANCESTOR);
    keld_guard::validate_windows_machine_directory(&support::directory(&root))
        .expect("the exact SYSTEM ancestor stays admitted");
    assert!(
        keld_guard::validate_windows_machine_directory(&support::directory(&weakened)).is_err(),
        "one Users FILE_ADD_FILE grant fails the exact SYSTEM ancestor profile"
    );
    fs::write(root.join(SEEDED), b"seeded").expect("seeding completion marker, written last");
    println!(
        "KELD_KEL254_SYSTEM_LOCATED_SEED_PASS root={}",
        root.display()
    );
}

#[test]
#[ignore = "operator acceptance: run this exact selector from an ordinary unelevated user token after the LocalSystem located-host seeding selector"]
fn machine_system_ordinary_user_selects_the_located_baseline() {
    support::assert_ordinary_token();
    let root = seeded(support::suite_root(), SEEDED);
    let trust = trust_for_with(&root.join(LABEL), &host_package_content());
    let before = state(&trust);
    let selection =
        select_from(&trust, "1.0.0").expect("an ordinary user selects the located SYSTEM baseline");
    assert_eq!(selection.artifact().version, "1.0.0");
    assert_eq!(
        selection.tree_root(),
        host_path(&trust, "1.0.0").parent().expect("tree root")
    );
    assert_eq!(selection.install_identity(), &trust.installation);
    assert_eq!(
        selection.install_identity().install_mode,
        DirectInstallMode::MachineSeamlessDirect
    );
    assert_eq!(selection.publisher_scope(), &trust.publisher_scope);
    drop(selection);
    assert_eq!(state(&trust), before, "selection writes nothing");
    println!("KELD_KEL254_SYSTEM_LOCATED_SELECT_PASS version=1.0.0");
}

#[test]
#[ignore = "operator acceptance: run this exact selector from an ordinary unelevated user token after the LocalSystem located-host seeding selector"]
fn machine_system_ordinary_user_refuses_a_weakened_ancestor() {
    support::assert_ordinary_token();
    let root = seeded(support::suite_root(), SEEDED);
    let trust = trust_for_with(&root.join(WEAKENED).join(LABEL), &host_package_content());
    let locator = host_path(&trust, "1.0.0");
    let executable = open_image(&locator);
    let error = refuses(
        &trust,
        &locator,
        &executable,
        &expected_for(&trust),
        "a weakened MachineSystem ancestor refuses before selection",
    );
    assert_eq!(baseline_step(&error), "recorded roots", "{error}");
    assert_eq!(error.code(), "KELD-UPDATE-013");
    assert!(
        matches!(&error, UpdateError::Baseline { detail, .. } if detail.contains("ACE count differs")),
        "{error}"
    );
    println!(
        "KELD_KEL254_SYSTEM_WEAKENED_ANCESTOR_REFUSED step=recorded-roots code=KELD-UPDATE-013"
    );
}

#[test]
#[ignore = "operator acceptance: requires an operator-attached non-NTFS fixed volume and a fresh fixture path in KELD_KEL254_NON_NTFS_ROOT"]
fn an_executable_on_a_non_ntfs_volume_refuses_before_any_installation_read() {
    let root = PathBuf::from(
        std::env::var_os(NON_NTFS_ROOT_ENV)
            .expect("operator supplies a fresh fixture path on an attached non-NTFS volume"),
    );
    let volume = root
        .parent()
        .and_then(Path::to_str)
        .expect("UTF-8 volume root");
    assert!(
        volume.len() == 3
            && volume.as_bytes()[0].is_ascii_alphabetic()
            && volume.ends_with(r":\")
            && !volume.eq_ignore_ascii_case(r"C:\"),
        "fixture is a direct child of a non-system drive root: {volume}"
    );
    assert!(
        leaf(&root).starts_with("KeldT2bNonNtfs-") && leaf(&root).len() > 20,
        "unique non-NTFS fixture name"
    );
    assert!(
        !root.exists(),
        "never reuse or repair an existing non-NTFS fixture"
    );
    // A plain located layout without `install-provenance`: any read past the volume gate
    // refuses at the located install root, which is this selector's falsifier.
    let locator = root
        .join(LABEL)
        .join("updates")
        .join("versions")
        .join("1.0.0")
        .join("tree")
        .join("keld-host.exe");
    fs::create_dir_all(locator.parent().expect("tree")).expect("plain located layout");
    fs::write(&locator, b"keld-host fixture image").expect("plain located host");
    let executable = open_image(&locator);
    let before = census(&root);
    let error = select_active_package_for_executable(
        &locator,
        &executable,
        &expected_for_identity(&crate::tests::expected_identity()),
    )
    .expect_err("an executable on a non-NTFS volume refuses");
    assert_eq!(binding_step(&error), "executable volume", "{error}");
    assert_eq!(error.code(), "KELD-UPDATE-018");
    assert!(
        matches!(&error, UpdateError::ExecutableBinding { detail, .. } if detail.contains("NTFS")),
        "{error}"
    );
    assert_eq!(census(&root), before, "the refusal writes nothing");
    drop(executable);
    fs::remove_dir_all(&root).expect("remove the isolated non-NTFS fixture after assertions");
    println!("KELD_KEL254_NON_NTFS_EXECUTABLE_REFUSED step=executable-volume code=KELD-UPDATE-018");
}

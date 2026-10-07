//! The updater helper's self-anchor (KEL-53 "Helper launch and self-anchor", §7 row
//! "17 (helper launch and self-anchor)"; KEL-270 T4d slice S9a) over real per-user
//! Windows state.
//!
//! Oracles are the typed refusal with its code and step, the exact bytes of every
//! protected record before and after (no case may write), and, for the journaled image,
//! a BLAKE3 the test computes from the file read by path. Only an elevated Administrators
//! token can seed a Machine-UAC installation, so the ordinary gate runs the public entry
//! up to its mode gate, which every per-user installation fails, and the gates after it
//! through `anchor_located` over the same per-user state; the Machine-UAC positive is an
//! operator cell.

use std::path::Path;

use super::locate::{expected_for, helper_path, open_image, rewrite_provenance, state};
use super::signed_host::host_embedded_for;
use super::support::package_content_with;
use super::transaction::{commit_with, verifier};
use super::writer::{seed_pending_activation_journal_with, seed_per_user_baseline_with};
use crate::records::{ActivationPhase, PointerKind};
use crate::windows_baseline::helper::{anchor_located, require_machine_uac};
use crate::windows_baseline::locate::locate;
use crate::windows_baseline::{
    UpdaterHelperAnchor, UpdaterHelperRole, WindowsBaselineTrust, anchor_updater_helper,
    load_windows_activation_write_snapshot, select_windows_active_package,
};
use crate::{DirectInstallMode, ProvenanceField, UpdateError, WindowsLocatedImage};

const ROLES: [UpdaterHelperRole; 2] = [UpdaterHelperRole::Activation, UpdaterHelperRole::Recovery];

struct Install {
    _fixture: tempfile::TempDir,
    trust: WindowsBaselineTrust,
    content: Vec<u8>,
}

/// A per-user installation whose tree holds `helper` as `keld-updater-helper.exe`.
fn installed_with(helper: &[u8]) -> Install {
    let fixture = tempfile::tempdir().expect("updater-helper self-anchor fixture");
    let content = package_content_with(b"keld-host fixture image", helper);
    let trust = seed_per_user_baseline_with(fixture.path(), &content);
    Install {
        _fixture: fixture,
        trust,
        content,
    }
}

/// A helper image that carries the fixture installation's own expectation.
fn embedded_helper() -> Vec<u8> {
    host_embedded_for(&crate::tests::expected_identity().app_id)
}

/// The public entry, as the helper calls it with its verified signer.
fn anchor(
    trust: &WindowsBaselineTrust,
    role: UpdaterHelperRole,
    locator: &Path,
) -> Result<UpdaterHelperAnchor, UpdateError> {
    let executable = open_image(locator);
    anchor_updater_helper(
        role,
        locator,
        &executable,
        &trust.publisher_scope,
        &trust.installation.app_id,
    )
}

/// The gates after the mode gate, over the per-user state the ordinary gate can seed.
fn anchor_past_mode(
    trust: &WindowsBaselineTrust,
    role: UpdaterHelperRole,
    version: &str,
    signer: (&[u8; 32], &str),
) -> Result<UpdaterHelperAnchor, UpdateError> {
    let locator = helper_path(trust, version);
    let executable = open_image(&locator);
    let installation = locate(
        WindowsLocatedImage::UpdaterHelper,
        &locator,
        &executable,
        &expected_for(trust),
    )?;
    anchor_located(role, installation, &executable, signer.0, signer.1)
}

fn recorded_signer(trust: &WindowsBaselineTrust) -> (&[u8; 32], &str) {
    (&trust.publisher_scope, &trust.installation.app_id)
}

/// Runs `attempt`, which must refuse and write nothing.
fn refuses(
    trust: &WindowsBaselineTrust,
    why: &str,
    attempt: impl FnOnce() -> Result<UpdaterHelperAnchor, UpdateError>,
) -> UpdateError {
    let before = state(trust);
    let error = attempt().expect_err(why);
    assert_eq!(state(trust), before, "{why}: the refusal writes nothing");
    error
}

fn helper_step(error: &UpdateError) -> &'static str {
    assert_eq!(error.code(), "KELD-UPDATE-021", "{error}");
    match error {
        UpdateError::UpdaterHelper { step, .. } => step,
        other => panic!("expected KELD-UPDATE-021, got {other:?}"),
    }
}

fn helper_detail(error: &UpdateError) -> &str {
    match error {
        UpdateError::UpdaterHelper { detail, .. } => detail,
        other => panic!("expected KELD-UPDATE-021, got {other:?}"),
    }
}

/// Points `current` at `version` of the installation's artifact, keeping the record's
/// protected descriptor.
fn write_current(trust: &WindowsBaselineTrust, version: &str) {
    let mut artifact = trust.installation.baseline.clone();
    version.clone_into(&mut artifact.version);
    std::fs::write(
        trust.installation.update_root.join("current"),
        crate::records::encode_pointer(PointerKind::Current, &artifact).expect("encode current"),
    )
    .expect("rewrite current");
}

#[test]
fn only_machine_uac_runs_the_updater_helper() {
    require_machine_uac(DirectInstallMode::MachineUacDirect, "install mode")
        .expect("MachineUacDirect runs the helper");
    for mode in [
        DirectInstallMode::PerUserDirect,
        DirectInstallMode::MachineSeamlessDirect,
    ] {
        let error = require_machine_uac(mode, "install mode").expect_err("no helper");
        assert_eq!(helper_step(&error), "install mode");
        assert_eq!(
            error.to_string(),
            format!(
                "KELD-UPDATE-021: updater helper install mode refused (the installation records `{mode:?}`, and only `MachineUacDirect` runs keld-updater-helper.exe). Only a MachineUacDirect installation runs keld-updater-helper.exe, and only its journaled image or the image of the version its role requires; start nothing in its place, and repair or reinstall through the trusted installer if the installation is damaged."
            )
        );
    }
}

#[test]
fn a_per_user_helper_refuses_on_its_recorded_mode_before_any_lease() {
    let install = installed_with(&embedded_helper());
    let locator = helper_path(&install.trust, "1.0.0");
    for role in ROLES {
        let error = refuses(
            &install.trust,
            "a per-user installation runs no helper",
            || anchor(&install.trust, role, &locator),
        );
        assert_eq!(helper_step(&error), "install mode", "{role:?}");
        assert!(helper_detail(&error).contains("`PerUserDirect`"), "{error}");
    }
    // While another writer holds the exclusive lease, the refusal is still the mode's:
    // the gate runs before the snapshot lease, which would report `WriterActive`.
    let writer = load_windows_activation_write_snapshot(&install.trust, &verifier(&install.trust))
        .expect("hold the exclusive writer lease");
    for role in ROLES {
        let error = refuses(&install.trust, "the mode gate precedes the lease", || {
            anchor(&install.trust, role, &locator)
        });
        assert_eq!(helper_step(&error), "install mode", "{role:?}: {error}");
    }
    drop(writer);
}

#[test]
fn a_record_naming_a_managed_owner_refuses_before_the_mode_gate() {
    // `keld.install-provenance/v2` admits only the direct owner, so a record that names
    // a package manager is refused as it decodes, before its mode is read.
    let install = installed_with(&embedded_helper());
    let record = install
        .trust
        .installation
        .install_root
        .join("install-provenance");
    let text = String::from_utf8(std::fs::read(&record).expect("record bytes"))
        .expect("the record is UTF-8");
    assert_eq!(text.matches(r#""owner":"direct""#).count(), 1, "{text}");
    // Overwriting keeps the record's protected descriptor; only its claim changes.
    std::fs::write(
        &record,
        text.replace(r#""owner":"direct""#, r#""owner":"msix-store""#),
    )
    .expect("name a managed owner");
    let locator = helper_path(&install.trust, "1.0.0");
    let error = refuses(
        &install.trust,
        "a managed installation runs no helper",
        || anchor(&install.trust, UpdaterHelperRole::Activation, &locator),
    );
    assert_eq!(error.code(), "KELD-UPDATE-014", "{error}");
    assert!(error.to_string().contains("unsupported owner"), "{error}");
}

#[test]
fn a_missing_duplicated_or_foreign_helper_payload_refuses() {
    let unsigned = std::fs::read(std::env::current_exe().expect("test executable path"))
        .expect("read the test executable image");
    let mut duplicated = embedded_helper();
    crate::tests::signed_image::rename_first_section(&mut duplicated, *b".keldeai");
    for (why, helper, pack_code) in [
        ("a helper without a container", unsigned, "KELD-PACK-007"),
        ("a helper with two containers", duplicated, "KELD-PACK-008"),
    ] {
        let install = installed_with(&helper);
        let located = helper_path(&install.trust, "1.0.0");
        // The same refusal with a locator that names no installation shows that the
        // payload is read from the handle before any installation is located.
        let nowhere = install
            .trust
            .installation
            .install_root
            .with_file_name("absent")
            .join("updates")
            .join("versions")
            .join("1.0.0")
            .join("tree")
            .join("keld-updater-helper.exe");
        for locator in [&located, &nowhere] {
            let executable = open_image(&located);
            let error = refuses(&install.trust, why, || {
                anchor_updater_helper(
                    UpdaterHelperRole::Recovery,
                    locator,
                    &executable,
                    &install.trust.publisher_scope,
                    &install.trust.installation.app_id,
                )
            });
            assert_eq!(error.code(), "KELD-UPDATE-019", "{why}: {error}");
            assert!(
                matches!(&error, UpdateError::ExpectedIdentityContainer { pack_code: found, .. } if *found == pack_code),
                "{why}: {error:?}"
            );
        }
    }
    let install = installed_with(&host_embedded_for("dev.keld.other"));
    let locator = helper_path(&install.trust, "1.0.0");
    let error = refuses(&install.trust, "a helper built for another app", || {
        anchor(&install.trust, UpdaterHelperRole::Recovery, &locator)
    });
    assert_eq!(error.code(), "KELD-UPDATE-003", "{error}");
    assert!(
        matches!(
            &error,
            UpdateError::ProvenanceMismatch { field: ProvenanceField::AppId, expected, .. }
                if expected == "dev.keld.other"
        ),
        "the record does not carry the helper's own expectation: {error:?}"
    );
}

#[test]
fn a_helper_outside_a_protected_version_tree_refuses() {
    let install = installed_with(&embedded_helper());
    let fixture_root = install
        .trust
        .installation
        .install_root
        .parent()
        .expect("fixture root")
        .to_path_buf();
    // A byte-identical copy beside the install root, and one in a forged layout of the
    // same shape: neither is the located tree's file, so the record is never consulted.
    let copy = fixture_root.join("keld-updater-helper.exe");
    std::fs::copy(helper_path(&install.trust, "1.0.0"), &copy).expect("copy the helper");
    let forged = fixture_root
        .join("forged")
        .join("updates")
        .join("versions")
        .join("1.0.0")
        .join("tree")
        .join("keld-updater-helper.exe");
    std::fs::create_dir_all(forged.parent().expect("forged tree")).expect("forged layout");
    std::fs::copy(&copy, &forged).expect("copy into the forged layout");
    let real = helper_path(&install.trust, "1.0.0");
    for (why, locator, executable, step) in [
        (
            "a copy run under the tree's path",
            &real,
            &copy,
            "executable identity",
        ),
        (
            "a copy in a forged user-writable layout",
            &forged,
            &forged,
            "located install root",
        ),
    ] {
        let handle = open_image(executable);
        let error = refuses(&install.trust, why, || {
            anchor_updater_helper(
                UpdaterHelperRole::Recovery,
                locator,
                &handle,
                &install.trust.publisher_scope,
                &install.trust.installation.app_id,
            )
        });
        assert_eq!(error.code(), "KELD-UPDATE-018", "{why}: {error}");
        assert!(
            matches!(
                &error,
                UpdateError::ExecutableBinding { image: WindowsLocatedImage::UpdaterHelper, step: found, .. }
                    if *found == step
            ),
            "{why}: {error:?}"
        );
    }
}

#[test]
fn a_forged_machine_uac_record_refuses_on_its_protection_profile() {
    // A user-writable layout cannot claim the Machine-UAC profile: its owner-private
    // record fails that profile's admission before the mode is trusted.
    let install = installed_with(&embedded_helper());
    rewrite_provenance(&install.trust, |recorded| {
        recorded.installation.install_mode = DirectInstallMode::MachineUacDirect;
    });
    let locator = helper_path(&install.trust, "1.0.0");
    let error = refuses(&install.trust, "a forged Machine-UAC record", || {
        anchor(&install.trust, UpdaterHelperRole::Activation, &locator)
    });
    assert_eq!(error.code(), "KELD-UPDATE-013", "{error}");
    assert!(
        matches!(
            &error,
            UpdateError::Baseline {
                step: "protected record profile",
                ..
            }
        ),
        "{error:?}"
    );
}

#[test]
fn a_signer_the_record_does_not_name_refuses() {
    let install = installed_with(b"keld-updater-helper fixture image");
    let app_id = install.trust.installation.app_id.clone();
    for (why, scope, verified_app, detail) in [
        (
            "another publisher",
            [0x08; 32],
            app_id.as_str(),
            "the installation's protected provenance records a different publisher than the verified signer".to_owned(),
        ),
        (
            "another app",
            install.trust.publisher_scope,
            "dev.keld.other",
            format!("the installation's protected provenance records app id `{app_id}`, not the verified `dev.keld.other`"),
        ),
    ] {
        for role in ROLES {
            let error = refuses(&install.trust, why, || {
                anchor_past_mode(&install.trust, role, "1.0.0", (&scope, verified_app))
            });
            assert_eq!(error.code(), "KELD-UPDATE-020", "{why}");
            assert_eq!(
                error,
                UpdateError::RecordedSignerMismatch {
                    detail: detail.clone()
                },
                "{why}/{role:?}"
            );
        }
    }
}

#[test]
fn without_a_journal_each_role_anchors_only_in_its_own_version() {
    let install = installed_with(b"keld-updater-helper fixture image");
    let trust = &install.trust;
    // The baseline is current and last-known-good: both roles anchor.
    for role in ROLES {
        let anchored = anchor_past_mode(trust, role, "1.0.0", recorded_signer(trust))
            .unwrap_or_else(|error| panic!("{role:?} anchors in the baseline: {error}"));
        assert_eq!(anchored.trust().installation, trust.installation);
        assert_eq!(anchored.trust().publisher_scope, trust.publisher_scope);
    }
    // After an update, `current` names previous-known-good 1.0.0 while last-known-good
    // is 2.0.0, so the two roles anchor in different trees.
    commit_with(trust, "2.0.0", &install.content);
    write_current(trust, "1.0.0");
    for (role, accepted, refused, step) in [
        (
            UpdaterHelperRole::Activation,
            "1.0.0",
            "2.0.0",
            "activation version",
        ),
        (
            UpdaterHelperRole::Recovery,
            "2.0.0",
            "1.0.0",
            "recovery version",
        ),
    ] {
        let before = state(trust);
        anchor_past_mode(trust, role, accepted, recorded_signer(trust))
            .unwrap_or_else(|error| panic!("{role:?} anchors in {accepted}: {error}"));
        assert_eq!(state(trust), before, "{role:?}: anchoring writes nothing");
        let error = refuses(trust, "a helper outside its role's version", || {
            anchor_past_mode(trust, role, refused, recorded_signer(trust))
        });
        assert_eq!(helper_step(&error), step, "{role:?}");
        assert!(
            helper_detail(&error).contains(&format!("in version `{refused}`")),
            "{error}"
        );
    }
}

#[test]
fn an_invalid_current_refuses_the_activation_role_only() {
    let install = installed_with(b"keld-updater-helper fixture image");
    let trust = &install.trust;
    std::fs::write(
        trust.installation.update_root.join("current"),
        b"not a pointer",
    )
    .expect("invalidate current");
    let error = refuses(trust, "no version is selected", || {
        anchor_past_mode(
            trust,
            UpdaterHelperRole::Activation,
            "1.0.0",
            recorded_signer(trust),
        )
    });
    assert_eq!(helper_step(&error), "activation version");
    assert!(
        helper_detail(&error).contains("current is invalid"),
        "{error}"
    );
    // The recovery role owns that repair; it anchors in last-known-good.
    let before = state(trust);
    anchor_past_mode(
        trust,
        UpdaterHelperRole::Recovery,
        "1.0.0",
        recorded_signer(trust),
    )
    .expect("the recovery role anchors in last-known-good");
    assert_eq!(state(trust), before, "anchoring writes nothing");
}

#[test]
fn with_a_journal_only_the_journaled_image_anchors_in_any_version() {
    // Several read buffers long, with no repeating block, so a digest of any part of the
    // image differs from the whole image's.
    let helper: Vec<u8> = (0..40_000_u32)
        .map(|index| u8::try_from(index % 251).expect("below 251"))
        .collect();
    for (label, journaled, anchors) in [
        // The independent oracle: BLAKE3 of the file bytes, read by path.
        ("journaled", *blake3::hash(&helper).as_bytes(), true),
        ("another image", [0x55; 32], false),
    ] {
        let install = installed_with(&helper);
        let trust = &install.trust;
        assert_eq!(
            std::fs::read(helper_path(trust, "1.0.0")).expect("helper bytes"),
            helper,
            "the tree holds the fixture helper"
        );
        seed_pending_activation_journal_with(trust, ActivationPhase::PublishPending, journaled);
        // The seeded candidate 2.0.0 copies the baseline tree, helper included.
        for version in ["1.0.0", "2.0.0"] {
            for role in ROLES {
                let result = anchor_past_mode(trust, role, version, recorded_signer(trust));
                if anchors {
                    result.unwrap_or_else(|error| {
                        panic!("{label}/{version}/{role:?} anchors on the digest: {error}")
                    });
                } else {
                    let error = result.expect_err("another image's digest refuses");
                    assert_eq!(
                        helper_step(&error),
                        "journaled image",
                        "{label}/{version}/{role:?}"
                    );
                    assert!(
                        helper_detail(&error).contains(&"55".repeat(32)),
                        "the refusal names the journaled digest: {error}"
                    );
                }
            }
        }
    }
}

/// Whether an ordinary writer can open `path` for writing now; a derived helper image
/// shares only reads, so while one is held the open fails with a sharing violation.
fn writable(path: &Path) -> bool {
    match std::fs::OpenOptions::new().write(true).open(path) {
        Ok(_) => true,
        Err(error) => {
            assert_eq!(
                error.raw_os_error(),
                Some(32),
                "a sharing violation: {error}"
            );
            false
        }
    }
}

#[test]
fn a_per_user_selection_offers_no_activation_helper() {
    let install = installed_with(b"keld-updater-helper fixture image");
    let selection =
        select_windows_active_package(&install.trust).expect("the per-user baseline selects");
    let error = selection
        .open_activation_helper()
        .expect_err("only MachineUacDirect activates through the helper");
    assert_eq!(helper_step(&error), "install mode");
    assert!(helper_detail(&error).contains("`PerUserDirect`"), "{error}");
    // Nothing was opened: the tree's helper stays writable by its owner.
    assert!(writable(&helper_path(&install.trust, "1.0.0")));
}

#[test]
fn the_activation_helper_is_the_selected_trees_file() {
    let install = installed_with(b"keld-updater-helper fixture image");
    let trust = &install.trust;
    commit_with(trust, "2.0.0", &install.content);
    // Copies beside the install root and in the previous version's tree are never derived.
    let planted = trust
        .installation
        .install_root
        .with_file_name("keld-updater-helper.exe");
    std::fs::copy(helper_path(trust, "1.0.0"), &planted).expect("plant a copy");
    let selection = select_windows_active_package(trust).expect("2.0.0 is selected");
    assert_eq!(selection.artifact().version, "2.0.0");
    let selected = helper_path(trust, "2.0.0");
    assert!(writable(&selected), "nothing holds the selected helper yet");
    let image = selection
        .open_tree_helper()
        .expect("the selected tree's helper is derived");
    assert_eq!(
        image.path(),
        selected,
        "the path is the selected tree's helper"
    );
    // The derived handle is on exactly that file: it alone is pinned against writers.
    assert!(
        !writable(&selected),
        "the derived image pins the selected file"
    );
    assert!(
        writable(&helper_path(trust, "1.0.0")),
        "the previous tree's helper"
    );
    assert!(writable(&planted), "the planted copy");
    drop(image);
    assert!(writable(&selected), "the pin ends with the derived image");
}

#[test]
fn a_selected_tree_without_its_helper_file_derives_nothing() {
    for (label, replace) in [("absent", false), ("a directory", true)] {
        let install = installed_with(b"keld-updater-helper fixture image");
        let trust = &install.trust;
        let selected = helper_path(trust, "1.0.0");
        std::fs::remove_file(&selected).expect("remove the tree's helper");
        if replace {
            std::fs::create_dir(&selected).expect("a directory in the helper's place");
        }
        // A copy beside the install root is never a fallback.
        std::fs::write(
            trust
                .installation
                .install_root
                .with_file_name("keld-updater-helper.exe"),
            b"keld-updater-helper fixture image",
        )
        .expect("plant a copy");
        let selection = select_windows_active_package(trust).expect("the baseline selects");
        let error = selection
            .open_tree_helper()
            .expect_err("no helper is derived without the tree's file");
        assert_eq!(helper_step(&error), "activation image", "{label}: {error}");
    }
}

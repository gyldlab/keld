//! T4d explicit-UAC installer admission tests.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::support::{self, MACHINE_UAC_ROOT_ENV, baseline};
use crate::windows_baseline::initialize::{
    BaselineBoundary, initialize_machine_uac_with_observer,
    initialize_machine_uac_with_token_check as initialize_with_injected_token,
    initialize_windows_machine_uac_baseline,
};
use crate::{DirectInstallMode, InstallOwner, InstallProvenance, UpdateError};

#[test]
fn machine_uac_authority_failure_precedes_all_filesystem_admission() {
    let temp = tempfile::tempdir().expect("isolated negative-control parent");
    let install = temp.path().join("must-not-be-created");
    let mut trust = support::trust_for(&install);
    trust.installation.install_mode = DirectInstallMode::MachineUacDirect;
    let verified = baseline(&trust);
    let error = initialize_with_injected_token(
        &verified,
        &temp.path().join("absent-baseline.tar"),
        &trust,
        || Err(io::Error::other("injected token query failure")),
    )
    .expect_err("failed authority admission refuses baseline setup");
    assert!(matches!(
        error,
        UpdateError::Baseline {
            step: "installer authority",
            ..
        }
    ));
    assert!(
        !install.exists(),
        "authority refusal precedes path admission"
    );
    assert_eq!(
        std::fs::read_dir(temp.path())
            .expect("unchanged fixture parent")
            .count(),
        0
    );
}

fn operator_root() -> PathBuf {
    let root = PathBuf::from(
        std::env::var_os(MACHINE_UAC_ROOT_ENV)
            .expect("operator supplies a unique direct C: child for UAC acceptance"),
    );
    assert_eq!(root.parent(), Some(Path::new(r"C:\")));
    let leaf = root.file_name().and_then(|value| value.to_str()).unwrap();
    assert!(leaf.starts_with("KeldKel270Uac-") && leaf.len() > 16);
    assert!(
        !root.exists(),
        "never reuse or repair an existing UAC fixture"
    );
    root
}

#[test]
#[ignore = "operator acceptance: run this exact selector in an explicitly elevated administrator process"]
fn machine_uac_elevated_installer_seeds_the_common_baseline() {
    let root = operator_root();
    let anchor = support::directory(Path::new(r"C:\"));
    keld_guard::validate_windows_machine_volume_anchor(&anchor)
        .expect("C: retains its protected volume-anchor ACL");
    let profile = keld_guard::WindowsInstallProtectionProfile::MachineUac;
    crate::windows_fs::create_directory_relative_with_profile(
        &anchor,
        root.file_name().and_then(|value| value.to_str()).unwrap(),
        profile,
    )
    .expect("elevated owner-capable Administrators token can assign the UAC ancestor profile");
    let source = root.join("baseline.tar");
    fs::write(&source, super::support::GOLDEN).expect("write signed baseline fixture");
    let trust = support::provision_machine_uac_as_administrator(&root, "application");
    let verified = baseline(&trust);
    let mut checked_stage_root = false;
    let receipt = initialize_machine_uac_with_observer(&verified, &source, &trust, |boundary| {
        if boundary == BaselineBoundary::StageCreated {
            let versions = trust.installation.update_root.join("versions");
            let stages: Vec<_> = fs::read_dir(&versions)
                .expect("read version parent at stage creation")
                .map(|entry| entry.expect("stage directory entry").path())
                .collect();
            assert_eq!(stages.len(), 1, "only the new incomplete stage exists");
            let stage = support::directory(&stages[0]);
            keld_guard::validate_windows_install_directory(&stage, profile)
                .expect("MachineUac root stage has exact ACL at first creation boundary");
            checked_stage_root = true;
        }
        Ok(())
    })
    .expect("explicit-UAC installer seeds the same journaled baseline transaction");
    assert!(
        checked_stage_root,
        "observe the root before population begins"
    );
    assert_eq!(receipt.loaded().identity(), &trust.installation);
    assert_eq!(receipt.loaded().version_floor(), "1.0.0");
    assert!(matches!(
        receipt.loaded().observation(),
        crate::ProvenanceObservation::Protected {
            record: InstallProvenance {
                identity,
                owner: InstallOwner::Direct,
            },
            version_floor: Some(floor),
        } if identity.install_mode == DirectInstallMode::MachineUacDirect && floor == "1.0.0"
    ));
    let install = &trust.installation.install_root;
    let update = &trust.installation.update_root;
    let profile = trust.installation.install_mode.protection_profile();
    for path in [
        install.as_path(),
        update.as_path(),
        &update.join("versions"),
        &update.join("versions/1.0.0"),
        &update.join("versions/1.0.0/tree"),
        &update.join("versions/1.0.0/tree/nest"),
    ] {
        let file = support::directory(path);
        keld_guard::validate_windows_install_directory(&file, profile)
            .expect("published directories retain exact Administrators/SYSTEM profile");
    }
    for path in [
        &install.join("install-provenance"),
        &update.join("bootstrap.lock"),
        &update.join("activation.lock"),
        &update.join("version-floor"),
        &update.join("current"),
        &update.join("last-known-good"),
        &update.join("versions/1.0.0/.complete"),
        &update.join("versions/1.0.0/content.tar"),
        &update.join("versions/1.0.0/tree/nest/one"),
    ] {
        let file = fs::File::open(path).expect("open protected baseline record");
        keld_guard::validate_windows_install_file(&file, profile)
            .expect("published files retain exact Administrators/SYSTEM profile");
    }
    let reloaded = crate::windows_baseline::load_windows_baseline(&trust)
        .expect("production read-only reload accepts the committed UAC baseline");
    assert_eq!(reloaded.version_floor(), "1.0.0");
    println!("KELD_KEL270_MACHINE_UAC_BASELINE_PASS mode=machine-uac-direct version=1.0.0");
    drop(reloaded);
    drop(receipt);
    fs::remove_dir_all(&root)
        .expect("remove isolated operator fixture after successful assertions");
}

#[test]
#[ignore = "operator acceptance: run this exact selector from the installing user's unelevated token"]
fn machine_uac_filtered_installer_refuses_before_any_filesystem_admission() {
    super::support::assert_ordinary_token();
    let temp = tempfile::tempdir().expect("isolated unelevated negative-control parent");
    let install = temp.path().join("must-not-be-created");
    let mut trust = support::trust_for(&install);
    trust.installation.install_mode = DirectInstallMode::MachineUacDirect;
    let verified = baseline(&trust);
    let error = initialize_windows_machine_uac_baseline(
        &verified,
        &temp.path().join("absent-baseline.tar"),
        &trust,
    )
    .expect_err("filtered or ordinary token cannot initialize protected machine state");
    assert!(matches!(
        error,
        UpdateError::Baseline {
            step: "installer authority",
            ..
        }
    ));
    assert!(
        !install.exists(),
        "refusal precedes all filesystem admission"
    );
    assert_eq!(
        fs::read_dir(temp.path())
            .expect("unchanged negative-control parent")
            .count(),
        0
    );
    println!("KELD_KEL270_MACHINE_UAC_FILTERED_REFUSAL_PASS writes=0");
}

#[test]
fn machine_uac_initializer_refuses_a_different_mode_before_authority_query() {
    let temp = tempfile::tempdir().expect("isolated wrong-mode parent");
    let install = temp.path().join("must-not-be-created");
    let trust = support::trust_for(&install);
    let verified = baseline(&trust);
    let mut called = false;
    let error = initialize_with_injected_token(
        &verified,
        &temp.path().join("absent-baseline.tar"),
        &trust,
        || {
            called = true;
            Ok(())
        },
    )
    .expect_err("UAC initializer cannot select a per-user identity");
    assert!(matches!(
        error,
        UpdateError::Baseline {
            step: "installer mode",
            ..
        }
    ));
    assert!(!called, "mode provenance is checked before token authority");
    assert!(!install.exists());
}

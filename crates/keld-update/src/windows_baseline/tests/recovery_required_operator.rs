//! Operator acceptance for the Machine-UAC recovery-required state of an ordinary startup
//! (KEL-53 criterion 17, §7 row "17 (recovery-required state)"; KEL-254 AC4 and AC16;
//! KEL-270 T4d slice S2): the end-to-end Machine-UAC cells of `recovery_required`.
//!
//! The elevated seeding selector writes every case and its completion marker last; the
//! ordinary-user selector then observes each case on both startup entry points, with the
//! exact bytes of every installation file and directory before and after. No ordinary
//! selector here may write installation state.

use std::fs;
use std::path::Path;

use super::locate::{expected_for, host_path, open_image};
use super::locate_operator::{LABEL, census, leaf, machine_uac_trust, seeded};
use super::machine_uac::{operator_root, operator_root_path};
use super::recovery_required::{RECOVERY_DISABLED, UNDECODABLE, phases, step_and_effect};
use super::repair_source::COMPLETION_RECORD_REMOVED;
use super::support::{self, baseline_with, host_package_content};
use super::writer::seed_pending_activation_journal;
use crate::UpdateError;
use crate::records::ActivationPhase;
use crate::windows_baseline::{
    initialize_windows_machine_uac_baseline, select_windows_active_package,
};

const SEEDED: &str = "kel270-recovery-required-seeded.txt";

/// One operator Machine-UAC case: the fixture leaf and how its state differs from the
/// committed baseline.
enum MachineCase {
    Journal(ActivationPhase),
    CurrentAbsent,
    CurrentUndecodable,
    /// `current` is absent and last-known-good (the baseline) lost its completion record,
    /// so it is not a valid repair source.
    CurrentAbsentLkgDamaged,
}

/// What an ordinary startup must return for a case: the typed state at its step, or the
/// untyped refusal of the failed last-known-good check, which keeps its exact reason
/// (KEL-254 AC16).
fn expected_refusal(case: &MachineCase) -> Result<&'static str, UpdateError> {
    match case {
        MachineCase::Journal(_) => Ok("active package selection"),
        MachineCase::CurrentAbsent | MachineCase::CurrentUndecodable => {
            Ok("current pointer repair")
        }
        MachineCase::CurrentAbsentLkgDamaged => Err(UpdateError::Baseline {
            step: "version contents",
            detail: COMPLETION_RECORD_REMOVED.to_owned(),
        }),
    }
}

fn assert_startup_refusal(
    label: &str,
    error: &UpdateError,
    expected: &Result<&'static str, UpdateError>,
) {
    match expected {
        Ok(step) => assert_eq!(
            step_and_effect(error),
            (*step, RECOVERY_DISABLED),
            "{label}: {error}"
        ),
        Err(untyped) => assert_eq!(error, untyped, "{label}: {error}"),
    }
}

fn machine_cases() -> Vec<(&'static str, MachineCase)> {
    let mut cases: Vec<_> = phases()
        .into_iter()
        .map(|(label, phase)| (label, MachineCase::Journal(phase)))
        .collect();
    cases.push(("current-absent", MachineCase::CurrentAbsent));
    cases.push(("current-undecodable", MachineCase::CurrentUndecodable));
    cases.push((
        "current-absent-lkg-damaged",
        MachineCase::CurrentAbsentLkgDamaged,
    ));
    cases
}

#[test]
#[ignore = "operator acceptance: run this exact selector in an explicitly elevated administrator process before the ordinary-user recovery-required selector"]
fn machine_uac_elevated_installer_seeds_recovery_required_states() {
    let root = operator_root();
    let anchor = support::directory(Path::new(r"C:\"));
    keld_guard::validate_windows_machine_volume_anchor(&anchor)
        .expect("C: retains its protected volume-anchor ACL");
    let profile = keld_guard::WindowsInstallProtectionProfile::MachineUac;
    let created =
        crate::windows_fs::create_directory_relative_with_profile(&anchor, leaf(&root), profile)
            .expect("elevated owner-capable Administrators token assigns the UAC ancestor profile");
    let content = host_package_content();
    let source = root.join("host-baseline.tar");
    fs::write(&source, &content).expect("write the signed host baseline fixture");
    for (label, case) in machine_cases() {
        drop(
            crate::windows_fs::create_directory_relative_with_profile(&created, label, profile)
                .expect("each case parent starts with the exact UAC ancestor profile"),
        );
        let trust = support::provision_machine_uac_as_administrator_with(
            &root.join(label),
            LABEL,
            &content,
        );
        drop(
            initialize_windows_machine_uac_baseline(
                &baseline_with(&trust, &content),
                &source,
                &trust,
            )
            .expect("explicit-UAC installer seeds the committed host baseline"),
        );
        let current = trust.installation.update_root.join("current");
        match case {
            MachineCase::Journal(phase) => {
                seed_pending_activation_journal(&trust, phase);
            }
            MachineCase::CurrentAbsent => {
                fs::remove_file(&current).expect("lose the protected current record");
            }
            MachineCase::CurrentUndecodable => {
                fs::write(&current, UNDECODABLE).expect("corrupt the protected current record");
            }
            MachineCase::CurrentAbsentLkgDamaged => {
                fs::remove_file(&current).expect("lose the protected current record");
                fs::remove_file(
                    trust
                        .installation
                        .update_root
                        .join("versions")
                        .join("1.0.0")
                        .join(".complete"),
                )
                .expect("damage the protected last-known-good version");
            }
        }
        assert_eq!(
            machine_uac_trust(&root.join(label), &content).installation,
            trust.installation,
            "{label}: the ordinary selector derives the same trusted installation"
        );
    }
    drop(created);
    fs::write(root.join(SEEDED), b"seeded").expect("seeding completion marker, written last");
    println!(
        "KELD_KEL270_RECOVERY_REQUIRED_SEED_PASS root={} cases={}",
        root.display(),
        machine_cases().len()
    );
}

#[test]
#[ignore = "operator acceptance: run this exact selector from an ordinary unelevated user token after the elevated recovery-required seeding selector"]
fn machine_uac_ordinary_startup_returns_recovery_required_and_writes_nothing() {
    support::assert_ordinary_token();
    let root = seeded(operator_root_path(), SEEDED);
    let content = host_package_content();
    for (label, case) in machine_cases() {
        let trust = machine_uac_trust(&root.join(label), &content);
        let expected = expected_refusal(&case);
        let install = &trust.installation.install_root;
        let before = census(install);
        let error = select_windows_active_package(&trust)
            .expect_err("an ordinary Machine-UAC startup selects nothing");
        assert_startup_refusal(label, &error, &expected);
        let locator = host_path(&trust, "1.0.0");
        let executable = open_image(&locator);
        let located = crate::windows_baseline::select_active_package_for_executable(
            crate::WindowsLocatedImage::Host,
            &locator,
            &executable,
            &expected_for(&trust),
            &trust.publisher_scope,
            &trust.installation.app_id,
        )
        .expect_err("an ordinary located Machine-UAC startup selects nothing");
        drop(executable);
        assert_startup_refusal(label, &located, &expected);
        assert_eq!(
            census(install),
            before,
            "{label}: the startup writes nothing"
        );
        let outcome = match &expected {
            Ok(step) => format!("step={step} guidance=RecoveryDisabled"),
            Err(untyped) => format!("untyped={}", untyped.code()),
        };
        println!("KELD_KEL270_RECOVERY_REQUIRED_PASS case={label} {outcome} writes=0");
    }
}

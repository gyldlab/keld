//! The Machine-UAC recovery-required state of an ordinary startup (KEL-53 criterion 17,
//! §7 row "17 (recovery-required state)"; KEL-254 AC4 and AC16; KEL-270 T4d slice S2).
//!
//! In `MachineUacDirect` only the elevated helper writes, so an ordinary startup that
//! finds a pending journal of any phase, or no journal with an absent or undecodable
//! `current` beside a valid last-known-good, returns the typed `MachineRecoveryRequired`
//! effect and writes nothing; a damaged last-known-good keeps its own untyped refusal.
//! `PerUserDirect` keeps its journal-bound refusal and its startup repair, and
//! `MachineSeamlessDirect` keeps its untyped refusal.
//!
//! Oracles are the typed error with its step and code, and the exact bytes of every
//! protected record (in the operator rows, of every installation file and directory)
//! before and after. Only an elevated Administrators token can seed a Machine-UAC
//! installation, so the end-to-end Machine-UAC rows are operator selectors in
//! `recovery_required_operator`; the ordinary gate exercises the mode decision itself over
//! real per-user state.

use std::fs;
use std::path::Path;

use super::locate::{expected_for, host_path, open_image, refuses, select_from, state};
use super::locate_operator::census;
use super::support::host_package_content;
use super::writer::{seed_pending_activation_journal, seed_per_user_baseline_with};
use crate::records::{ActivationPhase, PointerKind};
use crate::windows_baseline::load::{
    LocatedVersion, pending_journal_refusal, repair_invalid_current,
};
use crate::windows_baseline::{WindowsBaselineTrust, select_windows_active_package};
use crate::{
    ActivationEffect, ActivationFailureClass, DirectInstallMode, MachineRecoveryGuidance,
    UpdateError, WindowsLocatedImage,
};

/// The interim Machine-UAC state: this release does not provide the recovery-only role.
pub(super) const RECOVERY_DISABLED: ActivationEffect =
    ActivationEffect::MachineRecoveryRequired(MachineRecoveryGuidance::RecoveryDisabled);
pub(super) const UNDECODABLE: &[u8] = b"not a pointer";

/// Every persisted journal phase, labelled for fixture names and diagnostics.
pub(super) fn phases() -> [(&'static str, ActivationPhase); 4] {
    [
        ("publish-pending", ActivationPhase::PublishPending),
        ("awaiting-health", ActivationPhase::AwaitingHealth),
        (
            "health-accepted",
            ActivationPhase::HealthAccepted {
                health_receipt_digest: [0x99; 32],
            },
        ),
        (
            "rollback-pending",
            ActivationPhase::RollbackPending {
                failure: ActivationFailureClass::HealthRejected,
            },
        ),
    ]
}

/// The step and effect of a typed activation refusal, which carries the rest as text.
pub(super) fn step_and_effect(error: &UpdateError) -> (&'static str, ActivationEffect) {
    match error {
        UpdateError::Activation { step, effect, .. } => (step, *effect),
        other => panic!("expected a typed KELD-UPDATE-016 activation refusal: {other:?}"),
    }
}

/// The realistic causes the snapshot reports for an absent or undecodable `current`.
fn invalid_current_causes() -> [(&'static str, UpdateError); 2] {
    [
        (
            "absent",
            UpdateError::Baseline {
                step: "current pointer",
                detail: "the current record is absent".to_owned(),
            },
        ),
        (
            "undecodable",
            crate::records::decode_pointer(PointerKind::Current, UNDECODABLE)
                .expect_err("the fixture bytes are not a canonical pointer"),
        ),
    ]
}

#[test]
fn a_machine_uac_pending_journal_of_every_phase_is_typed_recovery_required() {
    for (label, phase) in phases() {
        let error = pending_journal_refusal(DirectInstallMode::MachineUacDirect, &phase);
        assert_eq!(
            step_and_effect(&error),
            ("active package selection", RECOVERY_DISABLED),
            "{label}: {error}"
        );
        assert_eq!(error.code(), "KELD-UPDATE-016", "{label}");
        assert!(
            error.to_string().contains("recovery-only role"),
            "{label}: the refusal names the only resolver: {error}"
        );
    }
}

#[test]
fn a_pending_journal_stays_journal_bound_outside_machine_uac() {
    for mode in [
        DirectInstallMode::PerUserDirect,
        DirectInstallMode::MachineSeamlessDirect,
    ] {
        for (label, phase) in phases() {
            let error = pending_journal_refusal(mode, &phase);
            assert_eq!(
                step_and_effect(&error),
                (
                    "active package selection",
                    ActivationEffect::JournalBoundRecoveryRequired
                ),
                "{mode:?}/{label}: {error}"
            );
        }
    }
}

/// A per-user installation of the host package, so both startup entry points can run.
pub(super) fn per_user_host_install(fixture: &Path) -> WindowsBaselineTrust {
    seed_per_user_baseline_with(fixture, &host_package_content())
}

#[test]
fn a_per_user_pending_journal_of_every_phase_stays_journal_bound_and_writes_nothing() {
    for (label, phase) in phases() {
        let fixture = tempfile::tempdir().expect("per-user pending journal fixture");
        let trust = per_user_host_install(fixture.path());
        seed_pending_activation_journal(&trust, phase);
        let before = state(&trust);
        let error = select_windows_active_package(&trust)
            .expect_err("a pending journal must not select a tree");
        assert_eq!(state(&trust), before, "{label}: selection writes nothing");
        assert_eq!(
            step_and_effect(&error),
            (
                "active package selection",
                ActivationEffect::JournalBoundRecoveryRequired
            ),
            "{label}: {error}"
        );
        let locator = host_path(&trust, "1.0.0");
        let executable = open_image(&locator);
        let error = refuses(
            &trust,
            &locator,
            &executable,
            &expected_for(&trust),
            "a pending journal must not select a located tree",
        );
        assert_eq!(
            step_and_effect(&error).1,
            ActivationEffect::JournalBoundRecoveryRequired,
            "{label}: {error}"
        );
    }
}

/// A per-user installation whose `current` is absent or undecodable, with a valid
/// last-known-good, and the mode-flipped trust that asks the startup gate to decide.
fn invalid_current_install(
    fixture: &Path,
    absent: bool,
    mode: DirectInstallMode,
) -> (WindowsBaselineTrust, WindowsBaselineTrust) {
    let trust = per_user_host_install(fixture);
    let current = trust.installation.update_root.join("current");
    if absent {
        fs::remove_file(&current).expect("lose the current record");
    } else {
        fs::write(&current, UNDECODABLE).expect("corrupt the current record");
    }
    let mut flipped = trust.clone();
    flipped.installation.install_mode = mode;
    (trust, flipped)
}

#[test]
fn a_machine_uac_invalid_current_is_typed_recovery_required_and_writes_nothing() {
    for (label, cause) in invalid_current_causes() {
        // No located host, the last-known-good host and a stale host: the ordinary
        // process never reaches the located-version gate in this mode.
        for located in [None, Some("1.0.0"), Some("0.9.0")] {
            let fixture = tempfile::tempdir().expect("Machine-UAC invalid current fixture");
            let (trust, machine) = invalid_current_install(
                fixture.path(),
                label == "absent",
                DirectInstallMode::MachineUacDirect,
            );
            let install = &trust.installation.install_root;
            let before = census(install);
            let located = located.map(|version| LocatedVersion {
                image: WindowsLocatedImage::Host,
                version,
            });
            let error = repair_invalid_current(&machine, &cause, located)
                .expect_err("an ordinary process never repairs a Machine-UAC current");
            assert_eq!(census(install), before, "{label}/{located:?}: no write");
            assert_eq!(
                step_and_effect(&error),
                ("current pointer repair", RECOVERY_DISABLED),
                "{label}/{located:?}: {error}"
            );
            assert_eq!(error.code(), "KELD-UPDATE-016");
            let (_, cause_detail) = cause.step_and_detail();
            assert!(
                error.to_string().contains(&cause_detail),
                "{label}: the refusal keeps the invalid-current evidence: {error}"
            );
        }
    }
}

#[test]
fn a_machine_seamless_invalid_current_keeps_its_landed_baseline_refusal() {
    for (label, cause) in invalid_current_causes() {
        let fixture = tempfile::tempdir().expect("machine-seamless invalid current fixture");
        let (trust, machine) = invalid_current_install(
            fixture.path(),
            label == "absent",
            DirectInstallMode::MachineSeamlessDirect,
        );
        let install = &trust.installation.install_root;
        let before = census(install);
        let error = repair_invalid_current(&machine, &cause, None)
            .expect_err("only the elevated writer may repair a machine-seamless installation");
        assert_eq!(census(install), before, "{label}: no write");
        assert!(
            matches!(
                &error,
                UpdateError::Baseline {
                    step: "current pointer repair",
                    detail,
                } if detail.contains("only the elevated writer of a machine installation")
            ),
            "{label}: {error:?}"
        );
        assert_eq!(error.code(), "KELD-UPDATE-013");
    }
}

#[test]
fn a_per_user_invalid_current_is_still_repaired_from_last_known_good() {
    for absent in [true, false] {
        let fixture = tempfile::tempdir().expect("per-user invalid current fixture");
        let (trust, _) =
            invalid_current_install(fixture.path(), absent, DirectInstallMode::PerUserDirect);
        let selection = select_from(&trust, "1.0.0").expect("the per-user startup repairs");
        assert_eq!(selection.artifact().version, "1.0.0");
        drop(selection);
        assert_eq!(
            fs::read(trust.installation.update_root.join("current")).expect("current"),
            crate::records::encode_pointer(PointerKind::Current, &trust.installation.baseline)
                .expect("canonical last-known-good pointer"),
            "absent={absent}: last-known-good is durably republished as current"
        );
    }
}

//! Windows writer admission refuses unsupported installation owner/mode before disk access.

use super::super::support;
use crate::windows_baseline::{
    load_windows_activation_write_snapshot, load_windows_baseline, load_windows_recovery_inspection,
};

#[test]
fn machine_writer_modes_refuse_before_opening_any_installation_path() {
    let fixture = tempfile::tempdir().expect("mode-refusal fixture");
    for mode in [
        crate::DirectInstallMode::MachineUacDirect,
        crate::DirectInstallMode::MachineSeamlessDirect,
    ] {
        let mut trust = support::trust_for(&fixture.path().join("absent-install"));
        trust.installation.install_mode = mode;
        let verifier = crate::UpdateVerifier::new(
            trust.installation.clone(),
            crate::tests::signing_key().verifying_key().to_bytes(),
        )
        .expect("mode-bound verifier");
        let error = load_windows_activation_write_snapshot(&trust, &verifier)
            .expect_err("privileged writer mechanisms have no production admission yet");
        assert!(
            matches!(
                error,
                crate::UpdateError::Baseline {
                    step: "activation writer mechanism",
                    ..
                }
            ),
            "mode {mode:?} refuses before path admission, got {error:?}"
        );
    }
}

#[test]
fn managed_owner_refuses_baseline_and_writer_before_filesystem_admission() {
    let fixture = tempfile::tempdir().expect("managed refusal fixture");
    let install = fixture.path().join("absent-managed-install");
    let mut trust = support::trust_for(&install);
    trust.owner = crate::InstallOwner::Managed {
        mechanism: "Microsoft Store MSIX".to_owned(),
    };
    assert!(
        trust.lifecycle_installation_id().is_err(),
        "managed deployments do not mint Keld direct-update lifecycle bindings"
    );
    let verifier = crate::UpdateVerifier::new(
        trust.installation.clone(),
        crate::tests::signing_key().verifying_key().to_bytes(),
    )
    .expect("trusted test verifier");

    let read_error = load_windows_baseline(&trust).expect_err("managed read refuses early");
    assert!(matches!(
        read_error,
        crate::UpdateError::ManagedInstall { ref mechanism }
            if mechanism == "Microsoft Store MSIX"
    ));
    let writer_error = load_windows_activation_write_snapshot(&trust, &verifier)
        .expect_err("managed writer refuses early");
    assert!(matches!(
        writer_error,
        crate::UpdateError::ManagedInstall { ref mechanism }
            if mechanism == "Microsoft Store MSIX"
    ));
    let signed_baseline = support::baseline(&trust);
    let init_error = crate::initialize_windows_baseline(
        &signed_baseline,
        &fixture.path().join("absent.tar"),
        &trust,
    )
    .expect_err("managed installation cannot bootstrap direct updater state");
    assert!(matches!(
        init_error,
        crate::UpdateError::ManagedInstall { ref mechanism }
            if mechanism == "Microsoft Store MSIX"
    ));
    let recovery_error = load_windows_recovery_inspection(&trust, &verifier)
        .expect_err("managed recovery refuses before filesystem admission");
    assert!(matches!(
        recovery_error,
        crate::UpdateError::ManagedInstall { ref mechanism }
            if mechanism == "Microsoft Store MSIX"
    ));
    assert!(
        !install.exists(),
        "early owner refusal creates no install tree"
    );
}

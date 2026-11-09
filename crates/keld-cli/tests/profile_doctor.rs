//! Static profile reporting must not invent a successful runtime admission.

use keld_cli::doctor::{DoctorFinding, findings_json, run_findings};

#[allow(clippy::expect_used)] // Test-only assertion that the public finding actually exists.
fn profile(root: &std::path::Path) -> DoctorFinding {
    run_findings(Some(root))
        .into_iter()
        .find(|finding| finding.label == "security_profile")
        .expect("doctor must report the profile independently of project layout")
}

#[test]
fn legacy_finding_discloses_forfeit_without_claiming_runtime_admission() {
    let root = tempfile::tempdir().expect("owned project");
    std::fs::write(
        root.path().join("keld.permissions.jsonc"),
        r#"{"profile":"legacy"}"#,
    )
    .expect("policy");
    let finding = profile(root.path());
    assert!(
        finding.ok,
        "valid declaration, not runtime qualification: {finding:?}"
    );
    assert!(finding.error.is_none());
    assert!(
        finding
            .detail
            .contains("legacy — zero-authority claim forfeited")
    );
    assert!(finding.detail.contains("runtime admission not performed"));
}

#[test]
fn strict_finding_does_not_convert_configuration_into_containment_proof() {
    let root = tempfile::tempdir().expect("owned project");
    std::fs::write(
        root.path().join("keld.permissions.jsonc"),
        r#"{"profile":"strict"}"#,
    )
    .expect("policy");
    let finding = profile(root.path());
    assert!(
        !finding.ok,
        "no exact artifact proof was supplied: {finding:?}"
    );
    assert_eq!(
        finding
            .error
            .expect("actionable missing-proof finding")
            .code,
        "KELD-GUARD011"
    );
    assert!(finding.detail.contains("requested strict"));
    assert!(finding.detail.contains("runtime admission not performed"));
    assert!(!finding.detail.contains("effective: strict"));
}

#[test]
fn absent_declaration_is_not_a_legacy_selection_or_observed_run() {
    let root = tempfile::tempdir().expect("owned project");
    for bytes in [None, Some(r#"{"sandbox":false,"appSandbox":"off"}"#)] {
        if let Some(bytes) = bytes {
            std::fs::write(root.path().join("keld.permissions.jsonc"), bytes).expect("policy");
        }
        let finding = profile(root.path());
        assert!(finding.ok, "{finding:?}");
        assert!(finding.detail.contains("no profile selected"));
        assert!(finding.detail.contains("unverified"));
        assert!(finding.detail.contains("runtime admission not performed"));
        assert!(!finding.detail.contains("legacy —"));
    }
}

#[test]
fn invalid_profile_keeps_guard_error_and_existing_doctor_json_array() {
    let root = tempfile::tempdir().expect("owned project");
    std::fs::write(
        root.path().join("keld.permissions.jsonc"),
        r#"{"profile":null}"#,
    )
    .expect("invalid policy");
    let finding = profile(root.path());
    assert!(!finding.ok);
    assert_eq!(
        finding.error.as_ref().expect("typed error").code,
        "KELD-GUARD005"
    );
    let json: serde_json::Value =
        serde_json::from_str(&findings_json(Some(root.path())).expect("JSON"))
            .expect("finding array");
    let array = json.as_array().expect("no wrapper migration");
    let emitted = array
        .iter()
        .find(|value| value["label"] == "security_profile")
        .expect("same finding");
    assert_eq!(emitted["detail"], finding.detail);
    assert_eq!(emitted["error"]["code"], "KELD-GUARD005");
}

#[cfg(windows)]
#[test]
fn canonical_windows_profile_diagnostics_match_ordinary_cli_paths() {
    let root = tempfile::tempdir().expect("owned project");
    let ordinary = root.path();
    let canonical = ordinary.canonicalize().expect("canonical project");
    for bytes in [r#"{"profile":null}"#, r#"{"profile":"legacy"}"#, "{}"] {
        std::fs::write(ordinary.join("keld.permissions.jsonc"), bytes).expect("policy");
        assert_eq!(profile(ordinary), profile(&canonical));
    }
}

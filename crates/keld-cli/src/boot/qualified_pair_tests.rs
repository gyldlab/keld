//! D1 actual filesystem admission of selected policy and exact Host/package pair.
use std::fs;

use sha2::{Digest as _, Sha256};

use super::tests::{fixture, fixture_dev_root, graph_fixture};
use super::{PERMISSIONS_FILE, stage_dev_boot, stage_dev_boot_with_approved_graph};

const POLICY: &[u8] =
    br#"{"profile":"legacy","windows":{"editor":{"web":{"csp":"original-editor-offline"}}}}"#;

fn qualified_fixture_host() -> [u8; 32] {
    // Independent fixture contract; do not hash the possibly mutated actual Host.
    Sha256::digest(b"developer-host-bytes").into()
}

#[test]
fn ordinary_and_unqualified_graph_staging_refuse_selected_policy_before_stage_creation() {
    let (_owned, project, host) = fixture();
    let approved = graph_fixture(&project);
    fs::write(project.join(PERMISSIONS_FILE), POLICY).expect("explicit policy");
    for result in [
        stage_dev_boot(&project, &host),
        stage_dev_boot_with_approved_graph(&project, &host, &approved, None),
    ] {
        let error = result.expect_err("no pair qualification cannot stage selected CSP");
        assert!(error.to_string().contains("KELD-CLI-047"), "{error}");
        assert!(
            error.to_string().contains("qualified Host/package pair"),
            "{error}"
        );
        assert!(
            !fixture_dev_root(&project).exists(),
            "rejection precedes namespace/copy"
        );
    }
}

#[test]
fn exact_qualified_pair_stages_same_policy_and_same_copied_host_bytes() {
    let (_owned, project, host) = fixture();
    let approved = graph_fixture(&project);
    fs::write(project.join(PERMISSIONS_FILE), POLICY).expect("explicit policy");
    let stage = stage_dev_boot_with_approved_graph(
        &project,
        &host,
        &approved,
        Some(qualified_fixture_host()),
    )
    .expect("exact independent tuple");
    assert_eq!(
        fs::read(stage.host()).expect("actual copied host"),
        b"developer-host-bytes"
    );
    assert_eq!(
        fs::read(stage.root().join(PERMISSIONS_FILE)).expect("actual staged policy"),
        POLICY
    );
    let descriptor: serde_json::Value = serde_json::from_slice(
        &fs::read(stage.root().join("keld.boot.json")).expect("published descriptor"),
    )
    .expect("JSON");
    assert_eq!(descriptor["schema"], 5);
    assert_eq!(
        descriptor["permissions"]["content_sha256"],
        format!("sha256:{:x}", Sha256::digest(POLICY))
    );
}

#[test]
fn old_host_bytes_refuse_and_existing_stage_guard_removes_the_failed_launch() {
    let (_owned, project, host) = fixture();
    let approved = graph_fixture(&project);
    fs::write(project.join(PERMISSIONS_FILE), POLICY).expect("policy held fixed");
    fs::write(&host, b"old-host-without-mandatory-resource-policy").expect("only Host changed");
    let error = stage_dev_boot_with_approved_graph(
        &project,
        &host,
        &approved,
        Some(qualified_fixture_host()),
    )
    .expect_err("old Host must not accept the qualified providers");
    assert!(
        error.to_string().contains("copied Host bytes differ"),
        "{error}"
    );
    let launches = fixture_dev_root(&project);
    assert!(launches.is_dir(), "copy reached actual private namespace");
    assert!(
        fs::read_dir(launches)
            .expect("inspect product cleanup")
            .next()
            .is_none(),
        "failed launch removed before fixture emergency cleanup"
    );
}

#[test]
fn qualified_host_never_bypasses_any_of_the_three_provider_fingerprints() {
    for relative in [
        "node_modules/electron/package.json",
        "node_modules/@keld/api/package.json",
        "node_modules/@keld/kipc/package.json",
    ] {
        let (_owned, project, host) = fixture();
        let approved = graph_fixture(&project);
        fs::write(project.join(PERMISSIONS_FILE), POLICY).expect("policy fixed");
        fs::write(project.join(relative), b"{\"name\":\"changed-provider\"}")
            .expect("only selected provider changed");
        let error = stage_dev_boot_with_approved_graph(
            &project,
            &host,
            &approved,
            Some(qualified_fixture_host()),
        )
        .expect_err("same Host does not approve changed package bytes");
        assert!(
            error.to_string().contains("approved package artifact"),
            "{relative}: {error}"
        );
        assert!(
            !fixture_dev_root(&project).exists(),
            "provider refusal precedes namespace"
        );
    }
}

#[test]
fn selected_policy_never_silently_downgrades_strict_to_legacy() {
    let (_owned, project, host) = fixture();
    let approved = graph_fixture(&project);
    let strict = std::str::from_utf8(POLICY)
        .expect("ASCII fixture")
        .replace("legacy", "strict");
    fs::write(project.join(PERMISSIONS_FILE), strict).expect("explicit strict request");
    let error = stage_dev_boot_with_approved_graph(
        &project,
        &host,
        &approved,
        Some(qualified_fixture_host()),
    )
    .expect_err("exact Host cannot downgrade strict for original resources");
    assert!(
        error.to_string().contains("explicit Legacy facade"),
        "{error}"
    );
    assert!(!fixture_dev_root(&project).exists());
}

//! Invalid project-source policy through the stock producer and shipping host.
//! Native absence uses the existing window/child/session observers plus ledger;
//! it does not claim exhaustive transient kernel listener-descriptor history.

use super::observer::Observation;
use super::project::{PublicFsProject, SENTINEL};
use crate::boot_admission::assert_invalid_stage_is_resource_free;
use crate::support::admission::NativeAbsenceWatcher;
use std::fs;

#[test]
fn malformed_project_policy_is_refused_before_stage_creation() {
    // GH #588: the compiler now validates the captured policy before it creates
    // any stage. Requiring a malformed *source* to launch the host would turn a
    // stronger early refusal into a test failure. Host-side malformed-policy
    // refusal remains independently exercised by boot_admission's InvalidPolicy
    // matrix, which mutates its owned stage and maintains native absence proof.
    let bytes = b"{nope}\n";
    let fixture = PublicFsProject::with_project_policy_source(bytes);
    let project = fixture.project_root();
    // All other required compiler inputs are real and valid: removing only
    // the permissions parser must stage this malformed source and fail the
    // negative control, not merely hit an unrelated missing-entry error.
    fs::write(project.join("src/main.ts"), b"export {};\n").expect("owned valid main");
    fs::write(
        project.join("index.html"),
        b"<!doctype html><title>fixture</title>\n",
    )
    .expect("owned valid renderer");
    let stage_parent = project.join(".keld/dev");
    assert!(!stage_parent.exists(), "no pre-existing stage parent");
    let error = keld_cli::boot::stage_dev_boot(
        project,
        std::path::Path::new(env!("CARGO_BIN_EXE_keld-host")),
    )
    .expect_err("malformed project policy must fail in the producer before staging");
    let detail = error.to_string();
    for required in [
        "KELD-CLI-047",
        "permissions profile",
        "KELD-GUARD005",
        "Fix the JSON or remove duplicate object keys",
    ] {
        assert!(detail.contains(required), "missing {required}: {detail}");
    }
    assert!(
        matches!(
            fs::symlink_metadata(project.join(".keld")),
            Err(ref err) if err.kind() == std::io::ErrorKind::NotFound
        ),
        "early refusal must not even create the dev-stage parent"
    );
    assert_eq!(
        fs::read(project.join("keld.permissions.jsonc")).expect("owned source bytes"),
        bytes,
        "the rejected source must not be rewritten into an accepted policy"
    );
    assert_eq!(
        fs::read(&fixture.outside).expect("independent denied-path sentinel"),
        SENTINEL,
        "early refusal changed an outside filesystem target"
    );
}

#[test]
fn relative_project_scope_is_resource_free_and_removes_exact_stage() {
    assert_invalid_source(
        br#"{"app":{"fs":{"read":["relative/**"]}}}"#,
        "relative-project-scope",
        "KELD-NATIVE-008",
        &[
            "Repair the absolute scope and start a fresh session.",
            "Correct the filesystem scopes in keld.permissions.jsonc, rebuild the staged boot artifact, and relaunch.",
        ],
    );
}

fn assert_invalid_source(bytes: &[u8], case: &str, code: &str, fixes: &[&str]) {
    let fixture = PublicFsProject::with_project_policy_source(bytes);
    let observation = Observation::bind();
    let prepared = fixture.prepare(0, observation.port);
    // prepare checks exact stock-staged bytes, descriptor digest and host image.
    // Neither policy nor descriptor is changed after this producer capture.
    let stage_root = prepared.stage.root().to_owned();
    let watcher = NativeAbsenceWatcher::compile(fixture.owned_root());
    let controls = tempfile::Builder::new()
        .prefix("kel140-policy-")
        .tempdir_in("/tmp")
        .expect("short owned invalid-startup control namespace");
    let output = assert_invalid_stage_is_resource_free(
        &prepared.stage,
        &watcher,
        &controls.path().join("control.sock"),
        case,
        code,
        None,
        true,
    );
    assert_eq!(output.status.code(), Some(1), "{case}: {output:?}");
    let stderr = std::str::from_utf8(&output.stderr).expect("typed host error UTF-8");
    for fix in fixes {
        assert!(
            stderr.contains(fix),
            "{case}: missing full remediation: {stderr}"
        );
    }
    assert!(
        matches!(
            observation.reports.try_recv(),
            Err(std::sync::mpsc::TryRecvError::Empty)
        ),
        "{case}: app or page reported before policy preflight",
    );
    assert!(
        matches!(fs::symlink_metadata(&stage_root), Err(error) if error.kind() == std::io::ErrorKind::NotFound),
        "{case}: exact nonce survived shipping host failure before fixture Drop",
    );
    // Prepared stage and project TempDir still exist: emergency fixture Drop
    // cannot erase a leaked nonce before this product cleanup assertion.
    eprintln!(
        "KELD_KEL140_POLICY_STARTUP case={case} code={code} stage_gone=true fixture_drop_pending=true"
    );
}

//! Run the B-3 generic source-only assertions in ordinary Cargo/nextest CI.
//! Native Pack/CLI acceptance remains a separate exact-artifact Mac gate.

use std::path::Path;
use std::process::Command;

#[test]
fn minimal_app_source_verifier_is_discovered_by_workspace_ci() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("gh1404-minimal-app");
    let output = Command::new("node")
        .args(["--test", "verify.test.mjs"])
        .current_dir(fixture)
        .env_remove("KELD_B3_PACK_RLIB")
        .env_remove("KELD_B3_CLI")
        .env_remove("KELD_B3_NODE_BIN")
        .output()
        .expect("Node 20+ must be available to CI source-verifier tests");
    assert!(
        output.status.success(),
        "B-3 source verifier failed: status={}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

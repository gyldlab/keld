//! Run the B-3 generic source-only assertions in ordinary Cargo/nextest CI.
//! Native Pack/CLI acceptance remains a separate exact-artifact Mac gate.

use std::path::Path;
use std::process::Command;

const REQUIRED_CASES: [&str; 16] = [
    "build selection and executable main retain inspected source identities",
    "source verifier rejects active-field decoys and inert entry",
    "committed manifest and npm lock v3 have one exact first-party alias",
    "a changed locked alias is detected independently of the app source",
    "a first-party SDK selector redirected in BOTH manifests is refused",
    "unreviewed npm lifecycle hooks and extra locked providers are refused",
    "package, lock and preload content bind to exact candidate SHA-256 bytes",
    "commented capture-root decoys cannot satisfy effective resource selection",
    "Pack's declared capture roots retain root graph inputs and future provider install",
    "real Pack graph transaction copies root inputs before intentional pre-sign refusal",
    "every pending executable/provider pin is unambiguously invalid",
    "comment-only stale sentinel text cannot hide an active valid digest",
    "the installed graph has exactly one trusted offline window policy",
    "the selected generic renderer is nested and matches the real app source",
    "the real application entry parses without evaluating an unqualified provider",
    "GH-1404 shipping CLI refuses missing provider approval before publishing",
];

/// Require the exact fixture case inventory, not merely Node's process exit.
/// Node returns success if an inherited --test-skip-pattern skips the whole file,
/// even though zero source assertions execute.
fn require_source_case_census(tap: &str) -> Result<(), String> {
    let lines: Vec<&str> = tap.lines().collect();
    if lines.first() != Some(&"TAP version 13") {
        return Err("missing Node TAP v13 test report".into());
    }
    let reported: Vec<&str> = lines
        .iter()
        .copied()
        .filter(|line| line.starts_with("ok ") || line.starts_with("not ok "))
        .collect();
    if reported.len() != REQUIRED_CASES.len() {
        return Err(format!(
            "expected {} named source cases, observed {}",
            REQUIRED_CASES.len(),
            reported.len()
        ));
    }
    for (index, name) in REQUIRED_CASES.iter().enumerate() {
        let expected = format!("ok {} - {name}", index + 1);
        // Only the real Mac Pack/CLI checks may skip in source-only CI.
        let needs_skip = index == 9 || index == 15;
        let valid = if needs_skip {
            reported[index].starts_with(&format!("{expected} # SKIP "))
        } else {
            reported[index] == expected
        };
        if !valid {
            return Err(format!(
                "source case {} is missing, renamed, failed, or unexpectedly skipped: {}",
                index + 1,
                reported[index]
            ));
        }
    }
    for required in [
        "1..16",
        "# tests 16",
        "# suites 0",
        "# pass 14",
        "# fail 0",
        "# cancelled 0",
        "# skipped 2",
        "# todo 0",
    ] {
        if lines.iter().filter(|line| **line == required).count() != 1 {
            return Err(format!("missing or repeated exact TAP summary: {required}"));
        }
    }
    Ok(())
}

#[test]
fn minimal_app_source_verifier_is_discovered_by_workspace_ci() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("gh1404-minimal-app");
    let output = Command::new("node")
        .args(["--test", "--test-reporter=tap", "verify.test.mjs"])
        .current_dir(fixture)
        // No inherited flags may suppress source tests or enable the
        // fixture's internal mutation-mode skips. The two optional native
        // checks are deliberately disabled only in this Cargo source test.
        .env_remove("NODE_OPTIONS")
        .env_remove("NODE_TEST_CONTEXT")
        .env_remove("KELD_B3_MUTANT")
        .env_remove("KELD_B3_PACK_RLIB")
        .env_remove("KELD_B3_CLI")
        .env_remove("KELD_B3_NODE_BIN")
        .output()
        .expect("Node 20+ must be available to CI source-verifier tests");
    let stdout = String::from_utf8(output.stdout).expect("Node TAP must be UTF-8");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "B-3 source verifier failed: status={}\nstdout:\n{stdout}\nstderr:\n{stderr}",
        output.status
    );
    require_source_case_census(&stdout).unwrap_or_else(|reason| {
        panic!("B-3 source test census failed: {reason}\nstdout:\n{stdout}\nstderr:\n{stderr}")
    });
}

#[test]
fn file_level_or_skipped_source_suite_cannot_satisfy_ci_census() {
    // Exact false-green from Node when inherited flags skip verify.test.mjs.
    let empty_suite = "TAP version 13\n1..1\n# Subtest: verify.test.mjs\nok 1 - verify.test.mjs\n# tests 1\n# pass 1\n# fail 0\n# skipped 0\n";
    assert!(require_source_case_census(empty_suite).is_err());
    // A superficially correct summary without the named assertions also fails.
    let false_summary = "TAP version 13\n1..16\n# tests 16\n# suites 0\n# pass 14\n# fail 0\n# cancelled 0\n# skipped 2\n# todo 0\n";
    assert!(require_source_case_census(false_summary).is_err());
}

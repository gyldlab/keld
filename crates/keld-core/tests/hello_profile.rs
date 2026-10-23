//! Explicit profiles must not escape through the direct diagnostic-session API.

use std::path::Path;
use std::time::Duration;

use keld_core::HostOwnedHelloSession;
use keld_runtime::RestartPolicy;

const ENTRY_MARKER: &str = "KEL78_DIAGNOSTIC_ENTRY_EXECUTED";

#[allow(clippy::expect_used)] // This integration-test helper owns assertion-only fixture creation.
fn write_entry(root: &Path) -> std::path::PathBuf {
    let entry = root.join("main.ts");
    let marker = serde_json::to_string(&root.join("executed.txt")).expect("owned marker path");
    std::fs::write(
        &entry,
        format!(
            // The interval holds this disposable child alive for its owner;
            // it never synchronizes an assertion or decides a test outcome.
            "setInterval(() => {{}}, 60000);\nawait Bun.write({marker}, 'executed');\nconsole.log('{ENTRY_MARKER}');\n"
        ),
    )
    .expect("write owned diagnostic entry");
    entry
}

#[allow(clippy::expect_used, clippy::panic)] // Test-only negative oracle; cleanup precedes failure.
fn assert_profile_refused(profile: &str) {
    let temp = tempfile::tempdir().expect("owned diagnostic project");
    let root = temp.path();
    let entry = write_entry(root);
    std::fs::write(
        root.join("keld.permissions.jsonc"),
        format!(r#"{{"profile":"{profile}"}}"#),
    )
    .expect("explicit project profile");
    match HostOwnedHelloSession::start_with_stdout_markers(
        root,
        entry,
        RestartPolicy::default(),
        &[ENTRY_MARKER],
    ) {
        Err(error) => {
            let detail = error.to_string();
            assert!(detail.contains("KELD-CORE-035"), "{detail}");
            assert!(detail.contains("profile"), "{detail}");
            assert!(
                !root.join("executed.txt").exists(),
                "refused diagnostic must not execute the application"
            );
        }
        Ok(session) => {
            let observed = session.wait_until_output_contains(ENTRY_MARKER, Duration::from_secs(5));
            let executed = std::fs::read(root.join("executed.txt"));
            let cleanup = session.shutdown();
            assert!(
                observed.is_ok(),
                "unexpected fixture startup failure: {observed:?}"
            );
            assert_eq!(
                executed.expect("independent application side effect"),
                b"executed"
            );
            cleanup.expect("diagnostic baseline child and listener reaped");
            panic!("explicit {profile} executed through the diagnostic path instead of refusing");
        }
    }
}

#[test]
fn explicit_strict_refuses_before_diagnostic_entry_execution() {
    assert_profile_refused("strict");
}

#[test]
fn explicit_legacy_refuses_before_diagnostic_entry_execution() {
    assert_profile_refused("legacy");
}

#[test]
fn profile_free_diagnostic_still_executes_and_reaps_its_owned_child() {
    let temp = tempfile::tempdir().expect("owned diagnostic control");
    let entry = write_entry(temp.path());
    let session = HostOwnedHelloSession::start_with_stdout_markers(
        temp.path(),
        entry,
        RestartPolicy::default(),
        &[ENTRY_MARKER],
    )
    .expect("existing missing-manifest diagnostic control");
    let observed = session.wait_until_output_contains(ENTRY_MARKER, Duration::from_secs(5));
    let executed = std::fs::read(temp.path().join("executed.txt"));
    let cleanup = session.shutdown();
    observed.expect("real Bun entry marker");
    assert_eq!(executed.expect("independent owned marker"), b"executed");
    cleanup.expect("owned diagnostic cleanup");
}

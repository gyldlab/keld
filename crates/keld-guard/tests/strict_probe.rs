//! KEL-78 T1: the synthetic probe binary must not claim OS containment.

#![allow(clippy::expect_used, clippy::panic)]

use std::process::Command;

#[test]
fn keld_strict_probe_prints_uncontained_direct_net() {
    let exe = env!("CARGO_BIN_EXE_keld-strict-probe");
    let output = Command::new(exe).output().expect("spawn keld-strict-probe");
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("utf8");
    assert!(
        stdout.contains("\"contained\":false"),
        "probe must not claim containment: {stdout}"
    );
    assert!(stdout.contains("\"probe\":\"direct_network\""), "{stdout}");
    assert!(
        stdout.contains("\"oracle\":\"not_os_deny\""),
        "direct net on this unsandboxed process must not be os_deny: {stdout}"
    );
}

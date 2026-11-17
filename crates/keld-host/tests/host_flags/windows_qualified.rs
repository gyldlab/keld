//! Native Host executable refusals before installed admission exists.
//!
//! These subprocess tests observe the actual qualified Host entry point in a
//! Cargo-built binary. They cannot prove signed/installed release admission or
//! cryptographic catalog authorization, which is not yet approved or implemented.

use std::ffi::OsString;
use std::fs;
use std::process::{Command, Output};

const ROLE: &str = "--keld-install-initial-v3";

fn host(args: &[OsString], dev_lease: bool) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_keld-host"));
    command.args(args);
    if dev_lease {
        command.env("KELD_DEV_LEASE", "1");
    } else {
        command.env_remove("KELD_DEV_LEASE");
    }
    command.output().expect("execute the shipping Host binary")
}

fn refusal(output: &Output, code: i32, marker: &str) {
    assert_eq!(output.status.code(), Some(code), "{output:?}");
    let stderr = std::str::from_utf8(&output.stderr).expect("Host refusal uses UTF-8");
    assert!(stderr.contains(marker), "{stderr}");
    assert!(output.stdout.is_empty(), "no install receipt: {output:?}");
    assert!(!stderr.contains("WINDOWS_INITIAL_BASELINE_COMMITTED"));
}

#[test]
fn untrusted_initial_install_data_is_refused_by_the_real_host_before_publication() {
    let root = tempfile::tempdir().expect("isolated untrusted request root");
    let request = root.path().join("initial-install-request.json");
    let args = [OsString::from(ROLE), request.as_os_str().to_os_string()];
    // An absent request must receive the same authority refusal as a present
    // request, without creating any filesystem state.
    refusal(&host(&args, false), 1, "KELD-HOST-INITIAL-002");
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);

    // Each string is a hostile *claim*. The expected refusal does not prove
    // the Host has verified a signature, package or selected installation.
    for data in [
        br#"{"catalog_signature":"forged","catalog":"untrusted"}"#.as_slice(),
        br#"{"publisher_scope":"wrong-signer","app_id":"wrong.app"}"#,
        br#"{"package":"other-app","version":"99.0.0"}"#,
        br#"{"manifest":"tampered","canonical_content":"changed"}"#,
        br#"{"install_root":"C:/substituted","update_root":"C:/outside"}"#,
    ] {
        fs::write(&request, data).expect("write hostile fixture");
        refusal(&host(&args, false), 1, "KELD-HOST-INITIAL-002");
        assert_eq!(fs::read(&request).unwrap(), data);
        assert_eq!(
            fs::read_dir(root.path()).unwrap().count(),
            1,
            "failed installation must not create artifacts"
        );
    }
}

#[test]
fn qualified_host_refuses_ambient_dev_lease_and_diagnostic_roles() {
    for role in [
        "--hello",
        "--keld-windows-dev-stage-cleanup-v1",
        "--keld-profile-test-root-probe-v1",
        "--allow-unsigned",
    ] {
        refusal(
            &host(&[OsString::from(role)], false),
            2,
            "KELD-HOST-INITIAL-001",
        );
    }
    // A dev lease cannot open either permitted role, including no-flag startup.
    refusal(&host(&[], true), 2, "KELD-HOST-INITIAL-001");
    let root = tempfile::tempdir().expect("test-owned request root");
    refusal(
        &host(
            &[OsString::from(ROLE), root.path().as_os_str().to_os_string()],
            true,
        ),
        2,
        "KELD-HOST-INITIAL-001",
    );
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
}

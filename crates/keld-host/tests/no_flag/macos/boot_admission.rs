use crate::support::EVENT_DEADLINE;
use crate::support::TITLE;
use crate::support::admission::{NativeAbsenceWatcher, PolicyReadFault};
use crate::support::control::wait_child_output;
use crate::support::control::wait_child_output_observing;
use crate::support::invalid_stage::{InvalidBoot, InvalidPolicy};
use crate::support::native_window::native_windows;
use crate::support::process::{await_process_state, session_dirs_for};
use crate::support::product::ProductFixture;
use std::fs;
use std::io::Write;
use std::os::unix::net::UnixListener;
use std::path::Path;
use std::process::{Command, Stdio};

#[test]
fn private_guardian_discriminator_without_authenticated_handoff_spawns_nothing() {
    let temp = tempfile::tempdir().expect("private-role fixture");
    let marker = temp.path().join("spawned");
    let entry = temp.path().join("entry.ts");
    fs::write(
        &entry,
        format!(
            "await Bun.write({}, 'spawned');\n",
            serde_json::to_string(&marker).expect("marker JSON")
        ),
    )
    .expect("private-role entry");
    let output = Command::new(env!("CARGO_BIN_EXE_keld-host"))
        .arg(keld_runtime::macos_guardian::SUPERVISED_GUARDIAN_ARG)
        .arg(temp.path())
        .arg("entry.ts")
        .arg("1")
        .arg("1")
        .env(
            "KELD_APP_LINK",
            "/tmp/forged#aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        )
        .stdin(Stdio::null())
        .output()
        .expect("invoke private discriminator without bootstrap");

    assert!(
        !output.status.success(),
        "forged private role became success"
    );
    assert!(!marker.exists(), "private argv discriminator spawned Bun");
    let stderr = String::from_utf8(output.stderr).expect("private-role stderr UTF-8");
    assert!(stderr.contains("KELD-RUNTIME-003"), "{stderr}");
    assert!(stderr.contains("registration bootstrap"), "{stderr}");
}

#[test]
fn invalid_dev_lease_contract_fails_before_app_resources() {
    let fixture = ProductFixture::new("invalid-dev-lease");
    for (value, expected) in [
        ("unsupported", "unsupported KELD_DEV_LEASE"),
        ("stdin-v1", "requires the CLI-owned pipe reader"),
    ] {
        let stage = fixture.stage();
        let child = Command::new(stage.host())
            .env("KELD_DEV_LEASE", value)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("launch invalid dev lease host");
        let pid = child.id();
        let output = wait_child_output(child, EVENT_DEADLINE);
        assert!(!output.status.success(), "invalid lease became success");
        let stderr = String::from_utf8(output.stderr).expect("invalid lease stderr UTF-8");
        assert!(stderr.contains("KELD-CORE-037"), "{stderr}");
        assert!(stderr.contains(expected), "{stderr}");
        assert!(native_windows(pid, TITLE).is_empty());
        assert!(
            session_dirs_for(pid).is_empty(),
            "invalid lease created an app-link session"
        );
    }
}

#[test]
fn every_invalid_boot_class_fails_before_transient_window_listener_or_bun() {
    let fixture = ProductFixture::new("invalid");
    let watcher = NativeAbsenceWatcher::compile(fixture.root.path());
    for invalid in InvalidBoot::ALL {
        let stage = fixture.stage();
        invalid.apply(stage.root(), fixture.root.path());
        assert_invalid_stage_is_resource_free(
            &stage,
            &watcher,
            &fixture
                .root
                .path()
                .join(format!("invalid-{}.sock", invalid.name())),
            invalid.name(),
            invalid.expected_code(),
            None,
        );
    }
}

#[test]
fn every_invalid_policy_class_fails_before_transient_window_listener_or_bun() {
    let fixture = ProductFixture::new("invalid-policy");
    let watcher = NativeAbsenceWatcher::compile(fixture.root.path());
    for invalid in InvalidPolicy::ALL {
        let stage = fixture.stage();
        invalid.apply(stage.root());
        assert_invalid_stage_is_resource_free(
            &stage,
            &watcher,
            &fixture
                .root
                .path()
                .join(format!("invalid-policy-{}.sock", invalid.name())),
            invalid.name(),
            invalid.expected_code(),
            None,
        );
    }
}

#[test]
fn retained_policy_read_failure_is_guard004_and_resource_free() {
    let fixture = ProductFixture::new("policy-read-failure");
    let watcher = NativeAbsenceWatcher::compile(fixture.root.path());
    let fault = PolicyReadFault::compile(fixture.root.path());
    let stage = fixture.stage();
    assert_invalid_stage_is_resource_free(
        &stage,
        &watcher,
        &fixture.root.path().join("policy-read-failure.sock"),
        "retained-read-failure",
        "KELD-GUARD004",
        Some(&fault),
    );
    assert!(
        fault.marker.exists(),
        "read fault never reached the retained permissions handle"
    );
}

fn assert_invalid_stage_is_resource_free(
    stage: &keld_cli::boot::DevBootStage,
    watcher: &NativeAbsenceWatcher,
    control_path: &Path,
    case: &str,
    expected_code: &str,
    read_fault: Option<&PolicyReadFault>,
) {
    let listener = UnixListener::bind(control_path).expect("bind invalid control observer");
    listener
        .set_nonblocking(true)
        .expect("nonblocking invalid control observer");
    let mut command = Command::new("/bin/sh");
    if let Some(fault) = read_fault {
        command
            .args([
                "-c",
                "kill -STOP $$; DYLD_INSERT_LIBRARIES=\"$2\" KELD_T2_READ_FAULT_MARKER=\"$3\" exec \"$1\"",
                "kel102-policy-read-fault",
            ])
            .arg(stage.host())
            .arg(&fault.library)
            .arg(&fault.marker);
    } else {
        command
            .args(["-c", "kill -STOP $$; exec \"$1\"", "kel96-invalid"])
            .arg(stage.host());
    }
    command
        .env("KELD_T1B_CONTROL", control_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let child = command.spawn().expect("start suspended invalid host");
    let host_pid = child.id();
    await_process_state(host_pid, 'T');
    let native = watcher.spawn(host_pid);
    let mut forbidden_control = false;
    let output = wait_child_output_observing(child, EVENT_DEADLINE, || match listener.accept() {
        Ok((mut stream, _)) => {
            forbidden_control = true;
            stream
                .write_all(b"QUIT\n")
                .expect("stop forbidden app through its owned lifecycle path");
        }
        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
        Err(error) => panic!("inspect invalid-stage control: {error}"),
    });
    assert!(!forbidden_control, "{case}: Bun entered before preflight");
    assert!(
        !output.status.success(),
        "{case}: invalid boot became success"
    );
    let native_output = native
        .wait_with_output()
        .expect("wait native absence watcher");
    assert!(
        native_output.status.success(),
        "{case}: watcher failed: {native_output:?}"
    );
    let observations = String::from_utf8(native_output.stdout).expect("watcher output UTF-8");
    assert_eq!(
        observations, "READY\nDONE\n",
        "{case}: transient resource: {observations}"
    );
    let stderr = String::from_utf8(output.stderr).expect("typed stderr UTF-8");
    assert!(stderr.contains(expected_code), "{case}: {stderr}");
    assert!(
        stderr.contains("[startup-resource-attempts listener=0 child=0 window=0]"),
        "{case}: internal pre-resource ledger was not empty: {stderr}"
    );
    let lower = stderr.to_ascii_lowercase();
    assert!(
        ["regenerate", "restore", "write", "set", "launch"]
            .iter()
            .any(|action| lower.contains(action)),
        "{case}: missing fix: {stderr}"
    );
    assert!(
        matches!(listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock),
        "{case}: Bun reached the control observer"
    );
    assert!(
        session_dirs_for(host_pid).is_empty(),
        "{case}: app-link directory remains"
    );
    let _ = fs::remove_file(control_path);
}

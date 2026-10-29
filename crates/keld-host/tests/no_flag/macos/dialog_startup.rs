//! Actual facade startup rollback with the dialog sender owner installed.
use super::{stage, visible_windows};
use crate::support::EVENT_DEADLINE;
use crate::support::control::{accept_before, parse_pid, read_control_line, wait_child_output};
use crate::support::dev_cycle::ShippingLaunchCleanup;
use crate::support::process::{await_process_gone, session_dirs_for};
use crate::support::product::ProductFixture;
use std::io::BufReader;
use std::os::unix::net::UnixListener;
use std::os::unix::process::CommandExt as _;
use std::process::{Command, Stdio};
use std::time::Instant;

#[test]
fn facade_engine_failure_with_dialog_owner_finishes_startup_cleanup() {
    let fixture = ProductFixture::new("gh478-dialog-startup-rollback");
    // This shared stage uses initialWindow:none and a digest-verified dialog
    // grant, so Core has both the facade registry and the dialog session.
    let staged = stage(&fixture);
    let stage_root = staged.root().to_path_buf();
    let socket = fixture.root.path().join("startup-control.sock");
    let listener = UnixListener::bind(&socket).expect("private observation listener");
    listener.set_nonblocking(true).expect("bounded accept");
    let child = Command::new(staged.host())
        .process_group(0)
        .current_dir(staged.root())
        .env("KELD_T1B_CONTROL", &socket)
        .env("KELD_DEV_LEASE", "stdin-v1")
        .env("KELD_TEST_DIALOG_STARTUP_FAILURE", "engine")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("actual facade Host");
    let mut cleanup = ShippingLaunchCleanup::new(child);
    let host = cleanup.cli.as_mut().expect("Host custody");
    let host_pid = host.id();
    let _lease = host
        .stdin
        .take()
        .expect("keep dev lease live throughout rollback");
    let control = accept_before(&listener, Instant::now() + EVENT_DEADLINE);
    control
        .set_read_timeout(Some(EVENT_DEADLINE))
        .expect("bounded role observation");
    let mut control = BufReader::new(control);
    let bound = read_control_line(&mut control);
    let mut fields = bound.split_whitespace();
    assert_eq!(fields.next(), Some("ROLE_BOUND"), "{bound}");
    let bun_pid = parse_pid(fields.next(), &bound);
    let guardian_pid = parse_pid(fields.next(), &bound);
    eprintln!("GH478_STARTUP_ROLLBACK host={host_pid} guardian={guardian_pid} bun={bun_pid}");
    // The owned Host's process death also invokes Guardian's existing group
    // cleanup. The helper watchdog kills/reaps the exact child on baseline hang.
    let output = wait_child_output(cleanup.release(), EVENT_DEADLINE);
    let stderr = String::from_utf8(output.stderr).expect("Host error UTF-8");
    assert!(!output.status.success(), "constructor failure was hidden");
    assert!(
        stderr.contains("macOS profile store initialization"),
        "{stderr}"
    );
    assert!(
        stderr.contains("controlled dialog startup engine failure"),
        "{stderr}"
    );
    await_process_gone(guardian_pid);
    await_process_gone(bun_pid);
    assert!(
        visible_windows(host_pid).is_empty(),
        "startup left native windows"
    );
    assert!(
        session_dirs_for(host_pid).is_empty(),
        "Host app-link directory survived"
    );
    assert!(
        session_dirs_for(guardian_pid).is_empty(),
        "Guardian app-link directory survived"
    );
    assert!(
        !stage_root.exists(),
        "failed dev stage survived proved cleanup"
    );
    // Cleanup must leave the machine able to launch a fresh shipping Host.
    // This observes real Open/Save cancellation, a healthy Message and clean Quit.
    super::run_real_host_files(false, 2, false);
}

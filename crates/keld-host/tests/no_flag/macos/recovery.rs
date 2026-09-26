use crate::support::EVENT_DEADLINE;
use crate::support::MARKER;
use crate::support::control::accept_before;
use crate::support::process::await_process_gone;
use crate::support::product::ProductFixture;
use crate::support::recovery_cycle::RecoveryCycle;
use std::io::Write;
use std::os::unix::net::UnixListener;
use std::process::Command;
use std::process::Stdio;
use std::time::Instant;

#[test]
fn no_flag_host_recovers_a_fresh_generation_in_the_same_native_window() {
    let fixture = ProductFixture::new("t3-generation-recovery");
    let mut recovery = RecoveryCycle::launch(&fixture, "recovery-quit");
    let first = recovery.crash_and_recover();
    let second = recovery.current_evidence();
    recovery.quit_and_expect_success();

    eprintln!(
        "KEL96_T3_EVIDENCE host={} window={} guardian={} first_bun={} second_bun={} old_link={} new_link={} marker={MARKER}",
        recovery.host_pid,
        recovery.window[0],
        first.guardian_pid,
        first.bun_pid,
        second.bun_pid,
        first.app_link,
        second.app_link,
    );
}

#[test]
fn recovered_generation_is_the_target_of_host_and_guardian_death_cleanup() {
    let fixture = ProductFixture::new("t3-death-after-recovery");

    let mut host_death = RecoveryCycle::launch(&fixture, "host-death-g2");
    host_death.crash_and_recover();
    host_death.kill_host_and_expect_current_group_reaped();

    let mut guardian_death = RecoveryCycle::launch(&fixture, "guardian-death-g2");
    guardian_death.crash_and_recover();
    guardian_death.kill_guardian_and_expect_current_group_reaped();
}

#[test]
fn live_child_link_loss_restarts_through_the_generation_owner() {
    let fixture = ProductFixture::new("t3-link-loss");
    let mut cycle = RecoveryCycle::launch(&fixture, "link-loss");
    cycle.close_link_and_recover();
    cycle.quit_and_expect_success();
}

#[test]
fn third_generation_crash_trips_breaker_without_a_fourth_generation() {
    let fixture = ProductFixture::new("t3-crash-loop");
    let mut cycle = RecoveryCycle::launch(&fixture, "crash-loop");
    cycle.crash_and_recover();
    cycle.crash_and_recover();
    {
        let current = cycle.current.as_mut().expect("third generation");
        current
            .writer
            .write_all(b"CRASH_ACKED\n")
            .expect("request acknowledged threshold crash");
        current.expect_line("CRASH_ACK");
    }
    let third = cycle.current_evidence();
    let output = cycle.wait_host();
    assert!(
        !output.status.success(),
        "acknowledged crash loop became success: status={:?}",
        output.status
    );
    let stderr = String::from_utf8(output.stderr).expect("crash-loop stderr UTF-8");
    eprintln!(
        "KEL260_BREAKER host={} guardian={} bun={} descendant={} status={} stderr={stderr}",
        cycle.host_pid, third.guardian_pid, third.bun_pid, third.descendant_pid, output.status
    );
    assert!(stderr.contains("KELD-CORE-033"), "{stderr}");
    assert!(stderr.contains("KELD-RUNTIME-002"), "{stderr}");
    assert!(
        matches!(cycle.listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock),
        "crash-loop threshold provisioned a fourth generation"
    );
    cycle.assert_current_group_gone();
    assert!(
        !third.endpoint.exists(),
        "crash loop left the app-link endpoint"
    );

    let mut relaunched = RecoveryCycle::launch(&fixture, "after-crash-loop");
    relaunched.quit_and_expect_success();
}

#[test]
fn recovery_wait_preserves_the_cli_lease_until_child_exit() {
    // The control byte orders the independent pipe observation after entry
    // into the wait. No child exit or elapsed delay can stand in for that edge.
    const PROBE: &str = r#"
import os
import socket
import sys

with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as control:
    control.connect(sys.argv[1])
    assert control.recv(1) == b"P"
    os.set_blocking(0, False)
    try:
        observed = os.read(0, 1)
    except BlockingIOError:
        print("LEASE_LIVE")
        sys.exit(0)
    assert observed == b"", repr(observed)
    print("LEASE_EOF")
    sys.exit(17)
"#;
    let root = tempfile::tempdir().expect("lease probe root");
    // Include deliberate EOF and a healthy follow-up to check both the probe
    // and resource reuse independently of the recovery helper's implementation.
    for (index, release_lease) in [false, true, false].into_iter().enumerate() {
        let path = root.path().join(format!("lease-{index}.sock"));
        let listener = UnixListener::bind(&path).expect("lease probe control");
        listener
            .set_nonblocking(true)
            .expect("nonblocking lease probe control");
        let mut child = Command::new("/usr/bin/python3")
            .args(["-c", PROBE])
            .arg(&path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("start independent lease probe");
        let host_pid = child.id();
        let mut cycle = RecoveryCycle {
            dev_lease_writer: child.stdin.take(),
            host: Some(child),
            host_pid,
            listener,
            window: Vec::new(),
            current: None,
            process_groups: Vec::new(),
        };
        let mut control = Some(accept_before(
            &cycle.listener,
            Instant::now() + EVENT_DEADLINE,
        ));
        if release_lease {
            drop(cycle.dev_lease_writer.take());
        }
        let output = cycle.wait_host_observing(|| {
            if let Some(mut control) = control.take() {
                control.write_all(b"P").expect("request lease observation");
            }
        });
        assert_eq!(
            output.status.code(),
            Some(if release_lease { 17 } else { 0 }),
            "wait changed the requested lease state: {output:?}"
        );
        assert_eq!(
            output.stdout,
            if release_lease {
                b"LEASE_EOF\n".as_slice()
            } else {
                b"LEASE_LIVE\n".as_slice()
            }
        );
        assert!(output.stderr.is_empty(), "{output:?}");
        await_process_gone(host_pid);
    }
}

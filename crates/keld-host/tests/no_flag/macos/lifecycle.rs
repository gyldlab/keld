use crate::support::TITLE;
use crate::support::native_window::native_windows;
use crate::support::process::await_process_gone;
use crate::support::process::kill_pid;
use crate::support::product::{GuardianPidStage, LiveCycle, ProductFixture, guardian_pid_stage};
use std::io::Write;
use std::os::unix::process::ExitStatusExt as _;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::process::Output;
use std::time::{Duration, Instant};

#[test]
fn no_flag_host_owns_real_window_session_death_reap_and_ordered_quit() {
    let fixture = ProductFixture::new("product");

    let mut killed = fixture.launch_cycle("host-death");
    killed.assert_live_product();
    let host_status = killed
        .host
        .as_mut()
        .expect("live host")
        .kill()
        .and_then(|()| killed.host.as_mut().expect("live host").wait())
        .expect("SIGKILL only the no-flag host");
    assert_eq!(
        host_status.signal(),
        Some(9),
        "host-only death must be SIGKILL"
    );
    killed.host.take();
    killed.expect_line("LINK_EOF");
    await_process_gone(killed.bun_pid);
    await_process_gone(killed.descendant_pid);
    await_process_gone(killed.guardian_pid);
    assert!(
        !killed.session_dir.exists(),
        "host death left the app-link locator"
    );
    killed.group_gone = true;

    let mut self_terminated = fixture.launch_cycle("self-termination");
    self_terminated.assert_live_product();
    self_terminated
        .control_writer
        .write_all(b"EXIT0\n")
        .expect("request unrequested status-zero Bun exit");
    let output = self_terminated.wait_host();
    assert!(
        !output.status.success(),
        "unrequested status-zero Bun exit became host success"
    );
    let stderr = String::from_utf8(output.stderr).expect("self-termination stderr UTF-8");
    assert!(stderr.contains("KELD-CORE-033"), "{stderr}");
    assert!(stderr.contains("KELD-RUNTIME-012"), "{stderr}");
    await_process_gone(self_terminated.bun_pid);
    await_process_gone(self_terminated.descendant_pid);
    await_process_gone(self_terminated.guardian_pid);
    assert!(
        !self_terminated.session_dir.exists(),
        "self-termination left the app-link locator"
    );
    self_terminated.group_gone = true;

    let mut guardian_failed = fixture.launch_cycle("guardian-failure");
    guardian_failed.assert_live_product();
    kill_pid(guardian_failed.guardian_pid);
    let output = guardian_failed.wait_host();
    assert!(
        !output.status.success(),
        "guardian death became host success"
    );
    let stderr = String::from_utf8(output.stderr).expect("guardian-failure stderr UTF-8");
    assert!(stderr.contains("KELD-CORE-033"), "{stderr}");
    assert!(stderr.contains("KELD-RUNTIME-013"), "{stderr}");
    await_process_gone(guardian_failed.bun_pid);
    await_process_gone(guardian_failed.descendant_pid);
    await_process_gone(guardian_failed.guardian_pid);
    assert!(
        native_windows(guardian_failed.host_pid, TITLE).is_empty(),
        "guardian failure left a native window"
    );
    guardian_failed.group_gone = true;

    let mut orderly = fixture.launch_cycle("relaunch-orderly");
    orderly.assert_live_product();
    let guardian_pid = orderly.guardian_pid;
    orderly
        .control_writer
        .write_all(b"QUIT\n")
        .expect("request fixture app.quit");
    orderly.expect_line("QUIT_REPLY");
    orderly.expect_line("LINK_EOF");
    let output = wait_orderly_host_with_native_guardian_receipt(&mut orderly, guardian_pid);
    assert!(
        output.status.success(),
        "ordered no-flag host exit: {output:?}"
    );
    let stderr = String::from_utf8(output.stderr).expect("host stderr UTF-8");
    assert!(
        !stderr.contains("pre-alpha"),
        "no-flag product launch returned through the old banner: {stderr}"
    );
    await_process_gone(orderly.bun_pid);
    await_process_gone(orderly.descendant_pid);
    await_process_gone(orderly.guardian_pid);
    assert!(
        !orderly.session_dir.exists(),
        "Quit left the app-link locator"
    );
    assert!(
        native_windows(orderly.host_pid, TITLE).is_empty(),
        "host exit left a native window"
    );
    orderly.group_gone = true;
}

/// The Bun fixture closes its own app socket before `LINK_EOF`. Independently
/// check whether the previously parent-attested native Guardian *process*
/// has disappeared while the Host is still running, after a real Quit Reply.
/// PID absence proves the native process has gone; it does not prove a clean S
/// acknowledgment, UI Quit, or complete process-group cleanup.
fn wait_orderly_host_with_native_guardian_receipt(
    cycle: &mut LiveCycle,
    guardian_pid: u32,
) -> Output {
    let mut last_stage = GuardianPidStage::Unproved;
    let mut saw_guardian_absent = false;
    let mut next_sample = Instant::now();
    let result = catch_unwind(AssertUnwindSafe(|| {
        cycle.wait_host_observing(|| {
            if saw_guardian_absent || Instant::now() < next_sample {
                return;
            }
            last_stage = guardian_pid_stage(guardian_pid);
            if last_stage == GuardianPidStage::Absent {
                saw_guardian_absent = true;
            }
            // A kernel signal-zero probe never sends a signal, blocks on I/O,
            // or renews the original 20s Host-exit deadline.
            next_sample = Instant::now() + Duration::from_millis(100);
        })
    }));
    result.unwrap_or_else(|failure| {
        let original_host_deadline = failure
            .downcast_ref::<String>()
            .is_some_and(|text| text.starts_with("child exceeded exit/output deadline"))
            || failure
                .downcast_ref::<&str>()
                .is_some_and(|text| text.starts_with("child exceeded exit/output deadline"));
        if original_host_deadline {
            let stage = if saw_guardian_absent {
                "guardian_kernel_pid_absent_while_host_live"
            } else if last_stage == GuardianPidStage::Present {
                "guardian_kernel_pid_last_seen_present"
            } else {
                "guardian_kernel_pid_state_unproved"
            };
            // Fixed enum only: no PID, token, filesystem path or raw Host output.
            eprintln!("KEL96_T1B_NATIVE_GUARDIAN_PHASE stage={stage}");
        }
        resume_unwind(failure)
    })
}

#[cfg(test)]
mod guardian_pid_phase_tests {
    use super::{GuardianPidStage, guardian_pid_stage};
    use crate::support::PROCESS_DEADLINE;
    use std::os::unix::process::ExitStatusExt as _;
    use std::process::{Child, Command};
    use std::thread;
    use std::time::Instant;

    struct OwnedSleep(Child);

    impl Drop for OwnedSleep {
        fn drop(&mut self) {
            // Even a failed assertion must not leave the test-owned process.
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    #[test]
    fn real_kernel_probe_requires_exact_owned_child_reap() {
        let mut child = OwnedSleep(
            Command::new("/bin/sleep")
                .arg("60")
                .spawn()
                .expect("spawn owned native process"),
        );
        let pid = child.0.id();
        let before = guardian_pid_stage(pid);
        child.0.kill().expect("terminate only the owned child");
        let deadline = Instant::now() + PROCESS_DEADLINE;
        let status = loop {
            match child.0.try_wait().expect("inspect exact test-owned child") {
                Some(status) => break status,
                None if Instant::now() < deadline => thread::yield_now(),
                None => panic!("owned native child exceeded existing process deadline"),
            }
        };
        assert_eq!(status.signal(), Some(9), "owned child must exit by SIGKILL");
        assert_eq!(before, GuardianPidStage::Present);
        assert_eq!(guardian_pid_stage(pid), GuardianPidStage::Absent);
        assert_eq!(guardian_pid_stage(0), GuardianPidStage::Unproved);
    }
}

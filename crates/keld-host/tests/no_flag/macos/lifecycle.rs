use crate::support::native_window::native_windows;
use crate::support::process::ProcessExitObserver;
use crate::support::process::await_process_gone;
use crate::support::process::kill_pid;
use crate::support::product::{GuardianPidStage, LiveCycle, ProductFixture, guardian_pid_stage};
use crate::support::{EVENT_DEADLINE, TITLE};
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
    // Bind the kernel event to the already parent-attested original Guardian
    // BEFORE Bun receives the real Quit command. This is observation only.
    // A nonchild NOTE_EXITSTATUS registration can be unavailable under the
    // kernel's permission rules. Keep the original T1B Quit acceptance alive
    // and report only an unproved diagnostic if registration was refused.
    let mut guardian_exit = ProcessExitObserver::try_arm(guardian_pid).ok();
    orderly
        .control_writer
        .write_all(b"QUIT\n")
        .expect("request fixture app.quit");
    orderly.expect_line("QUIT_REPLY");
    orderly.expect_line("LINK_EOF");
    let output = wait_orderly_host_with_native_guardian_receipt(&mut orderly, &mut guardian_exit);
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
    guardian_exit: &mut Option<ProcessExitObserver>,
) -> Output {
    let guardian_pid = cycle.guardian_pid;
    // The actual Host wait captures its own 20s deadline after pipe setup.
    // This earlier observation-only cut is conservative: a kevent64 read must
    // start AND finish before it to count as a timely original-Host witness.
    let witness_deadline = Instant::now() + EVENT_DEADLINE;
    let mut last_stage = GuardianPidStage::Unproved;
    let mut saw_guardian_absent = false;
    let mut exit_event = if guardian_exit.is_some() {
        GuardianExitEventPhase::NoEvent
    } else {
        GuardianExitEventPhase::Unproved
    };
    let mut next_sample = Instant::now();
    let result = catch_unwind(AssertUnwindSafe(|| {
        cycle.wait_host_observing(|| {
            let before_native_poll = Instant::now();
            if before_native_poll >= witness_deadline || before_native_poll < next_sample {
                return;
            }
            if exit_event == GuardianExitEventPhase::NoEvent
                && let Some(registered_exit) = guardian_exit.as_mut()
            {
                let received = registered_exit.try_poll_exit();
                // A native event delivered after the original observation cut
                // is unproved, even if it is otherwise a valid NOTE_EXITSTATUS.
                exit_event = if Instant::now() >= witness_deadline {
                    GuardianExitEventPhase::Unproved
                } else {
                    match received {
                        Ok(Some(_)) => GuardianExitEventPhase::Observed,
                        Ok(None) => GuardianExitEventPhase::NoEvent,
                        Err(_) => GuardianExitEventPhase::Unproved,
                    }
                };
            }
            if !saw_guardian_absent {
                last_stage = guardian_pid_stage(guardian_pid);
                if last_stage == GuardianPidStage::Absent {
                    saw_guardian_absent = true;
                }
            }
            // Retain the existing signal-zero receipt cadence. Neither this
            // zero-time native event poll nor the PID census renews Host time.
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
            eprintln!("KEL96_T1B_NATIVE_EXIT_EVENT stage={}", exit_event.marker());
        }
        resume_unwind(failure)
    })
}

/// Test-only observations about one pre-Quit kernel process subscription.
/// No event means no exit was observed; it does not prove the Guardian lived.
#[derive(Clone, Copy, PartialEq, Eq)]
enum GuardianExitEventPhase {
    NoEvent,
    Observed,
    Unproved,
}

impl GuardianExitEventPhase {
    const fn marker(self) -> &'static str {
        match self {
            Self::NoEvent => "guardian_exit_event_not_observed",
            Self::Observed => "guardian_exit_event_observed_before_host_deadline",
            Self::Unproved => "guardian_exit_event_unproved",
        }
    }
}

#[cfg(test)]
mod guardian_pid_phase_tests {
    use super::{GuardianPidStage, guardian_pid_stage};
    use crate::support::PROCESS_DEADLINE;
    use crate::support::process::ProcessExitObserver;
    use std::os::unix::process::ExitStatusExt as _;
    use std::process::{Child, Command, Stdio};
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

    #[test]
    fn native_kqueue_records_owned_process_exit_without_a_pid_probe() {
        // Native cat stays alive on a test-owned stdin. This is a real
        // OS process, and the observer is armed before its EOF release.
        let mut child = OwnedSleep(
            Command::new("/bin/cat")
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .expect("owned native event witness"),
        );
        let mut exit_watcher = ProcessExitObserver::arm(child.0.id());
        assert!(
            exit_watcher
                .try_poll_exit()
                .expect("initial native event check")
                .is_none(),
            "the still-running owned child cannot have exited"
        );
        drop(
            child
                .0
                .stdin
                .take()
                .expect("release exact owned native reader"),
        );
        let deadline = Instant::now() + PROCESS_DEADLINE;
        let exit_status = loop {
            if let Some(status) = exit_watcher
                .try_poll_exit()
                .expect("read native kernel exit event")
            {
                break status;
            }
            assert!(
                Instant::now() < deadline,
                "native kernel exit receipt absent before the original process deadline"
            );
            thread::yield_now();
        };
        assert!(exit_status.success(), "native status-zero event");
        assert!(
            child.0.wait().expect("reap owned native child").success(),
            "native child is reaped by its original owner"
        );
    }

    #[test]
    fn native_kqueue_retains_nonzero_exit_status_from_owned_process() {
        // The shell's builtin read waits on test-owned stdin, then exits 23.
        // A status-zero-only or NOTE_EXIT-without-STATUS implementation fails.
        let mut child = OwnedSleep(
            Command::new("/bin/sh")
                .args(["-c", "IFS= read -r line; exit 23"])
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .expect("owned native nonzero-exit witness"),
        );
        let mut exit_watcher = ProcessExitObserver::arm(child.0.id());
        assert!(
            exit_watcher
                .try_poll_exit()
                .expect("living native shell event check")
                .is_none(),
            "live shell cannot publish a terminal event"
        );
        drop(child.0.stdin.take().expect("release owned shell stdin"));
        let deadline = Instant::now() + PROCESS_DEADLINE;
        let exit_status = loop {
            if let Some(status) = exit_watcher
                .try_poll_exit()
                .expect("read actual nonzero kernel event")
            {
                break status;
            }
            assert!(
                Instant::now() < deadline,
                "nonzero native exit event absent before original process deadline"
            );
            thread::yield_now();
        };
        assert_eq!(exit_status.code(), Some(23), "kernel-reported exit code");
        assert_eq!(
            child.0.wait().expect("reap exact owned shell").code(),
            Some(23),
            "independent owned Child status must agree with kernel event"
        );
    }
}

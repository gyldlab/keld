use crate::support::EVENT_DEADLINE;
use crate::support::PROCESS_DEADLINE;
use crate::support::control::ReapOnDrop;
use crate::support::process::ProcessExitObserver;
use crate::support::process::signal_process_group;
use std::io::BufRead as _;
use std::io::BufReader;
use std::os::unix::process::CommandExt as _;
use std::os::unix::process::ExitStatusExt as _;
use std::process::Child;
use std::process::Command;
use std::process::ExitStatus;
use std::process::Stdio;
use std::time::Instant;

/// Signal a CLI's process group and wait for the CLI's own kernel exit status
/// until the caller's original deadline. A CLI that ignores or survives the
/// signal fails with the exit-census message at the deadline instead of
/// blocking forever in `Child::wait()`.
pub(crate) fn signal_group_and_await_exit(
    cli: &mut Child,
    group: u32,
    signal_name: &str,
    deadline: Instant,
) -> ExitStatus {
    // Arm before signaling, so the exit cannot be missed.
    let exit = ProcessExitObserver::arm_own_child(cli);
    signal_process_group(&format!("-{signal_name}"), group)
        .unwrap_or_else(|error| panic!("group {signal_name} failed: {error}"));
    let status = exit.wait_before(deadline);
    // The kernel already reported this exit, so reaping it cannot block.
    let reaped = cli.wait().expect("reap signaled shipping CLI");
    assert_eq!(
        reaped, status,
        "reaped CLI status differs from the kernel census"
    );
    status
}

/// A CLI stand-in in its own process group. The guard kills and reaps it on
/// every exit path, including a panic during setup or in the test body.
fn signal_stand_in(script: &str) -> ReapOnDrop {
    let mut child = ReapOnDrop(
        Command::new("/bin/sh")
            .args(["-c", script])
            .process_group(0)
            .stdout(Stdio::piped())
            .spawn()
            .expect("spawn CLI stand-in"),
    );
    // Signal only after the stand-in installed its disposition.
    let mut ready = String::new();
    BufReader::new(child.0.stdout.take().expect("stand-in stdout"))
        .read_line(&mut ready)
        .expect("stand-in ready line");
    assert_eq!(ready, "ready\n");
    child
}

#[test]
fn signaled_cli_exit_is_observed_with_its_signal() {
    let mut child = signal_stand_in("echo ready; exec /bin/sleep 600");
    let group = child.0.id();
    let status =
        signal_group_and_await_exit(&mut child.0, group, "INT", Instant::now() + EVENT_DEADLINE);
    assert_eq!(status.signal(), Some(2));
}

#[test]
fn signaled_cli_that_ignores_the_signal_fails_at_the_deadline_instead_of_hanging() {
    let mut child = signal_stand_in("trap '' INT; echo ready; exec /bin/sleep 600");
    let group = child.0.id();
    let started = Instant::now();
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        signal_group_and_await_exit(&mut child.0, group, "INT", started + PROCESS_DEADLINE)
    }));
    // Owned cleanup before the oracle: the stand-in must not outlive the test.
    drop(child);
    let panic = outcome.expect_err("an ignored signal cannot become a CLI exit");
    assert_eq!(
        panic.downcast_ref::<String>().map(String::as_str),
        Some(format!("no kernel exit status for PID {group}").as_str())
    );
    assert!(
        started.elapsed() < PROCESS_DEADLINE * 2,
        "the ignored signal was not bounded by the original deadline"
    );
}

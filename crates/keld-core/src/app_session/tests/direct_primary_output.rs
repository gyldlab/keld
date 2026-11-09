//! Actual direct-owner terminal branches; injected verdicts do not certify an OS fault.

use std::cell::Cell;
use std::io::{Read as _, Seek as _, SeekFrom, Write as _};
use std::net::{TcpListener, TcpStream};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use keld_runtime::primary::{PrimaryRoleConfig, PrimaryRoleSupervisor};
use keld_runtime::{RuntimeError, SupervisorOutcome};

use super::super::{DirectPrimaryOwner, SessionShutdownState};

const SCENARIO_ENV: &str = "KELD_CORE_TEST_DIRECT_OUTPUT_SCENARIO";
const CHILD_PORT_ENV: &str = "KELD_CORE_TEST_DIRECT_OUTPUT_PORT";
const STDOUT_SENTINEL: &str = "GH1459_RETAINED_STDOUT";
const STDERR_SENTINEL: &str = "GH1459_RETAINED_STDERR";
const CHILD_READY: &[u8] = b"GH1459_CHILD_PARKED";
const CHILD_SELECTOR: &str = "app_session::tests::direct_primary_output::supervised_output_child";
const BOUND: Duration = Duration::from_secs(20);

thread_local! {
    // Set only by the isolated test's caller, then moved into that one owner thread.
    static SHUTDOWN_FAULT: Cell<Option<&'static str>> = const { Cell::new(None) };
}

pub(in crate::app_session) fn take_shutdown_fault() -> Option<&'static str> {
    SHUTDOWN_FAULT.with(Cell::take)
}

pub(in crate::app_session) fn apply_shutdown_fault(
    fault: &mut Option<&'static str>,
    branch: &'static str,
    outcome: SupervisorOutcome,
) -> SupervisorOutcome {
    let Some(expected) = fault.take() else {
        return outcome;
    };
    assert_eq!(branch, expected, "fault reached the wrong owner branch");
    assert!(
        matches!(outcome, SupervisorOutcome::Stopped),
        "real child shutdown must finish before injecting the verdict fault: {outcome:?}"
    );
    SupervisorOutcome::Failed(RuntimeError::Lifecycle {
        phase: branch,
        source: std::io::Error::other("GH1459_CONTROLLED_SHUTDOWN_FAILURE"),
    })
}

struct ReapedProbe(Child);

impl Drop for ReapedProbe {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

fn assert_isolated_output(selector: &str, branch: &str) {
    let mut stdout = tempfile::tempfile().expect("private probe stdout");
    let mut stderr = tempfile::tempfile().expect("private probe stderr");
    let mut probe = ReapedProbe(
        Command::new(std::env::current_exe().expect("test executable"))
            .args(["--exact", selector, "--nocapture", "--test-threads=1"])
            .env(SCENARIO_ENV, branch)
            .env_remove(CHILD_PORT_ENV)
            .stdin(Stdio::null())
            .stdout(Stdio::from(stdout.try_clone().expect("probe stdout clone")))
            .stderr(Stdio::from(stderr.try_clone().expect("probe stderr clone")))
            .spawn()
            .expect("spawn exact direct-owner probe"),
    );
    let deadline = Instant::now() + BOUND;
    let status = loop {
        if let Some(status) = probe.0.try_wait().expect("probe status") {
            break status;
        }
        assert!(
            Instant::now() < deadline,
            "direct-owner probe exceeded its bound"
        );
        // Observe process completion; time only bounds an unsuccessful observation.
        std::thread::park_timeout(Duration::from_millis(10));
    };
    stdout.seek(SeekFrom::Start(0)).expect("rewind stdout");
    stderr.seek(SeekFrom::Start(0)).expect("rewind stderr");
    let mut out = String::new();
    let mut err = String::new();
    stdout.read_to_string(&mut out).expect("read probe stdout");
    stderr.read_to_string(&mut err).expect("read probe stderr");
    assert!(
        status.success(),
        "probe failed: {status}; stdout={out}; stderr={err}"
    );
    assert!(
        out.contains("GH1459_BRANCH_VERDICT_OBSERVED"),
        "selector did not exercise the branch: {out}"
    );
    assert!(
        out.contains("KELD-RUNTIME-003"),
        "owning typed error vanished: {out}"
    );
    assert!(
        out.contains("GH1459_CONTROLLED_SHUTDOWN_FAILURE"),
        "injected failure vanished: {out}"
    );
    assert!(out.contains(branch), "wrong terminal branch: {out}");
    assert_eq!(
        err.matches(STDERR_SENTINEL).count(),
        1,
        "captured stderr missing or duplicated: {err}"
    );
    assert_eq!(
        out.matches(STDOUT_SENTINEL).count(),
        1,
        "captured stdout missing or duplicated: {out}"
    );
}

fn run_owner_branch(branch: &'static str) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("private readiness control");
    listener
        .set_nonblocking(true)
        .expect("bounded readiness accept");
    let (supervisor, recovery) = PrimaryRoleSupervisor::start_with_bound_generations_gated(
        PrimaryRoleConfig::primary(std::env::current_exe().expect("test executable"))
            .args(["--exact", CHILD_SELECTOR, "--nocapture", "--test-threads=1"])
            .env(
                CHILD_PORT_ENV,
                listener
                    .local_addr()
                    .expect("private port")
                    .port()
                    .to_string(),
            )
            .env_remove(SCENARIO_ENV),
    )
    .expect("real role supervisor");
    let deadline = Instant::now() + BOUND;
    let mut control = loop {
        match listener.accept() {
            Ok((stream, _)) => break stream,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(
                    Instant::now() < deadline,
                    "supervised child never became ready"
                );
                std::thread::yield_now();
            }
            Err(error) => panic!("readiness accept: {error}"),
        }
    };
    control
        .set_nonblocking(false)
        .expect("blocking control read");
    control
        .set_read_timeout(Some(BOUND))
        .expect("bounded control read");
    let mut ready = [0; CHILD_READY.len()];
    control
        .read_exact(&mut ready)
        .expect("actual child readiness");
    assert_eq!(ready, CHILD_READY);
    SHUTDOWN_FAULT.with(|fault| fault.set(Some(branch)));
    let (window_commands, _window_events) = std::sync::mpsc::channel();
    let mut owner = DirectPrimaryOwner::start(
        supervisor,
        recovery,
        window_commands,
        SessionShutdownState::new(),
    )
    .expect("actual direct owner");
    let result = if branch == "requested shutdown" {
        owner.shutdown()
    } else {
        // Drop the only real command sender. A disconnected replacement lets Drop
        // retain its invariant while the actual owner selects Disconnected.
        let (replacement, _unused) = std::sync::mpsc::channel();
        drop(std::mem::replace(&mut owner.command_tx, replacement));
        owner.join()
    };
    let error = result.expect_err("controlled supervisor failure must stay terminal");
    println!("GH1459_BRANCH_VERDICT_OBSERVED {branch}: {error}");
    assert!(error.to_string().contains("KELD-RUNTIME-003"));
    // The independent control remains live until the supervisor has reaped the child.
    let mut end = [0];
    match control.read(&mut end) {
        Ok(0) => {}
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::ConnectionAborted
            ) => {}
        other => panic!("reaped child control remained live: {other:?}"),
    }
}

#[test]
fn requested_shutdown_forwards_output_after_failed_verdict() {
    const BRANCH: &str = "requested shutdown";
    if std::env::var(SCENARIO_ENV).as_deref() == Ok(BRANCH) {
        run_owner_branch(BRANCH);
    } else {
        assert_isolated_output(
            "app_session::tests::direct_primary_output::requested_shutdown_forwards_output_after_failed_verdict",
            BRANCH,
        );
    }
}

#[test]
fn owner_disconnect_forwards_output_after_failed_verdict() {
    const BRANCH: &str = "owner disconnect";
    if std::env::var(SCENARIO_ENV).as_deref() == Ok(BRANCH) {
        run_owner_branch(BRANCH);
    } else {
        assert_isolated_output(
            "app_session::tests::direct_primary_output::owner_disconnect_forwards_output_after_failed_verdict",
            BRANCH,
        );
    }
}

#[test]
fn supervised_output_child() {
    let Some(port) = std::env::var_os(CHILD_PORT_ENV) else {
        return;
    };
    let mut control = TcpStream::connect((
        "127.0.0.1",
        port.to_str()
            .expect("control port")
            .parse::<u16>()
            .expect("numeric port"),
    ))
    .expect("connect readiness control");
    writeln!(std::io::stdout(), "{STDOUT_SENTINEL}").expect("write captured stdout");
    std::io::stdout().flush().expect("flush captured stdout");
    writeln!(std::io::stderr(), "{STDERR_SENTINEL}").expect("write captured stderr");
    std::io::stderr().flush().expect("flush captured stderr");
    control
        .write_all(CHILD_READY)
        .expect("publish independent readiness");
    let mut stop = [0];
    let _ = control.read_exact(&mut stop);
}

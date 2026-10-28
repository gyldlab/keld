//! GH-528 T2 and T3 end-to-end cases (spec gh527 criteria 6, 7(a) and the
//! host half of 8; the T3 Quit-then-close rule): a Bun role running the
//! GH-527 `WorkerLink` fixture
//! (`packages/@keld/kipc/test/worker-link-role.ts`) against the guarded
//! primary router on a real authenticated app link. Every OS with a router
//! runs them (#528 T5). Windows has no post-Quit drain (`await_windows_quit_peer_close`
//! is its Quit tail), so the drain-end case is a Unix expectation and Windows
//! states its own (gh527 §10, T5 finding).
//! The colocated host lifecycle receipt case uses the parent
//! framed socket-pair fixture without a Bun WorkerLink role or HELLO. It qualifies
//! core reservation/acceptance only; OS acknowledgment remains separate.
//!
//! KEL-140 owns the added public-index retirement and reply-wins consumers.
//! They, the public FS ring cases and the native observer cases stay macOS
//! only, as they were before #528 T5 widened this module.
//! Keep them with this existing authenticated router/process owner; review the module's layout
//! trigger at KEL-140 acceptance rather than duplicate that owner in another harness.

use super::*;

#[cfg(target_os = "macos")]
#[path = "public_fs_ring_e2e.rs"]
mod public_fs_ring_e2e;

#[cfg(feature = "fs-acceptance-observer")]
keld_native::keld_native_fs_observer_items! {
    #[cfg(target_os = "macos")]
    #[path = "native_observer_e2e.rs"]
    pub(in crate::app_session) mod native_observer_e2e;
}

/// One Bun role running the GH-527 `WorkerLink` fixture
/// (`packages/@keld/kipc/test/worker-link-role.ts`) on a real
/// authenticated app link, and the guarded router serving that link.
///
/// The role owns its child and output readers from spawn on: [`Drop`] kills
/// and reaps a child that a failed wait or assertion left running, so no
/// test exit leaves a Bun process behind.
struct WorkerLinkRole {
    child: Option<std::process::Child>,
    stdout: Option<std::thread::JoinHandle<String>>,
    stderr: Option<std::thread::JoinHandle<String>>,
}

impl WorkerLinkRole {
    /// Spawns `scenario` with the call it makes and returns it with the
    /// router on its authenticated link.
    fn start(
        scenario: &str,
        channel: &'static keld_ipc::channel_table::ChannelEntry,
        payload: &[u8],
    ) -> (Self, GuardedTestRouter) {
        use std::process::{Command, Stdio};

        let listener = keld_ipc::BootstrapListener::bind().expect("bind role listener");
        let script = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../packages/@keld/kipc/test/worker-link-role.ts");
        let mut command = Command::new("bun");
        command
            .arg(script)
            .arg(scenario)
            .env("KELD_APP_LINK", listener.app_link())
            .env("KELD_KIPC_TEST_HOOKS", "1")
            .env("KELD_T2_CHANNEL", channel.wire_id().to_string())
            .env("KELD_T2_PAYLOAD_HEX", hex_of(payload))
            .stdin(Stdio::null());
        Self::spawn_authenticated(&listener, command)
    }

    #[cfg(target_os = "macos")]
    /// Uses the same custody and authenticated router for a public-index caller.
    fn start_public_fs() -> (Self, GuardedTestRouter) {
        Self::start_public_fs_scenario("retirement")
    }

    #[cfg(target_os = "macos")]
    fn start_public_fs_scenario(scenario: &str) -> (Self, GuardedTestRouter) {
        Self::start_public_fs_with_test_setup(scenario, None)
    }

    #[cfg(target_os = "macos")]
    fn start_public_fs_with_test_setup(
        scenario: &str,
        setup: Option<GuardedFsTestSetup>,
    ) -> (Self, GuardedTestRouter) {
        Self::start_public_fs_with_test_setup_and_report(scenario, setup, None)
    }

    #[cfg(target_os = "macos")]
    fn start_public_fs_with_report(
        scenario: &str,
        key: &'static str,
        observed: SyncSender<()>,
    ) -> (Self, GuardedTestRouter) {
        Self::start_public_fs_with_test_setup_and_report(scenario, None, Some((key, observed)))
    }

    #[cfg(target_os = "macos")]
    fn start_public_fs_with_test_setup_and_report(
        scenario: &str,
        setup: Option<GuardedFsTestSetup>,
        report: Option<(&'static str, SyncSender<()>)>,
    ) -> (Self, GuardedTestRouter) {
        use std::process::{Command, Stdio};

        let listener = keld_ipc::BootstrapListener::bind().expect("bind public FS role listener");
        let script = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/app_session/tests/fixtures/public_fs_role.ts");
        let mut command = Command::new("bun");
        command
            .arg(script)
            .arg(scenario)
            .env("KELD_APP_LINK", listener.app_link())
            .env_remove("KELD_KIPC_TEST_HOOKS")
            .stdin(Stdio::piped());
        Self::spawn_authenticated_with_test_setup_and_report(&listener, command, setup, report)
    }

    #[cfg(target_os = "macos")]
    /// Supplies the existing private EOF-terminated control to the owned caller.
    fn supply_public_fs_targets(&mut self, target: &Path, late_target: &Path) {
        drop(self.supply_public_fs_targets_retained(target, late_target));
    }

    #[cfg(target_os = "macos")]
    fn supply_public_fs_targets_retained(
        &mut self,
        target: &Path,
        late_target: &Path,
    ) -> std::process::ChildStdin {
        use std::io::Write as _;

        let control = serde_json::json!({
            "target": target,
            "lateTarget": late_target,
        });
        let mut input = self
            .child
            .as_mut()
            .expect("owned public caller")
            .stdin
            .take()
            .expect("private fixture control pipe");
        writeln!(input, "{control}").expect("supply owned target paths");
        input.flush().expect("flush owned target control line");
        input
    }

    fn spawn_authenticated(
        listener: &keld_ipc::BootstrapListener,
        command: std::process::Command,
    ) -> (Self, GuardedTestRouter) {
        Self::spawn_authenticated_with_test_setup(listener, command, None)
    }

    #[cfg(target_os = "macos")]
    /// Captures an owned helper process with the same bounded finish/Drop custody.
    fn spawn_captured(command: std::process::Command) -> Self {
        Self::spawn_captured_with_report(command, None)
    }

    fn spawn_captured_with_report(
        mut command: std::process::Command,
        mut report: Option<(&'static str, SyncSender<()>)>,
    ) -> Self {
        use std::io::{BufRead as _, Read as _};
        use std::process::Stdio;

        let mut child = command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("bun must be on PATH (same contract as keld-cli bun_echo)");
        let out = child.stdout.take().expect("role stdout");
        let mut err = child.stderr.take().expect("role stderr");
        Self {
            child: Some(child),
            stdout: Some(std::thread::spawn(move || {
                let mut reader = std::io::BufReader::new(out);
                let mut text = String::new();
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).expect("read owned role stdout") == 0 {
                        break;
                    }
                    text.push_str(&line);
                    if report.as_ref().is_some_and(|(key, _)| {
                        line.strip_prefix("KELD_WL ")
                            .and_then(|record| record.split_once('='))
                            .is_some_and(|(observed_key, value)| {
                                observed_key == *key && value.trim_end() == "true"
                            })
                    }) {
                        let (_, observed) = report.take().expect("matched owned report observer");
                        observed.send(()).expect("publish owned role report");
                    }
                }
                text
            })),
            stderr: Some(std::thread::spawn(move || {
                let mut text = String::new();
                let _ = err.read_to_string(&mut text);
                text
            })),
        }
    }

    fn spawn_authenticated_with_test_setup(
        listener: &keld_ipc::BootstrapListener,
        command: std::process::Command,
        setup: Option<GuardedFsTestSetup>,
    ) -> (Self, GuardedTestRouter) {
        Self::spawn_authenticated_with_test_setup_and_report(listener, command, setup, None)
    }

    fn spawn_authenticated_with_test_setup_and_report(
        listener: &keld_ipc::BootstrapListener,
        command: std::process::Command,
        setup: Option<GuardedFsTestSetup>,
        report: Option<(&'static str, SyncSender<()>)>,
    ) -> (Self, GuardedTestRouter) {
        let mut role = Self::spawn_captured_with_report(command, report);
        let deadline = Instant::now() + Duration::from_secs(20);
        let admission = listener.accept_authenticated_until(deadline, &NoRejections);
        let Ok(keld_ipc::BootstrapAdmission::Authenticated(stream)) = admission else {
            let child = role.child.as_mut().expect("owned unauthenticated role");
            let terminated = child.kill();
            let status = child.wait().expect("reap unauthenticated role");
            role.child = None;
            let (stdout, stderr) = role.join_output();
            panic!(
                "role did not authenticate: {admission:?}; terminate={terminated:?}; status={status:?}\n{stdout}\n{stderr}"
            );
        };
        (role, guarded_router_with_test_setup(stream, setup))
    }

    /// Waits for the role to exit (kill switch only) and returns its
    /// `KELD_WL key=value` report.
    fn finish(self) -> std::collections::BTreeMap<String, String> {
        self.finish_with_status().0
    }

    /// Retains the actual owning Child wait result for independent observers.
    fn finish_with_status(
        mut self,
    ) -> (
        std::collections::BTreeMap<String, String>,
        u32,
        std::process::ExitStatus,
    ) {
        let mut child = self.child.take().expect("role child present");
        let pid = child.id();
        let started = Instant::now();
        let status = loop {
            if let Some(status) = child.try_wait().expect("poll role") {
                break status;
            }
            if started.elapsed() > Duration::from_mins(1) {
                let _ = child.kill();
                break child.wait().expect("reap role after kill switch");
            }
            std::thread::park_timeout(Duration::from_millis(10));
        };
        let (stdout, stderr) = self.join_output();
        assert!(
            status.success(),
            "role failed: {status:?}\n{stdout}\n{stderr}"
        );
        let mut report = std::collections::BTreeMap::new();
        for (key, value) in stdout
            .lines()
            .filter_map(|line| line.strip_prefix("KELD_WL "))
            .filter_map(|rest| rest.split_once('='))
        {
            assert!(
                report.insert(key.to_owned(), value.to_owned()).is_none(),
                "duplicate role report key `{key}`: {stdout}"
            );
        }
        (report, pid, status)
    }

    /// Joins whichever output readers remain; each runs to its pipe's EOF.
    fn join_output(&mut self) -> (String, String) {
        let stdout = self
            .stdout
            .take()
            .map(|reader| reader.join().unwrap_or_default())
            .unwrap_or_default();
        let stderr = self
            .stderr
            .take()
            .map(|reader| reader.join().unwrap_or_default())
            .unwrap_or_default();
        (stdout, stderr)
    }
}

impl Drop for WorkerLinkRole {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        // The child's pipes are closed now, so both readers reach EOF.
        let _ = self.join_output();
    }
}

fn hex_of(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut out, byte| {
        let _ = write!(out, "{byte:02x}");
        out
    })
}

struct NoRejections;

impl keld_ipc::BootstrapRejectionObserver for NoRejections {
    fn rejected(&self, _rejection: keld_ipc::BootstrapRejection) {}
}

fn fs_write_payload(target: &Path) -> Vec<u8> {
    encode(&FsRequest::Write {
        path: target.display().to_string().replace('\\', "/"),
        bytes: b"t2".to_vec(),
    })
    .expect("encode FS write")
}

fn report_value<'a>(
    report: &'a std::collections::BTreeMap<String, String>,
    key: &str,
) -> Option<&'a str> {
    report.get(key).map(String::as_str)
}

/// Criterion 6 end to end: a Bun role parked in `callBlocking` on the FS
/// channel, with the host's FS worker holding that call, throws
/// `KELD-IPC-023` when the host retires the generation; the link is gone.
///
/// On Windows the 023 reaches a transport Worker that is late to read only
/// because the host's close keeps unread pipe data. `DisconnectNamedPipe`
/// discarded it: native Windows 11 runs gave `KELD-IPC-022` in 5 of 60 with
/// every CPU busy (#1073 W2, gh527 §10).
#[test]
fn bun_role_parked_on_fs_throws_023_when_the_host_retires() {
    let (_temp_target, target) = {
        let temp = tempfile::tempdir().expect("target root");
        let path = temp.path().join("unused.txt");
        (temp, path)
    };
    let (role, t) = WorkerLinkRole::start(
        "t2-blocking-call",
        &keld_ipc::channel_table::FS,
        &fs_write_payload(&target),
    );
    t.taken
        .recv_timeout(Duration::from_secs(20))
        .expect("the role's FS call reached the held worker");
    t.router.handle().retire_generation(1).expect("retire g1");
    let report = role.finish();
    assert_eq!(
        report_value(&report, "call-code"),
        Some("KELD-IPC-023"),
        "{report:?}"
    );
    assert_eq!(report_value(&report, "call-returned"), Some("false"));
    assert_eq!(report_value(&report, "after-code"), Some("KELD-IPC-022"));
    t.release.send(()).expect("release FS worker");
    t.router.shutdown().expect("router shutdown");
}

/// Criterion 7(a) end to end: a parked blocking `Quit` returns the host's
/// real `LifecycleResponse::Quit` bytes, and the link closes afterwards
/// (a later echo CALL is never answered and ends with the host's close).
#[test]
fn bun_role_blocking_quit_returns_the_real_reply_then_the_link_closes() {
    let quit = encode(&LifecycleRequest::Quit).expect("encode Quit");
    let (role, t) = WorkerLinkRole::start(
        "t2-blocking-call",
        &keld_ipc::channel_table::LIFECYCLE,
        &quit,
    );
    let TestPrimaryOwnerCommand::PrepareAcceptedShutdown(prepare) = t
        .guardian
        .recv_timeout(Duration::from_secs(20))
        .expect("Quit attribution")
    else {
        panic!("Quit skipped shutdown attribution");
    };
    prepare.send(Ok(())).expect("acknowledge attribution");
    let TestPrimaryOwnerCommand::Shutdown(shutdown) = t
        .guardian
        .recv_timeout(Duration::from_secs(20))
        .expect("guardian shutdown")
    else {
        panic!("unexpected guardian command after the Quit REPLY");
    };
    shutdown
        .send(Ok(()))
        .expect("acknowledge guardian shutdown");
    let report = role.finish();
    let expected = encode(&LifecycleResponse::Quit).expect("encode Quit response");
    let expected_hex = hex_of(&expected);
    assert_eq!(
        report_value(&report, "call-hex"),
        Some(expected_hex.as_str()),
        "{report:?}"
    );
    assert_eq!(report_value(&report, "call-returned"), Some("true"));
    assert_eq!(report_value(&report, "after-code"), Some("KELD-IPC-022"));
    assert_eq!(
        t.window
            .recv_timeout(Duration::from_secs(5))
            .expect("UI Quit"),
        AppWindowCommand::Quit
    );
    // The later echo CALL arrives after the Quit REPLY; the post-Quit drain
    // reads it on every OS, so the router shuts down cleanly.
    assert_router_shutdown_after_quit_with_trailing_call(t.router);
}

/// Criterion 8, host half: the role's transport Worker dies while main is
/// parked on an FS call the host still holds. The host observes link loss
/// and asks its owner to fail generation 1, KEL-75's natural-crash entry,
/// and the role throws `KELD-IPC-025`.
#[test]
fn bun_worker_death_is_link_loss_that_fails_the_generation() {
    let (_temp_target, target) = {
        let temp = tempfile::tempdir().expect("target root");
        let path = temp.path().join("unused.txt");
        (temp, path)
    };
    let (role, t) = WorkerLinkRole::start(
        "t2-worker-dies",
        &keld_ipc::channel_table::FS,
        &fs_write_payload(&target),
    );
    t.router.handle().signal_ready().expect("Ready");
    t.taken
        .recv_timeout(Duration::from_secs(20))
        .expect("the role's FS call reached the held worker");
    let TestPrimaryOwnerCommand::FailGeneration(attempt, reply) = t
        .guardian
        .recv_timeout(Duration::from_secs(20))
        .expect("the host reports the lost link to its owner")
    else {
        panic!("Worker death did not take the link-failure path");
    };
    assert_eq!(attempt, 1);
    reply.send(Ok(())).expect("acknowledge link failure");
    let report = role.finish();
    assert_eq!(
        report_value(&report, "call-code"),
        Some("KELD-IPC-025"),
        "{report:?}"
    );
    t.release.send(()).expect("release FS worker");
    // This fixture owns the primary command receiver. Let the entered FS call
    // take its terminal-write failure path, then acknowledge its retirement
    // before joining the worker which still owns that pending receipt.
    let TestPrimaryOwnerCommand::FailRetiredGeneration(retired, receipt) = t
        .guardian
        .recv_timeout(Duration::from_secs(20))
        .expect("failed FS terminal write reports its retired generation")
    else {
        panic!("failed FS terminal write skipped retired-generation recovery");
    };
    assert_eq!(retired, 1);
    // The macOS guardian's receipt carries the retirement instant; the Linux
    // and Windows direct owner's receipt carries none.
    #[cfg(target_os = "macos")]
    let acknowledged = Ok(Instant::now());
    #[cfg(not(target_os = "macos"))]
    let acknowledged = Ok(());
    receipt
        .send(acknowledged)
        .expect("acknowledge the retired FS generation");
    t.router.shutdown().expect("router shutdown");
}

/// GH-528 T3 (#636 gate review, coordinator decision): a Bun role whose Quit
/// is its last call (`quitAndCloseLink`) closes the link on the real
/// `LifecycleResponse::Quit` REPLY. The host's post-Quit drain ends at that
/// EOF, not at its idle backstop, before the Quit tail asks the guardian to
/// shut down, and the tail proceeds to the UI Quit. The role's `onEnd`
/// reports the close as `KELD-IPC-022`. *Negative control:* a Quit that does
/// not close the link ends the drain at `IdleBackstop`.
#[test]
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
fn bun_role_closes_on_the_quit_reply_and_the_drain_ends_at_eof() {
    let (role, t) =
        WorkerLinkRole::start("t3-quit-close", &keld_ipc::channel_table::LIFECYCLE, &[]);
    let (end_tx, drain_end) = mpsc::sync_channel(1);
    t.router.handle().observe_next_quit_drain_end(end_tx);
    let TestPrimaryOwnerCommand::PrepareAcceptedShutdown(prepare) = t
        .guardian
        .recv_timeout(Duration::from_secs(20))
        .expect("Quit attribution")
    else {
        panic!("Quit skipped shutdown attribution");
    };
    prepare.send(Ok(())).expect("acknowledge attribution");
    assert_eq!(
        drain_end
            .recv_timeout(Duration::from_secs(20))
            .expect("the post-Quit drain ended"),
        QuitDrainEnd::PeerClosed,
        "the drain ends at the role's EOF"
    );
    let TestPrimaryOwnerCommand::Shutdown(shutdown) = t
        .guardian
        .recv_timeout(Duration::from_secs(20))
        .expect("the Quit tail reaches the guardian after the drain")
    else {
        panic!("unexpected guardian command after the Quit drain");
    };
    shutdown
        .send(Ok(()))
        .expect("acknowledge guardian shutdown");
    assert_eq!(
        t.window
            .recv_timeout(Duration::from_secs(5))
            .expect("UI Quit"),
        AppWindowCommand::Quit
    );
    let report = role.finish();
    // The role accepted the real `LifecycleResponse::Quit` REPLY.
    assert_eq!(
        report_value(&report, "quit"),
        Some("returned"),
        "{report:?}"
    );
    assert_eq!(report_value(&report, "end-code"), Some("KELD-IPC-022"));
    t.router.shutdown().expect("router shutdown after Quit");
}

/// The Windows half of the case above (#528 T5). Windows has no post-Quit
/// drain: after the real `LifecycleResponse::Quit` REPLY, its Quit tail waits
/// for the role to close the link and treats any byte as a fault
/// (`await_windows_quit_peer_close_until`). A role that closes on the REPLY
/// therefore ends that wait at its close, the tail asks the owner to shut
/// down, and the UI quits. The role's `onEnd` reports `KELD-IPC-022`.
/// *Negative control:* a peer that never closes holds the wait to its
/// deadline instead (`quit_peer_close_wait_is_bounded_for_a_non_closing_client`).
/// A CALL written after the REPLY would fail this tail, not get `KELD-IPC-024`:
/// that is the gh527 criterion 7 finding for Windows (gh527 §10).
#[test]
#[cfg(windows)]
fn bun_role_closes_on_the_quit_reply_and_windows_awaits_its_close() {
    let (role, t) =
        WorkerLinkRole::start("t3-quit-close", &keld_ipc::channel_table::LIFECYCLE, &[]);
    let TestPrimaryOwnerCommand::PrepareAcceptedShutdown(prepare) = t
        .guardian
        .recv_timeout(Duration::from_secs(20))
        .expect("Quit attribution")
    else {
        panic!("Quit skipped shutdown attribution");
    };
    prepare.send(Ok(())).expect("acknowledge attribution");
    let TestPrimaryOwnerCommand::Shutdown(shutdown) = t
        .guardian
        .recv_timeout(Duration::from_secs(20))
        .expect("the Quit tail reaches the owner after the role's close")
    else {
        panic!("unexpected owner command after the Quit REPLY");
    };
    shutdown.send(Ok(())).expect("acknowledge owner shutdown");
    assert_eq!(
        t.window
            .recv_timeout(Duration::from_secs(5))
            .expect("UI Quit"),
        AppWindowCommand::Quit
    );
    let report = role.finish();
    assert_eq!(
        report_value(&report, "quit"),
        Some("returned"),
        "{report:?}"
    );
    assert_eq!(report_value(&report, "end-code"), Some("KELD-IPC-022"));
    t.router.shutdown().expect("router shutdown after Quit");
}

#[cfg(target_os = "macos")]
/// These fields describe one public Promise handler, not wire/native cardinality.
fn assert_public_retirement_report(report: &std::collections::BTreeMap<String, String>) {
    assert_eq!(
        [
            report_value(report, "call-code"),
            report_value(report, "call-returned"),
            report_value(report, "settlements"),
            report_value(report, "after-code"),
        ],
        [
            Some("KELD-IPC-023"),
            Some("false"),
            Some("1"),
            Some("KELD-IPC-022")
        ],
        "{report:?}"
    );
}

/// KEL-140 AC7 consumer: a public async FS caller sees the real retirement ERR.
/// The admitted pre-native lease and sentinel are observed separately from rejection.
/// `settlements` counts this public Promise handler, not wire ERRs or native syscalls.
#[test]
#[cfg(target_os = "macos")]
fn public_fs_write_preserves_retirement_error_until_owner_lease_drains() {
    use std::io::Write as _;

    let (mut role, t) = WorkerLinkRole::start_public_fs();
    let target = t.allowed.join("public-retire.txt");
    let late_target = t.allowed.join("public-after-retire.txt");
    fs::write(&target, b"unchanged").expect("seed independent native sentinel");
    let control = serde_json::json!({
        "target": target,
        "lateTarget": late_target,
    });
    let mut input = role
        .child
        .as_mut()
        .expect("owned public caller")
        .stdin
        .take()
        .expect("private fixture control pipe");
    writeln!(input, "{control}").expect("supply owned target paths");
    drop(input);
    t.router.handle().signal_ready().expect("real router Ready");
    t.taken
        .recv_timeout(Duration::from_secs(5))
        .expect("public fs.write reached the owning held worker");
    assert_eq!(fs::read(&target).expect("held sentinel"), b"unchanged");
    assert_eq!(
        t.snapshot.fs.state.lock().expect("held FS state").in_flight,
        1,
        "the owner's held lease must exist before retirement"
    );

    t.router
        .handle()
        .retire_generation(1)
        .expect("retire public caller generation");
    let report = role.finish();
    assert_public_retirement_report(&report);
    assert_eq!(
        t.snapshot
            .fs
            .state
            .lock()
            .expect("retired held FS state")
            .in_flight,
        1,
        "the public rejection must not masquerade as native lease drain"
    );
    assert_eq!(
        fs::read(&target).expect("pre-release sentinel"),
        b"unchanged"
    );
    assert!(
        matches!(
            fs::symlink_metadata(&late_target),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound
        ),
        "later public call entered the old native owner"
    );

    t.release
        .send(())
        .expect("release the original held FS worker");
    t.snapshot
        .fs
        .drain()
        .expect("observe actual owner lease drain");
    assert_eq!(
        t.snapshot
            .fs
            .state
            .lock()
            .expect("drained FS state")
            .in_flight,
        0
    );
    t.router
        .shutdown()
        .expect("router shutdown after owner drain");
    // Joining the owner's readers and worker fences late entry/effect checks.
    assert!(
        matches!(
            t.taken.try_recv(),
            Err(mpsc::TryRecvError::Empty | mpsc::TryRecvError::Disconnected)
        ),
        "post-retirement public call entered the worker again"
    );
    assert_eq!(
        fs::read(&target).expect("post-owner-join sentinel"),
        b"unchanged"
    );
    assert!(
        matches!(
            fs::symlink_metadata(&late_target),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound
        ),
        "post-retirement target acquired an effect after owner join"
    );
}

#[cfg(target_os = "macos")]
/// A successful public Promise is an adapter result, not a wire-answer census.
fn assert_public_success_report(report: &std::collections::BTreeMap<String, String>) {
    assert_eq!(
        [
            report_value(report, "call-code"),
            report_value(report, "call-returned"),
            report_value(report, "settlements"),
            report_value(report, "after-code"),
        ],
        [None, Some("true"), Some("1"), None],
        "{report:?}"
    );
}

/// The real public write REPLY survives retirement while its owner lease is held.
/// A fresh authenticated public caller then completes a real write/read after drain.
/// `retire_after_a_real_reply_sends_no_023` separately owns the raw wire census.
#[test]
#[cfg(target_os = "macos")]
fn public_fs_reply_wins_retirement_then_a_fresh_session_reads_its_write() {
    completed_public_write_preserves_reply_after_retirement();
    // The old helper has drained and dropped its complete owned scope.
    fresh_public_fs_session_reads_its_write();
}

#[cfg(target_os = "macos")]
fn completed_public_write_preserves_reply_after_retirement() {
    let (mut role, t) = WorkerLinkRole::start_public_fs_scenario("reply-wins");
    let target = t.allowed.join("public-reply-wins.txt");
    let late_target = t.allowed.join("public-reply-wins-unused.txt");
    fs::write(&target, b"unchanged").expect("seed reply-wins native sentinel");
    role.supply_public_fs_targets(&target, &late_target);
    let (written_tx, written) = mpsc::sync_channel(1);
    let (resume, resume_rx) = mpsc::channel();
    t.snapshot
        .fs
        .hold_next_terminal_write(written_tx, resume_rx);
    t.router.handle().signal_ready().expect("real router Ready");
    t.taken
        .recv_timeout(Duration::from_secs(5))
        .expect("public write reached the held native worker");
    assert_eq!(
        fs::read(&target).expect("held reply-wins sentinel"),
        b"unchanged"
    );
    assert_eq!(
        t.snapshot.fs.state.lock().expect("held FS state").in_flight,
        1
    );

    t.release.send(()).expect("release public native write");
    written
        .recv_timeout(Duration::from_secs(5))
        .expect("the real FS REPLY was written and its correlation retired");
    assert_eq!(fs::read(&target).expect("completed native write"), b"t2");
    assert_eq!(
        t.snapshot
            .fs
            .state
            .lock()
            .expect("replied held FS state")
            .in_flight,
        1,
        "reply publication must not masquerade as owner lease drain"
    );
    t.router
        .handle()
        .retire_generation(1)
        .expect("retire after the real public FS REPLY");
    // Release promptly: the terminal hook's existing bound is a kill switch.
    resume.send(()).expect("resume the replied FS worker");
    t.snapshot
        .fs
        .drain()
        .expect("observe reply-wins owner drain");
    assert_eq!(
        t.snapshot
            .fs
            .state
            .lock()
            .expect("drained FS state")
            .in_flight,
        0
    );
    let report = role.finish();
    assert_public_success_report(&report);
    t.router
        .shutdown()
        .expect("join reply-wins router readers and worker");
    assert_eq!(fs::read(&target).expect("post-join replied bytes"), b"t2");
    assert!(
        matches!(
            t.taken.try_recv(),
            Err(mpsc::TryRecvError::Empty | mpsc::TryRecvError::Disconnected)
        ),
        "the public reply-wins caller replayed its operation"
    );
    assert!(
        matches!(
            fs::symlink_metadata(&late_target),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound
        ),
        "the closed public scenario acquired an unused target effect"
    );
}

#[cfg(target_os = "macos")]
// A fresh realm/router has one canonical link; this is not supervisor recovery.
fn fresh_public_fs_session_reads_its_write() {
    let (mut role, t) = WorkerLinkRole::start_public_fs_scenario("healthy");
    let target = t.allowed.join("public-fresh-write-read.txt");
    let late_target = t.allowed.join("public-fresh-unused.txt");
    fs::write(&target, b"unchanged").expect("seed fresh native sentinel");
    role.supply_public_fs_targets(&target, &late_target);
    t.router.handle().signal_ready().expect("fresh real Ready");
    t.taken
        .recv_timeout(Duration::from_secs(5))
        .expect("fresh public write reached the held worker");
    assert_eq!(
        fs::read(&target).expect("fresh held sentinel"),
        b"unchanged"
    );
    t.release.send(()).expect("release fresh public write");
    t.taken
        .recv_timeout(Duration::from_secs(5))
        .expect("fresh public read follows its completed write");
    let (written_tx, written) = mpsc::sync_channel(1);
    let (resume, resume_rx) = mpsc::channel();
    // Arm only after the read is taken, so this fence cannot hold the write.
    t.snapshot
        .fs
        .hold_next_terminal_write(written_tx, resume_rx);
    t.release.send(()).expect("release fresh public read");
    written
        .recv_timeout(Duration::from_secs(5))
        .expect("the real public read REPLY was written");
    assert_eq!(fs::read(&target).expect("fresh written OS bytes"), b"t2");
    assert_eq!(
        t.snapshot
            .fs
            .state
            .lock()
            .expect("fresh replied held FS state")
            .in_flight,
        1
    );
    t.router
        .handle()
        .retire_generation(1)
        .expect("close fresh owned link after its real read REPLY");
    resume.send(()).expect("resume fresh read terminal path");
    t.snapshot.fs.drain().expect("observe fresh owner drain");
    assert_eq!(
        t.snapshot
            .fs
            .state
            .lock()
            .expect("fresh drained state")
            .in_flight,
        0
    );
    let report = role.finish();
    assert_public_success_report(&report);
    assert_eq!(
        report_value(&report, "read-returned"),
        Some("true"),
        "{report:?}"
    );
    assert_eq!(
        report_value(&report, "read-hex"),
        Some("7432"),
        "{report:?}"
    );
    t.router
        .shutdown()
        .expect("join fresh router readers and worker");
    assert_eq!(fs::read(&target).expect("fresh post-join OS bytes"), b"t2");
    assert!(
        matches!(
            t.taken.try_recv(),
            Err(mpsc::TryRecvError::Empty | mpsc::TryRecvError::Disconnected)
        ),
        "fresh public write/read acquired an extra worker entry"
    );
    assert!(
        matches!(
            fs::symlink_metadata(&late_target),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound
        ),
        "fresh public caller acquired an unused target effect"
    );
}

/// Core acceptance is distinct from the owner request and local reservation.
/// The actor reply is controlled here; real Q/guardian proof is a separate atom.
#[test]
#[cfg(target_os = "macos")]
fn lifecycle_quit_reserves_without_accepting_before_owner_receipt() {
    let (t, mut client) = guarded_test_router();
    let handle = t.router.handle();
    handle.signal_ready().expect("initial framed Ready");
    assert_lifecycle_event(&mut client, LifecycleEvent::Ready);
    write_quit_call(&mut client, 143);

    let command = t
        .guardian
        .recv_timeout(Duration::from_secs(5))
        .expect("Quit requested its existing owner receipt");
    let GuardianOwnerCommand::PrepareAcceptedShutdown(receipt) = command else {
        panic!("Quit used the wrong owner command");
    };
    let accepted_before_receipt = handle.shutdown.cause();
    let tail_before_receipt = handle.shutdown.tail_started.load(Ordering::Acquire);
    let admission_closed_before_receipt = handle.begin_fs_call(1, CorrelationId(144)).is_err();

    // Finish the actual framed tail before checking the recorded cut so
    // an expected baseline failure cannot strand the owning router thread.
    receipt
        .send(Ok(()))
        .expect("deliver controlled owner receipt");
    let (header, payload) =
        keld_ipc::link::read_frame(&mut client).expect("correlated Quit reply after owner receipt");
    assert_eq!(header.kind, FrameKind::Reply);
    assert_eq!(header.channel, LIFECYCLE_CHANNEL);
    assert_eq!(header.corr, CorrelationId(143));
    assert_eq!(
        decode::<LifecycleResponse>(&payload).expect("Quit response"),
        LifecycleResponse::Quit
    );
    drop(client);
    let command = t
        .guardian
        .recv_timeout(Duration::from_secs(5))
        .expect("ordered guardian shutdown request");
    let GuardianOwnerCommand::Shutdown(stopped) = command else {
        panic!("Quit tail used the wrong owner command");
    };
    stopped
        .send(Ok(()))
        .expect("complete controlled owner shutdown");
    assert!(matches!(
        t.window.recv_timeout(Duration::from_secs(5)),
        Ok(AppWindowCommand::Quit)
    ));
    t.router.shutdown().expect("bounded framed router cleanup");

    assert_eq!(
        accepted_before_receipt, SESSION_RUNNING,
        "Quit accepted before the existing owner receipt"
    );
    assert!(
        !tail_before_receipt,
        "Quit tail began before the existing owner receipt"
    );
    assert!(
        admission_closed_before_receipt,
        "shutdown reservation did not close native admission"
    );
}

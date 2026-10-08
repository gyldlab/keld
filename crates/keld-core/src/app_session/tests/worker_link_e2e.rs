//! GH-528 T2 and T3 end-to-end cases (spec gh527 criteria 6, 7(a) and the
//! host half of 8; the T3 Quit-then-close rule): a Bun role running the
//! GH-527 `WorkerLink` fixture
//! (`packages/@keld/kipc/test/worker-link-role.ts`) against the guarded
//! primary router on a real authenticated app link. macOS only, like the T1
//! harness; T5 qualifies Linux and Windows.

use super::*;

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
        channel: keld_ipc::ChannelId,
        payload: &[u8],
    ) -> (Self, GuardedTestRouter) {
        use std::io::Read as _;
        use std::process::{Command, Stdio};

        let listener = keld_ipc::BootstrapListener::bind().expect("bind role listener");
        let script = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../packages/@keld/kipc/test/worker-link-role.ts");
        let mut child = Command::new("bun")
            .arg(script)
            .arg(scenario)
            .env("KELD_APP_LINK", listener.app_link())
            .env("KELD_KIPC_TEST_HOOKS", "1")
            .env("KELD_T2_CHANNEL", channel.0.to_string())
            .env("KELD_T2_PAYLOAD_HEX", hex_of(payload))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("bun must be on PATH (same contract as keld-cli bun_echo)");
        let mut out = child.stdout.take().expect("role stdout");
        let mut err = child.stderr.take().expect("role stderr");
        let role = Self {
            child: Some(child),
            stdout: Some(std::thread::spawn(move || {
                let mut text = String::new();
                let _ = out.read_to_string(&mut text);
                text
            })),
            stderr: Some(std::thread::spawn(move || {
                let mut text = String::new();
                let _ = err.read_to_string(&mut text);
                text
            })),
        };
        let deadline = Instant::now() + Duration::from_secs(20);
        let admission = listener
            .accept_authenticated_until(deadline, &NoRejections)
            .expect("accept role");
        let keld_ipc::BootstrapAdmission::Authenticated(stream) = admission else {
            // Dropping `role` kills and reaps the child.
            panic!("role did not authenticate: {admission:?}");
        };
        (role, guarded_router(stream))
    }

    /// Waits for the role to exit (kill switch only) and returns its
    /// `KELD_WL key=value` report.
    fn finish(mut self) -> std::collections::BTreeMap<String, String> {
        let mut child = self.child.take().expect("role child present");
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
        stdout
            .lines()
            .filter_map(|line| line.strip_prefix("KELD_WL "))
            .filter_map(|rest| rest.split_once('='))
            .map(|(key, value)| (key.to_owned(), value.to_owned()))
            .collect()
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
#[test]
fn bun_role_parked_on_fs_throws_023_when_the_host_retires() {
    let (_temp_target, target) = {
        let temp = tempfile::tempdir().expect("target root");
        let path = temp.path().join("unused.txt");
        (temp, path)
    };
    let (role, t) =
        WorkerLinkRole::start("t2-blocking-call", FS_CHANNEL, &fs_write_payload(&target));
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
    let (role, t) = WorkerLinkRole::start("t2-blocking-call", LIFECYCLE_CHANNEL, &quit);
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
    t.router.shutdown().expect("router shutdown after Quit");
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
    let (role, t) = WorkerLinkRole::start("t2-worker-dies", FS_CHANNEL, &fs_write_payload(&target));
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
fn bun_role_closes_on_the_quit_reply_and_the_drain_ends_at_eof() {
    let (role, t) = WorkerLinkRole::start("t3-quit-close", LIFECYCLE_CHANNEL, &[]);
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

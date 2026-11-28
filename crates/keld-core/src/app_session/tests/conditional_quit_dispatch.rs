//! Actual authenticated Main-link component probe. The existing legacy Quit
//! request is an explicitly armed test carrier, never a new wire operation.

use super::super::{
    AppWindowCommand, Arc, AtomicBool, Duration, FsRequest, GuardedTestRouter, LifecycleRequest,
    LifecycleResponse, Ordering, PrimaryCallerContext, PrimaryLinkIdentity, PrimaryRouter,
    PrimaryRouterHandle, SessionShutdownState, encode, fs, gh499_graph,
    guarded_router_with_factory, mpsc, thread,
};
use super::{AcceptedRole, RoleOwner, read_control};
use crate::app_session::{BoundSetup, GuardianOwnerCommand, RecoveryArm};
use keld_ipc::CorrelationId;
use keld_ipc::window::WindowRequest;
use keld_wv::{AppDesktopDisplay, AppDesktopFacts, AppDesktopRect, AppWindowEvent, BuildStage};
use nix::sys::signal::{Signal, killpg};
use nix::unistd::{Pid, getpgid, getpgrp, getpid};
use std::io::{self, BufReader, Read as _, Seek as _, SeekFrom, Write as _};
use std::net::TcpStream;
use std::os::unix::process::CommandExt as _;
use std::process::{Command, Stdio};
use std::time::Instant;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

// The direct Runtime fixture spawns Bun in its own inherited process group.
// Keep the exact test leader unreaped while a watchdog can signal that group.
// A watchdog is a test failure, never a native terminal success witness.
struct OwnedTestGroup {
    child: super::super::ReapedRouterProbe,
    group: Pid,
    armed: bool,
}

impl OwnedTestGroup {
    fn start(command: &mut Command) -> TestResult<Self> {
        let process = command.process_group(0).spawn()?;
        let group = Pid::from_raw(i32::try_from(process.id())?);
        let mut result = Self {
            child: super::super::ReapedRouterProbe::new(process),
            group,
            armed: false,
        };
        if group == getpgrp() || getpgid(Some(group))? != group {
            return Err(io::Error::other("isolated reader test group mismatch").into());
        }
        result.armed = true;
        Ok(result)
    }

    fn wait(&mut self) -> TestResult<std::process::ExitStatus> {
        // Five existing identity-fixture bounds cover bind, Ready, held
        // replies and supervisor cleanup. No product deadline is changed.
        let deadline = Instant::now() + super::BOUND * 5;
        while Instant::now() < deadline {
            let group = self.group.as_raw();
            let census = read_owned_group_census(group, deadline)?;
            if let Some(already_live_peer) = exited_leader_has_live_peer(&census, group)? {
                // A fresh census starts after leader exit, when the
                // leader can no longer add a member to this group.
                let census = read_owned_group_census(group, deadline)?;
                let has_live_peer = exited_leader_has_live_peer(&census, group)?
                    .ok_or_else(|| io::Error::other("authenticated reader leader state changed"))?;
                if Instant::now() >= deadline {
                    break;
                }
                if already_live_peer || has_live_peer {
                    return Err(io::Error::other(
                        "authenticated reader leader exited with a live fixture group member",
                    )
                    .into());
                }
                // The census included the exact owned leader as a zombie.
                // Do not signal this group after *any* reaping wait.
                self.armed = false;
                return Ok(self.child.0.wait()?);
            }
            thread::park_timeout(Duration::from_millis(50));
        }
        Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "authenticated reader test watchdog",
        )
        .into())
    }
}

fn read_owned_group_census(group: i32, deadline: Instant) -> io::Result<String> {
    const MAX_BYTES: u64 = 64 * 1024;
    let mut stdout = tempfile::tempfile()?;
    let stderr = tempfile::tempfile()?;
    let mut census = super::super::ReapedRouterProbe::new(
        Command::new("/bin/ps")
            .args(["-x", "-g", &group.to_string(), "-o", "pid=,pgid=,stat="])
            .env("COMMAND_MODE", "unix2003")
            .env("LC_ALL", "C")
            .stdin(Stdio::null())
            .stdout(Stdio::from(stdout.try_clone()?))
            .stderr(Stdio::from(stderr.try_clone()?))
            .spawn()?,
    );
    while Instant::now() < deadline {
        if stdout.metadata()?.len() > MAX_BYTES || stderr.metadata()?.len() != 0 {
            return Err(io::Error::other(
                "authenticated reader group census output invalid",
            ));
        }
        if let Some(status) = census.0.try_wait()? {
            if !status.success() || stderr.metadata()?.len() != 0 {
                return Err(io::Error::other("authenticated reader group census failed"));
            }
            stdout.seek(SeekFrom::Start(0))?;
            let mut output = String::new();
            stdout.take(MAX_BYTES + 1).read_to_string(&mut output)?;
            if output.len() as u64 > MAX_BYTES {
                return Err(io::Error::other(
                    "authenticated reader group census too large",
                ));
            }
            return Ok(output);
        }
        thread::park_timeout(Duration::from_millis(10));
    }
    Err(io::Error::new(
        io::ErrorKind::TimedOut,
        "authenticated reader group census watchdog",
    ))
}

// Same snapshot for the unreaped leader and its peers. A missing or
// ambiguous leader is not proof of retirement.
fn exited_leader_has_live_peer(census: &str, group: i32) -> io::Result<Option<bool>> {
    let invalid = || io::Error::other("invalid authenticated reader group census");
    let mut leader_exited = None;
    let mut has_live_peer = false;
    for row in census.lines().filter(|row| !row.trim().is_empty()) {
        let mut fields = row.split_ascii_whitespace();
        let pid = fields
            .next()
            .and_then(|value| value.parse::<i32>().ok())
            .filter(|&pid| pid > 0)
            .ok_or_else(invalid)?;
        let process_group = fields.next().and_then(|value| value.parse::<i32>().ok());
        let state = fields.next().ok_or_else(invalid)?;
        if process_group != Some(group) || fields.next().is_some() {
            return Err(invalid());
        }
        let zombie = state.starts_with('Z');
        if pid == group {
            if leader_exited.replace(zombie).is_some() {
                return Err(invalid());
            }
        } else {
            has_live_peer |= !zombie;
        }
    }
    Ok(leader_exited.ok_or_else(invalid)?.then_some(has_live_peer))
}

#[test]
fn authenticated_reader_fixture_census_distinguishes_live_peers_from_zombies() -> io::Result<()> {
    assert_eq!(exited_leader_has_live_peer("70 70 S\n", 70)?, None);
    assert_eq!(
        exited_leader_has_live_peer("70 70 Z+\n71 70 T\n", 70)?,
        Some(true)
    );
    assert_eq!(
        exited_leader_has_live_peer("70 70 Z\n71 70 Z\n", 70)?,
        Some(false)
    );
    for census in [
        "",
        "71 70 R\n",
        "70 70 Z\n70 70 Z\n",
        "70 70 Z\n71 69 S\n",
        "70 70\n",
    ] {
        assert!(exited_leader_has_live_peer(census, 70).is_err());
    }
    Ok(())
}

impl Drop for OwnedTestGroup {
    fn drop(&mut self) {
        if self.armed {
            // This group belongs to our still-owned unreaped leader. Never
            // signal a cached group after a successful wait.
            let _ = killpg(self.group, Signal::SIGKILL);
            self.armed = false;
            let _ = self.child.0.wait();
        }
    }
}

fn run_case(name: &str, exercise: impl FnOnce() -> TestResult) -> TestResult {
    const CHILD_ENV: &str = "KELD_CORE_TEST_GH451_AUTHENTICATED_READER";
    const MARKER: &str = "KELD_GH451_AUTHENTICATED_READER_OBSERVED";
    if std::env::var(CHILD_ENV).ok().as_deref() == Some(name) {
        assert_eq!(
            getpgrp(),
            getpid(),
            "fixture must own its disposable process group"
        );
        exercise()?;
        println!("{MARKER} {name}");
        return Ok(());
    }
    let root = tempfile::tempdir()?;
    let mut stdout = tempfile::tempfile_in(root.path())?;
    let mut stderr = tempfile::tempfile_in(root.path())?;
    let selector =
        format!("app_session::tests::accepted_primary_identity::conditional_quit_dispatch::{name}");
    let mut command = Command::new(std::env::current_exe()?);
    command
        .args(["--exact", &selector, "--nocapture", "--test-threads=1"])
        .env(CHILD_ENV, name)
        .env("TMPDIR", root.path())
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout.try_clone()?))
        .stderr(Stdio::from(stderr.try_clone()?));
    let mut child = OwnedTestGroup::start(&mut command)?;
    let status = child.wait();
    drop(child);
    stdout.seek(SeekFrom::Start(0))?;
    stderr.seek(SeekFrom::Start(0))?;
    let mut output = String::new();
    let mut detail = String::new();
    stdout.take(64 * 1024).read_to_string(&mut output)?;
    stderr.take(64 * 1024).read_to_string(&mut detail)?;
    assert!(
        status.as_ref().is_ok_and(std::process::ExitStatus::success)
            && output.contains(&format!("{MARKER} {name}"))
            && output.contains("test result: ok. 1 passed; 0 failed;"),
        "isolated reader proof failed: status={status:?}, stdout={output}, stderr={detail}"
    );
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut output, "{byte:02x}").expect("write into String");
    }
    output
}

fn send_call(control: &mut BufReader<TcpStream>, operation: &str, payload: &[u8]) -> TestResult {
    writeln!(control.get_mut(), "GH451 {operation} {}", hex(payload))?;
    assert_eq!(read_control(control), format!("{operation}-QUEUED"));
    Ok(())
}

struct Fixture {
    role: AcceptedRole,
    control: BufReader<TcpStream>,
    guarded: GuardedTestRouter,
    _stage: tempfile::TempDir,
}

impl Fixture {
    fn new() -> TestResult<Self> {
        let role = AcceptedRole::start_with_mode(RoleOwner::Primary, Some("gh451"));
        let (bound, mut control) = role.bind();
        let (stage, selection) = gh499_graph::original_quit_selection();
        let guarded = guarded_router_with_factory(None, |window, guardian, snapshot, gate| {
            PrimaryRouter::start_bound_with_fs_setup(
                bound,
                window,
                guardian,
                SessionShutdownState::new(),
                Some(snapshot),
                Some(gate),
                BoundSetup {
                    window_native_support: Some(true),
                    original_selection: Some(selection),
                    ..BoundSetup::default()
                },
            )
        });
        let handle = guarded.router.handle();
        // Deliver a fixture desktop receipt using the actual Core event owner,
        // then acknowledge Ready's existing guardian command.
        handle.window_native_event(AppWindowEvent::DesktopObserved {
            facts: Some(AppDesktopFacts {
                primary_id: 17,
                displays: vec![AppDesktopDisplay {
                    id: 17,
                    bounds: AppDesktopRect {
                        x: 0,
                        y: 0,
                        width: 1440,
                        height: 900,
                    },
                    work_area: AppDesktopRect {
                        x: 0,
                        y: 24,
                        width: 1440,
                        height: 876,
                    },
                    scale_factor: 2.0,
                }],
                should_use_dark_colors: false,
            }),
        })?;
        let ready = thread::spawn(move || handle.signal_ready());
        let GuardianOwnerCommand::ArmRecovery(RecoveryArm::Initial(1), response) =
            guarded.guardian.recv_timeout(Duration::from_secs(5))?
        else {
            return Err(io::Error::other("expected existing Ready arm").into());
        };
        response.send(Ok(()))?;
        ready
            .join()
            .map_err(|_| io::Error::other("Ready fixture panicked"))??;
        send_call(
            &mut control,
            "SUBSCRIBE",
            &encode(&LifecycleRequest::Subscribe {
                channel: keld_ipc::channel_table::WINDOW.wire_id(),
            })?,
        )?;
        assert_eq!(
            read_control(&mut control),
            format!(
                "SUBSCRIBE-REPLY:{}",
                hex(&encode(&LifecycleResponse::Subscribed)?)
            )
        );
        Ok(Self {
            role,
            control,
            guarded,
            _stage: stage,
        })
    }

    fn caller(handle: &PrimaryRouterHandle) -> TestResult<PrimaryCallerContext> {
        let current = handle
            .current
            .lock()
            .map_err(|_| io::Error::other("current fixture poisoned"))?;
        let active = current
            .as_ref()
            .ok_or_else(|| io::Error::other("accepted fixture owner absent"))?;
        Ok(PrimaryCallerContext {
            attempt: active.attempt,
            identity: active.identity,
            reader_stop: Arc::clone(&active.reader_stop),
        })
    }

    fn finish(mut self) -> TestResult {
        // This is emergency fixture disposal, not evidence for the product
        // terminal tail. The carrier intentionally has no wire ACK.
        self.guarded.router.shutdown()?;
        assert!(matches!(
            self.role.finish(),
            Some(keld_runtime::SupervisorOutcome::Stopped)
        ));
        Ok(())
    }
}

#[test]
fn authenticated_reader_retains_after_transient_create_and_services_same_link() -> TestResult {
    run_case(
        "authenticated_reader_retains_after_transient_create_and_services_same_link",
        exercise_retained,
    )
}

fn exercise_retained() -> TestResult {
    let mut fixture = Fixture::new()?;
    let handle = fixture.guarded.router.handle();
    let caller = Fixture::caller(&handle)?;
    let mut foreign = caller.clone();
    foreign.reader_stop = Arc::new(AtomicBool::new(false));
    assert!(
        handle
            .arm_conditional_quit_fixture(&foreign, CorrelationId(3))
            .is_err()
    );
    let receipt = handle.arm_conditional_quit_fixture(&caller, CorrelationId(3))?;
    // Exercise the dispatch guard itself after arming. Nonmatching probes
    // must not consume the slot. The real carrier below proves it survived.
    let mut stale = caller.clone();
    stale.attempt = caller.attempt.saturating_add(1);
    let mut wrong_identity = caller.clone();
    wrong_identity.identity = PrimaryLinkIdentity::Fixture;
    for mismatched in [&foreign, &stale, &wrong_identity] {
        assert_eq!(
            handle.dispatch_conditional_quit_fixture(mismatched, CorrelationId(3))?,
            None
        );
    }
    assert_eq!(
        handle.dispatch_conditional_quit_fixture(&caller, CorrelationId(4))?,
        None
    );

    // This is a real Window Call on the authenticated reader, with an injected
    // failed native build receipt. The live census is again zero, but the
    // accepted incarnation advanced and permanently invalidates preparation.
    send_call(
        &mut fixture.control,
        "CREATE",
        &encode(&WindowRequest::Create {
            width: 640,
            height: 480,
            x: None,
            y: None,
        })?,
    )?;
    let AppWindowCommand::CreateWindow { build, .. } = fixture
        .guarded
        .window
        .recv_timeout(Duration::from_secs(5))?
    else {
        return Err(io::Error::other("expected existing Create command").into());
    };
    handle.window_native_event(AppWindowEvent::WindowBuildFailed {
        build,
        stage: BuildStage::Window,
    })?;
    assert_eq!(
        read_control(&mut fixture.control),
        "CREATE-ERR:KELD-CORE-041"
    );
    send_call(
        &mut fixture.control,
        "CARRIER",
        &encode(&LifecycleRequest::Quit)?,
    )?;
    assert_eq!(
        receipt.recv_timeout(Duration::from_secs(5))?,
        (CorrelationId(3), "Retained".to_owned())
    );
    assert!(handle.shutdown.is_running());
    assert!(
        handle
            .arm_conditional_quit_fixture(&caller, CorrelationId(4))
            .is_err()
    );
    fixture.control.get_mut().write_all(b"ECHO\n")?;
    assert_eq!(read_control(&mut fixture.control), "ECHO-OK");
    fixture.finish()
}

#[test]
fn authenticated_reader_holds_terminal_cut_through_real_fs_reply_and_lease() -> TestResult {
    run_case(
        "authenticated_reader_holds_terminal_cut_through_real_fs_reply_and_lease",
        exercise_drain,
    )
}

fn exercise_drain() -> TestResult {
    let mut fixture = Fixture::new()?;
    let handle = fixture.guarded.router.handle();
    let caller = Fixture::caller(&handle)?;
    let receipt = handle.arm_conditional_quit_fixture(&caller, CorrelationId(3))?;
    let fs_owner = &fixture.guarded.snapshot.fs;
    let target = fixture.guarded.allowed.join("conditional.txt");
    let (written_tx, written_rx) = mpsc::sync_channel(1);
    let (resume_tx, resume_rx) = mpsc::channel();
    fs_owner.hold_next_terminal_write(written_tx, resume_rx);
    send_call(
        &mut fixture.control,
        "FS",
        &encode(&FsRequest::Write {
            path: target.display().to_string(),
            bytes: b"reader".to_vec(),
        })?,
    )?;
    fixture.guarded.taken.recv_timeout(Duration::from_secs(5))?;
    let (waiting_tx, waiting_rx) = mpsc::sync_channel(1);
    fs_owner.observe_next_drain_wait(waiting_tx);
    send_call(
        &mut fixture.control,
        "CARRIER",
        &encode(&LifecycleRequest::Quit)?,
    )?;
    waiting_rx.recv_timeout(Duration::from_secs(5))?;
    assert!(handle.shutdown.is_running());
    assert!(matches!(receipt.try_recv(), Err(mpsc::TryRecvError::Empty)));
    // Submit ordinary later work through the actual role while the one
    // reader waits inside its drain. Queued is not a native admission.
    send_call(
        &mut fixture.control,
        "CREATE",
        &encode(&WindowRequest::Create {
            width: 640,
            height: 480,
            x: None,
            y: None,
        })?,
    )?;
    fixture.guarded.release.send(())?;
    written_rx.recv_timeout(Duration::from_secs(5))?;
    assert_eq!(read_control(&mut fixture.control), "FS-REPLY:01");
    assert_eq!(fs::read(&target)?, b"reader");
    assert!(handle.shutdown.is_running());
    assert!(!handle.shutdown.reader_stop.load(Ordering::Acquire));
    assert!(
        matches!(receipt.try_recv(), Err(mpsc::TryRecvError::Empty)),
        "real FS lease is still retained"
    );
    resume_tx.send(())?;
    assert_eq!(
        receipt.recv_timeout(Duration::from_secs(5))?,
        (CorrelationId(3), "Committed".to_owned())
    );
    assert!(!handle.shutdown.is_running());
    assert!(handle.shutdown.reader_stop.load(Ordering::Acquire));
    assert!(
        fixture.guarded.window.try_recv().is_err(),
        "no later Create was admitted"
    );
    fixture.finish()
}

//! Actual authenticated Main-link component probe. The existing legacy Quit
//! request is an explicitly armed test carrier, never a new wire operation.

use super::super::{
    AppWindowCommand, Arc, AtomicBool, Duration, FsRequest, GuardedTestRouter, LifecycleRequest,
    LifecycleResponse, Ordering, PrimaryCallerContext, PrimaryRouter, PrimaryRouterHandle,
    SessionShutdownState, encode, fs, gh499_graph, guarded_router_with_factory, mpsc, thread,
};
use super::{AcceptedRole, RoleOwner, read_control};
use crate::app_session::{BoundSetup, GuardianOwnerCommand, RecoveryArm};
use keld_ipc::CorrelationId;
use keld_ipc::window::WindowRequest;
use keld_wv::{AppDesktopDisplay, AppDesktopFacts, AppDesktopRect, AppWindowEvent, BuildStage};
use std::io::{self, BufReader, Write as _};
use std::net::TcpStream;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

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

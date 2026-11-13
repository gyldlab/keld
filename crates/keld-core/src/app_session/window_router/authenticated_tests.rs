//! Linux actual Runtime/Bun/HELLO identity and Window-port proof. Fake native
//! results do not qualify native Init, OS containment, or a public Window source.
//! The role imports packages/@keld/kipc/src/transport.ts and
//! packages/@keld/api/src/window-codec.ts; assertions consume their actual bytes.
use super::super::super::{
    DirectPrimaryOwnerCommand, HostAppError, PlatformPrimaryOwnerHandle, PrimaryLinkIdentity,
    PrimaryRouter, PrimaryRouterHandle, RecoveryArm, SessionShutdownState,
};
use keld_runtime::SupervisorOutcome;
use keld_runtime::primary::{
    BoundPrimaryGeneration, PrimaryRecoveryGate, PrimaryRoleConfig, PrimaryRoleSupervisor,
    RoleOwner,
};
use keld_wv::{AppWindowCommand, AppWindowEvent, WebviewId, WindowSnapshot};
use std::collections::HashMap;
use std::io::{BufRead as _, BufReader, Write as _};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
const BOUND: Duration = Duration::from_secs(20);

struct RoleProcess {
    supervisor: PrimaryRoleSupervisor,
    recovery: PrimaryRecoveryGate,
    listener: TcpListener,
    finished: bool,
}
impl RoleProcess {
    fn start() -> TestResult<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        let script = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/app_session/window_router/fixtures/authenticated_window_role.ts");
        let (supervisor, recovery) = PrimaryRoleSupervisor::start_with_bound_generations_gated(
            PrimaryRoleConfig::primary("bun")
                .arg(script)
                .arg(listener.local_addr()?.port().to_string())
                .env_remove("KELD_KIPC_TEST_HOOKS"),
        )?;
        Ok(Self {
            supervisor,
            recovery,
            listener,
            finished: false,
        })
    }
    fn bind(&self) -> TestResult<(BoundPrimaryGeneration, BufReader<TcpStream>)> {
        let bound = self
            .supervisor
            .recv_bound_generation(BOUND)
            .ok_or_else(|| std::io::Error::other("authenticated role Bound absent"))?;
        // The actor connects this control socket before WorkerLink's HELLO, so
        // an authenticated Bound proves an accepted control connection is queued.
        let (stream, _) = self.listener.accept()?;
        stream.set_read_timeout(Some(BOUND))?;
        stream.set_write_timeout(Some(BOUND))?;
        let mut control = BufReader::new(stream);
        assert_eq!(read_control(&mut control)?, "BOUND");
        Ok((bound, control))
    }
    fn finish(&mut self) -> TestResult<SupervisorOutcome> {
        self.supervisor.shutdown();
        let deadline = Instant::now() + BOUND;
        loop {
            if let Some(outcome) = self.supervisor.try_wait_for_outcome() {
                self.finished = true;
                return Ok(outcome);
            }
            if Instant::now() >= deadline {
                return Err(std::io::Error::other("actual role shutdown did not complete").into());
            }
            thread::yield_now();
        }
    }
}
impl Drop for RoleProcess {
    fn drop(&mut self) {
        if !self.finished {
            self.supervisor.shutdown();
        }
    }
}
fn read_control(control: &mut BufReader<TcpStream>) -> TestResult<String> {
    let mut line = String::new();
    if control.read_line(&mut line)? == 0 {
        return Err(std::io::Error::other("authenticated Window actor control EOF").into());
    }
    Ok(line.trim_end().to_owned())
}
fn command(control: &mut BufReader<TcpStream>, value: &str) -> TestResult {
    writeln!(control.get_mut(), "{value}")?;
    Ok(())
}

fn router_for_bound(
    bound: BoundPrimaryGeneration,
    owner: mpsc::Sender<DirectPrimaryOwnerCommand>,
    windows: mpsc::Sender<AppWindowCommand>,
) -> Result<PrimaryRouter, HostAppError> {
    let identity = PrimaryLinkIdentity::Role(bound.principal());
    let attempt = bound.attempt();
    // Deliberately empty: no paired-stream fixture current or fabricated identity
    // is installed and then relabeled as Runtime-authenticated.
    let handle = PrimaryRouterHandle {
        current: Arc::new(Mutex::new(None)),
        readers: Arc::new(Mutex::new(HashMap::new())),
        window_ready: Arc::new(AtomicBool::new(false)),
        last_window_closed: Arc::new(AtomicBool::new(false)),
        windows: None,
        recovery_armed: Arc::new(AtomicBool::new(false)),
        last_revoked_attempt: Arc::new(AtomicU32::new(0)),
        pre_ready_link_failed: Arc::new(AtomicBool::new(false)),
        shutdown: SessionShutdownState::new(),
        fs: None,
        fs_worker_commands: None,
        guardian: PlatformPrimaryOwnerHandle { command_tx: owner },
        window_commands: windows,
        quit_drain_hooks: Arc::default(),
    };
    let router = PrimaryRouter::finish_start(handle, None, attempt, Some(true), None)?;
    router
        .handle
        .install_prepared_generation_with_identity(attempt, bound.into_stream(), identity)
        .map_err(|failure| failure.error)?;
    Ok(router)
}

fn publish_ready(
    role: &RoleProcess,
    handle: &PrimaryRouterHandle,
    owner: &mpsc::Receiver<DirectPrimaryOwnerCommand>,
) -> TestResult<u32> {
    let handle = handle.clone();
    let ready = thread::spawn(move || handle.signal_ready());
    let DirectPrimaryOwnerCommand::ArmRecovery(RecoveryArm::Initial(attempt), reply) =
        owner.recv_timeout(BOUND)?
    else {
        return Err(std::io::Error::other("expected existing initial recovery arm").into());
    };
    assert!(role.recovery.arm());
    reply.send(Ok(()))?;
    ready
        .join()
        .map_err(|_| std::io::Error::other("Ready owner panicked"))??;
    Ok(attempt)
}

fn native_snapshot() -> WindowSnapshot {
    WindowSnapshot {
        x: 37,
        y: 91,
        width: 800,
        height: 600,
        focused: true,
        maximized: false,
        full_screen: false,
    }
}
fn create_from_native_result(
    control: &mut BufReader<TcpStream>,
    native: &mpsc::Receiver<AppWindowCommand>,
    handle: &PrimaryRouterHandle,
) -> TestResult {
    command(control, "CREATE")?;
    let AppWindowCommand::CreateWindow {
        build,
        width,
        height,
        x,
        y,
    } = native.recv_timeout(BOUND)?
    else {
        return Err(
            std::io::Error::other("actual Create did not reach native command port").into(),
        );
    };
    assert_eq!((width, height, x, y), (640, 480, None, None));
    handle.window_native_event(AppWindowEvent::WindowBuilt {
        build,
        view: WebviewId(7),
        snapshot: native_snapshot(),
    })?;
    // WorkerLink's state applier has decoded Created before the call's Promise
    // continuation. The separate direct-socket case owns exact wire-order proof.
    assert_eq!(
        read_control(control)?,
        "CREATE Created 1:1 Created 1:1 37,91,800,600 true,false,false null 1"
    );
    Ok(())
}
fn setter_requires_native_confirmation(
    control: &mut BufReader<TcpStream>,
    native: &mpsc::Receiver<AppWindowCommand>,
    handle: &PrimaryRouterHandle,
) -> TestResult {
    command(control, "SET-MAX")?;
    assert_eq!(
        native.recv_timeout(BOUND)?,
        AppWindowCommand::SetMaximized {
            view: WebviewId(7),
            maximized: true
        }
    );
    assert_eq!(read_control(control)?, "SET-MAX Accepted 1 false");
    handle.window_native_event(AppWindowEvent::WindowMaximized {
        view: WebviewId(7),
        maximized: true,
    })?;
    command(control, "MAX-FACT")?;
    assert_eq!(read_control(control)?, "MAX-FACT Maximized 1:1 2 true");
    Ok(())
}

#[test]
fn authenticated_primary_window_calls_use_actual_reader_and_host_facts() -> TestResult {
    let mut role = RoleProcess::start()?;
    let (bound, mut control) = role.bind()?;
    let principal = bound.principal();
    let attempt = bound.attempt();
    assert_eq!(principal.owner(), RoleOwner::Primary);
    let (owner_tx, owner_rx) = mpsc::channel();
    let (native_tx, native_rx) = mpsc::channel();
    let router = router_for_bound(bound, owner_tx, native_tx)?;
    let handle = router.handle();
    {
        let current = handle.current.lock().expect("installed actual generation");
        let active = current.as_ref().expect("authenticated current");
        assert!(active.identity == PrimaryLinkIdentity::Role(principal));
        assert_eq!(active.attempt, attempt);
        assert!(!active.reader_stop.load(Ordering::Acquire));
    }
    assert_eq!(handle.readers.lock().expect("reader owners").len(), 1);
    command(&mut control, "PRE-SUB")?;
    assert_eq!(read_control(&mut control)?, "PRE-SUB KELD-CORE-045");
    assert!(native_rx.try_recv().is_err());
    command(&mut control, "SUBSCRIBE")?;
    assert_eq!(read_control(&mut control)?, "SUBSCRIBE 1");
    assert_eq!(publish_ready(&role, &handle, &owner_rx)?, attempt);
    command(&mut control, "READY")?;
    assert_eq!(read_control(&mut control)?, "READY 00");
    create_from_native_result(&mut control, &native_rx, &handle)?;
    setter_requires_native_confirmation(&mut control, &native_rx, &handle)?;

    command(&mut control, "STALE")?;
    assert_eq!(read_control(&mut control)?, "STALE KELD-CORE-038 2");
    assert!(native_rx.try_recv().is_err());
    command(&mut control, "HEALTHY")?;
    assert_eq!(
        native_rx.recv_timeout(BOUND)?,
        AppWindowCommand::SetFullScreen {
            view: WebviewId(7),
            full_screen: true
        }
    );
    assert_eq!(read_control(&mut control)?, "HEALTHY Accepted 2 true");

    // Product cleanup is asserted before the RoleProcess fallback Drop can run.
    command(&mut control, "WAIT-END")?;
    router.shutdown()?;
    assert_eq!(
        native_rx.try_recv().expect("actual shutdown teardown"),
        AppWindowCommand::DestroyWindow { view: WebviewId(7) }
    );
    assert!(handle.current.lock().expect("retired current").is_none());
    assert!(
        handle
            .readers
            .lock()
            .expect("joined reader owners")
            .is_empty()
    );
    assert_eq!(read_control(&mut control)?, "ENDED KELD-IPC-022 2");
    handle.window_native_event(AppWindowEvent::WindowReleased { view: WebviewId(7) })?;
    handle.finish_window_facts()?;
    assert!(native_rx.try_recv().is_err());
    assert!(matches!(role.finish()?, SupervisorOutcome::Stopped));
    drop(control);
    // Close the retained listener too, after Stopped has been asserted. Its
    // emergency shutdown branch is not used as the product cleanup oracle.
    drop(role);
    Ok(())
}

#[test]
fn authenticated_primary_close_verdicts_and_destroy_wait_for_ui_release() -> TestResult {
    let mut role = RoleProcess::start()?;
    let (bound, mut control) = role.bind()?;
    assert_eq!(bound.principal().owner(), RoleOwner::Primary);
    let (owner_tx, owner_rx) = mpsc::channel();
    let (native_tx, native_rx) = mpsc::channel();
    let router = router_for_bound(bound, owner_tx, native_tx)?;
    let handle = router.handle();
    command(&mut control, "SUBSCRIBE")?;
    assert_eq!(read_control(&mut control)?, "SUBSCRIBE 1");
    publish_ready(&role, &handle, &owner_rx)?;
    command(&mut control, "READY")?;
    assert_eq!(read_control(&mut control)?, "READY 00");
    create_from_native_result(&mut control, &native_rx, &handle)?;
    command(&mut control, "REQUEST-CLOSE")?;
    assert_eq!(
        read_control(&mut control)?,
        "REQUEST-CLOSE CloseRequested 1 2 CloseRequested"
    );
    command(&mut control, "MERGE-CLOSE")?;
    assert_eq!(
        read_control(&mut control)?,
        "MERGE-CLOSE Merged 1 2 CloseRequested"
    );
    command(&mut control, "VETO-CLOSE")?;
    assert_eq!(read_control(&mut control)?, "VETO-CLOSE ReplyApplied 1 2");
    command(&mut control, "REQUEST-CLOSE")?;
    assert_eq!(
        read_control(&mut control)?,
        "REQUEST-CLOSE CloseRequested 2 3 CloseRequested"
    );
    command(&mut control, "STALE-CLOSE")?;
    assert_eq!(read_control(&mut control)?, "STALE-CLOSE ReplyStale 2 3");
    assert!(
        native_rx.try_recv().is_err(),
        "pending/veto/stale never request native destruction"
    );
    command(&mut control, "DESTROY-PAIR")?;
    assert_eq!(
        native_rx.recv_timeout(BOUND)?,
        AppWindowCommand::DestroyWindow { view: WebviewId(7) }
    );
    assert_eq!(read_control(&mut control)?, "DESTROY-WAIT AlreadyClosing 3");
    assert!(
        native_rx.try_recv().is_err(),
        "two real calls cause one teardown"
    );
    handle.window_native_event(AppWindowEvent::WindowReleased { view: WebviewId(7) })?;
    assert_eq!(
        read_control(&mut control)?,
        "DESTROY-PAIR Destroyed Destroyed Destroyed 4"
    );
    command(&mut control, "WAIT-END")?;
    router.shutdown()?;
    assert_eq!(read_control(&mut control)?, "ENDED KELD-IPC-022 4");
    assert!(handle.current.lock().expect("retired current").is_none());
    assert!(handle.readers.lock().expect("joined readers").is_empty());
    assert!(
        native_rx.try_recv().is_err(),
        "shutdown cannot tear down an already released window"
    );
    handle.finish_window_facts()?;
    assert!(matches!(role.finish()?, SupervisorOutcome::Stopped));
    drop(control);
    drop(role);
    Ok(())
}

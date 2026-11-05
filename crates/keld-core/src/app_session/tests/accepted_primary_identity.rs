//! KEL-97 authenticated identity proof; no OS-containment or grant claim.
#[cfg(target_os = "macos")]
use super::super::MacBoundStart;
use super::super::{PrimaryCallerContext, dispatch_primary_echo};
use super::*;
use keld_runtime::primary::{
    BoundPrimaryGeneration, PrimaryRoleConfig, PrimaryRoleSupervisor, RoleOwner,
};
use std::io::{BufRead as _, BufReader, Write as _};
use std::net::{TcpListener, TcpStream};

const BOUND: Duration = Duration::from_secs(20);

/// Actual role coordinator owns every Bun child/restart/reap, including unwind.
struct AcceptedRole {
    supervisor: PrimaryRoleSupervisor,
    listener: TcpListener,
    finished: bool,
}

impl AcceptedRole {
    fn start(config: PrimaryRoleConfig) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("private fixture control");
        listener.set_nonblocking(true).expect("bounded accept poll");
        let script = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/app_session/tests/fixtures/accepted_primary_identity_role.ts");
        let supervisor = PrimaryRoleSupervisor::start_with_bound_generations(
            config
                .arg(script)
                .arg(
                    listener
                        .local_addr()
                        .expect("owned port")
                        .port()
                        .to_string(),
                )
                .env_remove("KELD_KIPC_TEST_HOOKS"),
        )
        .expect("existing authenticated coordinator");
        Self {
            supervisor,
            listener,
            finished: false,
        }
    }

    fn finish(&mut self) -> Option<keld_runtime::SupervisorOutcome> {
        let deadline = Instant::now() + BOUND;
        self.supervisor.shutdown();
        loop {
            if Instant::now() >= deadline {
                return None;
            }
            if let Some(outcome) = self.supervisor.try_wait_for_outcome() {
                self.finished = true;
                return Some(outcome);
            }
            thread::yield_now();
        }
    }

    fn bind(&self) -> (BoundPrimaryGeneration, BufReader<TcpStream>) {
        let deadline = Instant::now() + BOUND;
        let stream = loop {
            assert!(Instant::now() < deadline, "actual child control absent");
            match self.listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => thread::yield_now(),
                Err(error) => panic!("owned control accept: {error}"),
            }
        };
        stream
            .set_nonblocking(false)
            .expect("accepted control stream is blocking");
        stream
            .set_read_timeout(Some(BOUND))
            .expect("bounded control read");
        stream
            .set_write_timeout(Some(BOUND))
            .expect("bounded control write");
        let bound = self
            .supervisor
            .recv_bound_generation(BOUND)
            .expect("actual HELLO accepted Bound");
        let mut control = BufReader::new(stream);
        assert_eq!(read_control(&mut control), "BOUND");
        (bound, control)
    }
}

impl Drop for AcceptedRole {
    fn drop(&mut self) {
        if !self.finished {
            self.supervisor.shutdown();
        }
    }
}

fn read_control(control: &mut BufReader<TcpStream>) -> String {
    let mut line = String::new();
    assert!(
        control
            .read_line(&mut line)
            .expect("actual controlled role reply")
            > 0
    );
    line.trim_end().to_owned()
}

#[cfg(unix)]
#[test]
fn accepted_app_bound_identity_is_refused_by_primary_admission() {
    let mut role = AcceptedRole::start(PrimaryRoleConfig::app_bound("bun"));
    let (bound, _control) = role.bind();
    assert_eq!(bound.principal().owner(), RoleOwner::AppBound);
    // This observes typed owner refusal at shipping admission. It does not
    // assert that an app CALL was submitted or independently census handler effects.
    let (owner, _commands) = mpsc::channel();
    let (window, _events) = mpsc::channel();
    let result = PrimaryRouter::start_bound(
        bound,
        window,
        PlatformPrimaryOwnerHandle { command_tx: owner },
        SessionShutdownState::new(),
        None,
        #[cfg(target_os = "macos")]
        MacBoundStart {
            auth_configuration: AuthConfiguration::new(),
            presentation: None,
            facade_windows: false,
        },
    );
    let Err(error) = result else {
        panic!("accepted app-bound role entered primary router");
    };
    assert!(
        error.to_string().contains("KELD-CORE-037"),
        "existing typed admission fault"
    );
    assert!(
        matches!(
            role.finish(),
            Some(keld_runtime::SupervisorOutcome::Stopped)
        ),
        "actual role terminal cleanup"
    );
}

#[cfg(unix)]
#[test]
fn accepted_primary_identity_fences_caller_context_before_healthy_echo() {
    let mut primary = AcceptedRole::start(PrimaryRoleConfig::primary("bun"));
    let (bound, mut control) = primary.bind();
    let principal = bound.principal();
    assert_eq!(principal.owner(), RoleOwner::Primary);

    let mut app_bound = AcceptedRole::start(PrimaryRoleConfig::app_bound("bun"));
    let (foreign_bound, _foreign_control) = app_bound.bind();
    let foreign_principal = foreign_bound.principal();
    assert_eq!(foreign_principal.owner(), RoleOwner::AppBound);

    let (owner, _commands) = mpsc::channel();
    let (window, _events) = mpsc::channel();
    let router = PrimaryRouter::start_bound(
        bound,
        window,
        PlatformPrimaryOwnerHandle { command_tx: owner },
        SessionShutdownState::new(),
        None,
        #[cfg(target_os = "macos")]
        MacBoundStart {
            auth_configuration: AuthConfiguration::new(),
            presentation: None,
            facade_windows: false,
        },
    )
    .expect("actual authenticated primary router");
    let handle = router.handle();
    let caller = {
        let current = handle.current.lock().expect("current primary lease");
        let active = current.as_ref().expect("accepted generation installed");
        assert!(active.identity == PrimaryLinkIdentity::Role(principal));
        PrimaryCallerContext {
            attempt: active.attempt,
            identity: active.identity,
            reader_stop: Arc::clone(&active.reader_stop),
        }
    };
    assert!(!caller.reader_stop.load(Ordering::Acquire));

    // These are deliberately invalid CONTEXTS, not fabricated role identities:
    // both principals were minted by actual Runtime coordinators after HELLO.
    // Keep the real attempt and reader token fixed to isolate the identity check.
    let mut wrong_role = caller.clone();
    wrong_role.identity = PrimaryLinkIdentity::Role(foreign_principal);
    let mut wrong_reader = caller.clone();
    wrong_reader.reader_stop = Arc::new(AtomicBool::new(false));
    for refused in [&wrong_role, &wrong_reader] {
        // If either fence admits this request, the shipping Echo codec rejects
        // its truncated varint and this assertion fails. No answer is written.
        dispatch_primary_echo(
            &handle.current,
            &handle.shutdown,
            refused,
            CorrelationId(97),
            &[0xff],
        )
        .expect("foreign caller must be inert before payload decode");
    }

    // A constant-refusal implementation cannot pass: the real Bun caller must
    // still get its exact decoded Echo response through the installed reader.
    control
        .get_mut()
        .write_all(b"ECHO\n")
        .expect("request actual Echo");
    assert_eq!(read_control(&mut control), "ECHO-OK");

    handle.shutdown.stop_reader();
    assert!(caller.reader_stop.load(Ordering::Acquire));
    dispatch_primary_echo(
        &handle.current,
        &handle.shutdown,
        &caller,
        CorrelationId(98),
        &[0xff],
    )
    .expect("the stopped actual reader must be inert before payload decode");
    router.shutdown().expect("actual router reader joined");
    drop(foreign_bound);
    for role in [&mut primary, &mut app_bound] {
        assert!(
            matches!(
                role.finish(),
                Some(keld_runtime::SupervisorOutcome::Stopped)
            ),
            "actual Runtime child and bootstrap cleanup"
        );
    }
}

#[cfg(windows)]
#[test]
#[allow(
    clippy::too_many_lines,
    reason = "keep both actual link lifetimes, crossed-call controls, and explicit cleanup in one scenario"
)]
fn accepted_primary_links_keep_equal_principals_bound_to_original_reader() {
    let mut primary_a = AcceptedRole::start(PrimaryRoleConfig::primary("bun"));
    let mut primary_b = AcceptedRole::start(PrimaryRoleConfig::primary("bun"));
    assert_ne!(
        primary_a.listener.local_addr().expect("control A address"),
        primary_b.listener.local_addr().expect("control B address"),
        "independent control ports give the two Echo payloads distinct markers"
    );
    let (bound_a, mut control_a) = primary_a.bind();
    let (bound_b, mut control_b) = primary_b.bind();
    let principal_a = bound_a.principal();
    let principal_b = bound_b.principal();
    assert_eq!(principal_a.owner(), RoleOwner::Primary);
    assert_eq!(
        principal_a, principal_b,
        "both coordinators begin at generation one"
    );

    let (owner_a, commands_a) = mpsc::channel();
    let (window_a, events_a) = mpsc::channel();
    let router_a = PrimaryRouter::start_bound(
        bound_a,
        window_a,
        PlatformPrimaryOwnerHandle {
            command_tx: owner_a,
        },
        SessionShutdownState::new(),
        None,
        super::super::AppReadyTrigger::NavigationReady,
    )
    .expect("authenticated primary router A");
    let (owner_b, commands_b) = mpsc::channel();
    let (window_b, events_b) = mpsc::channel();
    let router_b = PrimaryRouter::start_bound(
        bound_b,
        window_b,
        PlatformPrimaryOwnerHandle {
            command_tx: owner_b,
        },
        SessionShutdownState::new(),
        None,
        super::super::AppReadyTrigger::NavigationReady,
    )
    .expect("authenticated primary router B");
    let handle_a = router_a.handle();
    let handle_b = router_b.handle();
    let caller_a = {
        let current = handle_a.current.lock().expect("primary A lease");
        let active = current.as_ref().expect("A generation installed");
        assert!(active.identity == PrimaryLinkIdentity::Role(principal_a));
        PrimaryCallerContext {
            attempt: active.attempt,
            identity: active.identity,
            reader_stop: Arc::clone(&active.reader_stop),
        }
    };
    let caller_b = {
        let current = handle_b.current.lock().expect("primary B lease");
        let active = current.as_ref().expect("B generation installed");
        assert!(active.identity == PrimaryLinkIdentity::Role(principal_b));
        PrimaryCallerContext {
            attempt: active.attempt,
            identity: active.identity,
            reader_stop: Arc::clone(&active.reader_stop),
        }
    };
    assert_eq!(caller_a.attempt, caller_b.attempt);
    assert!(caller_a.identity == caller_b.identity);
    assert!(!Arc::ptr_eq(&caller_a.reader_stop, &caller_b.reader_stop));
    assert!(!caller_a.reader_stop.load(Ordering::Acquire));
    assert!(!caller_b.reader_stop.load(Ordering::Acquire));

    // Both contexts came from real authenticated readers. Only their retained
    // link instances differ: the crossed malformed request must not reach decode.
    let crossed_results =
        [(&handle_a, &caller_b), (&handle_b, &caller_a)].map(|(target, foreign)| {
            dispatch_primary_echo(
                &target.current,
                &target.shutdown,
                foreign,
                CorrelationId(97),
                &[0xff],
            )
        });

    // These control messages trigger genuine WorkerLink calls. Each child
    // checks its own distinct exact message and count from the app-link reply.
    for control in [&mut control_a, &mut control_b] {
        control
            .get_mut()
            .write_all(b"ECHO\n")
            .expect("request original caller Echo");
        assert_eq!(read_control(control), "ECHO-OK");
    }

    handle_a.shutdown.stop_reader();
    assert!(caller_a.reader_stop.load(Ordering::Acquire));
    assert!(!caller_b.reader_stop.load(Ordering::Acquire));
    let stopped_result = dispatch_primary_echo(
        &handle_a.current,
        &handle_a.shutdown,
        &caller_a,
        CorrelationId(98),
        &[0xff],
    );
    control_b
        .get_mut()
        .write_all(b"ECHO\n")
        .expect("unrelated original caller remains healthy");
    assert_eq!(read_control(&mut control_b), "ECHO-OK");

    router_a.shutdown().expect("primary A reader joined");
    router_b.shutdown().expect("primary B reader joined");
    // A healthy session with no window registry sends no owner or window fault.
    // Retained handles keep these senders live until after the observations.
    assert!(matches!(
        commands_a.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));
    assert!(matches!(
        commands_b.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));
    assert!(matches!(
        events_a.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));
    assert!(matches!(
        events_b.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));
    for role in [&mut primary_a, &mut primary_b] {
        assert!(
            matches!(
                role.finish(),
                Some(keld_runtime::SupervisorOutcome::Stopped)
            ),
            "actual Runtime child and bootstrap cleanup"
        );
        let ledger = role.supervisor.crash_ledger();
        assert_eq!(ledger.count, 0, "unexpected child crash: {ledger:?}");
        assert!(ledger.last.is_none(), "unexpected crash detail: {ledger:?}");
        assert_eq!(
            ledger.self_termination_count, 0,
            "unexpected child self-termination: {ledger:?}"
        );
        assert!(
            ledger.last_self_termination.is_none(),
            "unexpected self-termination detail: {ledger:?}"
        );
    }
    // Guard mutants reach the malformed codec without writing. Check those
    // results after normal cleanup so a deliberate failure cannot hide it.
    for result in crossed_results {
        result.expect("foreign actual reader must be inert before payload decode");
    }
    stopped_result.expect("stopped actual reader must be inert before payload decode");
}

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
    fn start(owner: RoleOwner) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("private fixture control");
        listener.set_nonblocking(true).expect("bounded accept poll");
        let script = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/app_session/tests/fixtures/accepted_primary_identity_role.ts");
        let config = match owner {
            RoleOwner::Primary => PrimaryRoleConfig::primary("bun"),
            RoleOwner::AppBound => PrimaryRoleConfig::app_bound("bun"),
        };
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

#[test]
fn accepted_app_bound_identity_is_refused_by_primary_admission() {
    let mut role = AcceptedRole::start(RoleOwner::AppBound);
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

#[test]
fn accepted_primary_identity_fences_caller_context_before_healthy_echo() {
    let mut primary = AcceptedRole::start(RoleOwner::Primary);
    let (bound, mut control) = primary.bind();
    let principal = bound.principal();
    assert_eq!(principal.owner(), RoleOwner::Primary);

    let mut app_bound = AcceptedRole::start(RoleOwner::AppBound);
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

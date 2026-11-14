//! KEL-97 authenticated identity proof; no OS-containment or grant claim.
#[cfg(target_os = "macos")]
use super::super::MacBoundStart;
#[cfg(windows)]
use super::super::WindowsBoundStart;
#[cfg(not(windows))]
use super::super::{PrimaryCallerContext, dispatch_primary_echo};
use super::*;
#[cfg(windows)]
use keld_runtime::primary::RolePrincipal;
use keld_runtime::primary::{
    BoundPrimaryGeneration, PrimaryRoleConfig, PrimaryRoleSupervisor, RoleOwner,
};
use std::io::{BufReader, Write as _};
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
            #[cfg(not(windows))]
            RoleOwner::AppBound => PrimaryRoleConfig::app_bound("bun"),
            #[cfg(windows)]
            RoleOwner::AppBound => panic!("app-bound fixture is unavailable on Windows"),
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
        std::io::BufRead::read_line(control, &mut line).expect("actual controlled role reply") > 0
    );
    line.trim_end().to_owned()
}

#[cfg(not(windows))]
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
        #[cfg(windows)]
        WindowsBoundStart {
            window_native_support: None,
            original_selection: None,
            renderer_outcomes: None,
        },
        #[cfg(target_os = "macos")]
        MacBoundStart {
            auth_configuration: AuthConfiguration::new(),
            presentation: None,
            facade_windows: false,
            original_selection: None,
            renderer_outcomes: None,
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

#[cfg(not(windows))]
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
        #[cfg(windows)]
        WindowsBoundStart {
            window_native_support: None,
            original_selection: None,
            renderer_outcomes: None,
        },
        #[cfg(target_os = "macos")]
        MacBoundStart {
            auth_configuration: AuthConfiguration::new(),
            presentation: None,
            facade_windows: false,
            original_selection: None,
            renderer_outcomes: None,
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
fn assert_same_content_admission_is_not_current(
    handle: &PrimaryRouterHandle,
    snapshot: &GuardSnapshot,
    path: &Path,
    digest: [u8; 32],
    admitted: keld_guard::ProfileState,
    attempt: u32,
    principal: RolePrincipal,
) {
    let other_verified = load_verified_manifest(
        File::open(path).expect("second retained file"),
        path.to_owned(),
        digest,
    )
    .expect("same verified policy bytes");
    let other_snapshot = GuardSnapshot::prepare_uncontained(other_verified, admitted)
        .expect("separately prepared guard snapshot");
    assert!(!Arc::ptr_eq(&snapshot.fs, &other_snapshot.fs));
    assert!(handle.is_current(attempt), "original admission is current");
    let original_binding = {
        let mut current = handle.current.lock().expect("active generation");
        current
            .as_mut()
            .expect("bound generation")
            .profile_binding
            .replace(Arc::clone(&other_snapshot.fs))
            .expect("original admitted binding")
    };
    assert_eq!(
        handle.observed_profile_binding(attempt, principal),
        Some((keld_guard::ProfileState::Legacy, digest)),
        "equal profile and digest do not establish object identity"
    );
    assert!(!handle.is_current(attempt), "different admission is stale");
    handle
        .current
        .lock()
        .expect("active generation")
        .as_mut()
        .expect("bound generation")
        .profile_binding = Some(original_binding);
    assert!(
        handle.is_current(attempt),
        "original admission is current again"
    );
}

#[cfg(windows)]
#[test]
fn windows_authenticated_generations_bind_retained_admission_to_the_current_principal() {
    use sha2::{Digest as _, Sha256};

    let _custody = keld_runtime::windows_job::install_host_death_job()
        .expect("install process-lifetime custody before starting any role");
    let root = tempfile::tempdir().expect("owned policy root");
    let path = root.path().join("policy.jsonc");
    let bytes = br#"{"profile":"legacy"}"#;
    std::fs::write(&path, bytes).expect("initial policy");
    let digest: [u8; 32] = Sha256::digest(bytes).into();
    let verified = load_verified_manifest(
        File::open(&path).expect("retained file"),
        path.clone(),
        digest,
    )
    .expect("verified policy");
    let admitted = super::super::profile_preflight::validate_profile(3, &verified, false)
        .expect("actual uncontained admission");
    let snapshot =
        GuardSnapshot::prepare_uncontained(verified, admitted).expect("retained guard snapshot");
    let mut role = AcceptedRole::start(RoleOwner::Primary);
    let (g1, mut control) = role.bind();
    let p1 = g1.principal();
    let a1 = g1.attempt();
    let (owner, _commands) = mpsc::channel();
    let (window, _events) = mpsc::channel();
    let router = PrimaryRouter::start_bound(
        g1,
        window,
        PlatformPrimaryOwnerHandle { command_tx: owner },
        SessionShutdownState::new(),
        Some(&snapshot),
        WindowsBoundStart {
            window_native_support: None,
            original_selection: None,
            renderer_outcomes: None,
        },
    )
    .expect("authenticated initial generation");
    let handle = router.handle();
    assert_eq!(
        handle.observed_profile_binding(a1, p1),
        Some((keld_guard::ProfileState::Legacy, digest))
    );
    assert_same_content_admission_is_not_current(
        &handle, &snapshot, &path, digest, admitted, a1, p1,
    );
    // An external custody probe may hold this exact test process after the
    // real bound publication. The marker conveys phase only, not authority.
    if std::env::var_os("KELD_W6_CUSTODY_HOLD").as_deref() == Some(std::ffi::OsStr::new("1")) {
        println!("KELD_W6_BOUND");
        std::io::stdout()
            .flush()
            .expect("flush custody phase marker");
        let mut line = String::new();
        std::io::stdin()
            .read_line(&mut line)
            .expect("wait for custody probe");
    }
    std::fs::write(&path, br#"{"profile":"strict"}"#).expect("replace path policy after capture");
    control
        .get_mut()
        .write_all(b"CRASH\n")
        .expect("crash first role");
    control.get_mut().flush().expect("flush crash");
    let (g2, _control2) = role.bind();
    let p2 = g2.principal();
    let a2 = g2.attempt();
    assert_ne!(p1, p2, "fresh host-minted principal");
    assert_ne!(a1, a2, "fresh coordinator attempt");
    handle
        .retire_generation(a1)
        .expect("retire first generation");
    assert_eq!(handle.observed_profile_binding(a1, p1), None);
    handle
        .install_generation_with_identity(a2, g2.into_stream(), PrimaryLinkIdentity::Role(p2))
        .expect("publish authenticated successor");
    assert_eq!(
        handle.observed_profile_binding(a2, p2),
        Some((keld_guard::ProfileState::Legacy, digest))
    );
    assert_eq!(
        handle.observed_profile_binding(a2, p1),
        None,
        "stale principal must not qualify successor"
    );
    assert_eq!(
        handle.observed_profile_binding(a1, p2),
        None,
        "stale attempt must not qualify successor"
    );
    router.shutdown().expect("router cleanup");
    assert!(matches!(
        role.finish(),
        Some(keld_runtime::SupervisorOutcome::Stopped)
    ));
}

//! KEL-97 authenticated identity proof; no OS-containment or grant claim.
#[cfg(target_os = "macos")]
use super::super::MacBoundStart;
#[cfg(not(windows))]
use super::super::{PrimaryCallerContext, dispatch_primary_echo};
use super::*;
use keld_runtime::primary::{
    BoundPrimaryGeneration, PrimaryRoleConfig, PrimaryRoleSupervisor, RoleOwner,
};
use std::io::{BufReader, Write as _};
use std::net::{TcpListener, TcpStream};

const BOUND: Duration = Duration::from_secs(20);

#[cfg(windows)]
#[path = "accepted_primary_identity/original_controls.rs"]
mod original_controls;
#[cfg(windows)]
#[path = "accepted_primary_identity/original_create.rs"]
mod original_create;

#[cfg(windows)]
fn expect_original_desktop_binding(
    router: &PrimaryRouter,
    commands: &mpsc::Receiver<AppWindowCommand>,
) {
    let handle = router.handle();
    let (attempt, reader) = {
        let current = handle.current.lock().expect("current owner");
        let active = current.as_ref().expect("authenticated primary");
        (active.attempt, Arc::clone(&active.reader_stop))
    };
    assert!(matches!(
        commands.recv_timeout(BOUND).expect("original desktop producer binding"),
        AppWindowCommand::WindowsDesktopProducer { capture }
            if capture.matches(attempt, &reader)
    ));
}

/// Actual role coordinator owns every Bun child/restart/reap, including unwind.
struct AcceptedRole {
    supervisor: PrimaryRoleSupervisor,
    listener: TcpListener,
    finished: bool,
}

impl AcceptedRole {
    fn start(owner: RoleOwner) -> Self {
        let config = match owner {
            RoleOwner::Primary => PrimaryRoleConfig::primary("bun"),
            #[cfg(not(windows))]
            RoleOwner::AppBound => PrimaryRoleConfig::app_bound("bun"),
            #[cfg(windows)]
            RoleOwner::AppBound => panic!("app-bound fixture is unavailable on Windows"),
        };
        Self::start_with_config(config)
    }

    fn start_with_config(config: PrimaryRoleConfig) -> Self {
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
        keld_wv::webview2::AppReadyTrigger::NavigationReady,
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
        keld_wv::webview2::AppReadyTrigger::NavigationReady,
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
        None, // This fixture binds policy to generations without a native window.
        None,
    )
    .expect("authenticated initial generation");
    let handle = router.handle();
    assert_eq!(
        handle.observed_profile_binding(a1, p1),
        Some((keld_guard::ProfileState::Legacy, digest))
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

#[cfg(windows)]
#[test]
fn windows_original_window_native_admission_requires_current_runtime_caller_and_registry_entry() {
    use super::super::PrimaryCallerContext;
    use super::super::graph_contract::tests::retained_original_window_resources;
    use super::super::window_registry::{CreateOutcome, OriginalWindowResources};
    use keld_ipc::window::{WindowGeneration, WindowRef};
    use std::sync::TryLockError;

    let _custody = keld_runtime::windows_job::install_host_death_job()
        .expect("install process-lifetime custody before starting a role");
    let (_fixture, source, preload) = retained_original_window_resources();
    let mut role = AcceptedRole::start(RoleOwner::Primary);
    let (first, mut control) = role.bind();
    let principal = first.principal();
    let (guardian, _commands) = mpsc::channel();
    let (window, _events) = mpsc::channel();
    let router = PrimaryRouter::start_bound(
        first,
        window,
        PlatformPrimaryOwnerHandle {
            command_tx: guardian,
        },
        SessionShutdownState::new(),
        None,
        Some(true),
        None,
    )
    .expect("authenticated Windows primary router");
    let handle = router.handle();
    let (caller, window) = {
        let _transition = handle.shutdown.transition_guard();
        assert!(handle.mark_window_registry_ready_guarded().expect("ready"));
        let current = handle.current.lock().expect("current owner");
        let active = current.as_ref().expect("authenticated primary");
        assert!(active.identity == PrimaryLinkIdentity::Role(principal));
        let caller = PrimaryCallerContext {
            identity: active.identity,
            attempt: active.attempt,
            reader_stop: Arc::clone(&active.reader_stop),
        };
        let created = handle
            .windows
            .as_ref()
            .expect("window service")
            .lock()
            .expect("registry owner")
            .create_original_for_caller(
                &caller,
                CorrelationId(1),
                (640, 480),
                (None, None),
                OriginalWindowResources { source, preload },
            )
            .expect("selected original window admitted");
        let CreateOutcome::Build { window, .. } = created else {
            panic!("first original creation must reserve its own native window");
        };
        (caller, window)
    };
    let witness = handle
        .renderer_role_scope(&caller)
        .expect("actual Runtime primary witness")
        .for_window(window);
    let windows = handle.windows.as_ref().expect("window service");
    let mut entered = 0;
    assert!(witness.try_admit_native(&mut || {
        entered += 1;
        assert!(matches!(
            handle.shutdown.transition.try_lock(),
            Err(TryLockError::WouldBlock)
        ));
        assert!(matches!(
            handle.current.try_lock(),
            Err(TryLockError::WouldBlock)
        ));
        assert!(matches!(windows.try_lock(), Err(TryLockError::WouldBlock)));
        true
    }));
    assert_eq!(entered, 1);
    assert!(handle.shutdown.transition.try_lock().is_ok());
    assert!(handle.current.try_lock().is_ok());
    assert!(windows.try_lock().is_ok());
    let foreign = PrimaryCallerContext {
        reader_stop: Arc::new(AtomicBool::new(false)),
        ..caller.clone()
    };
    let stale_window = WindowRef {
        incarnation: WindowGeneration(window.incarnation.0 + 1),
        ..window
    };
    for (context, reference) in [(&foreign, window), (&caller, stale_window)] {
        let refused = handle
            .renderer_role_scope(context)
            .map(|scope| scope.for_window(reference));
        assert!(
            refused.is_none_or(|witness| !witness.try_admit_native(&mut || {
                panic!("foreign Runtime allocation or window incarnation was admitted")
            }))
        );
    }

    writeln!(control.get_mut(), "CRASH").expect("crash first real Runtime role");
    control.get_mut().flush().expect("flush crash");
    let (second, _control) = role.bind();
    let successor = second.principal();
    assert_ne!(principal, successor);
    handle.retire_generation(caller.attempt).expect("retire A");
    handle
        .install_generation_with_identity(
            second.attempt(),
            second.into_stream(),
            PrimaryLinkIdentity::Role(successor),
        )
        .expect("install real Runtime successor");
    assert!(!witness.try_admit_native(&mut || panic!("retired A reached native admission")));
    let successor = {
        let current = handle.current.lock().expect("current owner");
        let active = current.as_ref().expect("authenticated successor");
        PrimaryCallerContext {
            identity: active.identity,
            attempt: active.attempt,
            reader_stop: Arc::clone(&active.reader_stop),
        }
    };
    let foreign_window = handle
        .renderer_role_scope(&successor)
        .map(|scope| scope.for_window(window));
    assert!(
        foreign_window.is_none_or(|witness| !witness.try_admit_native(&mut || {
            panic!("live successor entered predecessor's nonadoptable opening window")
        }))
    );
    router.shutdown().expect("router cleanup");
    assert!(matches!(
        role.finish(),
        Some(keld_runtime::SupervisorOutcome::Stopped)
    ));
}

// The shared Message owner admits nonces on macOS and exposes that owner to
// Linux Core fixtures. Windows does not have this dialog provider yet.
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn dialog_nonce_is_owned_by_the_authenticated_current_generation() {
    let mut primary = AcceptedRole::start(RoleOwner::Primary);
    let (bound, mut control) = primary.bind();
    let principal = bound.principal();
    let mut foreign = AcceptedRole::start(RoleOwner::AppBound);
    let (foreign_bound, _) = foreign.bind();
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
    let mut wrong_role = caller.clone();
    wrong_role.identity = PrimaryLinkIdentity::Role(foreign_bound.principal());
    let mut wrong_attempt = caller.clone();
    wrong_attempt.attempt += 1;
    for refused in [&wrong_role, &wrong_attempt] {
        super::super::with_primary_caller_locked(
            &handle.current,
            &handle.shutdown,
            refused,
            |_| panic!("foreign caller reached nonce owner"),
        )
        .expect("foreign context is inert");
    }
    super::super::with_primary_caller_locked(
        &handle.current,
        &handle.shutdown,
        &caller,
        |current| {
            let active = current.as_mut().expect("verified current generation");
            assert!(!active.accept_dialog_nonce(0));
            assert!(active.accept_dialog_nonce(1));
            assert!(!active.accept_dialog_nonce(1));
            assert!(active.accept_dialog_nonce(2));
            assert!(active.accept_dialog_nonce(u64::MAX));
            assert!(!active.accept_dialog_nonce(u64::MAX));
            Ok(())
        },
    )
    .expect("actual primary owns nonce sequence");
    control
        .get_mut()
        .write_all(b"ECHO\n")
        .expect("healthy call");
    assert_eq!(read_control(&mut control), "ECHO-OK");
    router.shutdown().expect("actual router reader joined");
    drop(foreign_bound);
    for role in [&mut primary, &mut foreign] {
        assert!(matches!(
            role.finish(),
            Some(keld_runtime::SupervisorOutcome::Stopped)
        ));
    }
}

/// This sends real guarded CALL frames from an authenticated Runtime-owned Bun
/// `WorkerLink`, not a fabricated Rust header. A denial must precede payload
/// decode, while an allowed parentless call returns the typed unsupported
/// result and never enqueues a native sheet.
#[cfg(target_os = "macos")]
#[test]
fn real_primary_dialog_guard_refuses_before_decode_and_unknown_cancel_has_no_ui() {
    for (manifest, command, expected) in [
        ("{}", "DIALOG_DENIED", "KELD-GUARD001"),
        (
            r#"{"app":{"system":["dialog"]}}"#,
            "DIALOG_ALLOWED_PARENTLESS",
            "KELD-NATIVE-010",
        ),
        (
            r#"{"app":{"system":["dialog"]}}"#,
            "DIALOG_UNKNOWN_CANCEL",
            "KELD-NATIVE-009",
        ),
    ] {
        use sha2::{Digest as _, Sha256};
        let temp = tempfile::tempdir().expect("private verified policy fixture");
        let path = temp.path().join("keld.permissions.jsonc");
        std::fs::write(&path, manifest).expect("exact policy bytes");
        let digest: [u8; 32] = Sha256::digest(manifest.as_bytes()).into();
        let verified = keld_guard::verified_manifest::load_verified_manifest(
            std::fs::File::open(&path).expect("retained manifest handle"),
            path,
            digest,
        )
        .expect("digest-qualified immutable policy");
        let snapshot =
            super::super::GuardSnapshot::prepare(verified).expect("actual verified owner prepared");
        let mut primary = AcceptedRole::start(RoleOwner::Primary);
        let (bound, mut control) = primary.bind();
        let (owner, _guardian_rx) = mpsc::channel();
        let (window, windows_rx) = mpsc::channel();
        let router = PrimaryRouter::start_bound(
            bound,
            window,
            PlatformPrimaryOwnerHandle { command_tx: owner },
            SessionShutdownState::new(),
            Some(&snapshot),
            MacBoundStart {
                auth_configuration: AuthConfiguration::new(),
                presentation: None,
                facade_windows: false,
            },
        )
        .expect("real guarded primary router");
        assert!(
            router.handle().dialog.is_some(),
            "guarded channel installed"
        );
        writeln!(control.get_mut(), "{command}").expect("send actual role command");
        assert_eq!(read_control(&mut control), format!("DIALOG-{expected}"));
        assert!(
            windows_rx.try_recv().is_err(),
            "denied, unsupported and unknown Cancel cannot enqueue native UI"
        );
        router.shutdown().expect("reader and original role joined");
        assert!(matches!(
            primary.finish(),
            Some(keld_runtime::SupervisorOutcome::Stopped)
        ));
    }
}

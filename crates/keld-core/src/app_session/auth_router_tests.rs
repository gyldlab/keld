//! Admission custody tests and actual socket/Native guard negative routes.
use std::fs::{self, File};

use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicBool, AtomicU32};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use keld_ipc::CallError;
use keld_ipc::codec::{decode, encode};
use keld_ipc::frame::{CorrelationId, FrameKind};
use keld_ipc::link::{AppLinkDeadlines as _, read_frame, write_frame};
use keld_native::auth::{AuthConfiguration, AuthProfileConfiguration, AuthRequest};
use sha2::{Digest as _, Sha256};

use super::super::{
    GuardianOwnerHandle, PrimaryRouter, PrimaryRouterHandle, SessionShutdownState,
    load_verified_manifest,
};
use super::{AUTH_CALL_CAPACITY, AuthSession, HashMap, Ordering, mpsc};

type FixtureResult<T> = Result<T, Box<dyn std::error::Error>>;

#[test]
fn logical_terminal_does_not_release_physical_leader_ticket() {
    let session = AuthSession::new();
    let leader = session
        .reserve(1, CorrelationId(11))
        .expect("reserve actual leader");
    let physical_lease = Arc::clone(&leader);
    assert!(session.take(&leader).expect("logical terminal"));
    drop(leader);
    assert_eq!(session.used.load(Ordering::Acquire), 1);
    drop(physical_lease); // models caller owner releasing only at Native Drained
    assert_eq!(session.used.load(Ordering::Acquire), 0);
}

#[test]
fn retired_ticket_cannot_remove_successor_with_reused_correlation() {
    let session = AuthSession::new();
    let old = session.reserve(1, CorrelationId(11)).expect("old caller");
    session.retire(1).expect("detach old caller");
    let new = session
        .reserve(2, CorrelationId(11))
        .expect("successor caller");
    assert!(!session.take(&old).expect("old terminal publication"));
    assert!(session.live(&new));
    assert_eq!(session.used.load(Ordering::Acquire), 2);
    session.take(&new).expect("new terminal");
    drop(old);
    drop(new);
    assert_eq!(session.used.load(Ordering::Acquire), 0);
}

#[test]
fn independent_auth_budget_supports_multiple_callers_and_rejects_duplicate() {
    let session = AuthSession::new();
    let tickets: Vec<_> = (1..=AUTH_CALL_CAPACITY)
        .map(|n| {
            session
                .reserve(1, CorrelationId(u32::try_from(n).expect("small capacity")))
                .expect("bounded caller")
        })
        .collect();
    assert_eq!(
        session.admission(1, CorrelationId(100)),
        super::super::GuardedCallAdmission::AtCapacity
    );
    assert!(session.reserve(1, CorrelationId(100)).is_err());
    assert!(session.reserve(1, CorrelationId(1)).is_err());
    assert_eq!(
        session.admission(1, CorrelationId(1)),
        super::super::GuardedCallAdmission::RejectSession
    );
    session.retire(1).expect("detach logical callers");
    assert_eq!(
        session.used.load(Ordering::Acquire),
        AUTH_CALL_CAPACITY,
        "leases still physically owned"
    );
    drop(tickets);
    assert_eq!(session.used.load(Ordering::Acquire), 0);
    assert_eq!(
        session.admission(2, CorrelationId(1)),
        super::super::GuardedCallAdmission::Admit
    );
}

struct Fixture {
    router: PrimaryRouter,
    client: UnixStream,
    _temp: tempfile::TempDir,
    finished: bool,
    fs: Option<super::super::GuardSnapshot>,
    // Reuse the same immutable verified policy to construct a genuine guard-denied result.
    verified: super::super::VerifiedManifest,
    // Retain the actual routing owner channels through router teardown.
    guardian: std::sync::mpsc::Receiver<super::super::GuardianOwnerCommand>,
    window: std::sync::mpsc::Receiver<super::super::AppWindowCommand>,
}
impl Fixture {
    fn new(manifest: &str, configuration: AuthConfiguration) -> FixtureResult<Self> {
        Self::new_with_fs(manifest, configuration, false)
    }
    fn new_with_fs(
        manifest: &str,
        configuration: AuthConfiguration,
        fs_enabled: bool,
    ) -> FixtureResult<Self> {
        let temp = tempfile::tempdir()?;
        let path = temp.path().join("keld.permissions.jsonc");
        fs::write(&path, manifest)?;
        let digest: [u8; 32] = Sha256::digest(manifest.as_bytes()).into();
        let verified = load_verified_manifest(File::open(&path)?, path, digest)?;
        let snapshot = if fs_enabled {
            Some(super::super::GuardSnapshot::prepare(verified.clone())?)
        } else {
            None
        };
        let (server, client) = UnixStream::pair()?;
        client.set_app_link_deadlines(Some(Duration::from_secs(5)))?;
        let (guardian_tx, guardian) = mpsc::channel();
        let (window_tx, window) = mpsc::channel();
        let handle = PrimaryRouterHandle {
            current: Arc::new(Mutex::new(None)),
            readers: Arc::new(Mutex::new(HashMap::new())),
            pending_echo: Arc::new(Mutex::new(None)),
            pending_echo_attempt: Arc::new(AtomicU32::new(0)),
            pending_echo_corr: Arc::new(AtomicU32::new(0)),
            next_host_corr: Arc::new(AtomicU32::new(1)),
            window_ready: Arc::new(AtomicBool::new(false)),
            last_window_closed: Arc::new(AtomicBool::new(false)),
            recovery_armed: Arc::new(AtomicBool::new(false)),
            last_revoked_attempt: Arc::new(AtomicU32::new(0)),
            shutdown: SessionShutdownState::new(),
            fs: snapshot.as_ref().map(super::super::GuardSnapshot::fs_weak),
            fs_worker_commands: None,
            auth: None,
            auth_commands: None,
            guardian: GuardianOwnerHandle {
                command_tx: guardian_tx,
            },
            window_commands: window_tx,
            quit_drain_hooks: Arc::default(),
        };
        let router = PrimaryRouter::finish_start(
            handle,
            snapshot
                .as_ref()
                .map(super::super::GuardSnapshot::take_initial_broker)
                .transpose()?,
            1,
            None,
            Some((verified.clone(), configuration, None)),
        )?;
        router
            .handle
            .install_prepared_generation(1, server)
            .map_err(|failure| failure.error)?;
        Ok(Self {
            router,
            client,
            _temp: temp,
            finished: false,
            fs: snapshot,
            verified,
            guardian,
            window,
        })
    }
    fn call_frame(
        &mut self,
        request: &AuthRequest,
        corr: u32,
    ) -> FixtureResult<(keld_ipc::frame::FrameHeader, Vec<u8>)> {
        write_frame(
            &mut self.client,
            FrameKind::Call,
            0,
            keld_ipc::channel_table::AUTH.id(),
            CorrelationId(corr),
            &encode(request)?,
        )?;
        Ok(read_frame(&mut self.client)?)
    }
    fn call(&mut self, request: &AuthRequest, corr: u32) -> FixtureResult<CallError> {
        let channel = keld_ipc::channel_table::AUTH.id();
        let (header, payload) = self.call_frame(request, corr)?;
        assert_eq!(
            (header.kind, header.channel, header.corr),
            (FrameKind::Err, channel, CorrelationId(corr))
        );
        Ok(decode(&payload)?)
    }
    fn finish(&mut self) -> Result<(), super::super::HostAppError> {
        self.finished = true;
        self.router.stop_and_join()
    }
    fn take_reader(
        &self,
        attempt: u32,
    ) -> FixtureResult<std::thread::JoinHandle<Result<(), super::super::HostAppError>>> {
        let mut readers = self
            .router
            .handle
            .readers
            .lock()
            .map_err(|_| "reader owner table poisoned")?;
        Ok(readers.remove(&attempt).ok_or("reader owner is absent")?)
    }
    fn retained_writer(&self) -> FixtureResult<UnixStream> {
        let current = self
            .router
            .handle
            .current
            .lock()
            .map_err(|_| "current generation lock poisoned")?;
        Ok(current
            .as_ref()
            .ok_or("current generation is absent")?
            .writer
            .try_clone()?)
    }
    fn acknowledge_prepare(&self) -> FixtureResult<()> {
        match self.guardian.recv_timeout(Duration::from_secs(5))? {
            super::super::GuardianOwnerCommand::PrepareAcceptedShutdown(reply) => {
                Ok(reply.send(Ok(()))?)
            }
            _ => Err("unexpected guardian prepare command".into()),
        }
    }
    fn acknowledge_tail_if_reached(&self) -> FixtureResult<bool> {
        match self.guardian.recv_timeout(Duration::from_secs(5)) {
            Ok(super::super::GuardianOwnerCommand::Shutdown(reply)) => {
                reply.send(Ok(()))?;
                Ok(true)
            }
            _ => Ok(false),
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if !self.finished {
            // An earlier test error/unwind still releases the real owner. Successful
            // paths check finish() in the test before this emergency fallback.
            if let Err(error) = self.router.stop_and_join() {
                eprintln!("emergency auth-fixture cleanup: {error}");
            }
        }
    }
}
fn begin(hint: Option<String>) -> AuthRequest {
    AuthRequest::Begin {
        provider: "entra-common".to_owned(),
        scopes: None,
        login_hint: hint,
    }
}

#[test]
fn actual_primary_auth_begin_not_granted_precedes_missing_client_and_bad_hint() -> FixtureResult<()>
{
    let mut fixture = Fixture::new("{}", AuthConfiguration::new())?;
    let error = fixture.call(&begin(Some("private\ninvalid".to_owned())), 41)?;
    assert_eq!(error.code, "KELD-GUARD001");
    assert!(!error.message.contains("private"));
    fixture.finish()?;
    Ok(())
}

#[test]
fn actual_primary_auth_begin_allow_without_client_returns_native_configuration_error()
-> FixtureResult<()> {
    let mut fixture = Fixture::new(
        r#"{"app":{"auth":{"begin":["https://login.microsoftonline.com/common"]}}}"#,
        AuthConfiguration::new(),
    )?;
    assert_eq!(fixture.call(&begin(None), 42)?.code, "KELD-AUTH-015");
    fixture.finish()?;
    Ok(())
}

#[test]
fn actual_primary_auth_begin_allow_with_client_validates_private_hint_before_effects()
-> FixtureResult<()> {
    let mut configuration = AuthConfiguration::new();
    configuration.insert(
        "entra-common".to_owned(),
        AuthProfileConfiguration {
            client_id: Some("literal-test-client".to_owned()),
            fixed_redirect: None,
            as_redirect: None,
        },
    );
    let mut fixture = Fixture::new(
        r#"{"app":{"auth":{"begin":["https://login.microsoftonline.com/common"]}}}"#,
        configuration,
    )?;
    let error = fixture.call(&begin(Some("private\ninvalid".to_owned())), 43)?;
    assert_eq!(error.code, "KELD-AUTH-010");
    assert!(!error.message.contains("private"));
    fixture.finish()?;
    Ok(())
}

#[test]
fn actual_primary_auth_token_unknown_account_uses_private_not_granted_constant() -> FixtureResult<()>
{
    let mut fixture = Fixture::new("{}", AuthConfiguration::new())?;
    let error = fixture.call(
        &AuthRequest::Token {
            account_id: "private-unknown-account".to_owned(),
            scopes: None,
        },
        44,
    )?;
    assert_eq!(error.code, "KELD-GUARD001");
    assert!(!error.message.contains("private-unknown-account"));
    assert!(!error.message.contains("unresolved"));
    assert!(!error.message.contains("https://"));
    fixture.finish()?;
    Ok(())
}

#[test]
fn auth_capacity_refuses_one_call_without_fatal_or_stream_desynchronization() -> FixtureResult<()> {
    let mut fixture = Fixture::new("{}", AuthConfiguration::new())?;
    let session = Arc::clone(
        fixture
            .router
            .handle
            .auth
            .as_ref()
            .ok_or("auth session absent")?,
    );
    assert_eq!(
        AUTH_CALL_CAPACITY, 32,
        "do not fix overflow by growing the bound"
    );
    let mut tickets: Vec<_> = (1..=32)
        .map(|corr| session.reserve(1, CorrelationId(corr)))
        .collect::<Result<_, _>>()?;
    let error = fixture.call(&begin(None), 100)?;
    assert_eq!(error.code, "KELD-IPC-005");
    assert!(error.message.contains("capacity"));
    assert!(error.message.contains("not admitted"));
    assert!(!error.message.contains("entra-common"));
    assert_eq!(session.used.load(Ordering::Acquire), 32);
    assert_eq!(
        session
            .pending
            .lock()
            .map_err(|_| "pending poisoned")?
            .len(),
        32
    );
    // A non-auth frame proves the refused frame body was fully consumed.
    write_frame(
        &mut fixture.client,
        FrameKind::Ping,
        0,
        keld_ipc::channel_table::ECHO.id(),
        CorrelationId(101),
        &[],
    )?;
    let (ping, payload) = read_frame(&mut fixture.client)?;
    assert_eq!(
        (ping.kind, ping.corr),
        (FrameKind::Ping, CorrelationId(101))
    );
    assert_eq!(payload.len(), 0);
    assert!(matches!(
        fixture.window.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));
    let released = tickets.pop().ok_or("missing retained ticket")?;
    assert!(session.take(&released)?);
    drop(released);
    assert_eq!(session.used.load(Ordering::Acquire), 31);
    assert_eq!(fixture.call(&begin(None), 102)?.code, "KELD-GUARD001");
    session.retire(1)?;
    drop(tickets);
    assert_eq!(session.used.load(Ordering::Acquire), 0);
    fixture.finish()?;
    Ok(())
}

#[test]
fn pre_quit_pending_auth_uses_wire_023_for_unknown_effect() -> FixtureResult<()> {
    let mut fixture = Fixture::new("{}", AuthConfiguration::new())?;
    let session = Arc::clone(
        fixture
            .router
            .handle
            .auth
            .as_ref()
            .ok_or("auth session absent")?,
    );
    // Real admission owner state, retained by the pending map. No Native work
    // is launched: this row isolates the terminal answer's effect claim.
    drop(session.reserve(1, CorrelationId(50))?);
    write_frame(
        &mut fixture.client,
        FrameKind::Call,
        0,
        keld_ipc::LIFECYCLE_CHANNEL,
        CorrelationId(51),
        &encode(&keld_ipc::LifecycleRequest::Quit)?,
    )?;
    fixture.acknowledge_prepare()?;
    let (quit, _) = read_frame(&mut fixture.client)?;
    assert_eq!(
        (quit.kind, quit.channel, quit.corr),
        (
            FrameKind::Reply,
            keld_ipc::LIFECYCLE_CHANNEL,
            CorrelationId(51)
        )
    );
    let (terminal, bytes) = read_frame(&mut fixture.client)?;
    // Preserve the actual terminal result, then complete normal product cleanup
    // before evaluating the expected old-024 failure. Drop is not the oracle.
    fixture.client.shutdown(std::net::Shutdown::Write)?;
    assert!(fixture.acknowledge_tail_if_reached()?);
    assert!(matches!(
        fixture.window.recv_timeout(Duration::from_secs(5))?,
        super::super::AppWindowCommand::Quit
    ));
    assert_eq!(session.used.load(Ordering::Acquire), 0);
    fixture.finish()?;
    assert_eq!(
        (terminal.kind, terminal.channel, terminal.corr),
        (
            FrameKind::Err,
            keld_ipc::channel_table::AUTH.id(),
            CorrelationId(50)
        )
    );
    assert!(
        bytes.starts_with(b"\x0cKELD-IPC-023"),
        "exact postcard code field must be 023, not 024"
    );
    let error: CallError = decode(&bytes)?;
    assert_eq!(error.code, "KELD-IPC-023");
    assert!(error.message.contains("host effect is unknown"));
    assert!(error.message.contains("Do not replay"));
    Ok(())
}

#[test]
fn accepted_quit_answers_auth_024_without_decoding_malformed_payload() -> FixtureResult<()> {
    use super::super::AppWindowCommand;
    let mut fixture = Fixture::new("{}", AuthConfiguration::new())?;
    write_frame(
        &mut fixture.client,
        FrameKind::Call,
        0,
        keld_ipc::LIFECYCLE_CHANNEL,
        CorrelationId(51),
        &encode(&keld_ipc::LifecycleRequest::Quit).expect("Quit payload"),
    )
    .expect("actual Quit CALL");
    fixture.acknowledge_prepare()?;
    let (quit, _) = read_frame(&mut fixture.client).expect("accepted Quit REPLY");
    assert_eq!(
        (quit.kind, quit.channel, quit.corr),
        (
            FrameKind::Reply,
            keld_ipc::LIFECYCLE_CHANNEL,
            CorrelationId(51)
        )
    );
    let auth = keld_ipc::channel_table::AUTH.id();
    write_frame(
        &mut fixture.client,
        FrameKind::Call,
        0,
        auth,
        CorrelationId(52),
        &[0xff],
    )
    .expect("post-Quit malformed auth CALL");
    fixture
        .client
        .shutdown(std::net::Shutdown::Write)
        .expect("peer ends its write direction");
    let (terminal, bytes) = read_frame(&mut fixture.client).expect("post-Quit auth terminal ERR");
    assert_eq!(
        (terminal.kind, terminal.channel, terminal.corr),
        (FrameKind::Err, auth, CorrelationId(52))
    );
    let error: CallError = decode(&bytes).expect("canonical terminal error");
    assert_eq!(
        error.code, "KELD-IPC-024",
        "malformed AUTH must never enter decoder/handler after accepted Quit"
    );
    match fixture
        .guardian
        .recv_timeout(Duration::from_secs(5))
        .expect("guardian tail")
    {
        super::super::GuardianOwnerCommand::Shutdown(reply) => {
            reply.send(Ok(())).expect("existing owner tail ack");
        }
        _ => panic!("unexpected guardian tail command"),
    }
    assert!(matches!(
        fixture
            .window
            .recv_timeout(Duration::from_secs(5))
            .expect("existing window Quit"),
        AppWindowCommand::Quit
    ));
    let session = fixture
        .router
        .handle
        .auth
        .as_ref()
        .expect("actual auth session");
    assert_eq!(
        session.used.load(Ordering::Acquire),
        0,
        "Quit must follow native all-drained + lease zero"
    );
    assert!(session.completion.lock().expect("drain observation").0);
    fixture.finish()?;
    Ok(())
}

fn poison_auth_pending(
    session: &Arc<AuthSession>,
    poison: impl FnOnce() + Send + 'static,
) -> FixtureResult<()> {
    let owner = Arc::clone(session);
    let joined = std::thread::spawn(move || -> Result<(), &'static str> {
        let _held = owner
            .pending
            .lock()
            .map_err(|_| "pending mutex was already poisoned")?;
        poison();
        Ok(())
    })
    .join();
    match joined {
        Err(_) if session.pending.is_poisoned() => Ok(()),
        Err(_) => Err("poison thread panicked without poisoning the pending mutex".into()),
        Ok(Err(error)) => Err(error.into()),
        Ok(Ok(())) => Err("poison callback returned without poisoning the pending mutex".into()),
    }
}

#[test]
fn auth_pending_poison_preserves_fs_echo_and_actual_link_retirement() -> FixtureResult<()> {
    use super::super::{PendingFsCall, PendingPrimaryEcho};
    use std::io::Read as _;
    let mut fixture = Fixture::new_with_fs("{}", AuthConfiguration::new(), true)?;
    let fs = Arc::clone(&fixture.fs.as_ref().expect("actual FS snapshot").fs);
    let lease = fs
        .begin_call(PendingFsCall {
            attempt: 1,
            correlation: CorrelationId(61),
        })
        .expect("actual FS pending lease");
    let (echo_tx, echo_rx) = mpsc::sync_channel(1);
    *fixture
        .router
        .handle
        .pending_echo
        .lock()
        .expect("actual echo slot") = Some(PendingPrimaryEcho {
        attempt: 1,
        correlation: CorrelationId(62),
        reply: echo_tx,
    });
    let session = Arc::clone(
        fixture
            .router
            .handle
            .auth
            .as_ref()
            .expect("actual auth owner"),
    );
    poison_auth_pending(&session, || {
        panic!("intentional actual auth pending poison")
    })?;
    assert!(session.pending.is_poisoned());
    let handle = fixture.router.handle.clone();
    let result;
    // A held duplicate prevents mere writer Drop from impersonating the required
    // explicit shutdown of the actual app link and all its duplicated descriptors.
    let writer_probe;
    {
        let _transition = handle.shutdown.transition_guard();
        let mut current = handle.current.lock().expect("current generation");
        writer_probe = current
            .as_ref()
            .expect("active writer")
            .writer
            .try_clone()
            .expect("actual descriptor duplicate");
        result = handle.retire_current_generation_locked(
            &mut current,
            1,
            "test retirement",
            "test link shutdown",
        );
    }
    let error = result.expect_err("preserve typed auth failure");
    assert!(error.to_string().contains("auth retirement"));
    let fs_retired = fs.pending_call_for(1).is_none();
    let echo_terminal = echo_rx
        .recv_timeout(Duration::from_secs(5))
        .is_ok_and(|reply| reply.is_err());
    fixture
        .client
        .set_app_link_read_deadline(Some(Duration::from_secs(5)))
        .expect("finite direct shutdown observation");
    let mut byte = [0];
    let link_shutdown = matches!(fixture.client.read(&mut byte), Ok(0));
    // Observe before all emergency harness cleanup: neither dropping lease nor
    // writer_probe may make the product oracle pass retroactively.
    assert_eq!(
        (fs_retired, echo_terminal, link_shutdown),
        (true, true, true),
        "auth poison short-circuited mandatory existing cleanup"
    );
    drop(writer_probe);
    drop(lease);
    if let Err(error) = fixture.finish() {
        eprintln!("expected poisoned-fixture cleanup: {error}");
    }
    Ok(())
}

#[test]
fn auth_and_fs_poison_are_both_carried_while_link_and_echo_still_retire() -> FixtureResult<()> {
    use super::super::PendingPrimaryEcho;
    use std::io::Read as _;
    let mut fixture = Fixture::new_with_fs("{}", AuthConfiguration::new(), true)?;
    let fs = Arc::clone(&fixture.fs.as_ref().expect("actual FS snapshot").fs);
    let fs_owner = Arc::clone(&fs);
    assert!(
        std::thread::spawn(move || {
            let _held = fs_owner.state.lock().expect("actual FS owner state");
            panic!("intentional actual FS state poison");
        })
        .join()
        .is_err()
    );
    let (echo_tx, echo_rx) = mpsc::sync_channel(1);
    *fixture
        .router
        .handle
        .pending_echo
        .lock()
        .expect("actual echo slot") = Some(PendingPrimaryEcho {
        attempt: 1,
        correlation: CorrelationId(63),
        reply: echo_tx,
    });
    poison_auth_pending(
        fixture.router.handle.auth.as_ref().expect("auth owner"),
        || panic!("intentional actual auth pending poison"),
    )?;
    let handle = fixture.router.handle.clone();
    let writer_probe;
    let error = {
        let _transition = handle.shutdown.transition_guard();
        let mut current = handle.current.lock().expect("actual current");
        writer_probe = current
            .as_ref()
            .expect("writer")
            .writer
            .try_clone()
            .expect("actual duplicate");
        handle
            .retire_current_generation_locked(
                &mut current,
                1,
                "test retirement",
                "test link shutdown",
            )
            .expect_err("independent typed cleanup failures")
    };
    let text = error.to_string();
    let echo_terminal = echo_rx
        .recv_timeout(Duration::from_secs(5))
        .is_ok_and(|reply| reply.is_err());
    fixture
        .client
        .set_app_link_read_deadline(Some(Duration::from_secs(5)))
        .expect("bounded actual close observation");
    let mut byte = [0];
    let closed = matches!(fixture.client.read(&mut byte), Ok(0));
    assert!(
        text.contains("auth retirement") && text.contains("filesystem retirement"),
        "lost independent cleanup error: {text}"
    );
    assert_eq!((echo_terminal, closed), (true, true));
    drop(writer_probe);
    if let Err(error) = fixture.finish() {
        eprintln!("expected poisoned-fixture cleanup: {error}");
    }
    Ok(())
}

#[test]
fn accepted_quit_auth_poison_still_quiesces_fs_and_reaches_guardian_tail() -> FixtureResult<()> {
    use std::io::Read as _;
    let mut fixture = Fixture::new_with_fs("{}", AuthConfiguration::new(), true)?;
    let fs = Arc::clone(&fixture.fs.as_ref().expect("actual FS snapshot").fs);
    let writer_probe = fixture.retained_writer()?;
    poison_auth_pending(
        fixture
            .router
            .handle
            .auth
            .as_ref()
            .expect("actual auth owner"),
        || panic!("intentional actual auth pending poison"),
    )?;
    write_frame(
        &mut fixture.client,
        FrameKind::Call,
        0,
        keld_ipc::LIFECYCLE_CHANNEL,
        CorrelationId(64),
        &encode(&keld_ipc::LifecycleRequest::Quit).expect("Quit literal"),
    )
    .expect("actual Quit CALL");
    fixture.acknowledge_prepare()?;
    let (reply, _) = read_frame(&mut fixture.client).expect("real accepted Quit REPLY");
    assert_eq!(
        (reply.kind, reply.channel, reply.corr),
        (
            FrameKind::Reply,
            keld_ipc::LIFECYCLE_CHANNEL,
            CorrelationId(64)
        )
    );
    let auth = keld_ipc::channel_table::AUTH.id();
    write_frame(
        &mut fixture.client,
        FrameKind::Call,
        0,
        auth,
        CorrelationId(65),
        &[0xff],
    )
    .expect("buffered post-Q malformed AUTH");
    fixture
        .client
        .shutdown(std::net::Shutdown::Write)
        .expect("role ends link after Quit reply");
    let buffered_024 = read_frame(&mut fixture.client).is_ok_and(|(header, payload)| {
        header.kind == FrameKind::Err
            && header.channel == auth
            && header.corr == CorrelationId(65)
            && decode::<CallError>(&payload).is_ok_and(|error| error.code == "KELD-IPC-024")
    });
    let reached_tail = fixture.acknowledge_tail_if_reached()?;
    let state = fs.state.lock().expect("existing FS observer");
    let quiesced = !state.accepting;
    let drained = state.in_flight == 0;
    drop(state);
    let reader = fixture.take_reader(1)?;
    let reader_error = reader
        .join()
        .expect("reader thread joined")
        .expect_err("auth poison remains a typed failure");
    assert!(reader_error.to_string().contains("auth retirement"));
    let generation_removed = fixture
        .router
        .handle
        .current
        .lock()
        .expect("current observer")
        .is_none();
    let mut byte = [0];
    let link_shutdown = matches!(fixture.client.read(&mut byte), Ok(0));
    assert_eq!(
        (
            quiesced,
            drained,
            reached_tail,
            buffered_024,
            generation_removed,
            link_shutdown
        ),
        (true, true, true, true, true, true),
        "auth poison after Q skipped existing cleanup"
    );
    drop(writer_probe);
    // Cleanup unproved is fatal: observe no successful existing window Quit.
    let commands: Vec<_> = fixture.window.try_iter().collect();
    assert!(
        !commands
            .iter()
            .any(|command| matches!(command, super::super::AppWindowCommand::Quit))
    );
    if let Err(error) = fixture.finish() {
        eprintln!("expected poisoned-fixture cleanup: {error}");
    }
    Ok(())
}

#[test]
fn accepted_quit_late_auth_tail_failure_wakes_exactly_one_ui_fatal() -> FixtureResult<()> {
    use super::super::AppWindowCommand;

    let mut fixture = Fixture::new("{}", AuthConfiguration::new())?;
    let session = Arc::clone(
        fixture
            .router
            .handle
            .auth
            .as_ref()
            .expect("actual host-owned Auth session"),
    );
    let (entered_tx, entered_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::sync_channel(1);
    *session
        .tail_exit_test_gate
        .lock()
        .expect("test-only Auth worker tail barrier") = Some((entered_tx, release_rx));

    // Use the real primary UnixStream and its correlated Quit reply, not a
    // fabricated worker outcome. Retire the generation BEFORE poisoning Auth:
    // the reader's earlier pre-retirement failure path must not send Fatal.
    write_frame(
        &mut fixture.client,
        FrameKind::Call,
        0,
        keld_ipc::LIFECYCLE_CHANNEL,
        CorrelationId(171),
        &encode(&keld_ipc::LifecycleRequest::Quit)?,
    )?;
    fixture.acknowledge_prepare()?;
    let (reply, payload) = read_frame(&mut fixture.client)?;
    assert_eq!(
        (reply.kind, reply.channel, reply.corr),
        (
            FrameKind::Reply,
            keld_ipc::LIFECYCLE_CHANNEL,
            CorrelationId(171)
        ),
        "actual primary IPC Quit must receive its original correlated reply"
    );
    assert!(
        matches!(
            decode::<keld_ipc::LifecycleResponse>(&payload)?,
            keld_ipc::LifecycleResponse::Quit
        ),
        "the correlated native frame must contain a valid lifecycle Quit reply"
    );
    fixture.client.shutdown(std::net::Shutdown::Write)?;
    assert!(
        fixture.acknowledge_tail_if_reached()?,
        "actual Guardian shutdown must still be reached"
    );
    entered_rx.recv_timeout(Duration::from_secs(5))?;
    fixture
        .take_reader(1)?
        .join()
        .expect("join original primary reader")
        .expect("clean pre-poison Quit reader retains no error");
    let generation_removed = fixture
        .router
        .handle
        .current
        .lock()
        .map_err(|_| "current generation lock poisoned")?
        .is_none();
    assert!(
        generation_removed,
        "the accepted Quit must retire its generation before the injected tail fault"
    );
    let completion = *session
        .completion
        .lock()
        .map_err(|_| "auth completion lock poisoned")?;
    assert_eq!(
        completion,
        (false, true),
        "the worker's physical drain has not yet completed, but Quit was accepted"
    );
    assert!(
        fixture.window.try_recv().is_err(),
        "no reader, Guardian or upstream Auth failure may preemptively mask the worker's missing terminal wake"
    );
    poison_auth_pending(&session, || {
        panic!("poison only after accepted Quit and original generation retirement");
    })?;
    release_tx.send(())?;

    let terminal = fixture.window.recv_timeout(Duration::from_secs(5));
    assert!(
        matches!(terminal, Ok(AppWindowCommand::Fatal)),
        "actual failed Auth worker must wake the original UI with Fatal; observed={terminal:?}"
    );
    let error = fixture
        .finish()
        .expect_err("failed terminal Auth cleanup must remain a typed error");
    assert_eq!(error.code(), "KELD-CORE-037");
    assert_eq!(error.phase, "auth tail");
    assert_eq!(error.detail, "caller lock poisoned");
    assert!(
        fixture.window.try_recv().is_err(),
        "late worker error may not emit duplicate Fatal or a false successful Quit"
    );
    Ok(())
}

#[test]
fn previously_reported_auth_failure_does_not_emit_second_terminal_fatal() -> FixtureResult<()> {
    use super::super::AppWindowCommand;

    let mut fixture = Fixture::new("{}", AuthConfiguration::new())?;
    let session = fixture
        .router
        .handle
        .auth
        .as_ref()
        .expect("actual host-owned Auth worker");
    // The existing record_failure path publishes the first Fatal when a
    // native ownership invariant fails, then continues draining the worker.
    // Its final Err must not publish a second independent terminal command.
    session.ownership_fault.store(true, Ordering::Release);
    let terminal = fixture.window.recv_timeout(Duration::from_secs(5));
    assert!(
        matches!(terminal, Ok(AppWindowCommand::Fatal)),
        "the existing Auth owner fault must wake the UI once; observed={terminal:?}"
    );
    let error = fixture
        .finish()
        .expect_err("the original typed Auth owner failure must remain fatal");
    assert_eq!(error.code(), "KELD-CORE-037");
    assert_eq!(error.phase, "auth ownership");
    assert!(
        fixture.window.try_recv().is_err(),
        "previously reported worker error must not add duplicate Fatal or a false Quit"
    );
    Ok(())
}

#[test]
fn cli_loss_auth_poison_preserves_echo_error_link_close_and_guardian_tail() -> FixtureResult<()> {
    use super::super::AppWindowCommand;
    use std::io::Read as _;
    let mut fixture = Fixture::new_with_fs("{}", AuthConfiguration::new(), true)?;
    let fs = Arc::clone(&fixture.fs.as_ref().expect("actual FS snapshot").fs);
    poison_auth_pending(
        fixture
            .router
            .handle
            .auth
            .as_ref()
            .expect("actual auth owner"),
        || panic!("intentional actual auth pending poison"),
    )?;
    let echo_owner = Arc::clone(&fixture.router.handle.pending_echo);
    assert!(
        std::thread::spawn(move || {
            let _held = echo_owner.lock().expect("actual Echo waiter lock");
            panic!("intentional actual Echo poison");
        })
        .join()
        .is_err()
    );
    let writer_probe = fixture.retained_writer()?;
    assert!(fixture.router.handle.shutdown.reserve_cli_lease_lost());
    let handle = fixture.router.handle.clone();
    let tail = std::thread::spawn(move || handle.cli_lease_lost());
    fixture.acknowledge_prepare()?;
    let reached_tail = fixture.acknowledge_tail_if_reached()?;
    let error = tail
        .join()
        .expect("actual CLI tail thread")
        .expect_err("retain independent cleanup errors");
    let detail = error.to_string();
    let current_removed = fixture
        .router
        .handle
        .current
        .lock()
        .expect("current observer")
        .is_none();
    let state = fs.state.lock().expect("FS actual cleanup observer");
    let fs_closed_and_drained = !state.accepting && state.in_flight == 0;
    drop(state);
    let mut byte = [0];
    let link_closed = matches!(fixture.client.read(&mut byte), Ok(0));
    assert!(
        detail.contains("auth retirement") && detail.contains("renderer Echo retirement"),
        "CLI loss discarded a cleanup error: {detail}"
    );
    assert_eq!(
        (
            reached_tail,
            current_removed,
            fs_closed_and_drained,
            link_closed
        ),
        (true, true, true, true)
    );
    assert!(
        !fixture
            .window
            .try_iter()
            .any(|command| matches!(command, AppWindowCommand::Quit)),
        "poisoned auth cleanup cannot publish successful Quit"
    );
    drop(writer_probe);
    if let Err(error) = fixture.finish() {
        eprintln!("expected poisoned-fixture cleanup: {error}");
    }
    Ok(())
}

struct AuthWriteProbe {
    session: Arc<AuthSession>,
    reader_stop: Arc<AtomicBool>,
    read_probe: UnixStream,
    publisher: std::thread::JoinHandle<Option<super::super::HostAppError>>,
}

fn start_auth_write_probe(fixture: &Fixture, correlation: u32) -> FixtureResult<AuthWriteProbe> {
    let handle = fixture.router.handle.clone();
    let session = Arc::clone(handle.auth.as_ref().ok_or("host auth owner absent")?);
    let (reader_stop, read_probe) = {
        let current = handle.current.lock().map_err(|_| "current poisoned")?;
        let active = current.as_ref().ok_or("current generation absent")?;
        (Arc::clone(&active.reader_stop), active.writer.try_clone()?)
    };
    let ticket = session.reserve(1, CorrelationId(correlation))?;
    let mut broker =
        keld_native::auth::AuthBroker::prepare(&fixture.verified, AuthConfiguration::new());
    let (state, ticket) = super::enter_auth_request(
        &mut broker,
        &handle,
        &session,
        &fixture.verified,
        super::AuthWork {
            ticket,
            request: begin(None),
            cancellation: Arc::clone(&reader_stop),
        },
        false,
    )?
    .ok_or("actual auth entry unavailable")?;
    let keld_native::auth::HandleState::Ready(ref response) = state else {
        return Err("empty policy did not produce a genuine Native Ready refusal".into());
    };
    let denied = response
        .clone()
        .into_response()
        .err()
        .ok_or("expected actual guard denial")?;
    if denied.code != "KELD-GUARD001" {
        return Err("unexpected Native guard-denied response".into());
    }
    // Fail only the actual outbound direction; the reader remains live until
    // publication retires it. Keep this duplicate to exclude Drop-only cleanup.
    read_probe.shutdown(std::net::Shutdown::Write)?;
    let publisher_session = Arc::clone(&session);
    let publisher = std::thread::spawn(move || {
        let mut operations = HashMap::new();
        let mut failure = None;
        super::dispatch_result(
            state,
            ticket,
            &mut operations,
            &handle,
            &publisher_session,
            &mut failure,
        );
        failure
    });
    Ok(AuthWriteProbe {
        session,
        reader_stop,
        read_probe,
        publisher,
    })
}

#[test]
fn auth_terminal_write_loss_keeps_host_owner_and_actual_successor_route() -> FixtureResult<()> {
    use super::super::{AppWindowCommand, GuardianOwnerCommand};
    use std::io::Read as _;
    let mut fixture = Fixture::new("{}", AuthConfiguration::new())?;
    let worker = fixture
        .router
        .auth_worker
        .as_ref()
        .ok_or("worker absent")?
        .join
        .as_ref()
        .ok_or("worker join absent")?
        .thread()
        .id();
    let probe = start_auth_write_probe(&fixture, 701)?;
    let AuthWriteProbe {
        session,
        reader_stop,
        mut read_probe,
        publisher,
    } = probe;
    let command = fixture.guardian.recv_timeout(Duration::from_secs(5))?;
    let GuardianOwnerCommand::FailGeneration(1, reply) = command else {
        return Err("auth write loss sent wrong existing guardian request".into());
    };
    let retired_before_ack = fixture
        .router
        .handle
        .current
        .lock()
        .map_err(|_| "current poisoned")?
        .is_none();
    let stopped_before_ack = reader_stop.load(Ordering::Acquire);
    reply.send(Ok(()))?;
    let failure = publisher
        .join()
        .map_err(|_| "publication caller panicked")?;
    let mut byte = [0];
    let read_closed = matches!(read_probe.read(&mut byte), Ok(0));
    let running = fixture.router.handle.shutdown.is_running();
    let accepting = session.accepting.load(Ordering::Acquire);
    let tail = session.tail.load(Ordering::Acquire);
    let no_fatal = !fixture
        .window
        .try_iter()
        .any(|event| matches!(event, AppWindowCommand::Fatal));
    fixture
        .take_reader(1)?
        .join()
        .map_err(|_| "retired reader panicked")??;
    let mut healthy = None;
    if failure.is_none() && running && accepting && !tail && no_fatal {
        let (server, client) = UnixStream::pair()?;
        client.set_app_link_deadlines(Some(Duration::from_secs(5)))?;
        fixture
            .router
            .handle
            .install_prepared_generation(2, server)
            .map_err(|failure| failure.error)?;
        fixture.client = client;
        healthy = Some(fixture.call(&begin(None), 702)?.code);
    }
    let same_owner = Arc::ptr_eq(
        &session,
        fixture
            .router
            .handle
            .auth
            .as_ref()
            .ok_or("successor auth owner absent")?,
    );
    let same_worker = fixture
        .router
        .auth_worker
        .as_ref()
        .ok_or("worker absent")?
        .join
        .as_ref()
        .ok_or("worker join absent")?
        .thread()
        .id()
        == worker;
    let zero = session.used.load(Ordering::Acquire);
    let cleanup = fixture.finish(); // Complete normal fixture cleanup before the baseline oracle.
    assert!(
        failure.is_none(),
        "successful retirement/ACK became Fatal: {failure:?}"
    );
    assert!((retired_before_ack && stopped_before_ack && read_closed) && running);
    assert!(accepting && !tail && no_fatal && same_owner && same_worker);
    assert_eq!(healthy.as_deref(), Some("KELD-GUARD001"));
    assert_eq!(zero, 0);
    cleanup?;
    Ok(())
}

#[test]
fn auth_terminal_write_loss_preserves_failed_ack_and_cleanup_as_fatal() -> FixtureResult<()> {
    use super::super::{AppWindowCommand, GuardianOwnerCommand};
    use std::io::Read as _;
    for cleanup_fault in [false, true] {
        let mut fixture = Fixture::new_with_fs("{}", AuthConfiguration::new(), cleanup_fault)?;
        if cleanup_fault {
            let fs = Arc::clone(&fixture.fs.as_ref().ok_or("FS snapshot absent")?.fs);
            assert!(
                std::thread::spawn(move || {
                    let _held = fs.state.lock().expect("actual cleanup state");
                    panic!("controlled FS retirement poison");
                })
                .join()
                .is_err()
            );
        }
        let AuthWriteProbe {
            session,
            reader_stop,
            mut read_probe,
            publisher,
        } = start_auth_write_probe(&fixture, 703)?;
        let command = fixture.guardian.recv_timeout(Duration::from_secs(5))?;
        let GuardianOwnerCommand::FailGeneration(1, reply) = command else {
            return Err("auth write loss sent wrong existing guardian request".into());
        };
        if cleanup_fault {
            reply.send(Ok(()))?;
        } else {
            reply.send(Err("controlled recovery acknowledgment failure".to_owned()))?;
        }
        let failure = publisher
            .join()
            .map_err(|_| "publication caller panicked")?;
        let mut byte = [0];
        let read_closed = matches!(read_probe.read(&mut byte), Ok(0));
        let retired = fixture
            .router
            .handle
            .current
            .lock()
            .map_err(|_| "current poisoned")?
            .is_none();
        let fatal = fixture
            .window
            .try_iter()
            .any(|event| matches!(event, AppWindowCommand::Fatal));
        let stopped = reader_stop.load(Ordering::Acquire);
        let tail = session.tail.load(Ordering::Acquire);
        let accepting = session.accepting.load(Ordering::Acquire);
        fixture
            .take_reader(1)?
            .join()
            .map_err(|_| "retired reader panicked")??;
        let cleanup = fixture.finish(); // Typed failed cleanup is retained, not a success fallback.
        let error = failure.ok_or("failed ACK/cleanup did not remain fatal")?;
        assert_eq!(error.code(), "KELD-CORE-037");
        assert_eq!(
            error.phase, "auth call error",
            "primary typed phase was replaced"
        );
        let rendered = error.to_string();
        assert!(rendered.contains("auth call error") && rendered.contains("cleanup"));
        if cleanup_fault {
            assert!(rendered.contains("filesystem retirement"));
        } else {
            assert!(rendered.contains("controlled recovery acknowledgment failure"));
        }
        assert!(retired && stopped && read_closed && fatal && tail && !accepting);
        if cleanup_fault {
            let error = cleanup.expect_err("actual poisoned fixture cleanup must remain typed");
            assert!(error.to_string().contains("filesystem"));
        } else {
            cleanup?;
        }
    }
    Ok(())
}

#[test]
fn intentional_live_retirement_keeps_reader_inert_and_auth_owner_for_successor() -> FixtureResult<()>
{
    use super::super::AppWindowCommand;
    let mut fixture = Fixture::new("{}", AuthConfiguration::new())?;
    let auth_owner = Arc::clone(
        fixture
            .router
            .handle
            .auth
            .as_ref()
            .expect("host auth owner"),
    );
    fixture
        .router
        .handle
        .retire_generation(1)
        .expect("existing intentional recovery retirement");
    let gap_presentation_stop = fixture.router.handle.prepare_auth_presentation_drive(false);
    let reader = fixture.take_reader(1)?;
    reader
        .join()
        .expect("old reader joined")
        .expect("intentionally retired reader stays inert");
    assert!(fixture.router.handle.shutdown.is_running());
    assert_eq!(
        gap_presentation_stop,
        keld_native::auth::AsMainStop::Running,
        "a retired role reader is not host shutdown"
    );
    assert!(
        !fixture
            .router
            .handle
            .shutdown
            .reader_stop
            .load(Ordering::Acquire)
    );
    assert!(
        !auth_owner.tail.load(Ordering::Acquire),
        "role retirement stopped host auth"
    );
    assert!(auth_owner.accepting.load(Ordering::Acquire));
    assert!(
        !fixture
            .window
            .try_iter()
            .any(|command| matches!(command, AppWindowCommand::Fatal))
    );
    let (server, client) = UnixStream::pair().expect("actual successor link");
    client
        .set_app_link_deadlines(Some(Duration::from_secs(5)))
        .expect("successor observer bound");
    fixture
        .router
        .handle
        .install_prepared_generation(2, server)
        .expect("existing successor publication");
    fixture.client = client;
    assert!(Arc::ptr_eq(
        &auth_owner,
        fixture.router.handle.auth.as_ref().expect("same owner")
    ));
    assert_eq!(
        fixture.call(&begin(None), 66)?.code,
        "KELD-GUARD001",
        "successor routes through actual retained auth worker"
    );
    fixture.finish()?;
    Ok(())
}

#[test]
fn anchor_close_keeps_host_auth_and_real_token_denials_live() -> FixtureResult<()> {
    use super::super::LifecycleEvent;
    for (manifest, expected) in [
        ("{}", "KELD-GUARD001"),
        (
            r#"{"app":{"auth":{"token":["https://login.microsoftonline.com/common"]}}}"#,
            "KELD-GUARD002",
        ),
    ] {
        let mut fixture = Fixture::new(manifest, AuthConfiguration::new())?;
        let owner = Arc::clone(
            fixture
                .router
                .handle
                .auth
                .as_ref()
                .ok_or("auth owner absent")?,
        );
        // This is the same operation the production main callback invokes.
        let presentation_stop = fixture.router.handle.prepare_auth_presentation_drive(true);
        let close = fixture.router.handle.signal_last_window_closed();
        let event = read_frame(&mut fixture.client);
        let token = fixture.call_frame(
            &AuthRequest::Token {
                account_id: "opaque-missing-after-close".to_owned(),
                scopes: None,
            },
            181,
        );
        let accepting = owner.accepting.load(Ordering::Acquire);
        let tail = owner.tail.load(Ordering::Acquire);
        let session_running = fixture.router.handle.shutdown.is_running();
        let global_reader_stop = fixture
            .router
            .handle
            .shutdown
            .reader_stop
            .load(Ordering::Acquire);
        let fatal = fixture
            .window
            .try_iter()
            .any(|command| command == super::super::AppWindowCommand::Fatal);
        // Join the actual reader/Native worker before any predicate can panic.
        let cleanup = fixture.finish();
        assert_eq!(
            (accepting, tail),
            (true, false),
            "anchor close ended host Auth; cleanup: {cleanup:?}"
        );
        cleanup?;
        close?;
        let (event_header, event_payload) = event?;
        assert_eq!(
            (event_header.kind, event_header.channel),
            (FrameKind::Event, keld_ipc::LIFECYCLE_CHANNEL)
        );
        assert_eq!(
            decode::<LifecycleEvent>(&event_payload)?,
            LifecycleEvent::LastWindowClosed
        );
        assert_eq!(
            presentation_stop,
            keld_native::auth::AsMainStop::AnchorClosing,
            "main presentation must stop before its anchor retires"
        );
        assert!(session_running);
        assert!(!global_reader_stop);
        assert!(!fatal, "anchor-free Token became a session fatal");
        let (header, payload) = token?;
        assert_eq!(
            (header.kind, header.channel, header.corr),
            (
                FrameKind::Err,
                keld_ipc::channel_table::AUTH.id(),
                CorrelationId(181)
            )
        );
        let error = decode::<CallError>(&payload)?;
        assert_eq!(error.code, expected);
        assert!(!error.message.contains("opaque-missing-after-close"));
    }
    Ok(())
}

#[test]
fn global_host_cut_still_stops_auth_before_presentation_drive() -> FixtureResult<()> {
    let mut fixture = Fixture::new("{}", AuthConfiguration::new())?;
    let owner = Arc::clone(
        fixture
            .router
            .handle
            .auth
            .as_ref()
            .ok_or("auth owner absent")?,
    );
    // Same global owner marker used by accepted host cleanup, not a role flag.
    fixture.router.handle.shutdown.stop_reader();
    let presentation_stop = fixture.router.handle.prepare_auth_presentation_drive(false);
    let observed = (
        owner.accepting.load(Ordering::Acquire),
        owner.tail.load(Ordering::Acquire),
    );
    let cleanup = fixture.finish();
    cleanup?;
    assert_eq!(presentation_stop, keld_native::auth::AsMainStop::HostTail);
    assert_eq!(observed, (false, true));
    Ok(())
}

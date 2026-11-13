#[test]
#[cfg(target_os = "macos")]
fn terminal_tail_failure_wakes_the_ui_fatal_path() {
    use std::os::unix::net::UnixStream;
    use std::sync::mpsc;
    use std::time::Duration;

    let (server, _client) = UnixStream::pair().expect("tail-failure session pair");
    let (window_tx, window_rx) = mpsc::channel();
    let (guardian_tx, guardian_rx) = mpsc::channel();
    let guardian_thread = std::thread::spawn(move || {
        let prepare_reply = match guardian_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("guardian tail-failure attribution")
        {
            GuardianOwnerCommand::PrepareAcceptedShutdown(reply) => reply,
            GuardianOwnerCommand::Shutdown(_) => panic!("tail skipped attribution"),
            GuardianOwnerCommand::AttachRouter(_, _) => panic!("unexpected router attach"),
            GuardianOwnerCommand::FailGeneration(_, _)
            | GuardianOwnerCommand::FailRetiredGeneration(_, _) => {
                panic!("unexpected link failure")
            }
            GuardianOwnerCommand::ArmRecovery(_, _) => panic!("unexpected recovery arm"),
            GuardianOwnerCommand::DenyRecovery => panic!("unexpected recovery denial"),
        };
        prepare_reply
            .send(Err(String::from("forced attribution failure")))
            .expect("guardian tail-failure reply");
    });
    let shutdown = SessionShutdownState::new();
    let router = PrimaryRouter::start(
        server,
        window_tx,
        GuardianOwnerHandle {
            command_tx: guardian_tx,
        },
        shutdown.clone(),
    )
    .expect("primary router");

    assert!(shutdown.claim_cli_lease_lost());
    assert_eq!(
        window_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("terminal tail Fatal wake"),
        AppWindowCommand::Fatal
    );
    let error = router
        .shutdown()
        .expect_err("terminal attribution failure must remain visible");
    assert!(error.to_string().contains("forced attribution failure"));
    guardian_thread.join().expect("guardian thread joins");
}

#[test]
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
fn generation_reader_stop_preserves_successor_and_terminal_scope() {
    let shutdown = SessionShutdownState::new();
    let retired = shutdown.register_generation_reader();
    let successor = shutdown.register_generation_reader();
    retired.store(true, Ordering::Release);
    assert!(retired.load(Ordering::Acquire));
    assert!(!successor.load(Ordering::Acquire));
    assert!(!shutdown.reader_stop.load(Ordering::Acquire));

    shutdown.stop_reader();
    assert!(successor.load(Ordering::Acquire));
    assert!(
        shutdown
            .register_generation_reader()
            .load(Ordering::Acquire),
        "a reader registered after terminal shutdown must start stopped"
    );
}

#[test]
#[cfg(target_os = "macos")]
fn session_shutdown_accepts_exactly_one_first_cause() {
    let shutdown = SessionShutdownState::new();
    assert!(shutdown.claim_cli_lease_lost());
    assert!(!shutdown.claim_lifecycle_quit());
    assert_eq!(shutdown.cause(), SESSION_CLI_LEASE_LOST);

    let lifecycle = SessionShutdownState::new();
    assert!(lifecycle.claim_lifecycle_quit());
    assert!(!lifecycle.claim_cli_lease_lost());
    assert_eq!(lifecycle.cause(), SESSION_LIFECYCLE_QUIT);

    let gated = SessionShutdownState::new();
    let transition = gated.transition_guard();
    let claimant = gated.clone();
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let claim = std::thread::spawn(move || {
        started_tx.send(()).expect("claim started");
        claimant.claim_cli_lease_lost()
    });
    started_rx.recv().expect("claim thread started");
    assert_eq!(
        gated.cause(),
        SESSION_RUNNING,
        "a terminal cause changed while a write transition was active"
    );
    drop(transition);
    assert!(claim.join().expect("claim thread joins"));
    assert!(!gated.claim_lifecycle_quit());
    assert_eq!(gated.cause(), SESSION_CLI_LEASE_LOST);
}

#[test]
#[cfg(target_os = "macos")]
fn terminal_cleanup_reports_independent_router_and_guardian_failures() {
    let error = collapse_app_results([
        Err(app_detail("router tail", "link close failed")),
        Err(app_detail("guardian tail", "group reap failed")),
    ])
    .expect_err("two terminal failures must be aggregated");
    let rendered = error.to_string();
    assert!(rendered.contains("link close failed"), "{rendered}");
    assert!(rendered.contains("group reap failed"), "{rendered}");
}

#[test]
#[cfg(target_os = "macos")]
fn link_shutdown_accepts_already_closed_but_propagates_other_io_errors() {
    finish_link_shutdown(
        Err(std::io::Error::from(std::io::ErrorKind::NotConnected)),
        "test link close",
    )
    .expect("already-closed link is idempotent success");
    let error = finish_link_shutdown(
        Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied)),
        "test link close",
    )
    .expect_err("unrelated link error must remain fatal");
    assert!(error.to_string().contains("permission denied"), "{error}");
}

/// One echo round trip on a router link. `BootstrapStream` is the Unix
/// socket on macOS and Linux and the KEL-101 named pipe on Windows, so the
/// cross-OS guarded router cases (#528 T5) share this helper.
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
fn assert_echo_call(client: &mut BootstrapStream, correlation: u32, message: &str) {
    use keld_ipc::codec::{decode, encode};
    use keld_ipc::echo::{EchoRequest, EchoResponse};
    use keld_ipc::link::read_frame;

    let request = EchoRequest {
        message: message.to_owned(),
        count: correlation,
    };
    write_frame(
        client,
        FrameKind::Call,
        0,
        ECHO_CHANNEL,
        CorrelationId(correlation),
        &encode(&request).expect("echo request"),
    )
    .expect("write echo Call");
    let (header, payload) = read_frame(client).expect("echo Reply");
    assert_eq!(header.kind, FrameKind::Reply);
    assert_eq!(header.channel, ECHO_CHANNEL);
    assert_eq!(header.corr, CorrelationId(correlation));
    assert_eq!(
        decode::<EchoResponse>(&payload).expect("echo response"),
        EchoResponse {
            message: message.to_owned(),
            count: correlation,
        }
    );
}

#[test]
#[cfg(windows)]
fn windows_shutdown_disconnect_is_idempotent_but_other_requests_fail() {
    let (shutdown_tx, shutdown_rx) = mpsc::sync_channel::<Result<(), String>>(1);
    drop(shutdown_tx);
    assert!(receive_direct_owner_reply(&shutdown_rx, "Windows primary shutdown", true).is_ok());

    let (request_tx, request_rx) = mpsc::sync_channel::<Result<(), String>>(1);
    drop(request_tx);
    assert!(
        receive_direct_owner_reply(&request_rx, "Windows primary recovery arm", false).is_err()
    );
}

#[test]
#[cfg(windows)]
fn windows_lease_tail_failure_wakes_the_window_fatally() {
    let (window_tx, window_rx) = mpsc::channel();
    let (error_tx, error_rx) = mpsc::channel();
    publish_windows_lease_result(
        Err(app_detail("Windows lease test", "forced tail failure")),
        &error_tx,
        &window_tx,
    );
    assert_eq!(
        window_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("lease failure Fatal wake"),
        AppWindowCommand::Fatal
    );
    assert!(
        error_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("lease failure diagnostic")
            .to_string()
            .contains("forced tail failure")
    );

    publish_windows_lease_result(Ok(()), &error_tx, &window_tx);
    assert!(matches!(
        window_rx.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));
    assert!(matches!(
        error_rx.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));
}

#[test]
#[cfg(windows)]
#[allow(unsafe_code)] // test owns the live tempfile handle for every Win32 flag query/mutation
fn windows_lease_handle_inheritance_is_cleared_before_bun_spawn() {
    use std::os::windows::io::AsRawHandle as _;

    let file = tempfile::tempfile().expect("temporary handle");
    let handle = file.as_raw_handle().cast();
    // SAFETY: `handle` is borrowed from `file`, which remains live through
    // every call. The test changes and reads only HANDLE_FLAG_INHERIT.
    assert_ne!(
        unsafe { SetHandleInformation(handle, HANDLE_FLAG_INHERIT, HANDLE_FLAG_INHERIT) },
        0,
        "mark test handle inheritable: {}",
        io::Error::last_os_error()
    );
    let mut flags = 0;
    // SAFETY: `flags` is a live writable u32 and `handle` is live.
    assert_ne!(unsafe { GetHandleInformation(handle, &raw mut flags) }, 0);
    assert_ne!(flags & HANDLE_FLAG_INHERIT, 0);

    clear_windows_handle_inheritance(handle).expect("clear handle inheritance");
    // SAFETY: same live handle and output storage as above.
    assert_ne!(unsafe { GetHandleInformation(handle, &raw mut flags) }, 0);
    assert_eq!(flags & HANDLE_FLAG_INHERIT, 0);
}

#[test]
#[cfg(windows)]
#[allow(unsafe_code)] // test owns and closes both anonymous-pipe handles
fn windows_preflight_rejects_a_lease_pipe_with_no_writer() {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
    use windows_sys::Win32::System::Pipes::CreatePipe;

    let mut reader = std::ptr::null_mut();
    let mut writer = std::ptr::null_mut();
    let attributes = SECURITY_ATTRIBUTES {
        nLength: u32::try_from(std::mem::size_of::<SECURITY_ATTRIBUTES>())
            .expect("SECURITY_ATTRIBUTES size"),
        lpSecurityDescriptor: std::ptr::null_mut(),
        bInheritHandle: 0,
    };
    // SAFETY: both output pointers and the attributes value remain live;
    // successful handles are closed below exactly once.
    assert_ne!(
        unsafe { CreatePipe(&raw mut reader, &raw mut writer, &raw const attributes, 0) },
        0
    );
    // SAFETY: `writer` is the unique live handle returned by CreatePipe.
    assert_ne!(unsafe { CloseHandle(writer) }, 0);
    assert!(!windows_lease_pipe_is_live(reader.addr()).expect("peek closed lease"));
    // SAFETY: `reader` is the remaining unique live pipe handle.
    assert_ne!(unsafe { CloseHandle(reader) }, 0);
}

#[test]
#[cfg(windows)]
fn windows_shutdown_claim_blocks_late_ui_startup() {
    let shutdown = SessionShutdownState::new();
    assert!(shutdown.claim_cli_lease_lost());
    let created = AtomicBool::new(false);
    let result = run_direct_startup_if_session_running(&shutdown, || {
        created.store(true, Ordering::Release);
    });
    assert!(result.is_none());
    assert!(!created.load(Ordering::Acquire));
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn assert_lifecycle_event(client: &mut std::os::unix::net::UnixStream, expected: LifecycleEvent) {
    let (header, payload) = keld_ipc::link::read_frame(client).expect("lifecycle Event");
    assert_eq!(header.kind, FrameKind::Event);
    assert_eq!(header.channel, LIFECYCLE_CHANNEL);
    assert_eq!(
        keld_ipc::codec::decode::<LifecycleEvent>(&payload).expect("lifecycle payload"),
        expected
    );
}

const DIGEST: &str = "sha256:ca3d163bab055381827226140568f3bef7eaac187cebd76878e0b63e9e442356";

#[cfg(any(target_os = "macos", target_os = "linux"))]
struct ReapedRouterProbe(std::process::Child);

#[cfg(any(target_os = "macos", target_os = "linux"))]
impl Drop for ReapedRouterProbe {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
const RECOVERED_CHILD_ENV: &str = "KELD_CORE_TEST_RECOVERED_EVENT_ORDER_CHILD";
#[cfg(any(target_os = "macos", target_os = "linux"))]
const RECOVERED_CHILD_OBSERVED: &str = "KELD_CORE_RECOVERED_EVENT_ORDER_OBSERVED";

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn assert_bounded_recovered_event_order_child() {
    use std::io::{Read as _, Seek as _, SeekFrom};
    use std::process::{Command, Stdio};

    const SELECTOR: &str =
        "app_session::tests::recovered_generation_orders_window_events_once_across_installation";
    let mut stdout = tempfile::tempfile().expect("private child stdout");
    let mut stderr = tempfile::tempfile().expect("private child stderr");
    let mut child = ReapedRouterProbe(
        Command::new(std::env::current_exe().expect("test binary"))
            .args(["--exact", SELECTOR, "--nocapture"])
            .env(RECOVERED_CHILD_ENV, "1")
            .stdin(Stdio::null())
            .stdout(Stdio::from(stdout.try_clone().expect("child stdout clone")))
            .stderr(Stdio::from(stderr.try_clone().expect("child stderr clone")))
            .spawn()
            .expect("spawn isolated event-order test"),
    );
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.0.try_wait().expect("inspect event-order child") {
            break status;
        }
        if started.elapsed() >= Duration::from_secs(10) {
            child.0.kill().expect("kill blocked event-order child");
            let status = child.0.wait().expect("reap blocked event-order child");
            stdout
                .seek(SeekFrom::Start(0))
                .expect("rewind child stdout");
            stderr
                .seek(SeekFrom::Start(0))
                .expect("rewind child stderr");
            let mut output = String::new();
            let mut detail = String::new();
            stdout
                .read_to_string(&mut output)
                .expect("read child stdout");
            stderr
                .read_to_string(&mut detail)
                .expect("read child stderr");
            panic!(
                "event-order child exceeded 10-second cleanup bound: status={status}, stdout={output}, stderr={detail}"
            );
        }
        std::thread::park_timeout(Duration::from_millis(10));
    };
    stdout
        .seek(SeekFrom::Start(0))
        .expect("rewind child stdout");
    stderr
        .seek(SeekFrom::Start(0))
        .expect("rewind child stderr");
    let mut output = String::new();
    let mut detail = String::new();
    stdout
        .read_to_string(&mut output)
        .expect("read child stdout");
    stderr
        .read_to_string(&mut detail)
        .expect("read child stderr");
    assert!(
        status.success()
            && output.contains(RECOVERED_CHILD_OBSERVED)
            && output.contains("test result: ok. 1 passed; 0 failed;"),
        "isolated event-order proof failed: status={status}, stdout={output}, stderr={detail}"
    );
}

#[test]
#[cfg(target_os = "macos")]
fn stale_and_foreign_link_contexts_are_inert_before_echo_decode() {
    use std::os::unix::net::UnixStream;
    let (writer, _peer) = UnixStream::pair().expect("actual app link pair");
    let current_stop = Arc::new(AtomicBool::new(false));
    let current = Mutex::new(Some(ActivePrimaryGeneration {
        identity: PrimaryLinkIdentity::Fixture,
        attempt: 2,
        writer,
        reader_stop: Arc::clone(&current_stop),
        #[cfg(target_os = "macos")]
        dialog_nonce_high_water: 0,
        pending_quit: None,
    }));
    let shutdown = SessionShutdownState::new();
    for caller in [
        PrimaryCallerContext {
            attempt: 1,
            identity: PrimaryLinkIdentity::Fixture,
            reader_stop: Arc::clone(&current_stop),
        },
        PrimaryCallerContext {
            attempt: 2,
            identity: PrimaryLinkIdentity::Fixture,
            reader_stop: Arc::new(AtomicBool::new(false)),
        },
    ] {
        // Literal malformed Echo is a direct decoder-entry falsifier: deleting
        // either corresponding context guard must surface IPC003 instead of Ok.
        dispatch_primary_echo(&current, &shutdown, &caller, CorrelationId(71), &[0xff])
            .expect("retired or foreign context never reaches unprivileged decoder");
    }
}

#[test]
#[cfg(target_os = "macos")]
fn current_g2_context_dispatches_original_echo_and_stopped_context_is_inert() {
    use keld_ipc::link::{AppLinkDeadlines as _, read_frame};
    use std::os::unix::net::UnixStream;
    let (writer, mut peer) = UnixStream::pair().expect("actual g2 app link");
    peer.set_app_link_deadlines(Some(Duration::from_secs(5)))
        .expect("existing observer bound");
    let stop = Arc::new(AtomicBool::new(false));
    let current = Mutex::new(Some(ActivePrimaryGeneration {
        identity: PrimaryLinkIdentity::Fixture,
        attempt: 2,
        writer,
        reader_stop: Arc::clone(&stop),
        #[cfg(target_os = "macos")]
        dialog_nonce_high_water: 0,
        pending_quit: None,
    }));
    let caller = PrimaryCallerContext {
        attempt: 2,
        identity: PrimaryLinkIdentity::Fixture,
        reader_stop: Arc::clone(&stop),
    };
    let shutdown = SessionShutdownState::new();
    let payload = encode(&keld_ipc::echo::EchoRequest {
        message: "current-g2".to_owned(),
        count: 72,
    })
    .expect("owning codec");
    dispatch_primary_echo(&current, &shutdown, &caller, CorrelationId(72), &payload)
        .expect("current original Echo");
    let (header, bytes) = read_frame(&mut peer).expect("actual g2 terminal wire");
    assert_eq!(
        (header.kind, header.channel, header.corr),
        (FrameKind::Reply, ECHO_CHANNEL, CorrelationId(72))
    );
    let reply: keld_ipc::echo::EchoResponse = decode(&bytes).expect("owning reply codec");
    assert_eq!((reply.message.as_str(), reply.count), ("current-g2", 72));
    stop.store(true, Ordering::Release);
    dispatch_primary_echo(&current, &shutdown, &caller, CorrelationId(73), &[0xff])
        .expect("stopped context remains inert before decoder");
}

#[test]
#[cfg(target_os = "macos")]
fn macos_publisher_and_signed_identifier_both_separate_profiles() {
    let first = MacValidatedAppIdentity::from_verified_parts("ABCDEFGHIJ", "com.example.app")
        .expect("verified signing facts");
    let publisher_changed =
        MacValidatedAppIdentity::from_verified_parts("KLMNOPQRST", "com.example.app")
            .expect("verified publisher change");
    let app_changed =
        MacValidatedAppIdentity::from_verified_parts("ABCDEFGHIJ", "com.example.other")
            .expect("verified signing identifier change");
    assert_ne!(first.profile_identity, publisher_changed.profile_identity);
    assert_ne!(first.profile_identity, app_changed.profile_identity);
    assert!(MacValidatedAppIdentity::from_verified_parts("ABCDEFGHIJ", "Com.Example.App").is_err());
    assert!(MacValidatedAppIdentity::from_verified_parts("", "com.example.app").is_err());
}

#[test]
#[cfg(target_os = "macos")]
fn macos_dev_selection_never_reads_release_identity_and_changes_each_launch() {
    let first = select_macos_profile_mode_with(true, || {
        panic!("explicit dev lease must bypass release signing identity")
    })
    .expect("first ephemeral selection");
    let second = select_macos_profile_mode_with(true, || {
        panic!("explicit dev lease must bypass release signing identity")
    })
    .expect("second ephemeral selection");
    let (WebProfileSelection::EphemeralDev(first), WebProfileSelection::EphemeralDev(second)) =
        (first, second)
    else {
        panic!("dev lease selected a persistent profile")
    };
    assert_ne!(first.launch_nonce(), second.launch_nonce());
}

#[test]
#[cfg(target_os = "macos")]
fn macos_release_selection_rejects_signature_failure() {
    let error = select_macos_profile_mode_with(false, || {
        Err(macos_identity_error(
            "synthetic signature validation failure",
        ))
    })
    .expect_err("release mode must fail closed");
    assert_eq!(error.code(), "KELD-WV-009");
    assert!(
        error
            .to_string()
            .contains("synthetic signature validation failure")
    );
}

#[cfg(windows)]
struct ReapedTestChild(std::process::Child);

#[cfg(windows)]
impl Drop for ReapedTestChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn valid_boot(entry: &str, renderer: &str) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "schema": 1,
        "name": "Fixture",
        "entry": entry,
        "renderer": renderer,
        "permissions": {
            "file": "keld.permissions.jsonc",
            "content_sha256": DIGEST,
        }
    }))
    .expect("serialize fixture boot")
}

fn must_err<T>(result: Result<T, HostAppError>, message: &str) -> HostAppError {
    match result {
        Ok(_) => panic!("{message}"),
        Err(error) => error,
    }
}

#[test]
#[cfg(windows)]
fn synthetic_windows_verified_parts_keep_publisher_and_app_separate() {
    let first = ValidatedAppIdentity::from_verified_parts([1; 32], "com.example.app")
        .expect("synthetic verified identity");
    let publisher_changed = ValidatedAppIdentity::from_verified_parts([2; 32], "com.example.app")
        .expect("synthetic publisher change");
    let app_changed = ValidatedAppIdentity::from_verified_parts([1; 32], "com.example.other")
        .expect("synthetic app change");
    assert_ne!(first.profile_identity, publisher_changed.profile_identity);
    assert_ne!(first.profile_identity, app_changed.profile_identity);
    for noncanonical in ["Com.Example.App", "com.example.app\n"] {
        let error = ValidatedAppIdentity::from_verified_parts([1; 32], noncanonical)
            .expect_err("a noncanonical verified app id has no profile identity");
        assert_eq!(error.code(), "KELD-WV-009");
    }
}

//! Proposed contract tests. Real SDK cases run in owning bounded children.
use super::*;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

const CASE_ENV: &str = "KELD_AUTH_HTTP_TEST_CASE";
const SENTINEL: &[u8] = b"harmless=form-body";

struct ChildOwner(std::process::Child);
impl Drop for ChildOwner {
    fn drop(&mut self) {
        if self.0.try_wait().expect("child query").is_none() {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}
fn capture(mut stream: impl Read) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut chunk = [0u8; 1024];
    loop {
        let count = stream.read(&mut chunk).expect("bounded capture");
        if count == 0 {
            break;
        }
        assert!(
            bytes.len() + count <= 32768,
            "fixture output exceeded bound"
        );
        bytes.extend_from_slice(&chunk[..count]);
    }
    bytes
}
fn isolated(name: &str, body: fn()) {
    let exact = format!("auth::macos_http::tests::{name}");
    isolated_selector(name, &exact, body);
}

// One existing bounded child owner also serves the broker's real SDK integration.
pub(in crate::auth) fn isolated_selector(name: &str, exact: &str, body: fn()) {
    if std::env::var(CASE_ENV).ok().as_deref() == Some(name) {
        println!("KELD_AUTH_HTTP_CASE_BEGIN {name}");
        body();
        println!("KELD_AUTH_HTTP_CASE_PASS {name}");
        return;
    }
    let mut owner = ChildOwner(
        Command::new(std::env::current_exe().expect("test executable"))
            .args(["--exact", exact, "--nocapture"])
            .env(CASE_ENV, name)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("owned test child"),
    );
    let stdout = owner.0.stdout.take().expect("stdout");
    let stderr = owner.0.stderr.take().expect("stderr");
    let out = thread::spawn(move || capture(stdout));
    let err = thread::spawn(move || capture(stderr));
    let deadline = Instant::now() + Duration::from_secs(18);
    let status = loop {
        if let Some(status) = owner.0.try_wait().expect("child query") {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = owner.0.kill();
            let status = owner.0.wait().expect("reap timeout");
            panic!("SDK child kill switch: {status}");
        }
        thread::yield_now();
    };
    let output = out.join().expect("stdout owner");
    let errors = err.join().expect("stderr owner");
    assert!(
        String::from_utf8_lossy(&output).contains(&format!("KELD_AUTH_HTTP_CASE_BEGIN {name}")),
        "child did not enter the exact SDK case: {}\n{}",
        String::from_utf8_lossy(&output),
        String::from_utf8_lossy(&errors)
    );
    assert!(
        status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output),
        String::from_utf8_lossy(&errors)
    );
    assert!(
        String::from_utf8_lossy(&output).contains(&format!("KELD_AUTH_HTTP_CASE_PASS {name}")),
        "successful child omitted the exact SDK completion marker"
    );
}
fn bounds() -> HttpDeadlines {
    let now = Instant::now();
    HttpDeadlines {
        request: now + Duration::from_secs(3),
        cleanup: now + Duration::from_secs(7),
    }
}
fn finish(operation: &mut HttpOperation) -> Result<HttpResponse, HttpError> {
    let outer = Instant::now() + Duration::from_secs(8);
    loop {
        match operation.poll() {
            HttpProgress::Complete(result) => return result,
            HttpProgress::CleanupUnproven(error) => panic!("SDK cleanup unproven: {error}"),
            HttpProgress::Pending => {}
        }
        assert!(Instant::now() < outer, "poll kill switch");
        thread::yield_now();
    }
}
pub(in crate::auth) fn accept(listener: &TcpListener) -> TcpStream {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                return prepare_stream(stream);
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(error) => panic!("accept: {error}"),
        }
        assert!(Instant::now() < deadline, "accept kill switch");
        thread::yield_now();
    }
}
// Reuse the same accepted-socket normalization and bounded I/O at every capture.
pub(in crate::auth) fn prepare_stream(stream: TcpStream) -> TcpStream {
    stream.set_nonblocking(false).expect("blocking stream");
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("read deadline");
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .expect("write deadline");
    stream
}

fn listener() -> TcpListener {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("isolated listener");
    listener
        .set_nonblocking(true)
        .expect("nonblocking listener");
    listener
}
pub(in crate::auth) fn request_body(stream: &mut TcpStream) -> Vec<u8> {
    let mut bytes = Vec::new();
    let split = loop {
        if let Some(index) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
            break index + 4;
        }
        assert!(bytes.len() < 8192, "header bound");
        let mut chunk = [0u8; 512];
        let count = stream.read(&mut chunk).expect("request header");
        assert!(count > 0, "request EOF");
        bytes.extend_from_slice(&chunk[..count]);
    };
    let headers = std::str::from_utf8(&bytes[..split]).expect("header UTF8");
    assert!(headers.starts_with("POST "), "actual POST absent");
    let length: usize = headers
        .lines()
        .find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.eq_ignore_ascii_case("Content-Length")
                .then_some(value.trim())
        })
        .expect("body length")
        .parse()
        .expect("length number");
    assert!(length <= 1024, "fixture body bound");
    let mut body = bytes[split..].to_vec();
    let already = body.len();
    assert!(already <= length);
    body.resize(length, 0);
    stream
        .read_exact(&mut body[already..])
        .expect("complete request body");
    body
}

#[test]
fn http_response_and_error_debug_are_redacted() {
    let response = HttpResponse {
        status: 200,
        body: b"refresh_token=PRIVATE_FIXTURE_SENTINEL".to_vec(),
        received_at: SystemTime::now(),
        complete_at: Instant::now(),
    };
    assert!(!format!("{response:?}").contains("PRIVATE_FIXTURE_SENTINEL"));
    let error = HttpError::new(HttpFailure::Network(-999));
    assert!(error.to_string().starts_with("KELD-AUTH-013:"));
    assert!(error.to_string().contains("no automatic retry"));
}

#[test]
fn invalid_endpoint_and_expired_request_fail_before_task() {
    autoreleasepool(|_| {
        for endpoint in [
            "http://example.test/token",
            "https://user:password@example.test/token",
            "https://example.test/token#fragment",
            "relative/token",
        ] {
            assert!(matches!(
                checked_https_url(endpoint),
                Err(HttpError {
                    primary: HttpFailure::Endpoint,
                    ..
                })
            ));
        }
    });
    let now = Instant::now();
    let result = HttpOperation::start(
        "entra-common",
        Vec::new(),
        HttpDeadlines {
            request: now,
            cleanup: now,
        },
        Arc::new(AtomicBool::new(false)),
    );
    assert!(matches!(
        result,
        Err(HttpError {
            primary: HttpFailure::Deadline,
            ..
        })
    ));
    let result = HttpOperation::start(
        "entra-common",
        vec![0; BODY_LIMIT + 1],
        bounds(),
        Arc::new(AtomicBool::new(false)),
    );
    assert!(matches!(
        result,
        Err(HttpError {
            primary: HttpFailure::RequestTooLarge,
            ..
        })
    ));
}

fn healthy_response() {
    let listener = listener();
    let address = listener.local_addr().expect("address");
    let server = thread::spawn(move || {
        let mut stream = accept(&listener);
        assert_eq!(request_body(&mut stream), SENTINEL);
        stream
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Length: 12\r\nConnection: close\r\n\r\nexact-result",
            )
            .expect("actual response");
    });
    let mut operation = HttpOperation::start_test_url(
        &format!("http://{address}/token"),
        SENTINEL,
        bounds(),
        Arc::new(AtomicBool::new(false)),
    )
    .expect("actual transport");
    let response = finish(&mut operation).expect("transport response");
    server.join().expect("server owner");
    assert_eq!(response.status(), 200);
    assert_eq!(response.body(), b"exact-result");
    assert!(operation.shared.delegate_dropped.load(Ordering::Acquire));
    assert_eq!(Arc::strong_count(&operation.shared), 1);
    let weak = Arc::downgrade(&operation.shared);
    drop(operation);
    assert!(weak.upgrade().is_none());
}

#[test]
fn exact_response_requires_real_delegate_quiescence() {
    isolated(
        "exact_response_requires_real_delegate_quiescence",
        healthy_response,
    );
}

#[test]
fn redirects_never_forward_the_token_post_body() {
    isolated("redirects_never_forward_the_token_post_body", || {
        for status in [307, 308] {
            let first = listener();
            let sink = listener();
            let address = first.local_addr().expect("first address");
            let target = sink.local_addr().expect("sink address");
            let server = thread::spawn(move || {
                let mut stream = accept(&first);
                assert_eq!(request_body(&mut stream), SENTINEL);
                let redirect = format!(
                    "HTTP/1.1 {status} Redirect\r\nLocation: http://{target}/sink\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                );
                stream
                    .write_all(redirect.as_bytes())
                    .expect("actual redirect");
            });
            let mut operation = HttpOperation::start_test_url(
                &format!("http://{address}/token"),
                SENTINEL,
                bounds(),
                Arc::new(AtomicBool::new(false)),
            )
            .expect("actual transport");
            let error = finish(&mut operation).expect_err("redirect must fail");
            server.join().expect("server owner");
            assert_eq!(error.primary, HttpFailure::Redirect);
            assert!(
                matches!(sink.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock),
                "sink received a request after actual terminal/invalidation/quiescence"
            );
        }
    });
}

#[test]
fn cancellation_keeps_actual_terminal_and_drop_receipts() {
    isolated(
        "cancellation_keeps_actual_terminal_and_drop_receipts",
        || {
            let listener = listener();
            let address = listener.local_addr().expect("address");
            let (seen_tx, seen) = mpsc::sync_channel(1);
            let (release, release_rx) = mpsc::sync_channel(1);
            let server = thread::spawn(move || {
                let mut stream = accept(&listener);
                assert_eq!(request_body(&mut stream), SENTINEL);
                seen_tx.send(()).expect("request receipt");
                release_rx
                    .recv_timeout(Duration::from_secs(5))
                    .expect("late release");
                let _ = stream.write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\nConnection: close\r\n\r\nlate",
                );
            });
            let mut operation = HttpOperation::start_test_url(
                &format!("http://{address}/token"),
                SENTINEL,
                bounds(),
                Arc::new(AtomicBool::new(false)),
            )
            .expect("actual transport");
            seen.recv_timeout(Duration::from_secs(5))
                .expect("actual held request");
            operation.cancel();
            release.send(()).expect("release only after cancellation");
            let error = finish(&mut operation).expect_err("cancelled exchange");
            server.join().expect("server owner");
            assert_eq!(error.primary, HttpFailure::Cancelled);
            assert_eq!(error.platform_code, Some(-999));
            assert!(operation.shared.delegate_dropped.load(Ordering::Acquire));
            assert_eq!(operation.shared.lock().body.len(), 0);
        },
    );
}

// Fixture construction only; all metadata/decision/body oracles stay in the case.
fn advertised_metadata_fixture(
    advertised: i64,
) -> (
    Retained<NSHTTPURLResponse>,
    Arc<Shared>,
    Owners,
    Retained<AuthHttpDelegate>,
) {
    let url = NSURL::URLWithString(&NSString::from_str("http://127.0.0.1/component-fixture"))
        .expect("component URL");
    let key = NSString::from_str("Content-Length");
    let value = NSString::from_str(&advertised.to_string());
    let headers = objc2_foundation::NSDictionary::from_slices(&[&*key], &[&*value]);
    let response = NSHTTPURLResponse::initWithURL_statusCode_HTTPVersion_headerFields(
        NSHTTPURLResponse::alloc(),
        &url,
        200,
        Some(&NSString::from_str("HTTP/1.1")),
        Some(&headers),
    )
    .expect("actual Foundation HTTP response");
    let shared = Arc::new(Shared {
        body_copy_calls: std::sync::atomic::AtomicUsize::new(0),
        state: Mutex::new(ResponseState::default()),
        deadlines: bounds(),
        cancelled: Arc::new(AtomicBool::new(false)),
        delegate_dropped: AtomicBool::new(false),
        invalidating: AtomicBool::new(false),
    });
    let delegate = AuthHttpDelegate::new(Arc::clone(&shared)).expect("existing delegate factory");
    let config = NSURLSessionConfiguration::ephemeralSessionConfiguration();
    let session = NSURLSession::sessionWithConfiguration(&config);
    // Never resume; only supply genuine ignored SDK arguments. No SDK
    // delegate is registered; existing Owners Drop is cleanup fallback.
    let task = session.dataTaskWithURL(&url);
    let owners = Owners {
        session,
        task,
        _delegate: Retained::clone(&delegate),
    };
    (response, shared, owners, delegate)
}

#[test]
fn actual_foundation_advertised_metadata_uses_existing_delegate_cap_without_copy() {
    // Component boundary: real Foundation objects and direct existing method.
    // This is not network/selector delivery or SDK quiescence evidence.
    isolated(
        "actual_foundation_advertised_metadata_uses_existing_delegate_cap_without_copy",
        || {
            autoreleasepool(|_| {
                const FIXTURE_LIMIT: i64 = 16 * 1024 * 1024;
                assert_eq!(i64::try_from(BODY_LIMIT).expect("wire cap"), FIXTURE_LIMIT);
                for (advertised, expected, expected_failure) in [
                    (0, NSURLSessionResponseDisposition::Allow, None),
                    (FIXTURE_LIMIT, NSURLSessionResponseDisposition::Allow, None),
                    (
                        FIXTURE_LIMIT + 1,
                        NSURLSessionResponseDisposition::Cancel,
                        Some(HttpFailure::ResponseTooLarge),
                    ),
                ] {
                    let (response, shared, owners, component_delegate) =
                        advertised_metadata_fixture(advertised);
                    assert_eq!(response.statusCode(), 200, "actual object prerequisite");
                    assert_eq!(
                        response.expectedContentLength(),
                        advertised,
                        "actual object prerequisite"
                    );

                    let calls = std::rc::Rc::new(std::cell::Cell::new(0usize));
                    let disposition = std::rc::Rc::new(std::cell::Cell::new(None));
                    let calls_for_completion = std::rc::Rc::clone(&calls);
                    let disposition_for_completion = std::rc::Rc::clone(&disposition);
                    let completion =
                        block2::RcBlock::new(move |value: NSURLSessionResponseDisposition| {
                            calls_for_completion.set(calls_for_completion.get() + 1);
                            disposition_for_completion.set(Some(value));
                        });
                    component_delegate.response(
                        objc2::sel!(URLSession:dataTask:didReceiveResponse:completionHandler:),
                        &owners.session,
                        &owners.task,
                        &response,
                        &completion,
                    );
                    assert_eq!(calls.get(), 1, "completion must be called exactly once");
                    assert_eq!(
                        disposition.get(),
                        Some(expected),
                        "literal disposition oracle"
                    );
                    {
                        let state = shared.lock();
                        assert_eq!(state.status, Some(200));
                        assert_eq!(
                            state.primary, expected_failure,
                            "literal typed-failure oracle"
                        );
                        assert_eq!(state.cleanup, None);
                        assert_eq!(state.body.len(), 0);
                        assert_eq!(state.body.capacity(), 0);
                        // These facts remain absent: this case never invents lifecycle receipts.
                        assert!(state.complete_at.is_none());
                        assert!(!state.invalidated);
                    }
                    assert_eq!(shared.body_copy_calls.load(Ordering::Relaxed), 0);
                    if advertised != 0 {
                        let payload = b"component-owned-small-data";
                        let data = NSData::with_bytes(payload);
                        component_delegate.data(
                            objc2::sel!(URLSession:dataTask:didReceiveData:),
                            &owners.session,
                            &owners.task,
                            &data,
                        );
                        let state = shared.lock();
                        if expected_failure.is_some() {
                            assert_eq!(state.primary, Some(HttpFailure::ResponseTooLarge));
                            assert!(state.body.is_empty(), "denied bytes must not be exposed");
                            assert_eq!(
                                state.body.capacity(),
                                0,
                                "denied bytes must not allocate storage"
                            );
                            assert_eq!(
                                shared.body_copy_calls.load(Ordering::Relaxed),
                                0,
                                "denied data must not reach NSData.to_vec"
                            );
                        } else {
                            assert_eq!(state.primary, None, "allowed neighbor must remain healthy");
                            assert_eq!(
                                state.body.as_slice(),
                                payload,
                                "allowed neighbor exercises the same data owner"
                            );
                            assert_eq!(
                                shared.body_copy_calls.load(Ordering::Relaxed),
                                1,
                                "copy observer must detect the actual allowed copy boundary"
                            );
                        }
                    }
                }
            });
        },
    );
}

#[test]
fn response_state_cap_uses_existing_wire_ceiling_and_preserves_cleanup_faults() {
    let mut state = ResponseState::default();
    assert!(state.append_body(&[]));
    assert!(state.append_body(&vec![0; keld_ipc::MAX_FRAME_LEN]));
    assert_eq!(state.body.len(), keld_ipc::MAX_FRAME_LEN);
    assert!(!state.append_body(&[1]));
    assert_eq!(state.primary, Some(HttpFailure::ResponseTooLarge));
    assert_eq!(state.body.len(), 0);
    state.note_cleanup(HttpCleanupFailure::QuiescenceUnproven);
    state.protocol_fault();
    assert_eq!(state.primary, Some(HttpFailure::ResponseTooLarge));
    assert_eq!(
        state.cleanup,
        Some(HttpCleanupFailure::ProtocolAndQuiescence)
    );
}

#[test]
fn deadline_is_not_renewed_while_real_request_is_held() {
    isolated("deadline_is_not_renewed_while_real_request_is_held", || {
        let listener = listener();
        let address = listener.local_addr().expect("address");
        let (seen_tx, seen) = mpsc::sync_channel(1);
        let (release, release_rx) = mpsc::sync_channel(1);
        let server = thread::spawn(move || {
            let mut stream = accept(&listener);
            assert_eq!(request_body(&mut stream), SENTINEL);
            seen_tx.send(()).expect("held request receipt");
            release_rx
                .recv_timeout(Duration::from_secs(5))
                .expect("release held server");
        });
        let now = Instant::now();
        let deadlines = HttpDeadlines {
            request: now + Duration::from_secs(3),
            cleanup: now + Duration::from_secs(4),
        };
        let mut operation = HttpOperation::start_test_url(
            &format!("http://{address}/token"),
            SENTINEL,
            deadlines,
            Arc::new(AtomicBool::new(false)),
        )
        .expect("actual transport");
        seen.recv_timeout(Duration::from_secs(2))
            .expect("actual request reached server");
        let error = finish(&mut operation).expect_err("deadline must fail");
        release.send(()).expect("release after actual terminal");
        server.join().expect("held server owner");
        // System timeout may arrive just before the host Instant because SDK
        // timer precision differs. Keep that attributed network failure honest.
        assert!(matches!(
            error.primary,
            HttpFailure::Deadline | HttpFailure::Network(-1001)
        ));
        assert!(operation.shared.delegate_dropped.load(Ordering::Acquire));
        assert!(Instant::now() < deadlines.cleanup);
    });
}

#[test]
fn host_poll_deadline_is_absolute_without_an_sdk_clock_substitute() {
    let now = Instant::now();
    let shared = Arc::new(Shared {
        body_copy_calls: std::sync::atomic::AtomicUsize::new(0),
        state: Mutex::new(ResponseState::default()),
        deadlines: HttpDeadlines {
            request: now
                .checked_sub(Duration::from_secs(1))
                .expect("past host bound"),
            cleanup: now + Duration::from_secs(1),
        },
        cancelled: Arc::new(AtomicBool::new(false)),
        delegate_dropped: AtomicBool::new(false),
        invalidating: AtomicBool::new(false),
    });
    // Pure host-clock atom, explicitly no SDK owner/OS qualification.
    let mut operation = HttpOperation {
        shared,
        owners: None,
        consumed: false,
        cleanup_reported: false,
    };
    assert!(matches!(operation.poll(), HttpProgress::Pending));
    assert_eq!(operation.shared.lock().primary, Some(HttpFailure::Deadline));
    let deadline = operation.shared.deadlines.request;
    assert!(matches!(operation.poll(), HttpProgress::Pending));
    assert_eq!(operation.shared.deadlines.request, deadline);
}

fn past_cleanup_callback_state(cancelled: bool) -> (Instant, Arc<Shared>) {
    let now = Instant::now();
    let past = now.checked_sub(Duration::from_secs(1)).expect("past clock");
    let shared = Arc::new(Shared {
        body_copy_calls: std::sync::atomic::AtomicUsize::new(0),
        state: Mutex::new(ResponseState {
            primary: Some(HttpFailure::Redirect),
            platform_code: Some(-999),
            ..ResponseState::default()
        }),
        deadlines: HttpDeadlines {
            request: past,
            cleanup: past,
        },
        cancelled: Arc::new(AtomicBool::new(cancelled)),
        delegate_dropped: AtomicBool::new(false),
        invalidating: AtomicBool::new(false),
    });
    (now, shared)
}

// Construction only: the scenario retains its clock expressions and lock/oracles.
fn request_stop_callback_state(cancelled: bool, deadlines: HttpDeadlines) -> Arc<Shared> {
    Arc::new(Shared {
        body_copy_calls: std::sync::atomic::AtomicUsize::new(0),
        state: Mutex::new(ResponseState::default()),
        deadlines,
        cancelled: Arc::new(AtomicBool::new(cancelled)),
        delegate_dropped: AtomicBool::new(false),
        invalidating: AtomicBool::new(false),
    })
}

#[test]
fn callback_contention_cannot_bypass_cleanup_or_cancellation() {
    isolated(
        "callback_contention_cannot_bypass_cleanup_or_cancellation",
        || {
            for cancelled in [false, true] {
                let (now, shared) = past_cleanup_callback_state(cancelled);
                let holder_shared = Arc::clone(&shared);
                let (held_tx, held_rx) = mpsc::sync_channel(1);
                let (release_tx, release_rx) = mpsc::sync_channel(1);
                let holder = thread::spawn(move || {
                    let _guard = holder_shared.state.lock().expect("held callback state");
                    held_tx.send(()).expect("lock-held receipt");
                    release_rx
                        .recv_timeout(Duration::from_secs(5))
                        .expect("bounded held state");
                });
                held_rx
                    .recv_timeout(Duration::from_secs(2))
                    .expect("actual lock contention");
                let mut operation = HttpOperation {
                    shared,
                    owners: None,
                    consumed: false,
                    cleanup_reported: false,
                };
                let before = Instant::now();
                let progress = operation.poll();
                assert!(
                    before.elapsed() < Duration::from_secs(1),
                    "poll blocked on callback state"
                );
                let HttpProgress::CleanupUnproven(snapshot) = progress else {
                    panic!("past cleanup clock returned Pending under contention");
                };
                // Exact derived classification name keeps these same tests
                // compilable against the failure-first backend, which does not
                // yet declare the corrected StateContended enum variant.
                assert_eq!(format!("{:?}", snapshot.primary), "StateContended");
                assert_eq!(snapshot.platform_code, None);
                assert_eq!(
                    snapshot.cleanup,
                    Some(HttpCleanupFailure::QuiescenceUnproven)
                );
                assert!(
                    operation.shared.invalidating.load(Ordering::Acquire),
                    "SDK stop intent bypassed by contention"
                );
                assert!(!operation.consumed, "unproved teardown consumed owner");
                release_tx.send(()).expect("release actual holder");
                holder.join().expect("holder reaped");
                // Resuming access must preserve the prior exchange fault and attach
                // the owner-local incident; no contended fallback overwrites it.
                assert!(matches!(operation.poll(), HttpProgress::Pending));
                {
                    let mut state = operation.shared.lock();
                    assert_eq!(state.primary, Some(HttpFailure::Redirect));
                    assert_eq!(state.platform_code, Some(-999));
                    assert_eq!(state.cleanup, Some(HttpCleanupFailure::QuiescenceUnproven));
                    state.complete_at = Some(now);
                    state.invalidated = true;
                }
                operation
                    .shared
                    .delegate_dropped
                    .store(true, Ordering::Release);
                assert!(matches!(
                    operation.poll(),
                    HttpProgress::Complete(Err(HttpError {
                        primary: HttpFailure::Redirect,
                        cleanup: Some(HttpCleanupFailure::QuiescenceUnproven),
                        platform_code: Some(-999),
                    }))
                ));
            }
            // Each request-clock/cancellation atom requests SDK stop while
            // cleanup remains in the future and state is unavailable.
            for cancelled in [false, true] {
                let now = Instant::now();
                let shared = request_stop_callback_state(
                    cancelled,
                    HttpDeadlines {
                        request: if cancelled {
                            now + Duration::from_secs(3)
                        } else {
                            now.checked_sub(Duration::from_secs(1))
                                .expect("past request")
                        },
                        cleanup: now + Duration::from_secs(7),
                    },
                );
                let holder_shared = Arc::clone(&shared);
                let guard = holder_shared.state.lock().expect("contended state");
                let mut operation = HttpOperation {
                    shared,
                    owners: None,
                    consumed: false,
                    cleanup_reported: false,
                };
                assert!(matches!(operation.poll(), HttpProgress::Pending));
                assert!(operation.shared.invalidating.load(Ordering::Acquire));
                drop(guard);
                drop(holder_shared);
            }
        },
    );
}

#[test]
fn unadvertised_chunked_body_limit_is_enforced_by_real_data_callbacks() {
    isolated(
        "unadvertised_chunked_body_limit_is_enforced_by_real_data_callbacks",
        || {
            let listener = listener();
            let address = listener.local_addr().expect("address");
            let server = thread::spawn(move || {
                let mut stream = accept(&listener);
                assert_eq!(request_body(&mut stream), SENTINEL);
                stream.write_all(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n").expect("unknown-length response");
                let chunk = vec![b'x'; 65536];
                let mut sent = 0;
                while sent < BODY_LIMIT + 1 {
                    let count = chunk.len().min(BODY_LIMIT + 1 - sent);
                    let header = format!("{count:x}\r\n");
                    stream
                        .write_all(header.as_bytes())
                        .expect("real chunk header");
                    stream
                        .write_all(&chunk[..count])
                        .expect("real chunk payload");
                    // Count accepted payload before framing: the adapter can
                    // cancel as soon as the excessive byte is observed.
                    sent += count;
                    if let Err(error) = stream.write_all(b"\r\n") {
                        assert_eq!(
                            sent,
                            BODY_LIMIT + 1,
                            "framing failed before all payload was accepted: {error}"
                        );
                        return sent;
                    }
                }
                // Cancellation may close after the excessive byte is observed;
                // the final framing is not part of the body-length oracle.
                let _ = stream.write_all(b"0\r\n\r\n");
                sent
            });
            let mut operation = HttpOperation::start_test_url(
                &format!("http://{address}/token"),
                SENTINEL,
                bounds(),
                Arc::new(AtomicBool::new(false)),
            )
            .expect("actual chunked transport");
            let error =
                finish(&mut operation).expect_err("streaming cap must reject excessive body");
            assert_eq!(
                server.join().expect("streaming server reaped"),
                BODY_LIMIT + 1
            );
            assert_eq!(error.primary, HttpFailure::ResponseTooLarge);
            assert_eq!(error.cleanup, None);
            {
                let state = operation.shared.lock();
                assert!(
                    state.body.is_empty(),
                    "failed exchange retained exposed body"
                );
                assert!(state.complete_at.is_some(), "actual task terminal absent");
                assert!(state.invalidated, "actual invalidation callback absent");
            }
            assert!(operation.shared.delegate_dropped.load(Ordering::Acquire));
            assert_eq!(Arc::strong_count(&operation.shared), 1);
            let weak = Arc::downgrade(&operation.shared);
            drop(operation);
            assert!(weak.upgrade().is_none());
            healthy_response();
        },
    );
}

#[test]
fn original_receipt_clocks_survive_quiescence_and_delayed_delivery() {
    // Pure ownership/model atom; no SDK callback or real-clock qualification.
    // Fixed wall receipt and modeled earlier monotonic receipt deliberately
    // precede delivery. No sleep or deadline renewal can decide this oracle.
    let delivered_at = Instant::now();
    let original_mono = delivered_at
        .checked_sub(Duration::from_secs(30))
        .expect("earlier modeled receipt");
    let original_wall = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
    let shared = Arc::new(Shared {
        body_copy_calls: std::sync::atomic::AtomicUsize::new(0),
        state: Mutex::new(ResponseState {
            status: Some(200),
            body: b"modeled-result".to_vec(),
            received_at: Some(original_wall),
            complete_at: Some(original_mono),
            invalidated: true,
            ..ResponseState::default()
        }),
        deadlines: HttpDeadlines {
            request: original_mono + Duration::from_secs(1),
            cleanup: delivered_at + Duration::from_secs(5),
        },
        cancelled: Arc::new(AtomicBool::new(false)),
        delegate_dropped: AtomicBool::new(false),
        invalidating: AtomicBool::new(false),
    });
    let mut operation = HttpOperation {
        shared,
        owners: None,
        consumed: false,
        cleanup_reported: false,
    };
    // Terminal/invalidation alone cannot expose the result. Clocks remain in
    // the owning state while real-delegate-drop custody is still unproved.
    assert!(matches!(operation.poll(), HttpProgress::Pending));
    {
        let state = operation.shared.lock();
        assert_eq!(state.received_at, Some(original_wall));
        assert_eq!(state.complete_at, Some(original_mono));
    }
    // Model the last independent receipt, not a claim about SDK destruction.
    operation
        .shared
        .delegate_dropped
        .store(true, Ordering::Release);
    let HttpProgress::Complete(Ok(response)) = operation.poll() else {
        panic!("modeled quiescent response must deliver");
    };
    assert_eq!(response.status(), 200);
    assert_eq!(response.body(), b"modeled-result");
    assert_eq!(response.received_at(), original_wall);
    assert_eq!(response.receipt_mono(), original_mono);
    assert!(response.receipt_mono() < delivered_at);
    // Direct response literal pins both private public-to-broker getters too.
    let literal = HttpResponse {
        status: 200,
        body: Vec::new(),
        received_at: original_wall,
        complete_at: original_mono,
    };
    assert_eq!(literal.received_at(), original_wall);
    assert_eq!(literal.receipt_mono(), original_mono);
}

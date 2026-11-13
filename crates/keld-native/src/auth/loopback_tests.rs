//! Proposed real temporary-socket contracts, not provider/browser/TLS proof.
use super::super::provider_profile;
use super::*;
use std::net::{SocketAddr, SocketAddrV4};
use std::thread;
use std::time::Duration;

fn flow() -> (LoopbackFlow, String) {
    let profile = provider_profile("entra-common").expect("closed profile");
    let plan = RedirectPlan::new(profile, None).expect("dynamic plan");
    let pending = PendingAuthorization::generate().expect("OS entropy");
    let flow = LoopbackFlow::bind(
        plan,
        pending,
        Instant::now() + Duration::from_secs(5),
        Arc::new(AtomicBool::new(false)),
    )
    .expect("real loopback bind");
    let uri = flow
        .authorization_url(profile, "test-client", None, None)
        .expect("URL");
    let state = uri
        .as_str()
        .split('?')
        .nth(1)
        .expect("query")
        .split('&')
        .find_map(|part| part.strip_prefix("state="))
        .expect("emitted state")
        .to_owned();
    (flow, state)
}
fn client(flow: &LoopbackFlow) -> TcpStream {
    let address = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, flow.bound_port));
    let stream =
        TcpStream::connect_timeout(&address, Duration::from_secs(1)).expect("real connection");
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("read bound");
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .expect("write bound");
    stream
}
fn finish(flow: &mut LoopbackFlow) -> Result<CorrelatedAuthorization, LoopbackError> {
    let kill_switch = Instant::now() + Duration::from_secs(6);
    loop {
        if let LoopbackProgress::Complete(result) = flow.poll() {
            return result;
        }
        assert!(
            Instant::now() < kill_switch,
            "loopback poll did not terminate"
        );
        thread::yield_now();
    }
}
fn request(flow: &LoopbackFlow, target: &str, host: &str) -> Vec<u8> {
    let _ = flow;
    format!("GET {target} HTTP/1.1\r\nHost: {host}\r\n\r\n").into_bytes()
}
fn read_page(stream: &mut TcpStream) -> Vec<u8> {
    let mut result = Vec::new();
    let mut chunk = [0u8; 256];
    loop {
        let count = stream.read(&mut chunk).expect("completion page");
        if count == 0 {
            return result;
        }
        assert!(result.len() + count <= 1024, "page bound");
        result.extend_from_slice(&chunk[..count]);
    }
}
#[test]
fn actual_dynamic_bind_fragmented_get_and_single_use_correlation() {
    let (mut flow, state) = flow();
    assert_eq!(
        flow.redirect().uri(),
        format!("http://localhost:{}/callback", flow.bound_port)
    );
    let target = format!("/callback?code=literal%2Bcode&state={state}");
    let wire = request(&flow, &target, &format!("localhost:{}", flow.bound_port));
    let mut peer = client(&flow);
    peer.write_all(&wire[..wire.len() - 1])
        .expect("fragment one");
    assert!(matches!(flow.poll(), LoopbackProgress::Pending)); // Real accept.
    assert!(matches!(flow.poll(), LoopbackProgress::Pending)); // Incomplete framing.
    peer.write_all(&wire[wire.len() - 1..])
        .expect("delimiter split");
    let authorization = finish(&mut flow).expect("actual correlated query");
    let form = authorization
        .token_form("test-client", flow.redirect())
        .expect("single-use form");
    assert!(form.as_str().contains("code=literal%2Bcode"));
    assert!(form.as_str().contains(&format!(
        "redirect_uri=http%3A%2F%2Flocalhost%3A{}%2Fcallback",
        flow.bound_port
    )));
    let page = read_page(&mut peer);
    assert!(page.starts_with(b"HTTP/1.1 200 OK\r\n"));
    assert!(!String::from_utf8_lossy(&page).contains(&state));
    assert!(!String::from_utf8_lossy(&page).contains("literal"));
    assert!(
        matches!(flow.poll(), LoopbackProgress::Complete(Err(error)) if error.code() == "KELD-AUTH-016")
    );
    assert!(
        TcpStream::connect_timeout(
            &SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, flow.bound_port)),
            Duration::from_secs(1)
        )
        .is_err()
    );
    assert!(flow.accepted.is_none() && flow.listener.is_none());
}
#[test]
fn quiet_first_peer_cannot_block_a_valid_second_callback() {
    let (mut flow, state) = flow();
    let original_deadline = flow.deadline;
    let mut quiet = client(&flow);
    while flow.accepted.is_none() {
        assert!(matches!(flow.poll(), LoopbackProgress::Pending));
        assert!(
            Instant::now() < original_deadline,
            "quiet accepted receipt absent"
        );
        thread::yield_now();
    }
    let accepted = flow.accepted.as_ref().expect("actual quiet accepted owner");
    assert_eq!(accepted.request.len(), 0);
    assert!(accepted.reply.is_none());
    let mut callback = client(&flow);
    let wire = request(
        &flow,
        &format!("/callback?code=C&state={state}"),
        &format!("localhost:{}", flow.bound_port),
    );
    callback
        .write_all(&wire)
        .expect("actual complete second GET");
    let correlated = finish(&mut flow).expect("valid second callback must progress");
    assert_eq!(
        flow.deadline, original_deadline,
        "absolute bound must not renew"
    );
    let form = correlated
        .token_form("test-client", flow.redirect())
        .expect("single-use form");
    assert!(form.as_str().contains("code=C"));
    let page = read_page(&mut callback); // reads the selected kernel stream to EOF
    assert!(page.starts_with(b"HTTP/1.1 200 OK\r\n"));
    assert!(!String::from_utf8_lossy(&page).contains(&state));
    assert!(!String::from_utf8_lossy(&page).contains("code=C"));
    let mut byte = [0u8; 1];
    assert!(
        matches!(quiet.read(&mut byte), Ok(0)),
        "quiet kernel peer did not close"
    );
    assert!(flow.listener.is_none() && flow.accepted.is_none() && flow.pending.is_none());
    assert!(matches!(flow.poll(), LoopbackProgress::Complete(Err(error))
        if error.code() == "KELD-AUTH-016"));
    actual_dynamic_bind_fragmented_get_and_single_use_correlation();
}

#[test]
fn full_idle_set_recycles_only_on_new_accept_then_allows_valid_callback() {
    let (mut flow, state) = flow();
    let original_deadline = flow.deadline;
    let mut first = client(&flow);
    first
        .set_nonblocking(true)
        .expect("nonblocking peer observation");
    while flow.accepted.is_none() {
        assert!(matches!(flow.poll(), LoopbackProgress::Pending));
        assert!(
            Instant::now() < original_deadline,
            "first accepted receipt absent"
        );
        thread::yield_now();
    }
    assert!(matches!(flow.poll(), LoopbackProgress::Pending));
    let mut byte = [0u8; 1];
    assert!(
        matches!(first.read(&mut byte), Err(error)
        if error.kind() == io::ErrorKind::WouldBlock),
        "ordinary idle poll closed first peer"
    );
    let mut second = client(&flow);
    assert!(matches!(flow.poll(), LoopbackProgress::Pending));
    let mut third = client(&flow);
    let second_address = second.local_addr().expect("second client identity");
    loop {
        assert!(matches!(flow.poll(), LoopbackProgress::Pending));
        let oldest_is_second = flow
            .accepted
            .as_ref()
            .is_some_and(|accepted| accepted.stream.peer_addr().ok() == Some(second_address));
        if oldest_is_second {
            break;
        }
        assert!(
            Instant::now() < original_deadline,
            "full idle set never admitted third peer"
        );
        thread::yield_now();
    }
    loop {
        match first.read(&mut byte) {
            Ok(0) => break,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
            other => panic!("recycled zero-byte peer did not close: {other:?}"),
        }
        assert!(
            Instant::now() < original_deadline,
            "recycled peer EOF absent"
        );
        thread::yield_now();
    }
    let mut callback = client(&flow);
    callback
        .write_all(&request(
            &flow,
            &format!("/callback?code=C&state={state}"),
            &format!("localhost:{}", flow.bound_port),
        ))
        .expect("valid callback after full idle set");
    let correlated = finish(&mut flow).expect("full idle set must not consume authorization");
    let form = correlated
        .token_form("test-client", flow.redirect())
        .expect("single-use form");
    assert!(form.as_str().contains("code=C"));
    assert_eq!(flow.deadline, original_deadline);
    assert!(read_page(&mut callback).starts_with(b"HTTP/1.1 200 OK\r\n"));
    assert!(matches!(second.read(&mut byte), Ok(0)));
    assert!(matches!(third.read(&mut byte), Ok(0)));
    assert!(flow.listener.is_none() && flow.accepted.is_none() && flow.pending.is_none());
}

#[test]
fn started_callback_remains_fatal_behind_a_quiet_peer() {
    for variant in 0..3 {
        let (mut flow, state) = flow();
        let original_deadline = flow.deadline;
        let mut quiet = client(&flow);
        while flow.accepted.is_none() {
            assert!(matches!(flow.poll(), LoopbackProgress::Pending));
            assert!(
                Instant::now() < original_deadline,
                "quiet accepted receipt absent"
            );
            thread::yield_now();
        }
        let mut invalid = client(&flow);
        let wire = match variant {
            0 => request(
                &flow,
                "/callback?code=C&state=wrong",
                &format!("localhost:{}", flow.bound_port),
            ),
            1 => request(
                &flow,
                &format!("/callback?code=C&state={state}"),
                "attacker.invalid",
            ),
            _ => b"GET /callback?state=incomplete HTTP/1.1\r\nHost:".to_vec(),
        };
        invalid
            .write_all(&wire)
            .expect("actual invalid second request");
        if variant == 2 {
            invalid
                .shutdown(std::net::Shutdown::Write)
                .expect("actual truncated EOF");
        }
        let error = finish(&mut flow).expect_err("invalid callback must consume whole flow");
        assert_eq!(error.code(), "KELD-AUTH-011");
        assert_eq!(flow.deadline, original_deadline);
        assert!(!error.to_string().contains(&state));
        let mut byte = [0u8; 1];
        assert!(matches!(quiet.read(&mut byte), Ok(0)));
        if variant == 2 {
            assert!(matches!(invalid.read(&mut byte), Ok(0)));
        } else {
            assert!(read_page(&mut invalid).starts_with(b"HTTP/1.1 400 Bad Request\r\n"));
        }
        assert!(flow.listener.is_none() && flow.accepted.is_none() && flow.pending.is_none());
        assert!(matches!(flow.poll(), LoopbackProgress::Complete(Err(error))
            if error.code() == "KELD-AUTH-016"));
    }
}

#[test]
fn host_path_framing_and_state_are_independent_real_socket_rejections() {
    for variant in 0..7 {
        let (mut flow, state) = flow();
        let valid_target = format!("/callback?code=C&state={state}");
        let valid_host = format!("localhost:{}", flow.bound_port);
        let wire = match variant {
            0 => request(&flow, &valid_target, "attacker.invalid"),
            1 => request(&flow, &format!("/wrong?code=C&state={state}"), &valid_host),
            2 => request(&flow, "/callback?code=C&state=wrong", &valid_host),
            3 => format!(
                "POST {valid_target} HTTP/1.1\r\nHost: {valid_host}\r\n\r\n"
            )
            .into_bytes(),
            4 => format!(
                "GET {valid_target} HTTP/1.1\r\nHost: {valid_host}\r\nHost: {valid_host}\r\n\r\n"
            )
            .into_bytes(),
            5 => format!(
                "GET {valid_target} HTTP/1.1\r\nHost: {valid_host}\r\nContent-Length: 1\r\n\r\nx"
            )
            .into_bytes(),
            _ => format!(
                "GET {valid_target} HTTP/1.1\r\nHost: {valid_host}\r\nTransfer-Encoding: chunked\r\n\r\n"
            )
            .into_bytes(),
        };
        assert!(wire.windows(4).any(|part| part == [13, 10, 13, 10]));
        assert!(
            wire.iter()
                .enumerate()
                .all(|(index, byte)| { *byte != 10 || (index > 0 && wire[index - 1] == 13) }),
            "negative fixture contains a bare LF"
        );
        let mut peer = client(&flow);
        peer.write_all(&wire).expect("rejection request");
        let error = finish(&mut flow).expect_err("callback rejected");
        assert_eq!(error.code(), "KELD-AUTH-011");
        assert!(!error.to_string().contains(&state));
        assert!(!format!("{error:?}").contains("wrong"));
        assert!(read_page(&mut peer).starts_with(b"HTTP/1.1 400 Bad Request\r\n"));
        assert!(flow.pending.is_none() && flow.listener.is_none() && flow.accepted.is_none());
    }
}
#[test]
fn fixed_bind_failure_is_004_and_never_falls_back_to_dynamic_port() {
    let occupied = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("occupied port");
    let port = occupied.local_addr().expect("actual port").port();
    let profile = provider_profile("entra-common").expect("profile");
    let plan = RedirectPlan::new(
        profile,
        Some(&format!("http://localhost:{port}/exact?tenant")),
    )
    .expect("fixed plan");
    assert_eq!(plan.bind_port(), port);
    let error = LoopbackFlow::bind(
        plan,
        PendingAuthorization::generate().expect("entropy"),
        Instant::now() + Duration::from_secs(2),
        Arc::new(AtomicBool::new(false)),
    )
    .expect_err("occupied bind refused");
    assert_eq!(error.code(), "KELD-AUTH-004");
    assert!(error.to_string().contains("bind"));
}
#[test]
fn cancel_and_absolute_bound_release_quiet_real_peer_then_allow_healthy_flow() {
    for cancelled in [true, false] {
        let (mut flow, _) = flow();
        let mut peer = client(&flow);
        // Pending is not an acceptance receipt. Observe the
        // actual accepted owner before stopping this quiet accepted connection.
        // Use the existing flow deadline, not a longer/new wait or sleep.
        while flow.accepted.is_none() {
            assert!(matches!(flow.poll(), LoopbackProgress::Pending));
            assert!(
                Instant::now() < flow.deadline,
                "accepted peer receipt missing"
            );
            thread::yield_now();
        }
        let accepted_before_stop = flow.accepted.is_some();
        let request_len = flow
            .accepted
            .as_ref()
            .map_or(0, |accepted| accepted.request.len());
        let reply_started = flow
            .accepted
            .as_ref()
            .is_some_and(|accepted| accepted.reply.is_some());
        eprintln!(
            "KELD_LOOPBACK_QUIET_PHASE cancelled={cancelled} accepted={accepted_before_stop} request_len={request_len} reply_started={reply_started}"
        );
        if cancelled {
            flow.cancel();
        } else {
            flow.deadline = Instant::now();
        }
        let error = finish(&mut flow).expect_err("stopped flow");
        assert_eq!(
            error.code(),
            if cancelled {
                "KELD-AUTH-007"
            } else {
                "KELD-AUTH-008"
            }
        );
        let mut byte = [0u8; 1];
        let peer_read = peer.read(&mut byte);
        match &peer_read {
            Ok(count) => eprintln!(
                "KELD_LOOPBACK_QUIET_READ cancelled={cancelled} accepted={accepted_before_stop} ok_count={count}"
            ),
            Err(error) => eprintln!(
                "KELD_LOOPBACK_QUIET_READ cancelled={cancelled} accepted={accepted_before_stop} error_kind={:?} raw_code={:?}",
                error.kind(),
                error.raw_os_error()
            ),
        }
        assert!(
            matches!(peer_read, Ok(0)),
            "quiet-peer EOF predicate failed; see attributed phase/read metadata"
        );
        assert!(flow.listener.is_none() && flow.accepted.is_none() && flow.pending.is_none());
    }
    actual_dynamic_bind_fragmented_get_and_single_use_correlation();
}
#[test]
fn real_http_frame_maximum_and_maximum_plus_one_use_existing_envelope() {
    for excess in [0, 1] {
        let (mut flow, state) = flow();
        let mut peer = client(&flow);
        let mut wire = format!(
            "GET /callback?code=C&state={state} HTTP/1.1\r\nHost: localhost:{}\r\nX-Pad: ",
            flow.bound_port
        )
        .into_bytes();
        wire.resize(MAX_OAUTH_TEXT_BYTES - 4 + excess, b'x');
        wire.extend_from_slice(b"\r\n\r\n");
        assert_eq!(wire.len(), MAX_OAUTH_TEXT_BYTES + excess);
        let writer = thread::spawn(move || {
            let mut accepted = 0;
            while accepted < wire.len() {
                match peer.write(&wire[accepted..]) {
                    Ok(0) | Err(_) => break,
                    Ok(count) => accepted += count,
                }
            }
            // Keep the client socket owned in the JoinHandle return until
            // callback/reply processing finishes; early local EOF is no oracle.
            (accepted, peer)
        });
        let result = finish(&mut flow);
        let (accepted, mut peer) = writer.join().expect("writer owner joined");
        assert_eq!(
            accepted,
            MAX_OAUTH_TEXT_BYTES + excess,
            "not all bounded frame bytes were accepted"
        );
        if excess == 0 {
            assert!(result.is_ok(), "maximum frame must remain accepted");
            assert!(read_page(&mut peer).starts_with(b"HTTP/1.1 200 OK\r\n"));
        } else {
            let error = result.expect_err("max-plus-one HTTP frame rejected");
            assert_eq!(error.code(), "KELD-AUTH-016");
            assert!(matches!(
                error.failure,
                Failure::Primitive(PrimitiveError::InputLimit)
            ));
        }
        assert!(flow.listener.is_none() && flow.accepted.is_none());
    }
}

#[test]
fn synthetic_default_ipv4_and_exact_encoded_callback_target_use_owning_plan() {
    // Synthetic default fixture is not a shipping provider.
    let synthetic = ProviderProfile {
        key: "fixture",
        issuer: "https://fixture.invalid",
        authorization_endpoint: "https://fixture.invalid/authorize",
        token_endpoint: "https://fixture.invalid/token",
        redirect_form: RedirectForm::Ipv4Loopback,
    };
    let plan = RedirectPlan::new(&synthetic, None).expect("synthetic plan");
    let flow = LoopbackFlow::bind(
        plan,
        PendingAuthorization::generate().expect("entropy"),
        Instant::now() + Duration::from_secs(2),
        Arc::new(AtomicBool::new(false)),
    )
    .expect("default actual bind");
    assert_eq!(
        flow.redirect().uri(),
        format!("http://127.0.0.1:{}/callback", flow.bound_port)
    );
    let redirect = RedirectPlan::new(&synthetic, Some("HTTP://LOCALHOST:4321/a%2Fb?tenant=a%26b"))
        .expect("exact plan")
        .complete(4321)
        .expect("port");
    assert_eq!(redirect.uri(), "HTTP://LOCALHOST:4321/a%2Fb?tenant=a%26b");
    let literal =
        b"GET /a%2Fb?tenant=a%26b&code=C&state=S HTTP/1.1\r\nHost: LOCALHOST:4321\r\n\r\n";
    assert_eq!(
        request_query(literal, literal.len(), &redirect, 4321).expect("raw encoded route"),
        "tenant=a%26b&code=C&state=S"
    );
    let changed = b"GET /a/b?tenant=a%26b&code=C&state=S HTTP/1.1\r\nHost: localhost:4321\r\n\r\n";
    assert!(request_query(changed, changed.len(), &redirect, 4321).is_err());
}

#[test]
fn late_page_cancellation_preserves_the_original_callback_fault() {
    let (mut flow, _) = flow();
    let mut peer = client(&flow);
    peer.write_all(&request(
        &flow,
        "/wrong?code=C&state=wrong",
        &format!("localhost:{}", flow.bound_port),
    ))
    .expect("bad callback");
    let phase_bound = Instant::now() + Duration::from_secs(2);
    while flow
        .accepted
        .as_ref()
        .is_none_or(|accepted| accepted.reply.is_none())
    {
        assert!(matches!(flow.poll(), LoopbackProgress::Pending));
        assert!(
            Instant::now() < phase_bound,
            "callback decision phase absent"
        );
        thread::yield_now();
    }
    flow.cancel();
    let error = finish(&mut flow).expect_err("cancelled page delivery");
    assert_eq!(error.code(), "KELD-AUTH-011");
    assert!(matches!(error.delivery_stopped, Some(Stop::Cancelled)));
    assert!(error.to_string().contains("cancelled before"));
}

#[test]
fn truncated_http_eof_discards_the_flow_before_correlation() {
    let (mut flow, _) = flow();
    let mut peer = client(&flow);
    peer.write_all(b"GET /callback?state=incomplete HTTP/1.1\r\nHost:")
        .expect("truncated request");
    peer.shutdown(std::net::Shutdown::Write)
        .expect("actual request EOF");
    let error = finish(&mut flow).expect_err("truncated frame refused");
    assert_eq!(error.code(), "KELD-AUTH-011");
    assert!(flow.pending.is_none() && flow.listener.is_none() && flow.accepted.is_none());
}

#[test]
fn fixture_request_wire_is_literal_crlf_http() {
    let (flow, _) = flow();
    let wire = request(&flow, "/callback?code=C&state=S", "localhost:4321");
    assert_eq!(
        wire,
        b"GET /callback?code=C&state=S HTTP/1.1\r\nHost: localhost:4321\r\n\r\n"
    );
    assert!(!wire.windows(2).any(|pair| pair == b"\n\n"));
}

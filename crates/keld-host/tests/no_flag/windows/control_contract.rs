//! Control byte/deadline and terminal-diagnostic contracts.

use std::io::{BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::support::control::{
    HandshakeKillSwitches, accept_ready_generation_with_descendant,
    accept_ready_generation_with_descendant_within, read_control_line_or_host_failure,
    try_read_control_line,
};
use crate::support::process::{wait_for_child_process, wait_for_cleanup_sentinel};
use crate::{CONTROL_LINE_LIMIT, PRODUCT_DEADLINE, startup_kill_switch, wait_child};

#[test]
fn control_line_preserves_the_byte_limit_and_expired_read_does_not_consume_data() {
    assert_eq!(CONTROL_LINE_LIMIT, 4096);
    for size in [4096, 4097] {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("control boundary listener");
        let mut peer =
            TcpStream::connect(listener.local_addr().expect("address")).expect("control peer");
        let (stream, _) = listener.accept().expect("accept control peer");
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("original timeout");
        let mut reader = BufReader::new(stream);
        let mut message = vec![b'x'; size - 1];
        message.push(b'\n');
        peer.write_all(&message).expect("send control boundary");
        let expired = try_read_control_line(&mut reader, Instant::now());
        assert_eq!(
            expired.err().as_deref(),
            Some("control line deadline elapsed")
        );
        let actual = try_read_control_line(&mut reader, Instant::now() + Duration::from_secs(1));
        if size == 4096 {
            assert_eq!(actual.expect("exact control bound").len(), 4095);
        } else {
            assert_eq!(
                actual.err().as_deref(),
                Some("control line exceeds 4096 bytes")
            );
        }
        assert_eq!(
            reader.get_ref().read_timeout().expect("restored timeout"),
            Some(Duration::from_secs(2))
        );
    }
}

#[test]
fn startup_terminal_record_preserves_host_failure_output() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("diagnostic control listener");
    let mut writer = TcpStream::connect(listener.local_addr().expect("control address"))
        .expect("diagnostic control peer");
    let (stream, _) = listener.accept().expect("accept diagnostic peer");
    let mut reader = BufReader::new(stream);
    writer
        .write_all(b"READY\nLINK_EOF\n")
        .expect("control records");
    let mut child = Command::new("cmd.exe")
        .args([
            "/d",
            "/c",
            "echo KEL265_DIAGNOSTIC_STDOUT & echo KEL265_DIAGNOSTIC_STDERR 1>&2 & exit /b 23",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("diagnostic child");
    let status = wait_child(&mut child, Instant::now() + PRODUCT_DEADLINE);
    assert_eq!(status.code(), Some(23));
    assert_eq!(
        read_control_line_or_host_failure(&mut reader, &mut child, "fixture READY"),
        "READY"
    );
    let failure = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        read_control_line_or_host_failure(&mut reader, &mut child, "fixture READY")
    }))
    .expect_err("terminal startup record must preserve the failed child diagnostics");
    let message = failure
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| failure.downcast_ref::<&str>().copied())
        .expect("panic message");
    for expected in [
        "LINK_EOF",
        "fixture READY",
        "KEL265_DIAGNOSTIC_STDOUT",
        "KEL265_DIAGNOSTIC_STDERR",
        "exit code: 23",
    ] {
        assert!(message.contains(expected), "missing {expected}: {message}");
    }
    assert!(
        child.stdout.is_none() && child.stderr.is_none(),
        "both captures were consumed"
    );
}

#[test]
fn startup_eof_before_ready_reports_last_valid_phase_and_host_failure_output() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("pre-ready control listener");
    let mut peer = TcpStream::connect(listener.local_addr().expect("control address"))
        .expect("pre-ready control peer");
    peer.write_all(b"HELLO 31337 fixture-private-app-link-token\nDESCENDANT 31338\nLINK_EOF\n")
        .expect("pre-ready terminal records");
    let mut child = Command::new("cmd.exe")
        .args([
            "/d",
            "/c",
            "echo KEL257_DIAGNOSTIC_STDOUT & echo KEL257_DIAGNOSTIC_STDERR 1>&2 & exit /b 23",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("pre-ready diagnostic child");
    let child_pid = child.id();
    assert_eq!(
        wait_child(&mut child, Instant::now() + PRODUCT_DEADLINE).code(),
        Some(23)
    );
    let failure = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        accept_ready_generation_with_descendant(&listener, &mut child)
    }))
    .expect_err("LINK_EOF before READY must fail generation acceptance");
    let message = failure
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| failure.downcast_ref::<&str>().copied())
        .expect("panic message");
    for expected in [
        "awaiting READY",
        "last valid phase=DESCENDANT",
        "LINK_EOF",
        "exit code: 23",
        "stdout:\nKEL257_DIAGNOSTIC_STDOUT",
        "stderr:\nKEL257_DIAGNOSTIC_STDERR",
    ] {
        assert!(message.contains(expected), "missing {expected}: {message}");
    }
    assert!(
        message.contains(&format!("child pid={child_pid}")),
        "missing actual child identity: {message}"
    );
    assert!(
        !message.contains("fixture-private-app-link-token")
            && !message.contains("last valid phase=READY"),
        "diagnostic disclosed HELLO data or advanced past terminal failure: {message}"
    );
    assert!(
        child.stdout.is_none() && child.stderr.is_none(),
        "both exited-child captures were consumed"
    );
}

#[test]
fn startup_eof_before_ready_reports_live_host_without_consuming_output() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("live pre-ready listener");
    let mut peer = TcpStream::connect(listener.local_addr().expect("control address"))
        .expect("live pre-ready peer");
    peer.write_all(b"HELLO 31337 fixture-private-app-link-token\nDESCENDANT 31338\nLINK_EOF\n")
        .expect("live pre-ready terminal records");
    // The retained stdin writer keeps this command blocked on an observable
    // resource, independently of the control helper's two-second exit wait.
    let mut child = Command::new("cmd.exe")
        .args(["/d", "/c", "set /p KEL257_INPUT="])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("stdin-blocked diagnostic child");
    let child_pid = child.id();
    let started = Instant::now();
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        accept_ready_generation_with_descendant(&listener, &mut child)
    }));
    let elapsed = started.elapsed();
    let observed_status = child.try_wait().expect("observe live child after failure");
    let captures_retained = child.stdout.is_some() && child.stderr.is_some();
    if observed_status.is_none() {
        child.kill().expect("clean up stdin-blocked child");
        wait_child(&mut child, Instant::now() + PRODUCT_DEADLINE);
    }
    let failure = outcome.expect_err("a live child cannot turn LINK_EOF into READY");
    let message = failure
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| failure.downcast_ref::<&str>().copied())
        .expect("panic message");
    for expected in [
        "awaiting READY",
        "last valid phase=DESCENDANT",
        "LINK_EOF",
        "child status=still running",
        "stdout/stderr=not read while child is alive",
    ] {
        assert!(message.contains(expected), "missing {expected}: {message}");
    }
    assert!(
        message.contains(&format!("child pid={child_pid}")),
        "missing actual live-child identity: {message}"
    );
    assert!(
        !message.contains("exit code:")
            && !message.contains("fixture-private-app-link-token")
            && !message.contains("last valid phase=READY"),
        "diagnostic fabricated an exit, disclosed HELLO, or advanced phase: {message}"
    );
    assert!(observed_status.is_none(), "fixture exited before cleanup");
    assert!(
        captures_retained,
        "live-child captures must remain unconsumed"
    );
    assert!(
        elapsed < Duration::from_secs(5),
        "terminal failure exceeded the existing bounded wait: {elapsed:?}"
    );
}

#[test]
fn startup_ready_echo_sequence_preserves_generation_values() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("ready control listener");
    let mut peer = TcpStream::connect(listener.local_addr().expect("control address"))
        .expect("ready control peer");
    peer.write_all(
        b"HELLO 31337 fixture-private-app-link-token\nDESCENDANT 31338\nREADY\nECHO1\nECHO2\n",
    )
    .expect("complete startup records");
    let mut child = Command::new("cmd.exe")
        .args(["/d", "/c", "exit /b 0"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("positive-flow child");
    assert!(wait_child(&mut child, Instant::now() + PRODUCT_DEADLINE).success());
    let (_, _, pid, link, descendant_pid) =
        accept_ready_generation_with_descendant(&listener, &mut child);
    assert_eq!(pid, 31337);
    assert_eq!(link, "fixture-private-app-link-token");
    assert_eq!(descendant_pid, 31338);
    assert!(
        child.stdout.is_some() && child.stderr.is_some(),
        "successful startup must not consume diagnostic captures"
    );
}

/// A session's READY arrives only after the host's `WebView2` startup, whose own
/// bounded phases (keld-wv's `WEBVIEW2_STARTUP_BOUND`) sum past the per-step product
/// deadline. The harness must accept a READY that is late by per-step standards while
/// the host is still alive, and must not stop at that per-step deadline.
///
/// The handshake runs under injected kill switches: a 250 ms per-step `record`
/// switch stands in for the product deadline, so a READY delayed past it proves the
/// READY read uses the `ready` switch without waiting out 20 s. The product pairing of
/// the two switches is pinned separately below.
#[test]
fn startup_ready_after_the_product_deadline_is_accepted_within_the_startup_bound() {
    let product = HandshakeKillSwitches::product();
    assert_eq!(product.record, PRODUCT_DEADLINE);
    assert_eq!(product.ready, startup_kill_switch());
    assert!(
        product.ready > product.record,
        "a product READY must outlast the per-step deadline: {product:?}"
    );

    let injected = HandshakeKillSwitches {
        record: Duration::from_millis(250),
        ready: Duration::from_secs(30),
    };
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("slow-startup control listener");
    let mut peer = TcpStream::connect(listener.local_addr().expect("control address"))
        .expect("slow-startup control peer");
    peer.write_all(b"HELLO 31337 fixture-private-app-link-token\nDESCENDANT 31338\n")
        .expect("pre-startup records");
    // The live, stdin-blocked child stands in for a host that is still starting.
    let mut child = Command::new("cmd.exe")
        .args(["/d", "/c", "set /p KEL824_INPUT="])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("stdin-blocked slow-startup child");
    let late_by = injected.record * 4;
    let started = Instant::now();
    // Fault injection, not synchronization: this peer is a startup that outlasts the
    // per-step switch, which the ready switch admits.
    let slow_startup = thread::spawn(move || {
        thread::sleep(late_by);
        peer.write_all(b"READY\nECHO1\nECHO2\n")
            .expect("late startup records");
        peer
    });
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        accept_ready_generation_with_descendant_within(&listener, &mut child, injected)
    }));
    let elapsed = started.elapsed();
    let _peer = slow_startup.join().expect("slow startup peer");
    child.kill().expect("clean up stdin-blocked child");
    wait_child(&mut child, Instant::now() + PRODUCT_DEADLINE);
    let (_, _, pid, link, descendant_pid) =
        outcome.expect("a live host's READY within the ready switch must be accepted");
    assert_eq!(pid, 31337);
    assert_eq!(link, "fixture-private-app-link-token");
    assert_eq!(descendant_pid, 31338);
    assert!(
        elapsed >= late_by,
        "READY was accepted before the injected startup delay: {elapsed:?}"
    );
}

/// A `keld dev` parent that exits before delegating its host or cleanup sentinel
/// can never delegate it. Both waits fail at once with the parent's status and
/// captured output, rather than waiting out their deadline and reporting only
/// `parent_alive=false` (the unexplained GH-1182 host-death failure under load).
#[test]
fn delegation_waits_report_an_exited_parent_with_its_output() {
    type DelegationWait = fn(&mut std::process::Child, Instant) -> u32;
    let host: DelegationWait =
        |parent, deadline| wait_for_child_process(parent, "keld-host.exe", deadline);
    let sentinel: DelegationWait = wait_for_cleanup_sentinel;
    for (wait, delegate) in [(host, "keld-host.exe"), (sentinel, "cleanup sentinel")] {
        let mut parent = Command::new("cmd.exe")
            .args([
                "/d",
                "/c",
                "echo GH1182_PARENT_STDOUT & echo GH1182_PARENT_STDERR 1>&2 & exit /b 29",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("exiting delegation parent");
        let parent_pid = parent.id();
        let started = Instant::now();
        let failure = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            wait(&mut parent, Instant::now() + PRODUCT_DEADLINE)
        }))
        .expect_err("an exited parent cannot delegate");
        let message = failure
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| failure.downcast_ref::<&str>().copied())
            .expect("panic message");
        for expected in [
            format!("parent {parent_pid} exited before delegating `{delegate}`"),
            "exit code: 29".to_owned(),
            "stdout:\nGH1182_PARENT_STDOUT".to_owned(),
            "stderr:\nGH1182_PARENT_STDERR".to_owned(),
        ] {
            assert!(message.contains(&expected), "missing {expected}: {message}");
        }
        assert!(
            started.elapsed() < PRODUCT_DEADLINE,
            "the wait ran out its deadline instead of failing on the parent's exit"
        );
    }
}

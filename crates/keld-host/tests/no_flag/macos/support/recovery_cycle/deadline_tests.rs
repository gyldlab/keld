//! Actual native Unix control-accept/line cuts for the recovered Bun fixture.
//! The numeric record fields are synthetic; no process is created or signaled.
use super::{
    ControlRecord, MAX_INITIAL_CONTROL_LINE_BYTES, NativeRecoveryStage, RecoveryControlFailure,
    accept_recovery_until, collect_initial_control_until, read_initial_line_until,
};
use std::io::Write as _;
use std::os::unix::net::{UnixListener, UnixStream};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

const NATIVE_CUT: Duration = Duration::from_millis(850);
const ENTRY_LIMIT: Duration = Duration::from_millis(600);
const RESCUE_LIMIT: Duration = Duration::from_secs(2);
const HELLO: &[u8] = b"HELLO 54321 /tmp/owned-fixture.sock#test-only\n";
const DESCENDANT: &[u8] = b"DESCENDANT 65432\n";

pub(super) fn native_listener() -> (tempfile::TempDir, UnixListener, PathBuf) {
    let root = tempfile::Builder::new()
        .prefix("krg-")
        .tempdir_in("/tmp")
        .expect("owned short Darwin Unix control directory");
    let path = root.path().join("control.sock");
    let listener = UnixListener::bind(&path).expect("real Unix recovery listener");
    listener
        .set_nonblocking(true)
        .expect("same nonblocking mode as recovery fixture");
    (root, listener, path)
}

#[test]
fn native_accept_completing_after_original_clock_is_rejected_at_accept() {
    let (_root, listener, address) = native_listener();
    let deadline = Instant::now() + NATIVE_CUT;
    let (entered_tx, entered_rx) = mpsc::sync_channel(1);
    let (released_tx, released_rx) = mpsc::sync_channel(1);
    let (accepted_tx, accepted_rx) = mpsc::sync_channel(1);
    let controller = thread::spawn(move || {
        let entered = entered_rx.recv_timeout(ENTRY_LIMIT);
        while Instant::now() < deadline {
            thread::yield_now();
        }
        let connected_at = Instant::now();
        let connected = UnixStream::connect(&address);
        let _ = released_tx.send(());
        (entered, connected_at, connected)
    });
    let mut first_poll = true;
    let result = catch_unwind(AssertUnwindSafe(|| {
        accept_recovery_until(&listener, deadline, &mut |stage| match stage {
            NativeRecoveryStage::BeforeAccept if first_poll => {
                first_poll = false;
                let _ = entered_tx.send(Instant::now());
                let _ = released_rx.recv_timeout(RESCUE_LIMIT);
            }
            NativeRecoveryStage::AfterAccept => {
                let _ = accepted_tx.send(Instant::now());
            }
            _ => {}
        })
    }));
    // The native peer remains owned until after the controller joins,
    // including a panic by the reader or the regression's expected refusal.
    let (entered, connected_at, connected) = controller.join().expect("join Unix peer");
    let accepted_at = accepted_rx.recv_timeout(RESCUE_LIMIT);
    assert!(
        entered.is_ok_and(|time| time < deadline),
        "native accept stage must begin before the original clock"
    );
    assert!(
        connected_at >= deadline,
        "the real peer must connect after cutoff"
    );
    assert!(
        connected.is_ok(),
        "native Unix control connection must succeed"
    );
    assert!(
        accepted_at.is_ok_and(|time| time >= deadline),
        "native accept callback must observe the real accepted peer after cutoff"
    );
    assert_eq!(
        result
            .expect("native accept reader must not panic")
            .expect_err("late native accept must fail directly, before any line read"),
        RecoveryControlFailure::AcceptCompletedLate
    );
}

fn late_native_lf_refusal(line: u8) {
    let (_root, listener, address) = native_listener();
    let deadline = Instant::now() + NATIVE_CUT;
    let (before_tx, before_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::sync_channel(1);
    let (accepted_tx, accepted_rx) = mpsc::sync_channel(1);
    let (lf_tx, lf_rx) = mpsc::sync_channel(1);
    let prefix: &[u8] = match line {
        0 => &HELLO[..HELLO.len() - 1],
        1 => &DESCENDANT[..DESCENDANT.len() - 1],
        _ => unreachable!("two native initial control records"),
    };
    let prefix_len = prefix.len();
    let controller = thread::spawn(move || {
        let mut peer = UnixStream::connect(&address).expect("connect real Unix control peer");
        peer.set_write_timeout(Some(RESCUE_LIMIT))
            .expect("bounded real control writer");
        if line == 1 {
            peer.write_all(HELLO).expect("write timely real HELLO");
        }
        peer.write_all(prefix)
            .expect("write real native record prefix");
        let entered = before_rx.recv_timeout(ENTRY_LIMIT);
        while Instant::now() < deadline {
            thread::yield_now();
        }
        let written_at = Instant::now();
        let written = peer.write_all(b"\n");
        let _ = release_tx.send(());
        (entered, written_at, written)
    });

    let mut first_lf_poll = true;
    let result = catch_unwind(AssertUnwindSafe(|| {
        let mut accepted = accept_recovery_until(&listener, deadline, &mut |stage| {
            if let NativeRecoveryStage::AfterAccept = stage {
                let _ = accepted_tx.send(Instant::now());
            }
        })
        .expect("native peer accepted before deadline");
        accepted
            .set_nonblocking(true)
            .expect("actual nonblocking native byte observer");
        if line == 1 {
            let hello = read_initial_line_until(&mut accepted, deadline, 0, &mut |_| {})
                .expect("real HELLO completed in original clock");
            assert_eq!(hello.as_bytes(), &HELLO[..HELLO.len() - 1]);
        }
        read_initial_line_until(&mut accepted, deadline, line, &mut |stage| match stage {
            NativeRecoveryStage::BeforeByte {
                line: current,
                received,
            } if current == line && received == prefix_len && first_lf_poll => {
                first_lf_poll = false;
                let _ = before_tx.send(Instant::now());
                let _ = release_rx.recv_timeout(RESCUE_LIMIT);
            }
            NativeRecoveryStage::AfterLf { line: current } if current == line => {
                let _ = lf_tx.send(Instant::now());
            }
            _ => {}
        })
    }));
    let (entered, written_at, written) = controller.join().expect("join real LF writer");
    let accepted_at = accepted_rx.recv_timeout(RESCUE_LIMIT);
    let lf_at = lf_rx.recv_timeout(RESCUE_LIMIT);
    assert!(
        accepted_at.is_ok_and(|time| time < deadline),
        "native Unix accept must precede the original clock"
    );
    assert!(
        entered.is_ok_and(|time| time < deadline),
        "the actual native reader must reach LF before original cutoff"
    );
    assert!(
        written_at >= deadline,
        "native peer must write LF after cutoff"
    );
    assert!(written.is_ok(), "native LF write must physically succeed");
    assert!(
        lf_at.is_ok_and(|time| time >= deadline),
        "actual native LF must be consumed by the reader after cutoff"
    );
    assert_eq!(
        result
            .expect("native line reader must not panic")
            .expect_err("late native LF must fail directly, before later frame parsing"),
        RecoveryControlFailure::LineLfLate(ControlRecord::of_line(line))
    );
}

#[test]
fn native_hello_lf_after_original_clock_fails_at_lf() {
    late_native_lf_refusal(0);
}

#[test]
fn native_descendant_lf_after_original_clock_fails_at_lf() {
    late_native_lf_refusal(1);
}

#[test]
fn timely_native_hello_and_descendant_retain_exact_bytes() {
    let (_root, listener, address) = native_listener();
    let (release_tx, release_rx) = mpsc::sync_channel(1);
    let controller = thread::spawn(move || {
        let mut peer = UnixStream::connect(&address).expect("connect timely Unix peer");
        peer.set_write_timeout(Some(RESCUE_LIMIT))
            .expect("bounded timely writer");
        peer.write_all(HELLO).expect("write timely HELLO");
        peer.write_all(DESCENDANT).expect("write timely DESCENDANT");
        // Keep the live peer through accepted-stream normalization. The
        // rescue timeout prevents a failed reader from stranding the writer.
        let _ = release_rx.recv_timeout(RESCUE_LIMIT);
    });
    let deadline = Instant::now() + Duration::from_secs(2);
    let (mut accepted_at, mut hello_at, mut descendant_at) = (None, None, None);
    let result = catch_unwind(AssertUnwindSafe(|| {
        collect_initial_control_until(&listener, deadline, &mut |stage| match stage {
            NativeRecoveryStage::AfterAccept => accepted_at = Some(Instant::now()),
            NativeRecoveryStage::AfterLf { line: 0 } => hello_at = Some(Instant::now()),
            NativeRecoveryStage::AfterLf { line: 1 } => descendant_at = Some(Instant::now()),
            _ => {}
        })
    }));
    let _ = release_tx.send(());
    controller
        .join()
        .expect("join timely native Unix controller");
    let (peer, hello, descendant) = result
        .expect("timely native observer did not panic")
        .expect("timely valid control records remain accepted");
    assert_eq!(hello.as_bytes(), &HELLO[..HELLO.len() - 1]);
    assert_eq!(descendant.as_bytes(), &DESCENDANT[..DESCENDANT.len() - 1]);
    assert!(accepted_at.is_some_and(|at| at < deadline));
    assert!(hello_at.is_some_and(|at| at < deadline));
    assert!(descendant_at.is_some_and(|at| at < deadline));
    drop(peer);
}

#[test]
fn timely_native_records_survive_owned_peer_close_before_steady_reader_setup() {
    let (_root, listener, address) = native_listener();
    let (close_tx, close_rx) = mpsc::sync_channel(1);
    let (closed_tx, closed_rx) = mpsc::sync_channel(1);
    let controller = thread::spawn(move || {
        let mut peer = UnixStream::connect(&address).expect("connect real Unix peer");
        peer.set_write_timeout(Some(RESCUE_LIMIT))
            .expect("bound native complete writer");
        peer.write_all(HELLO).expect("write real HELLO");
        peer.write_all(DESCENDANT).expect("write real DESCENDANT");
        let release = close_rx.recv_timeout(RESCUE_LIMIT);
        drop(peer);
        let _ = closed_tx.send((Instant::now(), release));
    });
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut closed_at = None;
    let result = catch_unwind(AssertUnwindSafe(|| {
        collect_initial_control_until(&listener, deadline, &mut |stage| {
            if let NativeRecoveryStage::AfterLf { line: 1 } = stage {
                let _ = close_tx.send(());
                let (at, release) = closed_rx
                    .recv_timeout(RESCUE_LIMIT)
                    .expect("real peer closure after native DESCENDANT LF");
                assert!(release.is_ok(), "peer waited for exact second LF");
                closed_at = Some(at);
            }
        })
    }));
    controller.join().expect("reap test-owned closed peer");
    assert!(
        closed_at.is_some_and(|at| at < deadline),
        "the real peer must close before the original clock"
    );
    let (accepted, hello, descendant) = result
        .expect("native control reader must not panic")
        .expect("both timely records remain accepted after real peer close");
    assert_eq!(hello.as_bytes(), &HELLO[..HELLO.len() - 1]);
    assert_eq!(descendant.as_bytes(), &DESCENDANT[..DESCENDANT.len() - 1]);
    drop(accepted);
}

#[test]
fn native_control_lines_bound_utf8_cap_and_eof_without_disclosing_bytes() {
    let cases = [
        (vec![0xff, b'\n'], RecoveryControlFailure::LineInvalidUtf8),
        (
            vec![b'x'; MAX_INITIAL_CONTROL_LINE_BYTES + 1],
            RecoveryControlFailure::LineOversized,
        ),
        (
            b"private incomplete data".to_vec(),
            RecoveryControlFailure::LineEof,
        ),
    ];
    for (input, expected) in cases {
        let (mut reader, mut writer) = UnixStream::pair().expect("owned Unix byte pair");
        reader
            .set_nonblocking(true)
            .expect("poll native invalid line");
        writer
            .set_write_timeout(Some(RESCUE_LIMIT))
            .expect("bound writer");
        writer
            .write_all(&input)
            .expect("write native invalid bytes");
        drop(writer);
        let failure = read_initial_line_until(
            &mut reader,
            Instant::now() + Duration::from_secs(2),
            0,
            &mut |_| {},
        )
        .expect_err("malformed native line must be refused");
        assert_eq!(
            failure, expected,
            "failed to classify the malformed native line"
        );
        assert!(
            !format!("{failure:?}").contains("private incomplete data"),
            "failure exposed private native payload"
        );
    }
}

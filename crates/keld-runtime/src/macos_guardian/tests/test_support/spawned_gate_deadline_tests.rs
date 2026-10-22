//! Native Unix admission and byte-read cuts for the shared pre-exec test gate.
//! The numeric fields are fixture text, never used to signal a process.
use super::{SpawnedGateStage, await_spawned_gate_until};
use std::io::{self, Write as _};
use std::os::unix::net::{UnixListener, UnixStream};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

const GATE_CUT: Duration = Duration::from_millis(750);
const ENTRY_LIMIT: Duration = Duration::from_millis(500);
const RESCUE_LIMIT: Duration = Duration::from_secs(2);
const FIXTURE_GROUP: i32 = 54_321;
const FIXTURE_GUARDIAN: u32 = 65_432;

fn native_record() -> (i32, Vec<u8>, Vec<u8>) {
    // Keep the synthetic numeric record distinct from the test runner's
    // actual PGID so the unchanged parser cannot fail on an accidental match.
    let group = if nix::unistd::getpgrp().as_raw() == FIXTURE_GROUP {
        FIXTURE_GROUP + 1
    } else {
        FIXTURE_GROUP
    };
    let prefix = format!("SPAWNED {group} {FIXTURE_GUARDIAN}").into_bytes();
    let mut complete = prefix.clone();
    complete.push(b'\n');
    (group, prefix, complete)
}

fn native_listener() -> (tempfile::TempDir, UnixListener, PathBuf) {
    // Short owner-private Darwin socket path; never contact a product listener.
    let directory = tempfile::tempdir_in("/tmp").expect("owned Unix gate directory");
    let address = directory.path().join("gate.sock");
    let listener = UnixListener::bind(&address).expect("native Unix gate listener");
    listener
        .set_nonblocking(true)
        .expect("same nonblocking native accept path as the Guardian fixtures");
    (directory, listener, address)
}

fn write_native_record(address: &PathBuf, bytes: &[u8]) -> io::Result<()> {
    let mut peer = UnixStream::connect(address)?;
    peer.set_write_timeout(Some(RESCUE_LIMIT))?;
    peer.write_all(bytes)
}

fn assert_expected_panic(result: std::thread::Result<(UnixStream, i32, u32)>, expected: &str) {
    let panic = result.expect_err("a late native record must not be accepted");
    let message = panic
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| panic.downcast_ref::<&str>().copied())
        .expect("deadline failure has a descriptive fixed-message panic");
    assert!(
        message.contains(expected),
        "wrong native gate failure; expected {expected:?}, observed {message:?}"
    );
}

#[test]
fn actual_native_accept_after_original_clock_cannot_authorize_gate() {
    let (_directory, listener, address) = native_listener();
    let (_, _, record) = native_record();
    let deadline = Instant::now() + GATE_CUT;
    let (before_tx, before_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::sync_channel(1);
    let (accepted_tx, accepted_rx) = mpsc::sync_channel(1);

    let writer = thread::spawn(move || {
        let entered = before_rx.recv_timeout(ENTRY_LIMIT);
        // A real connection and the complete valid record are published only
        // after the same monotonic cutoff observed by the waiting native accept.
        while Instant::now() < deadline {
            thread::yield_now();
        }
        let published_at = Instant::now();
        let sent = write_native_record(&address, &record);
        let _ = release_tx.send(());
        (entered, published_at, sent)
    });
    let mut first_poll = true;
    let result = catch_unwind(AssertUnwindSafe(|| {
        await_spawned_gate_until(&listener, deadline, |stage| match stage {
            SpawnedGateStage::BeforeAccept if first_poll => {
                first_poll = false;
                let _ = before_tx.send(Instant::now());
                let _ = release_rx.recv_timeout(RESCUE_LIMIT);
            }
            SpawnedGateStage::AfterAccept => {
                let _ = accepted_tx.send(Instant::now());
            }
            _ => {}
        })
    }));
    // Always join the retained real Unix peer before inspecting a reader panic.
    let (entered, published_at, sent) = writer.join().expect("join native gate writer");
    let accepted_at = accepted_rx.recv_timeout(RESCUE_LIMIT);
    assert!(
        entered.is_ok_and(|time| time < deadline),
        "the actual native accept was entered before the original cutoff"
    );
    assert!(published_at >= deadline, "peer must arrive after cutoff");
    assert!(
        sent.is_ok(),
        "complete valid record must reach the real socket"
    );
    assert!(
        accepted_at.is_ok_and(|time| time >= deadline),
        "the OS must actually accept the peer after the original cutoff"
    );
    assert_expected_panic(
        result,
        "native gate accept completed after original deadline",
    );
}

#[test]
fn actual_native_lf_after_original_clock_cannot_authorize_gate() {
    let (_directory, listener, address) = native_listener();
    let (_, prefix, _) = native_record();
    let prefix_len = prefix.len();
    let deadline = Instant::now() + GATE_CUT;
    let (before_lf_tx, before_lf_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::sync_channel(1);
    let (accepted_tx, accepted_rx) = mpsc::sync_channel(1);
    let (lf_tx, lf_rx) = mpsc::sync_channel(1);

    let writer = thread::spawn(move || {
        let mut peer = UnixStream::connect(&address).expect("connect real Unix LF writer");
        peer.set_write_timeout(Some(RESCUE_LIMIT))
            .expect("bounded Unix writer");
        peer.write_all(&prefix).expect("send real native prefix");
        let entered = before_lf_rx.recv_timeout(ENTRY_LIMIT);
        // Return both the receipt and a real postclock native LF write,
        // even if the native observer has already rejected the record.
        while Instant::now() < deadline {
            thread::yield_now();
        }
        let sent_at = Instant::now();
        let written = peer.write_all(b"\n");
        let _ = release_tx.send(());
        (entered, sent_at, written)
    });

    let mut paused_before_lf = false;
    let result = catch_unwind(AssertUnwindSafe(|| {
        await_spawned_gate_until(&listener, deadline, |stage| match stage {
            SpawnedGateStage::AfterAccept => {
                let _ = accepted_tx.send(Instant::now());
            }
            SpawnedGateStage::BeforeByte { received }
                if received == prefix_len && !paused_before_lf =>
            {
                paused_before_lf = true;
                let _ = before_lf_tx.send(Instant::now());
                let _ = release_rx.recv_timeout(RESCUE_LIMIT);
            }
            SpawnedGateStage::AfterByte { byte: b'\n' } => {
                let _ = lf_tx.send(Instant::now());
            }
            _ => {}
        })
    }));
    let (entered, sent_at, written) = writer.join().expect("join full native record writer");
    let accepted_at = accepted_rx.recv_timeout(RESCUE_LIMIT);
    let lf_observed_at = lf_rx.recv_timeout(RESCUE_LIMIT);
    assert!(
        accepted_at.is_ok_and(|time| time < deadline),
        "the real native accept must finish before the original deadline"
    );
    assert!(
        entered.is_ok_and(|time| time < deadline),
        "the real reader must reach the LF byte poll before the original deadline"
    );
    assert!(sent_at >= deadline, "LF must be written only after cutoff");
    assert!(written.is_ok(), "real Unix LF write must succeed");
    assert!(
        lf_observed_at.is_ok_and(|time| time >= deadline),
        "the native reader must physically consume LF after cutoff"
    );
    assert_expected_panic(
        result,
        "native pre-exec LF completed after original deadline",
    );
}

#[test]
fn timely_native_gate_record_retains_exact_positive_numeric_fields() {
    let (_directory, listener, address) = native_listener();
    // Only a synthetic numeric line; no process is created or signaled.
    let (expected_group, _, record) = native_record();
    write_native_record(&address, &record).expect("timely complete Unix PID record");
    let deadline = Instant::now() + Duration::from_secs(2);
    let (mut accepted_at, mut lf_at) = (None, None);
    let (peer, group, guardian) =
        await_spawned_gate_until(&listener, deadline, |stage| match stage {
            SpawnedGateStage::AfterAccept => accepted_at = Some(Instant::now()),
            SpawnedGateStage::AfterByte { byte: b'\n' } => lf_at = Some(Instant::now()),
            _ => {}
        });
    assert_eq!(group, expected_group);
    assert_eq!(guardian, FIXTURE_GUARDIAN);
    assert!(accepted_at.is_some_and(|time| time < deadline));
    assert!(lf_at.is_some_and(|time| time < deadline));
    drop(peer);
}

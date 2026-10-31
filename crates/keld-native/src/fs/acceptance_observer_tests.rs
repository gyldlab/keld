use super::{Hold, Snapshot, install_current_thread};
use crate::fs::{AFTER_WALK_TEST_HOOK, FS_TEST_COUNTERS, FsBroker, FsRequest, FsResponse};
use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::time::Duration;

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        Self(crate::fs::tests::race_owned_root("observer"))
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).expect("remove exact observer fixture");
    }
}

#[test]
fn occupied_hook_is_preserved_and_nested_scope_is_invalid() {
    let original = FS_TEST_COUNTERS.with(Cell::get);
    let ran = Rc::new(Cell::new(false));
    let flag = Rc::clone(&ran);
    AFTER_WALK_TEST_HOOK.with(|slot| {
        *slot.borrow_mut() = Some(Box::new(move || flag.set(true)));
    });
    let (returned, _) = mpsc::sync_channel(1);
    let (final_sink, _) = mpsc::sync_channel(1);
    assert!(install_current_thread(None, returned, final_sink).is_none());
    crate::fs::run_after_walk_test_hook();
    assert!(
        ran.get(),
        "rejected install must not take another private hook"
    );
    let (returned, _) = mpsc::sync_channel(1);
    let (final_sink, final_rows) = mpsc::sync_channel(1);
    let guard = install_current_thread(None, returned, final_sink).expect("first scope");
    let (returned, _) = mpsc::sync_channel(1);
    let (final_sink, _) = mpsc::sync_channel(1);
    assert!(install_current_thread(None, returned, final_sink).is_none());
    drop(guard);
    assert!(
        final_rows
            .recv_timeout(Duration::from_secs(2))
            .expect("original scope final")
            .fixture_fault
    );
    assert_eq!(FS_TEST_COUNTERS.with(Cell::get), original);
    assert!(AFTER_WALK_TEST_HOOK.with(|slot| slot.borrow().is_none()));
    let (returned, _) = mpsc::sync_channel(1);
    let (final_sink, final_rows) = mpsc::sync_channel(1);
    drop(install_current_thread(None, returned, final_sink).expect("reinstall after drop"));
    assert_eq!(
        final_rows
            .recv_timeout(Duration::from_secs(2))
            .expect("clean reinstallation"),
        Snapshot::default()
    );
}

#[test]
fn actual_native_calls_count_even_when_duplicate_bytes_match() {
    let fixture = Fixture::new();
    let verified = crate::fs::tests::test_verified_manifest(&fixture.0, &fixture.0);
    let broker = FsBroker::prepare(&verified).expect("real broker");
    let target = fixture.0.join("target");
    std::fs::write(&target, b"sentinel").expect("independent target");
    let (returned, returns) = mpsc::sync_channel(2);
    let (final_sink, final_rows) = mpsc::sync_channel(1);
    let guard = install_current_thread(None, returned, final_sink).expect("native scope");
    for expected in 1..=2 {
        assert!(matches!(
            broker
                .handle_request(
                    &verified,
                    keld_guard::Principal::AppProcess,
                    FsRequest::Write {
                        path: target.to_str().expect("fixture path").to_owned(),
                        bytes: b"same".to_vec()
                    },
                    &AtomicBool::new(false)
                )
                .expect("real write"),
            FsResponse::Write
        ));
        let row = returns
            .recv_timeout(Duration::from_secs(2))
            .expect("actual normal return");
        assert_eq!(row.request_entries, expected);
        assert_eq!(row.request_returns, expected);
        assert_eq!(row.walk_entries, expected);
        assert_eq!(row.target_opens, expected);
        assert_eq!(row.truncate_calls, expected);
        assert_eq!(row.content_writes, expected);
        assert!(!row.fixture_fault);
        assert_eq!(std::fs::read(&target).expect("independent bytes"), b"same");
    }
    drop(guard);
    assert_eq!(
        final_rows
            .recv_timeout(Duration::from_secs(2))
            .expect("after final native invocation")
            .request_entries,
        2
    );
}

#[test]
#[ignore = "explicit isolated macOS native lifetime qualification"]
fn hold_retains_pre_effect_state_then_returns_the_real_result() {
    child::run_and_assert_hold();
}

fn hold_schedule(fixture_root: &Path) {
    let verified = crate::fs::tests::test_verified_manifest(fixture_root, fixture_root);
    let target = fixture_root.join("target");
    std::fs::write(&target, b"sentinel").expect("seed target");
    let request = FsRequest::Write {
        path: target.to_str().expect("path").to_owned(),
        bytes: b"changed".to_vec(),
    };
    let (held, held_rows) = mpsc::sync_channel(1);
    let (release, released) = mpsc::sync_channel(1);
    let (returned, returns) = mpsc::sync_channel(1);
    let (final_sink, final_rows) = mpsc::sync_channel(1);
    let wait_control = Hold::new(held, released, Duration::from_secs(2)).expect("bounded hold");
    let worker = std::thread::spawn(move || {
        let broker = FsBroker::prepare(&verified).expect("real broker on worker");
        let guard =
            install_current_thread(Some(wait_control), returned, final_sink).expect("worker scope");
        let result = broker.handle_request(
            &verified,
            keld_guard::Principal::AppProcess,
            request,
            &AtomicBool::new(false),
        );
        drop(guard);
        result
    });
    let held = held_rows
        .recv_timeout(Duration::from_secs(2))
        .expect("actual retained after-walk");
    assert_eq!(
        (
            held.request_entries,
            held.request_returns,
            held.walk_entries,
            held.target_opens
        ),
        (1, 0, 1, 1)
    );
    assert_eq!(
        (held.create_calls, held.truncate_calls, held.content_writes),
        (0, 0, 0)
    );
    assert!(!held.fixture_fault);
    assert!(
        returns.try_recv().is_err(),
        "native return cannot precede release"
    );
    assert_eq!(
        std::fs::read(&target).expect("pre-effect OS bytes"),
        b"sentinel"
    );
    release.send(()).expect("release retained call");
    assert!(matches!(
        worker
            .join()
            .expect("worker exit")
            .expect("unchanged native result"),
        FsResponse::Write
    ));
    let terminal = returns
        .recv_timeout(Duration::from_secs(2))
        .expect("actual return");
    let final_row = final_rows
        .recv_timeout(Duration::from_secs(2))
        .expect("final scope fence after joined worker");
    assert_eq!(terminal, final_row);
    assert!(
        !final_row.fixture_fault,
        "release must precede the fixture watchdog"
    );
    assert_eq!(final_row.request_returns, 1);
    assert_eq!(final_row.content_writes, 1);
    assert_eq!(
        std::fs::read(&target).expect("post-effect OS bytes"),
        b"changed"
    );
    assert_next_native_read(fixture_root, &target);
}

fn assert_next_native_read(fixture_root: &Path, target: &Path) {
    // A distinct native invocation after the old worker/guard/broker have gone;
    // the held write's completion cannot establish healthy reuse.
    let fresh_verified = crate::fs::tests::test_verified_manifest(fixture_root, fixture_root);
    let fresh_broker = FsBroker::prepare(&fresh_verified).expect("fresh verified native broker");
    let (next_returned, next_returns) = mpsc::sync_channel(1);
    let (next_final_sink, next_final_rows) = mpsc::sync_channel(1);
    let next_guard = install_current_thread(None, next_returned, next_final_sink)
        .expect("separate fresh native observation scope");
    let next = fresh_broker
        .handle_request(
            &fresh_verified,
            keld_guard::Principal::AppProcess,
            FsRequest::Read {
                path: target.to_str().expect("fresh read path").to_owned(),
            },
            &AtomicBool::new(false),
        )
        .expect("next real native operation after held-worker cleanup");
    match next {
        FsResponse::Read { bytes } => assert_eq!(bytes, b"changed"),
        FsResponse::Write => panic!("fresh native read returned the wrong response variant"),
    }
    drop(next_guard);
    let next_return = next_returns
        .recv_timeout(Duration::from_secs(2))
        .expect("distinct next native return");
    assert_eq!(next_return.request_entries, 1);
    assert_eq!(next_return.request_returns, 1);
    assert_eq!(next_return.target_opens, 1);
    assert_eq!(next_return.content_reads, 2);
    assert_eq!(next_return.content_writes, 0);
    assert!(!next_return.fixture_fault);
    assert_eq!(
        next_final_rows
            .recv_timeout(Duration::from_secs(2))
            .expect("distinct next operation scope cleanup"),
        next_return
    );
    println!("KELD_NATIVE_OBSERVER_NEXT_OPERATION native_read=success bytes=6368616e676564");
}

#[derive(Clone, Copy)]
enum SinkFault {
    HeldDisconnected,
    ReleaseDisconnected,
    ReturnFull,
}

#[test]
fn held_sink_disconnect_is_the_only_fault() {
    assert_sink_fault(SinkFault::HeldDisconnected);
}

#[test]
fn release_disconnect_is_the_only_fault() {
    assert_sink_fault(SinkFault::ReleaseDisconnected);
}

#[test]
fn return_sink_full_is_the_only_fault() {
    assert_sink_fault(SinkFault::ReturnFull);
}

fn assert_sink_fault(fault: SinkFault) {
    let fixture = Fixture::new();
    let verified = crate::fs::tests::test_verified_manifest(&fixture.0, &fixture.0);
    let broker = FsBroker::prepare(&verified).expect("real broker");
    let target = fixture.0.join("target");
    let (held, held_rows) = mpsc::sync_channel(1);
    let held_rows = if matches!(fault, SinkFault::HeldDisconnected) {
        drop(held_rows);
        None
    } else {
        Some(held_rows)
    };
    let (release, released) = mpsc::sync_channel(1);
    if !matches!(fault, SinkFault::ReleaseDisconnected) {
        release.send(()).expect("healthy release prequeued");
    }
    drop(release);
    let return_capacity = usize::from(!matches!(fault, SinkFault::ReturnFull));
    let (returned, returns) = mpsc::sync_channel(return_capacity);
    let (final_sink, final_rows) = mpsc::sync_channel(1);
    let wait_control = Hold::new(held, released, Duration::from_secs(1)).expect("bounded hold");
    let guard = install_current_thread(Some(wait_control), returned, final_sink).expect("scope");
    assert!(matches!(
        broker
            .handle_request(
                &verified,
                keld_guard::Principal::AppProcess,
                FsRequest::Write {
                    path: target.to_str().expect("path").to_owned(),
                    bytes: b"real".to_vec()
                },
                &AtomicBool::new(false)
            )
            .expect("fixture failure cannot replace write"),
        FsResponse::Write
    ));
    if let Some(held_rows) = held_rows {
        assert!(
            !held_rows
                .recv_timeout(Duration::from_secs(2))
                .expect("healthy held sink")
                .fixture_fault
        );
    }
    if matches!(fault, SinkFault::ReturnFull) {
        assert!(
            returns.try_recv().is_err(),
            "full return sink admits no notification"
        );
    } else {
        assert!(
            returns
                .recv_timeout(Duration::from_secs(2))
                .expect("healthy return sink")
                .fixture_fault
        );
    }
    drop(guard);
    let row = final_rows
        .recv_timeout(Duration::from_secs(2))
        .expect("final invalid evidence");
    assert!(
        row.fixture_fault,
        "the single selected fault must invalidate evidence"
    );
    assert_eq!(
        (row.request_entries, row.request_returns, row.create_calls),
        (1, 1, 1)
    );
    assert_eq!(std::fs::read(&target).expect("real OS effect"), b"real");
}

#[test]
fn hold_bounds_reject_zero_and_excess_without_installation() {
    for timeout in [Duration::ZERO, Duration::from_secs(11)] {
        let (held, _) = mpsc::sync_channel(1);
        let (_, released) = mpsc::sync_channel(1);
        assert!(Hold::new(held, released, timeout).is_none());
    }
}

#[test]
fn unwind_is_not_a_native_normal_return() {
    let fixture = Fixture::new();
    let verified = crate::fs::tests::test_verified_manifest(&fixture.0, &fixture.0);
    let broker = FsBroker::prepare(&verified).expect("real broker");
    let (returned, returns) = mpsc::sync_channel(1);
    let (final_sink, final_rows) = mpsc::sync_channel(1);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _guard = install_current_thread(None, returned, final_sink).expect("scope");
        AFTER_WALK_TEST_HOOK.with(|slot| {
            *slot.borrow_mut() = Some(Box::new(|| panic!("private native-test unwind control")));
        });
        let _ = broker.handle_request(
            &verified,
            keld_guard::Principal::AppProcess,
            FsRequest::Write {
                path: fixture.0.join("target").to_str().expect("path").to_owned(),
                bytes: b"unused".to_vec(),
            },
            &AtomicBool::new(false),
        );
    }));
    assert!(result.is_err());
    assert!(returns.try_recv().is_err());
    let row = final_rows
        .recv_timeout(Duration::from_secs(2))
        .expect("unwind scope fence");
    assert_eq!(
        (row.request_entries, row.request_returns, row.content_writes),
        (1, 0, 0)
    );
    assert!(row.fixture_fault);
    assert!(!fixture.0.join("target").exists());
}

#[path = "acceptance_observer_child.rs"]
mod child;

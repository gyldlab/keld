//! KEL-140 AC7 occupied-ring replay through the existing public/native route.
//! The caller's one ring retains a real lifecycle event while its main realm
//! waits on an owned control pipe. No production or authority boundary changes.

use super::{
    GuardedTestRouter, LeaseCheckpoint, TestPrimaryOwnerCommand, WorkerLinkRole,
    assert_caller_report, assert_file_oracle, assert_in_flight_lease, assert_no_late_native_work,
    canonical_ring_bytes, exact_fit_content_length, fresh_public_fs_session_reads_its_write,
    report_value,
};
use keld_ipc::codec::encode;
use keld_ipc::{HEADER_LEN, LifecycleEvent};
use sha2::{Digest as _, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::io::Write as _;
use std::sync::mpsc;
use std::time::Duration;

fn pre_retirement_peer_loss(
    guardian: &mpsc::Receiver<TestPrimaryOwnerCommand>,
) -> mpsc::SyncSender<Result<(), String>> {
    let command = guardian
        .recv_timeout(Duration::from_secs(20))
        .expect("actual parked-peer close before host retirement or control release");
    let TestPrimaryOwnerCommand::FailGeneration(attempt, reply) = command else {
        panic!("occupied ring requested an unrelated lifecycle transition");
    };
    assert_eq!(attempt, 1, "actual old-generation peer loss");
    reply
}

fn assert_occupied_control_report(report: &BTreeMap<String, String>) {
    for (key, value) in [
        ("park-enter", "true"),
        ("release-count", "1"),
        ("release-byte", "1"),
        ("park-release", "true"),
        ("retained-events", "1"),
        ("event-before-release", "false"),
    ] {
        assert_eq!(report_value(report, key), Some(value), "{key}: {report:?}");
    }
}

fn assert_no_lifecycle_tail(
    guardian: &mpsc::Receiver<TestPrimaryOwnerCommand>,
    window: &mpsc::Receiver<super::super::AppWindowCommand>,
) {
    assert!(
        matches!(
            guardian.try_recv(),
            Err(mpsc::TryRecvError::Empty | mpsc::TryRecvError::Disconnected)
        ),
        "occupied overflow submitted another owner transition"
    );
    assert!(
        matches!(
            window.try_recv(),
            Err(mpsc::TryRecvError::Empty | mpsc::TryRecvError::Disconnected)
        ),
        "occupied overflow entered a UI Quit or fatal tail"
    );
}

#[test]
fn public_fs_native_read_exact_fit_overflows_occupied_ring_then_fresh_session_works() {
    let ring_bytes = canonical_ring_bytes();
    let content_length = exact_fit_content_length(ring_bytes);
    let event_bytes = HEADER_LEN
        + encode(&LifecycleEvent::LastWindowClosed)
            .expect("owning lifecycle event encoding")
            .len();
    assert!(
        event_bytes < ring_bytes,
        "lifecycle event independently fits"
    );
    let (parked_tx, parked) = mpsc::sync_channel(1);
    let (mut role, fixture) =
        WorkerLinkRole::start_public_fs_with_report("ring-occupied", "park-enter", parked_tx);
    let GuardedTestRouter {
        router,
        snapshot,
        allowed,
        taken,
        release,
        window,
        guardian,
        _temp: temp,
    } = fixture;
    let target = allowed.join("public-ring-occupied.bin");
    let unused = allowed.join("public-ring-occupied-unused.bin");
    let seeded = vec![0xa7; content_length];
    let digest = format!("{:x}", Sha256::digest(&seeded));
    fs::write(&target, seeded).expect("seed exact-fit native occupied-ring input");
    let mut control = role.supply_public_fs_targets_retained(&target, &unused);
    let (written_tx, written) = mpsc::sync_channel(1);
    let (resume, resume_rx) = mpsc::channel();
    snapshot.fs.hold_next_terminal_write(written_tx, resume_rx);
    router
        .handle()
        .signal_ready()
        .expect("actual public Ready before synchronous control");
    taken
        .recv_timeout(Duration::from_secs(5))
        .expect("public occupied Read reached the existing held worker");
    parked
        .recv_timeout(Duration::from_secs(5))
        .expect("live synchronous park marker before any event or native release");
    assert_in_flight_lease(&snapshot, LeaseCheckpoint::Held);
    assert_file_oracle(&target, content_length, &digest);

    router
        .handle()
        .signal_last_window_closed()
        .expect("queue actual lifecycle event before exact-fit native reply");
    release
        .send(())
        .expect("release real native occupied-ring Read");
    written
        .recv_timeout(Duration::from_secs(5))
        .expect("real occupied Read terminal write and correlation retirement");
    assert_in_flight_lease(&snapshot, LeaseCheckpoint::Replied);
    // No host close or unpark has happened: this request is the peer's actual
    // close after overflow, not a consequence of our later retirement.
    let peer_loss = pre_retirement_peer_loss(&guardian);
    router
        .handle()
        .retire_generation(1)
        .expect("retire old peer after actual close");
    peer_loss
        .send(Ok(()))
        .expect("acknowledge the existing Core peer-loss owner");
    control
        .write_all(&[1])
        .expect("release exact occupied fixture control byte");
    control.flush().expect("flush occupied control release");
    drop(control);
    let report = role.finish();
    // The existing helper's true branch selects the canonical overflow result;
    // this input still has the exact empty-ring fit length, not one extra byte.
    assert_caller_report(&report, true, content_length, &digest);
    assert_occupied_control_report(&report);
    assert_in_flight_lease(&snapshot, LeaseCheckpoint::Replied);

    resume
        .send(())
        .expect("resume occupied native terminal fence after caller settlement");
    snapshot
        .fs
        .drain()
        .expect("observe old occupied native lease drain");
    assert_in_flight_lease(&snapshot, LeaseCheckpoint::Drained);
    router
        .shutdown()
        .expect("join occupied old readers and native worker");
    assert_no_lifecycle_tail(&guardian, &window);
    assert_no_late_native_work(&taken, &target, &unused, content_length, &digest);
    let root = temp.path().to_path_buf();
    drop(snapshot);
    drop(temp);
    assert!(
        !root.exists(),
        "occupied fixture directory survived owned teardown"
    );
    fresh_public_fs_session_reads_its_write();
    eprintln!(
        "KELD_AC7_NATIVE_OCCUPIED_RING ring_bytes={ring_bytes} event_bytes={event_bytes} content_bytes={content_length} pre_retirement_peer_loss=true retained_events=1 settlements=1 old_router_joined=true fresh_session=true shipping_recovery_acceptance=false full_broker_census=false"
    );
}

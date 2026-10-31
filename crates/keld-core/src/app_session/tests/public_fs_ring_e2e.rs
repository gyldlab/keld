//! KEL-140 AC7: real native Read replies meet the public caller's default ring.
//! No boundary change. This is a guarded-router mechanism, not no-flag product
//! recovery, native syscall preemption, or an independent broker-handle census.

use super::{
    GuardSnapshot, GuardedTestRouter, TestPrimaryOwnerCommand, WorkerLinkRole,
    fresh_public_fs_session_reads_its_write, report_value,
};
use keld_ipc::codec::encode;
use keld_native::fs::{FsResponse, MAX_FS_CONTENT_BYTES};
use sha2::{Digest as _, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

#[path = "public_fs_ring_occupied_e2e.rs"]
mod public_fs_ring_occupied_e2e;

/// Constants come from the actual transport export; the native codec owns size.
/// The separate captured process opens no app-link and uses the existing child
/// custody/exit/report owner. The public application imports only its API index.
fn canonical_ring_bytes() -> usize {
    let mut command = Command::new("bun");
    command
        .current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
        .env_remove("KELD_APP_LINK")
        .env_remove("KELD_KIPC_TEST_HOOKS")
        .arg("--eval")
        .arg(concat!(
            "import { DEFAULT_RING_BYTES, HEADER_LEN } ",
            "from './packages/@keld/kipc/src/transport.ts';",
            "console.log('KELD_WL ring-bytes=' + DEFAULT_RING_BYTES);",
            "console.log('KELD_WL header-length=' + HEADER_LEN);",
        ));
    let report = WorkerLinkRole::spawn_captured(command).finish();
    assert_eq!(report.len(), 2, "one canonical sizing report: {report:?}");
    let header: usize = report_value(&report, "header-length")
        .expect("transport header length")
        .parse()
        .expect("numeric transport header length");
    assert_eq!(header, keld_ipc::HEADER_LEN, "Rust/TS header agreement");
    let ring: usize = report_value(&report, "ring-bytes")
        .expect("transport ring capacity")
        .parse()
        .expect("numeric transport ring capacity");
    // Independent GH-527 §4.7 contract oracle: deriving fixture volume alone
    // would silently follow a changed default and miss a queue enlargement.
    assert_eq!(ring, 1 << 20, "approved default is one MiB");
    ring
}

fn read_envelope_length(content_length: usize) -> usize {
    keld_ipc::HEADER_LEN
        + encode(&FsResponse::Read {
            bytes: vec![0; content_length],
        })
        .expect("owning native Read encoding")
        .len()
}

fn exact_fit_content_length(ring_bytes: usize) -> usize {
    let mut length = ring_bytes
        .checked_sub(keld_ipc::HEADER_LEN)
        .expect("ring holds a header");
    while read_envelope_length(length) > ring_bytes {
        length = length.checked_sub(1).expect("Read envelope can fit");
    }
    assert_eq!(
        read_envelope_length(length),
        ring_bytes,
        "exact-fit fixture"
    );
    assert_eq!(
        read_envelope_length(length + 1),
        ring_bytes + 1,
        "independent adjacent one-byte-over fixture"
    );
    assert!(
        length < MAX_FS_CONTENT_BYTES,
        "native content and its adjacent byte are admissible"
    );
    length
}

fn assert_file_oracle(path: &Path, length: usize, digest: &str) {
    let bytes = fs::read(path).expect("independent native file oracle");
    assert_eq!(bytes.len(), length, "native file length changed");
    assert_eq!(
        format!("{:x}", Sha256::digest(&bytes)),
        digest,
        "native file bytes changed"
    );
}

fn assert_caller_report(
    report: &BTreeMap<String, String>,
    one_byte_over: bool,
    content_length: usize,
    digest: &str,
) {
    assert_eq!(report_value(report, "settlements"), Some("1"), "{report:?}");
    if one_byte_over {
        assert_eq!(
            report_value(report, "call-code"),
            Some("KELD-IPC-026"),
            "{report:?}"
        );
        assert_eq!(
            report_value(report, "call-returned"),
            Some("false"),
            "{report:?}"
        );
        assert_eq!(report_value(report, "read-length"), None, "{report:?}");
        assert_eq!(report_value(report, "read-sha256"), None, "{report:?}");
    } else {
        assert_eq!(report_value(report, "call-code"), None, "{report:?}");
        assert_eq!(
            report_value(report, "call-returned"),
            Some("true"),
            "{report:?}"
        );
        let expected_length = content_length.to_string();
        assert_eq!(
            report_value(report, "read-length"),
            Some(expected_length.as_str()),
            "{report:?}"
        );
        assert_eq!(
            report_value(report, "read-sha256"),
            Some(digest),
            "{report:?}"
        );
    }
}

fn acknowledge_link_loss(guardian: &mpsc::Receiver<TestPrimaryOwnerCommand>) -> usize {
    let mut failures = 0;
    loop {
        // Reuse the authenticated-role fixture's existing twenty-second
        // observation bound as a kill switch, never as ordering evidence.
        let command = match guardian.recv_timeout(Duration::from_secs(20)) {
            Ok(command) => command,
            Err(mpsc::RecvTimeoutError::Disconnected) => return failures,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                panic!("old router did not release its Core owner channel");
            }
        };
        match command {
            TestPrimaryOwnerCommand::FailGeneration(attempt, reply) => {
                assert_eq!(attempt, 1, "exact old generation link loss");
                failures += 1;
                assert_eq!(failures, 1, "one old-generation link-loss request");
                reply
                    .send(Ok(()))
                    .expect("acknowledge existing Core link loss");
            }
            _ => panic!("ring read requested an unexpected lifecycle transition"),
        }
    }
}

#[derive(Clone, Copy)]
enum LeaseCheckpoint {
    Held,
    Replied,
    Drained,
}

fn assert_in_flight_lease(snapshot: &GuardSnapshot, checkpoint: LeaseCheckpoint) {
    let (expected, lock_phase, contract) = match checkpoint {
        LeaseCheckpoint::Held => (
            1,
            "held native state",
            Some("public admission owns a lease before native release"),
        ),
        LeaseCheckpoint::Replied => (
            1,
            "replied native state",
            Some("reply publication is independent of owner drain"),
        ),
        LeaseCheckpoint::Drained => (0, "drained native state", None),
    };
    let actual = snapshot.fs.state.lock().expect(lock_phase).in_flight;
    if let Some(contract) = contract {
        assert_eq!(actual, expected, "{contract}");
    } else {
        assert_eq!(actual, expected);
    }
}

fn assert_no_late_native_work(
    taken: &mpsc::Receiver<()>,
    target: &Path,
    unused: &Path,
    content_length: usize,
    digest: &str,
) {
    assert!(
        matches!(
            taken.try_recv(),
            Err(mpsc::TryRecvError::Empty | mpsc::TryRecvError::Disconnected)
        ),
        "the public ring caller submitted an extra native operation"
    );
    assert_file_oracle(target, content_length, digest);
    assert!(
        matches!(fs::symlink_metadata(unused), Err(error) if error.kind() == std::io::ErrorKind::NotFound),
        "the ring scenario acquired an unrelated effect"
    );
}

#[test]
fn public_fs_native_read_exact_ring_fit_returns_seeded_bytes() {
    native_read_at_ring_boundary(false);
}

#[test]
fn public_fs_native_read_one_byte_over_ring_rejects_once_then_fresh_session_works() {
    native_read_at_ring_boundary(true);
}

fn native_read_at_ring_boundary(one_byte_over: bool) {
    let ring_bytes = canonical_ring_bytes();
    let content_length = exact_fit_content_length(ring_bytes) + usize::from(one_byte_over);
    let scenario = if one_byte_over {
        "ring-over"
    } else {
        "ring-fit"
    };
    let (mut role, fixture) = WorkerLinkRole::start_public_fs_scenario(scenario);
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
    let target = allowed.join("public-ring-read.bin");
    let unused = allowed.join("public-ring-unused.bin");
    let seeded: Vec<u8> = (0..content_length)
        .map(|index| u8::try_from(index % 251).expect("bounded independent byte pattern"))
        .collect();
    let digest = format!("{:x}", Sha256::digest(&seeded));
    fs::write(&target, &seeded).expect("seed independently known native file");
    role.supply_public_fs_targets(&target, &unused);

    // IPC026 may close the peer before the test retires it. Service that real
    // existing Core owner request, without installing a successor or inventing
    // a supervisor outcome. Every other command remains a fixture failure.
    let owner = thread::spawn(move || acknowledge_link_loss(&guardian));
    let (written_tx, written) = mpsc::sync_channel(1);
    let (resume, resume_rx) = mpsc::channel();
    snapshot.fs.hold_next_terminal_write(written_tx, resume_rx);
    router
        .handle()
        .signal_ready()
        .expect("actual guarded-router Ready");
    taken
        .recv_timeout(Duration::from_secs(5))
        .expect("public Read reached the owning held worker");
    assert_in_flight_lease(&snapshot, LeaseCheckpoint::Held);
    assert_file_oracle(&target, content_length, &digest);
    release.send(()).expect("release the real native Read");
    written
        .recv_timeout(Duration::from_secs(5))
        .expect("real native Read REPLY and correlation retirement");
    assert_in_flight_lease(&snapshot, LeaseCheckpoint::Replied);
    router
        .handle()
        .retire_generation(1)
        .expect("retire replied old link");
    resume
        .send(())
        .expect("resume original terminal-write fence");
    snapshot
        .fs
        .drain()
        .expect("observe the real old lease drain");
    assert_in_flight_lease(&snapshot, LeaseCheckpoint::Drained);
    let report = role.finish();
    assert_caller_report(&report, one_byte_over, content_length, &digest);
    router
        .shutdown()
        .expect("join old router readers and native worker");
    let failures = owner
        .join()
        .expect("join actual Core owner acknowledgements");
    assert!(
        failures <= 1,
        "old link loss is not a successor qualification"
    );
    assert!(
        matches!(
            window.try_recv(),
            Err(mpsc::TryRecvError::Empty | mpsc::TryRecvError::Disconnected)
        ),
        "ring read must not enter a UI Quit or fatal tail"
    );
    assert_no_late_native_work(&taken, &target, &unused, content_length, &digest);
    let root = temp.path().to_path_buf();
    drop(snapshot);
    drop(temp);
    assert!(
        !root.exists(),
        "old fixture directory survived owned teardown"
    );
    fresh_public_fs_session_reads_its_write();
    eprintln!(
        "KELD_AC7_NATIVE_RING scenario={scenario} ring_bytes={ring_bytes} content_bytes={content_length} envelope_bytes={} settlements=1 old_router_joined=true fresh_session=true occupied_ring_acceptance=false full_broker_census=false",
        read_envelope_length(content_length),
    );
}

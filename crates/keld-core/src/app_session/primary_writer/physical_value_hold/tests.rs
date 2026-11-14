//! File/clock adversaries for the test control; these are not native acceptance.
#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "Owned test fixture setup and assertion failures terminate this test."
)]

use super::*;

fn directory() -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix("keld-x10-value-")
        .tempdir()
        .expect("owned control directory")
}

#[test]
fn physical_candidate_requires_actual_value_event_channel_and_zero_correlation() {
    let original = FrameHeader {
        kind: FrameKind::Event,
        flags: 0,
        channel: keld_ipc::channel_table::COMPAT_CONTROL.id(),
        corr: CorrelationId(0),
        len: 11,
    };
    let position = Position {
        index: 0,
        serial: 19,
    };
    assert!(candidate(position, &original));
    assert!(!candidate(
        Position {
            index: 1,
            ..position
        },
        &original
    ));
    assert!(!candidate(
        position,
        &FrameHeader {
            kind: FrameKind::Reply,
            ..original
        }
    ));
    assert!(!candidate(
        position,
        &FrameHeader {
            channel: keld_ipc::channel_table::ECHO.id(),
            ..original
        }
    ));
    assert!(!candidate(
        position,
        &FrameHeader {
            corr: CorrelationId(1),
            ..original
        }
    ));
}

#[test]
fn original_packet_deadline_refuses_even_an_existing_release() {
    let directory = directory();
    std::fs::write(directory.path().join("arm"), b"armed").expect("arm");
    std::fs::write(directory.path().join("release"), b"released").expect("release");
    let hold = PhysicalValueHold {
        directory: directory.path().to_owned(),
        selected: true,
    };
    let deadline = Instant::now();
    assert!(hold.released(deadline, deadline).is_err());
    assert!(
        hold.released(deadline, deadline + Duration::from_nanos(1))
            .is_err()
    );
    assert!(
        hold.released(deadline + Duration::from_secs(1), deadline)
            .expect("within original deadline")
    );
}

#[test]
fn release_requires_the_same_real_armed_files_and_refuses_nonfiles() {
    let directory = directory();
    let hold = PhysicalValueHold {
        directory: directory.path().to_owned(),
        selected: true,
    };
    let now = Instant::now();
    let deadline = now + Duration::from_secs(1);
    std::fs::write(directory.path().join("release"), b"early").expect("early release");
    assert!(hold.released(deadline, now).is_err());
    std::fs::write(directory.path().join("arm"), b"armed").expect("arm");
    assert!(hold.released(deadline, now).expect("same actual arm"));
    std::fs::remove_file(directory.path().join("release")).expect("remove fixture marker");
    std::fs::create_dir(directory.path().join("release")).expect("nonfile adversary");
    assert!(hold.released(deadline, now).is_err());
}

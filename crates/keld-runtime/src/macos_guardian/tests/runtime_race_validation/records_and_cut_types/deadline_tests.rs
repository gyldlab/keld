//! Native KGC1 complete-record timing tests for the private race relay.
use super::super::{runtime_io_kind, write_generation_control};
use super::{NativeRecordStage, receive_record_until, timed_pair};
use crate::macos_guardian::{CONTROL_PREPARE, GenerationControlRecord};
use std::io;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

#[test]
fn actual_native_full_record_after_original_poll_entry_is_not_timely() {
    let (mut receiver, mut sender) = timed_pair();
    let deadline = Instant::now() + Duration::from_millis(250);
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let (native_tx, native_rx) = mpsc::channel();
    let controller = thread::spawn(move || {
        let entered = entered_rx.recv_timeout(Duration::from_millis(200));
        while Instant::now() < deadline {
            thread::yield_now();
        }
        let delivery_at = Instant::now();
        let sent = write_generation_control(
            &mut sender,
            &GenerationControlRecord::new(CONTROL_PREPARE, 3),
        );
        let _ = release_tx.send(());
        (entered, delivery_at, sent)
    });

    let mut paused = false;
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        receive_record_until(&mut receiver, deadline, |stage| match stage {
            NativeRecordStage::BeforePoll if !paused => {
                paused = true;
                let _ = entered_tx.send(Instant::now());
                let _ = release_rx.recv_timeout(Duration::from_secs(1));
            }
            NativeRecordStage::AfterCompleteRecord => {
                let _ = native_tx.send(Instant::now());
            }
            NativeRecordStage::BeforePoll => {}
        })
    }));
    let (entered, delivery_at, sent) = controller.join().expect("join real Unix record writer");
    let after_native = native_rx.recv_timeout(Duration::from_secs(1));

    assert!(
        entered.is_ok_and(|at| at < deadline),
        "real pre-poll stage must be reached before the original clock"
    );
    assert!(
        delivery_at >= deadline,
        "writer only delivers after deadline"
    );
    assert!(
        sent.is_ok(),
        "real native complete KGC1 record was delivered"
    );
    assert!(
        after_native.is_ok_and(|at| at >= deadline),
        "reader must consume the actual full KGC1 record after deadline"
    );
    let error = result
        .expect("native record reader must not panic")
        .expect_err("late native complete record must fail closed");
    assert_eq!(
        runtime_io_kind(&error),
        Some(io::ErrorKind::TimedOut),
        "late record must retain the original typed lifecycle timeout"
    );
    assert!(error.to_string().contains("test Runtime record collection"));
}

#[test]
fn actual_timely_private_record_retains_original_fields() {
    let (mut receiver, mut sender) = timed_pair();
    write_generation_control(
        &mut sender,
        &GenerationControlRecord::new(CONTROL_PREPARE, 7),
    )
    .expect("real native timely control record");
    let record = receive_record_until(
        &mut receiver,
        Instant::now() + Duration::from_secs(1),
        |_| {},
    )
    .expect("complete record in original clock");
    assert_eq!(record.kind, CONTROL_PREPARE);
    assert_eq!(record.attempt, 7);
    assert_eq!(record.payload, b"", "a PREPARE record carries no payload");
}

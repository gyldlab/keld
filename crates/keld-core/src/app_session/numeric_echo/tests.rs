//! Pure controls of the same production storage phases. Native token minting,
//! lease validity and UI completion are separately exercised by Wv's owner tests.
use super::*;
use crate::app_session::PrimaryLinkIdentity;
use std::sync::atomic::{AtomicBool, AtomicUsize};

struct DropStorage(Arc<AtomicUsize>);
impl Drop for DropStorage {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}
fn stored() -> (State<DropStorage>, Arc<AtomicUsize>) {
    let drops = Arc::new(AtomicUsize::new(0));
    (State::prepared(DropStorage(Arc::clone(&drops))), drops)
}
fn ready_reply() -> (State<DropStorage>, Arc<AtomicUsize>, *const u8) {
    let (mut state, drops) = stored();
    state.admitted();
    assert!(state.begin_read());
    let bytes = vec![17, 23, 29];
    let address = bytes.as_ptr();
    assert!(state.finish(Body::Reply(bytes)).is_ok());
    (state, drops, address)
}

#[test]
fn retirement_cannot_release_storage_while_the_actual_read_is_live() {
    let (mut state, drops) = stored();
    state.admitted();
    assert!(state.begin_read());
    state.retire("cancelled A");
    state.materialize();
    assert!(!state.is_ready());
    assert!(state.take_ready().is_none());
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    assert!(state.finish(Body::Reply(vec![1, 2, 3])).is_ok());
    assert_eq!(state.ready_failure(), Some("cancelled A"));
    let (result, storage) = state.take_ready().expect("actual read returned");
    assert!(matches!(result, Err("cancelled A")));
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    drop(result);
    drop(storage);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
}

#[test]
fn aborted_body_scope_retains_storage_until_ordinary_progress_materializes_it() {
    let (mut state, drops) = stored();
    state.admitted();
    assert!(state.begin_read());
    state.read_ended("raw body ended");
    assert!(!state.is_ready());
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    state.materialize();
    assert_eq!(state.ready_failure(), Some("raw body ended"));
    drop(state);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
}

#[test]
fn failed_actual_notice_keeps_exact_result_buffer_and_storage() {
    let (mut state, drops, address) = ready_reply();
    let (tx, rx) = mpsc::sync_channel(1);
    drop(rx);
    state.send_notice(&tx);
    assert!(!state.needs_notice());
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    let (body, storage) = state
        .take_ready()
        .expect("closed notice did not take result");
    let Ok(Body::Reply(bytes)) = body else {
        panic!("original result lost")
    };
    assert_eq!(bytes.as_ptr(), address);
    assert_eq!(bytes, [17, 23, 29]);
    drop(bytes);
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    drop(storage);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
}

#[test]
fn full_notice_retries_only_the_unit_after_the_real_slot_is_released() {
    let (mut state, drops, address) = ready_reply();
    let (tx, rx) = mpsc::sync_channel(1);
    tx.try_send(Ok(PrimaryEchoReply::NumericReady)).unwrap();
    state.send_notice(&tx);
    assert!(state.needs_notice());
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    assert!(matches!(
        rx.try_recv(),
        Ok(Ok(PrimaryEchoReply::NumericReady))
    ));
    state.send_notice(&tx);
    assert!(!state.needs_notice());
    assert!(matches!(
        rx.try_recv(),
        Ok(Ok(PrimaryEchoReply::NumericReady))
    ));
    let (body, storage) = state.take_ready().unwrap();
    let Ok(Body::Reply(bytes)) = body else {
        panic!("wrong result")
    };
    assert_eq!(bytes.as_ptr(), address);
    drop(bytes);
    drop(storage);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
}

#[test]
fn prepared_source_is_not_authority_to_allocate_a_reply_body() {
    let (mut state, drops) = stored();
    assert!(!state.begin_read());
    state.retire("not admitted");
    state.materialize();
    assert_eq!(state.ready_failure(), Some("not admitted"));
    assert!(!state.begin_read());
    drop(state);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
}

#[test]
fn one_read_and_one_result_take_preserve_the_original_cancellation_winner() {
    let (mut state, drops) = stored();
    state.admitted();
    assert!(state.begin_read());
    assert!(!state.begin_read());
    state.retire("first cancellation");
    state.retire("later cancellation");
    state.read_ended("reader ended");
    state.materialize();
    assert_eq!(state.ready_failure(), Some("first cancellation"));
    let result = state.take_ready().unwrap();
    assert!(state.take_ready().is_none());
    assert!(state.storage().is_none());
    drop(result);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
}

#[test]
fn retirement_after_settlement_does_not_rewrite_the_actual_result() {
    let (mut state, drops, address) = ready_reply();
    state.retire("late role loss");
    state.materialize();
    let (body, storage) = state.take_ready().unwrap();
    let Ok(Body::Reply(bytes)) = body else {
        panic!("settled result was rewritten")
    };
    assert_eq!(bytes.as_ptr(), address);
    drop(bytes);
    drop(storage);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
}

#[test]
fn failed_coalesced_progress_keeps_actual_ready_storage_and_refuses_notice_or_take() {
    let (mut state, drops, address) = ready_reply();
    state.record_progress_failure(ProgressFailure::MissingWriter);
    let (tx, rx) = mpsc::sync_channel(1);
    state.send_notice(&tx);
    assert!(matches!(rx.try_recv(), Err(mpsc::TryRecvError::Empty)));
    assert!(state.take_ready().is_none());
    assert!(state.progress_failure().is_some());
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    // The actual original byte vector is still in the same retained record.
    assert_eq!(state.reply_address_for_test(), Some(address));
    drop(state);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
}

#[test]
fn canonical_reader_refuses_equal_number_foreign_allocation_and_wrong_pending_correlation_before_body()
 {
    use std::io::Cursor;
    let a = PrimaryCallerContext {
        attempt: 7,
        identity: PrimaryLinkIdentity::Fixture,
        reader_stop: Arc::new(AtomicBool::new(false)),
    };
    let b = PrimaryCallerContext {
        attempt: 7,
        identity: PrimaryLinkIdentity::Fixture,
        reader_stop: Arc::new(AtomicBool::new(false)),
    };
    for (incoming, pending_corr, accepted) in [
        (&a, CorrelationId(9), true),
        (&b, CorrelationId(9), false),
        (&a, CorrelationId(8), false),
    ] {
        let (mut state, drops) = stored();
        state.admitted();
        let mut wire = Vec::new();
        keld_ipc::link::write_frame(
            &mut wire,
            FrameKind::Reply,
            0,
            ECHO_CHANNEL,
            CorrelationId(9),
            &[3, 5, 7],
        )
        .unwrap();
        let mut stream = Cursor::new(wire);
        let result = keld_ipc::link::read_primary_app_frame_interruptible_with_payload_admission(
            &mut stream,
            &AtomicBool::new(false),
            || Some(CorrelationId(9)),
            &[],
            |_, _| keld_ipc::link::GuardedCallAdmission::RejectSession,
            |header| {
                read::admit_record(
                    (&a, pending_corr),
                    (incoming, header),
                    &mut state,
                    4096,
                    true,
                )
            },
        );
        assert_eq!(result.is_ok(), accepted);
        assert_eq!(
            stream.position(),
            u64::try_from(keld_ipc::HEADER_LEN + if accepted { 3 } else { 0 }).unwrap()
        );
        assert_eq!(drops.load(Ordering::SeqCst), 0);
        if !accepted {
            assert!(
                state.begin_read(),
                "a rejected source cannot consume A's read"
            );
        }
        drop(state);
        assert_eq!(drops.load(Ordering::SeqCst), 1);
    }
}

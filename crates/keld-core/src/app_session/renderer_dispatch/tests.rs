//! These exercise the exact retained runner/channel helpers with non-clone
//! storage. They do not stand in for actual native or producer-drain receipts.
use super::*;
use std::sync::atomic::AtomicUsize;

struct Owned {
    bytes: Vec<u8>,
    drops: Arc<AtomicUsize>,
}
impl Drop for Owned {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::SeqCst);
    }
}
fn owned() -> (Owned, Arc<AtomicUsize>) {
    let drops = Arc::new(AtomicUsize::new(0));
    (
        Owned {
            bytes: vec![11, 13, 17],
            drops: Arc::clone(&drops),
        },
        drops,
    )
}

#[test]
fn processing_error_returns_the_same_owned_input_without_drop() {
    let (value, drops) = owned();
    let pointer = value.bytes.as_ptr();
    let source = Mutex::new(Some(value));
    let returned = retained::run(&source, |value| {
        assert_eq!(value.bytes.as_ptr(), pointer);
        Err::<(), _>("actual processing failure")
    });
    assert!(matches!(
        returned.result,
        Err(RunFailure::Returned("actual processing failure"))
    ));
    assert!(source.lock().unwrap().is_none());
    assert_eq!(returned.input.as_ref().unwrap().bytes.as_ptr(), pointer);
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    drop(returned);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
}

#[test]
fn processing_unwind_retains_actual_partial_state_in_the_join_result() {
    let (value, drops) = owned();
    let pointer = value.bytes.as_ptr();
    let source = Mutex::new(Some(value));
    let returned = retained::run(&source, |value| {
        assert_eq!(value.bytes.as_ptr(), pointer);
        value.bytes[0] = 19;
        panic!("bounded owned processing fault");
        #[allow(unreachable_code)]
        Ok::<(), ()>(())
    });
    assert!(matches!(returned.result, Err(RunFailure::Panicked)));
    assert_eq!(returned.input.as_ref().unwrap().bytes, [19, 13, 17]);
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    drop(returned);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
}

#[test]
fn busy_before_entry_keeps_original_input_attached() {
    let (value, drops) = owned();
    let source = Mutex::new(Some(value));
    let guard = source.lock().unwrap();
    let returned = retained::run(&source, |_| -> Result<(), ()> {
        panic!("must not enter while source is busy")
    });
    assert!(matches!(returned.result, Err(RunFailure::Busy)));
    assert!(returned.input.is_none());
    assert!(guard.is_some());
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    drop(guard);
    drop(source);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
}

#[test]
fn actual_failed_channel_send_restores_the_same_delivery() {
    let (value, drops) = owned();
    let pointer = value.bytes.as_ptr();
    let mut source = Some(value);
    let (sender, receiver) = mpsc::channel();
    drop(receiver);
    assert_eq!(retained::send(&mut source, &sender), Err(()));
    assert_eq!(source.as_ref().unwrap().bytes.as_ptr(), pointer);
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    drop(source);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
}

#[test]
fn actual_channel_success_is_only_handoff_and_receiver_keeps_the_original() {
    let (value, drops) = owned();
    let pointer = value.bytes.as_ptr();
    let mut source = Some(value);
    let (sender, receiver) = mpsc::channel();
    assert_eq!(retained::send(&mut source, &sender), Ok(()));
    assert!(source.is_none());
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    let delivery = receiver.recv().unwrap();
    assert_eq!(delivery.bytes.as_ptr(), pointer);
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    drop(delivery);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
}

#[test]
fn retired_source_poll_keeps_actual_input_and_cause_until_refused_drain() {
    let (value, drops) = owned();
    let pointer = value.bytes.as_ptr();
    let source = Mutex::new(Some(value));
    let returned = retained::run(&source, |_| {
        Err(DispatchFailure::from_source(
            app_detail("old source witness", "exact retained A cause"),
            compat_router::RendererFailureDisposition::RetiredSource,
        ))
    });
    let (input, mut completion) = returned_completion(returned);
    assert_eq!(input.as_ref().unwrap().bytes.as_ptr(), pointer);
    completion
        .poll_failure()
        .expect("old A is not a global failure");
    completion
        .poll_failure()
        .expect("a later poll cannot reclassify A");
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    let refusal = completion
        .retained_refusal()
        .expect("actual drain must refuse");
    assert!(refusal.to_string().contains("exact retained A cause"));
    assert!(matches!(completion, Completion::RetiredSource(None)));
    assert!(
        completion.retained_refusal().is_some(),
        "cause transfer is not drain"
    );
    drop(input);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
}

#[test]
fn exact_fatal_disposition_returns_original_cause_without_consuming_input() {
    let (value, drops) = owned();
    let source = Mutex::new(Some(value));
    let returned = retained::run(&source, |_| {
        Err(DispatchFailure::from_source(
            app_detail("fenced source witness", "already queued exact A Fatal"),
            compat_router::RendererFailureDisposition::FatalQueued,
        ))
    });
    let (input, mut completion) = returned_completion(returned);
    let cause = completion.poll_failure().expect_err("existing exact fatal");
    assert!(cause.to_string().contains("already queued exact A Fatal"));
    assert!(input.is_some());
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    assert!(matches!(completion, Completion::Failed(None)));
    drop(input);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
}

#[test]
fn caught_input_unwind_is_retained_without_inventing_current_caller_failure() {
    let (value, drops) = owned();
    let source = Mutex::new(Some(value));
    let returned = retained::run(&source, |_| -> Result<(), DispatchFailure> {
        panic!("bounded in-request unwind")
    });
    let (input, mut completion) = returned_completion(returned);
    completion
        .poll_failure()
        .expect("unwind grants no source authority");
    assert!(matches!(completion, Completion::RetiredSource(Some(_))));
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    assert!(completion.retained_refusal().is_some());
    drop(input);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
}

#[test]
fn actual_drain_returns_original_retired_cause_before_blocked_protocol_deadline() {
    use crate::app_session::compat_router::owned_tests::fixture;
    use keld_wv::renderer_bridge::RendererBridgeServiceTurn;
    fixture(|router, caller, _, _, _, _, commands| {
        let owner = router.compat.as_ref().expect("existing Compat owner");
        let _control = owner
            .lock()
            .expect("actual ledger")
            .reserve_native_control(caller, 1, 1025, Arc::clone(owner))
            .expect("same real unsettled CONTROL record");
        assert!(
            !router
                .compat_endpoint_is_quiescent()
                .expect("actual protocol predicate")
        );

        let (_requests, receiver) = mpsc::channel();
        let (outcomes, _received) = mpsc::channel();
        let mut dispatch = RendererDispatch::prepare(receiver, outcomes, router.clone());
        let returned = retained::run(&dispatch.prepared, |_| {
            Err(DispatchFailure::RetiredSource(app_detail(
                "retired service source witness",
                "original A retained cause before timeout",
            )))
        });
        let (input, completion) = returned_completion(returned);
        dispatch.worker = Worker::Finished { input, completion };
        // An already elapsed original clock makes the historical wrong path
        // discriminate immediately, without waiting or renewing a test deadline.
        let deadline = Instant::now();
        dispatch.drain_deadline = Some(deadline);
        assert_eq!(
            dispatch
                .service(router, RendererBridgeServiceTurn::Poll)
                .unwrap(),
            None
        );
        assert!(
            commands.try_recv().is_err(),
            "Poll must not send global Fatal"
        );
        let error = dispatch
            .service(router, RendererBridgeServiceTurn::Drain)
            .expect_err("actual stopped source cannot drain");
        assert!(
            error
                .to_string()
                .contains("original A retained cause before timeout"),
            "{error}"
        );
        assert!(!error.to_string().contains(
            "actual Core protocol or numeric source remains retained at the original deadline"
        ));
        assert_eq!(dispatch.drain_deadline, Some(deadline));
        assert!(matches!(
            dispatch.worker,
            Worker::Finished {
                input: Some(_),
                completion: Completion::RetiredSource(None)
            }
        ));
        assert!(
            !dispatch.release_ready().unwrap(),
            "cause transfer grants no release"
        );
        assert!(
            !router.compat_endpoint_is_quiescent().unwrap(),
            "original protocol record is still held"
        );
    });
}

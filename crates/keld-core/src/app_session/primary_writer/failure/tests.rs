//! Real pipe failures plus the exact existing `DirectPrimaryOwner` decision seam.
#![allow(clippy::expect_used, clippy::panic)]

use super::super::{
    AppWindowCommand, ControlMessage, CorrelationId, HostAppError, PrimaryCallerContext,
    PrimaryWriterWorker,
};
use super::*;

mod bound;
mod observation;
mod reader;
use crate::app_session as app;
use keld_ipc::AppLinkDeadlines as _;
use std::cell::Cell;
use std::sync::atomic::Ordering;
use std::sync::mpsc::Receiver;
use std::thread;
use std::time::Duration;

struct Fixture {
    router: app::PrimaryRouterHandle,
    peer: keld_ipc::BootstrapStream,
    owner: Receiver<app::DirectPrimaryOwnerCommand>,
    windows: Receiver<AppWindowCommand>,
    worker: PrimaryWriterWorker,
    target: Arc<WriterLink>,
}
impl Fixture {
    fn new() -> Self {
        let (mut router, peer, owner, windows) =
            app::tests::initial_ready_router_with_window_loop();
        let target = {
            let mut current = router.current.lock().expect("fixture current");
            let active = current.as_mut().expect("fixture A");
            let target = WriterLink::new(
                active.writer.try_clone().expect("A pipe"),
                PrimaryCallerContext {
                    attempt: active.attempt,
                    identity: active.identity,
                    reader_stop: Arc::clone(&active.reader_stop),
                },
            )
            .expect("exact A writer link");
            active.writer_link = Some(Arc::clone(&target));
            target
        };
        let (writer, worker) =
            PrimaryWriterWorker::start(router.clone()).expect("one actual writer");
        router.primary_writer = Some(writer);
        router.window_ready.store(true, Ordering::Release);
        router.recovery_armed.store(true, Ordering::Release);
        Self {
            router,
            peer,
            owner,
            windows,
            worker,
            target,
        }
    }
    fn ping(&self, deadline: Instant) -> WriteReceipt {
        let frame = self
            .router
            .primary_writer_owner()
            .expect("writer")
            .try_prepare_control(
                Arc::clone(&self.target),
                ControlMessage::Ping {
                    channel: keld_ipc::LIFECYCLE_CHANNEL,
                    correlation: CorrelationId(91),
                },
                deadline,
            )
            .expect("bounded Ping");
        self.router
            .admit_primary_frame(self.target.caller(), frame)
            .expect("actual final fence")
            .expect("A admission")
    }
    fn actual_unknown(&self) -> WriteReceipt {
        self.peer
            .shutdown_app_link()
            .expect("actual failed original pipe");
        let receipt = self.ping(deadline());
        assert_eq!(
            receipt.wait(deadline()).expect("actual write returned"),
            WriteCompletion::OutcomeUnknown
        );
        assert!(self.router.current.lock().expect("current").is_none());
        receipt
    }
    fn report(&self, failure: PrimaryWriteFailure, restarted: &Cell<usize>, denied: &Cell<usize>) {
        let router = self.router.clone();
        let source = thread::spawn(move || router.report_primary_write_failure(failure));
        let app::DirectPrimaryOwnerCommand::FailPrimaryWriter(failure, reply) = self
            .owner
            .recv_timeout(Duration::from_secs(3))
            .expect("actual existing owner command")
        else {
            panic!("wrong failure command");
        };
        let result = self.router.apply_primary_writer_failure(
            &failure,
            |attempt| {
                assert_eq!(attempt, 1);
                assert!(
                    self.router.shutdown.transition.try_lock().is_err(),
                    "restart must be inside final canonical cut"
                );
                assert!(
                    self.router.current.try_lock().is_err(),
                    "current cannot change between predicate and restart"
                );
                restarted.set(restarted.get() + 1);
            },
            || {
                assert!(self.router.shutdown.transition.try_lock().is_err());
                denied.set(denied.get() + 1);
            },
        );
        reply
            .send(result.map(|_| ()))
            .expect("owner acknowledgement");
        source
            .join()
            .expect("source joined")
            .expect("failure report accepted or safely ignored");
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let caller = self.router.current.lock().ok().and_then(|current| {
            current.as_ref().map(|active| PrimaryCallerContext {
                attempt: active.attempt,
                identity: active.identity,
                reader_stop: Arc::clone(&active.reader_stop),
            })
        });
        if let Some(caller) = caller {
            let _ = self.router.retire_generation_after_write_failure(&caller);
        }
        let _ = self.worker.stop_and_join();
        if let Ok(mut readers) = self.router.readers.lock() {
            for (_, reader) in std::mem::take(&mut *readers) {
                let _ = reader.join();
            }
        }
    }
}
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(3)
}

#[test]
fn successful_write_and_short_observer_timeout_cannot_mint_failure() {
    let mut fixture = Fixture::new();
    let writer = fixture.router.primary_writer_owner().expect("writer");
    let (dequeued, resume) = writer.hold_next_dequeue_for_test();
    let receipt = fixture.ping(deadline());
    dequeued
        .recv_timeout(Duration::from_secs(3))
        .expect("actual dequeue");
    assert_eq!(
        receipt.wait(Instant::now() + Duration::from_millis(1)),
        Err(WriterRefusal::Deadline)
    );
    assert!(
        receipt.failure_evidence().expect("exact slot").is_none(),
        "short observer timeout does not expire original budget"
    );
    resume.send(()).expect("resume actual writer");
    fixture
        .peer
        .set_app_link_read_deadline(Some(Duration::from_secs(3)))
        .expect("bounded read");
    let (header, _) = keld_ipc::link::read_frame(&mut fixture.peer).expect("physical Ping");
    assert_eq!(header.corr, CorrelationId(91));
    assert_eq!(
        receipt.wait(deadline()).expect("completion"),
        WriteCompletion::Written
    );
    assert!(
        receipt
            .failure_evidence()
            .expect("exact written slot")
            .is_none()
    );
}

#[test]
fn exact_live_a_expired_admitted_write_is_retired_and_requested_once() {
    let fixture = Fixture::new();
    let writer = fixture.router.primary_writer_owner().expect("writer");
    let (dequeued, resume) = writer.hold_next_dequeue_for_test();
    let original_deadline = Instant::now() + Duration::from_millis(60);
    let receipt = fixture.ping(original_deadline);
    dequeued
        .recv_timeout(Duration::from_secs(3))
        .expect("actual admitted A write");
    assert_eq!(
        receipt.wait(original_deadline),
        Err(WriterRefusal::Deadline)
    );
    assert!(fixture.router.current.lock().expect("current A").is_some());
    let failure = receipt
        .failure_evidence()
        .expect("exact original expiry")
        .expect("admitted unknown outcome");
    let restarted = Cell::new(0);
    let denied = Cell::new(0);
    fixture.report(failure, &restarted, &denied);
    assert_eq!((restarted.get(), denied.get()), (1, 0));
    assert!(fixture.router.current.lock().expect("A revoked").is_none());
    assert_eq!(
        writer
            .shared
            .state
            .lock()
            .expect("charge retained")
            .charged(),
        1
    );
    resume.send(()).expect("finish exact closed A native entry");
    assert_eq!(
        receipt.wait(deadline()).expect("actual completion"),
        WriteCompletion::OutcomeUnknown
    );
    fixture.report(
        receipt
            .failure_evidence()
            .expect("late actual fact")
            .expect("unknown"),
        &restarted,
        &denied,
    );
    assert_eq!(
        (restarted.get(), denied.get()),
        (1, 0),
        "late actual completion cannot recreate consumed recovery authority"
    );
}

#[test]
fn actually_failed_retired_a_recovers_once_and_duplicate_is_inert() {
    let fixture = Fixture::new();
    let receipt = fixture.actual_unknown();
    let first = receipt
        .failure_evidence()
        .expect("actual slot")
        .expect("unknown");
    let duplicate = receipt
        .failure_evidence()
        .expect("same slot")
        .expect("same fact");
    drop(receipt); // Evidence contains no encoded payload or retained writer slot.
    let restarted = Cell::new(0);
    let denied = Cell::new(0);
    fixture.report(first, &restarted, &denied);
    fixture.report(duplicate, &restarted, &denied);
    assert_eq!((restarted.get(), denied.get()), (1, 0));
    assert!(
        fixture
            .router
            .shutdown
            .transition_guard()
            .failed_writer
            .is_none()
    );
    assert!(
        std::mem::size_of::<RetiredWriterFailure>() <= 64,
        "one weak metadata record, no payload retention"
    );
}

#[test]
fn accepted_shutdown_discards_exact_retired_a_fact_without_recovery_or_denial() {
    let fixture = Fixture::new();
    let receipt = fixture.actual_unknown();
    let failure = receipt
        .failure_evidence()
        .expect("A fact")
        .expect("unknown");
    drop(receipt);
    {
        let _transition = fixture.router.shutdown.transition_guard();
        assert!(
            fixture
                .router
                .shutdown
                .claim_guarded(app::SESSION_LIFECYCLE_QUIT)
        );
    }
    let restarted = Cell::new(0);
    let denied = Cell::new(0);
    fixture.report(failure, &restarted, &denied);
    assert_eq!((restarted.get(), denied.get()), (0, 0));
    assert!(
        fixture
            .router
            .shutdown
            .transition_guard()
            .failed_writer
            .is_none()
    );
}

#[test]
fn pre_ready_actual_write_failure_denies_only_its_exact_record() {
    let fixture = Fixture::new();
    fixture.router.window_ready.store(false, Ordering::Release);
    fixture
        .router
        .recovery_armed
        .store(false, Ordering::Release);
    let receipt = fixture.actual_unknown();
    let first = receipt
        .failure_evidence()
        .expect("A fact")
        .expect("unknown");
    let duplicate = receipt
        .failure_evidence()
        .expect("duplicate fact")
        .expect("unknown");
    drop(receipt);
    let restarted = Cell::new(0);
    let denied = Cell::new(0);
    fixture.report(first, &restarted, &denied);
    fixture.report(duplicate, &restarted, &denied);
    assert_eq!((restarted.get(), denied.get()), (0, 1));
    assert!(fixture.router.pre_ready_link_failed.load(Ordering::Acquire));
}

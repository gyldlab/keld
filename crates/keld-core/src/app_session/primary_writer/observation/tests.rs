//! Receipt ordering over a real Windows pipe and the existing primary writer.

use super::*;
use crate::app_session::{
    BootstrapStream, PrimaryCallerContext,
    primary_writer::{PrimaryWriterWorker, worker::CompletionHook},
    tests::{initial_ready_router_with_window_loop, primary_test_stream_pair},
};
use keld_ipc::{CorrelationId, ECHO_CHANNEL, FrameKind, link::read_frame};
use std::io;
use std::sync::mpsc;
use std::time::Duration;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
const FRAME: &[u8] = b"actual writer bytes";

struct Fixture {
    router: PrimaryRouterHandle,
    target: Arc<WriterLink>,
    peer: BootstrapStream,
    worker: PrimaryWriterWorker,
    _owner: mpsc::Receiver<crate::app_session::DirectPrimaryOwnerCommand>,
    _window: mpsc::Receiver<keld_wv::AppWindowCommand>,
}

impl Fixture {
    fn new() -> TestResult<Self> {
        let (mut router, peer, owner, window) = initial_ready_router_with_window_loop();
        let (writer, worker) = PrimaryWriterWorker::start(router.clone())?;
        router.primary_writer = Some(writer);
        let target = {
            let mut current = router.current.lock().expect("current owner");
            let active = current.as_mut().expect("original authenticated fixture");
            let caller = PrimaryCallerContext {
                attempt: active.attempt,
                identity: active.identity,
                reader_stop: Arc::clone(&active.reader_stop),
            };
            let stream = active
                .take_legacy_test_writer()
                .expect("original actual pipe");
            let target = WriterLink::new(stream, caller)?;
            active.writer_link = Some(Arc::clone(&target));
            target
        };
        Ok(Self {
            router,
            target,
            peer,
            worker,
            _owner: owner,
            _window: window,
        })
    }

    fn send(&self, deadline: Instant) -> TestResult<WriteReceipt> {
        let frame = self
            .router
            .primary_writer_owner()?
            .try_reserve_value(deadline)
            .map_err(|error| error.into_host_error("receipt race fixture"))?
            .copy_value(
                Arc::clone(&self.target),
                FrameKind::Reply,
                ECHO_CHANNEL,
                CorrelationId(1),
                FRAME,
            )?;
        Ok(self
            .router
            .admit_primary_frame(self.target.caller(), frame)?
            .ok_or_else(|| io::Error::other("frame never admitted"))?)
    }

    fn finish(&mut self) -> TestResult {
        if let Some(target) = self.router.current_primary_writer_target()? {
            self.router.retire_generation(target.caller().attempt)?;
        }
        let readers = std::mem::take(&mut *self.router.readers.lock().expect("reader custody"));
        for (_, reader) in readers {
            reader.join().expect("reader joins after link retirement")?;
        }
        self.worker.stop_and_join()?;
        Ok(())
    }
}

#[test]
fn written_slot_after_wait_deadline_keeps_the_physical_stamp_and_original_authority() -> TestResult
{
    for replace_role in [false, true] {
        let mut fixture = Fixture::new()?;
        let writer = fixture.router.primary_writer_owner()?;
        let (returned, physical) = mpsc::sync_channel(1);
        let (resume, publish) = mpsc::sync_channel(1);
        writer
            .shared
            .state
            .lock()
            .expect("writer state")
            .before_completion = Some(CompletionHook { returned, publish });
        let admitted_deadline = Instant::now() + Duration::from_secs(3);
        let receipt = fixture.send(admitted_deadline)?;
        let (outcome, written_at) = physical.recv_timeout(Duration::from_secs(2))?;
        let wire = read_frame(&mut fixture.peer)?;
        assert_eq!(wire.1, FRAME);
        assert_eq!(outcome, WriteCompletion::Written);

        // The physical write is already proved. Its charged slot is still
        // Writing when the observer's earlier deadline expires.
        let caller_deadline = written_at + Duration::from_millis(1);
        let observed = receipt.wait(caller_deadline);
        resume.send(())?;
        assert_eq!(observed, Err(WriterRefusal::Deadline));
        assert_eq!(
            receipt
                .wait(admitted_deadline + Duration::from_secs(1))
                .map_err(|error| error.into_host_error("published physical receipt"))?,
            WriteCompletion::Written
        );

        let _successor_peer = if replace_role {
            fixture
                .router
                .retire_generation(fixture.target.caller().attempt)?;
            let (server, peer) = primary_test_stream_pair();
            fixture
                .router
                .install_generation(2, server)
                .map_err(HostAppError::from)?;
            Some(peer)
        } else {
            None
        };
        let current = fixture
            .router
            .current_primary_writer_target()?
            .expect("current exact role");
        let (result, failure) = fixture
            .router
            .finish_primary_write_observation(
                &fixture.target,
                receipt,
                observed,
                caller_deadline,
                "actual Written slot after observer deadline",
            )
            .into_parts();
        let current_after = fixture.router.current_primary_writer_target()?;
        fixture.finish()?;
        assert_eq!(result?, written_at);
        assert!(
            failure.is_none(),
            "known Written grants no writer failure authority"
        );
        assert!(
            current_after
                .as_ref()
                .is_some_and(|after| Arc::ptr_eq(after, &current))
        );
    }
    Ok(())
}

#[test]
fn an_earlier_observer_cutoff_keeps_the_pending_original_writer_live() -> TestResult {
    let mut fixture = Fixture::new()?;
    let (dequeued, resume) = fixture
        .router
        .primary_writer_owner()?
        .hold_next_dequeue_for_test();
    let receipt = fixture.send(Instant::now() + Duration::from_secs(3))?;
    dequeued.recv_timeout(Duration::from_secs(2))?;
    let (result, failure) = fixture
        .router
        .observe_primary_write(
            &fixture.target,
            receipt,
            Instant::now(),
            "short observer cutoff",
        )
        .into_parts();
    let same_reader = fixture
        .router
        .current_primary_writer_target()?
        .is_some_and(|target| Arc::ptr_eq(&target, &fixture.target));
    resume.send(())?;
    let physical = read_frame(&mut fixture.peer);
    fixture.finish()?;
    assert!(
        result
            .expect_err("the observer cutoff ends its wait")
            .to_string()
            .contains("observer cutoff expired before the admitted primary write cutoff")
    );
    assert!(
        failure.is_none(),
        "an early observer timeout supplies no writer failure evidence"
    );
    assert!(
        same_reader,
        "an early observer timeout retired the current reader"
    );
    assert_eq!(physical?.1, FRAME);
    Ok(())
}

#[test]
fn a_pending_slot_at_its_admitted_cutoff_grants_exact_retirement() -> TestResult {
    let mut fixture = Fixture::new()?;
    let (dequeued, resume) = fixture
        .router
        .primary_writer_owner()?
        .hold_next_dequeue_for_test();
    let admitted = Instant::now() + Duration::from_millis(250);
    let receipt = fixture.send(admitted)?;
    dequeued.recv_timeout(Duration::from_secs(2))?;
    let (result, failure) = fixture
        .router
        .observe_primary_write(
            &fixture.target,
            receipt,
            admitted + Duration::from_secs(2),
            "admitted physical cutoff",
        )
        .into_parts();
    let role_retired = fixture.router.current_primary_writer_target()?.is_none();
    resume.send(())?;
    fixture.finish()?;
    assert!(
        result
            .expect_err("the original physical cutoff expires")
            .to_string()
            .contains("the original primary write deadline expired")
    );
    assert!(
        failure.is_some(),
        "a still-pending original slot has exact cutoff evidence"
    );
    assert!(
        role_retired,
        "only the actual cutoff closes the original role"
    );
    Ok(())
}

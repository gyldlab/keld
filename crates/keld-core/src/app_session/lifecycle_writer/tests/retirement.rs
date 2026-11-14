//! An actually blocked old pipe cannot hold retirement or redirect a later writer target.
use std::io::Read as _;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::thread;
use std::time::{Duration, Instant};

use super::{Fixture, fixture};
use crate::app_session::primary_writer::{ControlMessage, WriteCompletion};
use crate::app_session::tests::initial_ready_router_with_window_loop;
use crate::app_session::{BootstrapStream, CorrelationId, PrimaryRouterHandle};

fn successor_stream() -> (BootstrapStream, BootstrapStream) {
    let (spare, peer, owner, windows) = initial_ready_router_with_window_loop();
    let stream = spare
        .current
        .lock()
        .expect("fixture current")
        .take()
        .expect("fixture active")
        .writer;
    drop(spare);
    drop(owner);
    drop(windows);
    (stream, peer)
}

fn wait_for_original_revocation(router: &PrimaryRouterHandle) {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if router.current.lock().expect("canonical current").is_none() {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "old A must revoke without holding its writer wait"
        );
        thread::yield_now();
    }
}

#[test]
#[cfg_attr(
    windows,
    expect(
        clippy::too_many_lines,
        reason = "Keep the held original write beyond its deadline, exact revocation, successor isolation and final joins in one test."
    )
)]
fn blocked_original_write_outlives_terminal_deadline_then_closes_only_original_link() {
    let Fixture {
        router,
        mut peer,
        reader,
        caller,
        owner,
        windows,
        mut worker,
    } = fixture();
    let (successor, mut peer_b) = successor_stream();
    {
        let _transition = router.shutdown.transition_guard();
        router
            .current
            .lock()
            .expect("current")
            .as_mut()
            .expect("A")
            .pending_quit = Some(CorrelationId(41));
    }
    let target = router
        .current_primary_writer_target()
        .expect("target")
        .expect("A target");
    let deadline = Instant::now() + Duration::from_millis(500);
    let upstream = vec![0x5a_u8; 16 * 1024 * 1024];
    let frame = router
        .primary_writer_owner()
        .expect("one writer")
        .reserve_value(&caller, &router.shutdown, deadline)
        .expect("one VALUE charge")
        .copy_value(
            Arc::clone(&target),
            keld_ipc::FrameKind::Reply,
            keld_ipc::ECHO_CHANNEL,
            CorrelationId(7),
            &upstream,
        )
        .expect("one charged original frame");
    drop(upstream);
    let receipt = router
        .admit_primary_frame(&caller, frame)
        .expect("canonical enqueue")
        .expect("A live at enqueue");
    // A real header and body prefix proves the actual physical write entered the
    // Windows pipe. Stop reading well before its 16 MiB body can fit.
    let mut prefix = [0_u8; 256];
    peer.read_exact(&mut prefix)
        .expect("physical A frame prefix");
    assert_eq!(&prefix[128..], &[0x5a; 128]);
    assert!(receipt.poll().expect("original receipt").is_none());
    let started = Instant::now();
    let retiring = router.clone();
    let retiring = thread::spawn(move || retiring.retire_generation(1));
    wait_for_original_revocation(&router);
    assert!(router.shutdown.transition.try_lock().is_ok());
    assert!(router.current.try_lock().is_ok());
    assert!(caller.reader_stop.load(Ordering::Acquire));
    // This is the actual same-number generation installation method, using the same single
    // writer owner. A's already-admitted packets still retain their old link.
    router
        .install_generation(1, successor)
        .expect("install exact successor B");
    let target_b = router
        .current_primary_writer_target()
        .expect("B target")
        .expect("B live");
    router
        .submit_primary_control(
            target_b.caller(),
            ControlMessage::Ping {
                channel: keld_ipc::LIFECYCLE_CHANNEL,
                correlation: CorrelationId(77),
            },
            Instant::now() + Duration::from_secs(3),
            "retirement successor probe",
        )
        .expect("the same writer progresses to B");
    let (header, body) = keld_ipc::link::read_frame(&mut peer_b).expect("actual B frame");
    assert_eq!(header.kind, keld_ipc::FrameKind::Ping);
    assert_eq!(header.corr, CorrelationId(77));
    assert_eq!(body, [] as [u8; 0]);
    assert!(
        retiring.join().expect("bounded retirement joins").is_err(),
        "the shorter terminal deadline cannot report successful delivery"
    );
    assert!(started.elapsed() < Duration::from_secs(3));
    assert!(matches!(
        receipt
            .wait(Instant::now() + Duration::from_secs(1))
            .expect("actual original I/O returned"),
        WriteCompletion::OutcomeUnknown
    ));
    assert_eq!(
        router
            .current
            .lock()
            .expect("current B")
            .as_ref()
            .expect("B remains")
            .attempt,
        1
    );
    assert!(!Arc::ptr_eq(
        &caller.reader_stop,
        &target_b.caller().reader_stop
    ));
    assert!(!target_b.caller().reader_stop.load(Ordering::Acquire));
    drop(receipt);
    // Draining buffered partial bytes after retirement must reach actual A EOF,
    // not a synthetic stop flag. It cannot contain the complete 16 MiB frame.
    let mut drained = prefix.len();
    let mut chunk = [0_u8; 8192];
    loop {
        match peer.read(&mut chunk) {
            Ok(0) => break,
            Ok(count) => {
                drained += count;
                assert!(drained < 16 * 1024 * 1024);
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::BrokenPipe
                        | std::io::ErrorKind::ConnectionReset
                        | std::io::ErrorKind::ConnectionAborted
                        | std::io::ErrorKind::NotConnected
                ) =>
            {
                break;
            }
            Err(error) => panic!("exact A did not close after physical quiescence: {error}"),
        }
    }
    router
        .retire_generation_after_write_failure(target_b.caller())
        .expect("close owned B for fixture cleanup");
    router
        .readers
        .lock()
        .expect("readers")
        .remove(&1)
        .expect("actual B reader")
        .join()
        .expect("B reader joins")
        .expect("retired B reader stops");
    assert!(owner.try_recv().is_err());
    drop(reader);
    worker.stop_and_join().expect("one physical writer drains");
    drop(worker);
    drop(router);
    drop(windows);
}

#[test]
fn stopped_after_terminal_preparation_retires_exact_a_without_new_bytes() {
    let super::Fixture {
        router,
        mut peer,
        reader,
        caller,
        owner,
        windows,
        mut worker,
    } = super::fixture();
    {
        let _transition = router.shutdown.transition_guard();
        router
            .current
            .lock()
            .expect("current")
            .as_mut()
            .expect("A")
            .pending_quit = Some(crate::app_session::CorrelationId(43));
    }
    let (prepared, prepared_rx) = std::sync::mpsc::sync_channel(1);
    let (resume, resume_rx) = std::sync::mpsc::sync_channel(1);
    router
        .quit_drain_hooks
        .lock()
        .expect("existing lifecycle hooks")
        .retirement_prepared = Some((prepared, resume_rx));
    let retired = router.clone();
    let retire = std::thread::spawn(move || retired.retire_generation(1));
    prepared_rx
        .recv_timeout(std::time::Duration::from_secs(3))
        .expect("actual terminal reserved/encoded before final admission");
    {
        let _transition = router.shutdown.transition_guard();
        let current = router.current.lock().expect("current");
        assert!(std::sync::Arc::ptr_eq(
            &current.as_ref().expect("same A").reader_stop,
            &caller.reader_stop
        ));
        caller
            .reader_stop
            .store(true, std::sync::atomic::Ordering::Release);
    }
    resume.send(()).expect("resume final original-role fence");
    retire
        .join()
        .expect("retirement joins")
        .expect("trusted stopped A revokes without publishing new terminals");
    assert!(router.current.lock().expect("current").is_none());
    let mut byte = [0];
    match std::io::Read::read(&mut peer, &mut byte) {
        Ok(0) => {}
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::BrokenPipe
                    | std::io::ErrorKind::ConnectionReset
                    | std::io::ErrorKind::ConnectionAborted
                    | std::io::ErrorKind::NotConnected
            ) => {}
        result => panic!("stopped ordinary A published bytes or did not close: {result:?}"),
    }
    assert!(
        owner.try_recv().is_err(),
        "pure revocation does not invent recovery evidence"
    );
    drop(reader);
    worker.stop_and_join().expect("same actual writer drains");
    drop(worker);
    drop(router);
    drop(windows);
}

//! Actual terminal writer failure and existing generation-recovery owner.
//! Native outcomes are supplied at the Core callback boundary; this is not OS UI proof.
use super::{
    DialogOwner, DialogSession, MacDialogUiOutcome, WebviewId, options, original, with_manifest,
};
use crate::app_session as app;
use keld_ipc::codec::{decode, encode};
use keld_ipc::echo::{EchoRequest, EchoResponse};
use keld_ipc::frame::{CorrelationId, FrameKind};
use keld_ipc::link::{AppLinkDeadlines as _, read_frame, write_frame};
use std::io::Read as _;
use std::os::unix::net::UnixStream;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Fault {
    None,
    Acknowledgment,
    Retirement,
}

fn terminal_write_loss(fault: Fault, outcome: MacDialogUiOutcome) {
    let (mut handle, _peer, guardian, ui) = app::tests::initial_ready_router_with_window_loop();
    let (session, _wake, _temp) = with_manifest(r#"{"app":{"system":["dialog"]}}"#);
    let session = Arc::new(session);
    handle.dialog = Some(Arc::clone(&session));
    if fault == Fault::Retirement {
        let windows = Arc::new(Mutex::new(app::window_router::WindowSession::new(true)));
        let poison = Arc::clone(&windows);
        assert!(
            std::thread::spawn(move || {
                let _held = poison.lock().expect("actual window retirement state");
                panic!("controlled window retirement poison");
            })
            .join()
            .is_err()
        );
        handle.windows = Some(windows);
    }
    prepare_terminal(&session, outcome);
    let (mut read_probe, reader_stop) = {
        let current = handle.current.lock().expect("current writer");
        let active = current.as_ref().expect("generation one");
        (
            active.writer.try_clone().expect("retain old socket"),
            Arc::clone(&active.reader_stop),
        )
    };
    read_probe
        .set_app_link_deadlines(Some(Duration::from_secs(5)))
        .expect("bounded observation");
    // Keep the peer and this duplicate alive. Fail only the actual outbound
    // half, excluding peer EOF or Drop as a competing retirement owner.
    read_probe
        .shutdown(std::net::Shutdown::Write)
        .expect("fail outbound write only");
    let coordinator = coordinate_terminal(&handle, outcome);
    let mut retired_before_ack = false;
    let mut stopped_before_ack = false;
    let mut locks_released = false;
    let recovery_requested = match guardian.recv_timeout(Duration::from_secs(5)) {
        Ok(app::GuardianOwnerCommand::FailGeneration(1, reply)) => {
            retired_before_ack = handle
                .current
                .try_lock()
                .is_ok_and(|current| current.is_none());
            stopped_before_ack = reader_stop.load(Ordering::Acquire);
            locks_released = handle.shutdown.transition.try_lock().is_ok();
            reply
                .send(if fault == Fault::Acknowledgment {
                    Err("controlled dialog recovery acknowledgment failure".to_owned())
                } else {
                    Ok(())
                })
                .expect("existing guardian acknowledgment");
            true
        }
        _ => false,
    };
    let publication = coordinator.join().expect("coordinator joined");
    let fatal = ui
        .try_iter()
        .any(|command| matches!(command, app::AppWindowCommand::Fatal));
    let read_closed =
        reader_stop.load(Ordering::Acquire) && matches!(read_probe.read(&mut [0]), Ok(0));
    let running = handle.shutdown.is_running();
    // Clean up the unfixed baseline before asserting; it has no reader thread.
    if !recovery_requested {
        let _ = handle.retire_generation(1);
    }
    assert!(
        recovery_requested,
        "terminal write loss bypassed Guardian recovery: {publication:?}"
    );
    assert!(retired_before_ack && stopped_before_ack && locks_released && read_closed);
    assert!(session.fully_drained() && session.has_capacity());
    if fault != Fault::None {
        let error = publication.expect_err("failed retirement/ACK must remain fatal");
        assert!(fatal, "coordinator must preserve failed cleanup as Fatal");
        let rendered = error.to_string();
        assert!(
            rendered.contains("dialog correlated"),
            "original write error lost: {rendered}"
        );
        let expected = if fault == Fault::Retirement {
            "window role retirement"
        } else {
            "controlled dialog recovery acknowledgment failure"
        };
        assert!(
            rendered.contains(expected),
            "cleanup cause lost: {rendered}"
        );
        return;
    }
    publication.expect("successful retirement/ACK preserves the Host");
    assert!(running && !fatal);
    assert!(Arc::ptr_eq(
        &session,
        handle.dialog.as_ref().expect("same dialog owner")
    ));
    assert_successor_roundtrip(&handle);
}

fn prepare_terminal(session: &DialogSession, outcome: MacDialogUiOutcome) {
    // Parent admission/native presentation have separate tests. This cut owns
    // terminal publication only, so a parent lookup cannot mask the write fault.
    let owner = DialogOwner {
        parent: None,
        ..original(701, 1)
    };
    session
        .reserve(owner, options(), WebviewId(1))
        .expect("reserve actual custody");
    session
        .take_work()
        .expect("take queued work")
        .expect("one work item");
    session
        .record_native_outcome(owner, outcome, None)
        .expect("commit observed outcome");
}

fn coordinate_terminal(
    handle: &app::PrimaryRouterHandle,
    outcome: MacDialogUiOutcome,
) -> std::thread::JoinHandle<Result<(), app::HostAppError>> {
    let (events, receive) = mpsc::channel();
    events
        .send(app::AppWindowEvent::MacDialogFinished {
            attempt: 1,
            nonce: 1,
            outcome,
        })
        .expect("actual coordinator event");
    drop(events);
    let coordinator_handle = handle.clone();
    let commands = handle.window_commands.clone();
    std::thread::spawn(move || {
        app::coordinate_window_events(
            &receive,
            &coordinator_handle,
            &commands,
            app::AppReadyTrigger::HostInitialized,
            |_| Ok(()),
            None,
        )
    })
}

fn assert_successor_roundtrip(handle: &app::PrimaryRouterHandle) {
    // Install a real successor reader and route an actual framed call through
    // it. A merely cleared current-generation slot is not recovery evidence.
    let (server, mut successor) = UnixStream::pair().expect("successor stream pair");
    successor
        .set_app_link_deadlines(Some(Duration::from_secs(5)))
        .expect("successor deadline");
    handle
        .install_prepared_generation(2, server)
        .expect("same Host installs successor");
    let request = EchoRequest {
        message: "healthy after dialog write loss".to_owned(),
        count: 702,
    };
    let channel = keld_ipc::channel_table::ECHO.id();
    write_frame(
        &mut successor,
        FrameKind::Call,
        0,
        channel,
        CorrelationId(702),
        &encode(&request).expect("typed echo request"),
    )
    .expect("actual successor call");
    let response = read_frame(&mut successor);
    handle
        .retire_generation(2)
        .expect("retire successor after observation");
    let reader = handle
        .readers
        .lock()
        .expect("reader ownership")
        .remove(&2)
        .expect("retained successor reader");
    reader
        .join()
        .expect("successor reader joined")
        .expect("clean successor stop");
    let (header, payload) = response.expect("successor reply");
    assert_eq!(
        (header.kind, header.channel, header.corr),
        (FrameKind::Reply, channel, CorrelationId(702))
    );
    let response: EchoResponse = decode(&payload).expect("typed actual reply");
    assert_eq!(
        (response.message, response.count),
        (request.message, request.count)
    );
}

fn selected() -> MacDialogUiOutcome {
    MacDialogUiOutcome::Selected {
        response: 2,
        checkbox_checked: false,
    }
}

#[test]
fn dialog_terminal_reply_write_loss_recovers_actual_successor() {
    terminal_write_loss(Fault::None, selected());
}

#[test]
fn dialog_terminal_error_write_loss_recovers_actual_successor() {
    terminal_write_loss(Fault::None, MacDialogUiOutcome::Failed);
}

#[test]
fn dialog_terminal_write_loss_preserves_failed_guardian_ack() {
    terminal_write_loss(Fault::Acknowledgment, selected());
}

#[test]
fn dialog_terminal_write_loss_preserves_failed_retirement() {
    terminal_write_loss(Fault::Retirement, selected());
}

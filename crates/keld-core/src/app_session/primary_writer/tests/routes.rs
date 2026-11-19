//! The ordinary Windows routes use the installed physical writer and original pipe.
use super::super::super::{
    DirectPrimaryOwnerCommand, LifecycleEvent, LifecycleRequest, LifecycleResponse,
    tests::primary_test_stream_pair,
};
use super::*;
use keld_ipc::{CallError, ECHO_CHANNEL, LIFECYCLE_CHANNEL, codec, link};
use std::thread;

#[test]
fn ready_ping_echo_quit_and_drain_use_the_same_physical_owner() {
    let (mut router, _old_peer, commands, window) = initial_ready_router_with_window_loop();
    router
        .retire_generation(1)
        .expect("retire raw fixture link");
    let (writer, mut worker) = PrimaryWriterWorker::start(router.clone()).expect("physical owner");
    router.primary_writer = Some(writer.clone());
    let (host, mut peer) = primary_test_stream_pair();
    router
        .install_generation(2, host)
        .expect("install writer link");
    let target = {
        let current = router.current.lock().expect("actual current role");
        let active = current.as_ref().expect("generation two");
        assert!(active.legacy_test_writer().is_none());
        Arc::clone(active.writer_link.as_ref().expect("exact physical link"))
    };
    let guardian = thread::spawn(move || {
        for phase in ["Ready", "Quit preparation", "Quit completion"] {
            let received = commands
                .recv_timeout(Duration::from_secs(5))
                .expect("existing generation owner command");
            let reply = match (phase, received) {
                ("Ready", DirectPrimaryOwnerCommand::ArmRecovery(_, reply))
                | ("Quit preparation", DirectPrimaryOwnerCommand::PrepareAcceptedShutdown(reply))
                | ("Quit completion", DirectPrimaryOwnerCommand::Shutdown(reply)) => reply,
                (phase, _) => panic!("unexpected generation owner command at {phase}"),
            };
            reply.send(Ok(())).expect("settle owner command");
        }
    });
    router.signal_ready().expect("publish physical Ready");
    let (header, payload) = link::read_frame(&mut peer).expect("read physical Ready");
    assert_eq!(
        (header.kind, header.channel),
        (FrameKind::Event, LIFECYCLE_CHANNEL)
    );
    assert_eq!(
        codec::decode::<LifecycleEvent>(&payload).expect("Ready payload"),
        LifecycleEvent::Ready
    );

    let (held, resume) = writer.hold_next_dequeue_for_test();
    let (observe_timeout, actual_timeout) = mpsc::sync_channel(1);
    target.observe_next_write_deadline_for_test(observe_timeout);
    let ping = CorrelationId(17);
    link::write_frame(&mut peer, FrameKind::Ping, 0, LIFECYCLE_CHANNEL, ping, &[])
        .expect("Ping from peer");
    held.recv_timeout(Duration::from_secs(2))
        .expect("same worker holds actual Ping reply");
    assert_eq!(count(&writer), 1);
    resume.send(()).expect("allow Ping reply");
    let configured = actual_timeout
        .recv_timeout(Duration::from_secs(2))
        .expect("physical stream deadline")
        .expect("bounded original write");
    assert!(configured <= keld_ipc::APP_LINK_IO_DEADLINE);
    let (header, payload) = link::read_frame(&mut peer).expect("read Ping reply");
    assert_eq!(
        (header.kind, header.channel, header.corr),
        (FrameKind::Ping, LIFECYCLE_CHANNEL, ping)
    );
    assert!(payload.is_empty());

    let echo = CorrelationId(18);
    let request = keld_ipc::EchoRequest {
        message: String::from("physical echo"),
        count: 37,
    };
    let bytes = codec::encode(&request).expect("Echo encoding");
    link::write_frame(&mut peer, FrameKind::Call, 0, ECHO_CHANNEL, echo, &bytes)
        .expect("Echo from peer");
    let (header, payload) = link::read_frame(&mut peer).expect("read Echo reply");
    assert_eq!(
        (header.kind, header.channel, header.corr),
        (FrameKind::Reply, ECHO_CHANNEL, echo)
    );
    let response = codec::decode::<keld_ipc::EchoResponse>(&payload).expect("Echo reply");
    assert_eq!(
        (response.message, response.count),
        (request.message, request.count)
    );

    let quit = CorrelationId(19);
    let drained = CorrelationId(20);
    let bytes = codec::encode(&LifecycleRequest::Quit).expect("Quit encoding");
    link::write_frame(
        &mut peer,
        FrameKind::Call,
        0,
        LIFECYCLE_CHANNEL,
        quit,
        &bytes,
    )
    .expect("accepted Quit from peer");
    link::write_frame(
        &mut peer,
        FrameKind::Call,
        0,
        LIFECYCLE_CHANNEL,
        drained,
        &bytes,
    )
    .expect("one subsequent frame to drain");
    let (header, payload) = link::read_frame(&mut peer).expect("physical Quit reply first");
    assert_eq!(
        (header.kind, header.channel, header.corr),
        (FrameKind::Reply, LIFECYCLE_CHANNEL, quit)
    );
    assert_eq!(
        codec::decode::<LifecycleResponse>(&payload).expect("Quit reply"),
        LifecycleResponse::Quit
    );
    let (header, payload) = link::read_frame(&mut peer).expect("physical post-Quit drain response");
    assert_eq!(
        (header.kind, header.channel, header.corr),
        (FrameKind::Err, LIFECYCLE_CHANNEL, drained)
    );
    assert_eq!(
        codec::decode::<CallError>(&payload).expect("drain error"),
        CallError::quit_drained()
    );
    peer.shutdown_app_link().expect("peer retires");
    assert!(matches!(
        window.recv_timeout(Duration::from_secs(4)),
        Ok(AppWindowCommand::Quit)
    ));
    guardian.join().expect("guardian settles");
    let reader = router
        .readers
        .lock()
        .expect("reader owner")
        .remove(&2)
        .expect("original reader");
    reader
        .join()
        .expect("reader joins")
        .expect("terminal tail succeeds");
    assert_eq!(count(&writer), 0);
    worker.stop_and_join().expect("physical owner joins");
}

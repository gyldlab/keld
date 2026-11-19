//! Native OS Quit is intent for the selected current original owner, not a
//! native close receipt. The actual app-link frames are the independent oracle.

use super::*;
use std::sync::Mutex;

fn install_original(handle: &mut PrimaryRouterHandle) -> tempfile::TempDir {
    handle.windows = Some(Arc::new(Mutex::new(
        app::window_router::WindowSession::new(true),
    )));
    let (source, selection) = app::tests::gh499_graph::original_quit_selection();
    handle
        .windows
        .as_ref()
        .expect("window registry")
        .lock()
        .expect("registry lock")
        .install_original_selection(selection)
        .expect("selected original app");
    source
}

fn subscribe(handle: &PrimaryRouterHandle, peer: &mut app::BootstrapStream) {
    handle
        .subscribe_window(
            &caller(handle),
            CorrelationId(83),
            keld_ipc::channel_table::WINDOW.wire_id(),
        )
        .expect("original role subscribed");
    let (header, bytes) = read_frame(peer).expect("actual subscription reply");
    assert_eq!(
        (header.kind, header.channel, header.corr),
        (FrameKind::Reply, LIFECYCLE_CHANNEL, CorrelationId(83))
    );
    assert_eq!(
        decode::<keld_ipc::LifecycleResponse>(&bytes).expect("typed response"),
        keld_ipc::LifecycleResponse::Subscribed
    );
}

fn ready(
    handle: &PrimaryRouterHandle,
    peer: &mut app::BootstrapStream,
    commands: mpsc::Receiver<app::GuardianOwnerCommand>,
) {
    let owner = std::thread::spawn(move || {
        let app::GuardianOwnerCommand::ArmRecovery(_, reply) = commands
            .recv_timeout(Duration::from_secs(3))
            .expect("guardian receives the real Ready arm")
        else {
            panic!("Ready must arm the current guardian");
        };
        reply.send(Ok(())).expect("Ready arm acknowledged");
    });
    handle.signal_ready().expect("genuine Ready publication");
    let (header, bytes) = read_frame(peer).expect("actual Ready frame");
    assert_eq!(
        (header.kind, header.channel, header.corr),
        (FrameKind::Event, LIFECYCLE_CHANNEL, CorrelationId(0))
    );
    assert_eq!(bytes, [0]);
    owner.join().expect("guardian test owner joins");
}

fn no_quit_frame(peer: &mut app::BootstrapStream) {
    let failure = read_frame(peer).expect_err("unqualified native Quit must emit no frame");
    assert!(
        matches!(
            failure,
            keld_ipc::IpcError::Timeout | keld_ipc::IpcError::Io(_)
        ),
        "unexpected app-link outcome: {failure:?}"
    );
}

#[test]
fn native_os_quit_forwards_only_after_selected_subscribed_and_ready() {
    let (mut handle, mut peer, owner_commands, ui_commands) =
        app::tests::initial_ready_router_with_window_loop();
    peer.set_app_link_deadlines(Some(Duration::from_millis(100)))
        .expect("bounded wire observation");

    handle
        .native_os_quit_requested()
        .expect("no selected original owner cancels Quit");
    no_quit_frame(&mut peer);

    let _source = install_original(&mut handle);
    handle
        .native_os_quit_requested()
        .expect("unsubscribed original owner cancels Quit");
    no_quit_frame(&mut peer);

    subscribe(&handle, &mut peer);
    handle
        .native_os_quit_requested()
        .expect("unready original owner cancels Quit");
    no_quit_frame(&mut peer);

    ready(&handle, &mut peer, owner_commands);
    handle
        .native_os_quit_requested()
        .expect("selected original role receives intent");
    let (header, bytes) = read_frame(&mut peer).expect("actual native Quit intent frame");
    assert_eq!(
        (header.kind, header.channel, header.corr),
        (FrameKind::Event, LIFECYCLE_CHANNEL, CorrelationId(0))
    );
    assert_eq!(bytes, [2], "existing original QuitRequested unit");
    assert_eq!(
        decode::<LifecycleEvent>(&bytes).expect("strict lifecycle decoder"),
        LifecycleEvent::QuitRequested
    );
    assert!(
        ui_commands.try_recv().is_err(),
        "AppKit intent cannot enqueue a direct native close or terminal command"
    );
}

#[test]
fn native_os_quit_rejects_stale_subscription_and_stopped_reader() {
    let (mut handle, mut peer, owner_commands, _ui) =
        app::tests::initial_ready_router_with_window_loop();
    peer.set_app_link_deadlines(Some(Duration::from_millis(100)))
        .expect("bounded wire observation");
    let _source = install_original(&mut handle);
    subscribe(&handle, &mut peer);
    ready(&handle, &mut peer, owner_commands);

    // The old original subscription belongs to attempt 1. Changing the
    // current writer alone cannot make it a subscription for attempt 2.
    {
        let mut generation = handle.current.lock().expect("current role");
        generation.as_mut().expect("active role").attempt = 2;
    }
    handle
        .native_os_quit_requested()
        .expect("old subscription cannot authorize a successor");
    no_quit_frame(&mut peer);

    {
        let mut generation = handle.current.lock().expect("current role");
        let active = generation.as_mut().expect("active role");
        active.attempt = 1;
        active.reader_stop.store(true, Ordering::Release);
    }
    handle
        .native_os_quit_requested()
        .expect("stopped original reader cannot receive Quit");
    no_quit_frame(&mut peer);
}

#[test]
fn native_os_quit_failed_write_retires_exact_current_owner() {
    let (mut handle, mut peer, owner_commands, _ui) =
        app::tests::initial_ready_router_with_window_loop();
    peer.set_app_link_deadlines(Some(Duration::from_secs(3)))
        .expect("bounded wire observation");
    let _source = install_original(&mut handle);
    subscribe(&handle, &mut peer);
    ready(&handle, &mut peer, owner_commands);
    let reader_stop = Arc::clone(&caller(&handle).reader_stop);
    peer.shutdown(std::net::Shutdown::Both)
        .expect("real link shutdown");
    drop(peer);

    let error = handle
        .native_os_quit_requested()
        .expect_err("an actual failed intent write cannot be reported as success");
    assert!(
        error.to_string().contains("native OS QuitRequested write"),
        "{error}"
    );
    assert!(
        reader_stop.load(Ordering::Acquire),
        "failed write retires this reader"
    );
    assert!(
        handle.current.lock().expect("generation lock").is_none(),
        "failed write drops the same generation"
    );
}

#[test]
fn fresh_successor_native_quit_follows_its_actual_ready_frame() {
    let (mut router, mut first_peer, old_guardian, _ui) =
        app::tests::initial_ready_router_with_window_loop();
    first_peer
        .set_app_link_deadlines(Some(Duration::from_secs(3)))
        .expect("first peer deadline");
    let _source = install_original(&mut router);
    subscribe(&router, &mut first_peer);
    ready(&router, &mut first_peer, old_guardian);
    let first_owner = caller(&router);
    router
        .retire_generation(first_owner.attempt)
        .expect("G1 current link and owner retire");
    let (replacement, mut next_peer, _new_guardian, _next_ui) =
        app::tests::initial_ready_router_with_window_loop();
    next_peer
        .set_app_link_deadlines(Some(Duration::from_secs(3)))
        .expect("G2 peer deadline");
    let mut successor = replacement
        .current
        .lock()
        .expect("new generation")
        .take()
        .expect("new role");
    successor.attempt = first_owner.attempt + 1;
    *router.current.lock().expect("router current") = Some(successor);
    subscribe(&router, &mut next_peer);

    // Recovery is already armed. The existing Core Ready publisher writes
    // one real Ready [0] on this G2 socket and does not re-arm Guardian.
    router.signal_ready().expect("G2 actual Ready publisher");
    let (ready_header, ready_bytes) = read_frame(&mut next_peer).expect("G2 Ready frame");
    assert_eq!(
        (ready_header.kind, ready_header.channel, ready_header.corr),
        (FrameKind::Event, LIFECYCLE_CHANNEL, CorrelationId(0))
    );
    assert_eq!(ready_bytes, [0], "G2 must receive its own Ready first");

    // A newly generated G2 intent is valid; the negative held-G1 fixture
    // in GH451-buffered-native-intent-RED.md must distinguish the two.
    router
        .native_os_quit_requested()
        .expect("fresh selected G2 native Quit");
    let (quit_header, quit_bytes) =
        read_frame(&mut next_peer).expect("fresh G2 native QuitRequested");
    assert_eq!(
        (quit_header.kind, quit_header.channel, quit_header.corr),
        (FrameKind::Event, LIFECYCLE_CHANNEL, CorrelationId(0))
    );
    assert_eq!(quit_bytes, [2], "existing native intent unit");
    assert_eq!(
        decode::<LifecycleEvent>(&quit_bytes).expect("strict G2 event"),
        LifecycleEvent::QuitRequested
    );
}

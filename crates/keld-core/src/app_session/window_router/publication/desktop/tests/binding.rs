//! Selected Window publication over the real primary Windows writer.
use super::*;
use crate::app_session::{
    graph_contract::tests::retained_original_window_resources,
    window_registry::{CreateOutcome, OriginalWindowResources},
};
use keld_ipc::{
    FrameKind,
    window::{WindowEvent, WindowResponse},
};
use keld_wv::{AppWindowEvent, WebviewId, WindowSnapshot};

#[test]
fn selected_create_writes_created_then_exact_view_binding_then_reply() -> Result<(), TestError> {
    let mut fixture = DesktopFixture::new()?;
    fixture
        .peer
        .set_app_link_read_deadline(Some(Duration::from_secs(4)))?;
    let (_stage, source, preload) = retained_original_window_resources();
    let (build, window) = {
        let mut session = fixture
            .router
            .windows
            .as_ref()
            .expect("Window owner")
            .lock()
            .expect("registry");
        session.desktop = None;
        assert!(session.registry.mark_ready());
        session.subscribed = Some(WindowOwner::new(
            fixture.caller.identity,
            fixture.caller.attempt,
        ));
        let CreateOutcome::Build { build, window, .. } =
            session.registry.create_original_for_caller(
                &fixture.caller,
                CorrelationId(73),
                (640, 480),
                (None, None),
                OriginalWindowResources { source, preload },
            )?
        else {
            panic!("new selected build expected")
        };
        (build, window)
    };
    let disposition = fixture
        .router
        .window_native_event_ordered(AppWindowEvent::WindowBuilt {
            build,
            view: WebviewId(7),
            snapshot: WindowSnapshot {
                x: 0,
                y: 0,
                width: 640,
                height: 480,
                focused: false,
                maximized: false,
                full_screen: false,
            },
        })?;
    assert!(matches!(disposition, NativePublicationDisposition::Handled));
    let (header, bytes) = read_frame(&mut fixture.peer)?;
    assert_eq!(
        (header.kind, header.channel, header.corr),
        (
            FrameKind::Event,
            keld_ipc::channel_table::WINDOW.id(),
            CorrelationId(0)
        )
    );
    assert!(
        matches!(codec::decode::<WindowEvent>(&bytes)?, WindowEvent::Created { window: actual, .. } if actual == window)
    );
    let (header, bytes) = read_frame(&mut fixture.peer)?;
    assert_eq!(
        (header.kind, header.channel, header.corr),
        (
            FrameKind::Event,
            keld_ipc::channel_table::WINDOW.id(),
            CorrelationId(0)
        ),
        "binding must physically precede Create success"
    );
    assert_eq!(
        bytes,
        [12, 1, 1, 7],
        "exact selected Window/view wire tuple"
    );
    let (header, bytes) = read_frame(&mut fixture.peer)?;
    assert_eq!(
        (header.kind, header.corr),
        (FrameKind::Reply, CorrelationId(73))
    );
    assert_eq!(
        codec::decode::<WindowResponse>(&bytes)?,
        WindowResponse::Created { window }
    );
    fixture.worker.stop_and_join()?;
    Ok(())
}

#[test]
fn selected_replay_finishes_created_census_before_bindings_and_subscribe_reply()
-> Result<(), TestError> {
    let mut fixture = DesktopFixture::new()?;
    fixture
        .peer
        .set_app_link_read_deadline(Some(Duration::from_secs(4)))?;
    let (_stage, source, preload) = retained_original_window_resources();
    let mut expected = Vec::new();
    // The socket fixture's current caller is distinct from this retired owner.
    // Registry state is real, but these are not authenticated Runtime roles.
    let retired = PrimaryCallerContext {
        identity: fixture.caller.identity,
        attempt: fixture.caller.attempt + 1,
        reader_stop: Arc::new(AtomicBool::new(false)),
    };
    {
        let mut session = fixture
            .router
            .windows
            .as_ref()
            .expect("Window owner")
            .lock()
            .expect("registry");
        session.desktop = None;
        assert!(session.registry.mark_ready());
        for view in [7_u32, 8] {
            let CreateOutcome::Build { build, window, .. } =
                session.registry.create_original_for_caller(
                    &retired,
                    CorrelationId(view),
                    (640, 480),
                    (None, None),
                    OriginalWindowResources {
                        source: Arc::clone(&source),
                        preload: Arc::clone(&preload),
                    },
                )?
            else {
                panic!("new selected build expected")
            };
            let state = keld_ipc::window::WindowState {
                bounds: keld_ipc::window::Rect {
                    x: 0,
                    y: 0,
                    width: 640,
                    height: 480,
                },
                focused: false,
                maximized: false,
                full_screen: false,
            };
            assert!(matches!(
                session.registry.native_built(build, WebviewId(view), state),
                crate::app_session::window_registry::BuildOutcome::Created { .. }
            ));
            expected.push(window);
        }
        session
            .registry
            .role_lost(WindowOwner::new(retired.identity, retired.attempt), |_| {
                panic!("no close was pending")
            });
    }
    fixture.router.subscribe_window_ordered(
        &fixture.caller,
        CorrelationId(99),
        keld_ipc::channel_table::WINDOW.wire_id(),
    )?;
    for window in expected {
        let (header, bytes) = read_frame(&mut fixture.peer)?;
        assert_eq!(header.kind, FrameKind::Event);
        assert!(
            matches!(codec::decode::<WindowEvent>(&bytes)?, WindowEvent::Created { window: actual, .. } if actual == window)
        );
    }
    for bytes_expected in [[12, 1, 1, 7], [12, 2, 2, 8]] {
        let (header, bytes) = read_frame(&mut fixture.peer)?;
        assert_eq!(
            (header.kind, header.channel, header.corr),
            (
                FrameKind::Event,
                keld_ipc::channel_table::WINDOW.id(),
                CorrelationId(0)
            ),
            "both bindings must follow the whole Created census and precede Subscribe success"
        );
        assert_eq!(bytes, bytes_expected);
    }
    let (header, bytes) = read_frame(&mut fixture.peer)?;
    assert_eq!(
        (header.kind, header.corr),
        (FrameKind::Reply, CorrelationId(99))
    );
    assert_eq!(
        codec::decode::<LifecycleResponse>(&bytes)?,
        LifecycleResponse::Subscribed
    );
    fixture.worker.stop_and_join()?;
    Ok(())
}

#[test]
fn refused_binding_never_offers_selected_create_success() -> Result<(), TestError> {
    use crate::app_session::primary_writer::WriterRefusal;
    let mut fixture = DesktopFixture::new()?;
    fixture
        .peer
        .set_app_link_read_deadline(Some(Duration::from_secs(4)))?;
    let (_stage, source, preload) = retained_original_window_resources();
    let build = {
        let mut session = fixture
            .router
            .windows
            .as_ref()
            .expect("Window owner")
            .lock()
            .expect("registry");
        session.desktop = None;
        assert!(session.registry.mark_ready());
        session.subscribed = Some(WindowOwner::new(
            fixture.caller.identity,
            fixture.caller.attempt,
        ));
        let CreateOutcome::Build { build, .. } = session.registry.create_original_for_caller(
            &fixture.caller,
            CorrelationId(73),
            (640, 480),
            (None, None),
            OriginalWindowResources { source, preload },
        )?
        else {
            panic!("new selected build expected")
        };
        build
    };
    let writer = fixture.router.primary_writer_owner()?.clone();
    let (dequeued, resume) = writer.hold_next_dequeue_for_test();
    let router = fixture.router.clone();
    let event = std::thread::spawn(move || {
        router.window_native_event_ordered(AppWindowEvent::WindowBuilt {
            build,
            view: WebviewId(7),
            snapshot: WindowSnapshot {
                x: 0,
                y: 0,
                width: 640,
                height: 480,
                focused: false,
                maximized: false,
                full_screen: false,
            },
        })
    });
    // The first actual record has left the queue; the failure applies only to
    // the subsequent binding admission, while Created is allowed to complete.
    dequeued.recv_timeout(Duration::from_secs(3))?;
    writer.fail_next_send_for_test(WriterRefusal::Full);
    resume.send(())?;
    let (header, bytes) = read_frame(&mut fixture.peer)?;
    assert_eq!(header.kind, FrameKind::Event);
    assert!(matches!(
        codec::decode::<WindowEvent>(&bytes)?,
        WindowEvent::Created { .. }
    ));
    fixture.no_bytes()?;
    assert!(matches!(
        event.join().expect("publication thread")?,
        NativePublicationDisposition::FatalQueued(_)
    ));
    assert!(matches!(
        fixture.commands.try_recv()?,
        AppWindowCommand::Fatal
    ));
    fixture.worker.stop_and_join()?;
    Ok(())
}

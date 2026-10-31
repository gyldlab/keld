//! Actual socket/writer close ordering; fake UI receipts prove no native behavior.
use super::{Fixture, TestResult, fact, install_successor, reply, snapshot, verdict};
use keld_ipc::codec::{decode, encode};
use keld_ipc::link::read_frame;
use keld_ipc::window::{CloseVerdict, WindowEvent, WindowRequest, WindowResponse};
use keld_ipc::{CorrelationId, FrameKind, LIFECYCLE_CHANNEL, LifecycleEvent, LifecycleResponse};
use keld_wv::{AppWindowCommand, AppWindowEvent, WebviewId};
use std::time::Duration;

#[test]
fn bounds_precede_close_and_stale_allow_is_diagnosed_once_without_teardown() -> TestResult {
    let mut fixture = Fixture::new(Some(true))?;
    fixture.subscribe()?;
    fixture.ready()?;
    let window = fixture.build(1, 7)?;
    fixture
        .handle
        .window_native_event(AppWindowEvent::WindowBounds {
            view: WebviewId(7),
            x: 50,
            y: 60,
            width: 900,
            height: 700,
        })?;
    fixture.call(&WindowRequest::RequestClose { window }, 2)?;
    fact(
        &mut fixture,
        WindowEvent::Bounds {
            window,
            bounds: keld_ipc::window::Rect {
                x: 50,
                y: 60,
                width: 900,
                height: 700,
            },
        },
    )?;
    fact(
        &mut fixture,
        WindowEvent::CloseRequested {
            window,
            close_seq: 1,
        },
    )?;
    reply(
        &mut fixture,
        2,
        WindowResponse::CloseRequested { close_seq: 1 },
    )?;
    verdict(&fixture, window, 1, CloseVerdict::Veto, 3)?;
    reply(&mut fixture, 3, WindowResponse::ReplyApplied)?;
    fixture.call(&WindowRequest::RequestClose { window }, 4)?;
    fact(
        &mut fixture,
        WindowEvent::CloseRequested {
            window,
            close_seq: 2,
        },
    )?;
    reply(
        &mut fixture,
        4,
        WindowResponse::CloseRequested { close_seq: 2 },
    )?;
    verdict(&fixture, window, 1, CloseVerdict::Allow, 5)?;
    reply(&mut fixture, 5, WindowResponse::ReplyStale)?;
    assert!(
        fixture.commands.try_recv().is_err(),
        "stale allow cannot reach the UI"
    );
    fixture.call(
        &WindowRequest::SetFullScreen {
            window,
            full_screen: true,
        },
        6,
    )?;
    assert_eq!(
        fixture.commands.recv_timeout(Duration::from_secs(3))?,
        AppWindowCommand::SetFullScreen {
            view: WebviewId(7),
            full_screen: true
        }
    );
    reply(&mut fixture, 6, WindowResponse::Accepted)?;
    fixture
        .handle
        .window_native_event(AppWindowEvent::WindowFullScreen {
            view: WebviewId(7),
            full_screen: true,
        })?;
    fact(
        &mut fixture,
        WindowEvent::FullScreen {
            window,
            full_screen: true,
        },
    )?;
    verdict(&fixture, window, 2, CloseVerdict::Allow, 7)?;
    reply(&mut fixture, 7, WindowResponse::ReplyApplied)?;
    assert_eq!(
        fixture.commands.recv_timeout(Duration::from_secs(3))?,
        AppWindowCommand::DestroyWindow { view: WebviewId(7) }
    );
    assert!(fixture.commands.try_recv().is_err());
    Ok(())
}

#[test]
fn repeated_destroy_waits_for_actual_release_then_facts_precede_both_replies() -> TestResult {
    let mut fixture = Fixture::new(Some(true))?;
    fixture.subscribe()?;
    fixture.ready()?;
    let window = fixture.build(1, 7)?;
    fixture.call(&WindowRequest::Destroy { window }, 2)?;
    assert_eq!(
        fixture.commands.recv_timeout(Duration::from_secs(3))?,
        AppWindowCommand::DestroyWindow { view: WebviewId(7) }
    );
    fixture.call(&WindowRequest::Destroy { window }, 3)?;
    assert!(
        fixture.commands.try_recv().is_err(),
        "one physical release command"
    );
    // Closing's immediate acknowledgement fences both deferred calls. If either
    // Destroy completed before Released, this exact correlated read fails.
    fixture.call(&WindowRequest::RequestClose { window }, 4)?;
    reply(&mut fixture, 4, WindowResponse::AlreadyClosing)?;
    fixture
        .handle
        .window_native_event(AppWindowEvent::WindowFocus {
            view: WebviewId(7),
            focused: false,
        })?;
    fixture
        .handle
        .window_native_event(AppWindowEvent::WindowReleased { view: WebviewId(7) })?;
    fact(&mut fixture, WindowEvent::Destroyed { window })?;
    let (header, bytes) = read_frame(&mut fixture.peer)?;
    assert_eq!(
        (header.kind, header.channel),
        (FrameKind::Event, LIFECYCLE_CHANNEL)
    );
    assert_eq!(
        decode::<LifecycleEvent>(&bytes)?,
        LifecycleEvent::LastWindowClosed
    );
    reply(&mut fixture, 2, WindowResponse::Destroyed)?;
    reply(&mut fixture, 3, WindowResponse::Destroyed)?;
    fixture.call(&WindowRequest::Destroy { window }, 5)?;
    fixture.error(5, "KELD-CORE-038")?;
    assert!(fixture.commands.try_recv().is_err());
    Ok(())
}

#[test]
fn one_stalled_release_caps_host_destroy_correlations_with_typed_nonterminal_error() -> TestResult {
    let mut fixture = Fixture::new(Some(true))?;
    fixture.subscribe()?;
    fixture.ready()?;
    let window = fixture.build(1, 7)?;
    for correlation in 2..=65 {
        fixture.call(&WindowRequest::Destroy { window }, correlation)?;
        if correlation == 2 {
            assert_eq!(
                fixture.commands.recv_timeout(Duration::from_secs(3))?,
                AppWindowCommand::DestroyWindow { view: WebviewId(7) }
            );
        } else {
            assert!(fixture.commands.try_recv().is_err(), "one native teardown");
        }
    }
    fixture.call(&WindowRequest::Destroy { window }, 66)?;
    fixture.error(66, "KELD-CORE-046")?;
    // Correlated over-capacity denial keeps the same authenticated writer
    // alive, and ordinary Closing replies still behave as specified.
    fixture.call(&WindowRequest::RequestClose { window }, 67)?;
    reply(&mut fixture, 67, WindowResponse::AlreadyClosing)?;
    fixture
        .handle
        .window_native_event(AppWindowEvent::WindowReleased { view: WebviewId(7) })?;
    fact(&mut fixture, WindowEvent::Destroyed { window })?;
    let (header, body) = read_frame(&mut fixture.peer)?;
    assert_eq!(
        (header.kind, header.channel),
        (FrameKind::Event, LIFECYCLE_CHANNEL)
    );
    assert_eq!(
        decode::<LifecycleEvent>(&body)?,
        LifecycleEvent::LastWindowClosed
    );
    for correlation in 2..=65 {
        reply(&mut fixture, correlation, WindowResponse::Destroyed)?;
    }
    assert!(
        fixture.commands.try_recv().is_err(),
        "denied Destroy caused no UI effect"
    );
    Ok(())
}

#[test]
fn gap_close_replays_one_coalesced_pending_snapshot_and_only_successor_can_answer() -> TestResult {
    let mut fixture = Fixture::new(Some(true))?;
    fixture.subscribe()?;
    fixture.ready()?;
    let window = fixture.build(1, 7)?;
    fixture.call(&WindowRequest::RequestClose { window }, 2)?;
    fact(
        &mut fixture,
        WindowEvent::CloseRequested {
            window,
            close_seq: 1,
        },
    )?;
    reply(
        &mut fixture,
        2,
        WindowResponse::CloseRequested { close_seq: 1 },
    )?;
    let retired = fixture.caller()?;
    fixture.retire_reader(1)?;
    assert!(
        read_frame(&mut fixture.peer).is_err(),
        "retired link physically closes"
    );
    fixture
        .handle
        .window_native_event(AppWindowEvent::WindowCloseRequested { view: WebviewId(7) })?;
    fixture
        .handle
        .window_native_event(AppWindowEvent::WindowCloseRequested { view: WebviewId(7) })?;
    fixture
        .handle
        .window_native_event(AppWindowEvent::WindowFocus {
            view: WebviewId(7),
            focused: false,
        })?;
    fixture
        .handle
        .window_native_event(AppWindowEvent::WindowFocus {
            view: WebviewId(7),
            focused: true,
        })?;
    fixture
        .handle
        .window_native_event(AppWindowEvent::WindowMaximized {
            view: WebviewId(7),
            maximized: true,
        })?;
    assert!(
        fixture.commands.try_recv().is_err(),
        "recovery gap never tears down on close"
    );
    install_successor(&mut fixture)?;
    fixture.handle.subscribe_window(
        &fixture.caller()?,
        CorrelationId(3),
        keld_ipc::channel_table::WINDOW.wire_id(),
    )?;
    let mut expected = super::super::native_state(snapshot());
    expected.maximized = true;
    fact(
        &mut fixture,
        WindowEvent::Created {
            window,
            state: expected,
            pending_close: Some(2),
        },
    )?;
    let (header, bytes) = read_frame(&mut fixture.peer)?;
    assert_eq!(
        (header.kind, header.channel, header.corr),
        (FrameKind::Reply, LIFECYCLE_CHANNEL, CorrelationId(3))
    );
    assert_eq!(
        decode::<LifecycleResponse>(&bytes)?,
        LifecycleResponse::Subscribed
    );
    fixture.handle.dispatch_window(
        &retired,
        CorrelationId(4),
        &encode(&WindowRequest::CloseReply {
            window,
            close_seq: 2,
            verdict: CloseVerdict::Allow,
        })?,
    )?;
    verdict(&fixture, window, 2, CloseVerdict::Veto, 5)?;
    // No stale caller reply or additional gap facts may precede this reply.
    reply(&mut fixture, 5, WindowResponse::ReplyApplied)?;
    assert!(fixture.commands.try_recv().is_err());
    fixture.retire_reader(2)?;
    Ok(())
}

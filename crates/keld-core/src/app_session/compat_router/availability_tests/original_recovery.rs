//! Actual Core writer/subscriptions and native lease owner with modeled callbacks.
//! Runtime authentication and real `WebKit` effects remain separate native gates.

use super::{active_caller, availability_session};
use crate::app_session::window_registry::WindowOwner;
use crate::app_session::window_router::WindowSession;
use keld_ipc::codec::decode;
use keld_ipc::compat_control::{self, CompatControlEvent};
use keld_ipc::link::{AppLinkDeadlines as _, read_frame};
use keld_ipc::window::WindowEvent;
use keld_ipc::{CorrelationId, FrameKind, LifecycleEvent, LifecycleResponse};
use keld_wv::wkwebview::RendererBridgeOutcome;
use std::sync::{Arc, Mutex};
use std::time::Duration;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

fn subscribed(peer: &mut crate::app_session::BootstrapStream, correlation: u32) -> TestResult {
    let (header, payload) = read_frame(peer)?;
    assert_eq!(
        (header.kind, header.channel, header.corr),
        (
            FrameKind::Reply,
            keld_ipc::LIFECYCLE_CHANNEL,
            CorrelationId(correlation)
        )
    );
    assert_eq!(
        decode::<LifecycleResponse>(&payload)?,
        LifecycleResponse::Subscribed
    );
    Ok(())
}

fn initial_ready(
    handle: &crate::app_session::PrimaryRouterHandle,
    peer: &mut crate::app_session::BootstrapStream,
    guardian: std::sync::mpsc::Receiver<crate::app_session::GuardianOwnerCommand>,
) -> TestResult<std::sync::mpsc::Receiver<crate::app_session::GuardianOwnerCommand>> {
    // Service the actual existing owner command/receipt, as the neighboring
    // ready_recovery component does. Never set recovery_armed or fake Ready.
    let shutdown = handle.shutdown.clone();
    let ready = Arc::clone(&handle.window_ready);
    let owner_receipt = std::thread::spawn(move || {
        let crate::app_session::GuardianOwnerCommand::ArmRecovery(
            crate::app_session::RecoveryArm::Initial(1),
            receipt,
        ) = guardian
            .recv_timeout(Duration::from_secs(3))
            .expect("actual initial arm request")
        else {
            panic!("initial Ready must request this owner's recovery arm");
        };
        let transition = shutdown.transition_guard();
        assert!(ready.load(std::sync::atomic::Ordering::Acquire));
        receipt.send(Ok(())).expect("actual owner receipt");
        drop(transition);
        guardian
    });
    let ready_result = handle.signal_ready();
    let guardian = owner_receipt
        .join()
        .map_err(|_| "owner receipt thread panicked")?;
    ready_result?;
    let (_, payload) = read_frame(peer)?;
    assert_eq!(decode::<LifecycleEvent>(&payload)?, LifecycleEvent::Ready);
    Ok(guardian)
}

fn initial_route(
    handle: &crate::app_session::PrimaryRouterHandle,
    peer: &mut crate::app_session::BootstrapStream,
    outcomes: &std::sync::mpsc::Receiver<RendererBridgeOutcome>,
    old: &crate::app_session::PrimaryCallerContext,
) -> TestResult<keld_wv::wkwebview::RendererRoleWitness> {
    handle.subscribe_compat(
        old,
        CorrelationId(11),
        keld_ipc::channel_table::COMPAT_CONTROL.wire_id(),
    )?;
    let (header, payload) = read_frame(peer)?;
    assert_eq!(
        (header.kind, header.channel),
        (
            FrameKind::Event,
            keld_ipc::channel_table::COMPAT_CONTROL.id()
        )
    );
    assert!(matches!(
        compat_control::decode_event(&payload)?,
        CompatControlEvent::DocumentBound { .. }
    ));
    subscribed(peer, 11)?;
    let RendererBridgeOutcome::RouteState {
        ready: true,
        role: old_role,
        ..
    } = outcomes.recv_timeout(Duration::from_secs(3))?
    else {
        return Err("initial route availability missing".into());
    };
    assert!(old_role.matches(&old.reader_stop));
    Ok(old_role)
}

fn install_successor(
    handle: &crate::app_session::PrimaryRouterHandle,
    peer: &mut crate::app_session::BootstrapStream,
) -> TestResult<(
    crate::app_session::PrimaryRouterHandle,
    std::sync::mpsc::Receiver<crate::app_session::GuardianOwnerCommand>,
    std::sync::mpsc::Receiver<keld_wv::AppWindowCommand>,
)> {
    let (next_handle, next_peer, next_guardian, next_commands) =
        crate::app_session::tests::initial_ready_router_with_window_loop();
    let writer = next_handle
        .current
        .lock()
        .map_err(|_| "next current")?
        .take()
        .ok_or("next role")?
        .writer;
    *peer = next_peer;
    peer.set_app_link_deadlines(Some(Duration::from_secs(3)))?;
    handle
        .install_generation(2, writer)
        .map_err(|failure| failure.error)?;
    let (_, payload) = read_frame(peer)?;
    assert_eq!(decode::<LifecycleEvent>(&payload)?, LifecycleEvent::Ready);
    Ok((next_handle, next_guardian, next_commands))
}

fn recovered_snapshot(
    handle: &crate::app_session::PrimaryRouterHandle,
    peer: &mut crate::app_session::BootstrapStream,
    outcomes: &std::sync::mpsc::Receiver<RendererBridgeOutcome>,
    current: &crate::app_session::PrimaryCallerContext,
    window: keld_ipc::window::WindowRef,
    native: &keld_wv::wkwebview::MacNativeDocumentFixture,
    early_compat: bool,
) -> TestResult {
    let owner = WindowOwner::new(current.identity, current.attempt);
    let document = native.document();
    let stamp = document.stamp();
    if early_compat {
        handle.subscribe_compat(
            current,
            CorrelationId(20),
            keld_ipc::channel_table::COMPAT_CONTROL.wire_id(),
        )?;
        subscribed(peer, 20)?;
        assert!(
            outcomes.try_recv().is_err(),
            "no Window Subscribe means no document binding or availability"
        );
    }
    handle.subscribe_window(
        current,
        CorrelationId(21),
        keld_ipc::channel_table::WINDOW.wire_id(),
    )?;
    let (header, payload) = read_frame(peer)?;
    assert_eq!(
        (header.kind, header.channel),
        (FrameKind::Event, keld_ipc::channel_table::WINDOW.id())
    );
    assert!(
        matches!(decode::<WindowEvent>(&payload)?, WindowEvent::Created { window: replayed, .. } if replayed == window)
    );
    let (header, payload) = read_frame(peer)?;
    assert_eq!(
        (header.kind, header.channel),
        (FrameKind::Event, keld_ipc::channel_table::WINDOW.id())
    );
    assert_eq!(
        decode::<WindowEvent>(&payload)?,
        WindowEvent::DocumentRecovered {
            window,
            document_generation: stamp.observed_generation(),
            native_url: native.native_url()?.as_str().to_owned(),
        }
    );
    subscribed(peer, 21)?;
    assert!(
        handle
            .windows
            .as_ref()
            .ok_or("windows")?
            .lock()
            .map_err(|_| "windows lock")?
            .renderer_route(owner, stamp.view(), None)
            .is_err(),
        "snapshot does not grant routing"
    );
    assert!(outcomes.try_recv().is_err());
    Ok(())
}

fn assert_revoked(
    handle: &crate::app_session::PrimaryRouterHandle,
    peer: &mut crate::app_session::BootstrapStream,
    outcomes: &std::sync::mpsc::Receiver<RendererBridgeOutcome>,
    owner: WindowOwner,
    view: keld_wv::WebviewId,
) -> TestResult {
    subscribed(peer, 22)?;
    assert!(
        outcomes.try_recv().is_err(),
        "revoked page has no DocumentBound or native availability"
    );
    assert!(
        handle
            .windows
            .as_ref()
            .ok_or("windows")?
            .lock()
            .map_err(|_| "windows lock")?
            .renderer_route(owner, view, None)
            .is_err()
    );
    Ok(())
}

fn assert_rebound(
    handle: &crate::app_session::PrimaryRouterHandle,
    peer: &mut crate::app_session::BootstrapStream,
    outcomes: &std::sync::mpsc::Receiver<RendererBridgeOutcome>,
    current: &crate::app_session::PrimaryCallerContext,
    old: &crate::app_session::PrimaryCallerContext,
    window: keld_ipc::window::WindowRef,
    native: &keld_wv::wkwebview::MacNativeDocumentFixture,
) -> TestResult {
    let document = native.document();
    let stamp = document.stamp();
    let generation = stamp.observed_generation();
    let owner = WindowOwner::new(current.identity, current.attempt);
    let (header, payload) = read_frame(peer)?;
    assert_eq!(
        (header.kind, header.channel),
        (
            FrameKind::Event,
            keld_ipc::channel_table::COMPAT_CONTROL.id()
        )
    );
    assert_eq!(
        compat_control::decode_event(&payload)?,
        CompatControlEvent::DocumentBound {
            webview: stamp.view().0,
            navigation: generation,
            window_id: window.id.0,
            window_incarnation: window.incarnation.0,
        }
    );
    subscribed(peer, 22)?;
    let RendererBridgeOutcome::RouteState {
        webview,
        navigation,
        ready: true,
        role,
    } = outcomes.try_recv()?
    else {
        return Err("fresh successor availability missing".into());
    };
    assert_eq!((webview, navigation), (stamp.view(), u64::from(generation)));
    assert!(role.matches(&current.reader_stop));
    assert!(!role.matches(&old.reader_stop));
    let rebound = handle
        .windows
        .as_ref()
        .ok_or("windows")?
        .lock()
        .map_err(|_| "windows lock")?
        .renderer_route(owner, stamp.view(), None)?;
    assert!(rebound.document.same_binding(&document));
    assert!(
        outcomes.try_recv().is_err(),
        "one new binding emits one availability"
    );
    Ok(())
}

fn exercise(early_compat: bool, revoke_after_snapshot: bool) -> TestResult {
    let (mut handle, mut peer, guardian, _commands) =
        crate::app_session::tests::initial_ready_router_with_window_loop();
    peer.set_app_link_deadlines(Some(Duration::from_secs(3)))?;
    handle.windows = Some(Arc::new(Mutex::new(WindowSession::new(true))));
    let (compat, outcomes) = availability_session();
    handle.compat = Some(Arc::new(Mutex::new(compat)));
    let old = active_caller(&handle);
    handle.subscribe_window(
        &old,
        CorrelationId(10),
        keld_ipc::channel_table::WINDOW.wire_id(),
    )?;
    subscribed(&mut peer, 10)?;
    let _guardian = initial_ready(&handle, &mut peer, guardian)?;
    let (_owned, mut native, window, context) = handle
        .windows
        .as_ref()
        .ok_or("windows")?
        .lock()
        .map_err(|_| "windows lock")?
        .fixture_original_recovery_document(WindowOwner::new(old.identity, old.attempt))?;
    assert_eq!(context.view(), native.document().stamp().view());
    let old_role = initial_route(&handle, &mut peer, &outcomes, &old)?;
    handle.retire_generation(1)?;
    assert!(!old_role.is_live());
    assert!(matches!(
        outcomes.try_recv()?,
        RendererBridgeOutcome::RouteState { ready: false, .. }
    ));
    let (_next_handle, _next_guardian, _next_commands) = install_successor(&handle, &mut peer)?;
    let current = active_caller(&handle);
    let owner = WindowOwner::new(current.identity, current.attempt);
    recovered_snapshot(
        &handle,
        &mut peer,
        &outcomes,
        &current,
        window,
        &native,
        early_compat,
    )?;
    if revoke_after_snapshot {
        native.revoke();
    }
    handle.subscribe_compat(
        &current,
        CorrelationId(22),
        keld_ipc::channel_table::COMPAT_CONTROL.wire_id(),
    )?;
    if revoke_after_snapshot {
        assert_revoked(&handle, &mut peer, &outcomes, owner, context.view())?;
    } else {
        assert_rebound(
            &handle, &mut peer, &outcomes, &current, &old, window, &native,
        )?;
    }
    handle.retire_generation(2)?;
    if let Some(reader) = handle
        .readers
        .lock()
        .map_err(|_| "readers lock")?
        .remove(&2)
    {
        reader.join().map_err(|_| "component reader panicked")??;
    }
    Ok(())
}

#[test]
fn recovery_rebind_publishes_snapshot_then_binding_reply_and_fresh_role_availability() -> TestResult
{
    exercise(false, false)
}

#[test]
fn early_compat_then_window_requires_explicit_repeat_and_new_binding_availability() -> TestResult {
    exercise(true, false)
}

#[test]
fn recovery_snapshot_does_not_rebind_a_subsequently_revoked_native_document() -> TestResult {
    exercise(false, true)
}

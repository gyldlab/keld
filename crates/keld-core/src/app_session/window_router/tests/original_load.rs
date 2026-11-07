//! Actual Core handler/registry/ordered socket proof. Component window creation
//! and preflight inputs do not prove Runtime authentication, native milestones,
//! source qualification or original-app success. Opaque attempts are never forged.
use super::{
    Fixture, TestResult, fact, install_successor, native_state, owner_from, reply, snapshot,
};
use crate::app_session::window_registry::OriginalLoadAdmission;
use keld_ipc::codec::decode;
use keld_ipc::link::read_frame;
use keld_ipc::window::{WindowEvent, WindowRef, WindowRequest, WindowResponse};
use keld_ipc::{CallError, CorrelationId, FrameKind, LifecycleResponse};
use keld_wv::{
    AppWindowCommand, AppWindowEvent, MacOriginalNavigationFact, NativeNavigationContext,
    NativeNavigationRefusal,
};
use std::num::NonZeroU64;

fn capture(fixture: &Fixture, window: WindowRef, corr: u32) -> TestResult<NativeNavigationContext> {
    let owner = owner_from(fixture)?;
    let mut session = fixture
        .handle
        .windows
        .as_ref()
        .ok_or_else(|| std::io::Error::other("window owner"))?
        .lock()
        .map_err(|_| std::io::Error::other("window lock poisoned"))?;
    let OriginalLoadAdmission::Dispatch {
        operation, view, ..
    } = session
        .registry
        .admit_original_load(owner, CorrelationId(corr), window)?
    else {
        return Err(std::io::Error::other("expected one fresh original call").into());
    };
    Ok(NativeNavigationContext::new(
        NonZeroU64::new(operation).ok_or_else(|| std::io::Error::other("checked operation"))?,
        view,
    ))
}

#[test]
fn missing_original_source_and_oversize_preflight_are_correlated_nonfatal_without_native_work()
-> TestResult {
    let mut fixture = Fixture::new(Some(true))?;
    fixture.subscribe()?;
    fixture.ready()?;
    let window = fixture.build(1, 700)?;
    for (corr, uri) in [
        (20, String::new()),
        (21, "file:///unadmitted/index.html".to_owned()),
        (22, "x".repeat(8193)),
    ] {
        fixture.call(&WindowRequest::LoadUrl { window, uri }, corr)?;
        fixture.error(corr, "KELD-CORE-051")?;
        assert!(
            fixture.commands.try_recv().is_err(),
            "preflight refusal enqueues no native command"
        );
    }
    fixture.call(
        &WindowRequest::SetMaximized {
            window,
            maximized: true,
        },
        23,
    )?;
    assert!(
        matches!(fixture.commands.try_recv()?, AppWindowCommand::SetMaximized { view, maximized: true } if view.0 == 700)
    );
    reply(&mut fixture, 23, WindowResponse::Accepted)?;
    Ok(())
}

#[test]
fn captured_native_preflight_refusal_settles_only_its_original_call_and_preserves_writer()
-> TestResult {
    let mut fixture = Fixture::new(Some(true))?;
    fixture.subscribe()?;
    fixture.ready()?;
    let window = fixture.build(1, 700)?;
    let context = capture(&fixture, window, 20)?;
    let foreign = NativeNavigationContext::new(
        NonZeroU64::new(context.operation().get() + 1).expect("next"),
        context.view(),
    );
    fixture
        .handle
        .window_native_event(AppWindowEvent::MacOriginalNavigation {
            context: foreign,
            fact: MacOriginalNavigationFact::Refused {
                error: NativeNavigationRefusal::UnknownView,
            },
        })?;
    fixture
        .handle
        .window_native_event(AppWindowEvent::MacOriginalNavigation {
            context,
            fact: MacOriginalNavigationFact::Refused {
                error: NativeNavigationRefusal::MissingSource,
            },
        })?;
    fixture.error(20, "KELD-CORE-051")?;
    assert!(fixture.commands.try_recv().is_err());
    fixture.call(
        &WindowRequest::SetMaximized {
            window,
            maximized: false,
        },
        21,
    )?;
    assert!(
        matches!(fixture.commands.try_recv()?, AppWindowCommand::SetMaximized { view, maximized: false } if view.0 == 700)
    );
    reply(&mut fixture, 21, WindowResponse::Accepted)?;
    Ok(())
}

#[test]
fn queued_old_context_cannot_reply_to_successor_reusing_numeric_app_correlation() -> TestResult {
    let mut fixture = Fixture::new(Some(true))?;
    fixture.subscribe()?;
    fixture.ready()?;
    let window = fixture.build(1, 700)?;
    let old = capture(&fixture, window, 20)?;
    fixture.retire_reader(1)?;
    install_successor(&mut fixture)?;
    fixture.handle.subscribe_window(
        &fixture.caller()?,
        CorrelationId(11),
        keld_ipc::channel_table::WINDOW.wire_id(),
    )?;
    fact(
        &mut fixture,
        &WindowEvent::Created {
            window,
            state: native_state(snapshot()),
            pending_close: None,
        },
    )?;
    let (header, bytes) = read_frame(&mut fixture.peer)?;
    assert_eq!(header.kind, FrameKind::Reply);
    assert_eq!(
        decode::<LifecycleResponse>(&bytes)?,
        LifecycleResponse::Subscribed
    );
    let current = capture(&fixture, window, 20)?;
    assert_ne!(old.operation(), current.operation());
    fixture
        .handle
        .window_native_event(AppWindowEvent::MacOriginalNavigation {
            context: old,
            fact: MacOriginalNavigationFact::Refused {
                error: NativeNavigationRefusal::InvalidQuery,
            },
        })?;
    fixture
        .handle
        .window_native_event(AppWindowEvent::MacOriginalNavigation {
            context: current,
            fact: MacOriginalNavigationFact::Refused {
                error: NativeNavigationRefusal::MissingSource,
            },
        })?;
    let (header, bytes) = read_frame(&mut fixture.peer)?;
    assert_eq!(
        (header.kind, header.corr),
        (FrameKind::Err, CorrelationId(20))
    );
    let error = decode::<CallError>(&bytes)?;
    assert_eq!(error.code, "KELD-CORE-051");
    assert!(
        error
            .message
            .contains("verified native source is unavailable"),
        "only B's exact observed refusal reaches B"
    );
    assert!(!error.message.contains("original query was rejected"));
    fixture.call(
        &WindowRequest::SetFullScreen {
            window,
            full_screen: true,
        },
        21,
    )?;
    assert!(
        matches!(fixture.commands.try_recv()?, AppWindowCommand::SetFullScreen { view, full_screen: true } if view.0 == 700)
    );
    reply(&mut fixture, 21, WindowResponse::Accepted)?;
    fixture.retire_reader(2)?;
    Ok(())
}

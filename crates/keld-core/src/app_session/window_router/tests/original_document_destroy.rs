//! Actual Core writer/registry proof with state-only modeled native decisions.
//! No forged lease, real native admission, Runtime auth or physical OS proof.
use super::{Fixture, TestResult, fact, owner_from, reply};
use keld_ipc::CorrelationId;
use keld_ipc::codec::decode;
use keld_ipc::link::read_frame;
use keld_ipc::window::{WindowEvent, WindowRequest, WindowResponse};
use keld_ipc::{FrameKind, LIFECYCLE_CHANNEL, LifecycleEvent};
use keld_wv::wkwebview::OriginalDocumentDestroyAdmission;
use keld_wv::{AppWindowEvent, WebviewId};
use std::sync::Arc;

#[test]
fn original_document_destroy_waits_for_release_and_keeps_fact_reply_order() -> TestResult {
    let mut fixture = Fixture::new(Some(true))?;
    fixture.subscribe()?;
    fixture.ready()?;
    let window = fixture.build(1, 700)?;
    let owner = owner_from(&fixture)?;
    let admission = Arc::new(OriginalDocumentDestroyAdmission::fixture_accepted());
    fixture
        .handle
        .windows
        .as_ref()
        .expect("registry")
        .lock()
        .map_err(|_| std::io::Error::other("registry lock poisoned"))?
        .registry
        .fixture_original_destroy_decision(
            owner,
            CorrelationId(20),
            window,
            Arc::clone(&admission),
        )?;
    fixture
        .handle
        .window_native_event(AppWindowEvent::OriginalDocumentDestroyDecision {
            view: WebviewId(700),
            admission,
        })?;
    assert!(
        fixture.commands.try_recv().is_err(),
        "native admission already owns physical release"
    );
    // The next exact reply fences premature scope success without a sleep/read timeout.
    fixture.call(&WindowRequest::RequestClose { window }, 21)?;
    reply(&mut fixture, 21, WindowResponse::AlreadyClosing)?;
    fixture
        .handle
        .window_native_event(AppWindowEvent::WindowReleased {
            view: WebviewId(700),
        })?;
    fact(&mut fixture, &WindowEvent::Destroyed { window })?;
    let (header, bytes) = read_frame(&mut fixture.peer)?;
    assert_eq!(
        (header.kind, header.channel),
        (FrameKind::Event, LIFECYCLE_CHANNEL)
    );
    assert_eq!(
        decode::<LifecycleEvent>(&bytes)?,
        LifecycleEvent::LastWindowClosed
    );
    reply(&mut fixture, 20, WindowResponse::Destroyed)?;
    Ok(())
}

#[test]
fn original_document_destroy_missing_source_is_nonfatal_and_generic_destroy_unchanged() -> TestResult
{
    let mut fixture = Fixture::new(Some(true))?;
    fixture.subscribe()?;
    fixture.ready()?;
    let window = fixture.build(1, 700)?;
    fixture.call(
        &WindowRequest::DestroyOriginalDocument {
            window,
            document_generation: 2,
        },
        20,
    )?;
    fixture.error(20, "KELD-CORE-054")?;
    assert!(fixture.commands.try_recv().is_err());
    fixture.call(&WindowRequest::RequestClose { window }, 21)?;
    let (header, _) = read_frame(&mut fixture.peer)?;
    assert_eq!(header.kind, FrameKind::Event);
    reply(
        &mut fixture,
        21,
        WindowResponse::CloseRequested { close_seq: 1 },
    )?;
    Ok(())
}

#[test]
fn original_document_destroy_is_cancelled_before_current_take_interleaving() -> TestResult {
    let mut fixture = Fixture::new(Some(true))?;
    fixture.subscribe()?;
    fixture.ready()?;
    let window = fixture.build(1, 700)?;
    let owner = owner_from(&fixture)?;
    let admission = Arc::new(OriginalDocumentDestroyAdmission::new());
    fixture
        .handle
        .windows
        .as_ref()
        .expect("registry")
        .lock()
        .map_err(|_| std::io::Error::other("registry lock poisoned"))?
        .registry
        .reserve_original_document_destroy(
            owner,
            CorrelationId(20),
            window,
            Arc::clone(&admission),
        )?;
    // A state-only modeled UI claim runs at the exact point after current.take.
    // If the early cancellation is removed, late role_lost reconciliation alone
    // still prevents adoption but this forbidden old-role admission must fail.
    fixture
        .handle
        .quit_drain_hooks
        .lock()
        .map_err(|_| std::io::Error::other("retirement hook lock poisoned"))?
        .native_admission_after_take = Some(Arc::clone(&admission));
    fixture.retire_reader(1)?;
    assert_eq!(
        admission.decision(),
        keld_wv::wkwebview::OriginalDocumentDestroyDecision::Cancelled
    );
    assert!(
        fixture
            .handle
            .current
            .lock()
            .map_err(|_| std::io::Error::other("current lock poisoned"))?
            .is_none()
    );
    Ok(())
}

#[test]
fn poisoned_registry_cancels_native_ticket_before_current_take_and_keeps_failure() -> TestResult {
    use std::io::Read as _;
    use std::sync::atomic::Ordering;
    let mut fixture = Fixture::new(Some(true))?;
    fixture.subscribe()?;
    fixture.ready()?;
    let window = fixture.build(1, 700)?;
    let owner = owner_from(&fixture)?;
    let caller = fixture.caller()?;
    let admission = Arc::new(OriginalDocumentDestroyAdmission::new());
    let windows = Arc::clone(fixture.handle.windows.as_ref().expect("Registry owner"));
    windows
        .lock()
        .map_err(|_| std::io::Error::other("Registry initially unpoisoned"))?
        .registry
        .reserve_original_document_destroy(
            owner,
            CorrelationId(20),
            window,
            Arc::clone(&admission),
        )?;
    fixture
        .handle
        .quit_drain_hooks
        .lock()
        .map_err(|_| std::io::Error::other("retirement hook lock poisoned"))?
        .native_admission_after_take = Some(Arc::clone(&admission));
    let poison = Arc::clone(&windows);
    assert!(
        std::thread::spawn(move || {
            let _held = poison.lock().expect("controlled Registry poison guard");
            panic!("controlled poison with actual retained Pending ticket");
        })
        .join()
        .is_err()
    );
    let error = fixture
        .handle
        .retire_generation(1)
        .expect_err("poison must remain a reported failure");
    let detail = error.to_string();
    assert!(
        detail.contains("window native admission retirement"),
        "prelude cause lost: {detail}"
    );
    assert!(
        detail.contains("window role retirement"),
        "later cleanup cause lost: {detail}"
    );
    // Existing hook tries native's state-only CAS immediately after current.take.
    // Ignoring the prelude's error without cancelling the retained ticket would
    // make that old-role attempt Accepted here, even if link cleanup succeeded.
    assert_eq!(
        admission.decision(),
        keld_wv::wkwebview::OriginalDocumentDestroyDecision::Cancelled
    );
    assert!(
        fixture
            .handle
            .current
            .lock()
            .map_err(|_| std::io::Error::other("current lock poisoned"))?
            .is_none()
    );
    assert!(caller.reader_stop.load(Ordering::Acquire));
    assert!(
        windows.is_poisoned() && windows.lock().is_err(),
        "revocation cannot make the Registry admissible again"
    );
    assert_eq!(
        fixture.peer.read(&mut [0])?,
        0,
        "actual retired app link must close"
    );
    Ok(())
}

//! Core state-transition tests using explicit model native-ticket observations.
//! The existing built-window fixture and state-only tickets prove no OS effect,
//! opaque lease admission or Runtime authentication; native tests own those cuts.
use super::super::{
    PendingOriginalDocumentDestroy, PendingWindowCall, WindowError, WindowOwner, WindowRegistry,
};
use super::{TestResult, complete, owner, phase_name, ready};
use keld_ipc::CorrelationId;
use keld_ipc::window::WindowRef;
use keld_wv::WebviewId;
use keld_wv::wkwebview::{OriginalDocumentDestroyAdmission, OriginalDocumentDestroyDecision};
use std::sync::Arc;

impl WindowRegistry {
    /// Model a native terminal ticket already visible before its queued event.
    /// This helper exists only in this Core test module; it cannot mint a lease.
    pub(in crate::app_session) fn fixture_original_destroy_decision(
        &mut self,
        owner: WindowOwner,
        correlation: CorrelationId,
        window: WindowRef,
        admission: Arc<OriginalDocumentDestroyAdmission>,
    ) -> Result<(), WindowError> {
        self.checked_entry(owner, window)?;
        self.entries
            .get_mut(&window.id.0)
            .ok_or(WindowError::StaleWindow)?
            .original_destroy = Some(PendingOriginalDocumentDestroy {
            call: Some(PendingWindowCall { owner, correlation }),
            admission,
        });
        Ok(())
    }
}

#[test]
fn original_document_destroy_cancel_wins_before_retirement_without_closing_successor() -> TestResult
{
    let mut registry = ready();
    let window = complete(&mut registry, owner(1), 1, 700)?;
    let admission = Arc::new(OriginalDocumentDestroyAdmission::new());
    assert_eq!(
        registry.reserve_original_document_destroy(
            owner(1),
            CorrelationId(20),
            window,
            Arc::clone(&admission)
        )?,
        WebviewId(700)
    );
    assert!(registry.has_pending_call(owner(1), CorrelationId(20)));
    registry.cancel_native_admissions(owner(1), |_| {});
    assert_eq!(
        admission.decision(),
        OriginalDocumentDestroyDecision::Cancelled
    );
    registry.role_lost(owner(1), |_| {});
    registry.subscribe_successor(owner(2));
    assert_eq!(phase_name(registry.entries[&window.id.0].phase), 'P');
    assert_eq!(registry.replay(owner(2)).count(), 1);
    assert!(!registry.has_pending_call(owner(2), CorrelationId(20)));
    let outcome = registry.original_document_destroy_decision(WebviewId(700), &admission);
    assert!(
        outcome.refused.is_none(),
        "old caller gets no successor reply"
    );
    assert_eq!(registry.replay(owner(2)).count(), 1);
    Ok(())
}

#[test]
fn original_document_destroy_accepted_before_event_closes_before_owner_transfer() -> TestResult {
    let mut registry = ready();
    let window = complete(&mut registry, owner(1), 1, 700)?;
    let admission = Arc::new(OriginalDocumentDestroyAdmission::fixture_accepted());
    registry.fixture_original_destroy_decision(
        owner(1),
        CorrelationId(20),
        window,
        Arc::clone(&admission),
    )?;
    registry.cancel_native_admissions(owner(1), |_| {});
    assert_eq!(phase_name(registry.entries[&window.id.0].phase), 'C');
    registry.role_lost(owner(1), |_| {});
    registry.subscribe_successor(owner(2));
    assert_eq!(
        registry.replay(owner(2)).count(),
        0,
        "doomed physical view cannot be adopted"
    );
    registry.original_document_destroy_decision(WebviewId(700), &admission);
    let release = registry.native_released(WebviewId(700));
    assert!(!release.diagnostic);
    assert!(release.fact.is_none() && release.destroy_calls.is_empty());
    Ok(())
}

#[test]
fn original_document_destroy_refusal_preserves_live_close_and_new_load_metadata() -> TestResult {
    let mut registry = ready();
    let window = complete(&mut registry, owner(1), 1, 700)?;
    registry.request_close(owner(1), window)?;
    let new_load = registry.admit_original_load(owner(1), CorrelationId(30), window)?;
    let admission = Arc::new(OriginalDocumentDestroyAdmission::fixture_refused());
    registry.fixture_original_destroy_decision(
        owner(1),
        CorrelationId(20),
        window,
        Arc::clone(&admission),
    )?;
    let outcome = registry.original_document_destroy_decision(WebviewId(700), &admission);
    assert_eq!(
        outcome.refused.expect("exact old call").correlation,
        CorrelationId(20)
    );
    assert_eq!(phase_name(registry.entries[&window.id.0].phase), 'Q');
    assert_eq!(registry.entries[&window.id.0].last_close_seq, 1);
    assert!(registry.has_pending_call(owner(1), CorrelationId(30)));
    assert!(!registry.has_pending_call(owner(1), CorrelationId(20)));
    assert!(matches!(
        new_load,
        super::super::OriginalLoadAdmission::Dispatch { .. }
    ));
    Ok(())
}

#[test]
fn original_document_destroy_wrong_ticket_cannot_settle_the_captured_call() -> TestResult {
    let mut registry = ready();
    let window = complete(&mut registry, owner(1), 1, 700)?;
    let admitted = Arc::new(OriginalDocumentDestroyAdmission::new());
    registry.reserve_original_document_destroy(
        owner(1),
        CorrelationId(20),
        window,
        Arc::clone(&admitted),
    )?;
    let foreign = Arc::new(OriginalDocumentDestroyAdmission::fixture_accepted());
    registry.original_document_destroy_decision(WebviewId(700), &foreign);
    assert!(registry.has_pending_call(owner(1), CorrelationId(20)));
    assert_eq!(phase_name(registry.entries[&window.id.0].phase), 'P');
    assert!(admitted.cancel_before_admission());
    let outcome = registry.original_document_destroy_decision(WebviewId(700), &admitted);
    assert_eq!(
        outcome.refused.expect("own cancelled call").correlation,
        CorrelationId(20)
    );
    Ok(())
}

/// This marker tests Core's before-clear ordering only. It does not manufacture
/// the opaque native lease or qualify a positive Registered/availability effect.
fn mark_model_registration(
    registry: &mut WindowRegistry,
    window: WindowRef,
) -> Result<(), WindowError> {
    registry
        .entries
        .get_mut(&window.id.0)
        .ok_or(WindowError::StaleWindow)?
        .renderer
        .registered = true;
    Ok(())
}

#[test]
fn generic_destroy_runs_retirement_before_clearing_and_never_on_repeat() -> TestResult {
    let mut registry = ready();
    let window = complete(&mut registry, owner(1), 1, 700)?;
    mark_model_registration(&mut registry, window)?;
    let mut callbacks = 0;
    let outcome = registry.destroy_with_retirement(
        owner(1),
        window,
        CorrelationId(20),
        |_| {},
        |before| {
            callbacks += 1;
            let entry = &before.entries[&window.id.0];
            assert_eq!(phase_name(entry.phase), 'P');
            assert!(entry.owner_live && entry.owner == owner(1));
            assert!(
                entry.renderer.registered,
                "retirement must still name the old association"
            );
        },
    )?;
    assert!(matches!(outcome, super::super::CloseOutcome::Teardown(_)));
    assert_eq!(callbacks, 1);
    assert!(!registry.entries[&window.id.0].renderer.registered);
    assert_eq!(phase_name(registry.entries[&window.id.0].phase), 'C');
    let repeated = registry.destroy_with_retirement(
        owner(1),
        window,
        CorrelationId(21),
        |_| {},
        |_| callbacks += 1,
    )?;
    assert_eq!(repeated, super::super::CloseOutcome::AlreadyClosing);
    assert_eq!(
        callbacks, 1,
        "a repeated Destroy cannot retire another route"
    );
    let release = registry.native_released(WebviewId(700));
    assert!(matches!(
        release.fact,
        Some(super::super::OwnedWindowEvent {
            event: keld_ipc::window::WindowEvent::Destroyed { .. },
            ..
        })
    ));
    assert_eq!(release.last_window, Some(owner(1)));
    assert_eq!(
        release
            .destroy_calls
            .iter()
            .map(|call| call.correlation.0)
            .collect::<Vec<_>>(),
        vec![20, 21]
    );
    Ok(())
}

#[test]
fn refused_destroy_cannot_run_route_retirement_or_clear_registration() -> TestResult {
    let mut registry = ready();
    let window = complete(&mut registry, owner(1), 1, 700)?;
    mark_model_registration(&mut registry, window)?;
    let mut callbacks = 0;
    assert_eq!(
        registry.destroy_with_retirement(
            owner(2),
            window,
            CorrelationId(20),
            |_| {},
            |_| callbacks += 1
        ),
        Err(WindowError::WrongOwner)
    );
    registry
        .entries
        .get_mut(&window.id.0)
        .expect("built Entry")
        .destroy_calls = (0..super::super::MAX_PENDING_DESTROYS_PER_WINDOW)
        .map(|index| PendingWindowCall {
            owner: owner(1),
            correlation: CorrelationId(u32::try_from(index).expect("bounded fixture") + 100),
        })
        .collect();
    assert_eq!(
        registry.destroy_with_retirement(
            owner(1),
            window,
            CorrelationId(20),
            |_| {},
            |_| callbacks += 1
        ),
        Err(WindowError::PendingDestroyCapacity)
    );
    assert_eq!(callbacks, 0);
    assert!(registry.entries[&window.id.0].renderer.registered);
    assert_eq!(phase_name(registry.entries[&window.id.0].phase), 'P');
    Ok(())
}

#[test]
fn only_matching_close_allow_retires_before_the_entry_clears() -> TestResult {
    use super::super::CloseOutcome;
    use keld_ipc::window::CloseVerdict;
    let mut registry = ready();
    let window = complete(&mut registry, owner(1), 1, 700)?;
    mark_model_registration(&mut registry, window)?;
    let CloseOutcome::Requested { close_seq, .. } = registry.request_close(owner(1), window)?
    else {
        return Err(std::io::Error::other("fixture requires pending close").into());
    };
    let mut callbacks = 0;
    assert_eq!(
        registry.close_reply_with_retirement(
            owner(2),
            window,
            close_seq,
            CloseVerdict::Allow,
            |_| callbacks += 1
        ),
        Err(WindowError::WrongOwner)
    );
    assert_eq!(
        registry.close_reply_with_retirement(
            owner(1),
            window,
            close_seq + 1,
            CloseVerdict::Allow,
            |_| callbacks += 1
        )?,
        CloseOutcome::Stale
    );
    assert_eq!(
        registry.close_reply_with_retirement(
            owner(1),
            window,
            close_seq,
            CloseVerdict::Veto,
            |_| callbacks += 1
        )?,
        CloseOutcome::Vetoed
    );
    assert_eq!(callbacks, 0);
    assert!(registry.entries[&window.id.0].renderer.registered);
    let CloseOutcome::Requested { close_seq, .. } = registry.request_close(owner(1), window)?
    else {
        return Err(std::io::Error::other("fixture requires new pending close").into());
    };
    let outcome = registry.close_reply_with_retirement(
        owner(1),
        window,
        close_seq,
        CloseVerdict::Allow,
        |before| {
            callbacks += 1;
            let entry = &before.entries[&window.id.0];
            assert_eq!(phase_name(entry.phase), 'Q');
            assert_eq!(entry.last_close_seq, close_seq);
            assert!(
                entry.renderer.registered,
                "matching Allow must retire before clear"
            );
        },
    )?;
    assert!(matches!(outcome, CloseOutcome::Teardown(_)));
    assert_eq!(callbacks, 1);
    assert!(!registry.entries[&window.id.0].renderer.registered);
    assert_eq!(phase_name(registry.entries[&window.id.0].phase), 'C');
    Ok(())
}

#[test]
fn role_loss_clears_registration_and_subscription_cannot_renew_it() -> TestResult {
    let mut registry = ready();
    let window = complete(&mut registry, owner(1), 1, 700)?;
    mark_model_registration(&mut registry, window)?;
    registry.cancel_native_admissions(owner(1), |_| {});
    assert!(registry.entries[&window.id.0].renderer.registered);
    registry.role_lost(owner(1), |_| {});
    registry.subscribe_successor(owner(2));
    let entry = &registry.entries[&window.id.0];
    assert!(entry.owner_live && entry.owner == owner(2));
    assert!(!entry.renderer.registered);
    assert!(entry.renderer.document.is_none() && entry.renderer.committed.is_none());
    assert_eq!(
        registry.replay(owner(2)).count(),
        1,
        "window adoption does not adopt document routing"
    );
    Ok(())
}

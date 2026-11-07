//! N1 registry metadata contract; native milestones remain native-only evidence.
//! Existing registry fixtures model a built window only; these tests construct
//! NO native navigation attempt, stamp, Committed or Finished observation.
//! They prove admission/lifecycle metadata after owner adoption, not OS effects.

use super::super::{OriginalLoadAdmission, WindowCounter, WindowError};
use super::{TestResult, complete, owner, ready};
use keld_ipc::CorrelationId;
use keld_ipc::window::WindowGeneration;
use keld_wv::WebviewId;

#[test]
fn original_load_admission_retains_exact_call_and_refuses_a_second_pending_load() -> TestResult {
    let mut registry = ready();
    let window = complete(&mut registry, owner(1), 1, 7)?;
    let decision = registry.admit_original_load(owner(1), CorrelationId(20), window)?;
    let OriginalLoadAdmission::Dispatch {
        operation,
        window: admitted,
        view,
    } = decision
    else {
        return Err(std::io::Error::other("first admitted call must dispatch once").into());
    };
    assert_ne!(operation, 0);
    assert_eq!(admitted, window);
    assert_eq!(view, WebviewId(7));
    let context = registry
        .original_load_context(operation)
        .expect("one retained original call");
    assert_eq!(context.call.owner, owner(1));
    assert_eq!(context.call.correlation, CorrelationId(20));
    assert_eq!(context.window, window);
    assert_eq!(context.view, view);
    assert!(registry.has_pending_call(owner(1), CorrelationId(20)));
    assert_eq!(
        registry.admit_original_load(owner(1), CorrelationId(21), window)?,
        OriginalLoadAdmission::Busy
    );
    assert!(!registry.has_pending_call(owner(1), CorrelationId(21)));
    assert_eq!(
        registry
            .original_load_context(operation)
            .expect("busy cannot replace first call")
            .call
            .correlation,
        CorrelationId(20)
    );
    Ok(())
}

#[test]
fn retired_load_context_cannot_be_reassigned_to_a_successor_reused_correlation() -> TestResult {
    let mut registry = ready();
    let window = complete(&mut registry, owner(1), 1, 7)?;
    let OriginalLoadAdmission::Dispatch { operation: old, .. } =
        registry.admit_original_load(owner(1), CorrelationId(20), window)?
    else {
        return Err(std::io::Error::other("first admission").into());
    };
    registry.role_lost(owner(1), |_| {});
    assert!(registry.original_load_context(old).is_none());
    assert!(!registry.has_pending_call(owner(1), CorrelationId(20)));
    registry.subscribe_successor(owner(2));
    let OriginalLoadAdmission::Dispatch { operation: new, .. } =
        registry.admit_original_load(owner(2), CorrelationId(20), window)?
    else {
        return Err(std::io::Error::other("successor admission").into());
    };
    assert_ne!(
        old, new,
        "native operation context never aliases after role recovery"
    );
    assert!(registry.original_load_context(old).is_none());
    let context = registry
        .original_load_context(new)
        .expect("successor owns only new context");
    assert_eq!(context.call.owner, owner(2));
    assert_eq!(context.call.correlation, CorrelationId(20));
    assert_eq!(context.window, window);
    registry.session_end(|_| {});
    assert!(registry.original_load_context(new).is_none());
    assert!(!registry.has_pending_call(owner(2), CorrelationId(20)));
    Ok(())
}

#[test]
fn original_load_context_rejects_wrong_pair_owner_and_operation_exhaustion_without_admission()
-> TestResult {
    let mut registry = ready();
    let window = complete(&mut registry, owner(1), 1, 7)?;
    let mut stale = window;
    stale.incarnation = WindowGeneration(window.incarnation.0 + 1);
    assert_eq!(
        registry.admit_original_load(owner(1), CorrelationId(20), stale),
        Err(WindowError::StaleWindow)
    );
    assert_eq!(
        registry.admit_original_load(owner(2), CorrelationId(20), window),
        Err(WindowError::WrongOwner)
    );
    assert!(!registry.has_pending_call(owner(1), CorrelationId(20)));
    registry.last_original_load_operation = u64::MAX;
    assert_eq!(
        registry.admit_original_load(owner(1), CorrelationId(20), window),
        Err(WindowError::Exhausted(
            WindowCounter::OriginalLoadCorrelation
        ))
    );
    assert!(!registry.has_pending_call(owner(1), CorrelationId(20)));
    assert_eq!(registry.last_original_load_operation, u64::MAX);
    Ok(())
}

#[test]
fn renderer_mapping_uses_real_view_and_current_owner_without_consuming_adoption() -> TestResult {
    let mut registry = ready();
    let first = complete(&mut registry, owner(1), 1, 700)?;
    let second = complete(&mut registry, owner(1), 2, 701)?;
    assert_ne!(
        first.id.0, 700,
        "window identity is never the native view number"
    );
    assert_eq!(
        registry.admitted_renderer_window(owner(1), WebviewId(700)),
        Ok(first)
    );
    assert_eq!(
        registry.admitted_renderer_window(owner(1), WebviewId(701)),
        Ok(second)
    );
    assert_eq!(
        registry.admitted_renderer_window(owner(1), WebviewId(first.id.0)),
        Err(WindowError::StaleWindow)
    );
    assert_eq!(
        registry.admitted_renderer_window(owner(2), WebviewId(700)),
        Err(WindowError::WrongOwner)
    );
    registry.role_lost(owner(1), |_| {});
    assert_eq!(
        registry.admitted_renderer_window(owner(1), WebviewId(700)),
        Err(WindowError::WrongOwner)
    );
    registry.subscribe_successor(owner(2));
    assert_eq!(
        registry.admitted_renderer_window(owner(2), WebviewId(700)),
        Ok(first)
    );
    assert!(
        matches!(registry.create(owner(2), CorrelationId(30), 1, 1, None, None)?, super::super::CreateOutcome::Adopted(window) if window == first)
    );
    registry.destroy(owner(2), first, CorrelationId(31), |_| {})?;
    assert_eq!(
        registry.admitted_renderer_window(owner(2), WebviewId(700)),
        Err(WindowError::StaleWindow)
    );
    Ok(())
}

#[test]
fn original_load_clears_only_exact_dispatch_and_destroy_retires_logical_call() -> TestResult {
    let mut registry = ready();
    let window = complete(&mut registry, owner(1), 1, 7)?;
    let OriginalLoadAdmission::Dispatch { operation, .. } =
        registry.admit_original_load(owner(1), CorrelationId(20), window)?
    else {
        unreachable!()
    };
    registry.abandon_original_dispatch(operation + 1);
    assert!(registry.original_load_context(operation).is_some());
    registry.abandon_original_dispatch(operation);
    assert!(registry.original_load_context(operation).is_none());
    let OriginalLoadAdmission::Dispatch {
        operation: next, ..
    } = registry.admit_original_load(owner(1), CorrelationId(21), window)?
    else {
        unreachable!()
    };
    assert!(next > operation);
    registry.destroy(owner(1), window, CorrelationId(22), |_| {})?;
    assert!(registry.original_load_context(next).is_none());
    assert_eq!(
        registry
            .take_interrupted_original_load(window)
            .expect("router consumes original interruption")
            .correlation,
        CorrelationId(21)
    );
    assert!(!registry.has_pending_call(owner(1), CorrelationId(21)));
    Ok(())
}

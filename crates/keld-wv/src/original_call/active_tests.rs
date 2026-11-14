//! Pure call/send transitions; native inputs are modeled. This does not qualify
//! any Windows post or WK callback. Source custody uses the real file fixture.

#![allow(
    clippy::expect_used,
    reason = "Test fixture prerequisites must fail immediately before exercising active-call custody."
)]

use super::{ActiveOriginalCall, PendingOriginalSend};
use crate::WebviewId;
use crate::native_document::{NativeCommitIdentity, NativeDocumentOwner, VerifiedSourceId};
use crate::original_call::LocalCallBudget;
use crate::renderer_bridge::RendererRoleWitness;
use crate::renderer_contract::CheckedNativeDocumentLease;
use crate::resource_source::windows::tests::Fixture;
use std::num::NonZeroU64;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

fn committed(
    owner: &mut NativeDocumentOwner,
    source: VerifiedSourceId,
) -> CheckedNativeDocumentLease {
    let attempt = owner.begin_host_load(source).expect("modeled Host attempt");
    let stamp = owner
        .commit_admitted(
            attempt,
            source,
            NativeCommitIdentity(NonZeroU64::new(17).expect("nonzero modeled native identity")),
        )
        .expect("modeled native commit");
    CheckedNativeDocumentLease(owner.lease_for(stamp).expect("existing owner lease"))
}

fn bound(fixture: &Fixture) -> (NativeDocumentOwner, CheckedNativeDocumentLease) {
    let source = fixture.source_arc();
    let mut owner = NativeDocumentOwner::new(WebviewId(7));
    owner
        .bind_source(&source)
        .expect("same retained source binding");
    let lease = committed(&mut owner, source.source_id());
    (owner, lease)
}

#[test]
fn local_completion_requires_function_and_actual_native_receipt_in_both_orders() {
    let fixture = Fixture::new();
    let (_owner, lease) = bound(&fixture);
    for native_first in [false, true] {
        let mut budget = LocalCallBudget::default();
        budget.reserve(37).unwrap();
        let mut call =
            ActiveOriginalCall::entered(9, lease.clone(), "original".into(), None, 37, None);
        assert_eq!(call.id(), 9);
        assert!(call.lease().same_binding(&lease));
        assert_eq!(call.channel(), None);
        assert_eq!(call.local_bytes(), 37);
        assert!(!call.ready_to_release());
        assert!(call.observe_function_finished(10, "original").is_err());
        assert!(call.observe_function_finished(9, "successor").is_err());
        if native_first {
            assert!(call.observe_native_quiescence(9, &lease, true));
            assert!(!call.function_finished());
            assert!(call.native_finished());
        } else {
            call.observe_function_finished(9, "original").unwrap();
            assert!(call.function_finished());
            assert!(!call.native_finished());
        }
        assert!(
            !call.ready_to_release(),
            "one receipt cannot release the same retained charge"
        );
        assert_eq!(
            (budget.retained_records(), budget.retained_bytes()),
            (1, 37)
        );
        if native_first {
            call.observe_function_finished(9, "original").unwrap();
        } else {
            assert!(call.observe_native_quiescence(9, &lease, true));
        }
        assert!(call.ready_to_release());
        assert!(!call.needs_preflight_abort());
        // Existing matching duplicate terminal behavior remains idempotent.
        call.observe_function_finished(9, "original").unwrap();
        let bytes = call.local_bytes();
        drop(call);
        budget.release(bytes).unwrap();
        assert!(budget.is_empty());
    }
}

#[test]
fn effect_send_rechecks_exact_owner_captured_role_channel_and_unused_slot() {
    let fixture = Fixture::new();
    let (owner, lease) = bound(&fixture);
    let (foreign_owner, numerical_alias) = bound(&fixture);
    assert_eq!(lease.stamp(), numerical_alias.stamp());
    let stop = Arc::new(AtomicBool::new(false));
    let role = RendererRoleWitness::new(Arc::clone(&stop));
    let other_role = RendererRoleWitness::new(Arc::new(AtomicBool::new(false)));
    let mut call = ActiveOriginalCall::entered(
        9,
        lease.clone(),
        "original".into(),
        Some("save".into()),
        55,
        Some(role.clone()),
    );
    assert!(call.send_eligible(&owner, Some(&role), "original", "save"));
    assert!(!call.send_eligible(&owner, Some(&other_role), "original", "save"));
    assert!(!call.send_eligible(&foreign_owner, Some(&role), "original", "save"));
    assert!(!call.send_eligible(&owner, None, "original", "save"));
    assert!(!call.send_eligible(&owner, Some(&role), "successor", "save"));
    assert!(!call.send_eligible(&owner, Some(&role), "original", "export"));
    stop.store(true, Ordering::Release);
    assert!(!call.send_eligible(&owner, Some(&role), "original", "save"));
    // Retirement is irreversible; use a fresh original record/role for healthy handoff.
    let healthy = RendererRoleWitness::new(Arc::new(AtomicBool::new(false)));
    call = ActiveOriginalCall::entered(
        10,
        lease.clone(),
        "original".into(),
        Some("save".into()),
        55,
        Some(healthy.clone()),
    );
    assert!(call.send_eligible(&owner, Some(&healthy), "original", "save"));
    call.send_handed_off().unwrap();
    assert!(call.send_forwarded());
    assert!(!call.send_eligible(&owner, Some(&healthy), "original", "save"));
    assert!(call.send_handed_off().is_err());
    call.observe_function_finished(10, "original").unwrap();
    assert!(call.observe_native_quiescence(10, &lease, true));
    assert!(call.ready_to_release());
    assert!(
        !call.needs_preflight_abort(),
        "actual forwarded Send owns its existing terminal correlation"
    );
}

#[test]
fn unforwarded_effect_needs_both_receipts_and_original_preflight_abort() {
    let fixture = Fixture::new();
    let (_owner, lease) = bound(&fixture);
    let role = RendererRoleWitness::new(Arc::new(AtomicBool::new(false)));
    let mut call = ActiveOriginalCall::entered(
        9,
        lease.clone(),
        "original".into(),
        Some("save".into()),
        55,
        Some(role),
    );
    call.observe_function_finished(9, "original").unwrap();
    assert!(!call.ready_to_release());
    assert!(!call.needs_preflight_abort());
    assert!(call.observe_native_quiescence(9, &lease, true));
    assert!(call.ready_to_release());
    assert!(call.needs_preflight_abort());
    assert!(!call.send_forwarded());
    // The existing endpoint must accept PreflightAborted before the adapter can
    // remove this exact record. Its retained charge is still the original one.
    assert_eq!(call.local_bytes(), 55);
    assert!(call.needs_preflight_abort());
}

#[test]
fn failure_or_revocation_abandons_function_only_after_original_native_receipt() {
    let fixture = Fixture::new();
    for revoke in [false, true] {
        let (mut owner, lease) = bound(&fixture);
        let (_foreign, other) = bound(&fixture);
        assert_eq!(lease.stamp(), other.stamp());
        let mut call =
            ActiveOriginalCall::entered(9, lease.clone(), "original".into(), None, 37, None);
        if revoke {
            owner.destroy();
        }
        assert!(!call.function_finished());
        assert!(!call.native_finished());
        assert!(!call.ready_to_release());
        assert!(!call.observe_native_quiescence(10, &lease, false));
        assert!(!call.observe_native_quiescence(9, &other, false));
        assert!(!call.ready_to_release());
        assert!(call.observe_native_quiescence(9, &lease, revoke));
        assert!(call.native_finished());
        assert!(call.function_finished());
        assert!(call.ready_to_release());
        call.observe_function_finished(9, "original").unwrap();
        assert!(call.observe_function_finished(9, "successor").is_err());
    }
}

#[test]
fn refusal_before_entry_still_holds_native_completion_and_original_bytes() {
    let fixture = Fixture::new();
    let (_owner, lease) = bound(&fixture);
    let mut call =
        ActiveOriginalCall::refused_before_entry(9, lease.clone(), "original".into(), 37);
    assert!(
        call.function_finished(),
        "explicit no-entry transition, not a Function receipt"
    );
    assert!(!call.native_finished());
    assert!(!call.ready_to_release());
    assert_eq!(call.local_bytes(), 37);
    assert_eq!(call.channel(), None);
    assert!(call.send_handed_off().is_err());
    assert!(call.observe_native_quiescence(9, &lease, false));
    assert!(call.ready_to_release());
    assert!(!call.needs_preflight_abort());
    assert_eq!(call.local_bytes(), 37);
}

#[test]
fn pending_send_result_keeps_original_generation_and_id_after_successor_commit() {
    let fixture = Fixture::new();
    let (mut owner, lease) = bound(&fixture);
    let generation = u64::from(lease.stamp().observed_generation());
    let pending = PendingOriginalSend::after_handoff(11, lease.clone());
    assert_eq!(pending.id(), 11);
    assert!(pending.lease().same_binding(&lease));
    assert!(pending.matches_result(generation, 11));
    assert!(!pending.matches_result(generation, 12));
    let successor = committed(&mut owner, fixture.source_arc().source_id());
    assert!(!pending.lease().is_current());
    assert!(successor.is_current());
    assert!(!pending.matches_result(u64::from(successor.stamp().observed_generation()), 11));
    assert!(
        pending.matches_result(generation, 11),
        "original cleanup does not acquire the successor"
    );
}
#[test]
fn windows_post_return_never_substitutes_for_function_even_after_revoke() {
    let fixture = Fixture::new();
    let (mut owner, lease) = bound(&fixture);
    let (_foreign, other) = bound(&fixture);
    let mut call = ActiveOriginalCall::entered(9, lease.clone(), "original".into(), None, 37, None);
    owner.destroy();
    assert!(!call.observe_native_return(10, &lease));
    assert!(!call.observe_native_return(9, &other));
    assert!(call.observe_native_return(9, &lease));
    assert!(call.native_finished() && !call.function_finished());
    assert!(!call.ready_to_release());
    call.observe_function_finished(9, "original").unwrap();
    assert!(call.ready_to_release());
}

//! Binding transitions use the existing living owner and real retained source.
//! Native attempts/commits are modeled; these are not native bootstrap receipts.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "Test fixture prerequisites must fail immediately before exercising retained-source transitions."
)]

use super::{OriginalBinding, OriginalCredentialSlot};
use crate::WebviewId;
use crate::native_document::{NativeCommitIdentity, NativeDocumentOwner};
use crate::original_call::active::ActiveOriginalCall;
use crate::original_event::PendingOriginalEvent;
use crate::renderer_bridge::RendererRoleWitness;
use crate::renderer_contract::{CheckedNativeDocumentLease, NativeAttemptObservation};
use crate::resource_source::windows::tests::Fixture;
use crate::wv_link::WvConsumerDisposition;
use std::num::NonZeroU64;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

fn owner(fixture: &Fixture) -> NativeDocumentOwner {
    let mut owner = NativeDocumentOwner::new(WebviewId(7));
    owner
        .bind_source(&fixture.source_arc())
        .expect("same actual source");
    owner
}

fn prepare(
    binding: &mut OriginalBinding,
    owner: &mut NativeDocumentOwner,
    fixture: &Fixture,
    app: &str,
) -> NativeAttemptObservation {
    let attempt = NativeAttemptObservation(
        owner
            .begin_host_load(fixture.source_arc().source_id())
            .expect("modeled native attempt"),
    );
    binding
        .prepare_attempt(owner, attempt, app.into(), "trusted".into())
        .unwrap();
    // The platform replaces its retained script objects at this position.
    binding.clear_for_attempt();
    attempt
}

fn commit(
    owner: &mut NativeDocumentOwner,
    fixture: &Fixture,
    attempt: NativeAttemptObservation,
) -> CheckedNativeDocumentLease {
    let stamp = owner
        .commit_admitted(
            attempt.0,
            fixture.source_arc().source_id(),
            NativeCommitIdentity(NonZeroU64::new(17).unwrap()),
        )
        .expect("modeled actual commit");
    CheckedNativeDocumentLease(owner.lease_for(stamp).expect("sole owner lease"))
}

fn bound(
    fixture: &Fixture,
) -> (
    OriginalBinding,
    NativeDocumentOwner,
    CheckedNativeDocumentLease,
) {
    let mut owner = owner(fixture);
    let mut binding = OriginalBinding::default();
    let attempt = prepare(&mut binding, &mut owner, fixture, "app");
    let lease = commit(&mut owner, fixture, attempt);
    let candidate = binding.take_matching_commit(&owner).unwrap().unwrap();
    binding.finish_commit(&owner, candidate).unwrap();
    (binding, owner, lease)
}

#[test]
fn preparation_and_candidate_do_not_publish_binding_before_cancellation_finishes() {
    let fixture = Fixture::new();
    let mut owner = owner(&fixture);
    let mut binding = OriginalBinding::default();
    let attempt = prepare(&mut binding, &mut owner, &fixture, "app");
    assert!(binding.has_prepared_attempt());
    assert!(binding.current(&owner).is_none());
    assert!(!binding.authentic(&owner, "app", OriginalCredentialSlot::App));
    assert!(binding.take_matching_commit(&owner).is_err());

    let lease = commit(&mut owner, &fixture, attempt);
    let candidate = binding.take_matching_commit(&owner).unwrap().unwrap();
    assert!(!binding.has_prepared_attempt());
    // Existing queue cancellation runs while the candidate is held. The new
    // credential cannot become callable merely because the native commit arrived.
    assert!(binding.current(&owner).is_none());
    assert!(binding.credential(OriginalCredentialSlot::App).is_none());
    binding.finish_commit(&owner, candidate).unwrap();
    assert_eq!(binding.current(&owner), Some(lease.0.stamp()));
    assert!(binding.authentic(&owner, "app", OriginalCredentialSlot::App));
    assert!(binding.authentic(&owner, "trusted", OriginalCredentialSlot::Trusted));
    assert!(!binding.authentic(&owner, "trusted", OriginalCredentialSlot::App));
    assert!(!binding.original_current(&owner, &lease, "app"));
    binding.register(&owner, "app").unwrap();
    assert!(binding.original_current(&owner, &lease, "app"));
}

#[test]
fn candidate_and_live_binding_reject_an_equal_numeric_commit_from_another_owner() {
    let fixture = Fixture::new();
    let (binding, original, lease) = bound(&fixture);
    let (_other_binding, foreign, numerical_alias) = bound(&fixture);
    assert_eq!(lease.stamp(), numerical_alias.stamp());
    assert!(binding.current(&original).is_some());
    assert!(binding.current(&foreign).is_none());
    assert!(!binding.authentic(&foreign, "app", OriginalCredentialSlot::App));
    assert!(!binding.original_current(&foreign, &numerical_alias, "app"));

    let mut candidate_binding = OriginalBinding::default();
    let mut candidate_owner = owner(&fixture);
    let attempt = prepare(
        &mut candidate_binding,
        &mut candidate_owner,
        &fixture,
        "candidate",
    );
    let candidate_lease = commit(&mut candidate_owner, &fixture, attempt);
    assert_eq!(candidate_lease.stamp(), numerical_alias.stamp());
    let candidate = candidate_binding
        .take_matching_commit(&candidate_owner)
        .unwrap()
        .unwrap();
    assert!(
        candidate_binding
            .finish_commit(&foreign, candidate)
            .is_err()
    );
    assert!(
        candidate_binding
            .credential(OriginalCredentialSlot::App)
            .is_none()
    );
}

#[test]
fn finish_rechecks_living_owner_after_retirement_or_reentrant_successor() {
    let fixture = Fixture::new();
    for successor in [false, true] {
        let mut owner = owner(&fixture);
        let mut binding = OriginalBinding::default();
        let attempt = prepare(&mut binding, &mut owner, &fixture, "old");
        let original = commit(&mut owner, &fixture, attempt);
        let candidate = binding.take_matching_commit(&owner).unwrap().unwrap();
        if successor {
            let next = NativeAttemptObservation(
                owner
                    .begin_host_load(fixture.source_arc().source_id())
                    .unwrap(),
            );
            commit(&mut owner, &fixture, next);
        } else {
            owner.destroy();
        }
        assert!(!original.is_current());
        assert!(binding.finish_commit(&owner, candidate).is_err());
        assert!(binding.current(&owner).is_none());
        assert!(binding.credential(OriginalCredentialSlot::App).is_none());
    }
}

#[test]
fn unmatched_preparation_and_unbound_or_stale_attempts_cannot_bind() {
    let fixture = Fixture::new();
    let mut owner = owner(&fixture);
    let mut binding = OriginalBinding::default();
    let old = prepare(&mut binding, &mut owner, &fixture, "old");
    let next = NativeAttemptObservation(
        owner
            .begin_host_load(fixture.source_arc().source_id())
            .unwrap(),
    );
    assert!(
        binding
            .prepare_attempt(&owner, old, "old".into(), "old".into())
            .is_err()
    );
    commit(&mut owner, &fixture, next);
    assert!(binding.take_matching_commit(&owner).is_err());
    assert!(!binding.has_prepared_attempt());
    assert!(binding.current(&owner).is_none());

    let mut missing_source = NativeDocumentOwner::new(WebviewId(7));
    let attempt = NativeAttemptObservation(
        missing_source
            .begin_host_load(fixture.source_arc().source_id())
            .unwrap(),
    );
    assert!(
        binding
            .prepare_attempt(&missing_source, attempt, "a".into(), "t".into())
            .is_err()
    );
    owner.destroy();
    assert!(
        binding
            .prepare_attempt(&owner, next, "a".into(), "t".into())
            .is_err()
    );
}

#[test]
fn repeated_commit_keeps_registration_role_and_sequence_until_original_handoff() {
    let fixture = Fixture::new();
    let (mut binding, owner, lease) = bound(&fixture);
    assert!(binding.register(&owner, "wrong").is_err());
    binding.register(&owner, "app").unwrap();
    assert!(binding.register(&owner, "app").is_err());
    let role = RendererRoleWitness::new(Arc::new(AtomicBool::new(false)));
    let navigation = u64::from(lease.stamp().observed_generation());
    assert!(binding.observe_route(&owner, navigation, true, role.clone()));
    binding.accept_message_sequence(1).unwrap();

    assert!(binding.take_matching_commit(&owner).unwrap().is_none());
    assert!(binding.registered());
    assert!(binding.role_current(&role));
    assert!(binding.registration_notice().unwrap().same_binding(&lease));
    assert!(binding.accept_message_sequence(1).is_err());
    binding.accept_message_sequence(2).unwrap();

    let (_other_binding, _other_owner, other) = bound(&fixture);
    assert!(binding.registration_handed_off(&other).is_err());
    assert!(binding.registration_notice().unwrap().same_binding(&lease));
    // No handoff is observed on endpoint Full; the original notice remains.
    binding.discard_revoked_registration();
    assert!(binding.registration_notice().unwrap().same_binding(&lease));
    binding.registration_handed_off(&lease).unwrap();
    assert!(binding.registration_notice().is_none());
    assert!(binding.registration_handed_off(&lease).is_err());
}

#[test]
fn late_role_retirement_cannot_clear_a_successor_and_sequence_never_rewinds() {
    let fixture = Fixture::new();
    let (mut binding, owner, lease) = bound(&fixture);
    let navigation = u64::from(lease.stamp().observed_generation());
    let old = RendererRoleWitness::new(Arc::new(AtomicBool::new(false)));
    let stop = Arc::new(AtomicBool::new(false));
    let current = RendererRoleWitness::new(Arc::clone(&stop));
    assert!(binding.observe_route(&owner, navigation, true, old.clone()));
    assert!(binding.observe_route(&owner, navigation, true, current.clone()));
    assert!(!binding.observe_route(&owner, navigation, false, old));
    assert!(binding.role_current(&current));
    assert!(!binding.observe_route(&owner, navigation + 1, false, current.clone()));
    stop.store(true, Ordering::Release);
    assert!(!binding.role_current(&current));
    assert!(!binding.observe_route(&owner, navigation, true, current.clone()));
    assert!(binding.observe_route(&owner, navigation, false, current));
    assert!(binding.route_role().is_none());

    assert!(binding.accept_message_sequence(0).is_err());
    assert!(binding.accept_message_sequence(2).is_err());
    binding.accept_message_sequence(1).unwrap();
    // A later active-Send refusal does not roll back this already consumed ID.
    assert!(binding.accept_message_sequence(1).is_err());
    binding.accept_message_sequence(2).unwrap();
    binding.last_message = u32::MAX;
    assert!(binding.accept_message_sequence(0).is_err());
    assert!(binding.accept_message_sequence(u32::MAX).is_err());
}

#[test]
fn attempt_clear_preserves_preparation_then_commit_resets_only_binding_metadata() {
    let fixture = Fixture::new();
    let (mut binding, mut owner, old_lease) = bound(&fixture);
    binding.register(&owner, "app").unwrap();
    binding.accept_message_sequence(1).unwrap();
    let role = RendererRoleWitness::new(Arc::new(AtomicBool::new(false)));
    assert!(binding.observe_route(
        &owner,
        u64::from(old_lease.stamp().observed_generation()),
        true,
        role.clone(),
    ));

    let next = NativeAttemptObservation(
        owner
            .begin_host_load(fixture.source_arc().source_id())
            .unwrap(),
    );
    binding
        .prepare_attempt(&owner, next, "next".into(), "next-trusted".into())
        .unwrap();
    assert_eq!(binding.credential(OriginalCredentialSlot::App), Some("app"));
    assert!(!binding.authentic(&owner, "app", OriginalCredentialSlot::App));
    // Adapter replacement of retained native scripts precedes this clear.
    binding.clear_for_attempt();
    assert!(binding.has_prepared_attempt());
    assert!(binding.credential(OriginalCredentialSlot::App).is_none());
    assert!(!binding.registered());
    assert!(
        binding
            .registration_notice()
            .unwrap()
            .same_binding(&old_lease)
    );
    assert!(binding.role_current(&role));

    let next_lease = commit(&mut owner, &fixture, next);
    let candidate = binding.take_matching_commit(&owner).unwrap().unwrap();
    binding.finish_commit(&owner, candidate).unwrap();
    assert_eq!(binding.current(&owner), Some(next_lease.0.stamp()));
    assert_eq!(
        binding.credential(OriginalCredentialSlot::App),
        Some("next")
    );
    assert!(!binding.registered());
    assert!(binding.registration_notice().is_none());
    assert!(binding.route_role().is_none());
    binding.accept_message_sequence(1).unwrap();
}

#[test]
fn retirement_clears_live_credentials_but_retained_original_records_can_finish_cleanup() {
    let fixture = Fixture::new();
    let (mut binding, mut owner, lease) = bound(&fixture);
    binding.register(&owner, "app").unwrap();
    let mut call = ActiveOriginalCall::entered(9, lease.clone(), "app".into(), None, 31, None);
    let mut event = PendingOriginalEvent::begin(&owner, 11, lease.clone(), "app".into()).unwrap();
    owner.destroy();
    binding.clear_credentials();
    assert!(binding.current(&owner).is_none());
    assert!(!binding.registered());
    assert!(!binding.original_current(&owner, &lease, "app"));
    binding.discard_revoked_registration();
    assert!(binding.registration_notice().is_none());
    call.observe_function_finished(9, "app").unwrap();
    assert!(call.observe_native_quiescence(9, &lease, true));
    assert!(call.ready_to_release());
    event
        .observe_consumed(11, "app", WvConsumerDisposition::Consumed)
        .unwrap();
    assert!(event.observe_native_quiescence(11, &lease, true));
    assert_eq!(event.native_quiescence(), Some(true));
}

#[test]
fn only_an_actual_native_change_invalidates_otherwise_valid_prepared_metadata() {
    let fixture = Fixture::new();
    for native_changed in [false, true] {
        let mut owner = owner(&fixture);
        let mut binding = OriginalBinding::default();
        let stale = prepare(&mut binding, &mut owner, &fixture, "A");
        let current = prepare(&mut binding, &mut owner, &fixture, "B");
        assert!(
            binding
                .prepare_attempt(&owner, stale, "A".into(), "A".into())
                .is_err()
        );
        assert!(binding.has_prepared_attempt());
        if native_changed {
            // Model the adapter's actual replacement receipt followed by failed
            // revalidation. Retained scripts changed, so B metadata no longer
            // proves which native bootstrap exists. This is not a native receipt.
            binding.invalidate_prepared();
            binding.clear_for_attempt();
        }
        commit(&mut owner, &fixture, current);
        let candidate = binding.take_matching_commit(&owner);
        if native_changed {
            assert!(candidate.is_err());
            assert!(binding.current(&owner).is_none());
            assert!(binding.credential(OriginalCredentialSlot::App).is_none());
        } else {
            // An ordinary stale request did not change the actual native scripts.
            binding
                .finish_commit(&owner, candidate.unwrap().unwrap())
                .unwrap();
            assert_eq!(binding.credential(OriginalCredentialSlot::App), Some("B"));
        }
    }
}

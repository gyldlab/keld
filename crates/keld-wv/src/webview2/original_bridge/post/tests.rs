//! Pure custody/encoding controls; no modeled fact qualifies COM behavior.
use super::*;
use crate::WebviewId;
use crate::native_document::{NativeCommitIdentity, NativeDocumentOwner, VerifiedSourceId};
use crate::renderer_bridge::{
    NativeControlCharge, RendererRoleFence, RendererRouteRetirementFence,
};
use crate::renderer_contract::CheckedNativeDocumentLease;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct Observation {
    growth: Mutex<Vec<(usize, usize)>>,
    dropped: AtomicBool,
    allow: AtomicBool,
}
struct Charge(Arc<Observation>);
impl NativeControlCharge for Charge {
    fn try_extend(&self, _encoded: usize, _accounted: usize) -> bool {
        panic!("native preparation must add to the actual retained slot");
    }
    fn try_extend_storage(&self, encoded: usize, additional: usize) -> bool {
        self.0
            .growth
            .lock()
            .expect("fixture accounting log")
            .push((encoded, additional));
        self.0.allow.load(Ordering::Acquire)
    }
}
impl Drop for Charge {
    fn drop(&mut self) {
        self.0.dropped.store(true, Ordering::Release);
    }
}
struct NoAdmission;
impl RendererRoleFence for NoAdmission {
    fn try_admit_native(&self, _admit: &mut dyn FnMut() -> bool) -> bool {
        panic!("preparation is not final native admission");
    }
}
impl RendererRouteRetirementFence for NoAdmission {
    fn try_reserve_route_unavailable(
        &self,
        _document: &CheckedNativeDocumentLease,
        _encoded: usize,
        _accounted: usize,
    ) -> Option<NativeControlReservation> {
        panic!("preparation must retain its existing reservation");
    }
    fn try_admit_route_unavailable(
        &self,
        _document: &CheckedNativeDocumentLease,
        _admit: &mut dyn FnMut() -> bool,
    ) -> bool {
        panic!("preparation is not final retirement admission");
    }
}
fn charge(allow: bool) -> (NativeControlReservation, Arc<Observation>) {
    let observation = Arc::new(Observation::default());
    observation.allow.store(allow, Ordering::Release);
    (
        NativeControlReservation::from_owner(Charge(Arc::clone(&observation))),
        observation,
    )
}
fn current_authority() -> ControlAuthority {
    ControlAuthority::Current(RendererRoleWitness::with_fence(
        Arc::new(AtomicBool::new(false)),
        Arc::new(NoAdmission),
    ))
}
fn lease() -> (NativeDocumentOwner, CheckedNativeDocumentLease) {
    let mut owner = NativeDocumentOwner::new(WebviewId(17));
    let source = VerifiedSourceId([3; 32]);
    let attempt = owner
        .begin_host_load(source)
        .expect("modeled owner transition");
    let stamp = owner
        .commit_admitted(
            attempt,
            source,
            NativeCommitIdentity(std::num::NonZeroU64::new(19).expect("modeled native ID")),
        )
        .expect("modeled commit only, not native acceptance");
    let lease = owner.lease_for(stamp).expect("same actual state owner");
    (owner, CheckedNativeDocumentLease(lease))
}
const EPOCH: &str = "0123456789abcdef0123456789abcdef";

#[test]
fn canonical_control_golden_grows_same_charge_before_native_encoding() {
    let (_owner, lease) = lease();
    let (charge, observation) = charge(true);
    let carrier = HostRendererMessage::Error {
        request: 7,
        ok: false,
        code: "KELD-WV-011",
        detail: "A🚀🧪λ",
    };
    let prepared = PreparedControl::prepare(&carrier, EPOCH, lease, current_authority(), charge)
        .unwrap_or_else(|error| panic!("{}", error.detail));
    let golden = r#"{"v":2,"document":"0123456789abcdef0123456789abcdef","kind":"reply","request":7,"ok":false,"code":"KELD-WV-011","detail":"A🚀🧪λ"}"#;
    let mut expected: Vec<u16> = golden.encode_utf16().collect();
    expected.push(0);
    assert_eq!(prepared.wide, expected);
    assert_eq!(
        prepared.epoch,
        *EPOCH.as_bytes().first_chunk::<32>().expect("literal")
    );
    assert_eq!(
        *observation
            .growth
            .lock()
            .expect("recorded preallocation request"),
        [(golden.len(), golden.len() + 2 * (golden.len() + 1))],
    );
    assert!(!observation.dropped.load(Ordering::Acquire));
    let charge = prepared
        .into_charge()
        .unwrap_or_else(|_| panic!("never posted"));
    assert!(!observation.dropped.load(Ordering::Acquire));
    drop(charge);
    assert!(observation.dropped.load(Ordering::Acquire));
}

#[test]
fn failed_storage_growth_preserves_existing_control_custody() {
    let (_owner, lease) = lease();
    let (charge, observation) = charge(false);
    let failure = match PreparedControl::prepare(
        &HostRendererMessage::AppRegistered,
        EPOCH,
        lease,
        current_authority(),
        charge,
    ) {
        Err(failure) => failure,
        Ok(_) => panic!("failed canonical storage growth must refuse"),
    };
    assert_eq!(
        failure.detail,
        "existing CONTROL storage budget is unavailable"
    );
    assert_eq!(observation.growth.lock().expect("log").len(), 1);
    assert!(!observation.dropped.load(Ordering::Acquire));
    drop(failure.charge);
    assert!(observation.dropped.load(Ordering::Acquire));
}

#[test]
fn posting_and_unknown_cannot_extract_or_release_native_backing() {
    for phase in [PostPhase::Posting, PostPhase::OutcomeUnknown] {
        let (_owner, lease) = lease();
        let (charge, observation) = charge(true);
        let mut prepared = PreparedControl::prepare(
            &HostRendererMessage::Bound { ready: false },
            EPOCH,
            lease,
            current_authority(),
            charge,
        )
        .unwrap_or_else(|error| panic!("{}", error.detail));
        let backing = prepared.wide.as_ptr();
        prepared.phase = phase; // Deliberate pure custody fault injection.
        let mut retained = match prepared.into_charge() {
            Err(retained) => retained,
            Ok(_) => panic!("unknown native operation lost its backing"),
        };
        assert_eq!(retained.phase, phase);
        assert_eq!(retained.wide.as_ptr(), backing);
        assert!(!observation.dropped.load(Ordering::Acquire));
        // Test-only cleanup after proving retention, not a native Close receipt.
        retained.phase = PostPhase::Returned;
        drop(
            retained
                .into_charge()
                .unwrap_or_else(|_| panic!("fixture cleanup")),
        );
        assert!(observation.dropped.load(Ordering::Acquire));
    }
}

#[test]
fn retirement_authority_and_data_never_expand_the_control_carrier() {
    for carrier in [
        HostRendererMessage::Bound { ready: false },
        HostRendererMessage::RouteState { ready: true },
        HostRendererMessage::AppRefused { call: 1 },
        HostRendererMessage::Reply {
            request: 1,
            ok: true,
            payload: &[],
        },
    ] {
        let (_owner, lease) = lease();
        let (charge, observation) = charge(true);
        let failure = match PreparedControl::prepare(
            &carrier,
            EPOCH,
            lease,
            ControlAuthority::RetiredRoute(RouteRetirementPermit::from_owner(Arc::new(
                NoAdmission,
            ))),
            charge,
        ) {
            Err(failure) => failure,
            Ok(_) => panic!("retirement accepted a non-route-unavailable carrier"),
        };
        assert!(observation.growth.lock().expect("log").is_empty());
        assert!(!observation.dropped.load(Ordering::Acquire));
        drop(failure.charge);
    }
    let (_owner, lease) = lease();
    let (charge, observation) = charge(true);
    let prepared = PreparedControl::prepare(
        &HostRendererMessage::RouteState { ready: false },
        EPOCH,
        lease,
        ControlAuthority::RetiredRoute(RouteRetirementPermit::from_owner(Arc::new(NoAdmission))),
        charge,
    )
    .unwrap_or_else(|error| panic!("{}", error.detail));
    assert_eq!(observation.growth.lock().expect("log").len(), 1);
    drop(
        prepared
            .into_charge()
            .unwrap_or_else(|_| panic!("never posted")),
    );
}

#[test]
fn retry_classification_never_turns_an_entered_call_into_never_written() {
    assert!(PostPhase::Prepared.prior_attempt_result().is_none());
    for phase in [PostPhase::Posting, PostPhase::OutcomeUnknown] {
        assert!(matches!(
            phase.prior_attempt_result(),
            Some(PostResult::OutcomeUnknown)
        ));
    }
    assert!(matches!(
        PostPhase::Returned.prior_attempt_result(),
        Some(PostResult::AlreadyReturned)
    ));
}

#[test]
fn native_post_requires_a_consistent_canonical_admission_and_local_entry() {
    let mut prepared = PostPhase::Prepared;
    assert!(matches!(
        prepared.finish_admission(false),
        Some(PostResult::NotAdmitted)
    ));
    assert_eq!(prepared, PostPhase::Prepared);
    let mut posting = PostPhase::Posting;
    assert!(posting.finish_admission(true).is_none());
    for (admitted, mut phase) in [
        (true, PostPhase::Prepared),
        (false, PostPhase::Posting),
        (true, PostPhase::OutcomeUnknown),
    ] {
        assert!(matches!(
            phase.finish_admission(admitted),
            Some(PostResult::OutcomeUnknown)
        ));
        assert_eq!(phase, PostPhase::OutcomeUnknown);
    }
}

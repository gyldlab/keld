//! Test-only joining model. No receipt, signer facts or match constitutes trust.
use super::super::{
    DevRolePolicy, DevRolePolicyMatch, DevSignerFacts, WindowsD45HostRole, compare_dev_signer,
};
use super::{APP, CODE_EKU, RECOVERY, facts, policy};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Member {
    Image,
    PublicRoot,
    Observation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Entry {
    member: Member,
    digest: [u8; 32],
    identity: u128,
}

struct Receipt<'a> {
    operation: u128,
    entries: &'a [Entry],
}

// Constructed only at this synthetic test boundary, from independently
// supplied facts and policy. Not a replacement for authenticated D-WIRE.
struct SyntheticPolicyMatch {
    operation: u128,
    verified_image: Entry,
    root_digest: [u8; 32],
    _signer: DevRolePolicyMatch,
}

impl SyntheticPolicyMatch {
    fn compare(
        expected_operation: u128,
        expected_policy: &DevRolePolicy<'_>,
        independent_facts: &DevSignerFacts<'_>,
        verified_image: Entry,
    ) -> Option<Self> {
        if verified_image.member != Member::Image {
            return None;
        }
        Some(Self {
            operation: expected_operation,
            verified_image,
            root_digest: independent_facts.pins.root_certificate_sha256,
            _signer: compare_dev_signer(expected_policy, independent_facts).ok()?,
        })
    }
}

#[must_use = "a test model result cannot authorize the signer or installer"]
struct DeclaredInventoryMatched;

fn join(
    expected: Option<&SyntheticPolicyMatch>,
    candidate: &Receipt<'_>,
    observed: &[Entry; 3],
) -> Option<DeclaredInventoryMatched> {
    let expected = expected?;
    if candidate.operation != expected.operation
        || candidate.entries.len() != observed.len()
        || observed[0] != expected.verified_image
        || observed[1].digest != expected.root_digest
    {
        return None;
    }
    let members = [Member::Image, Member::PublicRoot, Member::Observation];
    for (slot, member) in members.into_iter().enumerate() {
        let live = observed[slot];
        if live.member != member || candidate.entries[slot] != live {
            return None;
        }
        if observed[..slot]
            .iter()
            .any(|other| other.identity == live.identity)
        {
            return None;
        }
    }
    Some(DeclaredInventoryMatched)
}

const LIVE: [Entry; 3] = [
    Entry {
        member: Member::Image,
        digest: [0x11; 32],
        identity: 1,
    },
    Entry {
        member: Member::PublicRoot,
        digest: [0x11; 32],
        identity: 2,
    },
    Entry {
        member: Member::Observation,
        digest: [0x33; 32],
        identity: 3,
    },
];

fn receipt(entries: &[Entry]) -> Receipt<'_> {
    Receipt {
        operation: 5,
        entries,
    }
}

#[test]
fn synthetic_receipt_can_join_existing_role_comparison_and_exact_inventory() {
    let matched = SyntheticPolicyMatch::compare(
        5,
        &policy(WindowsD45HostRole::Ordinary),
        &facts(APP, CODE_EKU),
        LIVE[0],
    )
    .expect("independent APP comparison");
    assert!(join(Some(&matched), &receipt(&LIVE), &LIVE).is_some());
}

#[test]
fn synthetic_receipt_cannot_supply_the_independent_role_comparison() {
    let role = policy(WindowsD45HostRole::Ordinary);
    let matched = SyntheticPolicyMatch::compare(5, &role, &facts(RECOVERY, CODE_EKU), LIVE[0]);
    assert!(matched.is_none());
    assert!(join(matched.as_ref(), &receipt(&LIVE), &LIVE).is_none());
}

#[test]
fn synthetic_receipt_must_bind_operation_order_count_and_distinct_objects() {
    let matched = SyntheticPolicyMatch::compare(
        5,
        &policy(WindowsD45HostRole::DedicatedRecovery),
        &facts(RECOVERY, CODE_EKU),
        LIVE[0],
    )
    .expect("independent RECOVERY comparison");
    let matched = Some(&matched);
    assert!(join(matched, &receipt(&LIVE[..2]), &LIVE).is_none());
    assert!(
        join(
            matched,
            &Receipt {
                operation: 6,
                entries: &LIVE
            },
            &LIVE
        )
        .is_none()
    );
    let mut other = LIVE;
    other.swap(0, 1);
    assert!(join(matched, &receipt(&other), &LIVE).is_none());
    let mut other = LIVE;
    other[2].identity = other[0].identity;
    assert!(join(matched, &receipt(&other), &other).is_none());
}

#[test]
fn synthetic_receipt_requires_identity_and_digest_independently() {
    let matched = SyntheticPolicyMatch::compare(
        5,
        &policy(WindowsD45HostRole::Ordinary),
        &facts(APP, CODE_EKU),
        LIVE[0],
    )
    .expect("independent APP comparison");
    let mut other = LIVE;
    other[0].identity = 4;
    assert!(join(Some(&matched), &receipt(&other), &LIVE).is_none());
    other = LIVE;
    other[0].digest = [0x44; 32];
    assert!(join(Some(&matched), &receipt(&other), &LIVE).is_none());
    assert!(join(Some(&matched), &receipt(&other), &other).is_none());
    other = LIVE;
    other[1].digest = [0x44; 32];
    assert!(join(Some(&matched), &receipt(&other), &other).is_none());
}

//! Pure D-45 data-consistency controls. These synthetic records are never a
//! real certificate, commissioned policy, signed Host or T-82 `WinTrust` pass.

use super::{
    CODE_SIGNING_OID, PUBLIC_TRUST_OID, WindowsD45HostRole, WindowsD45IssuerOperation,
    WindowsD45IssuerPolicy, WindowsD45NativeSignerFacts, WindowsD45ProfilePolicy,
    require_d45_issuer_profile_role,
};
use sha2::{Digest as _, Sha256};

// Literal closed D-3 field order: policy parsing stays with D-WIRE, while this
// Guard test checks independent digest/chain/role comparison only.
const POLICY_BYTES: &[u8] = br#"{"schema":"keld.public-trust-issuer-policy/v1","chainPolicy":"windows-generic-verify-v2","rootCertificateSha256":["1111111111111111111111111111111111111111111111111111111111111111","3333333333333333333333333333333333333333333333333333333333333333"],"leafIssuerSpkiSha256":["2222222222222222222222222222222222222222222222222222222222222222","4444444444444444444444444444444444444444444444444444444444444444"]}"#;
const ROTATED_SOURCE_POLICY_BYTES: &[u8] = br#"{"schema":"keld.public-trust-issuer-policy/v1","chainPolicy":"windows-generic-verify-v2","rootCertificateSha256":["5555555555555555555555555555555555555555555555555555555555555555"],"leafIssuerSpkiSha256":["6666666666666666666666666666666666666666666666666666666666666666"]}"#;
const APP_ID: &str = "com.example.app";
const APP_PROFILE: &str = "1.2.840.113556.1.8000.2554";
const RECOVERY_PROFILE: &str = "1.2.840.113556.1.8000.2555";
const OTHER_RECOVERY_PROFILE: &str = "1.2.840.113556.1.8000.2556";
const ROOTS: [[u8; 32]; 2] = [[0x11; 32], [0x33; 32]];
const ISSUERS: [[u8; 32]; 2] = [[0x22; 32], [0x44; 32]];
const OTHER_ROOTS: [[u8; 32]; 1] = [[0x55; 32]];
const OTHER_ISSUERS: [[u8; 32]; 1] = [[0x66; 32]];

fn sha256(data: &[u8]) -> [u8; 32] {
    let mut result = [0_u8; 32];
    result.copy_from_slice(&Sha256::digest(data));
    result
}

fn policy<'a>(digest: &'a [u8; 32], role: WindowsD45HostRole) -> WindowsD45ProfilePolicy<'a> {
    WindowsD45ProfilePolicy {
        role,
        issuer_operation: WindowsD45IssuerOperation::UnchangedPolicy,
        app_id: APP_ID,
        application_profile_oid: APP_PROFILE,
        source_recovery_profile_oid: RECOVERY_PROFILE,
        destination_recovery_profile_oid: OTHER_RECOVERY_PROFILE,
        source_issuer: WindowsD45IssuerPolicy {
            bytes: POLICY_BYTES,
            signed_digest: digest,
            root_certificate_hashes: &ROOTS,
            leaf_issuer_spki_hashes: &ISSUERS,
            wire_membership_binding: None,
        },
        destination_issuer: WindowsD45IssuerPolicy {
            bytes: POLICY_BYTES,
            signed_digest: digest,
            root_certificate_hashes: &ROOTS,
            leaf_issuer_spki_hashes: &ISSUERS,
            wire_membership_binding: None,
        },
    }
}

fn signer<'a>(ekus: &'a [&'a str]) -> WindowsD45NativeSignerFacts<'a> {
    WindowsD45NativeSignerFacts {
        signed_app_id: APP_ID,
        leaf_eku_oids: ekus,
        root_certificate_sha256: ROOTS[0],
        leaf_issuer_spki_sha256: ISSUERS[0],
    }
}

fn rejection(
    policy: Option<&WindowsD45ProfilePolicy<'_>>,
    signer: Option<&WindowsD45NativeSignerFacts<'_>>,
) -> String {
    require_d45_issuer_profile_role(policy, signer)
        .err()
        .expect("this independent mismatch must refuse")
        .detail()
        .to_owned()
}

#[test]
fn absent_native_and_signed_policy_evidence_always_refuse() {
    let digest = sha256(POLICY_BYTES);
    let authorized = policy(&digest, WindowsD45HostRole::Ordinary);
    let eku = [CODE_SIGNING_OID, PUBLIC_TRUST_OID, APP_PROFILE];
    let signer = signer(&eku);
    assert!(rejection(None, Some(&signer)).contains("policy is unavailable"));
    assert!(rejection(Some(&authorized), None).contains("native WinTrust issuer"));
    assert!(rejection(None, None).contains("policy is unavailable"));
}

#[test]
fn forged_widened_issuer_arrays_refuse_with_matching_native_evidence() {
    // Before this fix, a source/destination array independently widened to
    // contain a foreign root and issuer SPKI would return Ok even though the
    // signed canonical JSON named neither, provided native facts were Some.
    // Native presence must not bypass the absent D-WIRE membership binding.
    let digest = sha256(POLICY_BYTES);
    let mut unbound = policy(&digest, WindowsD45HostRole::Ordinary);
    let roots = [ROOTS[0], ROOTS[1], OTHER_ROOTS[0]];
    let issuers = [ISSUERS[0], ISSUERS[1], OTHER_ISSUERS[0]];
    unbound.source_issuer.root_certificate_hashes = &roots;
    unbound.destination_issuer.root_certificate_hashes = &roots;
    unbound.source_issuer.leaf_issuer_spki_hashes = &issuers;
    unbound.destination_issuer.leaf_issuer_spki_hashes = &issuers;
    let eku = [CODE_SIGNING_OID, PUBLIC_TRUST_OID, APP_PROFILE];
    let mut native = signer(&eku);
    native.root_certificate_sha256 = OTHER_ROOTS[0];
    native.leaf_issuer_spki_sha256 = OTHER_ISSUERS[0];
    assert!(
        rejection(Some(&unbound), Some(&native)).contains("not bound to canonical authenticated D-WIRE"),
        "valid-looking native evidence must not authorize forged issuer membership"
    );
}

#[test]
fn even_matching_synthetic_issuer_cohorts_cannot_grant_precommissioning() {
    // No synthetic positive can mint a logical policy match until the Pack
    // D-WIRE producer binds canonical decoded membership to the signed bytes.
    let digest = sha256(POLICY_BYTES);
    let ordinary = policy(&digest, WindowsD45HostRole::Ordinary);
    let ordinary_eku = [CODE_SIGNING_OID, PUBLIC_TRUST_OID, APP_PROFILE, "1.2.3.4"];
    assert!(
        rejection(Some(&ordinary), Some(&signer(&ordinary_eku)))
            .contains("not bound to canonical authenticated D-WIRE")
    );

    let mut recovery = policy(&digest, WindowsD45HostRole::DedicatedRecovery);
    recovery.destination_recovery_profile_oid = RECOVERY_PROFILE;
    let recovery_eku = [CODE_SIGNING_OID, PUBLIC_TRUST_OID, RECOVERY_PROFILE];
    assert!(
        rejection(Some(&recovery), Some(&signer(&recovery_eku)))
            .contains("not bound to canonical authenticated D-WIRE")
    );
}

#[test]
fn application_recovery_roles_cannot_be_crossed_or_combined() {
    let digest = sha256(POLICY_BYTES);
    let ordinary = policy(&digest, WindowsD45HostRole::Ordinary);
    let dual_role = [
        CODE_SIGNING_OID,
        PUBLIC_TRUST_OID,
        APP_PROFILE,
        RECOVERY_PROFILE,
    ];
    assert!(rejection(Some(&ordinary), Some(&signer(&dual_role))).contains("both application and recovery"));

    let recovery_only = [CODE_SIGNING_OID, PUBLIC_TRUST_OID, RECOVERY_PROFILE];
    assert!(rejection(Some(&ordinary), Some(&signer(&recovery_only))).contains("exact commissioned role"));

    let mut recovery = policy(&digest, WindowsD45HostRole::DedicatedRecovery);
    recovery.destination_recovery_profile_oid = RECOVERY_PROFILE;
    let application_only = [CODE_SIGNING_OID, PUBLIC_TRUST_OID, APP_PROFILE];
    assert!(rejection(Some(&recovery), Some(&signer(&application_only))).contains("exact commissioned role"));
    let dual = [CODE_SIGNING_OID, PUBLIC_TRUST_OID, RECOVERY_PROFILE, APP_PROFILE];
    assert!(rejection(Some(&recovery), Some(&signer(&dual))).contains("both application and recovery"));
}

#[test]
fn overlapping_generic_and_changed_recovery_profiles_refuse() {
    let digest = sha256(POLICY_BYTES);
    let eku = [CODE_SIGNING_OID, PUBLIC_TRUST_OID, APP_PROFILE];
    let facts = signer(&eku);

    let mut generic = policy(&digest, WindowsD45HostRole::Ordinary);
    generic.application_profile_oid = CODE_SIGNING_OID;
    assert!(rejection(Some(&generic), Some(&facts)).contains("profiles are missing or overlap"));

    let mut overlapping = policy(&digest, WindowsD45HostRole::Ordinary);
    overlapping.destination_recovery_profile_oid = APP_PROFILE;
    assert!(rejection(Some(&overlapping), Some(&facts)).contains("profiles are missing or overlap"));

    let recovery_eku = [CODE_SIGNING_OID, PUBLIC_TRUST_OID, RECOVERY_PROFILE];
    let changed = policy(&digest, WindowsD45HostRole::DedicatedRecovery);
    assert!(rejection(Some(&changed), Some(&signer(&recovery_eku)))
        .contains("profiles are missing or overlap"));
}

#[test]
fn missing_duplicate_and_generic_eku_never_select_a_role() {
    let digest = sha256(POLICY_BYTES);
    let policy = policy(&digest, WindowsD45HostRole::Ordinary);
    for eku in [
        vec![PUBLIC_TRUST_OID, APP_PROFILE],
        vec![CODE_SIGNING_OID, APP_PROFILE],
        vec![CODE_SIGNING_OID, CODE_SIGNING_OID, PUBLIC_TRUST_OID, APP_PROFILE],
        vec![CODE_SIGNING_OID, PUBLIC_TRUST_OID, APP_PROFILE, APP_PROFILE],
    ] {
        let detail = rejection(Some(&policy), Some(&signer(&eku)));
        assert!(detail.contains("EKU") || detail.contains("profile"), "{detail}");
    }
}

#[test]
fn signed_app_and_issuer_digest_mismatch_are_independent_refusals() {
    let digest = sha256(POLICY_BYTES);
    let eku = [CODE_SIGNING_OID, PUBLIC_TRUST_OID, APP_PROFILE];
    let mut facts = signer(&eku);
    facts.signed_app_id = "com.foreign.app";
    let expected_policy = policy(&digest, WindowsD45HostRole::Ordinary);
    assert!(rejection(Some(&expected_policy), Some(&facts)).contains("signed app ID"));

    let mut corrupted = digest;
    corrupted[0] ^= 1;
    let wrong_policy = policy(&corrupted, WindowsD45HostRole::Ordinary);
    assert!(rejection(Some(&wrong_policy), Some(&signer(&eku))).contains("signed issuer-policy digest"));
}

#[test]
fn unbound_source_and_destination_issuer_lists_refuse_native_membership() {
    let digest = sha256(POLICY_BYTES);
    let eku = [CODE_SIGNING_OID, PUBLIC_TRUST_OID, APP_PROFILE];
    let facts = signer(&eku);
    let mut source = policy(&digest, WindowsD45HostRole::Ordinary);
    source.source_issuer.root_certificate_hashes = &OTHER_ROOTS;
    assert!(
        rejection(Some(&source), Some(&facts))
            .contains("not bound to canonical authenticated D-WIRE")
    );

    let mut destination = policy(&digest, WindowsD45HostRole::Ordinary);
    destination.destination_issuer.leaf_issuer_spki_hashes = &OTHER_ISSUERS;
    assert!(
        rejection(Some(&destination), Some(&facts))
            .contains("not bound to canonical authenticated D-WIRE")
    );

    let mut foreign = signer(&eku);
    foreign.root_certificate_sha256 = OTHER_ROOTS[0];
    assert!(rejection(Some(&policy(&digest, WindowsD45HostRole::Ordinary)), Some(&foreign))
        .contains("not bound to canonical authenticated D-WIRE"));
}

#[test]
fn d7_rotation_needs_source_authorization_but_candidate_matches_destination_only() {
    // D-7 is not an intersection of two issuers for one candidate signature.
    // The U1 source needs its own prior full signer proof; U2 is separately
    // verified against the destination issuer policy chosen by authenticated
    // source transition. None of those authority proofs can be fabricated here.
    let destination_digest = sha256(POLICY_BYTES);
    let source_digest = sha256(ROTATED_SOURCE_POLICY_BYTES);
    let eku = [CODE_SIGNING_OID, PUBLIC_TRUST_OID, APP_PROFILE];
    let candidate_native = signer(&eku); // Matches destination, not rotated source.
    let mut candidate = policy(&destination_digest, WindowsD45HostRole::Ordinary);
    candidate.source_issuer.bytes = ROTATED_SOURCE_POLICY_BYTES;
    candidate.source_issuer.signed_digest = &source_digest;
    candidate.source_issuer.root_certificate_hashes = &OTHER_ROOTS;
    candidate.source_issuer.leaf_issuer_spki_hashes = &OTHER_ISSUERS;
    assert!(
        rejection(Some(&candidate), Some(&candidate_native))
            .contains("issuer policies differ without authenticated D-7"),
        "unchanged-policy operation must not adopt a new issuer cohort"
    );
    candidate.issuer_operation = WindowsD45IssuerOperation::D7DestinationCandidate;
    assert!(
        rejection(Some(&candidate), Some(&candidate_native))
            .contains("D-7 candidate issuer rotation lacks authenticated source-transition"),
        "destination-matching native facts cannot replace the U1/D7 authorization witness"
    );
}

#[test]
fn absent_issuer_entries_refuse_before_native_matching() {
    let digest = sha256(POLICY_BYTES);
    let eku = [CODE_SIGNING_OID, PUBLIC_TRUST_OID, APP_PROFILE];
    let facts = signer(&eku);
    let mut input = policy(&digest, WindowsD45HostRole::Ordinary);
    input.source_issuer.root_certificate_hashes = &[];
    assert!(rejection(Some(&input), Some(&facts)).contains("no root"));
    let mut input = policy(&digest, WindowsD45HostRole::Ordinary);
    input.destination_issuer.leaf_issuer_spki_hashes = &[];
    // Source is inspected before destination, and its D-WIRE binding is
    // deliberately uninhabited until the authenticated Pack producer exists.
    // Therefore a synthetic destination-empty fixture cannot reach the
    // destination list check; do not manufacture a source binding to test it.
    assert!(
        rejection(Some(&input), Some(&facts))
            .contains("issuer membership arrays are not bound to canonical authenticated D-WIRE"),
        "the unqualified source issuer must refuse before destination inspection"
    );
}

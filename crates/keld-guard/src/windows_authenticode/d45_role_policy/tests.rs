//! Pure D-45 data-consistency controls. These synthetic records are never a
//! real certificate, commissioned policy, signed Host or T-82 `WinTrust` pass.

use super::{
    CODE_SIGNING_OID, PUBLIC_TRUST_OID, WindowsD45HostRole, WindowsD45IssuerPolicy,
    WindowsD45NativeSignerFacts, WindowsD45ProfilePolicy, require_d45_issuer_profile_role,
};
use sha2::{Digest as _, Sha256};

// Literal closed D-3 field order: policy parsing stays with D-WIRE, while this
// Guard test checks independent digest/chain/role comparison only.
const POLICY_BYTES: &[u8] = br#"{"schema":"keld.public-trust-issuer-policy/v1","chainPolicy":"windows-generic-verify-v2","rootCertificateSha256":["1111111111111111111111111111111111111111111111111111111111111111","3333333333333333333333333333333333333333333333333333333333333333"],"leafIssuerSpkiSha256":["2222222222222222222222222222222222222222222222222222222222222222","4444444444444444444444444444444444444444444444444444444444444444"]}"#;
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
        app_id: APP_ID,
        application_profile_oid: APP_PROFILE,
        source_recovery_profile_oid: RECOVERY_PROFILE,
        destination_recovery_profile_oid: OTHER_RECOVERY_PROFILE,
        source_issuer: WindowsD45IssuerPolicy {
            bytes: POLICY_BYTES,
            signed_digest: digest,
            root_certificate_hashes: &ROOTS,
            leaf_issuer_spki_hashes: &ISSUERS,
        },
        destination_issuer: WindowsD45IssuerPolicy {
            bytes: POLICY_BYTES,
            signed_digest: digest,
            root_certificate_hashes: &ROOTS,
            leaf_issuer_spki_hashes: &ISSUERS,
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
fn separately_widened_policy_arrays_without_native_and_dwire_producers_never_grant() {
    // A caller could try to add a foreign root to a separately supplied array
    // while keeping the signed issuer bytes unchanged. Production cannot build
    // this private policy type at all; a fabricated test-only value with an
    // added root must still be refused when native evidence is unavailable.
    let digest = sha256(POLICY_BYTES);
    let mut unbound = policy(&digest, WindowsD45HostRole::Ordinary);
    let widened = [ROOTS[0], ROOTS[1], OTHER_ROOTS[0]];
    unbound.source_issuer.root_certificate_hashes = &widened;
    let eku = [CODE_SIGNING_OID, PUBLIC_TRUST_OID, APP_PROFILE];
    assert!(rejection(Some(&unbound), None).contains("native WinTrust issuer"));
    // A separate list without an authenticated D-WIRE object is also refused.
    assert!(rejection(None, Some(&signer(&eku))).contains("policy is unavailable"));
}

#[test]
fn exact_role_issuer_cohorts_agree_as_data_only() {
    // A synthetic logical pass cannot construct Guard's verified image or an
    // Update permission. It only proves the independent role predicate.
    let digest = sha256(POLICY_BYTES);
    let ordinary = policy(&digest, WindowsD45HostRole::Ordinary);
    let ordinary_eku = [CODE_SIGNING_OID, PUBLIC_TRUST_OID, APP_PROFILE, "1.2.3.4"];
    assert!(
        require_d45_issuer_profile_role(Some(&ordinary), Some(&signer(&ordinary_eku)),).is_ok()
    );

    let mut recovery = policy(&digest, WindowsD45HostRole::DedicatedRecovery);
    recovery.destination_recovery_profile_oid = RECOVERY_PROFILE;
    let recovery_eku = [CODE_SIGNING_OID, PUBLIC_TRUST_OID, RECOVERY_PROFILE];
    assert!(
        require_d45_issuer_profile_role(Some(&recovery), Some(&signer(&recovery_eku)),).is_ok()
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
    assert!(
        rejection(Some(&ordinary), Some(&signer(&dual_role)))
            .contains("both application and recovery")
    );

    let recovery_only = [CODE_SIGNING_OID, PUBLIC_TRUST_OID, RECOVERY_PROFILE];
    assert!(
        rejection(Some(&ordinary), Some(&signer(&recovery_only)))
            .contains("exact commissioned role")
    );

    let mut recovery = policy(&digest, WindowsD45HostRole::DedicatedRecovery);
    recovery.destination_recovery_profile_oid = RECOVERY_PROFILE;
    let application_only = [CODE_SIGNING_OID, PUBLIC_TRUST_OID, APP_PROFILE];
    assert!(
        rejection(Some(&recovery), Some(&signer(&application_only)))
            .contains("exact commissioned role")
    );
    let dual = [
        CODE_SIGNING_OID,
        PUBLIC_TRUST_OID,
        RECOVERY_PROFILE,
        APP_PROFILE,
    ];
    assert!(
        rejection(Some(&recovery), Some(&signer(&dual))).contains("both application and recovery")
    );
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
    assert!(
        rejection(Some(&overlapping), Some(&facts)).contains("profiles are missing or overlap")
    );

    let recovery_eku = [CODE_SIGNING_OID, PUBLIC_TRUST_OID, RECOVERY_PROFILE];
    let changed = policy(&digest, WindowsD45HostRole::DedicatedRecovery);
    assert!(
        rejection(Some(&changed), Some(&signer(&recovery_eku)))
            .contains("profiles are missing or overlap")
    );
}

#[test]
fn missing_duplicate_and_generic_eku_never_select_a_role() {
    let digest = sha256(POLICY_BYTES);
    let policy = policy(&digest, WindowsD45HostRole::Ordinary);
    for eku in [
        vec![PUBLIC_TRUST_OID, APP_PROFILE],
        vec![CODE_SIGNING_OID, APP_PROFILE],
        vec![
            CODE_SIGNING_OID,
            CODE_SIGNING_OID,
            PUBLIC_TRUST_OID,
            APP_PROFILE,
        ],
        vec![CODE_SIGNING_OID, PUBLIC_TRUST_OID, APP_PROFILE, APP_PROFILE],
    ] {
        let detail = rejection(Some(&policy), Some(&signer(&eku)));
        assert!(
            detail.contains("EKU") || detail.contains("profile"),
            "{detail}"
        );
    }
}

#[test]
fn signed_app_and_issuer_digest_mismatch_are_independent_refusals() {
    let digest = sha256(POLICY_BYTES);
    let eku = [CODE_SIGNING_OID, PUBLIC_TRUST_OID, APP_PROFILE];
    let mut facts = signer(&eku);
    facts.signed_app_id = "com.foreign.app";
    let policy = policy(&digest, WindowsD45HostRole::Ordinary);
    assert!(rejection(Some(&policy), Some(&facts)).contains("signed app ID"));

    let mut corrupted = digest;
    corrupted[0] ^= 1;
    let wrong_policy = policy(&corrupted, WindowsD45HostRole::Ordinary);
    assert!(
        rejection(Some(&wrong_policy), Some(&signer(&eku))).contains("signed issuer-policy digest")
    );
}

#[test]
fn source_and_destination_issuer_restrictions_both_apply() {
    let digest = sha256(POLICY_BYTES);
    let eku = [CODE_SIGNING_OID, PUBLIC_TRUST_OID, APP_PROFILE];
    let facts = signer(&eku);
    let mut source = policy(&digest, WindowsD45HostRole::Ordinary);
    source.source_issuer.root_certificate_hashes = &OTHER_ROOTS;
    assert!(rejection(Some(&source), Some(&facts)).contains("root certificate"));

    let mut destination = policy(&digest, WindowsD45HostRole::Ordinary);
    destination.destination_issuer.leaf_issuer_spki_hashes = &OTHER_ISSUERS;
    assert!(rejection(Some(&destination), Some(&facts)).contains("issuer SPKI"));

    let mut foreign = signer(&eku);
    foreign.root_certificate_sha256 = OTHER_ROOTS[0];
    assert!(
        rejection(
            Some(&policy(&digest, WindowsD45HostRole::Ordinary)),
            Some(&foreign)
        )
        .contains("root certificate")
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
    assert!(rejection(Some(&input), Some(&facts)).contains("no root or leaf"));
}

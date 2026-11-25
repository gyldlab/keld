//! Synthetic DEV/PROD policy separation oracles. No test signs an image,
//! mints an authenticated D-WIRE policy or enables a production callsite.

use super::{
    ANY_EKU_OID, CODE_SIGNING_OID, DEV_PROVIDER, DEV_ROOT_POLICY, DevRolePolicy, DevSignerFacts,
    DevSignerPins, PUBLIC_TRUST_OID, WindowsD45HostRole, compare_dev_signer,
};

const APP: DevSignerPins = DevSignerPins {
    root_certificate_sha256: [0x11; 32],
    leaf_issuer_spki_sha256: [0x22; 32],
    leaf_certificate_sha256: [0x33; 32],
    leaf_spki_sha256: [0x44; 32],
};
const RECOVERY: DevSignerPins = DevSignerPins {
    root_certificate_sha256: [0x11; 32],
    leaf_issuer_spki_sha256: [0x22; 32],
    leaf_certificate_sha256: [0x55; 32],
    leaf_spki_sha256: [0x66; 32],
};
const APP_ID: &str = "dev.example.keld";
const CODE_EKU: &[&str] = &[CODE_SIGNING_OID];

fn policy(role: WindowsD45HostRole) -> DevRolePolicy<'static> {
    DevRolePolicy {
        provider: DEV_PROVIDER,
        root_policy: DEV_ROOT_POLICY,
        app_id: APP_ID,
        role,
        app: APP,
        recovery: RECOVERY,
    }
}

fn facts<'a>(pins: DevSignerPins, ekus: &'a [&'a str]) -> DevSignerFacts<'a> {
    DevSignerFacts {
        signed_app_id: APP_ID,
        leaf_eku_oids: ekus,
        pins,
    }
}

fn rejection(policy: &DevRolePolicy<'_>, facts: &DevSignerFacts<'_>) -> String {
    compare_dev_signer(policy, facts)
        .err()
        .expect("the independent hostile signer must refuse")
        .detail()
        .to_owned()
}

#[test]
fn app_and_recovery_match_only_their_own_pinned_leaves() {
    assert!(
        compare_dev_signer(&policy(WindowsD45HostRole::Ordinary), &facts(APP, CODE_EKU)).is_ok()
    );
    assert!(
        compare_dev_signer(
            &policy(WindowsD45HostRole::DedicatedRecovery),
            &facts(RECOVERY, CODE_EKU),
        )
        .is_ok()
    );
    assert!(
        rejection(
            &policy(WindowsD45HostRole::Ordinary),
            &facts(RECOVERY, CODE_EKU)
        )
        .contains("foreign leaf certificate")
    );
    assert!(
        rejection(
            &policy(WindowsD45HostRole::DedicatedRecovery),
            &facts(APP, CODE_EKU)
        )
        .contains("foreign leaf certificate")
    );
}

#[test]
fn provider_pair_and_signed_app_id_never_fall_back_to_production() {
    let expected = facts(APP, CODE_EKU);
    let mut role = policy(WindowsD45HostRole::Ordinary);
    role.provider = 1;
    role.root_policy = 2;
    assert!(rejection(&role, &expected).contains("provider and root policy"));
    role.provider = DEV_PROVIDER;
    assert!(rejection(&role, &expected).contains("provider and root policy"));
    role.root_policy = DEV_ROOT_POLICY;
    role.app_id = "";
    assert!(rejection(&role, &expected).contains("signed app ID"));
    role.app_id = "prod.example.keld";
    assert!(rejection(&role, &expected).contains("signed app ID"));
    role.app_id = APP_ID;
    let mut foreign = facts(APP, CODE_EKU);
    foreign.signed_app_id = "foreign.dev.keld";
    assert!(rejection(&role, &foreign).contains("signed app ID"));
}

#[test]
fn same_ca_foreign_third_leaf_and_each_pin_substitution_refuse() {
    let role = policy(WindowsD45HostRole::Ordinary);
    let changed = [
        (
            DevSignerPins {
                root_certificate_sha256: [0x77; 32],
                ..APP
            },
            "foreign root",
        ),
        (
            DevSignerPins {
                leaf_issuer_spki_sha256: [0x77; 32],
                ..APP
            },
            "foreign leaf issuer",
        ),
        (
            DevSignerPins {
                leaf_certificate_sha256: [0x77; 32],
                ..APP
            },
            "foreign leaf certificate",
        ),
        (
            DevSignerPins {
                leaf_spki_sha256: [0x77; 32],
                ..APP
            },
            "foreign leaf SPKI",
        ),
    ];
    for (foreign_pins, detail) in changed {
        assert!(
            rejection(&role, &facts(foreign_pins, CODE_EKU)).contains(detail),
            "the changed {detail} must not inherit approval from a matching CA"
        );
    }
    let third_leaf = DevSignerPins {
        leaf_certificate_sha256: [0x88; 32],
        leaf_spki_sha256: [0x99; 32],
        ..APP
    };
    assert!(rejection(&role, &facts(third_leaf, CODE_EKU)).contains("foreign leaf certificate"));
}

#[test]
fn compromised_or_ambiguous_dev_role_policy_refuses_before_leaf_match() {
    let mut role = policy(WindowsD45HostRole::Ordinary);
    role.recovery.leaf_spki_sha256 = APP.leaf_spki_sha256;
    assert!(rejection(&role, &facts(APP, CODE_EKU)).contains("share a leaf key"));
    role = policy(WindowsD45HostRole::Ordinary);
    role.recovery.leaf_certificate_sha256 = APP.leaf_certificate_sha256;
    assert!(rejection(&role, &facts(APP, CODE_EKU)).contains("share a leaf key"));
    role = policy(WindowsD45HostRole::Ordinary);
    role.recovery.root_certificate_sha256 = [0x77; 32];
    assert!(rejection(&role, &facts(APP, CODE_EKU)).contains("pinned local CA"));
    role = policy(WindowsD45HostRole::Ordinary);
    role.recovery.leaf_issuer_spki_sha256 = [0x77; 32];
    assert!(rejection(&role, &facts(APP, CODE_EKU)).contains("pinned local CA"));
}

#[test]
fn dev_signer_refuses_missing_duplicate_or_public_trust_ekus() {
    let role = policy(WindowsD45HostRole::Ordinary);
    for ekus in [
        &[][..],
        &[PUBLIC_TRUST_OID][..],
        &[CODE_SIGNING_OID, CODE_SIGNING_OID][..],
        &[CODE_SIGNING_OID, PUBLIC_TRUST_OID][..],
        &[CODE_SIGNING_OID, ANY_EKU_OID][..],
    ] {
        assert!(
            rejection(&role, &facts(APP, ekus)).contains("forbidden EKU"),
            "signed dev EKU set {ekus:?} must refuse"
        );
    }
}

mod receipt_model;

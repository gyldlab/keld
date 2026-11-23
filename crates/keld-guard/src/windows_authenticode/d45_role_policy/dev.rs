//! Private comparison of local-development signer pins and authenticated-role inputs.
//!
//! The production Public Trust policy remains separate. These values must
//! eventually come from independent, authenticated D-WIRE policy and the same
//! retained native WinTrust signer. This module has neither producer and
//! exports no admission, verified-image, installer permit or native FFI.

use super::{CODE_SIGNING_OID, PUBLIC_TRUST_OID, WindowsD45HostRole, contains_once, refusal};
use crate::windows_authenticode::WindowsAuthenticodeError;

const DEV_PROVIDER: u8 = 2;
const DEV_ROOT_POLICY: u8 = 3;
const ANY_EKU_OID: &str = "2.5.29.37.0";

/// SHA-256 facts for one native selected signer, not a trust decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DevSignerPins {
    root_certificate_sha256: [u8; 32],
    leaf_issuer_spki_sha256: [u8; 32],
    leaf_certificate_sha256: [u8; 32],
    leaf_spki_sha256: [u8; 32],
}

/// Exact expected role selected by the authenticated operation, not the image.
/// The two roles share one pinned local CA but must use distinct leaf keys.
struct DevRolePolicy<'a> {
    provider: u8,
    root_policy: u8,
    app_id: &'a str,
    role: WindowsD45HostRole,
    app: DevSignerPins,
    recovery: DevSignerPins,
}

/// Future observations from one live verified primary signer on one retained hFile.
struct DevSignerFacts<'a> {
    signed_app_id: &'a str,
    leaf_eku_oids: &'a [&'a str],
    pins: DevSignerPins,
}

/// Pure comparison success cannot become an Authenticode or D-46 permit.
#[must_use = "development pin consistency is not installation authorization"]
struct DevRolePolicyMatch {
    _non_authority: (),
}

fn compare_dev_signer(
    policy: &DevRolePolicy<'_>,
    facts: &DevSignerFacts<'_>,
) -> Result<DevRolePolicyMatch, WindowsAuthenticodeError> {
    if (policy.provider, policy.root_policy) != (DEV_PROVIDER, DEV_ROOT_POLICY) {
        return Err(refusal(
            "the development provider and root policy pair is not (2,3)",
        ));
    }
    if policy.app_id.is_empty() || facts.signed_app_id != policy.app_id {
        return Err(refusal(
            "the development signer's signed app ID differs from policy",
        ));
    }
    if policy.app.leaf_spki_sha256 == policy.recovery.leaf_spki_sha256
        || policy.app.leaf_certificate_sha256 == policy.recovery.leaf_certificate_sha256
    {
        return Err(refusal(
            "development APP and RECOVERY share a leaf key or certificate",
        ));
    }
    if policy.app.root_certificate_sha256 != policy.recovery.root_certificate_sha256
        || policy.app.leaf_issuer_spki_sha256 != policy.recovery.leaf_issuer_spki_sha256
    {
        return Err(refusal(
            "development APP and RECOVERY do not share the pinned local CA",
        ));
    }
    if !contains_once(facts.leaf_eku_oids, CODE_SIGNING_OID)
        || facts.leaf_eku_oids.contains(&PUBLIC_TRUST_OID)
        || facts.leaf_eku_oids.contains(&ANY_EKU_OID)
    {
        return Err(refusal(
            "development signer lacks exact code signing or has forbidden EKU",
        ));
    }
    let expected = match policy.role {
        WindowsD45HostRole::Ordinary => policy.app,
        WindowsD45HostRole::DedicatedRecovery => policy.recovery,
    };
    if facts.pins.root_certificate_sha256 != expected.root_certificate_sha256 {
        return Err(refusal("development signer has a foreign root certificate"));
    }
    if facts.pins.leaf_issuer_spki_sha256 != expected.leaf_issuer_spki_sha256 {
        return Err(refusal("development signer has a foreign leaf issuer SPKI"));
    }
    if facts.pins.leaf_certificate_sha256 != expected.leaf_certificate_sha256 {
        return Err(refusal("development signer has a foreign leaf certificate"));
    }
    if facts.pins.leaf_spki_sha256 != expected.leaf_spki_sha256 {
        return Err(refusal("development signer has a foreign leaf SPKI"));
    }
    Ok(DevRolePolicyMatch { _non_authority: () })
}

#[cfg(test)]
mod tests;

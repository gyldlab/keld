//! D-45 issuer/profile/role comparison over independently authenticated inputs.
//!
//! This module owns no `WinTrust` state, native certificate reader, signer key,
//! installation permit, or public API. T-82 must first commission a native
//! producer for the signer leaf's exact EKUs, root certificate hash and immediate
//! issuer SPKI, and Pack's retained-handle D-WIRE identity. There is deliberately
//! no production constructor for these combined observations. A successful
//! predicate is data consistency only, not `Authenticode` or installer authority.

use sha2::{Digest as _, Sha256};

use super::refusals::{WindowsAuthenticodeError, windows_identity_error};

#[allow(
    dead_code,
    reason = "T82: native signer-facts producer not commissioned"
)]
const CODE_SIGNING_OID: &str = "1.3.6.1.5.5.7.3.3";
#[allow(
    dead_code,
    reason = "T82: native signer-facts producer not commissioned"
)]
const PUBLIC_TRUST_OID: &str = "1.3.6.1.4.1.311.97.1.0";
#[allow(
    dead_code,
    reason = "T82: authenticated issuer-policy producer not commissioned"
)]
const MAX_ISSUER_POLICY_BYTES: usize = 65_536;

/// The operation selected by an authenticated owner, never by the incoming Host.
#[allow(dead_code, reason = "T82: authenticated role owner not commissioned")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WindowsD45HostRole {
    Ordinary,
    DedicatedRecovery,
}

/// Source and destination issuer policies serve different artifact roles.
/// D-7 does not require the U2 candidate to be signed by a U1-eligible issuer:
/// the admitted U1 source is independently checked under its source policy,
/// and only the source-authorized transition selects the U2 destination policy.
#[allow(dead_code, reason = "T82: D7 transition witness not commissioned")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WindowsD45IssuerOperation {
    UnchangedPolicy,
    D7DestinationCandidate,
}

/// A canonical membership proof minted by the single D-WIRE codec from the
/// same authenticated issuer-policy bytes. There is no variant or constructor
/// until the approved Pack typed producer owns this exact binding. Hence an
/// arbitrary pair of hash arrays can NEVER produce an issuer match today.
#[allow(
    dead_code,
    reason = "T82: Pack D-WIRE binding producer not commissioned"
)]
#[derive(Debug)]
pub(super) enum WindowsD45DWireMembershipBinding {}

/// Canonical issuer-policy bytes and its *independent* signed Host digest.
/// D-WIRE must validate the closed JSON and bind these rows to D-9 metadata.
/// An authenticated byte digest alone does not authenticate separately
/// supplied membership arrays. The uninhabited D-WIRE binding witness makes
/// any logical positive impossible until the real typed producer exists.
#[allow(
    dead_code,
    reason = "T82: authenticated issuer-policy producer not commissioned"
)]
#[derive(Debug)]
struct WindowsD45IssuerPolicy<'a> {
    bytes: &'a [u8],
    signed_digest: &'a [u8; 32],
    root_certificate_hashes: &'a [[u8; 32]],
    leaf_issuer_spki_hashes: &'a [[u8; 32]],
    wire_membership_binding: Option<WindowsD45DWireMembershipBinding>,
}

/// The commissioned exact operation-policy cohort. All these fields must come
/// from authenticated source, destination and issuer-policy records, not from
/// a candidate's self-asserted profile or a request.
#[allow(
    dead_code,
    reason = "T82: authenticated policy cohort producer not commissioned"
)]
#[derive(Debug)]
struct WindowsD45ProfilePolicy<'a> {
    role: WindowsD45HostRole,
    issuer_operation: WindowsD45IssuerOperation,
    app_id: &'a str,
    application_profile_oid: &'a str,
    source_recovery_profile_oid: &'a str,
    destination_recovery_profile_oid: &'a str,
    source_issuer: WindowsD45IssuerPolicy<'a>,
    destination_issuer: WindowsD45IssuerPolicy<'a>,
}

/// Observations to be issued only by Guard's future `WinTrust` leaf and chain
/// decoder while the verified provider state remains live. No arbitrary public
/// Boolean for "`WinTrust` succeeded" or "profile valid" is admitted.
#[allow(
    dead_code,
    reason = "T82: native WinTrust signer observer not commissioned"
)]
#[derive(Debug)]
struct WindowsD45NativeSignerFacts<'a> {
    signed_app_id: &'a str,
    leaf_eku_oids: &'a [&'a str],
    root_certificate_sha256: [u8; 32],
    leaf_issuer_spki_sha256: [u8; 32],
}

/// D-45 pure comparison result. Cannot create `VerifiedWindowsImage` or a D-46
/// effect permit. No field is exported or usable as signer authority.
#[allow(dead_code, reason = "T82: verified policy binding not commissioned")]
#[must_use = "policy data consistency alone is never signer authority"]
pub(super) struct WindowsD45RolePolicyMatch {
    _non_authority: (),
}

#[allow(
    dead_code,
    reason = "T82: policy comparison not wired without native issuer"
)]
fn refusal(detail: &'static str) -> WindowsAuthenticodeError {
    windows_identity_error(
        detail,
        "Restore the independently commissioned signing, issuer and role policy; do not bypass Windows signature verification.",
    )
}

#[allow(dead_code, reason = "T82: role check not wired without native issuer")]
fn contains_once(values: &[&str], exact: &str) -> bool {
    values.iter().copied().filter(|oid| *oid == exact).count() == 1
}

#[allow(
    dead_code,
    reason = "T82: canonical Pack issuer-policy binding pending"
)]
fn require_issuer(
    policy: &WindowsD45IssuerPolicy<'_>,
    facts: &WindowsD45NativeSignerFacts<'_>,
) -> Result<(), WindowsAuthenticodeError> {
    if policy.bytes.is_empty() || policy.bytes.len() > MAX_ISSUER_POLICY_BYTES {
        return Err(refusal(
            "the authenticated issuer policy bytes are missing or out of bounds",
        ));
    }
    if Sha256::digest(policy.bytes)[..] != policy.signed_digest[..] {
        return Err(refusal(
            "the issuer policy bytes differ from the signed issuer-policy digest",
        ));
    }
    // D-WIRE exclusively checks the list count, sorted canonical JSON and
    // uniqueness before it creates a retained authenticated policy. Guard
    // checks only the native root and immediate-issuer membership predicate.
    if policy.root_certificate_hashes.is_empty() || policy.leaf_issuer_spki_hashes.is_empty() {
        return Err(refusal(
            "the authenticated issuer policy has no root or leaf issuer entries",
        ));
    }
    if policy.wire_membership_binding.is_none() {
        return Err(refusal(
            "the issuer membership arrays are not bound to canonical authenticated D-WIRE policy bytes",
        ));
    }
    if !policy
        .root_certificate_hashes
        .contains(&facts.root_certificate_sha256)
    {
        return Err(refusal(
            "the native verified root certificate is outside the commissioned issuer policy",
        ));
    }
    if !policy
        .leaf_issuer_spki_hashes
        .contains(&facts.leaf_issuer_spki_sha256)
    {
        return Err(refusal(
            "the native verified leaf issuer SPKI is outside the commissioned issuer policy",
        ));
    }
    Ok(())
}

/// Compare complete exact policy and native signer facts for one Host role.
///
/// Missing native evidence or authenticated D-WIRE facts always refuses. A
/// result cannot be used by Guard's existing legacy verify or KEL-53 writer:
/// no native producer, public constructor or privilege conversion exists.
#[allow(dead_code, reason = "T82: signed Host role verifier not commissioned")]
fn require_d45_issuer_profile_role(
    policy: Option<&WindowsD45ProfilePolicy<'_>>,
    native: Option<&WindowsD45NativeSignerFacts<'_>>,
) -> Result<WindowsD45RolePolicyMatch, WindowsAuthenticodeError> {
    let policy = policy
        .ok_or_else(|| refusal("the authenticated D-WIRE issuer and role policy is unavailable"))?;
    let facts = native.ok_or_else(|| {
        refusal("the native WinTrust issuer and signer EKU evidence is unavailable")
    })?;
    if policy.app_id.is_empty() || facts.signed_app_id != policy.app_id {
        return Err(refusal(
            "the WinTrust signed app ID differs from the authenticated Host identity",
        ));
    }
    if policy.application_profile_oid.is_empty()
        || policy.source_recovery_profile_oid.is_empty()
        || policy.destination_recovery_profile_oid.is_empty()
        || matches!(
            policy.application_profile_oid,
            CODE_SIGNING_OID | PUBLIC_TRUST_OID
        )
        || matches!(
            policy.source_recovery_profile_oid,
            CODE_SIGNING_OID | PUBLIC_TRUST_OID
        )
        || matches!(
            policy.destination_recovery_profile_oid,
            CODE_SIGNING_OID | PUBLIC_TRUST_OID
        )
        || policy.application_profile_oid == policy.source_recovery_profile_oid
        || policy.application_profile_oid == policy.destination_recovery_profile_oid
        || (policy.role == WindowsD45HostRole::DedicatedRecovery
            && policy.source_recovery_profile_oid != policy.destination_recovery_profile_oid)
    {
        return Err(refusal(
            "the commissioned application and recovery profiles are missing or overlap",
        ));
    }
    if !contains_once(facts.leaf_eku_oids, CODE_SIGNING_OID)
        || !contains_once(facts.leaf_eku_oids, PUBLIC_TRUST_OID)
    {
        return Err(refusal(
            "the verified primary signer lacks the required exact code-signing or Public Trust EKU",
        ));
    }
    let required_profile_oid = match policy.role {
        WindowsD45HostRole::Ordinary => policy.application_profile_oid,
        WindowsD45HostRole::DedicatedRecovery => policy.source_recovery_profile_oid,
    };
    if !contains_once(facts.leaf_eku_oids, required_profile_oid) {
        return Err(refusal(
            "the verified signer lacks the exact commissioned role profile OID",
        ));
    }
    let forbidden_profile_oids = match policy.role {
        WindowsD45HostRole::Ordinary => [
            policy.source_recovery_profile_oid,
            policy.destination_recovery_profile_oid,
        ],
        WindowsD45HostRole::DedicatedRecovery => [
            policy.application_profile_oid,
            policy.application_profile_oid,
        ],
    };
    if forbidden_profile_oids
        .iter()
        .any(|oid| facts.leaf_eku_oids.contains(oid))
    {
        return Err(refusal(
            "the verified signer carries both application and recovery-only profile roles",
        ));
    }
    match policy.issuer_operation {
        WindowsD45IssuerOperation::UnchangedPolicy => {
            // Ordinary/unchanged operations may reuse the installed issuer
            // policy; without an authorized D-7 transition, different policy
            // digests cannot silently widen the current cohort.
            if policy.source_issuer.signed_digest != policy.destination_issuer.signed_digest {
                return Err(refusal(
                    "source and destination issuer policies differ without authenticated D-7 authorization",
                ));
            }
            require_issuer(&policy.source_issuer, facts)?;
            require_issuer(&policy.destination_issuer, facts)?;
        }
        WindowsD45IssuerOperation::D7DestinationCandidate => {
            // The candidate's signer must match DESTINATION issuer policy
            // only. U1 is separately verified against the admitted SOURCE
            // policy; an authenticated transition plus U1/U2 proofs must be
            // issued by the KEL-53/D-7 owner before this route can proceed.
            // There is no such typed transition witness on this base.
            return Err(refusal(
                "D-7 candidate issuer rotation lacks authenticated source-transition and destination-policy authorization",
            ));
        }
    }
    Ok(WindowsD45RolePolicyMatch { _non_authority: () })
}

#[cfg(test)]
mod tests;

// Local development uses distinct, pinned APP/RECOVERY leaves rather than
// production Public Trust profile OIDs. T-82 has not supplied an authenticated
// policy or native signer producer; this pure comparator grants no authority.
#[allow(
    dead_code,
    reason = "T82: development signer issuer is not commissioned"
)]
mod dev;

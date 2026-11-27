//! Private KEL-270 D-4 expected-identity v2 *byte contract*.
//!
//! This module is compiled only with the opt-in `kel270-dwire-v2` feature
//! or in crate-local tests. Shipping default builds do not include it.
//! Neither encoding nor decoding authenticates an issuer, an Ed25519 key, an
//! Authenticode signature, the commissioned profile, or installation authority.
//! T-82 Pack API/wire review and Guard's independent native checks remain open.

use crate::PackError;

const DOMAIN: &[u8] = b"keld.expected-app-identity/v2\0";
pub(super) const MIN_PAYLOAD_BYTES: usize = 206;
pub(super) const MAX_PAYLOAD_BYTES: usize = 790;
const PROVIDER_PUBLIC_TRUST: u8 = 1;
const PROVIDER_LOCAL_DEVELOPMENT: u8 = 2;
const OWNER_DIRECT: u8 = 1;
const MODE_PER_USER_DIRECT: u8 = 1;
const PRINCIPAL_STRICT: u8 = 1;
const ROOT_V2: u8 = 2;
const ROOT_LOCAL_DEVELOPMENT_V2: u8 = 3;

/// The provider determines the grammar of the existing D-4 lp16 field.
/// Neither variant authenticates the signer or commissions a trust policy.
#[derive(Debug, Clone, Copy)]
pub(crate) enum V2SigningProfile<'a> {
    PublicTrustOid(&'a str),
    LocalDevelopmentAppSpkiSha256(&'a str),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum V2SigningProfileValue {
    PublicTrustOid(Box<str>),
    LocalDevelopmentAppSpkiSha256(Box<str>),
}

impl V2SigningProfileValue {
    fn wire_fields(&self) -> (u8, u8, &str) {
        match self {
            Self::PublicTrustOid(value) => (PROVIDER_PUBLIC_TRUST, ROOT_V2, value),
            Self::LocalDevelopmentAppSpkiSha256(value) => {
                (PROVIDER_LOCAL_DEVELOPMENT, ROOT_LOCAL_DEVELOPMENT_V2, value)
            }
        }
    }
}

/// Constructor inputs. These are observations, never a permit or signer policy.
#[derive(Debug, Clone, Copy)]
pub(crate) struct V2IdentityFields<'a> {
    pub(crate) app_id: &'a str,
    pub(crate) channel: &'a str,
    pub(crate) target: &'a str,
    pub(crate) update_public_key: [u8; 32],
    pub(crate) signing_profile: V2SigningProfile<'a>,
    pub(crate) issuer_policy_sha256: [u8; 32],
    pub(crate) repository_id: [u8; 32],
    pub(crate) repository_policy_sha256: [u8; 32],
    pub(crate) discovery_policy_sha256: [u8; 32],
}

/// Canonical untrusted v2 payload. Private fields prevent an unchecked
/// caller from constructing a value that bypasses the wire checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExpectedAppIdentityV2 {
    app_id: Box<str>,
    channel: &'static str,
    signing_profile: V2SigningProfileValue,
    update_public_key: [u8; 32],
    issuer_policy_sha256: [u8; 32],
    repository_id: [u8; 32],
    repository_policy_sha256: [u8; 32],
    discovery_policy_sha256: [u8; 32],
}

impl ExpectedAppIdentityV2 {
    pub(crate) fn new(fields: V2IdentityFields<'_>) -> Result<Self, PackError> {
        // Pack owns length and ASCII encoding only. KEL-135 alone owns the
        // canonical app-id grammar and checks it against verified signer data.
        if !(1..=255).contains(&fields.app_id.len()) || !fields.app_id.is_ascii() {
            return Err(invalid("v2 app id"));
        }
        let channel = match fields.channel {
            "stable" => "stable",
            "beta" => "beta",
            "canary" => "canary",
            _ => return Err(invalid("v2 channel")),
        };
        if fields.target != "windows-x64" {
            return Err(invalid("v2 target"));
        }
        let signing_profile = match fields.signing_profile {
            V2SigningProfile::PublicTrustOid(oid) if canonical_profile_oid(oid) => {
                V2SigningProfileValue::PublicTrustOid(oid.into())
            }
            V2SigningProfile::LocalDevelopmentAppSpkiSha256(hash)
                if canonical_app_spki_sha256(hash) =>
            {
                V2SigningProfileValue::LocalDevelopmentAppSpkiSha256(hash.into())
            }
            V2SigningProfile::PublicTrustOid(_) => return Err(invalid("v2 profile OID")),
            V2SigningProfile::LocalDevelopmentAppSpkiSha256(_) => {
                return Err(invalid("v2 APP SPKI SHA256"));
            }
        };
        Ok(Self {
            app_id: fields.app_id.into(),
            channel,
            signing_profile,
            update_public_key: fields.update_public_key,
            issuer_policy_sha256: fields.issuer_policy_sha256,
            repository_id: fields.repository_id,
            repository_policy_sha256: fields.repository_policy_sha256,
            discovery_policy_sha256: fields.discovery_policy_sha256,
        })
    }

    /// Serializes only the closed D-4 v2 field order. This output is not signed.
    pub(crate) fn encode(&self) -> Result<Vec<u8>, PackError> {
        let (provider, root_policy, signer_identifier) = self.signing_profile.wire_fields();
        let mut out = Vec::with_capacity(
            DOMAIN.len()
                + 3
                + self.app_id.len()
                + self.channel.len()
                + 11
                + 32
                + 1
                + 2
                + signer_identifier.len()
                + 4 * 32
                + 4,
        );
        out.extend_from_slice(DOMAIN);
        put_lp8(&mut out, self.app_id.as_bytes())?;
        put_lp8(&mut out, self.channel.as_bytes())?;
        put_lp8(&mut out, b"windows-x64")?;
        out.extend_from_slice(&self.update_public_key);
        out.push(provider);
        put_lp16(&mut out, signer_identifier.as_bytes())?;
        out.extend_from_slice(&self.issuer_policy_sha256);
        out.extend_from_slice(&self.repository_id);
        out.extend_from_slice(&self.repository_policy_sha256);
        out.extend_from_slice(&self.discovery_policy_sha256);
        out.extend_from_slice(&[
            OWNER_DIRECT,
            MODE_PER_USER_DIRECT,
            PRINCIPAL_STRICT,
            root_policy,
        ]);
        if !(MIN_PAYLOAD_BYTES..=MAX_PAYLOAD_BYTES).contains(&out.len()) {
            return Err(invalid("v2 payload size"));
        }
        Ok(out)
    }

    /// Production D-4 byte reader. It still refuses local-development bytes.
    /// No fallback to any other provider or legacy identity is permitted.
    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, PackError> {
        Self::decode_for_provider(bytes, PROVIDER_PUBLIC_TRUST)
    }

    /// Private, nonauthorizing local-development byte decoder for the retained
    /// PE observer. It is compiled only in the opt-in D-WIRE module or tests;
    /// no Host, `WinTrust`, issuer-policy, or permit owner consumes its output.
    /// Production PE readers continue to call `decode` only.
    pub(crate) fn decode_local_development(bytes: &[u8]) -> Result<Self, PackError> {
        Self::decode_for_provider(bytes, PROVIDER_LOCAL_DEVELOPMENT)
    }

    fn decode_for_provider(bytes: &[u8], expected_provider: u8) -> Result<Self, PackError> {
        if !(MIN_PAYLOAD_BYTES..=MAX_PAYLOAD_BYTES).contains(&bytes.len()) {
            return Err(invalid("v2 payload size"));
        }
        let mut rest = bytes
            .strip_prefix(DOMAIN)
            .ok_or_else(|| invalid("v2 domain"))?;
        let app_id = take_lp8(&mut rest, "v2 app id")?;
        let channel = take_lp8(&mut rest, "v2 channel")?;
        let target = take_lp8(&mut rest, "v2 target")?;
        let update_public_key = take_32(&mut rest, "v2 update key")?;
        if take_byte(&mut rest, "v2 provider")? != expected_provider {
            return Err(invalid("v2 provider"));
        }
        let signing_profile = if expected_provider == PROVIDER_PUBLIC_TRUST {
            V2SigningProfile::PublicTrustOid(take_lp16(&mut rest, "v2 profile OID")?)
        } else {
            V2SigningProfile::LocalDevelopmentAppSpkiSha256(take_lp16(
                &mut rest,
                "v2 APP SPKI SHA256",
            )?)
        };
        let issuer_policy_sha256 = take_32(&mut rest, "v2 issuer policy")?;
        let repository_id = take_32(&mut rest, "v2 repository id")?;
        let repository_policy_sha256 = take_32(&mut rest, "v2 repository policy")?;
        let discovery_policy_sha256 = take_32(&mut rest, "v2 discovery policy")?;
        for (expected, name) in [
            (OWNER_DIRECT, "v2 owner"),
            (MODE_PER_USER_DIRECT, "v2 installation mode"),
            (PRINCIPAL_STRICT, "v2 principal model"),
        ] {
            if take_byte(&mut rest, name)? != expected {
                return Err(invalid(name));
            }
        }
        let expected_root = if expected_provider == PROVIDER_PUBLIC_TRUST {
            ROOT_V2
        } else {
            ROOT_LOCAL_DEVELOPMENT_V2
        };
        if take_byte(&mut rest, "v2 root policy")? != expected_root {
            return Err(invalid("v2 root policy"));
        }
        if !rest.is_empty() {
            return Err(invalid("v2 trailing bytes"));
        }
        let payload = Self::new(V2IdentityFields {
            app_id,
            channel,
            target,
            update_public_key,
            signing_profile,
            issuer_policy_sha256,
            repository_id,
            repository_policy_sha256,
            discovery_policy_sha256,
        })?;
        // Enforce one byte spelling as well as typed field validity.
        if payload.encode()?.as_slice() != bytes {
            return Err(invalid("v2 noncanonical bytes"));
        }
        Ok(payload)
    }
}

/// The DEV lp16 field is an APP leaf SPKI SHA-256 identifier, not an OID or
/// an Authenticode/issuer-policy verification result.
fn canonical_app_spki_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

fn take_byte(rest: &mut &[u8], name: &'static str) -> Result<u8, PackError> {
    let (&value, remaining) = rest.split_first().ok_or_else(|| invalid(name))?;
    *rest = remaining;
    Ok(value)
}

fn take_bytes<'a>(
    rest: &mut &'a [u8],
    n: usize,
    name: &'static str,
) -> Result<&'a [u8], PackError> {
    if rest.len() < n {
        return Err(invalid(name));
    }
    let (value, remaining) = rest.split_at(n);
    *rest = remaining;
    Ok(value)
}

fn take_32(rest: &mut &[u8], name: &'static str) -> Result<[u8; 32], PackError> {
    take_bytes(rest, 32, name)?
        .try_into()
        .map_err(|_| invalid(name))
}

fn take_lp8<'a>(rest: &mut &'a [u8], name: &'static str) -> Result<&'a str, PackError> {
    let n = usize::from(take_byte(rest, name)?);
    if n == 0 {
        return Err(invalid(name));
    }
    let bytes = take_bytes(rest, n, name)?;
    std::str::from_utf8(bytes).map_err(|_| invalid(name))
}

fn take_lp16<'a>(rest: &mut &'a [u8], name: &'static str) -> Result<&'a str, PackError> {
    let length: [u8; 2] = take_bytes(rest, 2, name)?
        .try_into()
        .map_err(|_| invalid(name))?;
    let n = usize::from(u16::from_be_bytes(length));
    if !(3..=255).contains(&n) {
        return Err(invalid(name));
    }
    let bytes = take_bytes(rest, n, name)?;
    std::str::from_utf8(bytes).map_err(|_| invalid(name))
}

fn put_lp8(out: &mut Vec<u8>, value: &[u8]) -> Result<(), PackError> {
    let n = u8::try_from(value.len()).map_err(|_| invalid("v2 lp8 length"))?;
    if n == 0 {
        return Err(invalid("v2 lp8 length"));
    }
    out.push(n);
    out.extend_from_slice(value);
    Ok(())
}

fn put_lp16(out: &mut Vec<u8>, value: &[u8]) -> Result<(), PackError> {
    if !(3..=255).contains(&value.len()) {
        return Err(invalid("v2 lp16 length"));
    }
    let n = u16::try_from(value.len()).map_err(|_| invalid("v2 lp16 length"))?;
    out.extend_from_slice(&n.to_be_bytes());
    out.extend_from_slice(value);
    Ok(())
}

/// The D-2 canonical dotted-decimal OID grammar only. The independent Guard
/// verifier, not the byte codec, decides whether the exact OID is commissioned.
fn canonical_profile_oid(oid: &str) -> bool {
    if !(3..=255).contains(&oid.len()) || !oid.is_ascii() {
        return false;
    }
    // The two generic usage EKUs named by D-3 do not identify a commissioned
    // application/recovery profile. Other exact role exclusions remain Guard-owned.
    if matches!(oid, "1.3.6.1.5.5.7.3.3" | "1.3.6.1.4.1.311.97.1.0") {
        return false;
    }
    let mut arcs = oid.split('.');
    let Some(first) = arcs.next().and_then(canonical_arc) else {
        return false;
    };
    let Some(second) = arcs.next().and_then(canonical_arc) else {
        return false;
    };
    if first > 2 || (first < 2 && second > 39) {
        return false;
    }
    let mut count = 2;
    for arc in arcs {
        count += 1;
        if count > 64 || canonical_arc(arc).is_none() {
            return false;
        }
    }
    true
}

fn canonical_arc(arc: &str) -> Option<u32> {
    if arc.is_empty()
        || (arc.len() > 1 && arc.starts_with('0'))
        || !arc.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    arc.parse::<u32>().ok()
}

const fn invalid(detail: &'static str) -> PackError {
    PackError::ExpectedIdentityInvalid { detail }
}

#[cfg(test)]
mod tests;

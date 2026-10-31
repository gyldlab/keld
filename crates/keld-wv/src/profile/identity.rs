//! Canonical verified app identity, Apple UUID and explicit ephemeral profile selection.
//! Domain-separation hashes and strict encoding remain private to this owner.

use core::fmt;
use sha2::{Digest as _, Sha256};

use super::{ProfileError, ProfileErrorKind};

const PROFILE_IDENTITY_DOMAIN: &[u8] = b"keld.profile.identity/v1\0";
const APPLE_STORE_DOMAIN: &[u8] = b"keld.wk-store/v1\0";

/// Opaque 32-byte identity derived from authenticated publisher and app-id parts.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProfileIdentity([u8; 32]);

impl ProfileIdentity {
    /// Derives an identity from bytes already authenticated by trusted host code.
    ///
    /// This function validates the app-id syntax and applies the approved hash
    /// construction. It does not authenticate the publisher scope or app id;
    /// callers must obtain both from a platform package verifier.
    ///
    /// # Errors
    ///
    /// Returns `KELD-WV-009` when the app id is not canonical.
    pub fn from_host_verified_parts(
        publisher_scope: [u8; 32],
        canonical_app_id: &str,
    ) -> Result<Self, ProfileError> {
        validate_canonical_app_id(canonical_app_id)?;
        let app_id_len = u16::try_from(canonical_app_id.len())
            .map_err(|_| ProfileError::new(ProfileErrorKind::InvalidAppId))?;

        let mut hasher = Sha256::new();
        hasher.update(PROFILE_IDENTITY_DOMAIN);
        hasher.update(publisher_scope);
        hasher.update(app_id_len.to_be_bytes());
        hasher.update(canonical_app_id.as_bytes());
        let digest = hasher.finalize();
        let mut bytes = [0_u8; 32];
        bytes.copy_from_slice(&digest);
        Ok(Self(bytes))
    }

    /// Returns the full identity bytes retained in every ownership record.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Returns the only canonical filesystem segment for this identity.
    #[must_use]
    pub fn namespace_segment(&self) -> String {
        encode_lower_hex(&self.0)
    }

    /// Derives the deterministic RFC-variant `UUIDv8` bytes for a macOS store.
    #[must_use]
    pub fn apple_store_uuid(&self) -> AppleStoreUuid {
        let mut hasher = Sha256::new();
        hasher.update(APPLE_STORE_DOMAIN);
        hasher.update(self.0);
        let digest = hasher.finalize();
        let mut bytes = [0_u8; 16];
        bytes.copy_from_slice(&digest[..16]);
        bytes[6] = (bytes[6] & 0x0f) | 0x80;
        bytes[8] = (bytes[8] & 0x3f) | 0x80;
        AppleStoreUuid(bytes)
    }

    pub(super) fn from_namespace_segment(value: &str) -> Result<Self, ProfileError> {
        decode_lower_hex::<32>(value).map(Self)
    }

    #[cfg(all(windows, feature = "media-acceptance", test))]
    pub(crate) fn from_test_namespace_segment(value: &str) -> Result<Self, ProfileError> {
        Self::from_namespace_segment(value)
    }
}

impl fmt::Debug for ProfileIdentity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("ProfileIdentity")
            .field(&self.namespace_segment())
            .finish()
    }
}

/// Deterministic `UUIDv8` used only as the shorter Apple store identifier.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AppleStoreUuid([u8; 16]);

impl AppleStoreUuid {
    /// Returns the canonical UUID bytes with RFC variant and version 8 bits.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    pub(super) fn parse(value: &str) -> Result<Self, ProfileError> {
        if value.len() != 36
            || value.as_bytes().get(8) != Some(&b'-')
            || value.as_bytes().get(13) != Some(&b'-')
            || value.as_bytes().get(18) != Some(&b'-')
            || value.as_bytes().get(23) != Some(&b'-')
        {
            return Err(ProfileError::new(ProfileErrorKind::InvalidRecord));
        }
        let compact: String = value
            .chars()
            .filter(|character| *character != '-')
            .collect();
        let bytes = decode_lower_hex::<16>(&compact)?;
        if bytes[6] >> 4 != 8 || bytes[8] >> 6 != 2 {
            return Err(ProfileError::new(ProfileErrorKind::InvalidRecord));
        }
        Ok(Self(bytes))
    }
}

impl fmt::Display for AppleStoreUuid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let hex = encode_lower_hex(&self.0);
        write!(
            f,
            "{}-{}-{}-{}-{}",
            &hex[0..8],
            &hex[8..12],
            &hex[12..16],
            &hex[16..20],
            &hex[20..32]
        )
    }
}

impl fmt::Debug for AppleStoreUuid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

/// Host-minted identity for one explicitly ephemeral development launch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EphemeralProfile {
    launch_nonce: [u8; 32],
}

impl EphemeralProfile {
    /// Creates a launch identity from fresh OS randomness owned by the host.
    ///
    /// # Errors
    ///
    /// Returns `KELD-WV-009` when the nonce is all zeroes.
    pub fn from_host_random(launch_nonce: [u8; 32]) -> Result<Self, ProfileError> {
        if launch_nonce.iter().all(|byte| *byte == 0) {
            return Err(ProfileError::new(ProfileErrorKind::InvalidEphemeralNonce));
        }
        Ok(Self { launch_nonce })
    }

    /// Returns the opaque nonce for a platform's owner-private ephemeral store.
    #[must_use]
    pub const fn launch_nonce(&self) -> &[u8; 32] {
        &self.launch_nonce
    }

    /// Returns the canonical owner-private namespace segment for this launch.
    #[must_use]
    pub fn namespace_segment(&self) -> String {
        encode_lower_hex(&self.launch_nonce)
    }

    #[cfg(target_os = "windows")]
    pub(crate) fn from_namespace_segment(value: &str) -> Result<Self, ProfileError> {
        Self::from_host_random(decode_lower_hex::<32>(value)?)
    }
}

/// Host-selected profile mode before any platform store is constructed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebProfileSelection {
    /// Persistent release identity supplied by an authenticated package verifier.
    Persistent(ProfileIdentity),
    /// Explicit per-launch development state with no persistent identity fallback.
    EphemeralDev(EphemeralProfile),
}

impl WebProfileSelection {
    /// Selects a persistent release profile or fails when identity is absent.
    ///
    /// # Errors
    ///
    /// Returns `KELD-WV-009` when release identity is absent.
    pub fn persistent(identity: Option<ProfileIdentity>) -> Result<Self, ProfileError> {
        identity
            .map(Self::Persistent)
            .ok_or_else(ProfileError::missing_authenticated_identity)
    }

    /// Selects one explicit per-launch ephemeral development profile.
    #[must_use]
    pub const fn ephemeral_dev(profile: EphemeralProfile) -> Self {
        Self::EphemeralDev(profile)
    }
}

fn validate_canonical_app_id(value: &str) -> Result<(), ProfileError> {
    if value.is_empty() || value.len() > 255 || !value.is_ascii() {
        return Err(ProfileError::new(ProfileErrorKind::InvalidAppId));
    }
    for segment in value.as_bytes().split(|byte| *byte == b'.') {
        let Some(first) = segment.first() else {
            return Err(ProfileError::new(ProfileErrorKind::InvalidAppId));
        };
        let Some(last) = segment.last() else {
            return Err(ProfileError::new(ProfileErrorKind::InvalidAppId));
        };
        if !is_lower_alphanumeric(*first)
            || !is_lower_alphanumeric(*last)
            || !segment
                .iter()
                .all(|byte| is_lower_alphanumeric(*byte) || *byte == b'-')
        {
            return Err(ProfileError::new(ProfileErrorKind::InvalidAppId));
        }
    }
    Ok(())
}

const fn is_lower_alphanumeric(byte: u8) -> bool {
    byte.is_ascii_lowercase() || byte.is_ascii_digit()
}

pub(super) fn encode_lower_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

pub(super) fn decode_lower_hex<const N: usize>(value: &str) -> Result<[u8; N], ProfileError> {
    if value.len() != N * 2
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(ProfileError::new(ProfileErrorKind::InvalidRecord));
    }
    let mut output = [0_u8; N];
    for (index, pair) in value.as_bytes().as_chunks::<2>().0.iter().enumerate() {
        output[index] = (decode_hex_nibble(pair[0])? << 4) | decode_hex_nibble(pair[1])?;
    }
    Ok(output)
}

fn decode_hex_nibble(byte: u8) -> Result<u8, ProfileError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        _ => Err(ProfileError::new(ProfileErrorKind::InvalidRecord)),
    }
}

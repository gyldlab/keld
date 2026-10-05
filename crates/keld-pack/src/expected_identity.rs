//! Canonical `ExpectedAppIdentity` payload bytes (KEL-254 amendment A3 §4, task T2b).
//!
//! keld-pack is the single owner of these bytes. It enforces only the byte layout and
//! field bounds: `keld-update` validates the channel against its channel set and the
//! key as an Ed25519 public key, and KEL-135 owns the app-id grammar. The payload is
//! non-secret and carries no private key material.
//!
//! Layout: [`EXPECTED_APP_IDENTITY_DOMAIN`], then app id, channel and target, each as a
//! one-byte length followed by that many UTF-8 bytes, then the 32-byte update-signing
//! public key, then end of input.

use crate::PackError;

/// Versioned domain tag that opens every payload.
pub const EXPECTED_APP_IDENTITY_DOMAIN: &[u8] = b"keld.expected-app-identity/v1\0";
/// Exact length of the expected update-signing Ed25519 public key.
pub const EXPECTED_APP_IDENTITY_KEY_BYTES: usize = 32;

const MAX_APP_ID_BYTES: usize = 255;
const MAX_CHANNEL_BYTES: usize = 16;
const MAX_TARGET_BYTES: usize = 64;

/// One bounded text field whose one-byte length was proven at construction.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Field {
    text: Box<str>,
    len: u8,
}

impl Field {
    fn new(text: &str, max: usize, name: &'static str) -> Result<Self, PackError> {
        if text.is_empty() || text.len() > max {
            return Err(invalid(name));
        }
        if text.chars().any(char::is_control) {
            return Err(invalid(name));
        }
        let len = u8::try_from(text.len()).map_err(|_| invalid(name))?;
        Ok(Self {
            text: text.into(),
            len,
        })
    }

    fn encode_into(&self, out: &mut Vec<u8>) {
        out.push(self.len);
        out.extend_from_slice(self.text.as_bytes());
    }
}

/// Byte-validated expected build identity: app id, channel, target and the expected
/// update-signing public key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpectedAppIdentityPayload {
    app_id: Field,
    channel: Field,
    target: Field,
    update_public_key: [u8; EXPECTED_APP_IDENTITY_KEY_BYTES],
}

impl ExpectedAppIdentityPayload {
    /// Builds a payload after checking every field's byte bounds.
    ///
    /// # Errors
    /// Returns `KELD-PACK-005` when the app id is not 1..=255 bytes, the channel is not
    /// 1..=16 bytes, the target is not 1..=64 bytes, or any field contains a control
    /// character.
    pub fn new(
        app_id: &str,
        channel: &str,
        target: &str,
        update_public_key: [u8; EXPECTED_APP_IDENTITY_KEY_BYTES],
    ) -> Result<Self, PackError> {
        Ok(Self {
            app_id: Field::new(app_id, MAX_APP_ID_BYTES, "app id")?,
            channel: Field::new(channel, MAX_CHANNEL_BYTES, "channel")?,
            target: Field::new(target, MAX_TARGET_BYTES, "target")?,
            update_public_key,
        })
    }

    /// Expected application id.
    #[must_use]
    pub fn app_id(&self) -> &str {
        &self.app_id.text
    }

    /// Expected update channel spelling; `keld-update` owns the channel set.
    #[must_use]
    pub fn channel(&self) -> &str {
        &self.channel.text
    }

    /// Expected target string, such as `windows-x64`.
    #[must_use]
    pub fn target(&self) -> &str {
        &self.target.text
    }

    /// Expected update-signing public key; `keld-update` validates it as Ed25519.
    #[must_use]
    pub const fn update_public_key(&self) -> &[u8; EXPECTED_APP_IDENTITY_KEY_BYTES] {
        &self.update_public_key
    }

    /// Canonical payload bytes.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(
            EXPECTED_APP_IDENTITY_DOMAIN.len()
                + 3
                + self.app_id.text.len()
                + self.channel.text.len()
                + self.target.text.len()
                + EXPECTED_APP_IDENTITY_KEY_BYTES,
        );
        out.extend_from_slice(EXPECTED_APP_IDENTITY_DOMAIN);
        self.app_id.encode_into(&mut out);
        self.channel.encode_into(&mut out);
        self.target.encode_into(&mut out);
        out.extend_from_slice(&self.update_public_key);
        out
    }

    /// Decodes exactly one canonical payload.
    ///
    /// # Errors
    /// Returns `KELD-PACK-005` for a wrong domain tag, a truncated or oversized field,
    /// a non-UTF-8 or control-bearing field, a short key, or any trailing byte.
    pub fn decode(bytes: &[u8]) -> Result<Self, PackError> {
        let mut rest = bytes
            .strip_prefix(EXPECTED_APP_IDENTITY_DOMAIN)
            .ok_or_else(|| invalid("domain tag"))?;
        let app_id = take_field(&mut rest, "app id")?;
        let channel = take_field(&mut rest, "channel")?;
        let target = take_field(&mut rest, "target")?;
        let key: [u8; EXPECTED_APP_IDENTITY_KEY_BYTES] = rest
            .try_into()
            .map_err(|_| invalid("public key length or trailing bytes"))?;
        Self::new(app_id, channel, target, key)
    }
}

fn take_field<'a>(rest: &mut &'a [u8], name: &'static str) -> Result<&'a str, PackError> {
    let (&len, tail) = rest.split_first().ok_or_else(|| invalid(name))?;
    let len = usize::from(len);
    if tail.len() < len {
        return Err(invalid(name));
    }
    let (field, tail) = tail.split_at(len);
    *rest = tail;
    std::str::from_utf8(field).map_err(|_| invalid(name))
}

fn invalid(detail: &'static str) -> PackError {
    PackError::ExpectedIdentityInvalid { detail }
}

#[cfg(test)]
mod tests;

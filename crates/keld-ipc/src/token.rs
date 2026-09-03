//! Session token carried in kipc v2 `HELLO` (KEL-60).
//!
//! The host mints 32 random bytes and puts them in `KELD_APP_LINK` as
//! `<endpoint>#<64 hex chars>`. Both peers must send those exact bytes as the
//! `HELLO` payload before any other frame is dispatched.

use crate::IpcError;

/// Length of a session token in bytes (not the hex encoding).
pub const SESSION_TOKEN_LEN: usize = 32;

/// 32-byte high-entropy session token.
///
/// `Debug` is redacted so logs cannot leak the secret. Equality is
/// constant-time (XOR-OR over all bytes) so a mismatched token does not
/// short-circuit on the first differing byte.
#[derive(Clone, Copy)]
pub struct SessionToken([u8; SESSION_TOKEN_LEN]);

impl core::fmt::Debug for SessionToken {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("SessionToken([redacted])")
    }
}

impl PartialEq for SessionToken {
    fn eq(&self, other: &Self) -> bool {
        let mut diff = 0u8;
        for (left, right) in self.0.iter().zip(other.0.iter()) {
            diff |= left ^ right;
        }
        diff == 0
    }
}

impl Eq for SessionToken {}

impl SessionToken {
    /// Mints a fresh cryptographically random session token.
    ///
    /// This runs only during cold role/bootstrap provisioning. The token is
    /// redacted by [`core::fmt::Debug`] and must be passed only through the
    /// canonical `KELD_APP_LINK` bootstrap contract.
    ///
    /// # Errors
    ///
    /// Returns [`std::io::Error`] if the operating system's cryptographic
    /// random source is unavailable.
    pub fn random() -> std::io::Result<Self> {
        let mut bytes = [0u8; SESSION_TOKEN_LEN];
        getrandom::fill(&mut bytes).map_err(|error| std::io::Error::other(error.to_string()))?;
        Ok(Self(bytes))
    }

    /// Wraps an already-minted 32-byte secret.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; SESSION_TOKEN_LEN]) -> Self {
        Self(bytes)
    }

    /// Token bytes for the `HELLO` payload.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; SESSION_TOKEN_LEN] {
        &self.0
    }

    /// Decodes a 64-character hex string (upper or lower case).
    ///
    /// # Errors
    ///
    /// Returns [`IpcError::HelloAuth`] (`KELD-IPC-007`) if `hex` is not exactly
    /// 64 hex digits.
    pub fn from_hex(hex: &str) -> Result<Self, IpcError> {
        let bytes = hex.as_bytes();
        if bytes.len() != SESSION_TOKEN_LEN * 2 {
            return Err(IpcError::HelloAuth {
                detail: "KELD_APP_LINK token must be 64 hex characters",
            });
        }
        let mut out = [0u8; SESSION_TOKEN_LEN];
        for (i, slot) in out.iter_mut().enumerate() {
            let hi = hex_nibble(bytes[i * 2])?;
            let lo = hex_nibble(bytes[i * 2 + 1])?;
            *slot = (hi << 4) | lo;
        }
        Ok(Self(out))
    }

    /// Lowercase hex encoding (64 characters).
    #[must_use]
    pub fn to_hex(self) -> String {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut out = String::with_capacity(SESSION_TOKEN_LEN * 2);
        for byte in self.0 {
            out.push(char::from(HEX[usize::from(byte >> 4)]));
            out.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
        out
    }

    /// Parses a `HELLO` payload as a token.
    ///
    /// # Errors
    ///
    /// Returns [`IpcError::HelloAuth`] if `bytes` is not exactly 32 bytes.
    /// Live handshake receivers validate the declared `HELLO` shape before
    /// calling this constructor; a wrong declared length is therefore
    /// [`IpcError::Protocol`], not this constructor's authentication error.
    pub fn try_from_slice(bytes: &[u8]) -> Result<Self, IpcError> {
        let Ok(arr) = <[u8; SESSION_TOKEN_LEN]>::try_from(bytes) else {
            return Err(IpcError::HelloAuth {
                detail: "HELLO payload must be a 32-byte session token",
            });
        };
        Ok(Self(arr))
    }
}

fn hex_nibble(byte: u8) -> Result<u8, IpcError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(IpcError::HelloAuth {
            detail: "KELD_APP_LINK token must be 64 hex characters",
        }),
    }
}

/// Splits `KELD_APP_LINK` / `--link` into endpoint and token.
///
/// The last `#` separates the endpoint (UDS path or TCP port) from the 64-char
/// hex token. Generated endpoints never contain `#`.
///
/// # Errors
///
/// Returns [`IpcError::HelloAuth`] if the `#` token is missing or not valid hex.
pub fn parse_app_link(link: &str) -> Result<(&str, SessionToken), IpcError> {
    let Some((endpoint, hex)) = link.rsplit_once('#') else {
        return Err(IpcError::HelloAuth {
            detail: "KELD_APP_LINK must be <endpoint>#<64 hex chars>",
        });
    };
    if endpoint.is_empty() {
        return Err(IpcError::HelloAuth {
            detail: "KELD_APP_LINK must be <endpoint>#<64 hex chars>",
        });
    }
    Ok((endpoint, SessionToken::from_hex(hex)?))
}

/// Formats a `KELD_APP_LINK` value.
#[must_use]
pub fn format_app_link(endpoint: &str, token: &SessionToken) -> String {
    let mut out = String::with_capacity(endpoint.len() + 1 + SESSION_TOKEN_LEN * 2);
    out.push_str(endpoint);
    out.push('#');
    out.push_str(&token.to_hex());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: [u8; SESSION_TOKEN_LEN] = [0xA5; SESSION_TOKEN_LEN];

    #[test]
    fn hex_roundtrip_lowercase() {
        let token = SessionToken::from_bytes(FIXTURE);
        let hex = token.to_hex();
        assert_eq!(hex.len(), 64);
        assert!(
            hex.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')),
            "must be lowercase hex: {hex}"
        );
        assert_eq!(SessionToken::from_hex(&hex).expect("parse"), token);
        assert_eq!(hex, "a5".repeat(32));
    }

    #[test]
    fn from_hex_accepts_uppercase() {
        let hex = "A5".repeat(32);
        let token = SessionToken::from_hex(&hex).expect("uppercase");
        assert_eq!(token, SessionToken::from_bytes(FIXTURE));
        assert_eq!(token.to_hex(), "a5".repeat(32));
    }

    #[test]
    fn from_hex_rejects_wrong_length_and_non_hex() {
        for bad in [
            "",
            "aa",
            &"a5".repeat(31),
            &"a5".repeat(33),
            &"zz".repeat(32),
        ] {
            let err = SessionToken::from_hex(bad).expect_err("must reject");
            assert!(matches!(err, IpcError::HelloAuth { .. }), "got {err}");
            let msg = err.to_string();
            assert!(msg.contains("KELD-IPC-007"), "{msg}");
            assert!(
                !msg.to_lowercase().contains("a5a5"),
                "must not echo a guessed secret: {msg}"
            );
        }
    }

    #[test]
    fn parse_app_link_rsplits_on_last_hash() {
        let token = SessionToken::from_bytes(FIXTURE);
        let hex = token.to_hex();
        let link = format!("/tmp/a#b#{hex}");
        let (endpoint, got) = parse_app_link(&link).expect("parse");
        assert_eq!(endpoint, "/tmp/a#b");
        assert_eq!(got, token);
    }

    #[test]
    fn parse_app_link_rejects_missing_or_empty_endpoint() {
        let hex = SessionToken::from_bytes(FIXTURE).to_hex();
        for bad in ["", "nopath", &format!("#{hex}")] {
            let err = parse_app_link(bad).expect_err("must reject");
            assert!(err.to_string().contains("KELD-IPC-007"), "{err}");
        }
    }

    #[test]
    fn format_then_parse_preserves_endpoint() {
        let token = SessionToken::from_bytes(FIXTURE);
        let link = format_app_link("/tmp/e.sock", &token);
        assert!(link.starts_with("/tmp/e.sock#"));
        let (endpoint, got) = parse_app_link(&link).expect("parse");
        assert_eq!(endpoint, "/tmp/e.sock");
        assert_eq!(got, token);
    }

    #[test]
    fn debug_does_not_leak_bytes() {
        let token = SessionToken::from_bytes(FIXTURE);
        let debug = format!("{token:?}");
        assert_eq!(debug, "SessionToken([redacted])");
        assert!(!debug.contains("a5"), "{debug}");
        assert!(!debug.contains("165"), "{debug}");
    }

    #[test]
    fn one_bit_flip_is_not_equal() {
        let a = SessionToken::from_bytes(FIXTURE);
        let mut flipped = FIXTURE;
        flipped[31] ^= 1;
        let b = SessionToken::from_bytes(flipped);
        assert_ne!(a, b);
        assert_eq!(a, SessionToken::from_bytes(FIXTURE));
    }

    #[test]
    fn try_from_slice_rejects_wrong_len() {
        assert!(SessionToken::try_from_slice(&[]).is_err());
        assert!(SessionToken::try_from_slice(&[0xA5; 31]).is_err());
        assert!(SessionToken::try_from_slice(&[0xA5; 33]).is_err());
        assert_eq!(
            SessionToken::try_from_slice(&FIXTURE).expect("32 bytes"),
            SessionToken::from_bytes(FIXTURE)
        );
    }
}

//! Pure decoder for the private isolated-world -> host `keld.wv-link/v1` envelope.
//!
//! This is deliberately not KIPC. It accepts only the bounded renderer control
//! shapes frozen by KEL-142 and carries no app-link endpoint, token, principal,
//! generation, grant, or other application authority.

use core::fmt;
use core::num::{NonZeroU16, NonZeroU32};

use serde::de::{Error as _, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};

/// Maximum application bytes admitted by one renderer invoke.
pub const MAX_RENDERER_PAYLOAD_LEN: usize = 4096;

/// Maximum encoded private renderer envelope accepted by the pure decoder.
///
/// A valid maximum-size payload encoded as decimal JSON bytes is well below
/// this ceiling. The bound exists so hostile `document`/`kind` strings cannot
/// make decoder allocation scale without limit before host identity checks.
pub const MAX_WV_LINK_ENVELOPE_LEN: usize = 64 * 1024;

const PAYLOAD_TOO_LARGE_SENTINEL: &str = "keld-wv-link-payload-too-large";

/// Fail-closed reason produced before renderer work reaches app dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WvLinkRejectReason {
    /// The bytes are not the exact accepted JSON shape.
    MalformedEnvelope,
    /// The private envelope version is not `1`.
    UnsupportedVersion,
    /// The application payload exceeds [`MAX_RENDERER_PAYLOAD_LEN`].
    PayloadTooLarge,
    /// The renderer-local request identifier is zero.
    ZeroRequest,
    /// The selected application channel is zero.
    ZeroChannel,
}

/// Exact private control shapes admitted from the isolated world.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WvLinkEnvelope {
    /// Bind the current host-owned main-frame navigation.
    Bind,
    /// Dispatch one bounded application request for an already-bound document.
    Invoke {
        /// Opaque host-minted document nonce. Authenticity is checked by the
        /// host owner after decoding; the page does not mint this value.
        document: String,
        /// Nonzero renderer-local request identifier.
        request: NonZeroU32,
        /// Nonzero application channel identifier.
        channel: NonZeroU16,
        /// Copied application payload bytes.
        payload: Vec<u8>,
    },
}

#[derive(Debug)]
struct BoundedPayload(Vec<u8>);

impl<'de> Deserialize<'de> for BoundedPayload {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct PayloadVisitor;

        impl<'de> Visitor<'de> for PayloadVisitor {
            type Value = BoundedPayload;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("an array of at most 4096 bytes")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let hinted = seq.size_hint().unwrap_or(0).min(MAX_RENDERER_PAYLOAD_LEN);
                let mut payload = Vec::with_capacity(hinted);
                while let Some(byte) = seq.next_element::<u8>()? {
                    if payload.len() == MAX_RENDERER_PAYLOAD_LEN {
                        return Err(A::Error::custom(PAYLOAD_TOO_LARGE_SENTINEL));
                    }
                    payload.push(byte);
                }
                Ok(BoundedPayload(payload))
            }
        }

        deserializer.deserialize_seq(PayloadVisitor)
    }
}

#[derive(Debug, Default)]
enum Field<T> {
    #[default]
    Missing,
    Present(T),
}

impl<'de, T> Deserialize<'de> for Field<T>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        T::deserialize(deserializer).map(Self::Present)
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawEnvelope {
    v: u64,
    kind: String,
    #[serde(default)]
    document: Field<String>,
    #[serde(default)]
    request: Field<u32>,
    #[serde(default)]
    channel: Field<u16>,
    #[serde(default)]
    payload: Field<BoundedPayload>,
}

/// Decode one private renderer envelope without performing any host/app effect.
pub fn decode_wv_link(input: &[u8]) -> Result<WvLinkEnvelope, WvLinkRejectReason> {
    if input.len() > MAX_WV_LINK_ENVELOPE_LEN {
        return Err(WvLinkRejectReason::MalformedEnvelope);
    }

    let parsed = serde_json::from_slice::<RawEnvelope>(input);
    let raw = match parsed {
        Ok(raw) => raw,
        Err(error) => {
            if error.to_string().contains(PAYLOAD_TOO_LARGE_SENTINEL) {
                return Err(WvLinkRejectReason::PayloadTooLarge);
            }
            return Err(WvLinkRejectReason::MalformedEnvelope);
        }
    };

    let RawEnvelope {
        v,
        kind,
        document,
        request,
        channel,
        payload,
    } = raw;

    if v != 1 {
        return Err(WvLinkRejectReason::UnsupportedVersion);
    }

    match kind.as_str() {
        "bind" => {
            if matches!(document, Field::Missing)
                && matches!(request, Field::Missing)
                && matches!(channel, Field::Missing)
                && matches!(payload, Field::Missing)
            {
                Ok(WvLinkEnvelope::Bind)
            } else {
                Err(WvLinkRejectReason::MalformedEnvelope)
            }
        }
        "invoke" => {
            let Field::Present(document) = document else {
                return Err(WvLinkRejectReason::MalformedEnvelope);
            };
            let Field::Present(request) = request else {
                return Err(WvLinkRejectReason::MalformedEnvelope);
            };
            let Field::Present(channel) = channel else {
                return Err(WvLinkRejectReason::MalformedEnvelope);
            };
            let Field::Present(payload) = payload else {
                return Err(WvLinkRejectReason::MalformedEnvelope);
            };

            let request = NonZeroU32::new(request).ok_or(WvLinkRejectReason::ZeroRequest)?;
            let channel = NonZeroU16::new(channel).ok_or(WvLinkRejectReason::ZeroChannel)?;
            Ok(WvLinkEnvelope::Invoke {
                document,
                request,
                channel,
                payload: payload.0,
            })
        }
        _ => Err(WvLinkRejectReason::MalformedEnvelope),
    }
}

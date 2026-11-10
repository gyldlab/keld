//! Pure decoder for the private isolated-world -> host `keld.wv-link/v1` envelope.
//!
//! This is deliberately not KIPC. It accepts only the bounded renderer control
//! shapes frozen by KEL-142 and carries no app-link endpoint, token, principal,
//! generation, grant, or other application authority.

use core::fmt;
use core::num::{NonZeroU16, NonZeroU32};

use serde::de::{DeserializeSeed, Error as _, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};

/// Maximum application bytes admitted by one renderer invoke.
pub const MAX_RENDERER_PAYLOAD_LEN: usize = 4096;

/// Maximum encoded private renderer envelope accepted by the pure decoder.
///
/// A valid maximum-size payload encoded as decimal JSON bytes is well below
/// this ceiling. The bound exists so hostile `document`/`kind` strings cannot
/// make decoder allocation scale without limit before host identity checks.
pub const MAX_WV_LINK_ENVELOPE_LEN: usize = 64 * 1024;

/// Maximum copied app-role local argument envelope before strict decoding.
/// Transport v2 still uses its narrower own envelope/control bounds.
pub const MAX_APP_WV_LINK_ENVELOPE_LEN: usize = 512 * 1024;

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
    /// The literal named topic exceeds 256 UTF-8 bytes.
    InvalidNamedChannel,
    /// The v2 world credential is not exactly 32 lowercase hex bytes.
    InvalidDocument,
    /// The encoded v2 control envelope exceeds 4096 bytes.
    ControlTooLarge,
}

/// Exact private control shapes admitted from the isolated world.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WvLinkEnvelope {
    /// Bind the current host-owned main-frame navigation.
    Bind,
    /// Dispatch one bounded named one-way message through the same endpoint.
    Send {
        /// Native-owned private world credential; never a page field.
        document: String,
        /// Nonzero local one-way message correlation.
        message: NonZeroU32,
        /// Exact literal topic; no normalization or wildcard expansion.
        channel: String,
        /// Sole GH544 endpoint codec bytes, opaque to Rust.
        payload: Vec<u8>,
    },
    /// Terminal renderer-consumer result for an addressed native event.
    Consumed {
        /// Native-owned private world credential.
        document: String,
        /// Original nonzero event delivery correlation.
        delivery: NonZeroU32,
        /// Exact terminal consumer disposition.
        disposition: WvConsumerDisposition,
    },
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

/// Endpoint construction selects a protocol; hostile input cannot downgrade it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WvLinkMode {
    /// Existing KEL142 numeric-only protocol.
    LegacyV1,
    /// Approved GH540 closed extension on the same endpoint/ledger.
    CompatV2,
}

/// Exact addressed event consumer terminal state, independent of transport release.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "status", deny_unknown_fields)]
pub enum WvConsumerDisposition {
    /// Original registered listener completed normally.
    #[serde(rename = "consumed")]
    Consumed,
    /// No current listener existed; do not buffer/replay this event.
    #[serde(rename = "no-listener")]
    NoListener,
    /// Consumer refused; sanitized registered diagnostic only.
    #[serde(rename = "refused")]
    Refused {
        /// Registered diagnostic code.
        code: String,
        /// Bounded sanitized detail.
        message: String,
    },
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum CompatChannel {
    Numeric(u16),
    Named(String),
}

// This is a parser-work/retention bound on the tagged JSON syntax, not a
// second endpoint value codec. The runtime owns semantic depth16/node256.
// Every legal local node adds fewer than8 JSON syntax nodes; every semantic
// depth adds at most3 JSON levels. Fixed tag/key spelling adds less than16KiB.
const MAX_LOCAL_SYNTAX_NODES: usize = 2_048;
const MAX_LOCAL_SYNTAX_DEPTH: usize = 64;
const MAX_LOCAL_SYNTAX_BYTES: usize = 65_536 + 16_384;
#[derive(Default)]
struct LocalSyntaxBudget {
    nodes: usize,
    bytes: usize,
}
impl LocalSyntaxBudget {
    fn node<E: serde::de::Error>(&mut self, depth: usize) -> Result<(), E> {
        if depth > MAX_LOCAL_SYNTAX_DEPTH || self.nodes >= MAX_LOCAL_SYNTAX_NODES {
            return Err(E::custom("local syntax node/depth bound"));
        }
        self.nodes += 1;
        Ok(())
    }
    fn text<E: serde::de::Error>(&mut self, value: &str) -> Result<(), E> {
        self.bytes = self
            .bytes
            .checked_add(value.len())
            .filter(|bytes| *bytes <= MAX_LOCAL_SYNTAX_BYTES)
            .ok_or_else(|| E::custom("local syntax text bound"))?;
        Ok(())
    }
}
struct LocalValueSeed<'a> {
    budget: &'a mut LocalSyntaxBudget,
    depth: usize,
}
impl<'de> DeserializeSeed<'de> for LocalValueSeed<'_> {
    type Value = serde_json::Value;
    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        self.budget.node::<D::Error>(self.depth)?;
        deserializer.deserialize_any(self)
    }
}
impl<'de> Visitor<'de> for LocalValueSeed<'_> {
    type Value = serde_json::Value;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("bounded local tagged JSON data")
    }
    fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
        Ok(serde_json::Value::Null)
    }
    fn visit_bool<E: serde::de::Error>(self, value: bool) -> Result<Self::Value, E> {
        Ok(serde_json::Value::Bool(value))
    }
    fn visit_i64<E: serde::de::Error>(self, value: i64) -> Result<Self::Value, E> {
        Ok(value.into())
    }
    fn visit_u64<E: serde::de::Error>(self, value: u64) -> Result<Self::Value, E> {
        Ok(value.into())
    }
    fn visit_f64<E: serde::de::Error>(self, value: f64) -> Result<Self::Value, E> {
        serde_json::Number::from_f64(value)
            .map(serde_json::Value::Number)
            .ok_or_else(|| E::custom("invalid local number"))
    }
    fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
        self.budget.text::<E>(value)?;
        Ok(serde_json::Value::String(value.to_owned()))
    }
    fn visit_string<E: serde::de::Error>(self, value: String) -> Result<Self::Value, E> {
        self.budget.text::<E>(&value)?;
        Ok(serde_json::Value::String(value))
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        let mut values = Vec::new();
        while let Some(value) = seq.next_element_seed(LocalValueSeed {
            budget: &mut *self.budget,
            depth: self.depth + 1,
        })? {
            values.push(value);
        }
        Ok(serde_json::Value::Array(values))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let mut values = serde_json::Map::new();
        while let Some(key) = map.next_key_seed(LocalValueSeed {
            budget: &mut *self.budget,
            depth: self.depth + 1,
        })? {
            let serde_json::Value::String(key) = key else {
                return Err(A::Error::custom("local map key is not text"));
            };
            if values.contains_key(&key) {
                return Err(A::Error::custom("duplicate local JSON field"));
            }
            let value = map.next_value_seed(LocalValueSeed {
                budget: &mut *self.budget,
                depth: self.depth + 1,
            })?;
            values.insert(key, value);
        }
        Ok(serde_json::Value::Object(values))
    }
}
#[derive(Debug)]
struct BoundedLocalArguments(Vec<serde_json::Value>);
impl<'de> Deserialize<'de> for BoundedLocalArguments {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Arguments;
        struct RejectSeventeenth;
        impl<'de> DeserializeSeed<'de> for RejectSeventeenth {
            type Value = ();
            fn deserialize<D: Deserializer<'de>>(self, _deserializer: D) -> Result<(), D::Error> {
                Err(D::Error::custom("more than16 local arguments"))
            }
        }
        impl<'de> Visitor<'de> for Arguments {
            type Value = BoundedLocalArguments;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("at most16 bounded local arguments")
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let mut budget = LocalSyntaxBudget::default();
                let mut values = Vec::new();
                while values.len() < 16 {
                    let Some(value) = seq.next_element_seed(LocalValueSeed {
                        budget: &mut budget,
                        depth: 1,
                    })?
                    else {
                        return Ok(BoundedLocalArguments(values));
                    };
                    values.push(value);
                }
                let _ = seq.next_element_seed(RejectSeventeenth)?;
                Ok(BoundedLocalArguments(values))
            }
        }
        deserializer.deserialize_seq(Arguments)
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCompatEnvelope {
    v: u64,
    kind: String,
    #[serde(default)]
    document: Field<String>,
    #[serde(default)]
    request: Field<u32>,
    #[serde(default)]
    message: Field<u32>,
    #[serde(default)]
    delivery: Field<u32>,
    #[serde(default)]
    channel: Field<CompatChannel>,
    #[serde(default)]
    payload: Field<BoundedPayload>,
    #[serde(default)]
    disposition: Field<WvConsumerDisposition>,
    #[serde(default)]
    call: Field<u32>,
    #[serde(default)]
    method: Field<String>,
    #[serde(default)]
    args: Field<BoundedLocalArguments>,
    #[serde(default)]
    methods: Field<Vec<String>>,
}

fn valid_document(value: &str) -> bool {
    value.len() == 32
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// Decode the construction-selected private protocol using this one parser owner.
/// The legacy entry point below remains exact v1 and never auto-negotiates input.
pub fn decode_wv_link_for_mode(
    input: &[u8],
    mode: WvLinkMode,
) -> Result<WvLinkEnvelope, WvLinkRejectReason> {
    if mode == WvLinkMode::LegacyV1 {
        return decode_wv_link(input);
    }
    if input.len() > MAX_WV_LINK_ENVELOPE_LEN {
        return Err(WvLinkRejectReason::MalformedEnvelope);
    }
    let raw: RawCompatEnvelope = serde_json::from_slice(input).map_err(|error| {
        if error.to_string().contains(PAYLOAD_TOO_LARGE_SENTINEL) {
            WvLinkRejectReason::PayloadTooLarge
        } else {
            WvLinkRejectReason::MalformedEnvelope
        }
    })?;
    decode_raw_compat(raw, input.len())
}
fn local_fields_absent(raw: &RawCompatEnvelope) -> bool {
    matches!(
        (&raw.call, &raw.method, &raw.args, &raw.methods),
        (
            Field::Missing,
            Field::Missing,
            Field::Missing,
            Field::Missing
        )
    )
}
fn decode_raw_compat(
    raw: RawCompatEnvelope,
    input_len: usize,
) -> Result<WvLinkEnvelope, WvLinkRejectReason> {
    if input_len > MAX_WV_LINK_ENVELOPE_LEN || !local_fields_absent(&raw) {
        return Err(WvLinkRejectReason::MalformedEnvelope);
    }
    if raw.v != 2 {
        return Err(WvLinkRejectReason::UnsupportedVersion);
    }
    match raw.kind.as_str() {
        "bind" => decode_compat_bind(&raw, input_len),
        "invoke"
            if matches!(
                (&raw.message, &raw.delivery, &raw.disposition),
                (Field::Missing, Field::Missing, Field::Missing)
            ) =>
        {
            let (
                Field::Present(document),
                Field::Present(request),
                Field::Present(CompatChannel::Numeric(channel)),
                Field::Present(payload),
            ) = (raw.document, raw.request, raw.channel, raw.payload)
            else {
                return Err(WvLinkRejectReason::MalformedEnvelope);
            };
            if !valid_document(&document) {
                return Err(WvLinkRejectReason::InvalidDocument);
            }
            Ok(WvLinkEnvelope::Invoke {
                document,
                request: NonZeroU32::new(request).ok_or(WvLinkRejectReason::ZeroRequest)?,
                channel: NonZeroU16::new(channel).ok_or(WvLinkRejectReason::ZeroChannel)?,
                payload: payload.0,
            })
        }
        "send"
            if matches!(
                (&raw.request, &raw.delivery, &raw.disposition),
                (Field::Missing, Field::Missing, Field::Missing)
            ) =>
        {
            let (
                Field::Present(document),
                Field::Present(message),
                Field::Present(CompatChannel::Named(channel)),
                Field::Present(payload),
            ) = (raw.document, raw.message, raw.channel, raw.payload)
            else {
                return Err(WvLinkRejectReason::MalformedEnvelope);
            };
            if !valid_document(&document) {
                return Err(WvLinkRejectReason::InvalidDocument);
            }
            if channel.len() > 256 {
                return Err(WvLinkRejectReason::InvalidNamedChannel);
            }
            Ok(WvLinkEnvelope::Send {
                document,
                message: NonZeroU32::new(message).ok_or(WvLinkRejectReason::ZeroRequest)?,
                channel,
                payload: payload.0,
            })
        }
        "consumed"
            if matches!(
                (&raw.request, &raw.message, &raw.channel, &raw.payload),
                (
                    Field::Missing,
                    Field::Missing,
                    Field::Missing,
                    Field::Missing
                )
            ) =>
        {
            decode_consumer(raw.document, raw.delivery, raw.disposition, input_len)
        }
        _ => Err(WvLinkRejectReason::MalformedEnvelope),
    }
}

fn decode_compat_bind(
    raw: &RawCompatEnvelope,
    input_len: usize,
) -> Result<WvLinkEnvelope, WvLinkRejectReason> {
    if !matches!(
        (
            &raw.document,
            &raw.request,
            &raw.message,
            &raw.delivery,
            &raw.channel,
            &raw.payload,
            &raw.disposition
        ),
        (
            Field::Missing,
            Field::Missing,
            Field::Missing,
            Field::Missing,
            Field::Missing,
            Field::Missing,
            Field::Missing
        )
    ) {
        return Err(WvLinkRejectReason::MalformedEnvelope);
    }
    if input_len > 4_096 {
        return Err(WvLinkRejectReason::ControlTooLarge);
    }
    Ok(WvLinkEnvelope::Bind)
}

fn decode_consumer(
    document: Field<String>,
    delivery: Field<u32>,
    disposition: Field<WvConsumerDisposition>,
    input_len: usize,
) -> Result<WvLinkEnvelope, WvLinkRejectReason> {
    if input_len > 4_096 {
        return Err(WvLinkRejectReason::ControlTooLarge);
    }
    let (Field::Present(document), Field::Present(delivery), Field::Present(disposition)) =
        (document, delivery, disposition)
    else {
        return Err(WvLinkRejectReason::MalformedEnvelope);
    };
    if !valid_document(&document) {
        return Err(WvLinkRejectReason::InvalidDocument);
    }
    Ok(WvLinkEnvelope::Consumed {
        document,
        delivery: NonZeroU32::new(delivery).ok_or(WvLinkRejectReason::ZeroRequest)?,
        disposition,
    })
}

#[cfg(test)]
mod compat_v2_tests {
    use super::*;
    const DOC: &str = "0123456789abcdef0123456789abcdef";
    fn decode(value: &str) -> Result<WvLinkEnvelope, WvLinkRejectReason> {
        decode_wv_link_for_mode(value.as_bytes(), WvLinkMode::CompatV2)
    }
    #[test]
    fn protocol_mode_never_negotiates_or_downgrades() {
        assert_eq!(
            decode(r#"{"v":1,"kind":"bind"}"#),
            Err(WvLinkRejectReason::UnsupportedVersion)
        );
        assert_eq!(
            decode_wv_link(br#"{"v":2,"kind":"bind"}"#),
            Err(WvLinkRejectReason::UnsupportedVersion)
        );
        assert_eq!(decode(r#"{"v":2,"kind":"bind"}"#), Ok(WvLinkEnvelope::Bind));
    }
    #[test]
    fn named_send_and_consumer_have_closed_distinct_shapes() {
        let send = format!(
            r#"{{"v":2,"kind":"send","document":"{DOC}","message":1,"channel":"el:rendererReq","payload":[0,255]}}"#
        );
        assert!(matches!(decode(&send),Ok(WvLinkEnvelope::Send {message,..}) if message.get()==1));
        for extra in [
            r#", "request":1"#,
            r#", "message":2"#,
            r#", "principal":"app""#,
            r#", "disposition":null"#,
        ] {
            let invalid = format!("{}{}{}", &send[..send.len() - 1], extra, "}");
            assert!(decode(&invalid).is_err(), "{invalid}");
        }
        let consumed = format!(
            r#"{{"v":2,"kind":"consumed","document":"{DOC}","delivery":1,"disposition":{{"status":"consumed"}}}}"#
        );
        assert!(matches!(
            decode(&consumed),
            Ok(WvLinkEnvelope::Consumed { .. })
        ));
        assert!(decode(&consumed.replace("consumed\"}", "stale\"}")).is_err());
        assert!(decode(&send.replace(DOC, "")).is_err());
        assert!(decode(&send.replace("\"message\":1", "\"message\":0")).is_err());
    }
    #[test]
    fn payload_and_literal_utf8_channel_boundaries_are_independent() {
        for (size, allowed) in [(4096, true), (4097, false)] {
            let payload = vec![255_u8; size];
            let value = format!(
                r#"{{"v":2,"kind":"send","document":"{DOC}","message":1,"channel":"el:x","payload":{}}}"#,
                serde_json::to_string(&payload).expect("JSON")
            );
            assert_eq!(decode(&value).is_ok(), allowed);
        }
        for (count, allowed) in [(128, true), (129, false)] {
            let topic = "é".repeat(count);
            let value = format!(
                r#"{{"v":2,"kind":"send","document":"{DOC}","message":1,"channel":"{topic}","payload":[]}}"#
            );
            assert_eq!(decode(&value).is_ok(), allowed);
        }
    }
}

/// Canonical private one-way terminal result; no page Promise or synthetic reply.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "status")]
pub enum WvTerminalDisposition {
    /// Receiving endpoint completed its current handler.
    #[serde(rename = "consumed")]
    Consumed,
    /// No listener existed at the receiving endpoint.
    #[serde(rename = "no-listener")]
    NoListener,
    /// Effect was refused with a registered sanitized error.
    #[serde(rename = "refused")]
    Refused {
        /// Stable code.
        code: String,
        /// Sanitized message.
        message: String,
    },
    /// Original document no longer exists.
    #[serde(rename = "stale")]
    Stale,
    /// Original authenticated route ended.
    #[serde(rename = "route-retired")]
    RouteRetired {
        /// Effect certainty from the actual owner.
        certainty: WvDispatchCertainty,
    },
}
/// Explicit certainty from the actual send/write boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum WvDispatchCertainty {
    /// No application effect was dispatched.
    #[serde(rename = "not-dispatched")]
    NotDispatched,
    /// Some bytes/effect may already have reached the original route.
    #[serde(rename = "outcome-unknown")]
    OutcomeUnknown,
}

/// Closed local data record after the isolated app relay appended its private credential.
/// This is not an IPC value codec; Functions remain in the document runtime only.
#[derive(Debug, Serialize)]
pub struct AppPageCall {
    /// Current app-world private credential, never present in DOM/page data.
    pub document: String,
    /// Untrusted document-local correlation only.
    pub call: u32,
    /// One of the four actual original private Function names.
    pub method: String,
    /// Bounded owned local graph descriptors; runtime validates mutable copies.
    pub args: Vec<serde_json::Value>,
}
/// One selected app-world native parsing result. Transport uses the same raw owner.
#[derive(Debug)]
pub enum AppWvLinkEnvelope {
    /// The unchanged closed GH540 private v2 schema.
    Transport(WvLinkEnvelope),
    /// Same-Document untrusted data carried by current credentialed app runtime.
    Call(AppPageCall),
    /// Actual private original preload registrations have completed.
    Registered {
        /// Current world credential.
        document: String,
        /// Actual exact four-function registry.
        methods: Vec<String>,
    },
    /// Actual local cleanup completed; transport credit is independently owned.
    ReleaseCall {
        /// Current world credential.
        document: String,
        /// Original local correlation.
        call: u32,
    },
    /// Original Function invocation turn ended, possibly with retained bookkeeping.
    CallFinished {
        /// Current world credential.
        document: String,
        /// Original local correlation.
        call: u32,
    },
}
/// Sole app-role parser: native role selection admits local control plus exact v2.
/// A direct page or trusted-world input cannot select this app-role entry point.
pub fn decode_app_wv_link(input: &[u8]) -> Result<AppWvLinkEnvelope, WvLinkRejectReason> {
    if input.len() > MAX_APP_WV_LINK_ENVELOPE_LEN {
        return Err(WvLinkRejectReason::MalformedEnvelope);
    }
    let raw: RawCompatEnvelope = serde_json::from_slice(input).map_err(|error| {
        if error.to_string().contains(PAYLOAD_TOO_LARGE_SENTINEL) {
            WvLinkRejectReason::PayloadTooLarge
        } else {
            WvLinkRejectReason::MalformedEnvelope
        }
    })?;
    if raw.v == 2 {
        return decode_raw_compat(raw, input.len()).map(AppWvLinkEnvelope::Transport);
    }
    if raw.v != 1 {
        return Err(WvLinkRejectReason::UnsupportedVersion);
    }
    if !matches!(
        (
            &raw.request,
            &raw.message,
            &raw.delivery,
            &raw.channel,
            &raw.payload,
            &raw.disposition
        ),
        (
            Field::Missing,
            Field::Missing,
            Field::Missing,
            Field::Missing,
            Field::Missing,
            Field::Missing
        )
    ) {
        return Err(WvLinkRejectReason::MalformedEnvelope);
    }
    let Field::Present(document) = raw.document else {
        return Err(WvLinkRejectReason::MalformedEnvelope);
    };
    if !valid_document(&document) {
        return Err(WvLinkRejectReason::InvalidDocument);
    }
    match raw.kind.as_str() {
        "call" => decode_app_call(document, raw.call, raw.method, raw.args, &raw.methods),
        "registered" => {
            if input.len() > 4096 {
                return Err(WvLinkRejectReason::ControlTooLarge);
            }
            if !matches!(
                (&raw.call, &raw.method, &raw.args),
                (Field::Missing, Field::Missing, Field::Missing)
            ) {
                return Err(WvLinkRejectReason::MalformedEnvelope);
            }
            let Field::Present(methods) = raw.methods else {
                return Err(WvLinkRejectReason::MalformedEnvelope);
            };
            if methods.as_slice()
                != [
                    "request",
                    "registerMsgListener",
                    "sendMessage",
                    "listenOnce",
                ]
            {
                return Err(WvLinkRejectReason::MalformedEnvelope);
            }
            Ok(AppWvLinkEnvelope::Registered { document, methods })
        }
        "release-call" | "call-finished" => {
            if input.len() > 4096 {
                return Err(WvLinkRejectReason::ControlTooLarge);
            }
            if !matches!(
                (&raw.methods, &raw.method, &raw.args),
                (Field::Missing, Field::Missing, Field::Missing)
            ) {
                return Err(WvLinkRejectReason::MalformedEnvelope);
            }
            let Field::Present(call) = raw.call else {
                return Err(WvLinkRejectReason::MalformedEnvelope);
            };
            if call == 0 {
                return Err(WvLinkRejectReason::ZeroRequest);
            }
            if raw.kind == "release-call" {
                Ok(AppWvLinkEnvelope::ReleaseCall { document, call })
            } else {
                Ok(AppWvLinkEnvelope::CallFinished { document, call })
            }
        }
        _ => Err(WvLinkRejectReason::MalformedEnvelope),
    }
}
fn decode_app_call(
    document: String,
    call: Field<u32>,
    method: Field<String>,
    args: Field<BoundedLocalArguments>,
    methods: &Field<Vec<String>>,
) -> Result<AppWvLinkEnvelope, WvLinkRejectReason> {
    if !matches!(methods, Field::Missing) {
        return Err(WvLinkRejectReason::MalformedEnvelope);
    }
    let (Field::Present(call), Field::Present(method), Field::Present(args)) = (call, method, args)
    else {
        return Err(WvLinkRejectReason::MalformedEnvelope);
    };
    if call == 0
        || args.0.len() > 16
        || !matches!(
            method.as_str(),
            "request" | "registerMsgListener" | "sendMessage" | "listenOnce"
        )
    {
        return Err(WvLinkRejectReason::MalformedEnvelope);
    }
    Ok(AppWvLinkEnvelope::Call(AppPageCall {
        document,
        call,
        method,
        args: args.0,
    }))
}

#[cfg(test)]
mod app_local_decode_tests {
    fn local_call(args: &serde_json::Value) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({"v":1,"kind":"call","document":"0123456789abcdef0123456789abcdef","call":1,"method":"registerMsgListener","args":args})).expect("fixture JSON")
    }
    /// Native WK bridge must preserve the exact JS binary64 timestamp in one
    /// local argument, not a nearby decimal approximation. Otherwise the
    /// original draw.io Save checks a false external-file conflict even though
    /// the real file's mtimeMs has not changed.
    #[test]
    fn local_app_call_preserves_binary64_mtime_exactly() {
        let literal = "1793581255029.1807";
        let expected = literal.parse::<f64>().expect("valid JS number literal");
        let json = format!(
            "{{\"v\":1,\"kind\":\"call\",\"document\":\"0123456789abcdef0123456789abcdef\",\"call\":1,\"method\":\"request\",\"args\":[{{\"k\":\"object\",\"entries\":[[\"mtimeMs\",{literal}]]}}]}}"
        );
        let AppWvLinkEnvelope::Call(call) =
            decode_app_wv_link(json.as_bytes()).expect("one bounded current-document call")
        else {
            panic!("local call envelope expected");
        };
        let parsed = call.args[0]["entries"][0][1]
            .as_f64()
            .expect("numeric timestamp");
        assert_eq!(
            parsed.to_bits(),
            expected.to_bits(),
            "the native JSON boundary may not change one binary64 bit"
        );
        let reencoded = serde_json::to_string(&call).expect("native call serialization");
        assert!(
            reencoded.contains(literal),
            "the same exact f64 must be revived by the isolated app runtime"
        );
    }
    #[test]
    fn local_json_syntax_budget_refuses_expansion_before_owned_graph_retention() {
        let huge = local_call(&serde_json::json!([vec![0; MAX_LOCAL_SYNTAX_NODES + 1]]));
        assert!(huge.len() < MAX_APP_WV_LINK_ENVELOPE_LEN);
        assert!(decode_app_wv_link(&huge).is_err());
        let string = local_call(&serde_json::json!(["x".repeat(MAX_LOCAL_SYNTAX_BYTES + 1)]));
        assert!(decode_app_wv_link(&string).is_err());
        assert!(decode_app_wv_link(&local_call(&serde_json::json!(vec![0; 17]))).is_err());
    }
    #[test]
    fn legal_maximum_semantic_node_shape_fits_independent_syntax_budget() {
        let callbacks: Vec<_> = (0..255)
            .map(|_| serde_json::json!({"k":"callback","id":1}))
            .collect();
        assert!(
            decode_app_wv_link(&local_call(
                &serde_json::json!([{"k":"array","values":callbacks}])
            ))
            .is_ok()
        );
        let mut value = serde_json::json!(0);
        for _ in 0..15 {
            value = serde_json::json!({"k":"object","entries":[["a",value]]});
        }
        assert!(decode_app_wv_link(&local_call(&serde_json::json!([value]))).is_ok());
    }

    use super::*;
    #[test]
    fn local_data_and_transport_fields_never_cross_roles_or_kinds() {
        let call=br#"{"v":1,"kind":"call","document":"0123456789abcdef0123456789abcdef","call":1,"method":"registerMsgListener","args":["local",{"k":"callback","id":1}]}"#;
        assert!(matches!(
            decode_app_wv_link(call),
            Ok(AppWvLinkEnvelope::Call(_))
        ));
        assert!(decode_wv_link_for_mode(call, WvLinkMode::CompatV2).is_err());
        for field in [
            r#", "channel":"rendererReq""#,
            r#", "call":2"#,
            r#", "methods":null"#,
            r#", "principal":"app""#,
        ] {
            let text = std::str::from_utf8(call).expect("JSON");
            let invalid = format!("{}{}{}", &text[..text.len() - 1], field, "}");
            assert!(decode_app_wv_link(invalid.as_bytes()).is_err());
        }
    }
}

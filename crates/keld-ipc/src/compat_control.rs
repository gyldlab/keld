//! GH540 one family-neutral compatibility control channel.
//!
//! These closed postcard DTOs contain selectors and opaque endpoint value bytes.
//! Authenticated role/window/document provenance and guard decisions live in
//! their existing owners. Rust never interprets Electron application values.
//! The Host selects the receive policy and checks the **frame header length**
//! against `limit_for_kind` before a body read on an admitted compatibility receiver.
//! A typed decoder validates all inner field bounds after that bounded read.

use serde::{Deserialize, Deserializer, Serialize, de::Error as _};

use crate::CallError;
use crate::IpcError;
use crate::call_error::CallErrorFields;
use crate::codec::{decode, encode};
use crate::frame::FrameKind;

/// Host/app-link CALL and EVENT encoded payload budget.
pub const COMPAT_DATA_FRAME_BYTES: usize = 16_384;
/// REPLY/ERR and release-control encoded payload budget.
pub const COMPAT_CONTROL_FRAME_BYTES: usize = 4_096;
/// One host-opaque endpoint value.
pub const COMPAT_VALUE_BYTES: usize = 4_096;
/// Exact UTF-8 topic name ceiling; names are not capability grants.
pub const COMPAT_TOPIC_BYTES: usize = 256;
/// Maximum UTF-8 size of the Host-stamped committed URL.
pub const COMPAT_URL_BYTES: usize = 4_096;
/// Maximum UTF-8 size of the Host-stamped origin.
pub const COMPAT_ORIGIN_BYTES: usize = 1_024;
/// Maximum ASCII size of a consumer diagnostic code.
pub const COMPAT_ERROR_CODE_BYTES: usize = 64;
/// Maximum UTF-8 size of a consumer diagnostic message.
pub const COMPAT_ERROR_MESSAGE_BYTES: usize = 512;
/// Maximum received encoded size of a document retirement event.
pub const COMPAT_DOCUMENT_RETIRED_FRAME_BYTES: usize = 11;

/// A compatibility delivery or release requested by the authenticated Bun role.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompatControlRequest {
    /// Bun main → current admitted native main document at admission.
    DeliverToCurrentMain {
        /// Host-registered `WebView` selector.
        webview: u32,
        /// Literal topic, checked separately against exact Host grant.
        #[serde(deserialize_with = "deserialize_compat_topic")]
        topic: String,
        /// Encoded endpoint value, opaque to Host.
        #[serde(deserialize_with = "deserialize_compat_payload")]
        payload: Vec<u8>,
    },
    /// Main's addressed reply → original committed document only.
    DeliverToOriginalDocument {
        /// Host-registered `WebView` selector.
        webview: u32,
        /// Native-owned checked navigation identity, never independently minted.
        navigation: u32,
        /// Exact literal topic.
        #[serde(deserialize_with = "deserialize_compat_topic")]
        topic: String,
        /// Encoded endpoint value, opaque to Host.
        #[serde(deserialize_with = "deserialize_compat_payload")]
        payload: Vec<u8>,
    },
    /// Application consumer's terminal report (quantity-free, one ledger key).
    ReleaseConsumed {
        /// Host-registered `WebView` selector.
        webview: u32,
        /// Native-owned original committed navigation.
        navigation: u32,
        /// Native-minted message sequence.
        message: u32,
        /// Consumer result. An app cannot establish native quiescence.
        disposition: ConsumerDisposition,
    },
}

/// Authenticated Host → Bun event on the same allocated channel.
/// Generic fields permit borrowed encoding without another serializer or tag table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompatControlEventValue<S, P> {
    /// Renderer DATA, stamped by Host after native document authentication.
    RendererMessage {
        /// Host-registered `WebView` identity.
        webview: u32,
        /// Native-owned committed navigation.
        navigation: u32,
        /// Native-minted sequence, unique in this document/direction.
        message: u32,
        /// Host-stamped committed URL.
        url: S,
        /// Host-stamped origin serialization, including opaque `null`.
        origin: S,
        /// Literal application topic.
        topic: S,
        /// Opaque endpoint bytes.
        payload: P,
    },
    /// Native document retirement: no new admitted work for this document.
    DocumentRetired {
        /// Host-registered `WebView` identity.
        webview: u32,
        /// Native-owned committed navigation.
        navigation: u32,
    },
}

/// Owned decode/consumer form of the one canonical Host event schema.
pub type CompatControlEvent = CompatControlEventValue<String, Vec<u8>>;

/// One first-terminal-wins response per admitted CALL correlation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompatControlResponse {
    /// A delivery's terminal result, after the owner's native/ring settlement.
    Deliver {
        /// Destination or Host-owned retirement disposition.
        disposition: TerminalDisposition,
    },
    /// An authenticated release-control request was handled; no credit implied.
    Released,
}

/// Consumer-provided state. It cannot claim a stale or retired native route.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConsumerDisposition {
    /// Consumer finished synchronous listener dispatch.
    Consumed,
    /// Consumer had no listener for this admitted topic.
    NoListener,
    /// Consumer could not admit the endpoint value.
    Refused {
        /// Existing canonical encoded `CallError`, attributed to consumer.
        #[serde(deserialize_with = "deserialize_compat_diagnostic")]
        error: CallError,
    },
}

/// The Host may add only its own route states to a consumer result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TerminalDisposition {
    /// Consumer consumed the delivery.
    Consumed,
    /// No listener existed at dispatch.
    NoListener,
    /// Original native document no longer matches.
    Stale,
    /// Consumer refused the value.
    Refused {
        /// Bounded diagnostic attributed to consumer, not Host certainty.
        #[serde(deserialize_with = "deserialize_compat_diagnostic")]
        error: CallError,
    },
    /// Host-owned proof of retirement; ambiguous effects remain charged.
    RouteRetired {
        /// One of the two Host-observed dispatch certainties.
        certainty: DispatchCertainty,
    },
}

/// Host-owned certainty; a consumer cannot send one as `ConsumerDisposition`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DispatchCertainty {
    /// Host proved no initial write/dispatch.
    NotDispatched,
    /// Host cannot prove the operation did not run.
    OutcomeUnknown,
}

/// The declared per-kind limit, used in the canonical reader before body read.
///
/// A generic `ERR` keeps the one `CallError` codec. KIPC `FrameKind` and v2
/// HELLO remain unchanged. Only `CALL` and `EVENT` carry DATA envelopes.
#[must_use]
pub const fn limit_for_kind(kind: FrameKind) -> Option<usize> {
    match kind {
        FrameKind::Call | FrameKind::Event => Some(COMPAT_DATA_FRAME_BYTES),
        FrameKind::Reply | FrameKind::Err => Some(COMPAT_CONTROL_FRAME_BYTES),
        _ => None,
    }
}

fn invalid() -> IpcError {
    IpcError::Codec(postcard::Error::DeserializeBadEncoding)
}

fn valid_identity(value: u32) -> bool {
    value != 0
}

fn valid_topic(value: &str) -> bool {
    value.len() <= COMPAT_TOPIC_BYTES
}

fn valid_payload(payload: &[u8]) -> bool {
    payload.len() <= COMPAT_VALUE_BYTES
}

fn valid_diagnostic<S: AsRef<str>>(error: &CallErrorFields<S>) -> bool {
    let code = error.code.as_ref().as_bytes();
    code.len() > b"KELD-".len()
        && code.len() <= COMPAT_ERROR_CODE_BYTES
        && code.starts_with(b"KELD-")
        && code
            .iter()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || *byte == b'-')
        && error.message.as_ref().len() <= COMPAT_ERROR_MESSAGE_BYTES
}

fn deserialize_compat_topic<'de, D: Deserializer<'de>>(decoder: D) -> Result<String, D::Error> {
    let topic = <&str>::deserialize(decoder)?;
    if !valid_topic(topic) {
        return Err(D::Error::custom("compatibility topic exceeds its bound"));
    }
    Ok(topic.to_owned())
}

fn deserialize_compat_payload<'de, D: Deserializer<'de>>(decoder: D) -> Result<Vec<u8>, D::Error> {
    let payload = <&[u8]>::deserialize(decoder)?;
    if !valid_payload(payload) {
        return Err(D::Error::custom(
            "compatibility endpoint value exceeds its bound",
        ));
    }
    Ok(payload.to_vec())
}

fn deserialize_compat_diagnostic<'de, D: Deserializer<'de>>(
    decoder: D,
) -> Result<CallError, D::Error> {
    let error = CallErrorFields::<&str>::deserialize(decoder)?;
    if !valid_diagnostic(&error) {
        return Err(D::Error::custom(
            "compatibility diagnostic exceeds its domain",
        ));
    }
    Ok(CallError {
        code: error.code.to_owned(),
        message: error.message.to_owned(),
    })
}

fn valid_consumer(disposition: &ConsumerDisposition) -> bool {
    match disposition {
        ConsumerDisposition::Consumed | ConsumerDisposition::NoListener => true,
        ConsumerDisposition::Refused { error } => valid_diagnostic(error),
    }
}

fn valid_terminal(disposition: &TerminalDisposition) -> bool {
    match disposition {
        TerminalDisposition::Refused { error } => valid_diagnostic(error),
        TerminalDisposition::Consumed
        | TerminalDisposition::NoListener
        | TerminalDisposition::Stale
        | TerminalDisposition::RouteRetired { .. } => true,
    }
}

/// Validates operation-specific structure after the bounded canonical decode.
///
/// # Errors
///
/// Returns the existing `KELD-IPC-003` codec failure for out-of-domain fields.
pub fn validate_request(request: &CompatControlRequest) -> Result<(), IpcError> {
    let good = match request {
        CompatControlRequest::DeliverToCurrentMain {
            webview,
            topic,
            payload,
        } => valid_identity(*webview) && valid_topic(topic) && valid_payload(payload),
        CompatControlRequest::DeliverToOriginalDocument {
            webview,
            navigation,
            topic,
            payload,
        } => {
            valid_identity(*webview)
                && valid_identity(*navigation)
                && valid_topic(topic)
                && valid_payload(payload)
        }
        CompatControlRequest::ReleaseConsumed {
            webview,
            navigation,
            message,
            disposition,
        } => {
            valid_identity(*webview)
                && valid_identity(*navigation)
                && valid_identity(*message)
                && valid_consumer(disposition)
        }
    };
    if good { Ok(()) } else { Err(invalid()) }
}

/// Validates Host-stamped event field bounds after canonical decode.
///
/// # Errors
///
/// Returns the canonical codec error for a bad selector or oversized field.
pub fn validate_event<S: AsRef<str>, P: AsRef<[u8]>>(
    event: &CompatControlEventValue<S, P>,
) -> Result<(), IpcError> {
    let good = match event {
        CompatControlEventValue::RendererMessage {
            webview,
            navigation,
            message,
            url,
            origin,
            topic,
            payload,
        } => {
            valid_identity(*webview)
                && valid_identity(*navigation)
                && valid_identity(*message)
                && url.as_ref().len() <= COMPAT_URL_BYTES
                && origin.as_ref().len() <= COMPAT_ORIGIN_BYTES
                && valid_topic(topic.as_ref())
                && valid_payload(payload.as_ref())
        }
        CompatControlEventValue::DocumentRetired {
            webview,
            navigation,
        } => valid_identity(*webview) && valid_identity(*navigation),
    };
    if good { Ok(()) } else { Err(invalid()) }
}

/// Validates a complete terminal disposition under the small response budget.
///
/// # Errors
///
/// Returns the canonical codec error on an invalid consumer diagnostic.
pub fn validate_response(response: &CompatControlResponse) -> Result<(), IpcError> {
    match response {
        CompatControlResponse::Deliver { disposition } if !valid_terminal(disposition) => {
            Err(invalid())
        }
        _ => Ok(()),
    }
}

/// Decode exactly one bounded request through the shared postcard decoder.
///
/// # Errors
///
/// Refuses oversize, malformed, trailing, unknown-variant or out-of-domain data.
pub fn decode_request(bytes: &[u8]) -> Result<CompatControlRequest, IpcError> {
    if bytes.len() > COMPAT_DATA_FRAME_BYTES {
        return Err(IpcError::PayloadTooLarge);
    }
    let value: CompatControlRequest = decode(bytes)?;
    if matches!(value, CompatControlRequest::ReleaseConsumed { .. })
        && bytes.len() > COMPAT_CONTROL_FRAME_BYTES
    {
        return Err(IpcError::PayloadTooLarge);
    }
    validate_request(&value)?;
    Ok(value)
}

/// Canonically encode one validated role request within its per-kind budget.
///
/// # Errors
///
/// Refuses out-of-domain and oversized request envelopes before writer handoff.
pub fn encode_request(request: &CompatControlRequest) -> Result<Vec<u8>, IpcError> {
    validate_request(request)?;
    let encoded = encode(request)?;
    if encoded.len() > COMPAT_DATA_FRAME_BYTES
        || (matches!(request, CompatControlRequest::ReleaseConsumed { .. })
            && encoded.len() > COMPAT_CONTROL_FRAME_BYTES)
    {
        return Err(IpcError::PayloadTooLarge);
    }
    Ok(encoded)
}

/// Decode an authenticated Host event with exact field and frame limits.
///
/// # Errors
///
/// Refuses malformed or out-of-domain event data.
pub fn decode_event(bytes: &[u8]) -> Result<CompatControlEvent, IpcError> {
    if bytes.len() > COMPAT_DATA_FRAME_BYTES {
        return Err(IpcError::PayloadTooLarge);
    }
    let value: CompatControlEventValue<&str, &[u8]> = decode(bytes)?;
    if matches!(value, CompatControlEventValue::DocumentRetired { .. })
        && bytes.len() > COMPAT_DOCUMENT_RETIRED_FRAME_BYTES
    {
        return Err(IpcError::PayloadTooLarge);
    }
    validate_event(&value)?;
    Ok(match value {
        CompatControlEventValue::RendererMessage {
            webview,
            navigation,
            message,
            url,
            origin,
            topic,
            payload,
        } => CompatControlEvent::RendererMessage {
            webview,
            navigation,
            message,
            url: url.to_owned(),
            origin: origin.to_owned(),
            topic: topic.to_owned(),
            payload: payload.to_vec(),
        },
        CompatControlEventValue::DocumentRetired {
            webview,
            navigation,
        } => CompatControlEvent::DocumentRetired {
            webview,
            navigation,
        },
    })
}

/// Canonically encode Host-authenticated event fields inside their frame budget.
///
/// # Errors
///
/// Refuses out-of-domain and oversized events before writer handoff.
pub fn encode_event(event: &CompatControlEvent) -> Result<Vec<u8>, IpcError> {
    validate_event(event)?;
    let encoded = encode(event)?;
    if encoded.len() > COMPAT_DATA_FRAME_BYTES {
        return Err(IpcError::PayloadTooLarge);
    }
    Ok(encoded)
}

/// Decode a response from the existing canonical frame and codec.
///
/// # Errors
///
/// Refuses malformed, trailing or oversized response bytes.
pub fn decode_response(bytes: &[u8]) -> Result<CompatControlResponse, IpcError> {
    if bytes.len() > COMPAT_CONTROL_FRAME_BYTES {
        return Err(IpcError::PayloadTooLarge);
    }
    let value: CompatControlResponse = decode(bytes)?;
    validate_response(&value)?;
    Ok(value)
}

/// Canonically encode one validated terminal response under the control cap.
///
/// # Errors
///
/// Refuses invalid consumer diagnostic or oversize envelope.
pub fn encode_response(response: &CompatControlResponse) -> Result<Vec<u8>, IpcError> {
    validate_response(response)?;
    let encoded = encode(response)?;
    if encoded.len() > COMPAT_CONTROL_FRAME_BYTES {
        return Err(IpcError::PayloadTooLarge);
    }
    Ok(encoded)
}

#[cfg(test)]
mod tests;

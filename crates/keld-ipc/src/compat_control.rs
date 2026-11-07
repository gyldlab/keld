//! GH540 one family-neutral compatibility control channel.
//!
//! These closed postcard DTOs contain selectors and opaque endpoint value bytes.
//! Authenticated role/window/document provenance and guard decisions live in
//! their existing owners. Rust never interprets Electron application values.
//! The Host selects the receive policy and checks the **frame header length**
//! against `limit_for_kind` before reading the body from its single reader.
//! A typed decoder validates all inner field bounds after that bounded read.

use serde::{Deserialize, Serialize};

use crate::CallError;
use crate::IpcError;
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

/// A compatibility delivery or release requested by the authenticated Bun role.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompatControlRequest {
    /// Bun main → current admitted native main document at admission.
    DeliverToCurrentMain {
        /// Host-registered `WebView` selector.
        webview: u32,
        /// Literal topic, checked separately against exact Host grant.
        topic: String,
        /// Encoded endpoint value, opaque to Host.
        payload: Vec<u8>,
    },
    /// Main's addressed reply → original committed document only.
    DeliverToOriginalDocument {
        /// Host-registered `WebView` selector.
        webview: u32,
        /// Native-owned checked navigation identity, never independently minted.
        navigation: u32,
        /// Exact literal topic.
        topic: String,
        /// Encoded endpoint value, opaque to Host.
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompatControlEvent {
    /// Renderer DATA, stamped by Host after native document authentication.
    RendererMessage {
        /// Host-registered `WebView` identity.
        webview: u32,
        /// Native-owned committed navigation.
        navigation: u32,
        /// Native-minted sequence, unique in this document/direction.
        message: u32,
        /// Exact existing Core WindowRef.id, selected by Host. Not `WebviewId`.
        window_id: u32,
        /// Exact existing Core WindowRef.incarnation, selected by Host.
        window_incarnation: u64,
        /// Host-stamped committed URL.
        url: String,
        /// Host-stamped origin serialization, including opaque `null`.
        origin: String,
        /// Literal application topic.
        topic: String,
        /// Opaque endpoint bytes.
        payload: Vec<u8>,
    },
    /// Native document retirement: no new admitted work for this document.
    DocumentRetired {
        /// Host-registered `WebView` identity.
        webview: u32,
        /// Native-owned committed navigation.
        navigation: u32,
    },
    /// Current native registration, independent of renderer application DATA.
    /// The matching Window event owns its committed URL. This fact grants no
    /// delivery authority, creates no DATA charge and has no consumer receipt.
    DocumentBound {
        /// Host-registered `WebView` identity, never inferred from a window ID.
        webview: u32,
        /// Exact native-owned committed navigation.
        navigation: u32,
        /// Existing Core WindowRef.id selected by the registered route owner.
        window_id: u32,
        /// Existing Core WindowRef.incarnation selected by that same owner.
        window_incarnation: u64,
    },
}

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

fn valid_diagnostic(error: &CallError) -> bool {
    let code = error.code.as_bytes();
    code.len() > b"KELD-".len()
        && code.len() <= 64
        && code.starts_with(b"KELD-")
        && code
            .iter()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || *byte == b'-')
        && error.message.len() <= 512
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
pub fn validate_event(event: &CompatControlEvent) -> Result<(), IpcError> {
    let good = match event {
        CompatControlEvent::RendererMessage {
            webview,
            navigation,
            message,
            window_id,
            window_incarnation,
            url,
            origin,
            topic,
            payload,
        } => {
            valid_identity(*webview)
                && valid_identity(*navigation)
                && valid_identity(*message)
                && valid_identity(*window_id)
                && *window_incarnation != 0
                && url.len() <= 4096
                && origin.len() <= 1024
                && valid_topic(topic)
                && valid_payload(payload)
        }
        CompatControlEvent::DocumentRetired {
            webview,
            navigation,
        } => valid_identity(*webview) && valid_identity(*navigation),
        CompatControlEvent::DocumentBound {
            webview,
            navigation,
            window_id,
            window_incarnation,
        } => {
            valid_identity(*webview)
                && valid_identity(*navigation)
                && valid_identity(*window_id)
                && *window_incarnation != 0
        }
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
    let value: CompatControlEvent = decode(bytes)?;
    validate_event(&value)?;
    Ok(value)
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
mod tests {
    use super::{CompatControlEvent, decode_event, encode_event};

    fn bound() -> CompatControlEvent {
        CompatControlEvent::DocumentBound {
            webview: 1,
            navigation: 2,
            window_id: 7,
            window_incarnation: 258,
        }
    }

    #[test]
    fn document_bound_has_append_only_literal_bytes() {
        let literal = [2, 1, 2, 7, 0x82, 2];
        assert_eq!(encode_event(&bound()).expect("encode bound"), literal);
        assert_eq!(decode_event(&literal).expect("decode literal"), bound());
        assert_eq!(
            encode_event(&CompatControlEvent::DocumentRetired {
                webview: 1,
                navigation: 2,
            })
            .expect("existing retirement"),
            [1, 1, 2]
        );
        let data = CompatControlEvent::RendererMessage {
            webview: 1,
            navigation: 2,
            message: 3,
            window_id: 7,
            window_incarnation: 258,
            url: String::from("x"),
            origin: String::from("null"),
            topic: String::from("y"),
            payload: vec![255],
        };
        assert_eq!(
            encode_event(&data).expect("existing DATA"),
            [
                0, 1, 2, 3, 7, 0x82, 2, 1, 120, 4, 110, 117, 108, 108, 1, 121, 1, 255
            ]
        );
    }

    #[test]
    fn document_bound_has_nonzero_identities_and_a_26_byte_maximum() {
        for (webview, navigation, window_id, window_incarnation) in
            [(0, 2, 7, 258), (1, 0, 7, 258), (1, 2, 0, 258), (1, 2, 7, 0)]
        {
            assert!(
                encode_event(&CompatControlEvent::DocumentBound {
                    webview,
                    navigation,
                    window_id,
                    window_incarnation,
                })
                .is_err()
            );
        }
        let maximum = CompatControlEvent::DocumentBound {
            webview: u32::MAX,
            navigation: u32::MAX,
            window_id: u32::MAX,
            window_incarnation: u64::MAX,
        };
        let literal = [
            2, 255, 255, 255, 255, 15, 255, 255, 255, 255, 15, 255, 255, 255, 255, 15, 255, 255,
            255, 255, 255, 255, 255, 255, 255, 1,
        ];
        assert_eq!(literal.len(), 26);
        assert_eq!(encode_event(&maximum).expect("maximum"), literal);
        assert_eq!(decode_event(&literal).expect("maximum literal"), maximum);
    }

    #[test]
    fn document_bound_rejects_malformed_wire() {
        let literal = [2, 1, 2, 7, 0x82, 2];
        for end in 0..literal.len() {
            assert!(decode_event(&literal[..end]).is_err());
        }
        for invalid in [
            &[2, 0, 2, 7, 1][..],
            &[2, 1, 0, 7, 1],
            &[2, 1, 2, 0, 1],
            &[2, 1, 2, 7, 0],
            &[3],
            &[2, 1, 2, 7, 0x82, 2, 0],
        ] {
            assert!(decode_event(invalid).is_err());
        }
    }
}

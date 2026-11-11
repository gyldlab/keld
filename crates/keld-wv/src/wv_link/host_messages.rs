//! GH1184 closed Windows Host-to-renderer carrier. This carries data, never JS.
//! Native source/lease/role admission and reservations remain the caller's duty.

use super::{
    AppPageCall, MAX_APP_WV_LINK_ENVELOPE_LEN, MAX_RENDERER_PAYLOAD_LEN, MAX_WV_LINK_ENVELOPE_LEN,
    WvLinkRejectReason, WvTerminalDisposition, valid_document,
};
use serde::Serialize;
use std::io::{self, Write};

/// One already admitted outcome. Borrowed data is encoded without a payload clone.
#[derive(Debug, Serialize)]
#[serde(tag = "kind")]
pub enum HostRendererMessage<'a> {
    /// Current document's authority-free binding acknowledgement.
    #[serde(rename = "bound")]
    Bound {
        /// Actual current authenticated route availability.
        ready: bool,
    },
    /// Addressed event with the existing endpoint-value bytes.
    #[serde(rename = "event")]
    Event {
        /// Existing nonzero delivery correlation.
        delivery: u32,
        /// Exact unprefixed literal channel.
        channel: &'a str,
        /// Opaque endpoint-value encoding.
        payload: &'a [u8],
    },
    /// Original one-way send terminal result, never a fabricated app response.
    #[serde(rename = "send-result")]
    SendResult {
        /// Original nonzero one-way correlation.
        message: u32,
        /// Existing closed terminal disposition.
        disposition: &'a WvTerminalDisposition,
    },
    /// Existing authenticated role availability; no role credential is serialized.
    #[serde(rename = "route-state")]
    RouteState {
        /// Whether this exact native route remains ready.
        ready: bool,
    },
    /// Native registration acknowledgement for the exact document.
    #[serde(rename = "app-registered")]
    AppRegistered,
    /// Refusal of one original local call.
    #[serde(rename = "app-refused")]
    AppRefused {
        /// Nonzero local call correlation.
        call: u32,
    },
    /// Native-admitted entry into an unchanged original preload method.
    #[serde(rename = "app-call")]
    AppCall {
        /// Document authenticated on the original local call; never serialized twice.
        #[serde(skip)]
        source_document: &'a str,
        /// Nonzero local correlation.
        call: u32,
        /// One of the existing four original methods.
        method: &'a str,
        /// Existing bounded Local graph, never executable values.
        args: &'a [serde_json::Value],
    },
    /// Numeric Echo success using the existing binary value.
    #[serde(rename = "reply")]
    Reply {
        /// Nonzero request correlation.
        request: u32,
        /// True discriminant, validated by the encoder.
        ok: bool,
        /// Existing opaque reply bytes.
        payload: &'a [u8],
    },
    /// Numeric Echo failure, sanitized by its existing error owner.
    #[serde(rename = "reply")]
    Error {
        /// Nonzero request correlation.
        request: u32,
        /// False discriminant, validated by the encoder.
        ok: bool,
        /// Registered error code.
        code: &'a str,
        /// Bounded sanitized error detail.
        detail: &'a str,
    },
}

#[derive(Serialize)]
struct Envelope<'a, 'b> {
    v: u8,
    document: &'a str,
    #[serde(flatten)]
    message: &'a HostRendererMessage<'b>,
}

struct BoundedWriter {
    bytes: Vec<u8>,
    maximum: usize,
}
impl Write for BoundedWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.maximum - self.bytes.len() {
            return Err(io::Error::other(
                "Host renderer envelope exceeds reserved bound",
            ));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'a> HostRendererMessage<'a> {
    /// Use the canonical already-decoded local call; no second Local codec.
    #[must_use]
    pub fn app_call(call: &'a AppPageCall) -> Self {
        Self::AppCall {
            source_document: &call.document,
            call: call.call,
            method: &call.method,
            args: &call.args,
        }
    }

    fn maximum(&self) -> usize {
        match self {
            Self::AppCall { .. } => MAX_APP_WV_LINK_ENVELOPE_LEN,
            Self::Event { .. } | Self::Reply { .. } => MAX_WV_LINK_ENVELOPE_LEN,
            _ => 4096,
        }
    }

    /// Encode only after the native owner reserves this representation's capacity.
    /// The exact-capacity buffer refuses before exceeding the closed carrier bound.
    ///
    /// # Errors
    /// Rejects malformed correlations, mismatched call document, invalid topics,
    /// excessive payloads and encoded size. This function grants no authority.
    pub fn encode(&self, document: &str) -> Result<Vec<u8>, WvLinkRejectReason> {
        if !valid_document(document) {
            return Err(WvLinkRejectReason::InvalidDocument);
        }
        match self {
            Self::Event {
                delivery,
                channel,
                payload,
            } => {
                if *delivery == 0 {
                    return Err(WvLinkRejectReason::ZeroRequest);
                }
                if channel.is_empty()
                    || channel.len() > 256
                    || channel.chars().any(|c| c == '*' || c.is_control())
                {
                    return Err(WvLinkRejectReason::InvalidNamedChannel);
                }
                if payload.len() > MAX_RENDERER_PAYLOAD_LEN {
                    return Err(WvLinkRejectReason::PayloadTooLarge);
                }
            }
            Self::SendResult { message: 0, .. }
            | Self::AppRefused { call: 0 }
            | Self::AppCall { call: 0, .. }
            | Self::Reply { request: 0, .. }
            | Self::Error { request: 0, .. } => return Err(WvLinkRejectReason::ZeroRequest),
            Self::Reply { ok, payload, .. } => {
                if !ok {
                    return Err(WvLinkRejectReason::MalformedEnvelope);
                }
                if payload.len() > MAX_RENDERER_PAYLOAD_LEN {
                    return Err(WvLinkRejectReason::PayloadTooLarge);
                }
            }
            Self::Error { ok: true, .. } => return Err(WvLinkRejectReason::MalformedEnvelope),
            Self::AppCall {
                source_document,
                method,
                args,
                ..
            } => {
                if *source_document != document {
                    return Err(WvLinkRejectReason::InvalidDocument);
                }
                super::validate_local_arguments(args)?;
                if !matches!(
                    *method,
                    "request" | "registerMsgListener" | "sendMessage" | "listenOnce"
                ) || args.len() > 16
                {
                    return Err(WvLinkRejectReason::MalformedEnvelope);
                }
            }
            _ => (),
        }
        let maximum = self.maximum();
        let mut writer = BoundedWriter {
            bytes: Vec::with_capacity(maximum),
            maximum,
        };
        serde_json::to_writer(
            &mut writer,
            &Envelope {
                v: 2,
                document,
                message: self,
            },
        )
        .map_err(|_| WvLinkRejectReason::MalformedEnvelope)?;
        Ok(writer.bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const DOC: &str = "0123456789abcdef0123456789abcdef";
    #[test]
    fn exact_binding_and_event_wire_bytes() {
        assert_eq!(HostRendererMessage::Bound { ready: false }.encode(DOC).unwrap(),
            br#"{"v":2,"document":"0123456789abcdef0123456789abcdef","kind":"bound","ready":false}"#);
        assert_eq!(HostRendererMessage::Event { delivery: 7, channel: "mainResp", payload: &[0, 255] }.encode(DOC).unwrap(),
            br#"{"v":2,"document":"0123456789abcdef0123456789abcdef","kind":"event","delivery":7,"channel":"mainResp","payload":[0,255]}"#);
    }
    #[test]
    fn closed_reply_shapes_never_include_authority_or_null_fields() {
        let ok = HostRendererMessage::Reply {
            request: 3,
            ok: true,
            payload: &[1],
        }
        .encode(DOC)
        .unwrap();
        assert_eq!(ok, br#"{"v":2,"document":"0123456789abcdef0123456789abcdef","kind":"reply","request":3,"ok":true,"payload":[1]}"#);
        let error = HostRendererMessage::Error {
            request: 3,
            ok: false,
            code: "KELD-IPC-001",
            detail: "refused",
        }
        .encode(DOC)
        .unwrap();
        assert_eq!(error, br#"{"v":2,"document":"0123456789abcdef0123456789abcdef","kind":"reply","request":3,"ok":false,"code":"KELD-IPC-001","detail":"refused"}"#);
    }
    #[test]
    fn zero_foreign_document_and_bounds_refuse() {
        assert!(
            HostRendererMessage::Bound { ready: true }
                .encode("not-a-document")
                .is_err()
        );
        assert!(
            HostRendererMessage::AppRefused { call: 0 }
                .encode(DOC)
                .is_err()
        );
        assert!(
            HostRendererMessage::Event {
                delivery: 1,
                channel: "*",
                payload: &[]
            }
            .encode(DOC)
            .is_err()
        );
        assert!(
            HostRendererMessage::Event {
                delivery: 1,
                channel: "c",
                payload: &vec![0; MAX_RENDERER_PAYLOAD_LEN + 1]
            }
            .encode(DOC)
            .is_err()
        );
        assert!(
            HostRendererMessage::Reply {
                request: 1,
                ok: false,
                payload: &[]
            }
            .encode(DOC)
            .is_err()
        );
        assert!(
            HostRendererMessage::Error {
                request: 1,
                ok: true,
                code: "e",
                detail: "x"
            }
            .encode(DOC)
            .is_err()
        );
        assert!(
            HostRendererMessage::Error {
                request: 1,
                ok: false,
                code: "e",
                detail: &"x".repeat(4096)
            }
            .encode(DOC)
            .is_err()
        );
    }
    #[test]
    fn app_call_cannot_be_retargeted_to_a_different_document() {
        let call = AppPageCall {
            document: DOC.to_owned(),
            call: 1,
            method: "request".to_owned(),
            args: vec![],
        };
        assert!(HostRendererMessage::app_call(&call).encode(DOC).is_ok());
        assert!(
            HostRendererMessage::app_call(&call)
                .encode("ffffffffffffffffffffffffffffffff")
                .is_err(),
            "an A call must not be encoded for document B"
        );
    }
}

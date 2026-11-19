use super::{
    CompatControlEvent, CompatControlRequest, CompatControlResponse, ConsumerDisposition,
    DispatchCertainty, TerminalDisposition, decode_event, decode_request, decode_response,
    encode_event, encode_request, encode_response,
};
use crate::channel_table::COMPAT_CONTROL;
use crate::codec::{decode, encode};
use crate::frame::{CorrelationId, FrameHeader, FrameKind};
use crate::link::read_validated_frame;
use crate::receive::validate_received_header;
use crate::{CallError, HEADER_LEN, IpcError, ReceivePolicy};
use std::fmt::Debug;
use std::io::Cursor;

const FIXTURE: &str = include_str!("../../tests/fixtures/compat-control-v0.tsv");

fn vector(id: &str) -> Vec<u8> {
    let hex = FIXTURE
        .lines()
        .find_map(|line| {
            let (name, suffix) = line.split_once('\t')?;
            let (_, hex) = suffix.split_once('\t')?;
            (name == id).then_some(hex)
        })
        .expect("approved compatibility vector");
    assert_eq!(hex.len() % 2, 0);
    (0..hex.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&hex[index..index + 2], 16).expect("literal hex"))
        .collect()
}

fn replay<T: Debug + PartialEq>(
    id: &str,
    expected: &T,
    decode: impl FnOnce(&[u8]) -> Result<T, IpcError>,
    encode: impl FnOnce(&T) -> Result<Vec<u8>, IpcError>,
) {
    let wire = vector(id);
    assert_eq!(
        &decode(&wire).expect("decode approved bytes"),
        expected,
        "{id}"
    );
    assert_eq!(
        encode(expected).expect("encode approved value"),
        wire,
        "{id}"
    );
}

#[test]
fn compat_requests_use_the_approved_postcard_vectors() {
    use CompatControlRequest::{DeliverToCurrentMain, DeliverToOriginalDocument, ReleaseConsumed};
    let requests = [
        (
            "deliver-current",
            DeliverToCurrentMain {
                webview: 1,
                topic: "x".into(),
                payload: vec![0, 1, 255],
            },
        ),
        (
            "deliver-original",
            DeliverToOriginalDocument {
                webview: 1,
                navigation: 2,
                topic: "x".into(),
                payload: vec![0, 1, 255],
            },
        ),
        (
            "consumed",
            ReleaseConsumed {
                webview: 1,
                navigation: 2,
                message: 3,
                disposition: ConsumerDisposition::Consumed,
            },
        ),
        (
            "no-listener",
            ReleaseConsumed {
                webview: 1,
                navigation: 2,
                message: 3,
                disposition: ConsumerDisposition::NoListener,
            },
        ),
        (
            "deliver-current-max",
            DeliverToCurrentMain {
                webview: u32::MAX,
                topic: "x".into(),
                payload: Vec::new(),
            },
        ),
    ];
    for (id, request) in requests {
        replay(id, &request, decode_request, encode_request);
    }
}

#[test]
fn compat_host_events_preserve_the_approved_field_order() {
    use super::CompatControlEventValue::{DocumentRetired, RendererMessage};
    let events: [(&str, CompatControlEvent); 3] = [
        (
            "renderer-message",
            RendererMessage {
                webview: 1,
                navigation: 2,
                message: 3,
                url: "file:///a".into(),
                origin: "null".into(),
                topic: "x".into(),
                payload: vec![0, 1, 255],
            },
        ),
        (
            "document-retired",
            DocumentRetired {
                webview: 1,
                navigation: 2,
            },
        ),
        (
            "document-retired-max",
            DocumentRetired {
                webview: u32::MAX,
                navigation: u32::MAX,
            },
        ),
    ];
    for (id, event) in events {
        replay(id, &event, decode_event, encode_event);
    }
    let wire = vector("renderer-message");
    for end in 0..wire.len() {
        assert!(decode_event(&wire[..end]).is_err());
    }
    assert!(decode_event(&[2]).is_err(), "unknown event ordinal");
    assert!(decode_event(&[1, 0, 1]).is_err(), "zero native identity");
    assert!(decode_event(&[1, 1, 2, 0]).is_err(), "trailing event bytes");
}

#[test]
fn compat_retirement_rejects_a_longer_received_body() {
    let mut extended_tag = vec![0x81, 0];
    extended_tag.extend_from_slice(&vector("document-retired-max")[1..]);
    assert_eq!(extended_tag.len(), 12);
    assert_eq!(
        decode::<CompatControlEvent>(&extended_tag).expect("postcard accepts the overlong tag"),
        CompatControlEvent::DocumentRetired {
            webview: u32::MAX,
            navigation: u32::MAX,
        }
    );
    assert!(matches!(
        decode_event(&extended_tag),
        Err(IpcError::PayloadTooLarge)
    ));
}

#[test]
fn compat_body_checks_borrowed_fields_before_owning_them() {
    use super::{COMPAT_ERROR_MESSAGE_BYTES, COMPAT_VALUE_BYTES, CompatControlEventValue};

    let payload = vec![7; COMPAT_VALUE_BYTES + 1];
    let event = CompatControlEventValue::RendererMessage {
        webview: 1,
        navigation: 2,
        message: 3,
        url: "file:///a",
        origin: "null",
        topic: "x",
        payload: payload.as_slice(),
    };
    let event_bytes = encode(&event).expect("physical fixture body");
    let borrowed: CompatControlEventValue<&str, &[u8]> =
        decode(&event_bytes).expect("borrow event fields");
    let CompatControlEventValue::RendererMessage {
        payload: borrowed_payload,
        ..
    } = borrowed
    else {
        panic!("fixture must be a renderer message");
    };
    assert_eq!(borrowed_payload, payload);
    assert!(
        event_bytes
            .as_ptr_range()
            .contains(&borrowed_payload.as_ptr())
    );
    assert!(matches!(
        decode_event(&event_bytes),
        Err(IpcError::Codec(_))
    ));

    let request = CompatControlRequest::DeliverToCurrentMain {
        webview: 1,
        topic: "x".into(),
        payload,
    };
    let request_bytes = encode(&request).expect("physical fixture body");
    assert!(matches!(
        decode::<CompatControlRequest>(&request_bytes),
        Err(IpcError::Codec(_))
    ));
    let request = CompatControlRequest::ReleaseConsumed {
        webview: 1,
        navigation: 2,
        message: 3,
        disposition: ConsumerDisposition::Refused {
            error: CallError {
                code: "KELD-GUARD003".into(),
                message: "x".repeat(COMPAT_ERROR_MESSAGE_BYTES + 1),
            },
        },
    };
    let request_bytes = encode(&request).expect("physical fixture body");
    assert!(matches!(
        decode::<CompatControlRequest>(&request_bytes),
        Err(IpcError::Codec(_))
    ));
}

#[test]
fn compat_responses_keep_delivery_and_release_distinct() {
    let deliveries = [
        ("deliver-consumed", TerminalDisposition::Consumed),
        ("deliver-no-listener", TerminalDisposition::NoListener),
        ("deliver-stale", TerminalDisposition::Stale),
        (
            "route-not-dispatched",
            TerminalDisposition::RouteRetired {
                certainty: DispatchCertainty::NotDispatched,
            },
        ),
        (
            "route-outcome-unknown",
            TerminalDisposition::RouteRetired {
                certainty: DispatchCertainty::OutcomeUnknown,
            },
        ),
    ];
    for (id, disposition) in deliveries {
        replay(
            id,
            &CompatControlResponse::Deliver { disposition },
            decode_response,
            encode_response,
        );
    }
    replay(
        "released",
        &CompatControlResponse::Released,
        decode_response,
        encode_response,
    );
    let error = CallError {
        code: "KELD-GUARD003".into(),
        message: "x".into(),
    };
    replay("call-error", &error, decode, encode);
    let mut nested = vec![0, 3];
    nested.extend(vector("call-error"));
    assert_eq!(
        decode_response(&nested).expect("nested CallError"),
        CompatControlResponse::Deliver {
            disposition: TerminalDisposition::Refused { error }
        },
    );
}

#[test]
fn compat_receiver_checks_direction_and_cap_before_the_body() {
    let call = ReceivePolicy::compat_call_receiver();
    let event = ReceivePolicy::event_receiver(&COMPAT_CONTROL).expect("compat EVENT policy");
    let reply = ReceivePolicy::reply_waiter(&COMPAT_CONTROL, CorrelationId(9))
        .expect("compat reply policy");
    assert!(ReceivePolicy::privileged_call_receiver(&COMPAT_CONTROL).is_err());
    for (policy, kind, corr, limit) in [
        (call, FrameKind::Call, 9, 16_384),
        (event, FrameKind::Event, 0, 16_384),
        (reply, FrameKind::Reply, 9, 4_096),
        (reply, FrameKind::Err, 9, 4_096),
    ] {
        let header = FrameHeader {
            kind,
            flags: 0,
            channel: COMPAT_CONTROL.id(),
            corr: CorrelationId(corr),
            len: limit,
        };
        validate_received_header(&policy, header).expect("exact approved cap");
        let over = FrameHeader {
            len: limit + 1,
            ..header
        };
        let mut stream = Cursor::new([over.encode().as_slice(), &[0xa4]].concat());
        assert!(matches!(
            read_validated_frame(&mut stream, &policy),
            Err(IpcError::PayloadTooLarge)
        ));
        assert_eq!(
            stream.position(),
            HEADER_LEN as u64,
            "body sentinel remains unread"
        );
        let wrong = if kind == FrameKind::Call { event } else { call };
        assert!(validate_received_header(&wrong, header).is_err());
    }
}

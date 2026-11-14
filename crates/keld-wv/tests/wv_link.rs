//! Contract tests for the private KEL-142 WebView-link envelope decoder.

use keld_wv::wv_link::{
    MAX_RENDERER_PAYLOAD_LEN, MAX_WV_LINK_ENVELOPE_LEN, WvLinkEnvelope, WvLinkMode,
    WvLinkRejectReason, decode_app_wv_link, decode_wv_link, decode_wv_link_for_mode,
};

fn invoke(document: &str, request: &str, channel: &str, payload: &str) -> String {
    format!(
        r#"{{"v":1,"kind":"invoke","document":"{document}","request":{request},"channel":{channel},"payload":[{payload}]}}"#
    )
}

const CONTRACT_PAYLOAD_MAX: usize = 4096;

fn repeated_byte_json(byte: u8, count: usize) -> String {
    std::iter::repeat_n(byte.to_string(), count)
        .collect::<Vec<_>>()
        .join(",")
}

#[test]
fn exact_bind_and_invoke_shapes_are_admitted() {
    assert_eq!(
        decode_wv_link(br#"{"v":1,"kind":"bind"}"#),
        Ok(WvLinkEnvelope::Bind)
    );
    assert_eq!(
        decode_wv_link(b"{\"v\":1,\"kind\":\"bind\"}\n\t "),
        Ok(WvLinkEnvelope::Bind),
        "trailing whitespace is JSON whitespace, not trailing data"
    );

    let encoded = invoke("doc-1", "4294967295", "65535", "0,255");
    let admitted = decode_wv_link(encoded.as_bytes()).expect("valid invoke");
    let WvLinkEnvelope::Invoke {
        document,
        request,
        channel,
        payload,
    } = admitted
    else {
        panic!("invoke decoded as bind");
    };
    assert_eq!(document, "doc-1");
    assert_eq!(request.get(), u32::MAX);
    assert_eq!(channel.get(), u16::MAX);
    assert_eq!(payload, [0, 255]);
}

#[test]
fn malformed_unknown_duplicate_and_authority_fields_fail_closed() {
    let malformed = [
        br#"{"v":1,"kind":"bind","extra":1}"#.as_slice(),
        br#"{"v":1,"v":1,"kind":"bind"}"#.as_slice(),
        br#"{"v":1,"kind":"bind","document":"x"}"#.as_slice(),
        br#"{"v":1,"kind":"unknown"}"#.as_slice(),
        br#"{"v":1,"kind":"invoke","document":"d","request":1,"channel":1}"#.as_slice(),
        br#"{"v":1,"kind":"invoke","document":"d","request":1,"channel":1,"payload":[],"token":"secret"}"#.as_slice(),
        br#"{"v":1,"kind":"invoke","document":"d","request":1,"channel":1,"payload":[],"endpoint":"/tmp/x"}"#.as_slice(),
        br#"{"v":1,"kind":"invoke","document":"d","request":1,"channel":1,"payload":[],"principal":"app"}"#.as_slice(),
        br#"{"v":1,"kind":"invoke","document":"d","request":1,"channel":1,"payload":[],"generation":2}"#.as_slice(),
        br#"{"v":1,"kind":"invoke","document":"d","request":1,"channel":1,"payload":[],"grant":"fs"}"#.as_slice(),
        br#"{"v":1,"kind":"invoke","document":null,"request":1,"channel":1,"payload":[]}"#.as_slice(),
        br#"{"v":1,"kind":"invoke","document":"d","request":1.5,"channel":1,"payload":[]}"#.as_slice(),
        br#"{"v":1,"kind":"invoke","document":"d","request":1,"channel":1.5,"payload":[]}"#.as_slice(),
        br#"{"v":1,"kind":"invoke","document":"d","request":1,"channel":1,"payload":[256]}"#.as_slice(),
        br#"{"v":1,"kind":"invoke","document":"d","request":1,"channel":1,"payload":[-1]}"#.as_slice(),
        br#"{"v":1,"kind":"bind"} trailing"#.as_slice(),
    ];

    for bytes in malformed {
        assert_eq!(
            decode_wv_link(bytes),
            Err(WvLinkRejectReason::MalformedEnvelope),
            "unexpected admission for {}",
            String::from_utf8_lossy(bytes)
        );
    }

    assert_eq!(
        decode_wv_link(&[0xff, 0xfe]),
        Err(WvLinkRejectReason::MalformedEnvelope)
    );
}

/// Unknown JSON fields resembling the old overflow sentinel must remain malformed.
#[test]
fn unknown_field_name_cannot_spoof_payload_overflow() {
    for bytes in [
        br#"{"v":1,"kind":"bind","keld-wv-link-payload-too-large":0}"#.as_slice(),
        br#"{"v":1,"kind":"bind","keld-wv-link-payload-\u0074oo-large":0}"#.as_slice(),
    ] {
        assert_eq!(
            decode_wv_link(bytes),
            Err(WvLinkRejectReason::MalformedEnvelope),
            "unknown fields cannot become payload-overflow reports: {}",
            String::from_utf8_lossy(bytes)
        );
    }
}

#[test]
fn version_and_nonzero_identifiers_have_distinct_rejections() {
    assert_eq!(
        decode_wv_link(br#"{"v":2,"kind":"bind"}"#),
        Err(WvLinkRejectReason::UnsupportedVersion)
    );

    let zero_request = invoke("d", "0", "1", "");
    assert_eq!(
        decode_wv_link(zero_request.as_bytes()),
        Err(WvLinkRejectReason::ZeroRequest)
    );

    let zero_channel = invoke("d", "1", "0", "");
    assert_eq!(
        decode_wv_link(zero_channel.as_bytes()),
        Err(WvLinkRejectReason::ZeroChannel)
    );

    for encoded in [
        invoke("d", "4294967296", "1", ""),
        invoke("d", "1", "65536", ""),
        invoke("d", "-1", "1", ""),
        invoke("d", "1", "-1", ""),
    ] {
        assert_eq!(
            decode_wv_link(encoded.as_bytes()),
            Err(WvLinkRejectReason::MalformedEnvelope)
        );
    }
}

#[test]
fn payload_boundary_is_exact_and_large_envelopes_are_prebounded() {
    assert_eq!(MAX_RENDERER_PAYLOAD_LEN, CONTRACT_PAYLOAD_MAX);
    let max = invoke(
        "d",
        "1",
        "1",
        &repeated_byte_json(255, CONTRACT_PAYLOAD_MAX),
    );
    let admitted = decode_wv_link(max.as_bytes()).expect("4096 bytes must be admitted");
    let WvLinkEnvelope::Invoke { payload, .. } = admitted else {
        panic!("invoke decoded as bind");
    };
    assert_eq!(payload.len(), CONTRACT_PAYLOAD_MAX);

    let too_large = invoke(
        "d",
        "1",
        "1",
        &repeated_byte_json(255, CONTRACT_PAYLOAD_MAX + 1),
    );
    assert_eq!(
        decode_wv_link(too_large.as_bytes()),
        Err(WvLinkRejectReason::PayloadTooLarge)
    );

    let huge_document = "x".repeat(MAX_WV_LINK_ENVELOPE_LEN);
    let huge = invoke(&huge_document, "1", "1", "");
    assert!(huge.len() > MAX_WV_LINK_ENVELOPE_LEN);
    assert_eq!(
        decode_wv_link(huge.as_bytes()),
        Err(WvLinkRejectReason::MalformedEnvelope)
    );
}

#[test]
fn compat_and_app_decoders_share_typed_payload_overflow() {
    let document = "0123456789abcdef0123456789abcdef";
    for (kind, id, channel) in [
        ("invoke", "request", "1"),
        ("send", "message", r#""mainResp""#),
    ] {
        for count in [CONTRACT_PAYLOAD_MAX, CONTRACT_PAYLOAD_MAX + 1] {
            let payload = repeated_byte_json(0, count);
            let input = format!(
                r#"{{"v":2,"kind":"{kind}","document":"{document}","{id}":1,"channel":{channel},"payload":[{payload}]}}"#
            );
            let transport = decode_wv_link_for_mode(input.as_bytes(), WvLinkMode::CompatV2);
            let app = decode_app_wv_link(input.as_bytes());
            if count == CONTRACT_PAYLOAD_MAX {
                assert!(transport.is_ok(), "{kind} exact boundary");
                assert!(app.is_ok(), "{kind} exact boundary");
            } else {
                assert_eq!(transport.err(), Some(WvLinkRejectReason::PayloadTooLarge));
                assert_eq!(app.err(), Some(WvLinkRejectReason::PayloadTooLarge));
            }
        }
    }
}

#[test]
fn displayed_json_keys_cannot_spoof_overflow_in_v2_or_app_roles() {
    for input in [
        br#"{"v":2,"kind":"bind","keld-wv-link-payload-too-large":0}"#.as_slice(),
        br#"{"v":2,"kind":"bind","keld-wv-link-payload-\u0074oo-large":0}"#.as_slice(),
    ] {
        assert_eq!(
            decode_wv_link_for_mode(input, WvLinkMode::CompatV2).err(),
            Some(WvLinkRejectReason::MalformedEnvelope)
        );
        assert_eq!(
            decode_app_wv_link(input).err(),
            Some(WvLinkRejectReason::MalformedEnvelope)
        );
    }
}

#[test]
fn payload_overflow_does_not_hide_a_malformed_tail() {
    let payload = repeated_byte_json(0, CONTRACT_PAYLOAD_MAX + 1);
    let legacy = invoke("d", "1", "1", &format!("{payload},256"));
    assert_eq!(
        decode_wv_link(legacy.as_bytes()).err(),
        Some(WvLinkRejectReason::MalformedEnvelope)
    );
    for tail in [",256]}", r#"],"unknown":0}"#] {
        let input = format!(
            r#"{{"v":2,"kind":"invoke","document":"0123456789abcdef0123456789abcdef","request":1,"channel":1,"payload":[{payload}{tail}"#
        );
        assert_eq!(
            decode_wv_link_for_mode(input.as_bytes(), WvLinkMode::CompatV2).err(),
            Some(WvLinkRejectReason::MalformedEnvelope)
        );
        assert_eq!(
            decode_app_wv_link(input.as_bytes()).err(),
            Some(WvLinkRejectReason::MalformedEnvelope)
        );
    }
}

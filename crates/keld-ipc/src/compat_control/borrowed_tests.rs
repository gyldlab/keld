//! Same canonical enum/validator and wire bytes for borrowed charged encoding.
use super::*;

fn borrowed<'a>(
    url: &'a str,
    origin: &'a str,
    topic: &'a str,
    payload: &'a [u8],
) -> CompatControlEventValue<&'a str, &'a [u8]> {
    CompatControlEventValue::RendererMessage {
        webview: 1,
        navigation: 2,
        message: 3,
        window_id: 7,
        window_incarnation: 258,
        url,
        origin,
        topic,
        payload,
    }
}

#[test]
fn borrowed_events_keep_all_literal_tags_fields_and_exact_counted_bytes() {
    let value = [0, 255, 1];
    let cases = [
        (
            CompatControlEvent::RendererMessage {
                webview: 1,
                navigation: 2,
                message: 3,
                window_id: 7,
                window_incarnation: 258,
                url: "file:///x".into(),
                origin: "file://".into(),
                topic: "topic".into(),
                payload: value.to_vec(),
            },
            borrowed("file:///x", "file://", "topic", &value),
        ),
        (
            CompatControlEvent::DocumentRetired {
                webview: 1,
                navigation: 2,
            },
            CompatControlEventValue::DocumentRetired {
                webview: 1,
                navigation: 2,
            },
        ),
        (
            CompatControlEvent::DocumentBound {
                webview: 1,
                navigation: 2,
                window_id: 7,
                window_incarnation: 258,
            },
            CompatControlEventValue::DocumentBound {
                webview: 1,
                navigation: 2,
                window_id: 7,
                window_incarnation: 258,
            },
        ),
    ];
    for (owned, borrowed) in cases {
        validate_event(&borrowed).expect("same validation");
        let expected = encode_event(&owned).expect("existing canonical encoder");
        let counted = crate::codec::encoded_len(&borrowed).expect("canonical count");
        let mut exact = vec![0; counted];
        let used = crate::codec::encode_into(&borrowed, &mut exact).expect("same enum serializer");
        assert_eq!(used, expected);
        assert_eq!(counted, expected.len());
        assert_eq!(decode_event(used).expect("same owned decode"), owned);
    }
}

#[test]
fn borrowed_validation_keeps_the_same_identity_and_field_boundaries() {
    let url = "u".repeat(4097);
    let origin = "o".repeat(1025);
    let topic = "t".repeat(COMPAT_TOPIC_BYTES + 1);
    let payload = vec![7; COMPAT_VALUE_BYTES + 1];
    for event in [
        borrowed(&url, "file://", "t", &[]),
        borrowed("u", &origin, "t", &[]),
        borrowed("u", "file://", &topic, &[]),
        borrowed("u", "file://", "t", &payload),
        CompatControlEventValue::DocumentRetired {
            webview: 0,
            navigation: 2,
        },
        CompatControlEventValue::DocumentBound {
            webview: 1,
            navigation: 2,
            window_id: 7,
            window_incarnation: 0,
        },
    ] {
        assert!(validate_event(&event).is_err());
        let bytes = crate::codec::encode(&event).expect("malformed fixture bytes");
        assert!(
            decode_event(&bytes).is_err(),
            "owned decoder enforces the same invalid field"
        );
    }
    validate_event(&borrowed(
        &url[..4096],
        &origin[..1024],
        &topic[..COMPAT_TOPIC_BYTES],
        &payload[..COMPAT_VALUE_BYTES],
    ))
    .expect("exact individual field maxima remain legal");
}

//! Repeatable contracts for the private-link fuzz corpus and v2 oracle bounds.
use keld_wv::wv_link::{
    WvConsumerDisposition, WvLinkEnvelope, WvLinkMode, WvLinkRejectReason, decode_wv_link_for_mode,
};

const INVOKE: &[u8] = include_bytes!("../fuzz/corpus/wv_link/valid-v2-invoke.json");
const SEND: &[u8] = include_bytes!("../fuzz/corpus/wv_link/valid-v2-send.json");
const CONSUMED: &[u8] = include_bytes!("../fuzz/corpus/wv_link/valid-v2-consumed.json");
const REFUSED: &[u8] = include_bytes!("../fuzz/corpus/wv_link/valid-v2-refused.json");
const ZERO_MESSAGE: &[u8] = include_bytes!("../fuzz/corpus/wv_link/v2-zero-message.json");

#[test]
fn committed_v2_corpus_has_exact_outcomes_in_both_modes() {
    for fixture in [INVOKE, SEND, CONSUMED, REFUSED, ZERO_MESSAGE] {
        assert!(decode_wv_link_for_mode(fixture, WvLinkMode::LegacyV1).is_err());
    }
    assert!(matches!(
        decode_wv_link_for_mode(INVOKE, WvLinkMode::CompatV2),
        Ok(WvLinkEnvelope::Invoke { .. })
    ));
    assert!(matches!(
        decode_wv_link_for_mode(SEND, WvLinkMode::CompatV2),
        Ok(WvLinkEnvelope::Send { .. })
    ));
    assert!(matches!(
        decode_wv_link_for_mode(CONSUMED, WvLinkMode::CompatV2),
        Ok(WvLinkEnvelope::Consumed {
            disposition: WvConsumerDisposition::Consumed,
            ..
        })
    ));
    assert!(matches!(
        decode_wv_link_for_mode(REFUSED, WvLinkMode::CompatV2),
        Ok(WvLinkEnvelope::Consumed {
            disposition: WvConsumerDisposition::Refused { .. },
            ..
        })
    ));
    assert_eq!(
        decode_wv_link_for_mode(ZERO_MESSAGE, WvLinkMode::CompatV2),
        Err(WvLinkRejectReason::ZeroRequest)
    );
}

#[test]
fn v2_invoke_document_is_hex_but_legacy_document_remains_opaque() {
    let valid = std::str::from_utf8(INVOKE).expect("JSON corpus");
    for document in [
        "opaque-document",
        "0123456789abcdef0123456789abcdeF",
        "0123456789abcdef0123456789abcdef0",
    ] {
        let invalid = valid.replace("0123456789abcdef0123456789abcdef", document);
        assert_eq!(
            decode_wv_link_for_mode(invalid.as_bytes(), WvLinkMode::CompatV2),
            Err(WvLinkRejectReason::InvalidDocument)
        );
        let legacy = invalid.replace("\"v\":2", "\"v\":1");
        assert!(
            matches!(decode_wv_link_for_mode(legacy.as_bytes(), WvLinkMode::LegacyV1), Ok(WvLinkEnvelope::Invoke { document: actual, .. }) if actual == document)
        );
    }
}

#[test]
fn v2_bind_encoded_boundary_does_not_narrow_legacy_bind() {
    for size in [4096, 4097] {
        let mut v2 = br#"{"v":2,"kind":"bind"}"#.to_vec();
        v2.resize(size, b' ');
        let expected = if size == 4096 {
            Ok(WvLinkEnvelope::Bind)
        } else {
            Err(WvLinkRejectReason::ControlTooLarge)
        };
        assert_eq!(decode_wv_link_for_mode(&v2, WvLinkMode::CompatV2), expected);
        let mut v1 = br#"{"v":1,"kind":"bind"}"#.to_vec();
        v1.resize(size, b' ');
        assert_eq!(
            decode_wv_link_for_mode(&v1, WvLinkMode::LegacyV1),
            Ok(WvLinkEnvelope::Bind)
        );
    }
}

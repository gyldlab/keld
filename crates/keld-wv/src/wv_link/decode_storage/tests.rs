use super::*;
use crate::wv_link::{AppWvLinkEnvelope, WvLinkRejectReason, decode_app_wv_link};
use serde::de::Visitor;
use std::cell::Cell;

const DOC: &str = "0123456789abcdef0123456789abcdef";

fn decode(input: &[u8], limit: usize) -> (Result<AppWvLinkEnvelope, WvLinkRejectReason>, usize) {
    let mut charged = 0usize;
    let mut reserve = |bytes| {
        let Some(next) = charged.checked_add(bytes).filter(|next| *next <= limit) else {
            return false;
        };
        charged = next;
        true
    };
    let result = AppDecodeAdmission::reserve(input.len(), &mut reserve)
        .and_then(|admitted| admitted.decode(input));
    (result, charged)
}

fn parity(input: &[u8]) {
    let old = decode_app_wv_link(input);
    let (new, _) = decode(input, usize::MAX);
    assert_eq!(
        format!("{new:?}"),
        format!("{old:?}"),
        "{:?}",
        String::from_utf8_lossy(input)
    );
}
fn call(args: &str) -> Vec<u8> {
    format!(
        r#"{{"v":1,"kind":"call","document":"{DOC}","call":1,"method":"request","args":{args}}}"#
    )
    .into_bytes()
}

#[test]
fn typed_schema_acceptance_and_exact_values_match_the_existing_owner() {
    let good = [
        br#"{"v":2,"kind":"bind"}"#.to_vec(),
        format!(r#"{{"v":2,"kind":"invoke","document":"{DOC}","request":4294967295,"channel":65535,"payload":[0,255]}}"#).into_bytes(),
        format!(r#"{{"v":1,"kind":"registered","document":"{DOC}","methods":["request","registerMsgListener","sendMessage","listenOnce"]}}"#).into_bytes(),
        call(r#"[{"x":[null,true,false,-9223372036854775808,18446744073709551615,1793581255029.1807,"plain","\u00e9","\ud83d\ude00"]}]"#),
        format!(r#"{{"v":2,"kind":"send","document":"{DOC}","message":1,"channel":"mainResp","payload":[]}}"#).into_bytes(),
        format!(r#"{{"v":2,"kind":"consumed","document":"{DOC}","delivery":1,"disposition":{{"status":"refused","code":"KELD-IPC-001","message":"\u0078"}}}}"#).into_bytes(),
    ];
    for input in good {
        assert!(
            decode_app_wv_link(&input).is_ok(),
            "canonical positive fixture"
        );
        assert!(decode(&input, 524_288).0.is_ok(), "same existing byte cap");
        parity(&input);
    }
    let input = call("[1793581255029.1807]");
    let AppWvLinkEnvelope::Call(value) = decode(&input, 524_288).0.unwrap() else {
        panic!("local call");
    };
    assert_eq!(
        value.args[0].as_f64().unwrap().to_bits(),
        "1793581255029.1807".parse::<f64>().unwrap().to_bits()
    );
}

#[test]
fn wrong_types_integer_edges_duplicate_unknown_and_untagged_channels_stay_closed() {
    let malformed = [
        br"null".to_vec(),
        br#"{"v":2,"v":2,"kind":"bind"}"#.to_vec(),
        br#"{"v":2,"kind":"bind","unknown":0}"#.to_vec(),
        br#"{"v":2.0,"kind":"bind"}"#.to_vec(),
        br#"{"v":-1,"kind":"bind"}"#.to_vec(),
        br#"{"v":18446744073709551616,"kind":"bind"}"#.to_vec(),
        br#"{"v":"2","kind":"bind"}"#.to_vec(),
        br#"{"v":null,"kind":"bind"}"#.to_vec(),
        br#"{"v":2,"kind":false}"#.to_vec(),
        br#"{"v":2,"kind":"bind"} true"#.to_vec(),
        call(r#"[{"a":1,"a":2}]"#),
        call("[1e309]"),
        call(r#"["\ud800"]"#),
    ];
    for input in malformed {
        assert!(
            decode_app_wv_link(&input).is_err(),
            "canonical negative fixture"
        );
        parity(&input);
    }
    for request in [
        "0",
        "-1",
        "1.5",
        "4294967296",
        "true",
        "null",
        "\"1\"",
        "[]",
        "{}",
    ] {
        let input = format!(
            r#"{{"v":2,"kind":"invoke","document":"{DOC}","request":{request},"channel":1,"payload":[]}}"#
        );
        parity(input.as_bytes());
    }
    for channel in [
        "0",
        "65536",
        "-1",
        "1.0",
        "null",
        "false",
        "[]",
        "{}",
        "\"1\"",
        "\"\\u0031\"",
    ] {
        let input = format!(
            r#"{{"v":2,"kind":"invoke","document":"{DOC}","request":1,"channel":{channel},"payload":[]}}"#
        );
        parity(input.as_bytes());
    }
    for byte in ["256", "-1", "1.5", "true", "null", "\"1\"", "[]", "{}"] {
        let input = format!(
            r#"{{"v":2,"kind":"invoke","document":"{DOC}","request":1,"channel":1,"payload":[{byte}]}}"#
        );
        parity(input.as_bytes());
    }
    // serde_json's existing struct deserializer also exposes sequence input.
    // Keep that actual baseline, including its defaults, rather than inventing
    // an object-only rule in the allocation wrapper.
    for input in [
        br#"[2,"bind"]"#.as_slice(),
        br"[]",
        br"[2]",
        br#"["2","bind"]"#,
    ] {
        parity(input);
    }
}

#[test]
fn typed_payload_overflow_and_malformed_diagnostics_use_fixed_error_storage() {
    for count in [4096, 4097] {
        let payload = vec![0u8; count];
        let input = format!(
            r#"{{"v":2,"kind":"invoke","document":"{DOC}","request":1,"channel":1,"payload":{}}}"#,
            serde_json::to_string(&payload).unwrap()
        );
        parity(input.as_bytes());
        if count == 4097 {
            assert!(matches!(
                decode(input.as_bytes(), usize::MAX).0,
                Err(WvLinkRejectReason::PayloadTooLarge)
            ));
        }
    }
    let quoted = format!(
        r#"{{"v":"{}{}","kind":"bind"}}"#,
        "x".repeat(32_768),
        "keld-wv-link-payload-too-large"
    );
    parity(quoted.as_bytes());
    assert_eq!(
        decode(quoted.as_bytes(), usize::MAX).0.err(),
        Some(WvLinkRejectReason::MalformedEnvelope)
    );
    assert!(size_of::<DecodeError>() <= size_of::<usize>());
    assert!(
        size_of::<RawCompatEnvelope>()
            + size_of::<AppWvLinkEnvelope>()
            + size_of::<serde_json::Deserializer<serde_json::de::SliceRead<'_>>>()
            + size_of::<DecodeError>()
            < 1024
    );
}

#[test]
fn initial_admission_precedes_copy_and_length_mismatch_never_enters_the_parser() {
    let calls = Cell::new(0usize);
    let mut refuse = |_| {
        calls.set(calls.get() + 1);
        false
    };
    assert!(AppDecodeAdmission::reserve(8, &mut refuse).is_err());
    assert_eq!(calls.get(), 1);
    let (result, charged) = decode(b"{", 0);
    assert!(result.is_err());
    assert_eq!(charged, 0);
    let mut bytes = 0usize;
    let mut reserve = |additional| {
        bytes += additional;
        true
    };
    let admitted = AppDecodeAdmission::reserve(1, &mut reserve).unwrap();
    assert!(admitted.decode(br#"{"v":2,"kind":"bind"}"#).is_err());
    assert_eq!(bytes, layout::initial_bytes(1).unwrap());
}

#[test]
fn allocation_refusal_runs_before_the_original_string_visitor() {
    struct CopyVisitor<'a>(&'a Cell<usize>);
    impl Visitor<'_> for CopyVisitor<'_> {
        type Value = String;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("a copied string")
        }
        fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<String, E> {
            self.0.set(self.0.get() + 1);
            Ok(value.to_owned())
        }
    }
    let copied = Cell::new(0);
    let mut refuse = |_| false;
    let mut state = State {
        reserve: &mut refuse,
        failure: None,
    };
    let mut json = serde_json::Deserializer::from_slice(br#""\u0061""#);
    let result = serde::Deserializer::deserialize_string(
        access::Admitted {
            inner: &mut json,
            state: &mut state,
        },
        CopyVisitor(&copied),
    );
    assert_eq!(result, Err(DecodeError::Storage));
    assert_eq!(copied.get(), 0, "no allocation after refusal");
}

#[test]
fn tiny_map_pressure_spends_the_same_byte_bound_before_container_growth() {
    let args = format!(
        "[[{}]]",
        (0..500).map(|_| r#"{"x":0}"#).collect::<Vec<_>>().join(",")
    );
    let input = call(&args);
    assert!(
        decode_app_wv_link(&input).is_ok(),
        "syntax budget permits this graph"
    );
    let (bounded, charged) = decode(&input, 524_288);
    assert!(
        bounded.is_err(),
        "many tiny nodes cannot bypass actual backing admission"
    );
    assert!(charged > 400_000 && charged <= 524_288);
    let (unlimited, larger) = decode(&input, usize::MAX);
    assert!(unlimited.is_ok());
    assert!(larger > 524_288);
}

#[test]
fn partial_errors_never_refund_storage_behind_the_callers_owner() {
    let input = call(r#"[{"a":[1,2,3],"a":"duplicate"}]"#);
    let (failed, retained) = decode(&input, 524_288);
    assert!(failed.is_err());
    assert!(retained > layout::initial_bytes(input.len()).unwrap());
    let input = call(r#"[{"a":[1,2,3],"b":"owned"}]"#);
    let (success, charged) = decode(&input, 524_288);
    assert!(success.is_ok());
    drop(success);
    let (refused, retained_before_refusal) = decode(&input, charged - 1);
    assert!(
        refused.is_err(),
        "one missing allocation admission must refuse"
    );
    assert!(retained_before_refusal > layout::initial_bytes(input.len()).unwrap());
    assert!(retained_before_refusal < charged);
}

#[test]
fn sequence_first_allocation_and_map_alignment_have_explicit_growth_headroom() {
    use std::alloc::Layout;
    assert!(layout::sequence_step(Layout::new::<u8>(), true).unwrap() >= 8);
    assert!(
        layout::sequence_step(Layout::new::<String>(), true).unwrap() >= 4 * size_of::<String>()
    );
    let key = Layout::from_size_align(64, 64).unwrap();
    let value = Layout::from_size_align(128, 128).unwrap();
    let charged = layout::map_entry(key, value).unwrap();
    assert!(charged >= 2 * (11 * (64 + 128) + 12 * size_of::<usize>()));
    assert!(layout::initial_bytes(usize::MAX).is_none());
    assert!(layout::string_bytes(usize::MAX).is_none());
}

#[test]
fn float_roundtrip_slow_path_is_prepaid_before_any_number_visitor() {
    // Reviewed pinned lexical bound: 320 simultaneous64-bit limbs, in
    // addition to1024 fixed metadata and16 bytes of scratch growth slack.
    assert_eq!(layout::initial_bytes(0), Some(3600));
    let values = [
        "2.2250738585072011e-308",
        "2.2250738585072014e-308",
        "4.9406564584124654e-324",
        "2.4703282292062327e-324",
        "1.7976931348623157e308",
    ];
    for literal in values {
        let input = call(&format!("[{literal}]"));
        parity(&input);
        assert!(decode(&input, 524_288).0.is_ok());
    }
    let mantissa = format!("1.{}1e-300", "23456789".repeat(100));
    let input = call(&format!("[{mantissa}]"));
    parity(&input);
    assert!(decode(&input, 524_288).0.is_ok());
}

#[test]
fn native_json_string_preserves_escaped_nul_for_the_unchanged_envelope_parser() {
    let raw = br#""{\"v\":2,\"kind\":\"bind\"}\u0000foreign""#;
    let mut charged = 0usize;
    let mut reserve = |bytes: usize| {
        charged += bytes;
        true
    };
    let text = AppDecodeAdmission::reserve(raw.len(), &mut reserve)
        .unwrap()
        .decode_native_string(raw)
        .unwrap();
    assert!(text.as_bytes().contains(&0));
    let mut reserve = |_| true;
    assert!(
        AppDecodeAdmission::reserve(text.len(), &mut reserve)
            .unwrap()
            .decode(text.as_bytes())
            .is_err(),
        "embedded NUL cannot hide a foreign tail"
    );
}

#[test]
fn native_transport_primitive_acceptance_matches_only_canonical_json_string() {
    for raw in [br#""plain""#.as_slice(), br#""escaped\n\u00e9""#.as_slice()] {
        let expected: String = serde_json::from_slice(raw).unwrap();
        let mut reserve = |_| true;
        let actual = AppDecodeAdmission::reserve(raw.len(), &mut reserve)
            .unwrap()
            .decode_native_string(raw)
            .unwrap();
        assert_eq!(actual, expected);
    }
    for raw in [
        b"null".as_slice(),
        b"true",
        b"1",
        b"1e-300",
        b"[]",
        b"{}",
        b"\"ok\" trailing",
    ] {
        let mut reserve = |_| true;
        assert!(
            AppDecodeAdmission::reserve(raw.len(), &mut reserve)
                .unwrap()
                .decode_native_string(raw)
                .is_err()
        );
    }
}

#[test]
fn native_transport_string_allocation_requires_the_same_credit_callback() {
    let raw = br#""owned copied value""#;
    let initial = initial_storage_bytes(raw.len()).unwrap();
    let mut charged = 0usize;
    let mut reserve = |bytes: usize| {
        if bytes > initial.saturating_sub(charged) {
            return false;
        }
        charged += bytes;
        true
    };
    let admitted = AppDecodeAdmission::reserve(raw.len(), &mut reserve).unwrap();
    assert!(admitted.decode_native_string(raw).is_err());
    assert_eq!(
        charged, initial,
        "refusal does not refund live raw/parser storage"
    );
}

#[test]
fn bounded_typed_overflow_cannot_be_spoofed_or_hide_malformed_input() {
    for input in [
        br#"{"v":2,"kind":"bind","keld-wv-link-payload-too-large":0}"#.as_slice(),
        br#"{"v":2,"kind":"bind","keld-wv-link-payload-\u0074oo-large":0}"#.as_slice(),
    ] {
        parity(input);
        assert_eq!(
            decode(input, 524_288).0.err(),
            Some(WvLinkRejectReason::MalformedEnvelope)
        );
    }
    let payload = std::iter::repeat_n("0", 4097).collect::<Vec<_>>().join(",");
    let send = format!(
        r#"{{"v":2,"kind":"send","document":"{DOC}","message":1,"channel":"mainResp","payload":[{payload}]}}"#
    );
    parity(send.as_bytes());
    let (result, retained) = decode(send.as_bytes(), 524_288);
    assert_eq!(result.err(), Some(WvLinkRejectReason::PayloadTooLarge));
    assert!(retained > layout::initial_bytes(send.len()).unwrap());
    for tail in [",256]}", r#"],"unknown":0}"#] {
        let input = format!(
            r#"{{"v":2,"kind":"invoke","document":"{DOC}","request":1,"channel":1,"payload":[{payload}{tail}"#
        );
        parity(input.as_bytes());
        let (result, retained) = decode(input.as_bytes(), 524_288);
        assert_eq!(result.err(), Some(WvLinkRejectReason::MalformedEnvelope));
        assert!(retained > layout::initial_bytes(input.len()).unwrap());
    }
}

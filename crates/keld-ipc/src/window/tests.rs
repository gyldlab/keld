use std::collections::BTreeMap;
use std::fmt::Debug;

use serde::Serialize;
use serde::de::DeserializeOwned;

use super::{
    CloseVerdict, Rect, WindowEvent, WindowGeneration, WindowId, WindowRef, WindowRequest,
    WindowResponse, WindowState,
};
use crate::IpcError;
use crate::codec::{decode, encode, encode_into};

mod desktop;

const VECTORS: &str = include_str!("../../tests/fixtures/window-v0.tsv");
const WINDOW: WindowRef = WindowRef {
    id: WindowId(7),
    incarnation: WindowGeneration(300),
};
const BOUNDS: Rect = Rect {
    x: -3,
    y: 4,
    width: 640,
    height: 480,
};
const STATE: WindowState = WindowState {
    bounds: BOUNDS,
    focused: true,
    maximized: false,
    full_screen: true,
};

fn vectors(family: &str) -> BTreeMap<&str, Vec<u8>> {
    let mut result = BTreeMap::new();
    for line in VECTORS.lines().filter(|line| !line.starts_with('#')) {
        let fields: Vec<_> = line.split('\t').collect();
        assert_eq!(fields.len(), 3, "malformed shared vector: {line}");
        if fields[1] != family {
            continue;
        }
        assert_eq!(fields[2].len() % 2, 0, "odd hex length: {line}");
        let bytes = fields[2]
            .as_bytes()
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| {
                u8::from_str_radix(std::str::from_utf8(pair).expect("ASCII hex"), 16)
                    .expect("literal hex byte")
            })
            .collect();
        assert!(
            result.insert(fields[0], bytes).is_none(),
            "duplicate vector {}",
            fields[0]
        );
    }
    result
}

fn assert_vectors<T: Serialize + DeserializeOwned + Debug + PartialEq>(
    family: &str,
    cases: &[(&str, T)],
) {
    let mut expected = vectors(family);
    for (id, value) in cases {
        let bytes = expected
            .remove(id)
            .expect("case exists once in shared oracle");
        assert_eq!(encode(value).expect("encode payload"), bytes, "{id}");
        let mut scratch = vec![0; super::NAVIGATION_EVENT_STORAGE_BYTES].into_boxed_slice();
        assert_eq!(
            encode_into(value, &mut scratch).expect("encode in fixed storage"),
            bytes,
            "{id}"
        );
        assert_eq!(
            decode::<T>(&bytes).expect("decode literal payload"),
            *value,
            "{id}"
        );

        for end in 0..bytes.len() {
            assert!(
                matches!(decode::<T>(&bytes[..end]), Err(IpcError::Codec(_))),
                "{id}: truncated payload of length {end} decoded"
            );
        }
        let mut trailing = bytes;
        trailing.push(0);
        assert!(
            matches!(decode::<T>(&trailing), Err(IpcError::Codec(_))),
            "{id}: trailing byte decoded"
        );
    }
    assert!(
        expected.is_empty(),
        "unreplayed {family} vectors: {expected:?}"
    );
}

#[test]
fn identity_geometry_and_state_match_shared_vectors() {
    assert_vectors(
        "reference",
        &[
            ("reference-distinct-identities", WINDOW),
            (
                "reference-integer-maxima",
                WindowRef {
                    id: WindowId(u32::MAX),
                    incarnation: WindowGeneration(u64::MAX),
                },
            ),
        ],
    );
    assert_vectors(
        "rect",
        &[
            ("rect-signed-coordinates", BOUNDS),
            (
                "rect-integer-boundaries",
                Rect {
                    x: i32::MIN,
                    y: i32::MAX,
                    width: u32::MAX,
                    height: 0,
                },
            ),
        ],
    );
    assert_vectors(
        "state",
        &[
            ("state-independent-flags", STATE),
            (
                "state-focused-only",
                WindowState {
                    bounds: BOUNDS,
                    focused: true,
                    maximized: false,
                    full_screen: false,
                },
            ),
            (
                "state-maximized-only",
                WindowState {
                    bounds: BOUNDS,
                    focused: false,
                    maximized: true,
                    full_screen: false,
                },
            ),
            (
                "state-full-screen-only",
                WindowState {
                    bounds: BOUNDS,
                    focused: false,
                    maximized: false,
                    full_screen: true,
                },
            ),
        ],
    );
}

#[test]
fn window_events_match_shared_vectors() {
    assert_vectors(
        "event",
        &[
            (
                "event-created",
                WindowEvent::Created {
                    window: WINDOW,
                    state: STATE,
                    pending_close: None,
                },
            ),
            (
                "event-created-pending-close",
                WindowEvent::Created {
                    window: WINDOW,
                    state: STATE,
                    pending_close: Some(300),
                },
            ),
            (
                "event-bounds",
                WindowEvent::Bounds {
                    window: WINDOW,
                    bounds: BOUNDS,
                },
            ),
            (
                "event-focus",
                WindowEvent::Focus {
                    window: WINDOW,
                    focused: true,
                },
            ),
            (
                "event-maximized",
                WindowEvent::Maximized {
                    window: WINDOW,
                    maximized: false,
                },
            ),
            (
                "event-full-screen",
                WindowEvent::FullScreen {
                    window: WINDOW,
                    full_screen: true,
                },
            ),
            (
                "event-close-requested",
                WindowEvent::CloseRequested {
                    window: WINDOW,
                    close_seq: 300,
                },
            ),
            ("event-destroyed", WindowEvent::Destroyed { window: WINDOW }),
            (
                "event-navigation-committed",
                WindowEvent::NavigationCommitted {
                    window: WINDOW,
                    document_generation: 5,
                    native_url: "keld-resource://0123456789abcdef0123456789abcdef/index.html?q=1"
                        .to_owned(),
                },
            ),
            (
                "event-navigation-finished",
                WindowEvent::NavigationFinished {
                    window: WINDOW,
                    document_generation: 5,
                },
            ),
        ],
    );
}

fn preload_request_vectors() -> [(&'static str, WindowRequest); 2] {
    [
        // Exact postcard bytes are independently pinned in window-v0.tsv.
        // This must fail to compile until tag-6 is appended at the Rust owner.
        (
            "request-create-preload-empty",
            WindowRequest::CreateWithPreload {
                width: 640,
                height: 480,
                x: Some(-3),
                y: Some(4),
                preload_absolute: String::from("/app/p.js"),
                additional_arguments: vec![],
            },
        ),
        (
            "request-create-preload-adaptive",
            WindowRequest::CreateWithPreload {
                width: 1,
                height: 2,
                x: None,
                y: None,
                preload_absolute: String::from("/a/p.js"),
                additional_arguments: vec![String::from("--initial-adaptive-colors=auto")],
            },
        ),
    ]
}

#[test]
fn window_requests_match_shared_vectors() {
    let mut cases = vec![
        (
            "request-create-positioned",
            WindowRequest::Create {
                width: 640,
                height: 480,
                x: Some(-3),
                y: Some(4),
            },
        ),
        (
            "request-create-no-position",
            WindowRequest::Create {
                width: 0,
                height: 0,
                x: None,
                y: None,
            },
        ),
        (
            "request-create-maxima",
            WindowRequest::Create {
                width: u32::MAX,
                height: u32::MAX,
                x: Some(i32::MIN),
                y: Some(i32::MAX),
            },
        ),
        (
            "request-maximized",
            WindowRequest::SetMaximized {
                window: WINDOW,
                maximized: true,
            },
        ),
        (
            "request-full-screen",
            WindowRequest::SetFullScreen {
                window: WINDOW,
                full_screen: false,
            },
        ),
        (
            "request-close",
            WindowRequest::RequestClose { window: WINDOW },
        ),
        (
            "request-close-allow",
            WindowRequest::CloseReply {
                window: WINDOW,
                close_seq: 300,
                verdict: CloseVerdict::Allow,
            },
        ),
        (
            "request-close-veto",
            WindowRequest::CloseReply {
                window: WINDOW,
                close_seq: 300,
                verdict: CloseVerdict::Veto,
            },
        ),
        ("request-destroy", WindowRequest::Destroy { window: WINDOW }),
        (
            "request-destroy-original-document",
            WindowRequest::DestroyOriginalDocument {
                window: WINDOW,
                document_generation: 5,
            },
        ),
        (
            "request-load-url",
            WindowRequest::LoadUrl {
                window: WINDOW,
                uri: "file:///s/drawio/src/main/webapp/index.html?q=1".to_owned(),
            },
        ),
    ];
    cases.extend(preload_request_vectors());
    assert_vectors("request", &cases);
}

#[test]
fn window_responses_match_shared_vectors() {
    assert_vectors(
        "response",
        &[
            (
                "response-created",
                WindowResponse::Created { window: WINDOW },
            ),
            ("response-accepted", WindowResponse::Accepted),
            (
                "response-close-requested",
                WindowResponse::CloseRequested { close_seq: 300 },
            ),
            ("response-merged", WindowResponse::Merged { close_seq: 300 }),
            ("response-already-closing", WindowResponse::AlreadyClosing),
            ("response-reply-applied", WindowResponse::ReplyApplied),
            ("response-reply-stale", WindowResponse::ReplyStale),
            ("response-destroyed", WindowResponse::Destroyed),
            ("response-loaded", WindowResponse::Loaded),
            (
                "response-adopted",
                WindowResponse::Adopted { window: WINDOW },
            ),
            (
                "response-close-sequence-maximum",
                WindowResponse::CloseRequested {
                    close_seq: u64::MAX,
                },
            ),
        ],
    );
    assert_vectors(
        "verdict",
        &[
            ("verdict-allow", CloseVerdict::Allow),
            ("verdict-veto", CloseVerdict::Veto),
        ],
    );
}

#[test]
fn window_payloads_reject_unknown_discriminants_and_invalid_scalar_values() {
    assert!(matches!(
        decode::<WindowEvent>(&[10]),
        Err(IpcError::Codec(_))
    ));
    assert!(matches!(
        decode::<WindowRequest>(&[9]),
        Err(IpcError::Codec(_))
    ));
    assert!(matches!(
        decode::<WindowResponse>(&[10]),
        Err(IpcError::Codec(_))
    ));
    assert!(matches!(
        decode::<CloseVerdict>(&[2]),
        Err(IpcError::Codec(_))
    ));
    // Focus bool = 2; valid discriminant and host reference precede it.
    assert!(matches!(
        decode::<WindowEvent>(&[2, 7, 0xac, 2, 2]),
        Err(IpcError::Codec(_))
    ));
    // Create x Option = 2; width and height are both zero.
    assert!(matches!(
        decode::<WindowRequest>(&[0, 0, 0, 2]),
        Err(IpcError::Codec(_))
    ));
    // A five-byte u32 whose high bits exceed the field's representable range.
    assert!(matches!(
        decode::<WindowId>(&[0xff, 0xff, 0xff, 0xff, 0x10]),
        Err(IpcError::Codec(_))
    ));
    assert!(matches!(
        decode::<WindowGeneration>(&[0xff; 10]),
        Err(IpcError::Codec(_))
    ));
}

#[test]
fn preload_metadata_bounds_are_utf8_bytes_not_characters() {
    fn request(path: String, arguments: Vec<String>) -> WindowRequest {
        WindowRequest::CreateWithPreload {
            width: 1,
            height: 2,
            x: None,
            y: None,
            preload_absolute: path,
            additional_arguments: arguments,
        }
    }
    for value in [
        request("/".to_owned() + &"x".repeat(2047), vec![]),
        request("/p".into(), vec![String::new(); 16]),
        request("/p".into(), vec!["é".repeat(128); 8]),
    ] {
        let bytes = encode(&value).expect("bounded request encodes");
        assert_eq!(
            decode::<WindowRequest>(&bytes).expect("bounded metadata"),
            value
        );
    }
    for value in [
        request(String::new(), vec![]),
        request("/".to_owned() + &"x".repeat(2048), vec![]),
        request("/".to_owned() + &"é".repeat(1024), vec![]),
        request("/p".into(), vec![String::new(); 17]),
        request("/p".into(), vec!["x".repeat(257)]),
        request("/p".into(), vec!["é".repeat(129)]),
        request("/p".into(), vec!["x".repeat(256); 9]),
    ] {
        let bytes = encode(&value).expect("untrusted wire can encode invalid metadata");
        assert!(matches!(
            decode::<WindowRequest>(&bytes),
            Err(IpcError::Codec(_))
        ));
    }
}

#[test]
fn borrowed_native_commit_matches_the_owning_literal_schema_without_url_copy() {
    let native_url = "keld-resource://0123456789abcdef0123456789abcdef/index.html?q=1";
    let mut scratch = vec![0_u8; super::NAVIGATION_EVENT_STORAGE_BYTES].into_boxed_slice();
    let bytes = super::encode_navigation_committed_into(WINDOW, 5, native_url, &mut scratch)
        .expect("borrowed native commit");
    let expected = vectors("event")
        .remove("event-navigation-committed")
        .expect("shared append-only literal");
    assert_eq!(bytes, expected);
    assert_eq!(
        decode::<WindowEvent>(bytes).expect("owned app decoder"),
        WindowEvent::NavigationCommitted {
            window: WINDOW,
            document_generation: 5,
            native_url: native_url.to_owned()
        }
    );
    let mut tiny = [0_u8; 4];
    assert!(super::encode_navigation_committed_into(WINDOW, 5, native_url, &mut tiny).is_err());
    let maximum = "a".repeat(8192);
    let bytes = super::encode_navigation_committed_into(WINDOW, u32::MAX, &maximum, &mut scratch)
        .expect("native UTF-8 reservation ceiling fits the owning wire storage");
    assert_eq!(
        decode::<WindowEvent>(bytes).expect("maximum observation"),
        WindowEvent::NavigationCommitted {
            window: WINDOW,
            document_generation: u32::MAX,
            native_url: maximum
        }
    );
}

#[test]
fn original_document_destroy_keeps_its_selector_and_generic_destroy_bytes() {
    let a = WindowRequest::DestroyOriginalDocument {
        window: WINDOW,
        document_generation: 5,
    };
    let b = WindowRequest::DestroyOriginalDocument {
        window: WINDOW,
        document_generation: 6,
    };
    assert_eq!(encode(&a).expect("original A selector"), [8, 7, 0xac, 2, 5]);
    assert_eq!(encode(&b).expect("original B selector"), [8, 7, 0xac, 2, 6]);
    assert_eq!(
        encode(&WindowRequest::Destroy { window: WINDOW }).expect("generic contract unchanged"),
        [5, 7, 0xac, 2]
    );
}

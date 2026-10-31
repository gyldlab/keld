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
        let mut scratch = [0; 64];
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
        ],
    );
}

#[test]
fn window_requests_match_shared_vectors() {
    assert_vectors(
        "request",
        &[
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
        ],
    );
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
        decode::<WindowEvent>(&[7]),
        Err(IpcError::Codec(_))
    ));
    assert!(matches!(
        decode::<WindowRequest>(&[6]),
        Err(IpcError::Codec(_))
    ));
    assert!(matches!(
        decode::<WindowResponse>(&[9]),
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

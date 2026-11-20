//! Contract tests for selected WINDOW view binding ingress.

use keld_ipc::{
    codec,
    window::{self, WindowEvent, WindowGeneration, WindowId, WindowRef},
};

const MAXIMUM: [u8; 21] = [
    0x0c, 0xff, 0xff, 0xff, 0xff, 0x0f, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x01,
    0xff, 0xff, 0xff, 0xff, 0x0f,
];
const OVERLONG: [u8; 22] = [
    0x8c, 0x00, 0xff, 0xff, 0xff, 0xff, 0x0f, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0x01, 0xff, 0xff, 0xff, 0xff, 0x0f,
];

#[test]
fn binding_literal_is_an_appended_window_event() {
    let literal = [0x0c, 7, 0xac, 2, 5];
    let event = window::decode_event(&literal).expect("checked binding decodes");
    assert_eq!(
        event,
        WindowEvent::OriginalRendererViewBound {
            window: WindowRef {
                id: WindowId(7),
                incarnation: WindowGeneration(300)
            },
            webview: 5,
        }
    );
    assert_eq!(codec::encode(&event).expect("binding encodes"), literal);
}

#[test]
fn binding_received_limit_counts_original_tag_bytes() {
    assert!(window::decode_event(&MAXIMUM).is_ok());
    assert!(window::decode_event(&OVERLONG).is_err());
    assert!(
        codec::decode::<WindowEvent>(&OVERLONG).is_ok(),
        "generic postcard is structural only"
    );
    assert_eq!(
        codec::decode::<(u32, u32, u64, u32)>(&OVERLONG).expect("unrelated schema"),
        (12, u32::MAX, u64::MAX, u32::MAX)
    );
}

#[test]
fn binding_rejects_each_zero_identity_and_malformed_tail() {
    for bytes in [
        vec![12, 0, 1, 1],
        vec![12, 1, 0, 1],
        vec![12, 1, 1, 0],
        vec![12, 1, 1],
        vec![12, 1, 1, 1, 0],
        vec![12, 0xff, 0xff, 0xff, 0xff, 0x10, 1, 1],
    ] {
        assert!(window::decode_event(&bytes).is_err(), "invalid {bytes:?}");
    }
}

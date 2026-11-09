//! GH-478: canonical postcard discriminants and hostile dialog envelopes.

use keld_ipc::codec::{decode, encode};
use keld_native::dialog::{
    CancelDisposition, DialogReplyV1, DialogRequestV1, decode_dialog_request,
};

fn file_filter() -> keld_native::dialog::FileDialogFilter {
    keld_native::dialog::FileDialogFilter {
        name: "draw.io".into(),
        extensions: vec!["drawio".into(), "xml".into()],
    }
}

#[test]
fn open_and_save_append_after_original_message_and_cancel_bytes() {
    use keld_native::dialog::{OpenDialogOptions, OpenProperty, SaveDialogOptions};
    let open = DialogRequestV1::Open {
        nonce: 1,
        parent: None,
        options: OpenDialogOptions {
            default_path: None,
            filters: vec![],
            properties: vec![OpenProperty::OpenFile, OpenProperty::OpenDirectory],
        },
    };
    let save = DialogRequestV1::Save {
        nonce: 1,
        parent: None,
        options: SaveDialogOptions {
            default_path: None,
            filters: vec![],
            security_scoped_bookmarks: false,
        },
    };
    let open_bytes = encode(&open).expect("Open postcard");
    let save_bytes = encode(&save).expect("Save postcard");
    assert_eq!(open_bytes, [2, 1, 0, 0, 0, 2, 0, 1]);
    assert_eq!(save_bytes, [3, 1, 0, 0, 0, 0]);
    assert_eq!(
        decode_dialog_request(&open_bytes).expect("decode Open"),
        open
    );
    assert_eq!(
        decode_dialog_request(&save_bytes).expect("decode Save"),
        save
    );
    assert_eq!(
        encode(&DialogReplyV1::Open {
            canceled: true,
            file_paths: vec![],
        })
        .expect("empty Open cancellation"),
        [2, 1, 0]
    );
    assert_eq!(
        encode(&DialogReplyV1::Save {
            canceled: true,
            file_path: String::new(),
        })
        .expect("empty Save cancellation"),
        [3, 1, 0]
    );
    assert_eq!(
        encode(&DialogReplyV1::Message {
            response: 2,
            checkbox_checked: false,
        })
        .unwrap(),
        [0, 2, 0]
    );
    assert_eq!(
        encode(&DialogReplyV1::Cancel {
            disposition: CancelDisposition::Requested,
        })
        .unwrap(),
        [1, 0]
    );
}

#[test]
fn file_dialog_options_reject_unsafe_filter_grammar_and_bookmark_authority() {
    use keld_native::dialog::{OpenDialogOptions, OpenProperty, SaveDialogOptions};
    let minimal_open = OpenDialogOptions {
        default_path: Some("filename.drawio".into()),
        filters: vec![file_filter()],
        properties: vec![OpenProperty::OpenFile, OpenProperty::OpenDirectory],
    };
    let request = |options| DialogRequestV1::Open {
        nonce: 1,
        parent: None,
        options,
    };
    assert!(decode_dialog_request(&encode(&request(minimal_open.clone())).unwrap()).is_ok());
    for extensions in [
        vec!["*.drawio".into()],
        vec![".xml".into()],
        vec!["x/y".into()],
        vec!["drawio".into(), "drawio".into()],
        vec![],
        vec!["x".repeat(129)],
        vec!["*".into(), "drawio".into()],
        vec!["a\u{007f}".into()],
    ] {
        let mut invalid = minimal_open.clone();
        invalid.filters[0].extensions = extensions;
        assert!(decode_dialog_request(&encode(&request(invalid)).unwrap()).is_err());
    }
    let mut too_many = minimal_open.clone();
    too_many.filters = vec![file_filter(); 33];
    assert!(decode_dialog_request(&encode(&request(too_many)).unwrap()).is_err());
    let mut duplicate_property = minimal_open.clone();
    duplicate_property.properties.push(OpenProperty::OpenFile);
    assert!(decode_dialog_request(&encode(&request(duplicate_property)).unwrap()).is_err());
    let mut overlong = minimal_open;
    overlong.default_path = Some("x".repeat(4097));
    assert!(decode_dialog_request(&encode(&request(overlong)).unwrap()).is_err());

    let save = SaveDialogOptions {
        default_path: None,
        filters: vec![file_filter()],
        security_scoped_bookmarks: true,
    };
    assert!(
        decode_dialog_request(
            &encode(&DialogRequestV1::Save {
                nonce: 1,
                parent: None,
                options: save,
            })
            .unwrap()
        )
        .is_err(),
        "bookmark must not mint a scope/blank file"
    );
}

#[test]
fn file_result_paths_reject_nonabsolute_or_unsafe_native_values_and_cancel_with_files() {
    use keld_native::dialog::validate_dialog_reply;
    let positive = DialogReplyV1::Open {
        canceled: false,
        file_paths: vec!["/tmp/one.drawio".into(), "/tmp/two.xml".into()],
    };
    validate_dialog_reply(&positive).expect("two real absolute POSIX paths");
    for path in [
        "",
        "./relative.drawio",
        "C:\\Windows",
        "../escape",
        "/tmp/x\0y",
        "/tmp/dir\\file",
    ] {
        assert!(
            validate_dialog_reply(&DialogReplyV1::Open {
                canceled: false,
                file_paths: vec![path.into()],
            })
            .is_err(),
            "bad path {path:?}"
        );
        assert!(
            validate_dialog_reply(&DialogReplyV1::Save {
                canceled: false,
                file_path: path.into(),
            })
            .is_err()
        );
    }
    assert!(
        validate_dialog_reply(&DialogReplyV1::Open {
            canceled: true,
            file_paths: vec!["/tmp/last-choice".into()],
        })
        .is_err(),
        "Cancel must never replay the last selected path"
    );
    assert!(
        validate_dialog_reply(&DialogReplyV1::Save {
            canceled: true,
            file_path: "/tmp/last-choice".into(),
        })
        .is_err()
    );
    assert!(
        validate_dialog_reply(&DialogReplyV1::Open {
            canceled: false,
            file_paths: vec!["/tmp/valid".into(); 33],
        })
        .is_err(),
        "returned paths bounded at 32 before the wire"
    );
}

#[test]
fn cancel_uses_a_distinct_wire_variant_and_a_nonzero_nonce() {
    let bytes = [1, 1];
    let request = decode_dialog_request(&bytes).expect("canonical Cancel");
    assert!(matches!(request, DialogRequestV1::Cancel { nonce: 1 }));
    assert_eq!(encode(&request).expect("encode Cancel"), bytes);
    assert!(decode_dialog_request(&[1, 0]).is_err(), "zero nonce");
    assert!(decode_dialog_request(&[1, 1, 0]).is_err(), "trailing byte");
    assert!(decode_dialog_request(&[2, 1]).is_err(), "unknown variant");
}

#[test]
fn message_and_cancel_replies_have_distinct_canonical_payloads() {
    let message = DialogReplyV1::Message {
        response: 2,
        checkbox_checked: false,
    };
    let cancel = DialogReplyV1::Cancel {
        disposition: CancelDisposition::Requested,
    };
    assert_eq!(encode(&message).expect("message reply"), [0, 2, 0]);
    assert_eq!(encode(&cancel).expect("cancel reply"), [1, 0]);
    assert!(
        decode::<DialogReplyV1>(&[1, 0, 1]).is_err(),
        "trailing byte"
    );
    assert!(decode::<DialogReplyV1>(&[0, 2, 2]).is_err(), "invalid bool");
}

fn options() -> keld_native::dialog::MessageOptions {
    keld_native::dialog::MessageOptions {
        message: "x".to_owned(),
        kind: keld_native::dialog::MessageKind::Question,
        buttons: vec![],
        default_id: None,
        cancel_id: None,
        normalize_access_keys: false,
        title: None,
        detail: None,
        no_link: true,
        checkbox_label: None,
        checkbox_checked: None,
    }
}

fn request(options: keld_native::dialog::MessageOptions) -> Result<Vec<u8>, keld_ipc::IpcError> {
    encode(&DialogRequestV1::Message {
        nonce: 1,
        parent: None,
        options,
    })
}

#[test]
fn message_options_enforce_bounds_and_relations_before_ui() {
    use keld_native::dialog::{MAX_DIALOG_BUTTONS, MAX_DIALOG_OPTIONS_BYTES};
    let minimal = options();
    let wire = request(minimal.clone()).expect("encode request");
    assert_eq!(wire, [0, 1, 0, 1, b'x', 3, 0, 0, 0, 0, 0, 0, 1, 0, 0]);
    assert!(decode_dialog_request(&wire).is_ok());
    let mut invalid_kind = wire.clone();
    invalid_kind[5] = 5;
    assert!(
        decode_dialog_request(&invalid_kind).is_err(),
        "unknown style"
    );
    let mut invalid_length = wire;
    invalid_length[6] = 17;
    assert!(
        decode_dialog_request(&invalid_length).is_err(),
        "oversized button vector"
    );
    let mut boundary = minimal.clone();
    boundary.buttons = vec![String::new(); MAX_DIALOG_BUTTONS];
    assert!(decode_dialog_request(&request(boundary.clone()).expect("encode request")).is_ok());
    boundary.buttons.push(String::new());
    assert!(decode_dialog_request(&request(boundary).expect("encode request")).is_err());

    let mut boundary = minimal.clone();
    boundary.message = "x".repeat(MAX_DIALOG_OPTIONS_BYTES - 32);
    while encode(&boundary).expect("encode options").len() < MAX_DIALOG_OPTIONS_BYTES {
        boundary.message.push('x');
    }
    assert_eq!(
        encode(&boundary).expect("encode options").len(),
        MAX_DIALOG_OPTIONS_BYTES
    );
    assert!(decode_dialog_request(&request(boundary.clone()).expect("encode request")).is_ok());
    boundary.message.push('x');
    assert!(decode_dialog_request(&request(boundary).expect("encode request")).is_err());

    let mut invalid = minimal.clone();
    invalid.default_id = Some(1);
    assert!(decode_dialog_request(&request(invalid).expect("encode request")).is_err());
    let mut invalid = minimal.clone();
    invalid.checkbox_checked = Some(false);
    assert!(decode_dialog_request(&request(invalid).expect("encode request")).is_err());
    let mut empty_label = minimal.clone();
    empty_label.checkbox_label = Some(String::new());
    assert!(decode_dialog_request(&request(empty_label.clone()).expect("encode request")).is_ok());
    empty_label.checkbox_checked = Some(false);
    assert!(decode_dialog_request(&request(empty_label).expect("encode request")).is_err());
    let mut valid = minimal;
    valid.detail = Some("embedded\0text".to_owned());
    assert!(decode_dialog_request(&request(valid).expect("encode request")).is_ok());
}

#[test]
fn message_parent_ref_has_rust_owned_literal_field_order() {
    use keld_ipc::window::{WindowGeneration, WindowId, WindowRef};
    let value = DialogRequestV1::Message {
        nonce: 1,
        parent: Some(WindowRef {
            id: WindowId(300),
            incarnation: WindowGeneration(513),
        }),
        options: options(),
    };
    let bytes = [
        0, 1, 1, 0xac, 0x02, 0x81, 0x04, 1, b'x', 3, 0, 0, 0, 0, 0, 0, 1, 0, 0,
    ];
    assert_eq!(encode(&value).expect("encode parent"), bytes);
    assert_eq!(decode_dialog_request(&bytes).expect("decode parent"), value);
}

//! Exact wire/bounds contracts; these do not claim native menu presentation.
use keld_ipc::codec;
use keld_native::menu::{
    MenuAction, MenuFault, MenuInstallReceipt, MenuKind, MenuNodeV1, MenuRequestV1, MenuRole,
    decode_menu_request, validate_menu_request,
};

fn ordinary() -> MenuNodeV1 {
    MenuNodeV1 {
        depth: 0,
        kind: MenuKind::Normal,
        label: "X".to_owned(),
        enabled: true,
        checked: None,
        action: MenuAction::NativeRole(MenuRole::Copy),
    }
}
#[test]
fn menu_wire_matches_the_independent_postcard_vector_and_real_receipt() {
    let request = MenuRequestV1 {
        nodes: vec![ordinary()],
    };
    assert_eq!(
        codec::encode(&request).expect("encode"),
        [1, 0, 0, 1, 88, 1, 0, 0, 8]
    );
    assert_eq!(
        decode_menu_request(&[1, 0, 0, 1, 88, 1, 0, 0, 8]).expect("decode"),
        request
    );
    assert_eq!(
        codec::encode(&MenuInstallReceipt { generation: 300 }).expect("receipt"),
        [172, 2]
    );
    let checkbox = MenuRequestV1 {
        nodes: vec![MenuNodeV1 {
            kind: MenuKind::Checkbox,
            checked: Some(false),
            action: MenuAction::UnsupportedClick,
            ..ordinary()
        }],
    };
    assert_eq!(
        codec::encode(&checkbox).expect("encode"),
        [1, 0, 3, 1, 88, 1, 1, 0, 1]
    );
}
#[test]
fn menu_decode_refuses_hostile_collection_counts_trailing_and_unknown_discriminants() {
    for payload in [
        &[][..],
        &[129, 2][..],
        &[1, 0, 99][..],
        &[1, 0, 0, 1, 88, 1, 0, 0, 99][..],
        &[0, 0][..],
    ] {
        assert!(decode_menu_request(payload).is_err(), "{payload:?}");
    }
    assert_eq!(
        decode_menu_request(&[0])
            .expect("empty projection")
            .nodes
            .len(),
        0
    );
}
#[test]
fn menu_shape_count_depth_label_and_checked_limits_are_independent() {
    assert!(
        validate_menu_request(&MenuRequestV1 {
            nodes: vec![ordinary(); 256]
        })
        .is_ok()
    );
    assert!(
        validate_menu_request(&MenuRequestV1 {
            nodes: vec![ordinary(); 257]
        })
        .is_err()
    );
    for bad in [
        MenuNodeV1 {
            depth: 1,
            ..ordinary()
        },
        MenuNodeV1 {
            depth: 9,
            ..ordinary()
        },
        MenuNodeV1 {
            label: "x\0y".to_owned(),
            ..ordinary()
        },
        MenuNodeV1 {
            label: "é".repeat(2049),
            ..ordinary()
        },
        MenuNodeV1 {
            checked: Some(false),
            ..ordinary()
        },
        MenuNodeV1 {
            kind: MenuKind::Checkbox,
            ..ordinary()
        },
        MenuNodeV1 {
            kind: MenuKind::Separator,
            ..ordinary()
        },
    ] {
        assert!(validate_menu_request(&MenuRequestV1 { nodes: vec![bad] }).is_err());
    }
    assert!(
        validate_menu_request(&MenuRequestV1 {
            nodes: vec![
                ordinary(),
                MenuNodeV1 {
                    depth: 1,
                    ..ordinary()
                }
            ]
        })
        .is_err()
    );
    let tree = (0..=8)
        .map(|depth| MenuNodeV1 {
            depth,
            kind: if depth == 8 {
                MenuKind::Normal
            } else {
                MenuKind::Submenu
            },
            action: MenuAction::Inert,
            ..ordinary()
        })
        .collect();
    assert!(validate_menu_request(&MenuRequestV1 { nodes: tree }).is_ok());
    assert!(
        validate_menu_request(&MenuRequestV1 {
            nodes: vec![
                MenuNodeV1 {
                    label: "x".repeat(4096),
                    ..ordinary()
                };
                256
            ]
        })
        .is_err()
    );
}
#[test]
fn role_defaults_have_one_owner_and_no_close_quit_selector_is_carried_on_wire() {
    assert_eq!(MenuRole::Close.name(), "close");
    assert_eq!(MenuRole::Close.key(), "w");
    assert_eq!(MenuRole::Quit.key(), "q");
    assert_eq!(MenuRole::HideOthers.modifiers(), 5);
    assert_eq!(MenuRole::PasteAndMatchStyle.modifiers(), 7);
    assert_eq!(MenuRole::Unhide.key(), "");
}

#[test]
fn typed_menu_failures_share_the_registered_resource_free_code_and_fix_owner() {
    for (fault, code, ticket) in [
        (MenuFault::InvalidProjection, "KELD-COMPAT-052", "Menu"),
        (MenuFault::UnsupportedClick, "KELD-COMPAT-053", "GH556"),
        (MenuFault::UnsupportedQuit, "KELD-COMPAT-054", "GH563"),
        (MenuFault::NativeFailure, "KELD-NATIVE-017", "Host/AppKit"),
    ] {
        let error = fault.call_error();
        assert_eq!(error.code, code);
        assert!(error.message.starts_with(code));
        assert!(error.message.contains(fault.fix()));
        assert!(error.message.contains(ticket));
    }
}

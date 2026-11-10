//! GH1189 consumer bytes for the pinned Open options; no native panel or new codec.
use keld_ipc::codec::encode;
use keld_native::dialog::{
    DialogRequestV1, FileDialogFilter, OpenDialogOptions, OpenProperty, decode_dialog_request,
};

#[test]
fn pinned_open_options_match_rust_owned_filter_vector() {
    // Desktop 2edf9fb9 / webapp 24b76c2c chooseFileEntry. The saved-directory
    // value, nonce and absent parent are synthetic fixture inputs, not Host proof.
    let request = DialogRequestV1::Open {
        nonce: 1,
        parent: None,
        options: OpenDialogOptions {
            default_path: Some("C:\\Users\\fixture\\Documents".into()),
            filters: vec![
                FileDialogFilter {
                    name: "draw.io Diagrams".into(),
                    extensions: ["drawio", "xml", "png", "svg", "html", "pdf"]
                        .into_iter()
                        .map(String::from)
                        .collect(),
                },
                FileDialogFilter {
                    name: "VSDX Documents".into(),
                    extensions: vec!["vsdx".into()],
                },
                FileDialogFilter {
                    name: "Mermaid".into(),
                    extensions: vec!["mmd".into(), "mermaid".into()],
                },
                FileDialogFilter {
                    name: "All Files".into(),
                    extensions: vec!["*".into()],
                },
            ],
            properties: vec![OpenProperty::OpenFile],
        },
    };
    let actual = encode(&request).expect("existing Rust dialog encoder");
    let expected: Vec<u8> = serde_json::from_str(include_str!("fixtures/dialog_open_filters.json"))
        .expect("shared literal byte vector");
    assert_eq!(actual, expected, "consumer Open bytes changed");
    assert_eq!(
        decode_dialog_request(&expected).expect("existing Rust dialog decoder"),
        request,
        "literal vector must retain every ordered filter field"
    );
    let mut reordered = request.clone();
    if let DialogRequestV1::Open { options, .. } = &mut reordered {
        options.filters.reverse();
    }
    let reordered_bytes = encode(&reordered).expect("reordered consumer request");
    assert_ne!(reordered_bytes, expected, "filter order is observable");
    assert_eq!(
        decode_dialog_request(&reordered_bytes).expect("reordered vector decode"),
        reordered
    );
}

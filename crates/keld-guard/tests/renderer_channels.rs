//! Exact renderer-topic decisions stay separate from native path permits.
//! The test exercises Guard policy only; native identity and effects are Core-owned.

use keld_guard::{
    Decision, DenyReason, NativeCapabilityQuery, Principal, RendererChannelQuery, evaluate_query,
    parse_manifest,
};

fn view() -> Principal {
    Principal::Webview {
        id: 7,
        generation: 3,
    }
}

#[test]
fn exact_el_channel_uses_window_key_and_minted_webview_shape_only() {
    let manifest = parse_manifest(
        r#"{"profile":"legacy","windows":{"editor":{"channels":["el:doc.open","el:doc.saved"]}}}"#,
    )
    .expect("closed policy");
    let editor = manifest.window_channel_grants("editor");
    assert_eq!(editor.window_key(), Some("editor"));
    let allowed = evaluate_query(
        &manifest,
        view(),
        RendererChannelQuery {
            channel: "doc.open",
            current_window: editor,
        },
    );
    assert!(matches!(&allowed, Decision::Allow(_)), "{allowed:?}");

    for channel in [
        "doc",
        "doc.open/child",
        "el:doc.open",
        "doc.open*",
        "",
        "doc.open\n",
    ] {
        let outcome = evaluate_query(
            &manifest,
            view(),
            RendererChannelQuery {
                channel,
                current_window: editor,
            },
        );
        assert!(
            matches!(
                &outcome,
                Decision::Deny(DenyReason::ChannelForbidden { .. })
            ),
            "{channel:?}: {outcome:?}"
        );
    }
    let missing = manifest.window_channel_grants("not-editor");
    assert_eq!(missing.window_key(), None);
    assert!(matches!(
        evaluate_query(
            &manifest,
            view(),
            RendererChannelQuery {
                channel: "doc.open",
                current_window: missing,
            },
        ),
        Decision::Deny(DenyReason::ChannelForbidden { .. })
    ));
}

#[test]
fn app_and_plugin_principals_never_inherit_a_window_channel_or_app_scope() {
    let manifest = parse_manifest(
        r#"{"app":{"fs":{"read":["/data/**"]}},"windows":{"editor":{"channels":["el:doc.open"]}}}"#,
    )
    .expect("policy");
    let window = manifest.window_channel_grants("editor");
    for principal in [
        Principal::AppProcess,
        Principal::Plugin { id: 1 },
        Principal::Webview {
            id: 0,
            generation: 2,
        },
        Principal::Webview {
            id: 7,
            generation: 0,
        },
    ] {
        assert!(matches!(
            evaluate_query(
                &manifest,
                principal,
                RendererChannelQuery {
                    channel: "doc.open",
                    current_window: window,
                },
            ),
            Decision::Deny(DenyReason::ChannelForbidden { .. })
        ));
    }

    // Native grants use the original AppProcess scope and a distinct permit.
    let native = NativeCapabilityQuery {
        operation: "fs.read",
        path: "/data/notes.xml",
    };
    assert!(matches!(
        evaluate_query(&manifest, Principal::AppProcess, native),
        Decision::Allow(_)
    ));
    assert!(matches!(
        evaluate_query(&manifest, view(), native),
        Decision::Deny(DenyReason::NotAppProcess { .. })
    ));
    assert!(matches!(
        evaluate_query(
            &manifest,
            view(),
            RendererChannelQuery {
                channel: "fs.read",
                current_window: window,
            },
        ),
        Decision::Deny(DenyReason::ChannelForbidden { .. })
    ));
}

#[test]
fn a_foreign_manifest_handle_cannot_select_an_otherwise_matching_channel() {
    let a =
        parse_manifest(r#"{"windows":{"editor":{"channels":["el:open"]}}}"#).expect("manifest a");
    let b =
        parse_manifest(r#"{"windows":{"editor":{"channels":["el:open"]}}}"#).expect("manifest b");
    let wrong_owner = a.window_channel_grants("editor");
    let outcome = evaluate_query(
        &b,
        view(),
        RendererChannelQuery {
            channel: "open",
            current_window: wrong_owner,
        },
    );
    assert!(
        matches!(
            &outcome,
            Decision::Deny(DenyReason::ChannelForbidden { .. })
        ),
        "{outcome:?}"
    );
}

#[test]
fn malformed_or_wildcard_renderer_channel_declarations_never_become_permits() {
    for source in [
        r#"{"windows":{"editor":{"channels":["fs.read"]}}}"#,
        r#"{"windows":{"editor":{"channels":["el:"]}}}"#,
        r#"{"windows":{"editor":{"channels":["el:*"]}}}"#,
        r#"{"windows":{"editor":{"channels":["el:open*"]}}}"#,
        r#"{"windows":{"editor":{"channels":["el:open\n"]}}}"#,
        r#"{"windows":{"editor":{"channels":["el:open",null]}}}"#,
        r#"{"windows":{"editor":{"channels":"el:open"}}}"#,
        r#"{"windows":{"editor":{"channels":["el:open"],"channels":["el:open"]}}}"#,
    ] {
        assert!(parse_manifest(source).is_err(), "{source}");
    }
    let missing = parse_manifest(r#"{"windows":{"editor":{}}}"#).expect("empty default deny");
    let outcome = evaluate_query(
        &missing,
        view(),
        RendererChannelQuery {
            channel: "doc.open",
            current_window: missing.window_channel_grants("editor"),
        },
    );
    assert!(matches!(
        outcome,
        Decision::Deny(DenyReason::ChannelForbidden { .. })
    ));
}

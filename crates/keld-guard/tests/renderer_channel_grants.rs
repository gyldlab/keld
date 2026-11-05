//! Window-selection and decision atoms. These tests make no native-auth claim.
use keld_guard::{
    Decision, DenyReason, NativeCapabilityQuery, Principal, RendererChannelQuery, evaluate_query,
    parse_manifest,
};

fn webview() -> Principal {
    Principal::Webview {
        id: 7,
        generation: 3,
    }
}

#[test]
fn exact_decoded_literals_cannot_inherit_neighbor_app_or_normalized_grants()
-> Result<(), Box<dyn std::error::Error>> {
    let policy = parse_manifest(
        r#"{"app":{"fs":{"read":["/**"]}},"windows":{
            "editor":{"channels":["el:\u0072endererReq","el:bad%20name","el:e\u0301","el:a","el:a"]},
            "other":{"channels":["el:other"]}}}"#,
    )?;
    let selected = policy.window_channel_grants("editor");
    assert_eq!(selected.window_key(), Some("editor"));
    for topic in ["rendererReq", "bad%20name", "e\u{0301}", "a"] {
        assert!(
            matches!(
                evaluate_query(
                    &policy,
                    webview(),
                    RendererChannelQuery {
                        channel: topic,
                        current_window: selected,
                    }
                ),
                Decision::Allow(_)
            ),
            "{topic}"
        );
    }
    for (key, topic) in [
        ("other", "rendererReq"),
        ("editor", "rendererreq"),
        ("editor", "rendererReqX"),
        ("editor", " rendererReq"),
        ("editor", "bad name"),
        ("editor", "é"),
        ("missing", "rendererReq"),
        ("editor", "other"),
        ("editor", "el:rendererReq"),
        ("editor", ""),
    ] {
        assert!(
            matches!(
                evaluate_query(
                    &policy,
                    webview(),
                    RendererChannelQuery {
                        channel: topic,
                        current_window: policy.window_channel_grants(key),
                    }
                ),
                Decision::Deny(DenyReason::ChannelForbidden { .. })
            ),
            "{key}/{topic}"
        );
    }
    Ok(())
}

#[test]
fn missing_grants_legacy_and_invalid_numeric_principal_shapes_are_denied()
-> Result<(), Box<dyn std::error::Error>> {
    for source in [
        "{}",
        r#"{"app":{"fs":{"read":["/**"]}}}"#,
        r#"{"profile":"legacy","windows":{"editor":{}}}"#,
        r#"{"profile":"legacy","windows":{"editor":{"channels":[]}}}"#,
    ] {
        let policy = parse_manifest(source)?;
        assert!(matches!(
            evaluate_query(
                &policy,
                webview(),
                RendererChannelQuery {
                    channel: "rendererReq",
                    current_window: policy.window_channel_grants("editor"),
                }
            ),
            Decision::Deny(DenyReason::ChannelForbidden { .. })
        ));
    }
    let policy = parse_manifest(r#"{"windows":{"editor":{"channels":["el:a"]}}}"#)?;
    for principal in [
        Principal::AppProcess,
        Principal::Plugin { id: 1 },
        Principal::Webview {
            id: 0,
            generation: 1,
        },
        Principal::Webview {
            id: 1,
            generation: 0,
        },
    ] {
        assert!(matches!(
            evaluate_query(
                &policy,
                principal,
                RendererChannelQuery {
                    channel: "a",
                    current_window: policy.window_channel_grants("editor"),
                }
            ),
            Decision::Deny(DenyReason::ChannelForbidden { .. })
        ));
    }
    Ok(())
}

#[test]
fn a_selection_from_another_snapshot_cannot_authorize_the_current_policy()
-> Result<(), Box<dyn std::error::Error>> {
    let first = parse_manifest(r#"{"windows":{"editor":{"channels":["el:a"]}}}"#)?;
    let second = parse_manifest(r#"{"windows":{"editor":{"channels":[]}}}"#)?;
    let query = RendererChannelQuery {
        channel: "a",
        current_window: first.window_channel_grants("editor"),
    };
    assert!(matches!(
        evaluate_query(&first, webview(), query),
        Decision::Allow(_)
    ));
    assert!(matches!(
        evaluate_query(&second, webview(), query),
        Decision::Deny(DenyReason::ChannelForbidden { .. })
    ));
    Ok(())
}

#[test]
fn el_fs_read_is_only_a_message_and_never_authorizes_native_app_scopes()
-> Result<(), Box<dyn std::error::Error>> {
    let policy = parse_manifest(r#"{"windows":{"editor":{"channels":["el:fs.read"]}}}"#)?;
    assert!(matches!(
        evaluate_query(
            &policy,
            webview(),
            RendererChannelQuery {
                channel: "fs.read",
                current_window: policy.window_channel_grants("editor"),
            }
        ),
        Decision::Allow(_)
    ));
    assert!(matches!(
        evaluate_query(
            &policy,
            webview(),
            NativeCapabilityQuery {
                operation: "fs.read",
                path: "/secret",
            }
        ),
        Decision::Deny(DenyReason::NotAppProcess { .. })
    ));
    assert!(matches!(
        evaluate_query(
            &policy,
            Principal::AppProcess,
            NativeCapabilityQuery {
                operation: "fs.read",
                path: "/secret",
            }
        ),
        Decision::Deny(DenyReason::NotGranted { .. })
    ));
    Ok(())
}

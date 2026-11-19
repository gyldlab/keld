use std::path::Path;

use serde_json::{Value, json};

use super::super::parse_boot_bytes;
use super::{InitialWindow, WINDOWS_GRAPH_BOOT_SCHEMA};

fn descriptor() -> Value {
    json!({
        "schema": 8,
        "name": "Editor",
        "entry": "src/main.mjs",
        "permissions": {
            "file": "keld.permissions.jsonc",
            "content_sha256": format!("sha256:{}", "11".repeat(32)),
        },
        "graph": {
            "file": "keld.stage-graph.json",
            "content_sha256": format!("sha256:{}", "22".repeat(32)),
        },
        "rendererSource": {"root": "client/assets", "entry": "nested/editor.html"},
        "preload": {
            "path": "runtime/preload.cjs",
            "content_sha256": format!("sha256:{}", "33".repeat(32)),
        },
    })
}

#[test]
fn schema8_selects_only_closed_relative_inputs_and_preserves_metadata() {
    let mut value = descriptor();
    let bare = parse_boot_bytes(value.to_string().as_bytes()).expect("metadata is optional");
    let bare = bare.windows_graph.expect("graph data");
    assert!(bare.version.is_none());
    assert!(bare.single_instance.is_none());
    assert!(bare.application_name_for_user_agent.is_none());

    value["version"] = json!("31.7.0");
    value["singleInstance"] = json!({"id": "org.example.editor"});
    value["applicationNameForUserAgent"] = json!("Example Editor/31.7.0");
    let boot = parse_boot_bytes(value.to_string().as_bytes()).expect("exact boot descriptor");
    assert_eq!(boot.schema, WINDOWS_GRAPH_BOOT_SCHEMA);
    assert_eq!(boot.name, "Editor");
    assert_eq!(boot.entry, Path::new("src/main.mjs"));
    assert!(matches!(boot.initial, InitialWindow::None));
    assert!(boot.auth_configuration.is_empty());
    assert_eq!(boot.permissions_digest, [0x11; 32]);
    let graph = boot.windows_graph.expect("graph data");
    assert_eq!(graph.graph_sha256, [0x22; 32]);
    assert_eq!(graph.source.root(), Path::new("client/assets"));
    assert_eq!(graph.source.entry(), Path::new("nested/editor.html"));
    assert_eq!(
        graph.source.preload_path(),
        Path::new("runtime/preload.cjs")
    );
    assert_eq!(graph.source.preload_sha256(), [0x33; 32]);
    assert_eq!(graph.version.as_deref(), Some("31.7.0"));
    assert_eq!(
        graph
            .single_instance
            .as_ref()
            .map(crate::SingleInstanceDeclaration::id),
        Some("org.example.editor")
    );
    assert_eq!(
        graph
            .application_name_for_user_agent
            .as_ref()
            .map(keld_wv::ApplicationNameForUserAgent::as_str),
        Some("Example Editor/31.7.0")
    );
}

#[test]
fn schema8_refuses_unknown_null_position_and_duplicate_fields() {
    let valid = descriptor().to_string();
    let duplicate_text = valid.replacen("\"schema\":8", "\"schema\":8,\"schema\":8", 1);
    assert_ne!(duplicate_text, valid, "duplicate must be present");
    assert!(parse_boot_bytes(duplicate_text.as_bytes()).is_err());
    let duplicate_graph = valid.replacen(
        "\"file\":\"keld.stage-graph.json\"",
        "\"file\":\"keld.stage-graph.json\",\"file\":\"keld.stage-graph.json\"",
        1,
    );
    assert_ne!(
        duplicate_graph, valid,
        "duplicate nested field must be present"
    );
    assert!(parse_boot_bytes(duplicate_graph.as_bytes()).is_err());
    for (field, contents) in [
        ("schema", json!(6)),
        ("profile", json!("legacy")),
        ("renderer", json!("index.html")),
        ("auth", json!({})),
        ("route", json!("installed")),
        ("version", Value::Null),
        ("version", json!("")),
        ("version", json!("x".repeat(4097))),
        ("version", json!("ab\0c")),
        ("singleInstance", Value::Null),
        ("singleInstance", json!(["org.example.editor"])),
        (
            "singleInstance",
            json!({"id":"org.example.editor","extra":true}),
        ),
        ("singleInstance", json!({"id":"../outside"})),
        ("applicationNameForUserAgent", Value::Null),
        ("applicationNameForUserAgent", json!(" leading space")),
        ("entry", json!("../editor.mjs")),
        ("entry", json!("src/ma\0in.mjs")),
        ("name", json!("Editor\0")),
        (
            "rendererSource",
            json!(["client/assets", "nested/editor.html"]),
        ),
        (
            "rendererSource",
            json!({"root":"client/assets","entry":"../editor.html"}),
        ),
        (
            "permissions",
            json!({"file":"other.json","content_sha256":format!("sha256:{}", "11".repeat(32))}),
        ),
        (
            "graph",
            json!({"file":"keld.stage-graph.json","content_sha256":format!("sha256:{}", "FF".repeat(32))}),
        ),
        (
            "preload",
            json!(["runtime/preload.cjs", format!("sha256:{}", "33".repeat(32))]),
        ),
    ] {
        let mut value = descriptor();
        value[field] = contents;
        assert!(
            parse_boot_bytes(value.to_string().as_bytes()).is_err(),
            "{field}"
        );
    }
    for field in ["permissions", "graph", "rendererSource", "preload"] {
        let mut value = descriptor();
        value
            .as_object_mut()
            .expect("descriptor object")
            .remove(field);
        assert!(
            parse_boot_bytes(value.to_string().as_bytes()).is_err(),
            "{field}"
        );
    }
}

#[test]
fn schema8_preflight_requires_verified_explicit_legacy_and_one_window_policy() {
    use crate::app_session::profile_preflight::validate_windows_graph_preflight;
    use keld_guard::ProfileState;
    use keld_guard::verified_manifest::load_verified_manifest;
    use sha2::{Digest as _, Sha256};
    use std::fs::{self, File};

    let directory = tempfile::tempdir().expect("owned policy fixture");
    let path = directory.path().join("keld.permissions.jsonc");
    for (policy, expected) in [
        (
            r#"{"profile":"legacy","windows":{"editor":{"web":{"csp":"original-editor-offline"}}}}"#,
            Some(ProfileState::Legacy),
        ),
        (
            r#"{"profile":"strict","windows":{"editor":{"web":{"csp":"original-editor-offline"}}}}"#,
            None,
        ),
        (r#"{"profile":"legacy"}"#, None),
        (
            r#"{"profile":"legacy","windows":{"one":{"web":{"csp":"original-editor-offline"}},"two":{"web":{"csp":"original-editor-offline"}}}}"#,
            None,
        ),
        ("{}", None),
    ] {
        fs::write(&path, policy).expect("fixture policy");
        let digest = Sha256::digest(policy.as_bytes()).into();
        let manifest =
            load_verified_manifest(File::open(&path).expect("owned file"), path.clone(), digest)
                .expect("verified policy");
        assert_eq!(
            validate_windows_graph_preflight(WINDOWS_GRAPH_BOOT_SCHEMA, &manifest).ok(),
            expected
        );
        assert!(validate_windows_graph_preflight(4, &manifest).is_err());
    }
}

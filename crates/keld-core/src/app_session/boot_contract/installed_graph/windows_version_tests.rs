use super::parse_selected_graph_shape;

fn document() -> serde_json::Value {
    serde_json::json!({
        "schema":8,"name":"Declared version","entry":"main/original.mjs",
        "permissions":{"file":"keld.permissions.jsonc","content_sha256":format!("sha256:{}","11".repeat(32))},
        "graph":{"file":"keld.stage-graph.json","content_sha256":format!("sha256:{}","22".repeat(32))},
        "rendererSource":{"root":"client/assets","entry":"nested/editor.html"},
        "preload":{"path":"bootstrap/preload.cjs","content_sha256":format!("sha256:{}","33".repeat(32))}
    })
}

#[test]
fn boot8_retains_only_the_exact_optional_declared_version() {
    let absent = parse_selected_graph_shape(&document().to_string(), 8).expect("old descriptor");
    assert!(
        absent
            .development_graph
            .expect("same graph selection")
            .version
            .is_none()
    );
    for value in ["31.7.0-selected".to_owned(), "λ".repeat(2048)] {
        let mut declared = document();
        declared["version"] = serde_json::json!(value);
        let boot = parse_selected_graph_shape(&declared.to_string(), 8).expect("bounded metadata");
        let selected = boot.development_graph.expect("same selection");
        assert_eq!(selected.version.as_deref(), Some(value.as_str()));
        assert_eq!(selected.graph_sha256, [0x22; 32]);
    }
}

#[test]
fn boot8_declared_version_cannot_disappear_on_null_invalid_or_duplicate_input() {
    for value in [
        serde_json::Value::Null,
        serde_json::json!(1),
        serde_json::json!([]),
        serde_json::json!(""),
        serde_json::json!("nul\0value"),
        serde_json::json!("x".repeat(4097)),
        serde_json::json!("λ".repeat(2049)),
    ] {
        let mut bad = document();
        bad["version"] = value;
        assert!(parse_selected_graph_shape(&bad.to_string(), 8).is_err());
    }
    let text = document().to_string();
    let duplicate = format!(
        "{{\"version\":\"first\",\"version\":\"second\",{}",
        &text[1..]
    );
    assert!(parse_selected_graph_shape(&duplicate, 8).is_err());
    for (field, value) in [
        (
            "singleInstance",
            serde_json::json!({"id":"dev.example.metadata"}),
        ),
        (
            "applicationNameForUserAgent",
            serde_json::json!("Unsupported/1"),
        ),
    ] {
        let mut bad = document();
        bad["version"] = serde_json::json!("allowed-version");
        bad[field] = value;
        assert!(parse_selected_graph_shape(&bad.to_string(), 8).is_err());
    }
}

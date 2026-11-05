//! Host cold-consumer contract; parsing does not claim native UA behavior.
use super::{parse_boot_bytes, valid_boot};

fn descriptor(schema: u8) -> serde_json::Value {
    let mut value: serde_json::Value =
        serde_json::from_slice(&valid_boot("src/main.ts", "index.html")).expect("old descriptor");
    value["schema"] = schema.into();
    if matches!(schema, 2 | 4 | 5) {
        value.as_object_mut().expect("object").remove("renderer");
    }
    if schema == 5 {
        value["graph"] = serde_json::json!({"file":"keld.stage-graph.json", "content_sha256": format!("sha256:{}", "ab".repeat(32))});
    }
    value
}

#[test]
fn application_name_for_user_agent_existing_boot_families_admit_or_refuse_platform() {
    #[cfg(target_os = "macos")]
    let schemas = [1, 2, 3, 4, 5].as_slice();
    #[cfg(not(target_os = "macos"))]
    let schemas = [1, 2, 3, 4].as_slice();
    for &schema in schemas {
        for text in [
            String::from("FirstPartyB/2 FrameworkMetadata"),
            "~".repeat(256),
        ] {
            let mut value = descriptor(schema);
            value["applicationNameForUserAgent"] = text.into();
            let bytes = serde_json::to_vec(&value).expect("cold descriptor");
            #[cfg(target_os = "macos")]
            {
                let parsed = parse_boot_bytes(&bytes).expect("validated cold metadata");
                assert_eq!(
                    parsed
                        .application_name_for_user_agent
                        .as_ref()
                        .map(keld_wv::ApplicationNameForUserAgent::as_str),
                    value["applicationNameForUserAgent"].as_str(),
                    "retained exact value in schema {schema}"
                );
                assert_eq!(parsed.name, "Fixture");
                assert!(parsed.auth_configuration.is_empty());
                assert_eq!(
                    parsed.permissions_digest,
                    [
                        0xca, 0x3d, 0x16, 0x3b, 0xab, 0x05, 0x53, 0x81, 0x82, 0x72, 0x26, 0x14,
                        0x05, 0x68, 0xf3, 0xbe, 0xf7, 0xea, 0xac, 0x18, 0x7c, 0xeb, 0xd7, 0x68,
                        0x78, 0xe0, 0xb6, 0x3e, 0x9e, 0x44, 0x23, 0x56
                    ]
                );
            }
            #[cfg(not(target_os = "macos"))]
            {
                let error = match parse_boot_bytes(&bytes) {
                    Ok(_) => panic!("unsupported backend ignored metadata"),
                    Err(error) => error,
                };
                assert!(
                    error.to_string().contains("applicationNameForUserAgent"),
                    "{error}"
                );
                assert!(error.to_string().contains("macOS"), "{error}");
            }
        }
    }
}

#[test]
fn application_name_for_user_agent_boot_rejects_invalid_duplicate_and_unknown_fields() {
    for value in [
        serde_json::Value::Null,
        serde_json::json!(7),
        serde_json::json!(false),
        serde_json::json!(""),
        serde_json::json!(" leading"),
        serde_json::json!("trailing "),
        serde_json::json!("a\nb"),
        serde_json::json!("a\tb"),
        serde_json::json!("a\0b"),
        serde_json::json!("a\u{7f}b"),
        serde_json::json!("café"),
        serde_json::json!("x".repeat(257)),
    ] {
        let mut document = descriptor(1);
        document["applicationNameForUserAgent"] = value;
        assert!(
            parse_boot_bytes(&serde_json::to_vec(&document).expect("invalid descriptor")).is_err()
        );
    }
    let old = String::from_utf8(valid_boot("src/main.ts", "index.html")).expect("UTF-8");
    for fields in [
        r#""applicationNameForUserAgent":"A","applicationNameForUserAgent":"B","#,
        r#""applicationNameForUserAgent":"A","applicationNameForUser\u0041gent":"B","#,
        r#""applicationNameForUserAgent":"A","unknownUserAgentMetadata":"B","#,
    ] {
        let bytes = old.replacen('{', &format!("{{{fields}"), 1);
        assert!(
            parse_boot_bytes(bytes.as_bytes()).is_err(),
            "accepted duplicate/unknown field"
        );
    }
}

#[test]
fn application_name_for_user_agent_old_boot_defaults_stay_accepted() {
    for schema in [1, 2, 3, 4] {
        assert!(
            parse_boot_bytes(&serde_json::to_vec(&descriptor(schema)).expect("old descriptor"))
                .is_ok()
        );
    }
}

//! Installed-only boot-6 selection. Decoding data grants no resource authority.
//! The current-process bundle and retained graph owners perform admission.

use serde::{Deserialize, Deserializer};

use super::{
    AuthConfiguration, HostAppError, InitialWindow, PERMISSIONS_FILE, ParsedBoot,
    PermissionsDocument, boot_error, decode_digest, deserialize_object, parse_development_shape,
    probe_boot_bytes, validate_relative_path,
};
use crate::app_session::graph_contract::{GRAPH_FILE, SelectedRendererSource};
use crate::hello::SingleInstanceDeclaration;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InstalledDocument {
    schema: u8,
    name: String,
    entry: String,
    #[serde(deserialize_with = "deserialize_object")]
    permissions: PermissionsDocument,
    #[serde(deserialize_with = "deserialize_object")]
    graph: PermissionsDocument,
    #[serde(rename = "rendererSource", deserialize_with = "deserialize_object")]
    renderer_source: RendererSourceDocument,
    #[serde(deserialize_with = "deserialize_object")]
    preload: PreloadDocument,
    #[serde(
        default,
        rename = "singleInstance",
        deserialize_with = "deserialize_optional_single_instance"
    )]
    single_instance: Option<SingleInstanceDeclaration>,
    #[serde(
        default,
        rename = "applicationNameForUserAgent",
        deserialize_with = "deserialize_optional_application_name_for_user_agent"
    )]
    application_name_for_user_agent: Option<keld_wv::ApplicationNameForUserAgent>,
}

fn deserialize_optional_single_instance<'de, D>(
    deserializer: D,
) -> Result<Option<SingleInstanceDeclaration>, D::Error>
where
    D: Deserializer<'de>,
{
    // Only omission selects None. Reject null and struct-as-array decoding;
    // the shared declaration owns the closed ID field and canonical grammar.
    deserialize_object(deserializer).map(Some)
}

fn deserialize_optional_application_name_for_user_agent<'de, D>(
    deserializer: D,
) -> Result<Option<keld_wv::ApplicationNameForUserAgent>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    keld_wv::ApplicationNameForUserAgent::new(value)
        .map(Some)
        .map_err(serde::de::Error::custom)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RendererSourceDocument {
    root: String,
    entry: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PreloadDocument {
    path: String,
    content_sha256: String,
}

/// Closed, syntax-checked selection; only the retained graph validator mints
/// access to its selected resource tree or preload bytes.
#[derive(Debug)]
pub(in crate::app_session) struct InstalledGraphBoot {
    pub(in crate::app_session) graph_sha256: [u8; 32],
    pub(in crate::app_session) source: SelectedRendererSource,
    pub(in crate::app_session) single_instance: Option<SingleInstanceDeclaration>,
    pub(in crate::app_session) application_name_for_user_agent:
        Option<keld_wv::ApplicationNameForUserAgent>,
}

pub(in crate::app_session) fn parse_installed_boot_bytes(
    bytes: &[u8],
) -> Result<ParsedBoot, HostAppError> {
    let (text, probe) = probe_boot_bytes(bytes)?;
    if probe.schema != 6 {
        return parse_development_shape(text, probe);
    }
    let document: InstalledDocument = serde_json::from_str(text).map_err(|error| {
        boot_error(
            format!("closed installed graph boot was rejected: {error}"),
            "Use the exact installed boot-6 compiler; do not add renderer, auth, profile, duplicate or unknown fields.",
        )
    })?;
    if document.schema != 6
        || document.name.is_empty()
        || document.name.contains('\0')
        || document.permissions.file != PERMISSIONS_FILE
        || document.graph.file != GRAPH_FILE
    {
        return Err(boot_error(
            "installed graph boot requires schema 6, a nonempty name and the fixed graph/permissions filenames",
            "Rebuild the signed application with the supported installed graph compiler.",
        ));
    }
    let source = SelectedRendererSource::new(
        &document.renderer_source.root,
        &document.renderer_source.entry,
        &document.preload.path,
        decode_digest(&document.preload.content_sha256)?,
    )?;
    super::super::graph_contract::validate_graph_relative_path(&document.entry)?;
    Ok(ParsedBoot {
        schema: 6,
        name: document.name,
        entry: validate_relative_path("entry", &document.entry)?,
        initial: InitialWindow::None,
        permissions_digest: decode_digest(&document.permissions.content_sha256)?,
        auth_configuration: AuthConfiguration::default(),
        installed_graph: Some(InstalledGraphBoot {
            graph_sha256: decode_digest(&document.graph.content_sha256)?,
            source,
            single_instance: document.single_instance,
            application_name_for_user_agent: document.application_name_for_user_agent,
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::parse_installed_boot_bytes;
    use crate::app_session::boot_contract::{InitialWindow, parse_boot_bytes};
    use std::path::Path;

    fn document() -> serde_json::Value {
        // Literal independent application values: no production compiler oracle.
        serde_json::json!({
            "schema": 6, "name": "Other Editor", "entry": "main/entry.mjs",
            "permissions": {"file": "keld.permissions.jsonc", "content_sha256": format!("sha256:{}", "11".repeat(32))},
            "graph": {"file": "keld.stage-graph.json", "content_sha256": format!("sha256:{}", "22".repeat(32))},
            "rendererSource": {"root": "client/assets", "entry": "nested/editor.html"},
            "preload": {"path": "runtime/preload.cjs", "content_sha256": format!("sha256:{}", "33".repeat(32))}
        })
    }

    #[test]
    fn installed_graph_boot_accepts_one_closed_shape_but_development_refuses_it() {
        let bytes = document().to_string().into_bytes();
        assert!(
            parse_boot_bytes(&bytes).is_err(),
            "developer parser must not mint installed selection"
        );
        let parsed = parse_installed_boot_bytes(&bytes).expect("closed installed data");
        assert_eq!(parsed.schema, 6);
        assert_eq!(parsed.name, "Other Editor");
        assert_eq!(parsed.entry, Path::new("main/entry.mjs"));
        assert!(matches!(parsed.initial, InitialWindow::None));
        assert_eq!(parsed.permissions_digest, [0x11; 32]);
        let graph = parsed
            .installed_graph
            .expect("separate installed-only graph data");
        assert_eq!(graph.graph_sha256, [0x22; 32]);
        assert_eq!(graph.source.root(), Path::new("client/assets"));
        assert_eq!(graph.source.entry(), Path::new("nested/editor.html"));
        assert_eq!(
            graph.source.preload_path(),
            Path::new("runtime/preload.cjs")
        );
        assert_eq!(graph.source.preload_sha256(), [0x33; 32]);
        assert!(graph.single_instance.is_none());
        assert!(graph.application_name_for_user_agent.is_none());
    }

    #[test]
    fn installed_graph_boot_accepts_optional_native_runtime_declarations() {
        let mut with_runtime = document();
        with_runtime["singleInstance"] = serde_json::json!({"id":"dev.example.editor"});
        with_runtime["applicationNameForUserAgent"] = serde_json::json!("OtherEditor/1 Native");
        let parsed = parse_installed_boot_bytes(with_runtime.to_string().as_bytes())
            .expect("closed runtime declarations");
        let graph = parsed.installed_graph.expect("installed graph");
        assert_eq!(
            graph.single_instance.expect("declared lock").id(),
            "dev.example.editor"
        );
        assert_eq!(
            graph
                .application_name_for_user_agent
                .expect("declared native metadata")
                .as_str(),
            "OtherEditor/1 Native"
        );
    }

    #[test]
    fn installed_runtime_declarations_reject_null_arrays_unknowns_and_bad_values() {
        for (field, invalid) in [
            ("singleInstance", serde_json::Value::Null),
            ("singleInstance", serde_json::json!([])),
            ("singleInstance", serde_json::json!(["dev.example.editor"])),
            ("singleInstance", serde_json::json!({})),
            (
                "singleInstance",
                serde_json::json!({"id":"dev.example.editor","extra":true}),
            ),
            ("singleInstance", serde_json::json!({"id":"../outside"})),
            (
                "singleInstance",
                serde_json::json!({"id":"Dev.Example.Editor"}),
            ),
            ("singleInstance", serde_json::json!({"id":"a".repeat(129)})),
            ("applicationNameForUserAgent", serde_json::Value::Null),
            ("applicationNameForUserAgent", serde_json::json!([])),
            ("applicationNameForUserAgent", serde_json::json!("")),
            ("applicationNameForUserAgent", serde_json::json!(" Leading")),
            (
                "applicationNameForUserAgent",
                serde_json::json!("Trailing "),
            ),
            (
                "applicationNameForUserAgent",
                serde_json::json!("Line\nBreak"),
            ),
            ("applicationNameForUserAgent", serde_json::json!("é")),
            (
                "applicationNameForUserAgent",
                serde_json::json!("a".repeat(257)),
            ),
        ] {
            let mut changed = document();
            changed[field] = invalid;
            assert!(
                parse_installed_boot_bytes(changed.to_string().as_bytes()).is_err(),
                "accepted {changed}"
            );
        }
        for (field, valid) in [
            ("singleInstance", serde_json::json!({"id":"a".repeat(128)})),
            (
                "applicationNameForUserAgent",
                serde_json::json!("a".repeat(256)),
            ),
        ] {
            let mut changed = document();
            changed[field] = valid;
            assert!(parse_installed_boot_bytes(changed.to_string().as_bytes()).is_ok());
        }
    }

    #[test]
    fn installed_runtime_declarations_refuse_duplicate_keys_without_value_roundtrip() {
        let base = document().to_string();
        for added in [
            r#""singleInstance":{"id":"dev.example.editor","id":"dev.example.editor"},"#,
            r#""singleInstance":{"id":"dev.example.editor"},"singleInstance":{"id":"dev.example.editor"},"#,
            r#""applicationNameForUserAgent":"A","applicationNameForUserAgent":"A","#,
        ] {
            let malformed = format!("{{{added}{}", &base[1..]);
            assert!(
                parse_installed_boot_bytes(malformed.as_bytes()).is_err(),
                "{malformed}"
            );
        }
    }

    #[test]
    fn installed_graph_boot_refuses_every_missing_unknown_or_mixed_authority_field() {
        for key in [
            "schema",
            "name",
            "entry",
            "permissions",
            "graph",
            "rendererSource",
            "preload",
        ] {
            let mut changed = document();
            changed.as_object_mut().expect("object").remove(key);
            assert!(
                parse_installed_boot_bytes(changed.to_string().as_bytes()).is_err(),
                "missing {key}"
            );
        }
        for key in ["renderer", "auth", "profile", "grant", "graphSource"] {
            let mut changed = document();
            changed[key] = serde_json::json!({});
            assert!(
                parse_installed_boot_bytes(changed.to_string().as_bytes()).is_err(),
                "extra {key}"
            );
        }
        for key in ["permissions", "graph", "rendererSource", "preload"] {
            for value in [
                serde_json::Value::Null,
                serde_json::json!([]),
                serde_json::json!({"unknown": true}),
            ] {
                let mut changed = document();
                changed[key] = value;
                assert!(
                    parse_installed_boot_bytes(changed.to_string().as_bytes()).is_err(),
                    "nested {key}"
                );
            }
        }
    }

    #[test]
    fn installed_graph_boot_rejects_duplicate_keys_trailing_data_escapes_and_bad_digests() {
        let bytes = document().to_string();
        for malformed in [
            bytes.replacen("\"schema\":6", "\"schema\":6,\"schema\":6", 1),
            bytes.replacen(
                "\"root\":\"client/assets\"",
                "\"root\":\"client/assets\",\"root\":\"other\"",
                1,
            ),
            bytes.replacen(
                "\"file\":\"keld.stage-graph.json\"",
                "\"file\":\"other.json\"",
                1,
            ),
            bytes.replacen("client/assets", "../escape", 1),
            bytes.replacen("nested/editor.html", "../escape.html", 1),
            bytes.replacen("runtime/preload.cjs", "/external/preload.cjs", 1),
            bytes.replacen("sha256:", "sha512:", 1),
            bytes.replacen(&"22".repeat(32), &"FF".repeat(32), 1),
            format!("{bytes} {{}}"),
        ] {
            assert_ne!(
                malformed, bytes,
                "negative control must change the document"
            );
            assert!(parse_installed_boot_bytes(malformed.as_bytes()).is_err());
        }
        assert!(parse_installed_boot_bytes(&vec![b' '; 64 * 1024 + 1]).is_err());
    }

    #[test]
    fn installed_graph_parser_preserves_existing_closed_facade_boot_bytes() {
        for schema in [2, 4] {
            let mut ordinary = document();
            ordinary["schema"] = serde_json::json!(schema);
            for key in ["graph", "rendererSource", "preload"] {
                ordinary.as_object_mut().expect("object").remove(key);
            }
            let bytes = ordinary.to_string().into_bytes();
            let old = parse_boot_bytes(&bytes).expect("existing facade boot");
            let parsed =
                parse_installed_boot_bytes(&bytes).expect("preserved installed facade boot");
            assert_eq!(parsed.name, old.name);
            assert_eq!(parsed.entry, old.entry);
            assert_eq!(parsed.permissions_digest, old.permissions_digest);
            assert!(parsed.installed_graph.is_none());
            for (field, value) in [
                (
                    "singleInstance",
                    serde_json::json!({"id":"dev.example.editor"}),
                ),
                ("applicationNameForUserAgent", serde_json::json!("Editor/1")),
            ] {
                let mut mixed = ordinary.clone();
                mixed[field] = value;
                assert!(parse_installed_boot_bytes(mixed.to_string().as_bytes()).is_err());
            }
        }
    }
}

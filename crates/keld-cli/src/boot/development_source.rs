//! Closed authoring for a selected Windows development source.

use std::collections::BTreeMap;

use keld_core::{LiteralConfigValue, literal_config_from_ts};

use super::BootCompileError;

/// Application data captured from the selected development authoring source.
#[derive(Debug)]
pub(super) struct DevelopmentAppConfig {
    pub(super) name: String,
    pub(super) version: String,
    pub(super) entry: String,
    pub(super) renderer_source: RendererSourceConfig,
    pub(super) single_instance: Option<keld_core::SingleInstanceDeclaration>,
    pub(super) application_name_for_user_agent: Option<keld_core::ApplicationNameForUserAgent>,
}

/// Application-selected paths, never a source or native permission grant.
#[derive(Debug)]
pub(super) struct RendererSourceConfig {
    pub(super) root: String,
    pub(super) entry: String,
}

/// Data-only expected provider and original preload identities.
/// A KELD-qualified original release still needs separate source receipts.
#[derive(Debug)]
pub(super) struct GraphSourceConfig {
    pub(super) package_sha256: [u8; 32],
    pub(super) lock_sha256: [u8; 32],
    /// Canonical GH499 order: electron, @keld/api, @keld/kipc.
    pub(super) provider_trees: [[u8; 32]; 3],
    pub(super) preload: PreloadPin,
}

/// An exact app-relative preload path and build-selected expected digest.
#[derive(Debug)]
pub(super) struct PreloadPin {
    pub(super) path: String,
    pub(super) sha256: [u8; 32],
}

type Fields = BTreeMap<String, LiteralConfigValue>;

fn fields(source: &str, phase: &'static str) -> Result<Fields, BootCompileError> {
    literal_config_from_ts(source).map_err(|error| BootCompileError::new(phase, error.to_string()))
}

fn required_text(
    source: &mut Fields,
    field: &'static str,
    phase: &'static str,
) -> Result<String, BootCompileError> {
    match source.remove(field) {
        Some(LiteralConfigValue::Text(value)) if !value.is_empty() => Ok(value),
        _ => Err(BootCompileError::new(
            phase,
            format!("required '{field}' must be a nonempty literal string"),
        )),
    }
}

fn optional_text(
    source: &mut Fields,
    field: &'static str,
    phase: &'static str,
) -> Result<Option<String>, BootCompileError> {
    match source.remove(field) {
        Some(LiteralConfigValue::Text(value)) if !value.is_empty() => Ok(Some(value)),
        None => Ok(None),
        _ => Err(BootCompileError::new(
            phase,
            format!("optional '{field}' must be a nonempty literal string"),
        )),
    }
}

fn closed(source: &Fields, phase: &'static str) -> Result<(), BootCompileError> {
    if let Some(unknown) = source.keys().next() {
        return Err(BootCompileError::new(
            phase,
            format!("unknown field '{unknown}' in closed authoring object"),
        ));
    }
    Ok(())
}

fn required_object(
    source: &mut Fields,
    field: &'static str,
    phase: &'static str,
) -> Result<Fields, BootCompileError> {
    match source.remove(field) {
        Some(LiteralConfigValue::Object(object)) => Ok(object),
        _ => Err(BootCompileError::new(
            phase,
            format!("required '{field}' must be a closed literal object"),
        )),
    }
}

fn required_digest(
    source: &mut Fields,
    field: &'static str,
    phase: &'static str,
) -> Result<[u8; 32], BootCompileError> {
    let encoded = required_text(source, field, phase)?;
    parse_sha256(&encoded).ok_or_else(|| {
        BootCompileError::new(
            phase,
            format!("'{field}' must be exactly 64 lowercase hexadecimal characters"),
        )
    })
}

fn validate_graph_path(path: &str, phase: &'static str) -> Result<(), BootCompileError> {
    // Core is the only GH499 path grammar owner; no cross-platform clone.
    keld_core::app_session::validate_graph_relative_path(path)
        .map_err(|error| BootCompileError::new(phase, error.to_string()))
}

fn parse_renderer_source(source: &mut Fields) -> Result<RendererSourceConfig, BootCompileError> {
    let phase = "keld.config.ts rendererSource";
    let mut source = required_object(source, "rendererSource", phase)?;
    let root = required_text(&mut source, "root", phase)?;
    let entry = required_text(&mut source, "entry", phase)?;
    closed(&source, phase)?;
    validate_graph_path(&root, phase)?;
    validate_graph_path(&entry, phase)?;
    Ok(RendererSourceConfig { root, entry })
}

fn parse_graph_source(source: &mut Fields) -> Result<GraphSourceConfig, BootCompileError> {
    let phase = "keld.build.ts graphSource";
    let mut graph = required_object(source, "graphSource", phase)?;
    let package_sha256 = required_digest(&mut graph, "packageSha256", phase)?;
    let lock_sha256 = required_digest(&mut graph, "lockSha256", phase)?;
    let mut providers = required_object(&mut graph, "providerTrees", phase)?;
    let provider_trees = [
        required_digest(&mut providers, "electron", phase)?,
        required_digest(&mut providers, "api", phase)?,
        required_digest(&mut providers, "kipc", phase)?,
    ];
    closed(&providers, phase)?;
    let mut preload = required_object(&mut graph, "preload", phase)?;
    let path = required_text(&mut preload, "path", phase)?;
    validate_graph_path(&path, phase)?;
    let sha256 = required_digest(&mut preload, "sha256", phase)?;
    closed(&preload, phase)?;
    closed(&graph, phase)?;
    Ok(GraphSourceConfig {
        package_sha256,
        lock_sha256,
        provider_trees,
        preload: PreloadPin { path, sha256 },
    })
}

fn read_app_version(root: &mut Fields, phase: &'static str) -> Result<String, BootCompileError> {
    let version = optional_text(root, "version", phase)?.unwrap_or_default();
    if version.len() > 4096 || version.contains('\0') {
        return Err(BootCompileError::new(
            phase,
            "declared version must be NUL-free and at most 4096 UTF-8 bytes",
        ));
    }
    Ok(version)
}

fn parse_sha256(text: &str) -> Option<[u8; 32]> {
    let bytes = text.as_bytes();
    if bytes.len() != 64
        || !bytes
            .iter()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
    {
        return None;
    }
    let mut digest = [0_u8; 32];
    for (index, pair) in bytes.as_chunks::<2>().0.iter().enumerate() {
        let high = (pair[0] as char).to_digit(16)?;
        let low = (pair[1] as char).to_digit(16)?;
        digest[index] = u8::try_from(high * 16 + low).ok()?;
    }
    Some(digest)
}

/// Captures explicit Legacy facade selectors and expected graph digests.
pub(super) fn parse_development_graph(
    app_source: &str,
    build_source: &str,
) -> Result<(DevelopmentAppConfig, GraphSourceConfig), BootCompileError> {
    let phase = "development keld.config.ts";
    let mut app = fields(app_source, phase)?;
    let name = required_text(&mut app, "name", phase)?;
    let id = required_text(&mut app, "id", phase)?;
    keld_core::validate_canonical_app_id(&id)
        .map_err(|error| BootCompileError::new(phase, error.to_string()))?;
    if name.len() > 255 || name.chars().any(char::is_control) {
        return Err(BootCompileError::new(
            phase,
            "name exceeds 255 bytes or contains control characters",
        ));
    }
    let version = read_app_version(&mut app, phase)?;
    let entry = optional_text(&mut app, "entry", phase)?
        .unwrap_or_else(|| keld_core::DEFAULT_ENTRY.to_owned());
    app.remove("initialWindow");
    if !keld_core::initial_window_from_config_ts(app_source)
        .map_err(|error| BootCompileError::new(phase, error.to_string()))?
    {
        return Err(BootCompileError::new(
            phase,
            "rendererSource requires initialWindow: 'none'",
        ));
    }
    let renderer_source = parse_renderer_source(&mut app)?;
    let single_instance = keld_core::single_instance_from_config_ts(app_source)
        .map_err(|error| BootCompileError::new(phase, error.to_string()))?;
    if single_instance
        .as_ref()
        .is_some_and(|declaration| declaration.id() != id)
    {
        return Err(BootCompileError::new(
            phase,
            "singleInstance.id must equal the canonical application id",
        ));
    }
    let application_name_for_user_agent =
        keld_core::application_name_for_user_agent_from_config_ts(app_source)
            .map_err(|error| BootCompileError::new(phase, error.to_string()))?;
    app.remove("singleInstance");
    app.remove("applicationNameForUserAgent");
    closed(&app, phase)?;

    let phase = "development keld.build.ts";
    let mut build = fields(build_source, phase)?;
    let graph = parse_graph_source(&mut build)?;
    closed(&build, phase)?;
    Ok((
        DevelopmentAppConfig {
            name,
            version,
            entry,
            renderer_source,
            single_instance,
            application_name_for_user_agent,
        },
        graph,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const APP: &str = "export default {
        id:'dev.example.editor', name:'Editor', initialWindow:'none',
        rendererSource:{root:'client/published', entry:'pages/editor.html'}
    } as const;";

    fn build() -> String {
        format!(
            "export default {{graphSource:{{
                packageSha256:'{}',lockSha256:'{}',
                providerTrees:{{kipc:'{}',api:'{}',electron:'{}'}},
                preload:{{path:'bootstrap/app-preload.cjs',sha256:'{}'}}
            }}}} as const;",
            "aa".repeat(32),
            "bb".repeat(32),
            "ee".repeat(32),
            "dd".repeat(32),
            "cc".repeat(32),
            "ff".repeat(32),
        )
    }

    #[test]
    fn returns_canonical_selectors_and_ordered_expected_hashes() {
        let (app, graph) = parse_development_graph(APP, &build()).expect("approved authoring");
        assert_eq!(app.name, "Editor");
        assert_eq!(app.version, "");
        assert_eq!(app.entry, keld_core::DEFAULT_ENTRY);
        assert_eq!(app.renderer_source.root, "client/published");
        assert_eq!(app.renderer_source.entry, "pages/editor.html");
        assert_eq!(graph.package_sha256, [0xaa; 32]);
        assert_eq!(graph.lock_sha256, [0xbb; 32]);
        assert_eq!(graph.provider_trees, [[0xcc; 32], [0xdd; 32], [0xee; 32]]);
        assert_eq!(graph.preload.path, "bootstrap/app-preload.cjs");
        assert_eq!(graph.preload.sha256, [0xff; 32]);
    }

    #[test]
    fn preserves_optional_metadata_independently_and_keeps_exact_version() {
        let before = "rendererSource:";
        let single = "singleInstance:{id:'dev.example.editor'},";
        let suffix = "applicationNameForUserAgent:'Editor 31.7.0',";
        for (single_field, suffix_field, expected_id, expected_suffix) in [
            ("", "", None, None),
            (single, "", Some("dev.example.editor"), None),
            ("", suffix, None, Some("Editor 31.7.0")),
            (
                single,
                suffix,
                Some("dev.example.editor"),
                Some("Editor 31.7.0"),
            ),
        ] {
            let source = APP.replace(
                before,
                &format!("version:'31.7.0',{single_field}{suffix_field}{before}"),
            );
            let (app, _) = parse_development_graph(&source, &build()).expect("optional metadata");
            assert_eq!(app.version, "31.7.0");
            assert_eq!(
                app.single_instance.as_ref().map(|value| value.id()),
                expected_id
            );
            assert_eq!(
                app.application_name_for_user_agent
                    .as_ref()
                    .map(|value| value.as_str()),
                expected_suffix,
            );
        }
    }

    #[test]
    fn reports_typed_staging_errors_for_invalid_metadata() {
        for (field, expected) in [
            (
                "singleInstance:{id:'dev.elsewhere.editor'},",
                "singleInstance.id must equal",
            ),
            (
                "singleInstance:{id:'Dev.example.editor'},",
                "invalid singleInstance declaration",
            ),
            (
                "singleInstance:{id:'dev.example.editor',extra:'value'},",
                "invalid singleInstance declaration",
            ),
            (
                "applicationNameForUserAgent:' Editor',",
                "invalid applicationNameForUserAgent",
            ),
            (
                "applicationNameForUserAgent:{label:'Editor'},",
                "invalid applicationNameForUserAgent",
            ),
        ] {
            let source = APP.replace("rendererSource:", &format!("{field}rendererSource:"));
            let error = parse_development_graph(&source, &build()).expect_err(field);
            assert!(
                matches!(error, BootCompileError::Staging { phase: "development keld.config.ts", detail } if detail.contains(expected)),
                "wrong staging error for {field}",
            );
        }
    }

    #[test]
    fn applies_the_donor_version_contract() {
        let exact = "\u{00e9}".repeat(2048);
        let source = APP.replace(
            "name:",
            &format!("entry:'boot.ts', version:'{exact}', name:"),
        );
        let (app, _) = parse_development_graph(&source, &build()).expect("bounded version");
        assert_eq!(app.version, exact);
        assert_eq!(app.entry, "boot.ts");
        for bad in ["".to_owned(), r"a\u0000".to_owned(), exact + "x"] {
            let source = APP.replace("name:", &format!("version:'{bad}', name:"));
            assert!(parse_development_graph(&source, &build()).is_err());
        }
    }

    #[test]
    fn refuses_invalid_or_ambiguous_app_authoring() {
        for (old, replacement) in [
            ("initialWindow:'none',", ""),
            ("initialWindow:'none',", "initialWindow:'app',"),
            ("rendererSource:{", "renderer:'index.html',rendererSource:{"),
            ("rendererSource:{", "auth:{},rendererSource:{"),
            ("rendererSource:{", "rendererSource:{extra:'name',"),
            ("rendererSource:{", "rendererSource:{root:'extra',"),
            ("rendererSource:{", "rendererSource:other, unused:{"),
            ("client/published", "../client"),
            ("client/published", "client//published"),
            ("pages/editor.html", "/absolute.html"),
            ("dev.example.editor", "Dev.Example.Editor"),
        ] {
            let source = APP.replace(old, replacement);
            assert_ne!(source, APP);
            assert!(
                parse_development_graph(&source, &build()).is_err(),
                "accepted {source}"
            );
        }
        let no_selector =
            "export default { id:'dev.example.editor',name:'Editor',initialWindow:'none' };";
        assert!(parse_development_graph(no_selector, &build()).is_err());
    }

    #[test]
    fn refuses_invalid_or_ambiguous_graph_authoring() {
        let build = build();
        for (old, replacement) in [
            ("graphSource:", "signing:'developer-local',graphSource:"),
            ("graphSource:", "graph:"),
            ("packageSha256:", "packageHash:"),
            ("packageSha256:", "packageSha256:'f',packageSha256:"),
            ("providerTrees:{", "providerTrees:{extra:'1',"),
            ("preload:{", "preload:{algorithm:'sha256',"),
            (
                "preload:{path:",
                "preload:{path:process.env.PRELOAD,original:",
            ),
            ("bootstrap/app-preload.cjs", "bootstrap/../app-preload.cjs"),
            ("bootstrap/app-preload.cjs", r"bootstrap\\app-preload.cjs"),
        ] {
            let source = build.replace(old, replacement);
            assert_ne!(source, build);
            assert!(
                parse_development_graph(APP, &source).is_err(),
                "accepted {source}"
            );
        }
        for digest in ["ff".repeat(31), "FF".repeat(32), "ff".repeat(33)] {
            let source = build.replace(&"ff".repeat(32), &digest);
            assert!(
                parse_development_graph(APP, &source).is_err(),
                "accepted {source}"
            );
        }
    }
}

//! Closed, inert authoring data for the first native local macOS build.

use std::collections::BTreeMap;
use std::path::PathBuf;

use keld_core::{LiteralConfigValue, literal_config_from_ts};

use super::BuildError;

/// Values captured once from the application's shared authoring source.
#[derive(Debug)]
pub(super) struct AppConfig {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) version: String,
    pub(super) entry: String,
    pub(super) renderer: Option<String>,
    /// Optional closed source selection for one explicitly Legacy facade.
    #[cfg(target_os = "macos")]
    pub(super) renderer_source: Option<RendererSourceConfig>,
}

/// Application-selected paths, never a source or native permission grant.
#[cfg(target_os = "macos")]
#[derive(Debug)]
pub(super) struct RendererSourceConfig {
    pub(super) root: String,
    pub(super) entry: String,
}

/// Data-only expected provider and original preload identities.
/// A KELD-qualified original release still needs separate source receipts.
#[cfg(target_os = "macos")]
#[derive(Debug)]
pub(super) struct GraphSourceConfig {
    pub(super) package_sha256: [u8; 32],
    pub(super) lock_sha256: [u8; 32],
    /// Canonical GH499 order: electron, @keld/api, @keld/kipc.
    pub(super) provider_trees: [[u8; 32]; 3],
    pub(super) preload: PreloadPin,
}

/// An exact app-relative preload path and build-selected expected digest.
#[cfg(target_os = "macos")]
#[derive(Debug)]
pub(super) struct PreloadPin {
    pub(super) path: String,
    pub(super) sha256: [u8; 32],
}

/// Input qualification pin. Strings here cannot mint publisher authority.
#[derive(Debug)]
pub(super) struct ExecutablePin {
    pub(super) path: PathBuf,
    pub(super) sha256: [u8; 32],
    pub(super) version: String,
}

/// Exact local package selection. No paths are opened during parsing.
#[derive(Debug)]
pub(super) struct LocalBuildConfig {
    pub(super) bundle_version: String,
    pub(super) minimum_system_version: String,
    pub(super) icon: Option<String>,
    pub(super) resources: Vec<String>,
    pub(super) host: ExecutablePin,
    pub(super) bun: ExecutablePin,
    #[cfg(target_os = "macos")]
    pub(super) graph_source: Option<GraphSourceConfig>,
    pub(super) out_dir: PathBuf,
}

type Fields = BTreeMap<String, LiteralConfigValue>;

fn invalid(phase: &'static str, detail: impl Into<String>) -> BuildError {
    BuildError::input(phase, detail.into())
}

fn fields(source: &str, phase: &'static str) -> Result<Fields, BuildError> {
    literal_config_from_ts(source).map_err(|error| invalid(phase, error.to_string()))
}

fn required_text(
    source: &mut Fields,
    field: &'static str,
    phase: &'static str,
) -> Result<String, BuildError> {
    match source.remove(field) {
        Some(LiteralConfigValue::Text(value)) if !value.is_empty() => Ok(value),
        _ => Err(invalid(
            phase,
            format!("required '{field}' must be a nonempty literal string"),
        )),
    }
}

fn optional_text(
    source: &mut Fields,
    field: &'static str,
    phase: &'static str,
) -> Result<Option<String>, BuildError> {
    match source.remove(field) {
        Some(LiteralConfigValue::Text(value)) if !value.is_empty() => Ok(Some(value)),
        None => Ok(None),
        _ => Err(invalid(
            phase,
            format!("optional '{field}' must be a nonempty literal string"),
        )),
    }
}

fn required_string_array(
    source: &mut Fields,
    field: &'static str,
    phase: &'static str,
) -> Result<Vec<String>, BuildError> {
    match source.remove(field) {
        Some(LiteralConfigValue::Strings(values)) => Ok(values),
        _ => Err(invalid(
            phase,
            format!("required '{field}' must be an array of literal strings"),
        )),
    }
}

fn closed(source: &Fields, phase: &'static str) -> Result<(), BuildError> {
    if let Some(unknown) = source.keys().next() {
        return Err(invalid(
            phase,
            format!("unknown field '{unknown}' in closed authoring object"),
        ));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn optional_object(
    source: &mut Fields,
    field: &'static str,
    phase: &'static str,
) -> Result<Option<Fields>, BuildError> {
    match source.remove(field) {
        None => Ok(None),
        Some(LiteralConfigValue::Object(object)) => Ok(Some(object)),
        _ => Err(invalid(
            phase,
            format!("optional '{field}' must be a closed literal object"),
        )),
    }
}

#[cfg(target_os = "macos")]
fn required_digest(
    source: &mut Fields,
    field: &'static str,
    phase: &'static str,
) -> Result<[u8; 32], BuildError> {
    let encoded = required_text(source, field, phase)?;
    parse_sha256(&encoded).ok_or_else(|| {
        invalid(
            phase,
            format!("'{field}' must be exactly 64 lowercase hexadecimal characters"),
        )
    })
}

#[cfg(target_os = "macos")]
fn validate_graph_path(path: &str, phase: &'static str) -> Result<(), BuildError> {
    // Core is the only GH499 path grammar owner; no cross-platform clone.
    keld_core::app_session::validate_graph_relative_path(path)
        .map_err(|error| invalid(phase, error.to_string()))
}

#[cfg(target_os = "macos")]
fn parse_renderer_source(source: &mut Fields) -> Result<Option<RendererSourceConfig>, BuildError> {
    let phase = "keld.config.ts rendererSource";
    let Some(mut source) = optional_object(source, "rendererSource", phase)? else {
        return Ok(None);
    };
    let root = required_text(&mut source, "root", phase)?;
    let entry = required_text(&mut source, "entry", phase)?;
    closed(&source, phase)?;
    validate_graph_path(&root, phase)?;
    validate_graph_path(&entry, phase)?;
    Ok(Some(RendererSourceConfig { root, entry }))
}

#[cfg(target_os = "macos")]
fn parse_graph_source(source: &mut Fields) -> Result<Option<GraphSourceConfig>, BuildError> {
    let phase = "keld.build.ts graphSource";
    let Some(mut graph) = optional_object(source, "graphSource", phase)? else {
        return Ok(None);
    };
    let package_sha256 = required_digest(&mut graph, "packageSha256", phase)?;
    let lock_sha256 = required_digest(&mut graph, "lockSha256", phase)?;
    let mut providers = optional_object(&mut graph, "providerTrees", phase)?.ok_or_else(|| {
        invalid(
            phase,
            "required 'providerTrees' must be a closed literal object",
        )
    })?;
    let provider_trees = [
        required_digest(&mut providers, "electron", phase)?,
        required_digest(&mut providers, "api", phase)?,
        required_digest(&mut providers, "kipc", phase)?,
    ];
    closed(&providers, phase)?;
    let mut preload = optional_object(&mut graph, "preload", phase)?
        .ok_or_else(|| invalid(phase, "required 'preload' must be a closed literal object"))?;
    let path = required_text(&mut preload, "path", phase)?;
    validate_graph_path(&path, phase)?;
    let sha256 = required_digest(&mut preload, "sha256", phase)?;
    closed(&preload, phase)?;
    closed(&graph, phase)?;
    Ok(Some(GraphSourceConfig {
        package_sha256,
        lock_sha256,
        provider_trees,
        preload: PreloadPin { path, sha256 },
    }))
}

/// Reads only supported application root fields, without evaluating TypeScript.
pub(super) fn parse_app(source: &str) -> Result<AppConfig, BuildError> {
    let phase = "keld.config.ts";
    let mut root = fields(source, phase)?;
    let name = required_text(&mut root, "name", phase)?;
    let id = required_text(&mut root, "id", phase)?;
    keld_core::validate_canonical_app_id(&id).map_err(|error| invalid(phase, error.to_string()))?;
    if name.len() > 255 || name.chars().any(char::is_control) {
        return Err(invalid(
            phase,
            "name exceeds 255 bytes or contains control characters",
        ));
    }
    let version = required_text(&mut root, "version", phase)?;
    let entry = optional_text(&mut root, "entry", phase)?
        .unwrap_or_else(|| keld_core::DEFAULT_ENTRY.to_owned());
    let renderer = optional_text(&mut root, "renderer", phase)?;
    let initial_window = optional_text(&mut root, "initialWindow", phase)?;
    #[cfg(target_os = "macos")]
    let renderer_source = parse_renderer_source(&mut root)?;
    #[cfg(target_os = "macos")]
    {
        let single_instance = keld_core::single_instance_from_config_ts(source)
            .map_err(|error| invalid(phase, error.to_string()))?;
        let application_name_for_user_agent =
            keld_core::application_name_for_user_agent_from_config_ts(source)
                .map_err(|error| invalid(phase, error.to_string()))?;
        if renderer_source.is_none()
            && (single_instance.is_some() || application_name_for_user_agent.is_some())
        {
            return Err(invalid(
                phase,
                "singleInstance and applicationNameForUserAgent require the installed rendererSource graph",
            ));
        }
        if single_instance
            .as_ref()
            .is_some_and(|declaration| declaration.id() != id.as_str())
        {
            return Err(invalid(
                phase,
                "singleInstance.id must equal the canonical application id",
            ));
        }
        // These optional values have one shared reader and are serialized by
        // the boot-6 compiler from this same captured authoring source.
        root.remove("singleInstance");
        root.remove("applicationNameForUserAgent");
    }
    let auth = root.remove("auth");
    if auth
        .as_ref()
        .is_some_and(|value| !matches!(value, LiteralConfigValue::Object(_)))
    {
        return Err(invalid(phase, "auth must be a literal object"));
    }
    closed(&root, phase)?;
    let facade = keld_core::initial_window_from_config_ts(source)
        .map_err(|error| invalid(phase, error.to_string()))?;
    if initial_window
        .as_deref()
        .is_some_and(|value| value != "none")
    {
        return Err(invalid(
            phase,
            "initialWindow supports only the literal 'none'",
        ));
    }
    if facade && (renderer.is_some() || auth.is_some()) {
        return Err(invalid(
            phase,
            "initialWindow: 'none' forbids renderer and auth",
        ));
    }
    #[cfg(target_os = "macos")]
    if renderer_source.is_some() && !facade {
        return Err(invalid(
            phase,
            "rendererSource requires initialWindow: 'none' and an explicitly Legacy facade",
        ));
    }
    Ok(AppConfig {
        id,
        name,
        version,
        entry,
        renderer: (!facade)
            .then(|| renderer.unwrap_or_else(|| keld_core::DEFAULT_RENDERER.to_owned())),
        #[cfg(target_os = "macos")]
        renderer_source,
    })
}

fn parse_executable_pin(
    source: &mut Fields,
    field: &'static str,
) -> Result<ExecutablePin, BuildError> {
    let phase = "keld.build.ts";
    let Some(LiteralConfigValue::Object(mut object)) = source.remove(field) else {
        return Err(invalid(
            phase,
            format!("'{field}' must be an object with path, sha256 and version"),
        ));
    };
    let path = required_text(&mut object, "path", phase)?;
    if path.chars().any(char::is_control) {
        return Err(invalid(
            phase,
            format!("{field}.path contains control characters"),
        ));
    }
    let hash = required_text(&mut object, "sha256", phase)?;
    let sha256 = parse_sha256(&hash).ok_or_else(|| {
        invalid(
            phase,
            format!("{field}.sha256 must be 64 lowercase hexadecimal characters"),
        )
    })?;
    let version = required_text(&mut object, "version", phase)?;
    if version.len() > 128 || version.chars().any(char::is_control) {
        return Err(invalid(
            phase,
            format!("{field}.version exceeds 128 bytes or contains control characters"),
        ));
    }
    closed(&object, phase)?;
    Ok(ExecutablePin {
        path: PathBuf::from(path),
        sha256,
        version,
    })
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

/// Parses the supported local target and signing without interpreting executable
/// expressions or acquiring code. Platform availability is checked by the caller.
pub(super) fn parse_build(source: &str) -> Result<LocalBuildConfig, BuildError> {
    let phase = "keld.build.ts";
    let mut root = fields(source, phase)?;
    if required_text(&mut root, "target", phase)? != "macos-arm64" {
        return Err(invalid(phase, "target must be exactly 'macos-arm64'"));
    }
    if required_text(&mut root, "signing", phase)? != "adhoc-local" {
        return Err(invalid(phase, "signing must be exactly 'adhoc-local'"));
    }
    let bundle_version = required_text(&mut root, "bundleVersion", phase)?;
    let minimum_system_version = required_text(&mut root, "minimumSystemVersion", phase)?;
    let icon = optional_text(&mut root, "icon", phase)?;
    let resources = required_string_array(&mut root, "resources", phase)?;
    let host = parse_executable_pin(&mut root, "host")?;
    let bun = parse_executable_pin(&mut root, "bun")?;
    #[cfg(target_os = "macos")]
    let graph_source = parse_graph_source(&mut root)?;
    let out_dir = PathBuf::from(required_text(&mut root, "outDir", phase)?);
    closed(&root, phase)?;
    Ok(LocalBuildConfig {
        bundle_version,
        minimum_system_version,
        icon,
        resources,
        host,
        bun,
        #[cfg(target_os = "macos")]
        graph_source,
        out_dir,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const APP: &str = "export default {name:'Alpha', id:'dev.example.alpha', version:'1.2.3', entry:'src/main.ts', renderer:'index.html'} as const;";
    #[cfg(target_os = "macos")]
    const GRAPH_APP: &str = "export default {name:'Alpha', id:'dev.example.alpha', version:'1.2.3', entry:'src/main.ts', initialWindow:'none', rendererSource:{root:'client/published',entry:'pages/editor.html'}} as const;";

    fn build() -> String {
        format!(
            "export default {{target:'macos-arm64',signing:'adhoc-local',bundleVersion:'4',minimumSystemVersion:'14.0',resources:[],host:{{path:'host',sha256:'{}',version:'1'}},bun:{{path:'bun',sha256:'{}',version:'2'}},outDir:'output'}} as const;",
            "a".repeat(64),
            "0".repeat(64)
        )
    }

    #[cfg(target_os = "macos")]
    fn graph_build() -> String {
        let source = format!(
            "graphSource:{{packageSha256:'{}',lockSha256:'{}',providerTrees:{{electron:'{}',api:'{}',kipc:'{}'}},preload:{{path:'bootstrap/app-preload.cjs',sha256:'{}'}}}},",
            "a".repeat(64),
            "b".repeat(64),
            "c".repeat(64),
            "d".repeat(64),
            "e".repeat(64),
            "f".repeat(64),
        );
        build().replace("outDir:'output'", &(source + "outDir:'output'"))
    }

    #[test]
    fn closed_app_authoring_uses_canonical_id_and_existing_renderer_defaults() {
        let app = parse_app(APP).expect("valid static app");
        assert_eq!(app.id, "dev.example.alpha");
        assert_eq!(app.name, "Alpha");
        assert_eq!(app.version, "1.2.3");
        assert_eq!(app.entry, "src/main.ts");
        assert_eq!(app.renderer.as_deref(), Some("index.html"));
        let facade = parse_app("export default {name:'Alpha',id:'dev.example.alpha',version:'1.2.3',initialWindow:'none'} as const;").expect("facade");
        assert_eq!(facade.renderer, None);
    }

    #[test]
    fn closed_app_rejects_aliases_duplicate_identity_and_dynamic_values() {
        for source in [
            APP.replace("id:'dev.example.alpha'", "app:{id:'dev.example.alpha'}"),
            APP.replace(
                "id:'dev.example.alpha'",
                "id:'dev.example.alpha',id:'dev.example.alpha'",
            ),
            APP.replace("id:'dev.example.alpha'", "id:'Dev.Example.Alpha'"),
            APP.replace("name:'Alpha'", "name: process.env.NAME"),
            APP.replace(
                "renderer:'index.html'",
                "initialWindow:'none',renderer:'index.html'",
            ),
            APP.replace("version:'1.2.3'", "version:'1.2.3',grants:'all'"),
        ] {
            assert!(parse_app(&source).is_err(), "accepted {source}");
        }
    }

    #[test]
    fn build_fields_capture_exact_digest_and_refuse_bad_modes() {
        let parsed = parse_build(&build()).expect("valid build");
        assert_eq!(parsed.host.sha256, [0xaa; 32]);
        assert_eq!(parsed.bun.sha256, [0; 32]);
        assert_eq!(parsed.host.path, PathBuf::from("host"));
        assert_eq!(parsed.host.version, "1");
        assert_eq!(parsed.bun.path, PathBuf::from("bun"));
        assert_eq!(parsed.bun.version, "2");
        assert_eq!(parsed.bundle_version, "4");
        assert_eq!(parsed.minimum_system_version, "14.0");
        assert_eq!(parsed.out_dir, PathBuf::from("output"));
        assert_eq!(parsed.icon, None);
        assert_eq!(parsed.resources.len(), 0);
        for source in [
            build().replace("adhoc-local", "developer-id"),
            build().replace("macos-arm64", "windows-x64"),
            build().replace(&"a".repeat(64), &"A".repeat(64)),
            build().replace("resources:[]", "resources:['a',42]"),
            build().replace("outDir:'output'", "outDir:'output',outDir:'another'"),
            build().replace("host:{path:", "host:{unexpected:'yes',path:"),
            build().replace("bun:{path:", "bun:{path:process.env.BUN,ignored:"),
        ] {
            assert!(parse_build(&source).is_err(), "accepted {source}");
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn installed_graph_runtime_declarations_are_optional_and_app_bound() {
        for declarations in [
            "singleInstance:{id:'dev.example.alpha'},".to_owned(),
            "applicationNameForUserAgent:'Example/1 Editor',".to_owned(),
            "singleInstance:{id:'dev.example.alpha'},applicationNameForUserAgent:'Example/1',"
                .to_owned(),
            format!("applicationNameForUserAgent:'{}',", "a".repeat(256)),
        ] {
            let source = GRAPH_APP.replace("rendererSource:", &(declarations + "rendererSource:"));
            let app = parse_app(&source).expect("closed installed runtime declarations");
            assert_eq!(app.id, "dev.example.alpha");
            assert!(app.renderer_source.is_some());
        }
        assert!(
            parse_app(GRAPH_APP).is_ok(),
            "omission retains existing graph config"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn installed_graph_runtime_declarations_refuse_loss_or_identity_substitution() {
        for declaration in [
            "singleInstance:{id:'dev.example.other'}".to_owned(),
            "singleInstance:{id:'Dev.Example.Alpha'}".to_owned(),
            "singleInstance:{}".to_owned(),
            "singleInstance:{id:'dev.example.alpha',extra:'x'}".to_owned(),
            "singleInstance:{id:'dev.example.alpha',id:'dev.example.alpha'}".to_owned(),
            "singleInstance:null".to_owned(),
            "singleInstance:[]".to_owned(),
            "singleInstance:{id:process.env.APP_ID}".to_owned(),
            "applicationNameForUserAgent:''".to_owned(),
            "applicationNameForUserAgent:' Leading'".to_owned(),
            "applicationNameForUserAgent:'Trailing '".to_owned(),
            "applicationNameForUserAgent:'A\\nB'".to_owned(),
            "applicationNameForUserAgent:'é'".to_owned(),
            "applicationNameForUserAgent:null".to_owned(),
            "applicationNameForUserAgent:'A',applicationNameForUserAgent:'A'".to_owned(),
            format!("applicationNameForUserAgent:'{}'", "a".repeat(257)),
        ] {
            let source =
                GRAPH_APP.replace("rendererSource:", &format!("{declaration},rendererSource:"));
            assert!(parse_app(&source).is_err(), "accepted {source}");
        }
        for declaration in [
            "singleInstance:{id:'dev.example.alpha'},",
            "applicationNameForUserAgent:'Example/1',",
        ] {
            let renderer = APP.replace("renderer:", &(declaration.to_owned() + "renderer:"));
            assert!(
                parse_app(&renderer).is_err(),
                "non-graph renderer accepted {renderer}"
            );
            let facade = format!(
                "export default {{name:'Alpha',id:'dev.example.alpha',version:'1.2.3',initialWindow:'none',{declaration}}} as const;"
            );
            assert!(
                parse_app(&facade).is_err(),
                "non-graph facade accepted {facade}"
            );
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn closed_graph_source_captures_independent_expected_data_without_app_permissions() {
        let app = parse_app(GRAPH_APP).expect("closed facade source");
        assert_eq!(app.renderer, None);
        let selected = app.renderer_source.expect("optional source selection");
        assert_eq!(selected.root, "client/published");
        assert_eq!(selected.entry, "pages/editor.html");

        let build = parse_build(&graph_build()).expect("closed build-only source pins");
        let expected = build.graph_source.expect("explicit graphSource");
        assert_eq!(expected.package_sha256, [0xaa; 32]);
        assert_eq!(expected.lock_sha256, [0xbb; 32]);
        assert_eq!(
            expected.provider_trees,
            [[0xcc; 32], [0xdd; 32], [0xee; 32]]
        );
        assert_eq!(expected.preload.path, "bootstrap/app-preload.cjs");
        assert_eq!(expected.preload.sha256, [0xff; 32]);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn graph_source_refuses_missing_extra_duplicate_and_expression_fields() {
        for source in [
            graph_build().replace("packageSha256:", "packageHash:"),
            graph_build().replace("packageSha256:", "packageSha256:'f',packageSha256:"),
            graph_build().replace("providerTrees:{", "providerTrees:{extra:'1',"),
            graph_build().replace("providerTrees:{", "providerTrees:undefined,unused:{"),
            graph_build().replace(
                "preload:{path:",
                "preload:{path:process.env.PRELOAD,original:",
            ),
            graph_build().replace("preload:{path:", "preload:{path:null,unused:"),
            graph_build().replace("preload:{path:", "preload:{path:'x',path:"),
            graph_build().replace("preload:{path:", "preload:{algorithm:'sha256',path:"),
            graph_build().replace(&"f".repeat(64), &"F".repeat(64)),
            graph_build().replace("graphSource:{", "graphSource:{},graphSource:{"),
        ] {
            assert!(parse_build(&source).is_err(), "accepted {source}");
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn renderer_source_refuses_non_facade_renderer_auth_and_unknown_fields() {
        for source in [
            GRAPH_APP.replace("initialWindow:'none',", ""),
            GRAPH_APP.replace("initialWindow:'none',", "renderer:'index.html',"),
            GRAPH_APP.replace("initialWindow:'none',", "initialWindow:'none',auth:{},"),
            GRAPH_APP.replace("rendererSource:{root:", "rendererSource:{other:'yes',root:"),
            GRAPH_APP.replace("rendererSource:{root:", "rendererSource:null,extra:{root:"),
            GRAPH_APP.replace(
                "rendererSource:{root:",
                "rendererSource:{root:'again',root:",
            ),
            GRAPH_APP.replace(
                "rendererSource:{root:",
                "rendererSource:dynamic,alias:{root:",
            ),
        ] {
            assert!(parse_app(&source).is_err(), "accepted {source}");
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn renderer_source_and_preload_use_the_gh499_canonical_component_grammar() {
        for source in [
            GRAPH_APP.replace("client/published", "../client"),
            GRAPH_APP.replace("client/published", "client//published"),
            GRAPH_APP.replace("pages/editor.html", "../editor.html"),
            GRAPH_APP.replace("pages/editor.html", "/etc/passwd"),
        ] {
            assert!(parse_app(&source).is_err(), "accepted {source}");
        }
        for source in [
            graph_build().replace("bootstrap/app-preload.cjs", "bootstrap/../preload.js"),
            graph_build().replace("bootstrap/app-preload.cjs", "./preload.js"),
            graph_build().replace("bootstrap/app-preload.cjs", r"app\\other.js"),
        ] {
            assert!(parse_build(&source).is_err(), "accepted {source}");
        }
    }
}

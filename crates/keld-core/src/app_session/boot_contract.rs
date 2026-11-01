//! Exact strict versioned application boot descriptors and portable input admission.
//! Host resource acquisition, package verification, and session lifecycle stay parent-owned.

use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Deserializer};

use super::{AuthConfiguration, DIGEST_PREFIX, HostAppError, MAX_BOOT_BYTES, PERMISSIONS_FILE};

#[cfg(any(target_os = "macos", target_os = "linux", windows, test))]
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BootDocument {
    schema: u8,
    name: String,
    #[serde(
        default,
        deserialize_with = "crate::hello::deserialize_optional_app_version"
    )]
    version: Option<String>,
    entry: String,
    renderer: String,
    #[serde(deserialize_with = "deserialize_object")]
    permissions: PermissionsDocument,
    /// Optional schema-v1 operator auth carrier. This is deserialized only;
    /// Native remains the sole owner of profile filtering and semantic checks.
    #[serde(
        default,
        deserialize_with = "keld_native::auth::deserialize_auth_configuration"
    )]
    auth: AuthConfiguration,
}

#[cfg(any(target_os = "macos", target_os = "linux", windows, test))]
#[derive(Deserialize)]
struct SchemaProbe {
    schema: u8,
}

#[cfg(any(target_os = "macos", target_os = "linux", windows, test))]
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FacadeBootDocument {
    schema: u8,
    name: String,
    #[serde(
        default,
        deserialize_with = "crate::hello::deserialize_optional_app_version"
    )]
    version: Option<String>,
    entry: String,
    #[serde(deserialize_with = "deserialize_object")]
    permissions: PermissionsDocument,
}

/// Graph is an additional closed, owner-private field only in schema five.
/// No profile, renderer, auth or application-selected grant is added.
#[cfg(target_os = "macos")]
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GraphFacadeBootDocument {
    schema: u8,
    name: String,
    #[serde(
        default,
        deserialize_with = "crate::hello::deserialize_optional_app_version"
    )]
    version: Option<String>,
    entry: String,
    #[serde(deserialize_with = "deserialize_object")]
    permissions: PermissionsDocument,
    #[serde(deserialize_with = "deserialize_object")]
    graph: GraphBindingDocument,
}

#[cfg(target_os = "macos")]
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GraphBindingDocument {
    file: String,
    content_sha256: String,
}

#[cfg(any(target_os = "macos", target_os = "linux", windows, test))]
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PermissionsDocument {
    file: String,
    content_sha256: String,
}

#[cfg(any(target_os = "macos", target_os = "linux", windows, test))]
pub(super) struct ParsedBoot {
    pub(super) schema: u8,
    pub(super) name: String,
    pub(super) version: Option<String>,
    pub(super) entry: PathBuf,
    pub(super) initial: InitialWindow,
    pub(super) permissions_digest: [u8; 32],
    #[cfg(target_os = "macos")]
    pub(super) graph_digest: Option<[u8; 32]>,
    pub(super) auth_configuration: AuthConfiguration,
}

/// Selects the exact compiled contract from window ownership and the sole manifest declaration.
///
/// Schemas 1/3 are renderer-declared without/with an explicit profile; schemas
/// 2/4 are facade boots without/with one. The descriptor carries no second
/// profile value: verified permissions remain the sole authority.
#[must_use]
pub const fn boot_schema(facade: bool, profile: Option<keld_guard::DeclaredProfile>) -> u8 {
    match (facade, profile.is_some()) {
        (false, false) => 1,
        (true, false) => 2,
        (false, true) => 3,
        (true, true) => 4,
    }
}

// Require an object without a Value round-trip that would erase duplicate keys.
// The derived DTO remains the sole field/type parser at both closed boundaries.
fn deserialize_object<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct ObjectVisitor<T>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>> serde::de::Visitor<'de> for ObjectVisitor<T> {
        type Value = T;
        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("a closed boot-contract JSON object")
        }
        fn visit_map<M: serde::de::MapAccess<'de>>(self, map: M) -> Result<T, M::Error> {
            T::deserialize(serde::de::value::MapAccessDeserializer::new(map))
        }
    }
    deserializer.deserialize_map(ObjectVisitor(std::marker::PhantomData))
}

#[cfg(any(target_os = "macos", target_os = "linux", windows, test))]
pub(super) enum InitialWindow {
    Renderer(PathBuf),
    None,
}

/// Parse schema five without erasing duplicate keys or relaxing its fixed graph leaf.
#[cfg(target_os = "macos")]
fn parse_graph_document(text: &str) -> Result<GraphFacadeBootDocument, HostAppError> {
    let document: GraphFacadeBootDocument = serde_json::from_str(text).map_err(|source| {
        boot_error(
            format!("closed graph schema five was rejected: {source}"),
            "Retain the required graph/permissions objects; remove renderer, auth and extra fields.",
        )
    })?;
    if document.graph.file != super::graph_contract::GRAPH_FILE {
        return Err(boot_error(
            "graph.file must be keld.stage-graph.json",
            "Regenerate the fixed owner-private graph leaf.",
        ));
    }
    Ok(document)
}

/// Bound and probe the boot input before selecting its existing closed DTO.
#[cfg(any(target_os = "macos", target_os = "linux", windows, test))]
fn bounded_boot_schema(bytes: &[u8]) -> Result<(&str, SchemaProbe), HostAppError> {
    if bytes.len() > MAX_BOOT_BYTES {
        return Err(boot_error(
            "keld.boot.json exceeds the 64 KiB limit",
            "Regenerate a bounded boot descriptor of a supported exact schema.",
        ));
    }
    let text = std::str::from_utf8(bytes).map_err(|source| {
        boot_error(
            format!("keld.boot.json is not UTF-8: {source}"),
            "Write the descriptor as strict UTF-8 JSON.",
        )
    })?;
    // Probe the version with Serde, not a JSON Value map: even a duplicate
    // schema key must fail before selecting either closed descriptor shape.
    let mut decoder = serde_json::Deserializer::from_str(text);
    let probe: SchemaProbe = deserialize_object(&mut decoder).map_err(|source| {
        boot_error(
            format!("boot descriptor schema probe was rejected: {source}"),
            "Write one schema key and a strict versioned boot descriptor.",
        )
    })?;
    decoder.end().map_err(|source| {
        boot_error(
            format!("trailing boot descriptor input: {source}"),
            "Write exactly one closed JSON boot object.",
        )
    })?;
    Ok((text, probe))
}

#[cfg(any(target_os = "macos", target_os = "linux", windows, test))]
pub(super) fn parse_boot_bytes(bytes: &[u8]) -> Result<ParsedBoot, HostAppError> {
    let (text, probe) = bounded_boot_schema(bytes)?;
    #[cfg(target_os = "macos")]
    let mut graph_digest = None;
    let (name, version, entry, renderer, permissions, auth_configuration) = match probe.schema {
        1 | 3 => {
            let document: BootDocument = serde_json::from_str(text).map_err(|source| {
                boot_error(
                    format!("strict renderer-boot JSON was rejected: {source}"),
                    "Remove duplicate/unknown fields and regenerate the renderer boot schema 1 or 3.",
                )
            })?;
            debug_assert_eq!(document.schema, probe.schema);
            (
                document.name,
                document.version,
                document.entry,
                Some(document.renderer),
                document.permissions,
                document.auth,
            )
        }
        2 | 4 => {
            let document: FacadeBootDocument = serde_json::from_str(text).map_err(|source| {
                boot_error(
                    format!("strict facade-boot JSON was rejected: {source}"),
                    "Remove renderer, auth, profile, duplicate and unknown fields from the facade descriptor.",
                )
            })?;
            debug_assert_eq!(document.schema, probe.schema);
            (
                document.name,
                document.version,
                document.entry,
                None,
                document.permissions,
                AuthConfiguration::default(),
            )
        }
        #[cfg(target_os = "macos")]
        5 => {
            let document = parse_graph_document(text)?;
            debug_assert_eq!(document.schema, probe.schema);
            graph_digest = Some(decode_digest_field(
                &document.graph.content_sha256,
                "graph.content_sha256",
            )?);
            (
                document.name,
                document.version,
                document.entry,
                None,
                document.permissions,
                AuthConfiguration::default(),
            )
        }
        version => {
            return Err(boot_error(
                format!("unsupported schema {version}"),
                "Regenerate the descriptor with supported schema 1, 2, 3, 4 or 5 (macOS graph only).",
            ));
        }
    };
    if name.is_empty() {
        return Err(boot_error(
            "name must be a non-empty string",
            "Set the reviewed project name before compiling the boot descriptor.",
        ));
    }
    let entry = validate_relative_path("entry", &entry)?;
    let initial = match renderer {
        Some(renderer) => InitialWindow::Renderer(validate_relative_path("renderer", &renderer)?),
        None => InitialWindow::None,
    };
    if permissions.file != PERMISSIONS_FILE {
        return Err(boot_error(
            format!(
                "permissions.file must be the literal {PERMISSIONS_FILE}, found {}",
                permissions.file
            ),
            "Regenerate the descriptor with the fixed permissions filename.",
        ));
    }
    let permissions_digest = decode_digest(&permissions.content_sha256)?;
    Ok(ParsedBoot {
        schema: probe.schema,
        name,
        version,
        entry,
        initial,
        permissions_digest,
        #[cfg(target_os = "macos")]
        graph_digest,
        auth_configuration,
    })
}

#[cfg(any(target_os = "macos", target_os = "linux", windows, test))]
pub(super) fn boot_error(detail: impl Into<String>, fix: &'static str) -> HostAppError {
    HostAppError::new("KELD-CORE-035", "boot descriptor validation", detail, fix)
}

#[cfg(any(target_os = "macos", target_os = "linux", windows, test))]
pub(super) fn target_error(kind: &'static str, detail: impl Into<String>) -> HostAppError {
    HostAppError::new(
        "KELD-CORE-036",
        "staged target validation",
        format!("{kind}: {}", detail.into()),
        "Regenerate the owner-private stage with readable regular files and no symlinks.",
    )
}

#[cfg(any(target_os = "macos", target_os = "linux", windows, test))]
pub(super) fn validate_relative_path(
    kind: &'static str,
    value: &str,
) -> Result<PathBuf, HostAppError> {
    if value.is_empty()
        || value.contains('\\')
        || value.contains(':')
        || value
            .split('/')
            .any(|part| part.is_empty() || matches!(part, "." | ".."))
    {
        return Err(target_error(
            kind,
            "path is not a portable project-relative path",
        ));
    }
    let path = Path::new(value);
    for component in path.components() {
        if !matches!(component, Component::Normal(_)) {
            return Err(target_error(
                kind,
                "path contains a root, prefix, dot, or dot-dot",
            ));
        }
    }
    Ok(path.to_path_buf())
}

#[cfg(any(target_os = "macos", target_os = "linux", windows, test))]
pub(super) fn decode_digest(value: &str) -> Result<[u8; 32], HostAppError> {
    decode_digest_field(value, "permissions.content_sha256")
}

#[cfg(any(target_os = "macos", target_os = "linux", windows, test))]
fn decode_digest_field(value: &str, field: &str) -> Result<[u8; 32], HostAppError> {
    let Some(hex) = value.strip_prefix(DIGEST_PREFIX) else {
        return Err(boot_error(
            format!("{field} must start with sha256:"),
            "Regenerate the exact lowercase SHA-256 descriptor value.",
        ));
    };
    if hex.len() != 64
        || !hex
            .as_bytes()
            .iter()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
    {
        return Err(boot_error(
            format!("{field} must contain 64 lowercase hexadecimal digits"),
            "Regenerate the exact lowercase SHA-256 descriptor value.",
        ));
    }
    let mut digest = [0_u8; 32];
    for (index, chunk) in hex.as_bytes().as_chunks::<2>().0.iter().enumerate() {
        let text = std::str::from_utf8(chunk).map_err(|source| {
            boot_error(
                format!("digest encoding is invalid: {source}"),
                "Regenerate the exact lowercase SHA-256 descriptor value.",
            )
        })?;
        digest[index] = u8::from_str_radix(text, 16).map_err(|source| {
            boot_error(
                format!("digest encoding is invalid: {source}"),
                "Regenerate the exact lowercase SHA-256 descriptor value.",
            )
        })?;
    }
    Ok(digest)
}

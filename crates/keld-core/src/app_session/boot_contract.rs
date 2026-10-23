//! Closed renderer boot descriptors and portable input admission.
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
#[serde(deny_unknown_fields)]
struct PermissionsDocument {
    file: String,
    content_sha256: String,
}

#[cfg(any(target_os = "macos", target_os = "linux", windows, test))]
pub(super) struct ParsedBoot {
    pub(super) schema: u8,
    pub(super) name: String,
    pub(super) entry: PathBuf,
    pub(super) renderer: PathBuf,
    pub(super) permissions_digest: [u8; 32],
    pub(super) auth_configuration: AuthConfiguration,
}

/// Selects the renderer contract from the sole manifest declaration.
///
/// Schema 1 preserves the profile-free renderer shape. Schema 3 preserves that
/// exact shape but requires explicit-profile enforcement. Facade schemas 2/4
/// belong to GH-446 and cannot be inferred or emitted by this renderer API.
#[must_use]
pub const fn renderer_boot_schema(profile: Option<keld_guard::DeclaredProfile>) -> u8 {
    if profile.is_some() { 3 } else { 1 }
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
pub(super) fn parse_boot_bytes(bytes: &[u8]) -> Result<ParsedBoot, HostAppError> {
    if bytes.len() > MAX_BOOT_BYTES {
        return Err(boot_error(
            "keld.boot.json exceeds the 64 KiB limit",
            "Regenerate a bounded renderer boot descriptor (schema 1 or 3).",
        ));
    }
    let text = std::str::from_utf8(bytes).map_err(|source| {
        boot_error(
            format!("keld.boot.json is not UTF-8: {source}"),
            "Write the descriptor as strict UTF-8 JSON.",
        )
    })?;
    let mut decoder = serde_json::Deserializer::from_str(text);
    let document: BootDocument = deserialize_object(&mut decoder).map_err(|source| {
        boot_error(
            format!("closed renderer-boot JSON was rejected: {source}"),
            "Remove duplicate/unknown fields and regenerate a JSON object of schema 1 or 3.",
        )
    })?;
    decoder.end().map_err(|source| {
        boot_error(
            format!("trailing boot descriptor input: {source}"),
            "Write exactly one closed JSON boot object.",
        )
    })?;
    if !matches!(document.schema, 1 | 3) {
        return Err(boot_error(
            format!("unsupported schema {}", document.schema),
            "Regenerate the renderer boot with schema 1 without a profile or schema 3 with an explicit profile; facade boot is not implemented here.",
        ));
    }
    if document.name.is_empty() {
        return Err(boot_error(
            "name must be a non-empty string",
            "Set the reviewed project name before compiling the boot descriptor.",
        ));
    }
    let entry = validate_relative_path("entry", &document.entry)?;
    let renderer = validate_relative_path("renderer", &document.renderer)?;
    if document.permissions.file != PERMISSIONS_FILE {
        return Err(boot_error(
            format!(
                "permissions.file must be the literal {PERMISSIONS_FILE}, found {}",
                document.permissions.file
            ),
            "Regenerate the descriptor with the fixed permissions filename.",
        ));
    }
    let permissions_digest = decode_digest(&document.permissions.content_sha256)?;
    Ok(ParsedBoot {
        schema: document.schema,
        name: document.name,
        entry,
        renderer,
        permissions_digest,
        auth_configuration: document.auth,
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
    let Some(hex) = value.strip_prefix(DIGEST_PREFIX) else {
        return Err(boot_error(
            "permissions.content_sha256 must start with sha256:",
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
            "permissions.content_sha256 must contain 64 lowercase hexadecimal digits",
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

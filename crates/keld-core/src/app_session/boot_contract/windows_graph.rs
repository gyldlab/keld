//! Closed Windows development graph boot data. This parser grants no authority.

use serde::{Deserialize, Deserializer};

use super::{
    AuthConfiguration, HostAppError, InitialWindow, PERMISSIONS_FILE, ParsedBoot,
    PermissionsDocument, boot_error, decode_digest, decode_digest_for, deserialize_object,
};
use crate::SingleInstanceDeclaration;
use crate::app_session::graph_document::GRAPH_FILE;
use crate::app_session::graph_selection::{SelectedRendererSource, relative_path};

pub(in crate::app_session) const WINDOWS_GRAPH_BOOT_SCHEMA: u8 = 8;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GraphBootDocument {
    schema: u8,
    name: String,
    #[serde(default, deserialize_with = "optional_version")]
    version: Option<String>,
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
        deserialize_with = "optional_single_instance"
    )]
    single_instance: Option<SingleInstanceDeclaration>,
    #[serde(
        default,
        rename = "applicationNameForUserAgent",
        deserialize_with = "optional_user_agent"
    )]
    application_name_for_user_agent: Option<keld_wv::ApplicationNameForUserAgent>,
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

/// Syntax-checked graph and metadata. The retained root and verified policy
/// still have to admit this selection.
#[derive(Debug)]
pub(in crate::app_session) struct WindowsGraphBoot {
    pub(in crate::app_session) graph_sha256: [u8; 32],
    pub(in crate::app_session) source: SelectedRendererSource,
    pub(in crate::app_session) version: Option<String>,
    pub(in crate::app_session) single_instance: Option<SingleInstanceDeclaration>,
    pub(in crate::app_session) application_name_for_user_agent:
        Option<keld_wv::ApplicationNameForUserAgent>,
}

fn optional_version<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    // #[serde(default)] handles absence. A present null is not a string.
    let value = String::deserialize(deserializer)?;
    if value.is_empty() || value.contains('\0') || value.len() > 4096 {
        return Err(serde::de::Error::custom(
            "declared version must be nonempty, NUL-free and at most 4096 UTF-8 bytes",
        ));
    }
    Ok(Some(value))
}

fn optional_single_instance<'de, D>(
    deserializer: D,
) -> Result<Option<SingleInstanceDeclaration>, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_object(deserializer).map(Some)
}

fn optional_user_agent<'de, D>(
    deserializer: D,
) -> Result<Option<keld_wv::ApplicationNameForUserAgent>, D::Error>
where
    D: Deserializer<'de>,
{
    keld_wv::ApplicationNameForUserAgent::new(String::deserialize(deserializer)?)
        .map(Some)
        .map_err(serde::de::Error::custom)
}

pub(super) fn parse(text: &str) -> Result<ParsedBoot, HostAppError> {
    let document: GraphBootDocument = serde_json::from_str(text).map_err(|error| {
        boot_error(
            format!("closed Windows development graph boot was rejected: {error}"),
            "Use the exact graph boot compiler with only the approved descriptor fields.",
        )
    })?;
    if document.schema != WINDOWS_GRAPH_BOOT_SCHEMA
        || document.name.is_empty()
        || document.name.contains('\0')
        || document.permissions.file != PERMISSIONS_FILE
        || document.graph.file != GRAPH_FILE
    {
        return Err(boot_error(
            "Windows development graph boot requires schema 8, a nonempty NUL-free name and the fixed graph and permissions filenames",
            "Rebuild the application with the supported graph boot compiler.",
        ));
    }
    let source = SelectedRendererSource::new(
        &document.renderer_source.root,
        &document.renderer_source.entry,
        &document.preload.path,
        decode_digest_for("preload.content_sha256", &document.preload.content_sha256)?,
    )?;
    Ok(ParsedBoot {
        schema: WINDOWS_GRAPH_BOOT_SCHEMA,
        name: document.name,
        entry: relative_path(&document.entry)?,
        initial: InitialWindow::None,
        permissions_digest: decode_digest(&document.permissions.content_sha256)?,
        auth_configuration: AuthConfiguration::default(),
        windows_graph: Some(WindowsGraphBoot {
            graph_sha256: decode_digest_for(
                "graph.content_sha256",
                &document.graph.content_sha256,
            )?,
            source,
            version: document.version,
            single_instance: document.single_instance,
            application_name_for_user_agent: document.application_name_for_user_agent,
        }),
    })
}

#[cfg(test)]
mod tests;

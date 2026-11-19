//! Shared graph-v1 shape and path admission. Filesystem authority is separate.

use super::boot_contract::{decode_digest_for, deserialize_object};
use super::graph_selection::{SelectedRendererSource, relative_path};
use super::{HostAppError, target_error};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Fixed graph file selected by a graph boot descriptor.
pub const GRAPH_FILE: &str = "keld.stage-graph.json";
/// Maximum serialized graph length.
pub const MAX_GRAPH_BYTES: usize = 8 * 1024 * 1024;
pub(super) const MAX_FILES: usize = 32_768;
pub(super) const MAX_ENTRIES: usize = 65_536;
pub(super) const MAX_SELECTED_PRELOAD_BYTES: usize = 512 * 1024;
pub(super) const GRAPH_SCHEMA: &str = "keld.stage.graph/v1";
const RESERVED_GRAPH_ROOTS: [&str; 5] = [
    "keld-host",
    "keld-host.exe",
    "keld.boot.json",
    "keld.permissions.jsonc",
    GRAPH_FILE,
];

fn graph_error(phase: &'static str, detail: impl std::fmt::Display) -> HostAppError {
    target_error("graph admission", format!("{phase}: {detail}"))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct GraphDocument {
    pub(super) schema: String,
    pub(super) entry: String,
    #[serde(deserialize_with = "deserialize_graph_files")]
    pub(super) files: Vec<GraphFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct GraphFile {
    pub(super) path: String,
    pub(super) size: u64,
    pub(super) sha256: String,
}

fn deserialize_graph_files<'de, D>(deserializer: D) -> Result<Vec<GraphFile>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(transparent)]
    struct ClosedFile(#[serde(deserialize_with = "deserialize_object")] GraphFile);

    Vec::<ClosedFile>::deserialize(deserializer)
        .map(|files| files.into_iter().map(|ClosedFile(file)| file).collect())
}

pub(super) fn parse_sha(value: &str) -> Result<[u8; 32], HostAppError> {
    decode_digest_for("graph file sha256", value)
}

pub(super) fn excluded(relative: &str) -> bool {
    RESERVED_GRAPH_ROOTS.contains(&relative)
}

pub(super) fn validate_graph_shape(bytes: &[u8]) -> Result<GraphDocument, HostAppError> {
    if bytes.len() > MAX_GRAPH_BYTES {
        return Err(graph_error(
            "graph size",
            "8 MiB exact graph limit exceeded",
        ));
    }
    let mut decoder = serde_json::Deserializer::from_slice(bytes);
    let doc: GraphDocument = deserialize_object(&mut decoder)
        .map_err(|source| graph_error("closed graph JSON", source))?;
    decoder
        .end()
        .map_err(|source| graph_error("closed graph JSON", source))?;
    if doc.schema != GRAPH_SCHEMA {
        return Err(graph_error("graph schema", "expected keld.stage.graph/v1"));
    }
    relative_path(&doc.entry)?;
    validate_graph_files(&doc.files)?;
    if !doc.files.iter().any(|row| row.path == doc.entry) {
        return Err(graph_error(
            "graph entry",
            "entry is absent from captured files",
        ));
    }
    for required in ["package.json", "package-lock.json"] {
        if !doc.files.iter().any(|row| row.path == required) {
            return Err(graph_error("graph input", format!("missing {required}")));
        }
    }
    Ok(doc)
}

pub(super) fn validate_graph_files(files: &[GraphFile]) -> Result<(), HostAppError> {
    if files.len() > MAX_FILES || files.is_empty() {
        return Err(graph_error("census", "empty or more than 32768 files"));
    }
    let mut previous = None;
    for row in files {
        relative_path(&row.path)?;
        if excluded(&row.path) {
            return Err(graph_error("reserved file", &row.path));
        }
        if previous.is_some_and(|prev| prev >= row.path.as_str()) {
            return Err(graph_error("graph ordering", "unsorted or duplicate paths"));
        }
        parse_sha(&row.sha256)?;
        previous = Some(row.path.as_str());
    }
    let paths: Vec<&str> = files.iter().map(|row| row.path.as_str()).collect();
    keld_guard::validate_windows_graph_namespace(&paths)
        .map_err(|error| graph_error("Windows graph namespace", error))?;
    let roots: BTreeSet<&str> = paths
        .into_iter()
        .map(|path| path.split_once('/').map_or(path, |(root, _)| root))
        .collect();
    for root in roots {
        for reserved in RESERVED_GRAPH_ROOTS {
            if keld_guard::windows_package_components_equal(root, reserved)
                .map_err(|error| graph_error("Windows protected graph member", error))?
            {
                return Err(graph_error("reserved file", root));
            }
        }
    }
    Ok(())
}

/// Validates bounded closed graph-v1 data and platform namespace syntax.
///
/// This checks data shape only and grants no filesystem or resource authority.
///
/// # Errors
/// Refuses malformed, oversized, incomplete, unordered, or colliding graph data.
pub fn validate_stage_graph_shape(bytes: &[u8]) -> Result<(), HostAppError> {
    validate_graph_shape(bytes).map(|_| ())
}

pub(super) fn verify_selected_rows(
    graph: &GraphDocument,
    selected: &SelectedRendererSource,
) -> Result<(), HostAppError> {
    let root = selected
        .root()
        .to_str()
        .ok_or_else(|| graph_error("selected entry", "non UTF-8 root"))?;
    let relative = selected
        .entry()
        .to_str()
        .ok_or_else(|| graph_error("selected entry", "non UTF-8 entry"))?;
    let selected_entry = format!("{root}/{relative}");
    if graph
        .files
        .binary_search_by(|row| row.path.as_str().cmp(&selected_entry))
        .is_err()
    {
        return Err(graph_error(
            "selected entry",
            "missing from the admitted graph",
        ));
    }
    let preload = selected
        .preload_path()
        .to_str()
        .ok_or_else(|| graph_error("selected preload", "non UTF-8 path"))?;
    let preload_index = graph
        .files
        .binary_search_by(|row| row.path.as_str().cmp(preload))
        .map_err(|_| graph_error("selected preload", "missing from the admitted graph"))?;
    let row = &graph.files[preload_index];
    if row.size > MAX_SELECTED_PRELOAD_BYTES as u64
        || parse_sha(&row.sha256)? != selected.preload_sha256()
    {
        return Err(graph_error(
            "selected preload",
            "size or digest differs from boot pin",
        ));
    }
    Ok(())
}

#[cfg(all(test, windows))]
#[allow(clippy::expect_used, clippy::panic)] // Literal assertions are test oracles.
mod tests {
    use super::validate_stage_graph_shape;

    fn row(path: &str) -> serde_json::Value {
        serde_json::json!({"path": path, "size": 1, "sha256": format!("sha256:{}", "00".repeat(32))})
    }

    #[test]
    fn graph_shape_uses_windows_ordinal_namespace_and_closed_fields() {
        let doc = serde_json::json!({"schema":"keld.stage.graph/v1", "entry":"main.js", "files":[row("main.js"), row("package-lock.json"), row("package.json")]});
        assert!(validate_stage_graph_shape(doc.to_string().as_bytes()).is_ok());
        let collision = serde_json::json!({"schema":"keld.stage.graph/v1", "entry":"A/main.js", "files":[row("A/main.js"), row("a/MAIN.JS"), row("package-lock.json"), row("package.json")]});
        assert!(validate_stage_graph_shape(collision.to_string().as_bytes()).is_err());
        let prefix_collision = serde_json::json!({"schema":"keld.stage.graph/v1", "entry":"A/x.js", "files":[row("A/x.js"), row("a/y.js"), row("package-lock.json"), row("package.json")]});
        assert!(validate_stage_graph_shape(prefix_collision.to_string().as_bytes()).is_err());
        let protected = serde_json::json!({"schema":"keld.stage.graph/v1", "entry":"KELD-HOST.EXE", "files":[row("KELD-HOST.EXE"), row("package-lock.json"), row("package.json")]});
        assert!(validate_stage_graph_shape(protected.to_string().as_bytes()).is_err());
        let mut unknown = doc;
        unknown["unknown"] = serde_json::json!(true);
        assert!(validate_stage_graph_shape(unknown.to_string().as_bytes()).is_err());
    }

    #[test]
    fn stage_graph_requires_objects_at_the_root_and_each_file_row() {
        let files = [
            row("main.js"),
            row("package-lock.json"),
            row("package.json"),
        ];
        let root_as_array = serde_json::json!(["keld.stage.graph/v1", "main.js", files]);
        assert!(validate_stage_graph_shape(root_as_array.to_string().as_bytes()).is_err());
        let digest = format!("sha256:{}", "00".repeat(32));
        let row_as_array = serde_json::json!({
            "schema": "keld.stage.graph/v1",
            "entry": "main.js",
            "files": [["main.js", 1, digest], files[1], files[2]]
        });
        assert!(validate_stage_graph_shape(row_as_array.to_string().as_bytes()).is_err());
    }
}

//! Retained Windows development graph validation and provider qualification.
//!
//! The producer and Host compare the same closed graph with real file identities.
//! Application data cannot select the framework qualification.

use std::collections::BTreeSet;
use std::fs::File;
use std::io::Read as _;
use std::path::{Path, PathBuf};

use sha2::{Digest as _, Sha256};

use super::graph_document::{
    GRAPH_FILE, GRAPH_SCHEMA, GraphDocument, GraphFile, MAX_ENTRIES, MAX_FILES, MAX_GRAPH_BYTES,
    MAX_SELECTED_PRELOAD_BYTES, excluded, parse_sha, validate_graph_files, validate_graph_shape,
    verify_selected_rows,
};
use super::graph_selection::{
    MAX_COMPONENTS, MAX_PATH_BYTES, SelectedRendererSource, relative_path,
};
use super::{HostAppError, read_bounded, target_error};

mod windows;
mod windows_boot;
use windows::{
    ObjectMetadata, RetainedRoot, borrow_retained_root, census_directory, file_metadata,
    missing_configuration_reference, open_relative_file, path_metadata, root_identity,
    same_identity, sorted_census_children,
};
pub use windows_boot::{
    WindowsDistributionQualification, qualified_windows_distribution,
    read_windows_bound_graph_from_retained_root, windows_graph_target,
};

fn clone_root(root: &RetainedRoot) -> Result<RetainedRoot, HostAppError> {
    root.try_clone()
        .map_err(|source| graph_error("retained root clone", source))
}

const MAX_JSON_BYTES: usize = 4 * 1024 * 1024;
const BUFFER_BYTES: usize = 64 * 1024;

const PACKAGES: [(&str, &str); 3] = [
    ("node_modules/electron", "@keld/electron"),
    ("node_modules/@keld/api", "@keld/api"),
    ("node_modules/@keld/kipc", "@keld/kipc"),
];

/// Independently pinned input to the explicit GH589 first-proof staging caller.
/// Its values must be obtained from reviewed source/lock and the actual
/// independently packed and extracted GH502/API/KIPC provider artifacts.
/// This type is not an application configuration or user-authored grant.
#[derive(Clone, Debug)]
pub struct ApprovedGraphCaptureInput {
    /// Independently selected exact root package-manifest digest.
    pub expected_package_sha256: [u8; 32],
    /// Independently selected exact npm-v3 lockfile digest.
    pub expected_lock_sha256: [u8; 32],
    /// In PACKAGES order: the canonical *unpacked* subtree fingerprint of
    /// electron, @keld/api, and @keld/kipc, including all regular files.
    pub expected_package_trees: [[u8; 32]; 3],
}

/// A graph admitted against exact copied or installed inputs and retained root.
/// Private fields prevent callers from minting an admitted graph from bare data.
#[derive(Debug)]
pub struct AdmittedStageGraph {
    root: PathBuf,
    retained_root: RetainedRoot,
    graph_bytes: Vec<u8>,
    graph_sha256: [u8; 32],
    graph: GraphDocument,
    root_identity: (u64, u64),
    /// Only the closed development boot binder attaches this selector.
    selected_source: Option<SelectedRendererSource>,
    /// A byte-identical replacement of either selected leaf must fail while
    /// this admitted graph lives; full copied-tree identity belongs to Pack.
    selected_file_identities: Option<(ObjectMetadata, ObjectMetadata)>,
    /// The exact canonical Windows entry stays deny-write/delete pinned through
    /// all sessions and any retained fatal native owner.
    #[cfg(windows)]
    windows_runtime_entry: Option<File>,
}

fn graph_error(phase: &'static str, detail: impl std::fmt::Display) -> HostAppError {
    target_error("graph admission", format!("{phase}: {detail}"))
}

fn graph_hash(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

fn sha_text(digest: &[u8; 32]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut result = String::with_capacity(71);
    result.push_str("sha256:");
    for &byte in digest {
        result.push(char::from(HEX[usize::from(byte >> 4)]));
        result.push(char::from(HEX[usize::from(byte & 15)]));
    }
    result
}

struct ObservedFile {
    path: String,
    metadata: ObjectMetadata,
}

fn check_census_directory(
    directory: &Path,
    selected: &File,
    expected: &ObjectMetadata,
) -> Result<(), HostAppError> {
    let from_fd =
        file_metadata(selected).map_err(|source| graph_error("directory census", source))?;
    let from_path =
        path_metadata(directory).map_err(|source| graph_error("directory census", source))?;
    if !from_fd.is_dir()
        || !from_path.is_dir()
        || !same_identity(&from_fd, expected)
        || !same_identity(&from_path, expected)
    {
        return Err(graph_error(
            "directory census",
            "physical directory changed during census",
        ));
    }
    Ok(())
}

fn census(root: &Path, retained: &RetainedRoot) -> Result<Vec<ObservedFile>, HostAppError> {
    census_subtree(root, retained, None)
}

fn census_subtree(
    root: &Path,
    retained: &RetainedRoot,
    namespace_prefix: Option<&str>,
) -> Result<Vec<ObservedFile>, HostAppError> {
    let root_meta = file_metadata(&census_directory(retained, "")?)
        .map_err(|source| graph_error("directory census", source))?;
    let mut pending = vec![(root.to_owned(), String::new(), root_meta)];
    let mut files = Vec::new();
    let mut directories = Vec::new();
    let mut entries = 0_usize;
    while let Some((directory, prefix, expected_dir)) = pending.pop() {
        let held = census_directory(retained, &prefix)?;
        check_census_directory(&directory, &held, &expected_dir)?;
        let children = sorted_census_children(&directory, &held, MAX_ENTRIES - entries)?;
        for (name, metadata) in children {
            entries = entries
                .checked_add(1)
                .ok_or_else(|| graph_error("census", "entry overflow"))?;
            if entries > MAX_ENTRIES {
                return Err(graph_error("census", "more than 65536 filesystem entries"));
            }
            let relative = if prefix.is_empty() {
                name
            } else {
                format!("{prefix}/{name}")
            };
            relative_path(&relative)?;
            let logical = namespace_prefix
                .map_or_else(|| relative.clone(), |prefix| format!("{prefix}/{relative}"));
            relative_path(&logical)?;
            if logical == "keld.config.ts"
                || logical == ".git"
                || logical == ".keld"
                || logical.starts_with(".git/")
                || logical.starts_with(".keld/")
            {
                return Err(graph_error("forbidden input", relative));
            }
            if metadata.is_dir() {
                directories.push(relative.clone());
                pending.push((root.join(&relative), relative, metadata));
            } else if metadata.is_file() {
                if !excluded(&logical) {
                    files.push(ObservedFile {
                        path: relative,
                        metadata,
                    });
                    if files.len() > MAX_FILES {
                        return Err(graph_error("census", "more than 32768 regular files"));
                    }
                }
            } else {
                return Err(graph_error("non-regular input", relative));
            }
        }
        check_census_directory(&directory, &held, &expected_dir)?;
    }
    files.sort_by(|left, right| left.path.cmp(&right.path));
    // The graph has exactly three fields: there is no fourth directory list.
    // Each physical directory must therefore be implied by a captured file's
    // relative parent path. An extra or empty directory changes the census.
    let mut implied = BTreeSet::new();
    for file in &files {
        let mut parent = Path::new(&file.path).parent();
        while let Some(path) = parent {
            if path.as_os_str().is_empty() {
                break;
            }
            let name = path
                .to_str()
                .ok_or_else(|| graph_error("directory", "non UTF-8 parent"))?;
            implied.insert(name.to_owned());
            parent = path.parent();
        }
    }
    if let Some(extra) = directories
        .iter()
        .find(|path| !implied.contains(path.as_str()))
    {
        return Err(graph_error("unlisted directory", extra));
    }
    Ok(files)
}

fn read_hash(mut file: File) -> Result<([u8; 32], u64), HostAppError> {
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; BUFFER_BYTES].into_boxed_slice();
    let mut bytes = 0_u64;
    loop {
        let n = file
            .read(buffer.as_mut())
            .map_err(|source| graph_error("file hash", source))?;
        if n == 0 {
            break;
        }
        bytes = bytes
            .checked_add(n as u64)
            .ok_or_else(|| graph_error("file hash", "overflow"))?;
        hasher.update(&buffer[..n]);
    }
    Ok((hasher.finalize().into(), bytes))
}

fn check_files(root: &Path, dir: &RetainedRoot, doc: &GraphDocument) -> Result<(), HostAppError> {
    let actual = census(root, dir)?;
    let selected: Vec<_> = doc.files.iter().map(|row| row.path.as_str()).collect();
    let present: Vec<_> = actual.iter().map(|row| row.path.as_str()).collect();
    if selected != present {
        let missing = selected.iter().find(|name| !present.contains(name));
        let unlisted = present.iter().find(|name| !selected.contains(name));
        return Err(graph_error(
            "file census",
            format!("missing={missing:?}; unlisted={unlisted:?}"),
        ));
    }
    for (row, observed) in doc.files.iter().zip(&actual) {
        let file = open_relative_file(dir, &relative_path(&row.path)?, "graph file")?;
        let metadata =
            file_metadata(&file).map_err(|source| graph_error("file metadata", source))?;
        let from_path = path_metadata(root.join(&row.path))
            .map_err(|source| graph_error("file metadata", source))?;
        if !same_identity(&metadata, &observed.metadata)
            || !same_identity(&from_path, &observed.metadata)
        {
            return Err(graph_error("file census identity", &row.path));
        }
        if metadata.len() != row.size {
            return Err(graph_error("file size", &row.path));
        }
        let held = file
            .try_clone()
            .map_err(|source| graph_error("file retained identity", source))?;
        let (actual_sha, count) = read_hash(file)?;
        if count != row.size || actual_sha != parse_sha(&row.sha256)? {
            return Err(graph_error("file digest", &row.path));
        }
        let after_fd =
            file_metadata(&held).map_err(|source| graph_error("file metadata", source))?;
        let after_path = path_metadata(root.join(&row.path))
            .map_err(|source| graph_error("file metadata", source))?;
        if !same_identity(&after_fd, &observed.metadata)
            || !same_identity(&after_path, &observed.metadata)
        {
            return Err(graph_error("file census identity", &row.path));
        }
    }
    // The second physical census detects source/stage members inserted or
    // replaced after their first enumeration but before the final file hash.
    let final_census = census(root, dir)?;
    if final_census.len() != actual.len()
        || !final_census.iter().zip(&actual).all(|(after, before)| {
            after.path == before.path && same_identity(&after.metadata, &before.metadata)
        })
    {
        return Err(graph_error(
            "file census identity",
            "physical inventory changed during admission",
        ));
    }
    Ok(())
}

fn read_json(dir: &RetainedRoot, file: &str) -> Result<serde_json::Value, HostAppError> {
    let selected = open_relative_file(dir, &relative_path(file)?, "graph package JSON")?;
    let data = read_bounded(selected, MAX_JSON_BYTES, "graph package JSON")?;
    let text =
        std::str::from_utf8(&data).map_err(|source| graph_error("package/lock JSON", source))?;
    keld_guard::parse_unique_json(text).map_err(|source| graph_error("package/lock JSON", source))
}

fn installed_package_root(path: &str) -> Option<String> {
    let stem = path.strip_suffix("/package.json")?;
    let components: Vec<_> = stem.split('/').collect();
    let index = components
        .iter()
        .rposition(|item| *item == "node_modules")?;
    if index + 2 == components.len()
        || (index + 3 == components.len() && components[index + 1].starts_with('@'))
    {
        return Some(stem.to_owned());
    }
    None
}

/// Apply the existing bounded local-file spelling to only the fixed archive
/// stem selected by the Electron or updater package owner. No filesystem target
/// is opened or trusted here. Independently selected package/lock source bytes,
/// canonical installed identities and the same three approved packed-provider
/// fingerprints continue to govern this developer-owned source admission.
fn first_party_file_archive(alias: &str, version: &str, archive_stem: &str) -> bool {
    const MAX_FILE_SPEC_BYTES: usize = 4096;
    let Some(raw) = alias.strip_prefix("file:") else {
        return false;
    };
    if raw.is_empty()
        || raw.len() > MAX_FILE_SPEC_BYTES
        || raw.starts_with("//")
        || raw
            .chars()
            .any(|ch| ch.is_control() || matches!(ch, '\\' | ':' | '?' | '#' | '%'))
    {
        return false;
    }
    let selected = raw.strip_prefix("./").unwrap_or(raw);
    let segments = selected.strip_prefix('/').unwrap_or(selected);
    if segments
        .split('/')
        .any(|segment| segment.is_empty() || matches!(segment, "." | ".."))
    {
        return false;
    }
    segments.rsplit('/').next() == Some(format!("{archive_stem}-{version}.tgz").as_str())
}

fn first_party_sha512_integrity(value: &str) -> bool {
    let Some(bytes) = value.strip_prefix("sha512-").map(str::as_bytes) else {
        return false;
    };
    bytes.len() == 88
        && bytes.ends_with(b"==")
        && bytes[..86]
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(*byte, b'+' | b'/'))
}

// This is the existing local-archive lock receipt rule, shared by the two
// fixed first-party package selectors. It never opens the archive or mints a
// provider identity; independently selected root/lock and graph inputs govern.
fn validate_local_archive_receipt(
    alias: &str,
    installed: &serde_json::Value,
    resolved_phase: &'static str,
    integrity_phase: &'static str,
) -> Result<(), HostAppError> {
    if installed
        .get("resolved")
        .and_then(serde_json::Value::as_str)
        != Some(alias)
    {
        return Err(graph_error(
            resolved_phase,
            "installed lock resolution differs from selected root archive",
        ));
    }
    if !installed
        .get("integrity")
        .and_then(serde_json::Value::as_str)
        .is_some_and(first_party_sha512_integrity)
    {
        return Err(graph_error(
            integrity_phase,
            "missing strict SHA-512 archive receipt",
        ));
    }
    Ok(())
}

fn validate_electron_archive(
    alias: &str,
    packages: &serde_json::Map<String, serde_json::Value>,
) -> Result<(), HostAppError> {
    let installed_electron = packages
        .get("node_modules/electron")
        .ok_or_else(|| graph_error("lock", "missing installed Electron row"))?;
    let version = installed_electron
        .get("version")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| graph_error("lock", "installed Electron version missing"))?;
    let local_first_party = first_party_file_archive(alias, version, "keld-electron");
    if !alias.starts_with("npm:@keld/electron@") && !local_first_party {
        return Err(graph_error(
            "electron alias",
            "root electron must be @keld/electron or independently pinned first-party local archive",
        ));
    }
    if local_first_party {
        validate_local_archive_receipt(
            alias,
            installed_electron,
            "electron resolved archive",
            "electron archive integrity",
        )?;
    }
    Ok(())
}

// This is a conflict-language check, never a module resolver. A wildcard
// pattern overlaps an entire protected family whenever its literal prefix can
// start that family's root or one of its subpaths; suffixes cannot make an
// unbounded subpath family safe. Protect private Electron and SDK subpaths too.
fn reserved_configuration_mapping(pattern: &str) -> bool {
    ["electron", "@keld/api", "@keld/kipc"]
        .into_iter()
        .any(|root| {
            if let Some((prefix, _)) = pattern.split_once('*') {
                root.starts_with(prefix)
                    || prefix
                        .strip_prefix(root)
                        .is_some_and(|tail| tail.starts_with('/'))
            } else {
                pattern == root
                    || pattern
                        .strip_prefix(root)
                        .is_some_and(|tail| tail.starts_with('/'))
            }
        })
}

// Only explicit captured relative JSON inputs are inspectable in this bounded
// first-proof contract. Package lookup, config merging and module selection stay
// with upstream Bun; unknown/escaping config sources remain unqualified.
fn configuration_reference(file: &str, reference: &str) -> Result<String, HostAppError> {
    if reference.len() > MAX_PATH_BYTES
        || !(reference.starts_with("./") || reference.starts_with("../"))
        || reference
            .rsplit_once('.')
            .is_none_or(|(_, extension)| extension != "json")
    {
        return Err(graph_error(
            "configuration inheritance",
            format!("{file}: unsupported {reference}"),
        ));
    }
    let mut components = file
        .rsplit_once('/')
        .map_or_else(Vec::new, |(parent, _)| parent.split('/').collect());
    for component in reference.split('/') {
        match component {
            "." => {}
            ".." => {
                if components.pop().is_none() {
                    return Err(graph_error(
                        "configuration inheritance",
                        format!("{file}: reference escapes the captured graph"),
                    ));
                }
            }
            other => components.push(other),
        }
    }
    let relative = components.join("/");
    relative_path(&relative)?;
    Ok(relative)
}

// Exact census spelling is not proof of physical absence on a filesystem
// with case/normalization aliases. Reuse the nofollow directory owner and
// accept only the kernel's typed missing-leaf result, never an I/O fault.

fn validate_configuration(
    dir: &RetainedRoot,
    doc: &GraphDocument,
    file: &str,
    active: &mut BTreeSet<String>,
    checked: &mut BTreeSet<String>,
) -> Result<(), HostAppError> {
    if checked.contains(file) {
        return Ok(());
    }
    if active.len() >= MAX_COMPONENTS || !active.insert(file.to_owned()) {
        return Err(graph_error(
            "configuration inheritance",
            format!("{file}: cycle or depth exceeds 64"),
        ));
    }
    let config = read_json(dir, file)?;
    if !config.is_object() || config.pointer("/compilerOptions/baseUrl").is_some() {
        return Err(graph_error(
            "unqualified configuration",
            format!("{file}: non-object or unsupported baseUrl lookup"),
        ));
    }
    if let Some(paths) = config.pointer("/compilerOptions/paths") {
        let paths = paths
            .as_object()
            .ok_or_else(|| graph_error("unqualified configuration paths", file))?;
        if let Some(pattern) = paths
            .keys()
            .find(|pattern| reserved_configuration_mapping(pattern))
        {
            return Err(graph_error(
                "unapproved reserved tsconfig remapping",
                format!("{file}: {pattern}"),
            ));
        }
    }
    if let Some(inherited) = config.get("extends") {
        let references = match inherited {
            serde_json::Value::String(_) => std::slice::from_ref(inherited),
            serde_json::Value::Array(references) if references.len() <= MAX_COMPONENTS => {
                references.as_slice()
            }
            _ => {
                return Err(graph_error(
                    "configuration inheritance",
                    format!("{file}: require at most 64 explicit relative JSON references"),
                ));
            }
        };
        for reference in references {
            let reference = reference.as_str().ok_or_else(|| {
                graph_error(
                    "configuration inheritance",
                    format!("{file}: non-string reference"),
                )
            })?;
            let selected = configuration_reference(file, reference)?;
            if doc
                .files
                .binary_search_by(|row| row.path.as_str().cmp(selected.as_str()))
                .is_ok()
            {
                validate_configuration(dir, doc, &selected, active, checked)?;
            } else {
                missing_configuration_reference(dir, &selected)?;
            }
            // Missing in-graph relative build parents are preserved as data.
            // The pinned local Bun/current-snapshot resolve-only and inert
            // import proofs own acceptance of that observed case; no external
            // source or arbitrary config package is opened to fill the gap.
        }
    }
    active.remove(file);
    checked.insert(file.to_owned());
    Ok(())
}

fn validate_package_scopes(
    dir: &RetainedRoot,
    doc: &GraphDocument,
    packages: &serde_json::Map<String, serde_json::Value>,
) -> Result<(), HostAppError> {
    // Any actual installed package manifest must have a matching lock entry.
    // Source-internal src/package.json is not a second installed package.
    let mut checked_configs = BTreeSet::new();
    for row in &doc.files {
        if let Some(root) = installed_package_root(&row.path) {
            let manifest = read_json(dir, &row.path)?;
            let name = manifest
                .get("name")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| graph_error("installed package", format!("{root}: missing name")))?;
            if name == "electron"
                || (root.ends_with("/electron") && root != "node_modules/electron")
            {
                return Err(graph_error("nested foreign electron", root));
            }
            if PACKAGES.iter().any(|(canonical_root, canonical_name)| {
                name == *canonical_name && root != *canonical_root
            }) {
                return Err(graph_error("duplicate installed provider", root));
            }
            let version = manifest
                .get("version")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| {
                    graph_error("installed package", format!("{root}: missing version"))
                })?;
            let locked = packages
                .get(&root)
                .ok_or_else(|| graph_error("lock", format!("missing installed path {root}")))?;
            if locked.get("version").and_then(serde_json::Value::as_str) != Some(version) {
                return Err(graph_error("lock", format!("{root} version mismatch")));
            }
            if manifest.get("imports").is_some() {
                return Err(graph_error("unapproved package imports", root));
            }
        }
        if row.path == "package.json" || row.path.ends_with("/package.json") {
            let nested = read_json(dir, &row.path)?;
            if nested.get("imports").is_some() {
                return Err(graph_error(
                    "unapproved effective package imports",
                    &row.path,
                ));
            }
        }
        if row.path.ends_with("/tsconfig.json")
            || row.path == "tsconfig.json"
            || row.path.ends_with("/jsconfig.json")
            || row.path == "jsconfig.json"
        {
            validate_configuration(
                dir,
                doc,
                &row.path,
                &mut BTreeSet::new(),
                &mut checked_configs,
            )?;
        }
        if row.path.ends_with("/bunfig.toml")
            || row.path.ends_with("/.bunfig.toml")
            || row.path == "bunfig.toml"
            || row.path == ".bunfig.toml"
        {
            return Err(graph_error("unapproved bunfig remapping", &row.path));
        }
    }
    Ok(())
}

fn validate_updater_package(
    dir: &RetainedRoot,
    root_dependencies: &serde_json::Map<String, serde_json::Value>,
    locked_root: &serde_json::Map<String, serde_json::Value>,
    packages: &serde_json::Map<String, serde_json::Value>,
) -> Result<(), HostAppError> {
    if let Some(updater) = root_dependencies
        .get("electron-updater")
        .and_then(serde_json::Value::as_str)
    {
        if locked_root
            .get("electron-updater")
            .and_then(serde_json::Value::as_str)
            != Some(updater)
        {
            return Err(graph_error(
                "updater",
                "migrated updater alias differs from lock",
            ));
        }
        let installed = read_json(dir, "node_modules/electron-updater/package.json")?;
        if installed.get("name").and_then(serde_json::Value::as_str)
            != Some("@keld/electron-updater")
        {
            return Err(graph_error(
                "updater",
                "installed updater is not the inert adapter",
            ));
        }
        let version = installed
            .get("version")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| graph_error("updater", "installed updater version missing"))?;
        // validate_package_scopes already binds this canonical manifest's
        // version to the installed npm lock row; reuse that owner, not a parser.
        let local = first_party_file_archive(updater, version, "keld-electron-updater");
        if !updater.starts_with("npm:@keld/electron-updater@") && !local {
            return Err(graph_error(
                "updater",
                "foreign electron-updater is not approved",
            ));
        }
        if local {
            let locked = packages
                .get("node_modules/electron-updater")
                .ok_or_else(|| graph_error("updater", "installed updater lock row missing"))?;
            validate_local_archive_receipt(
                updater,
                locked,
                "updater resolved archive",
                "updater archive integrity",
            )?;
        }
    }
    Ok(())
}

fn validate_package_data(dir: &RetainedRoot, doc: &GraphDocument) -> Result<(), HostAppError> {
    let package = read_json(dir, "package.json")?;
    let lock = read_json(dir, "package-lock.json")?;
    if lock
        .get("lockfileVersion")
        .and_then(serde_json::Value::as_u64)
        != Some(3)
    {
        return Err(graph_error("lock", "require migrated npm lock v3"));
    }
    let root_dependencies = package
        .get("dependencies")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| graph_error("root package", "missing runtime dependencies"))?;
    let alias = root_dependencies
        .get("electron")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| graph_error("electron alias", "missing root electron dependency"))?;
    if package
        .get("devDependencies")
        .and_then(serde_json::Value::as_object)
        .is_some_and(|deps| deps.contains_key("electron"))
    {
        return Err(graph_error(
            "electron alias",
            "second dev dependency could select real Electron",
        ));
    }
    if let Some(scripts) = package
        .get("scripts")
        .and_then(serde_json::Value::as_object)
    {
        for (script, command) in scripts {
            let value = command.as_str().ok_or_else(|| {
                graph_error("package scripts", format!("{script}: non-string script"))
            })?;
            if value.contains("electron-builder")
                || value.split_whitespace().any(|arg| arg == "electron")
            {
                return Err(graph_error(
                    "package scripts",
                    format!("{script}: Electron binary not admitted"),
                ));
            }
        }
    }
    let packages = lock
        .get("packages")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| graph_error("lock", "missing packages map"))?;
    validate_electron_archive(alias, packages)?;
    let locked_root = packages
        .get("")
        .and_then(|root| root.get("dependencies"))
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| graph_error("lock", "missing root dependencies"))?;
    for (path, metadata) in packages {
        if path != "node_modules/electron"
            && (path.ends_with("/node_modules/electron")
                || (path.starts_with("node_modules/")
                    && metadata.get("name").and_then(serde_json::Value::as_str)
                        == Some("electron")))
        {
            return Err(graph_error("nested foreign electron", path));
        }
    }
    if locked_root
        .get("electron")
        .and_then(serde_json::Value::as_str)
        != Some(alias)
    {
        return Err(graph_error("lock", "root electron alias differs from lock"));
    }
    for (root, expected_name) in PACKAGES {
        let manifest = read_json(dir, &format!("{root}/package.json"))?;
        if manifest.get("name").and_then(serde_json::Value::as_str) != Some(expected_name) {
            return Err(graph_error(
                "electron/provider package identity",
                format!("{root} is not {expected_name}"),
            ));
        }
        if manifest.get("version").and_then(serde_json::Value::as_str)
            != packages
                .get(root)
                .and_then(|row| row.get("version"))
                .and_then(serde_json::Value::as_str)
        {
            return Err(graph_error("lock", format!("{root} version differs")));
        }
    }
    validate_package_scopes(dir, doc, packages)?;
    if package.get("imports").is_some() {
        return Err(graph_error("unapproved root package imports", "imports"));
    }
    validate_updater_package(dir, root_dependencies, locked_root, packages)?;
    Ok(())
}

fn validate_document(
    root: &Path,
    opened: &RetainedRoot,
    document: &GraphDocument,
) -> Result<(), HostAppError> {
    check_files(root, opened, document)?;
    validate_package_data(opened, document)?;
    Ok(())
}

fn admit_graph_against_expected(
    root: &Path,
    retained_root: RetainedRoot,
    leaf_bytes: &[u8],
    expected: &ApprovedGraphCaptureInput,
) -> Result<AdmittedStageGraph, HostAppError> {
    let graph = validate_graph_shape(leaf_bytes)?;
    let identity = root_identity(root, &retained_root)?;
    validate_document(root, &retained_root, &graph)?;
    let root_package =
        open_relative_file(&retained_root, Path::new("package.json"), "graph package")?;
    let root_lock =
        open_relative_file(&retained_root, Path::new("package-lock.json"), "graph lock")?;
    if read_hash(root_package)?.0 != expected.expected_package_sha256
        || read_hash(root_lock)?.0 != expected.expected_lock_sha256
    {
        return Err(graph_error(
            "approved package/lock",
            "external source receipt differs",
        ));
    }
    verify_provider_rows(&graph.files, &expected.expected_package_trees)?;
    let result = AdmittedStageGraph {
        root: root.to_owned(),
        retained_root,
        graph_bytes: leaf_bytes.to_vec(),
        graph_sha256: graph_hash(leaf_bytes),
        graph,
        root_identity: identity,
        selected_source: None,
        selected_file_identities: None,
        #[cfg(windows)]
        windows_runtime_entry: None,
    };
    // Preserve the exact retained root against replacement during final
    // source/provider admission, including the last digest calculation.
    if root_identity(&result.root, &result.retained_root)? != result.root_identity {
        return Err(graph_error("retained root", "source root changed"));
    }
    Ok(result)
}

fn parse_bound_graph(
    leaf_bytes: &[u8],
    expected_graph_sha256: [u8; 32],
    selected_entry: &Path,
) -> Result<GraphDocument, HostAppError> {
    if leaf_bytes.len() > MAX_GRAPH_BYTES {
        return Err(graph_error(
            "graph size",
            "8 MiB exact graph limit exceeded",
        ));
    }
    if graph_hash(leaf_bytes) != expected_graph_sha256 {
        return Err(graph_error(
            "graph digest",
            "boot descriptor graph hash differs",
        ));
    }
    let graph = validate_graph_shape(leaf_bytes)?;
    if Path::new(&graph.entry) != selected_entry {
        return Err(graph_error(
            "graph entry",
            "selected boot entry differs from bound graph entry",
        ));
    }
    Ok(graph)
}

fn bind_bound_graph(
    root: &Path,
    selected_root: &RetainedRoot,
    leaf_bytes: &[u8],
    expected_graph_sha256: [u8; 32],
    graph: GraphDocument,
) -> Result<AdmittedStageGraph, HostAppError> {
    let identity = root_identity(root, selected_root)?;
    validate_document(root, selected_root, &graph)?;
    let retained_root = clone_root(selected_root)?;
    let result = AdmittedStageGraph {
        root: root.to_owned(),
        retained_root,
        graph_bytes: leaf_bytes.to_vec(),
        graph_sha256: expected_graph_sha256,
        graph,
        root_identity: identity,
        selected_source: None,
        selected_file_identities: None,
        #[cfg(windows)]
        windows_runtime_entry: None,
    };
    if root_identity(&result.root, &result.retained_root)? != result.root_identity {
        return Err(graph_error("retained root", "bound root changed"));
    }
    Ok(result)
}

fn capture_graph_rows(
    root_path: &Path,
    root_fd: &RetainedRoot,
    namespace_prefix: Option<&str>,
) -> Result<Vec<GraphFile>, HostAppError> {
    let before = census_subtree(root_path, root_fd, namespace_prefix)?;
    let rows = before
        .iter()
        .map(|observed| {
            let path = &observed.path;
            let file = open_relative_file(root_fd, &relative_path(path)?, "graph source")?;
            let metadata =
                file_metadata(&file).map_err(|source| graph_error("graph source", source))?;
            if !same_identity(&metadata, &observed.metadata) {
                return Err(graph_error("graph source census", path));
            }
            let held = file
                .try_clone()
                .map_err(|source| graph_error("graph source", source))?;
            let (digest, size) = read_hash(file)?;
            let after =
                file_metadata(&held).map_err(|source| graph_error("graph source", source))?;
            let current = path_metadata(root_path.join(path))
                .map_err(|source| graph_error("graph source", source))?;
            if !same_identity(&current, &observed.metadata)
                || !same_identity(&after, &observed.metadata)
            {
                return Err(graph_error("graph source census", path));
            }
            Ok(GraphFile {
                path: namespace_prefix
                    .map_or_else(|| path.clone(), |prefix| format!("{prefix}/{path}")),
                size,
                sha256: sha_text(&digest),
            })
        })
        .collect::<Result<Vec<_>, HostAppError>>()?;
    let after = census_subtree(root_path, root_fd, namespace_prefix)?;
    if after.len() != before.len()
        || !after.iter().zip(&before).all(|(after, before)| {
            after.path == before.path && same_identity(&after.metadata, &before.metadata)
        })
    {
        return Err(graph_error(
            "graph source census",
            "inventory changed during capture",
        ));
    }
    Ok(rows)
}

fn make_graph_leaf_on_retained_root(
    root_path: &Path,
    root_fd: &RetainedRoot,
    main_entry: &str,
) -> Result<Vec<u8>, HostAppError> {
    relative_path(main_entry)?;
    let identity = root_identity(root_path, root_fd)?;
    let rows = capture_graph_rows(root_path, root_fd, None)?;
    let graph = GraphDocument {
        schema: GRAPH_SCHEMA.to_owned(),
        entry: main_entry.to_owned(),
        files: rows,
    };
    let bytes = serde_json::to_vec(&graph).map_err(|source| graph_error("graph encode", source))?;
    if root_identity(root_path, root_fd)? != identity {
        return Err(graph_error(
            "graph source",
            "physical root changed during leaf capture",
        ));
    }
    Ok(bytes)
}

/// Compile the existing closed graph-v1 over Pack's *already copied* app
/// directory. The root path is supplied by Pack, paired with its borrowed,
/// retained no-follow directory handle; neither is selected by app JavaScript.
///
/// The expected source/provider hashes must originate from a separate reviewed
/// input receipt, never from hashes computed from this target app directory.
/// This data-only operation writes nothing and produces no native capability.
///
/// # Errors
/// Refuses physical root replacement, census drift, missing selected source,
/// root package/lock mismatch, or any of three provider fingerprint mismatches.
pub fn make_graph_leaf_from_retained_root(
    root_path: &Path,
    root_fd: &File,
    expected: &ApprovedGraphCaptureInput,
    selected: &SelectedRendererSource,
    main_entry: &str,
) -> Result<Vec<u8>, HostAppError> {
    let retained = borrow_retained_root(root_path, root_fd)?;
    let bytes = make_graph_leaf_on_retained_root(root_path, &retained, main_entry)?;
    let mut admitted = admit_graph_against_expected(root_path, retained, &bytes, expected)?;
    admitted.bind_selected_source(selected)?;
    if root_identity(&admitted.root, &admitted.retained_root)? != admitted.root_identity {
        return Err(graph_error(
            "retained root",
            "source changed after qualification",
        ));
    }
    Ok(bytes)
}

/// Verify the three canonical provider trees before a development stage exists.
///
/// The caller supplies independently qualified framework receipts. This operation
/// reads only the fixed provider subtrees through the retained project root and
/// grants no source, stage, policy, or native authority. The copied stage must
/// still pass the complete graph compiler before launch.
///
/// # Errors
/// Refuses absent or substituted providers, namespace/census drift, unserviceable
/// files, or any provider digest that differs from its independent receipt.
pub fn verify_provider_trees_from_retained_root(
    root_path: &Path,
    root_fd: &File,
    expected: &[[u8; 32]; 3],
) -> Result<(), HostAppError> {
    let retained = borrow_retained_root(root_path, root_fd)?;
    let identity = root_identity(root_path, &retained)?;
    let mut rows = Vec::new();
    for (prefix, _) in PACKAGES {
        let directory = census_directory(&retained, prefix)?;
        let provider_root = root_path.join(prefix);
        let provider = borrow_retained_root(&provider_root, &directory)?;
        rows.extend(capture_graph_rows(&provider_root, &provider, Some(prefix))?);
        if rows.len() > MAX_FILES {
            return Err(graph_error(
                "approved package artifact",
                "provider census exceeds graph limit",
            ));
        }
    }
    rows.sort_by(|left, right| left.path.cmp(&right.path));
    validate_graph_files(&rows)?;
    verify_provider_rows(&rows, expected)?;
    if root_identity(root_path, &retained)? != identity {
        return Err(graph_error(
            "approved package artifact",
            "retained project root changed",
        ));
    }
    Ok(())
}

impl AdmittedStageGraph {
    /// Exact serialized graph digest bound by the installed boot descriptor.
    #[must_use]
    pub fn graph_sha256(&self) -> [u8; 32] {
        self.graph_sha256
    }

    fn checked_selected_member_identity(
        &self,
        relative: &Path,
    ) -> Result<ObjectMetadata, HostAppError> {
        let file = open_relative_file(&self.retained_root, relative, "selected graph member")?;
        let opened =
            file_metadata(&file).map_err(|source| graph_error("selected graph member", source))?;
        let named = path_metadata(self.root.join(relative))
            .map_err(|source| graph_error("selected graph member", source))?;
        if !opened.is_file() || !same_identity(&opened, &named) {
            return Err(graph_error(
                "selected graph member",
                "selected inode or physical metadata differs",
            ));
        }
        Ok(opened)
    }

    fn bind_selected_source(
        &mut self,
        selected: &SelectedRendererSource,
    ) -> Result<(), HostAppError> {
        if self.selected_source.is_some() {
            return Err(graph_error("selected source", "already selected"));
        }
        verify_selected_rows(&self.graph, selected)?;
        let entry_identity = self.checked_selected_member_identity(&selected.full_entry())?;
        let preload_identity = self.checked_selected_member_identity(selected.preload_path())?;
        self.selected_file_identities = Some((entry_identity, preload_identity));
        self.selected_source = Some(selected.clone());
        Ok(())
    }

    fn recheck_selected_file_identities(&self) -> Result<(), HostAppError> {
        let Some(selected) = self.selected_source.as_ref() else {
            return Ok(());
        };
        let Some((entry_identity, preload_identity)) = self.selected_file_identities.as_ref()
        else {
            return Err(graph_error(
                "selected source",
                "bound selected file identities absent",
            ));
        };
        let now_entry = self.checked_selected_member_identity(&selected.full_entry())?;
        let now_preload = self.checked_selected_member_identity(selected.preload_path())?;
        if !same_identity(&now_entry, entry_identity)
            || !same_identity(&now_preload, preload_identity)
        {
            return Err(graph_error(
                "selected source",
                "selected file replaced even if bytes match",
            ));
        }
        Ok(())
    }

    /// Reads the exact boot-selected preload before native document admission.
    ///
    /// # Errors
    /// Refuses any alternate absolute selector, byte mismatch, inode swap,
    /// missing graph row, nonregular leaf or preload larger than 512 KiB.
    pub fn read_selected_preload_bytes(
        &self,
        absolute_selector: &Path,
    ) -> Result<Vec<u8>, HostAppError> {
        let selected_source = self.selected_source()?;
        let selected = absolute_selector
            .to_str()
            .ok_or_else(|| graph_error("preload selector", "non UTF-8 selector"))?;
        let expected_selector = self.root.join(selected_source.preload_path());
        let exact_selector = absolute_selector == expected_selector;
        #[cfg(windows)]
        let exact_selector = {
            use std::path::{Component, Prefix};

            // The Host retains a verbatim root. Bun supplies an ordinary drive
            // path and the original app can append the leaf with one slash.
            // Compare both literal spellings to the trusted member. Never
            // resolve the caller's path or normalize its parent directory.
            exact_selector
                || matches!(
                    expected_selector.components().next(),
                    Some(Component::Prefix(prefix))
                        if matches!(prefix.kind(), Prefix::VerbatimDisk(_))
                ) && {
                    let ordinary = keld_runtime::primary::strip_verbatim_prefix(&expected_selector);
                    absolute_selector.as_os_str() == ordinary.as_os_str()
                        || ordinary
                            .to_str()
                            .and_then(|path| path.rsplit_once('\\'))
                            .zip(selected.rsplit_once('/'))
                            .is_some_and(|(expected, supplied)| expected == supplied)
                }
        };
        if selected.len() > 2_048
            || !absolute_selector.is_absolute()
            || selected.contains('\0')
            || !exact_selector
        {
            return Err(graph_error(
                "preload selector",
                "not the exact sealed boot-selected preload member",
            ));
        }
        self.recheck()?;
        self.read_bound_preload_member(selected_source)
    }

    fn read_bound_preload_member(
        &self,
        selected_source: &SelectedRendererSource,
    ) -> Result<Vec<u8>, HostAppError> {
        let preload_path = selected_source
            .preload_path()
            .to_str()
            .ok_or_else(|| graph_error("preload member", "non UTF-8 preload"))?;
        let row = self
            .graph
            .files
            .binary_search_by(|row| row.path.as_str().cmp(preload_path))
            .map(|index| &self.graph.files[index])
            .map_err(|_| graph_error("preload member", "not in the bound graph"))?;
        if row.size > MAX_SELECTED_PRELOAD_BYTES as u64
            || parse_sha(&row.sha256)? != selected_source.preload_sha256()
        {
            return Err(graph_error(
                "preload member",
                "digest or size differs from boot pin",
            ));
        }
        let file = open_relative_file(
            &self.retained_root,
            selected_source.preload_path(),
            "selected preload",
        )?;
        let before =
            file_metadata(&file).map_err(|source| graph_error("preload metadata", source))?;
        if !before.is_file() || before.len() != row.size {
            return Err(graph_error(
                "preload metadata",
                "preload is changed or not regular",
            ));
        }
        if self
            .selected_file_identities
            .as_ref()
            .is_none_or(|(_, bound)| !same_identity(&before, bound))
        {
            return Err(graph_error(
                "preload identity",
                "opened file differs from retained selection",
            ));
        }
        let retained_file = file
            .try_clone()
            .map_err(|source| graph_error("preload identity", source))?;
        let bytes = read_bounded(file, MAX_SELECTED_PRELOAD_BYTES, "selected preload")?;
        let after = file_metadata(&retained_file)
            .map_err(|source| graph_error("preload identity", source))?;
        let from_path = path_metadata(self.root.join(selected_source.preload_path()))
            .map_err(|source| graph_error("preload identity", source))?;
        let digest: [u8; 32] = Sha256::digest(&bytes).into();
        if bytes.len() as u64 != row.size
            || digest != selected_source.preload_sha256()
            || !same_identity(&before, &after)
            || !same_identity(&before, &from_path)
        {
            return Err(graph_error("preload identity", "selected preload changed"));
        }
        Ok(bytes)
    }

    pub(super) fn selected_source(&self) -> Result<&SelectedRendererSource, HostAppError> {
        self.selected_source
            .as_ref()
            .ok_or_else(|| graph_error("selected source", "no source was bound"))
    }

    /// Revalidates current members against the retained graph and selected roots.
    ///
    /// # Errors
    /// Refuses changed roots, selected identities, graph bytes or resource members.
    pub fn recheck(&self) -> Result<(), HostAppError> {
        let current = root_identity(&self.root, &self.retained_root)?;
        if current != self.root_identity {
            return Err(graph_error("retained stage", "root identity changed"));
        }
        let file = open_relative_file(&self.retained_root, Path::new(GRAPH_FILE), "graph leaf")?;
        let bytes = read_bounded(file, MAX_GRAPH_BYTES, "graph size")?;
        if bytes != self.graph_bytes || graph_hash(&bytes) != self.graph_sha256 {
            return Err(graph_error("graph digest", "selected leaf changed"));
        }
        validate_document(&self.root, &self.retained_root, &self.graph)?;
        if let Some(selected) = self.selected_source.as_ref() {
            verify_selected_rows(&self.graph, selected)?;
        }
        self.recheck_selected_file_identities()?;
        Ok(())
    }
}

impl keld_runtime::primary::RoleSourceRecheck for AdmittedStageGraph {
    fn recheck(&self) -> std::io::Result<()> {
        // The role preparer checks this same graph before every generation.
        AdmittedStageGraph::recheck(self).map_err(std::io::Error::other)
    }
}

fn verify_provider_rows(rows: &[GraphFile], expected: &[[u8; 32]; 3]) -> Result<(), HostAppError> {
    for (index, (prefix, _)) in PACKAGES.iter().enumerate() {
        if package_tree_fingerprint(rows, prefix)? != expected[index] {
            return Err(graph_error(
                "approved package artifact",
                format!("{prefix}: external provider identity differs"),
            ));
        }
    }
    Ok(())
}

fn package_tree_fingerprint(files: &[GraphFile], prefix: &str) -> Result<[u8; 32], HostAppError> {
    let mut hasher = Sha256::new();
    let mut matched = 0_usize;
    for row in files {
        let Some(relative) = row
            .path
            .strip_prefix(prefix)
            .and_then(|rest| rest.strip_prefix('/'))
        else {
            continue;
        };
        matched += 1;
        hasher.update(relative.as_bytes());
        hasher.update([0]);
        hasher.update(row.size.to_string().as_bytes());
        hasher.update([0]);
        hasher.update(parse_sha(&row.sha256)?);
        hasher.update([0]);
    }
    if matched == 0 {
        return Err(graph_error(
            "approved package artifact",
            format!("empty {prefix}"),
        ));
    }
    Ok(hasher.finalize().into())
}

#[cfg(test)]
mod tests;

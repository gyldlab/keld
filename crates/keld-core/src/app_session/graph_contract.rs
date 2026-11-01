//! GH499's macOS first-proof data-only graph admission.
//!
//! The graph never evaluates source, runs a package manager, or creates a grant.
//! The CLI supplies independent GH502 package receipts before it publishes a
//! graph; the Host and Guardian consume only the same owner-private stage.
//! This is a cooperating-developer dev-stage integrity check, not signed code.

use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::Read as _;
use std::os::fd::AsFd as _;
use std::os::fd::OwnedFd;
use std::os::unix::fs::MetadataExt as _;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use super::{HostAppError, open_relative_file, open_root, read_bounded, target_error};

pub const GRAPH_FILE: &str = "keld.stage-graph.json";
pub const MAX_GRAPH_BYTES: usize = 8 * 1024 * 1024;
const MAX_JSON_BYTES: usize = 4 * 1024 * 1024;
const MAX_FILES: usize = 32_768;
const MAX_ENTRIES: usize = 65_536;
const MAX_COMPONENTS: usize = 64;
const MAX_PATH_BYTES: usize = 4_096;
const BUFFER_BYTES: usize = 64 * 1024;
const GRAPH_SCHEMA: &str = "keld.stage.graph/v1";

const PACKAGES: [(&str, &str); 3] = [
    ("node_modules/electron", "@keld/electron"),
    ("node_modules/@keld/api", "@keld/api"),
    ("node_modules/@keld/kipc", "@keld/kipc"),
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct GraphDocument {
    schema: String,
    entry: String,
    files: Vec<GraphFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct GraphFile {
    path: String,
    size: u64,
    sha256: String,
}

/// Independently pinned input to the explicit GH589 first-proof staging caller.
/// Its values must be obtained from reviewed source/lock and the actual
/// independently packed and extracted GH502/API/KIPC provider artifacts.
/// This type is not an application configuration or user-authored grant.
#[derive(Clone, Debug)]
pub struct ApprovedGraphCaptureInput {
    pub expected_package_sha256: [u8; 32],
    pub expected_lock_sha256: [u8; 32],
    /// In PACKAGES order: the canonical *unpacked* subtree fingerprint of
    /// electron, @keld/api, and @keld/kipc, including all regular files.
    pub expected_package_trees: [[u8; 32]; 3],
}

#[derive(Debug)]
pub struct AdmittedStageGraph {
    root: PathBuf,
    retained_root: OwnedFd,
    graph_bytes: Vec<u8>,
    graph_sha256: [u8; 32],
    graph: GraphDocument,
    root_identity: (u64, u64),
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

fn parse_sha(value: &str) -> Result<[u8; 32], HostAppError> {
    let hex = value
        .strip_prefix("sha256:")
        .ok_or_else(|| graph_error("digest", "missing sha256: prefix"))?;
    if hex.len() != 64
        || !hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(graph_error(
            "digest",
            "expected 64 lowercase hexadecimal digits",
        ));
    }
    let mut result = [0_u8; 32];
    for (index, pair) in hex.as_bytes().as_chunks::<2>().0.iter().enumerate() {
        let text = std::str::from_utf8(pair).map_err(|source| graph_error("digest", source))?;
        result[index] =
            u8::from_str_radix(text, 16).map_err(|source| graph_error("digest", source))?;
    }
    Ok(result)
}

/// Apply the single GH499 component/length grammar at both CLI capture and
/// Host/Guardian stage readback; an app cannot introduce a second path reader.
pub fn validate_graph_relative_path(input: &str) -> Result<(), HostAppError> {
    relative_path(input).map(|_| ())
}

fn relative_path(path: &str) -> Result<PathBuf, HostAppError> {
    if path.len() > MAX_PATH_BYTES
        || path.is_empty()
        || path.contains('\\')
        || path.contains('\0')
        || path.contains(':')
    {
        return Err(graph_error("path", "invalid length or forbidden character"));
    }
    let components = path.split('/').collect::<Vec<_>>();
    if components.len() > MAX_COMPONENTS
        || components
            .iter()
            .any(|name| name.is_empty() || *name == "." || *name == "..")
    {
        return Err(graph_error("path", "too many components or traversal"));
    }
    let parsed = Path::new(path);
    if parsed
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(graph_error("path", "not project-relative"));
    }
    Ok(parsed.to_owned())
}

fn excluded(relative: &str) -> bool {
    matches!(
        relative,
        "keld-host" | "keld.boot.json" | "keld.permissions.jsonc" | GRAPH_FILE
    )
}

struct ObservedFile {
    path: String,
    metadata: fs::Metadata,
}

fn same_identity(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    left.dev() == right.dev()
        && left.ino() == right.ino()
        && left.mode() == right.mode()
        && left.len() == right.len()
        && left.mtime() == right.mtime()
        && left.mtime_nsec() == right.mtime_nsec()
        && left.ctime() == right.ctime()
        && left.ctime_nsec() == right.ctime_nsec()
}

fn census_directory(root: &OwnedFd, relative: &str) -> Result<File, HostAppError> {
    use nix::fcntl::{OFlag, openat};
    use nix::sys::stat::Mode;

    let mut retained = File::from(
        root.as_fd()
            .try_clone_to_owned()
            .map_err(|source| graph_error("directory census", source))?,
    );
    if !relative.is_empty() {
        for component in Path::new(relative).components() {
            let Component::Normal(name) = component else {
                return Err(graph_error("directory census", "nonportable source path"));
            };
            retained = File::from(
                openat(
                    retained.as_fd(),
                    name,
                    OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_CLOEXEC | OFlag::O_NOFOLLOW,
                    Mode::empty(),
                )
                .map_err(|source| graph_error("directory census", source))?,
            );
        }
    }
    Ok(retained)
}

fn check_census_directory(
    directory: &Path,
    selected: &File,
    expected: &fs::Metadata,
) -> Result<(), HostAppError> {
    let from_fd = selected
        .metadata()
        .map_err(|source| graph_error("directory census", source))?;
    let from_path = fs::symlink_metadata(directory)
        .map_err(|source| graph_error("directory census", source))?;
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

fn census(root: &Path, retained: &OwnedFd) -> Result<Vec<ObservedFile>, HostAppError> {
    let root_meta = File::from(
        retained
            .as_fd()
            .try_clone_to_owned()
            .map_err(|source| graph_error("directory census", source))?,
    )
    .metadata()
    .map_err(|source| graph_error("directory census", source))?;
    let mut pending = vec![(root.to_owned(), String::new(), root_meta)];
    let mut files = Vec::new();
    let mut directories = Vec::new();
    let mut entries = 0_usize;
    while let Some((directory, prefix, expected_dir)) = pending.pop() {
        let held = census_directory(retained, &prefix)?;
        check_census_directory(&directory, &held, &expected_dir)?;
        let mut children = fs::read_dir(&directory)
            .map_err(|source| graph_error("directory census", source))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|source| graph_error("directory census", source))?;
        children.sort_by_key(fs::DirEntry::file_name);
        for child in children {
            entries = entries
                .checked_add(1)
                .ok_or_else(|| graph_error("census", "entry overflow"))?;
            if entries > MAX_ENTRIES {
                return Err(graph_error("census", "more than 65536 filesystem entries"));
            }
            let name = child
                .file_name()
                .into_string()
                .map_err(|_| graph_error("path", "non UTF-8 source path"))?;
            let relative = if prefix.is_empty() {
                name
            } else {
                format!("{prefix}/{name}")
            };
            relative_path(&relative)?;
            if relative == "keld.config.ts"
                || relative == ".git"
                || relative == ".keld"
                || relative.starts_with(".git/")
                || relative.starts_with(".keld/")
            {
                return Err(graph_error("forbidden input", relative));
            }
            let metadata = fs::symlink_metadata(child.path())
                .map_err(|source| graph_error("source type", source))?;
            let ty = metadata.file_type();
            if ty.is_symlink() {
                return Err(graph_error("symlink", relative));
            }
            if ty.is_dir() {
                directories.push(relative.clone());
                pending.push((child.path(), relative, metadata));
            } else if ty.is_file() {
                if !excluded(&relative) {
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

fn validate_graph_shape(bytes: &[u8]) -> Result<GraphDocument, HostAppError> {
    if bytes.len() > MAX_GRAPH_BYTES {
        return Err(graph_error(
            "graph size",
            "8 MiB exact graph limit exceeded",
        ));
    }
    let doc: GraphDocument =
        serde_json::from_slice(bytes).map_err(|source| graph_error("closed graph JSON", source))?;
    if doc.schema != GRAPH_SCHEMA {
        return Err(graph_error("graph schema", "expected keld.stage.graph/v1"));
    }
    relative_path(&doc.entry)?;
    if doc.files.len() > MAX_FILES || doc.files.is_empty() {
        return Err(graph_error("census", "empty or more than 32768 files"));
    }
    let mut previous = None::<String>;
    let mut folded = BTreeSet::new();
    for row in &doc.files {
        relative_path(&row.path)?;
        if excluded(&row.path) {
            return Err(graph_error("reserved file", &row.path));
        }
        if previous
            .as_deref()
            .is_some_and(|prev| prev >= row.path.as_str())
        {
            return Err(graph_error("graph ordering", "unsorted or duplicate paths"));
        }
        if !folded.insert(row.path.to_lowercase()) {
            return Err(graph_error("graph ordering", "case-colliding paths"));
        }
        parse_sha(&row.sha256)?;
        previous = Some(row.path.clone());
    }
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

fn check_files(root: &Path, dir: &OwnedFd, doc: &GraphDocument) -> Result<(), HostAppError> {
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
        let metadata = file
            .metadata()
            .map_err(|source| graph_error("file metadata", source))?;
        let from_path = fs::symlink_metadata(root.join(&row.path))
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
        let after_fd = held
            .metadata()
            .map_err(|source| graph_error("file metadata", source))?;
        let after_path = fs::symlink_metadata(root.join(&row.path))
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

fn read_json(dir: &OwnedFd, file: &str) -> Result<serde_json::Value, HostAppError> {
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

/// Only the GH502 first-party packed Electron artifact is admissible as a
/// local file dependency. Real npm offline installs record an absolute
/// `file:/.../keld-electron-VERSION.tgz` value; no filesystem target is opened
/// or trusted here. Its externally pinned package and lock source bytes,
/// installed @keld/electron manifest and all three independently approved
/// packed-provider tree digests still govern admission.
fn first_party_file_electron_archive(alias: &str, version: &str) -> bool {
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
    segments.rsplit('/').next() == Some(format!("keld-electron-{version}.tgz").as_str())
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
    let local_first_party = first_party_file_electron_archive(alias, version);
    if !alias.starts_with("npm:@keld/electron@") && !local_first_party {
        return Err(graph_error(
            "electron alias",
            "root electron must be @keld/electron or independently pinned first-party local archive",
        ));
    }
    if local_first_party {
        if installed_electron
            .get("resolved")
            .and_then(serde_json::Value::as_str)
            != Some(alias)
        {
            return Err(graph_error(
                "electron resolved archive",
                "installed Electron lock resolution differs from approved root archive",
            ));
        }
        if !installed_electron
            .get("integrity")
            .and_then(serde_json::Value::as_str)
            .is_some_and(first_party_sha512_integrity)
        {
            return Err(graph_error(
                "electron archive integrity",
                "missing strict SHA-512 archive receipt",
            ));
        }
    }
    Ok(())
}

fn validate_package_scopes(
    dir: &OwnedFd,
    doc: &GraphDocument,
    packages: &serde_json::Map<String, serde_json::Value>,
) -> Result<(), HostAppError> {
    // Any actual installed package manifest must have a matching lock entry.
    // Source-internal src/package.json is not a second installed package.
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
            let config = read_json(dir, &row.path)?;
            if config.get("extends").is_some() || config.pointer("/compilerOptions/paths").is_some()
            {
                return Err(graph_error("unapproved tsconfig remapping", &row.path));
            }
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
    dir: &OwnedFd,
    root_dependencies: &serde_json::Map<String, serde_json::Value>,
    locked_root: &serde_json::Map<String, serde_json::Value>,
) -> Result<(), HostAppError> {
    if let Some(updater) = root_dependencies
        .get("electron-updater")
        .and_then(serde_json::Value::as_str)
    {
        if !updater.starts_with("npm:@keld/electron-updater@") {
            return Err(graph_error(
                "updater",
                "foreign electron-updater is not approved",
            ));
        }
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
    }
    Ok(())
}

fn validate_package_data(dir: &OwnedFd, doc: &GraphDocument) -> Result<(), HostAppError> {
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
    validate_updater_package(dir, root_dependencies, locked_root)?;
    Ok(())
}

fn root_identity(path: &Path, opened: &OwnedFd) -> Result<(u64, u64), HostAppError> {
    let retained = File::from(
        opened
            .as_fd()
            .try_clone_to_owned()
            .map_err(|source| graph_error("retained stage", source))?,
    );
    let held = retained
        .metadata()
        .map_err(|source| graph_error("retained stage", source))?;
    let current =
        fs::symlink_metadata(path).map_err(|source| graph_error("retained stage", source))?;
    if !current.is_dir() || current.dev() != held.dev() || current.ino() != held.ino() {
        return Err(graph_error("retained stage", "root was substituted"));
    }
    Ok((current.dev(), current.ino()))
}

fn validate_document(
    root: &Path,
    opened: &OwnedFd,
    document: &GraphDocument,
) -> Result<(), HostAppError> {
    check_files(root, opened, document)?;
    validate_package_data(opened, document)?;
    Ok(())
}

/// One producer-only data admission. The expected three subtree fingerprints
/// originate from independently approved GH502/API/KIPC installed artifacts.
pub fn admit_stage_graph(
    root: &Path,
    leaf_bytes: &[u8],
    expected: &ApprovedGraphCaptureInput,
) -> Result<AdmittedStageGraph, HostAppError> {
    let graph = validate_graph_shape(leaf_bytes)?;
    let retained_root = open_root(root)?;
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
    for (index, (prefix, _)) in PACKAGES.iter().enumerate() {
        if package_tree_fingerprint(&graph, prefix)? != expected.expected_package_trees[index] {
            return Err(graph_error(
                "approved package artifact",
                format!("{prefix}: external provider identity differs"),
            ));
        }
    }
    Ok(AdmittedStageGraph {
        root: root.to_owned(),
        retained_root,
        graph_bytes: leaf_bytes.to_vec(),
        graph_sha256: graph_hash(leaf_bytes),
        graph,
        root_identity: identity,
    })
}

/// Guardian uses this after its private registration authenticated; the Host
/// uses the identical nofollow read as part of its validated stage selection.
pub fn read_bound_graph_leaf(
    root: &Path,
    expected_sha256: [u8; 32],
    selected_entry: &Path,
) -> Result<AdmittedStageGraph, HostAppError> {
    let dir = open_root(root)?;
    read_bound_graph_leaf_from_retained(root, &dir, expected_sha256, selected_entry)
}

/// Host binds the graph to the descriptor, entry and permissions read from
/// its *existing* retained stage-root object; the Guardian passes its own
/// freshly selected retained root after authenticated registration.
pub fn read_bound_graph_leaf_from_retained(
    root: &Path,
    dir: &OwnedFd,
    expected_sha256: [u8; 32],
    selected_entry: &Path,
) -> Result<AdmittedStageGraph, HostAppError> {
    let file = open_relative_file(&dir, Path::new(GRAPH_FILE), "graph leaf")?;
    let bytes = read_bounded(file, MAX_GRAPH_BYTES, "graph size")?;
    validate_bound_graph(root, dir, &bytes, expected_sha256, selected_entry)
}

/// One validator shared by Host and Guardian, tied to the exact retained
/// directory already used to read the descriptor or private graph leaf.
fn validate_bound_graph(
    root: &Path,
    selected_root: &OwnedFd,
    leaf_bytes: &[u8],
    expected_graph_sha256: [u8; 32],
    selected_entry: &Path,
) -> Result<AdmittedStageGraph, HostAppError> {
    if leaf_bytes.len() > MAX_GRAPH_BYTES {
        return Err(graph_error(
            "graph size",
            "8 MiB exact graph limit exceeded",
        ));
    }
    if graph_hash(leaf_bytes) != expected_graph_sha256 {
        return Err(graph_error(
            "graph digest",
            "descriptor/Guardian graph hash differs",
        ));
    }
    let graph = validate_graph_shape(leaf_bytes)?;
    if Path::new(&graph.entry) != selected_entry {
        return Err(graph_error(
            "graph entry",
            "selected boot/Guardian entry differs from bound graph entry",
        ));
    }
    let identity = root_identity(root, selected_root)?;
    validate_document(root, selected_root, &graph)?;
    let retained_root = selected_root
        .as_fd()
        .try_clone_to_owned()
        .map_err(|source| graph_error("retained stage root clone", source))?;
    Ok(AdmittedStageGraph {
        root: root.to_owned(),
        retained_root,
        graph_bytes: leaf_bytes.to_vec(),
        graph_sha256: expected_graph_sha256,
        graph,
        root_identity: identity,
    })
}

/// Before any full source capture, verify every actual installed first-party
/// package against three independently approved packed/extracted artifacts.
/// These values are external to the app-authored manifest and the later graph.
/// No package entry, script, or executable is imported or evaluated.
pub fn verify_approved_source_inputs(
    project: &Path,
    expected: &ApprovedGraphCaptureInput,
) -> Result<(), HostAppError> {
    let selected_root = open_root(project)?;
    let package = open_relative_file(&selected_root, Path::new("package.json"), "source package")?;
    let lock = open_relative_file(
        &selected_root,
        Path::new("package-lock.json"),
        "source lock",
    )?;
    if package
        .metadata()
        .map_err(|source| graph_error("source package size", source))?
        .len()
        > MAX_JSON_BYTES as u64
        || lock
            .metadata()
            .map_err(|source| graph_error("source lock size", source))?
            .len()
            > MAX_JSON_BYTES as u64
    {
        return Err(graph_error(
            "approved source JSON",
            "4 MiB package/lock cap exceeded",
        ));
    }
    if read_hash(package)?.0 != expected.expected_package_sha256
        || read_hash(lock)?.0 != expected.expected_lock_sha256
    {
        return Err(graph_error(
            "approved source package/lock",
            "external digest differs",
        ));
    }
    for (index, (prefix, _)) in PACKAGES.iter().enumerate() {
        let mut pending = vec![(project.join(prefix), prefix.to_string())];
        let mut rows = Vec::new();
        let mut entries = 0_usize;
        while let Some((directory, relative)) = pending.pop() {
            let mut children = fs::read_dir(&directory)
                .map_err(|source| graph_error("source provider directory", source))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|source| graph_error("source provider directory", source))?;
            children.sort_by_key(fs::DirEntry::file_name);
            for child in children {
                entries = entries
                    .checked_add(1)
                    .ok_or_else(|| graph_error("source provider census", "overflow"))?;
                if entries > MAX_ENTRIES {
                    return Err(graph_error("source provider census", "65536 entry limit"));
                }
                let name = child
                    .file_name()
                    .into_string()
                    .map_err(|_| graph_error("source provider path", "non UTF-8"))?;
                let next = format!("{relative}/{name}");
                relative_path(&next)?;
                let ty = child
                    .file_type()
                    .map_err(|source| graph_error("source provider kind", source))?;
                if ty.is_symlink() {
                    return Err(graph_error("source provider symlink", &next));
                }
                if ty.is_dir() {
                    pending.push((child.path(), next));
                } else if ty.is_file() {
                    let file =
                        open_relative_file(&selected_root, Path::new(&next), "source provider")?;
                    let (digest, size) = read_hash(file)?;
                    rows.push(GraphFile {
                        path: next,
                        size,
                        sha256: sha_text(&digest),
                    });
                    if rows.len() > MAX_FILES {
                        return Err(graph_error("source provider census", "32768 file limit"));
                    }
                } else {
                    return Err(graph_error("source provider non-regular entry", &next));
                }
            }
        }
        rows.sort_by(|left, right| left.path.cmp(&right.path));
        let tree = GraphDocument {
            schema: GRAPH_SCHEMA.to_owned(),
            entry: String::new(),
            files: rows,
        };
        if package_tree_fingerprint(&tree, prefix)? != expected.expected_package_trees[index] {
            return Err(graph_error(
                "approved package artifact",
                format!(
                    "{prefix}: source installed files differ from independently pinned provider"
                ),
            ));
        }
    }
    Ok(())
}

/// Producer serializes exactly three mandatory fields, sorted captured files.
pub fn make_graph_leaf(
    stage: &Path,
    entry: &str,
    expected: &ApprovedGraphCaptureInput,
) -> Result<Vec<u8>, HostAppError> {
    relative_path(entry)?;
    let root_fd = open_root(stage)?;
    let rows = census(stage, &root_fd)?
        .into_iter()
        .map(|observed| {
            let path = observed.path;
            let file = open_relative_file(&root_fd, &relative_path(&path)?, "graph source")?;
            let metadata = file
                .metadata()
                .map_err(|source| graph_error("graph source", source))?;
            if !same_identity(&metadata, &observed.metadata) {
                return Err(graph_error("graph source census", &path));
            }
            let (digest, size) = read_hash(file)?;
            let current = fs::symlink_metadata(stage.join(&path))
                .map_err(|source| graph_error("graph source", source))?;
            if !same_identity(&current, &observed.metadata) {
                return Err(graph_error("graph source census", &path));
            }
            Ok(GraphFile {
                path,
                size,
                sha256: sha_text(&digest),
            })
        })
        .collect::<Result<Vec<_>, HostAppError>>()?;
    let graph = GraphDocument {
        schema: GRAPH_SCHEMA.to_owned(),
        entry: entry.to_owned(),
        files: rows,
    };
    let bytes = serde_json::to_vec(&graph).map_err(|source| graph_error("graph encode", source))?;
    let _ = admit_stage_graph(stage, &bytes, expected)?;
    Ok(bytes)
}

/// A verified code-root entry in the already-admitted graph, relative to
/// drawio/src/main/webapp. Resource intake may use it only under its own
/// authenticated pending/committed native document owner.
#[derive(Debug, Clone)]
pub struct VerifiedResourceAsset {
    pub path: PathBuf,
    pub size: u64,
    pub sha256: [u8; 32],
}

impl AdmittedStageGraph {
    #[must_use]
    pub fn graph_sha256(&self) -> [u8; 32] {
        self.graph_sha256
    }

    /// Select the canonical installed transport from this admitted graph.
    /// Guardian rechecks the complete graph before each generation; this
    /// bounded final read also checks the exact preload leaf without resolving
    /// an application alias or copying a second transport sidecar.
    ///
    /// # Errors
    /// Refuses a missing, replaced or changed canonical transport member.
    pub fn installed_transport_preload(&self) -> Result<PathBuf, HostAppError> {
        const MEMBER: &str = "node_modules/@keld/kipc/transport.js";
        let row = self
            .graph
            .files
            .iter()
            .find(|row| row.path == MEMBER)
            .ok_or_else(|| {
                graph_error(
                    "transport preload",
                    "canonical installed transport is absent",
                )
            })?;
        root_identity(&self.root, &self.retained_root)?;
        let file = open_relative_file(&self.retained_root, Path::new(MEMBER), "transport preload")?;
        let (digest, size) = read_hash(file)?;
        if size != row.size || digest != parse_sha(&row.sha256)? {
            return Err(graph_error(
                "transport preload",
                "canonical installed transport changed",
            ));
        }
        Ok(self.root.join(MEMBER))
    }

    /// Retain an *independent* no-follow code-root directory handle for the
    /// GH542/GH473 native resource owner; the graph and Stage still own theirs.
    /// No descriptor is sent through an `AppWindowCommand` or page-facing field.
    pub fn clone_verified_code_root(&self) -> Result<OwnedFd, HostAppError> {
        use nix::fcntl::{OFlag, openat};
        use nix::sys::stat::Mode;
        self.recheck()?;
        let mut parent: Option<OwnedFd> = None;
        for component in ["drawio", "src", "main", "webapp"] {
            let anchor = parent
                .as_ref()
                .map_or_else(|| self.retained_root.as_fd(), std::os::fd::AsFd::as_fd);
            parent = Some(
                openat(
                    anchor,
                    component,
                    OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_CLOEXEC | OFlag::O_NOFOLLOW,
                    Mode::empty(),
                )
                .map_err(|source| graph_error("resource root", source))?,
            );
        }
        parent.ok_or_else(|| graph_error("resource root", "missing committed code directory"))
    }

    /// Copy only the validated relative asset facts into GH542's independent
    /// native resource owner. There is no second scan or parser, and no grant.
    pub fn verified_resource_assets(&self) -> Result<Vec<VerifiedResourceAsset>, HostAppError> {
        self.recheck()?;
        let prefix = "drawio/src/main/webapp/";
        let mut assets = Vec::new();
        for entry in &self.graph.files {
            if let Some(relative) = entry.path.strip_prefix(prefix) {
                assets.push(VerifiedResourceAsset {
                    path: relative_path(relative)?,
                    size: entry.size,
                    sha256: parse_sha(&entry.sha256)?,
                });
            }
        }
        if !assets
            .iter()
            .any(|entry| entry.path == Path::new("index.html"))
        {
            return Err(graph_error(
                "resource assets",
                "verified webapp index.html missing",
            ));
        }
        Ok(assets)
    }

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
        validate_document(&self.root, &self.retained_root, &self.graph)
    }
}

fn package_tree_fingerprint(doc: &GraphDocument, prefix: &str) -> Result<[u8; 32], HostAppError> {
    let mut hasher = Sha256::new();
    let mut matched = 0_usize;
    for row in &doc.files {
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

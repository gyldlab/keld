//! Descriptor-relative source capture. No project symlink is traversed.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, Metadata, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{FileExt as _, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Component, Path, PathBuf};

use rustix::fs::{Dir, Mode, OFlags, open, openat};
use serde::Serialize;
use sha2::{Digest, Sha256};

use super::{MacosExecutableInput, MacosGeneratedFile, MacosPackError, MacosPackageInput};

const MAX_MEMBERS: usize = 100_000;
const MAX_CAPTURE_BYTES: u64 = 8 * 1024 * 1024 * 1024;
const MAX_GENERATED_BYTES: usize = 16 * 1024 * 1024;
const RESERVED: [&str; 3] = [
    "keld.boot.json",
    "keld.permissions.jsonc",
    "keld.stage-graph.json",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Identity {
    device: u64,
    inode: u64,
    mode: u32,
    size: u64,
    modified: (i64, i64),
    changed: (i64, i64),
}

impl Identity {
    pub(super) fn of(metadata: &Metadata) -> Self {
        Self {
            device: metadata.dev(),
            inode: metadata.ino(),
            mode: metadata.mode(),
            size: metadata.len(),
            modified: (metadata.mtime(), metadata.mtime_nsec()),
            changed: (metadata.ctime(), metadata.ctime_nsec()),
        }
    }

    pub(super) fn same_object_and_mode(&self, other: &Self) -> bool {
        (self.device, self.inode, self.mode) == (other.device, other.inode, other.mode)
    }
}

#[derive(Debug)]
struct Member {
    directory: bool,
    identity: Identity,
}

pub(super) struct Capture {
    root: File,
    root_path: PathBuf,
    members: BTreeMap<String, Member>,
    generated: BTreeMap<String, Vec<u8>>,
    declared_roots: Vec<String>,
    bytes: u64,
}

/// Observed copied file or directory data; not an admission capability.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FileReceipt {
    /// Canonical relative name; `.` identifies the application root.
    pub path: String,
    /// Regular file or directory; links and special files are refused.
    pub kind: ReceiptKind,
    /// Exact POSIX permission bits.
    pub mode: u32,
    /// Observed regular-file length; absent for directories.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    /// Lowercase SHA-256 of regular-file bytes; absent for directories.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
}

/// Physical member kinds supported by local application packaging.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ReceiptKind {
    /// A regular file with a content digest.
    File,
    /// A directory with no file length or digest.
    Directory,
}

impl FileReceipt {
    pub(super) fn file(path: String, size: u64, mode: u32, sha256: String) -> Self {
        Self {
            path,
            kind: ReceiptKind::File,
            mode,
            size: Some(size),
            sha256: Some(sha256),
        }
    }

    pub(super) fn directory(path: String, mode: u32) -> Self {
        Self {
            path,
            kind: ReceiptKind::Directory,
            mode,
            size: None,
            sha256: None,
        }
    }
}

/// Checks exact directory kind and mode from an opened no-follow handle.
///
/// # Errors
/// Refuses altered directory permissions independent of any valid code signature.
pub(super) fn directory_mode(path: &Path) -> Result<u32, MacosPackError> {
    let directory = open_absolute(path, true)?;
    checked_directory_mode(&directory, path)
}

fn checked_directory_mode(directory: &File, path: &Path) -> Result<u32, MacosPackError> {
    let metadata = directory
        .metadata()
        .map_err(|error| MacosPackError::new("directory mode", error))?;
    if !metadata.is_dir() || metadata.mode() & 0o7777 != 0o755 {
        return Err(MacosPackError::new(
            "directory mode",
            format!(
                "owned bundle directory must have exact mode 0755: {}",
                path.display()
            ),
        ));
    }
    Ok(metadata.mode() & 0o7777)
}

/// Makes one owned directory and enforces its exact mode on a no-follow handle.
///
/// # Errors
/// Refuses any create, no-follow open, chmod, or post-chmod identity failure.
pub(super) fn create_0755_directory(path: &Path) -> Result<u32, MacosPackError> {
    fs::create_dir(path).map_err(|error| MacosPackError::new("owned directory creation", error))?;
    enforce_0755_directory(path)
}

/// Applies the mode via a retained no-follow directory handle, including
/// directories emitted by native signing tools after app assembly.
pub(super) fn enforce_0755_directory(path: &Path) -> Result<u32, MacosPackError> {
    let directory = open_absolute(path, true)?;
    directory
        .set_permissions(fs::Permissions::from_mode(0o755))
        .map_err(|error| MacosPackError::new("owned directory mode", error))?;
    checked_directory_mode(&directory, path)
}

pub(super) fn hex(digest: &[u8; 32]) -> String {
    use std::fmt::Write as _;
    let mut text = String::with_capacity(64);
    for byte in digest {
        // Formatting into a String cannot fail.
        let _ = write!(&mut text, "{byte:02x}");
    }
    text
}

pub(super) fn relative(path: &str) -> Result<(), MacosPackError> {
    if path.is_empty()
        || path.len() > 4096
        || path.starts_with('/')
        || path.contains('\\')
        || path.chars().any(char::is_control)
        || path
            .split('/')
            .any(|part| part.is_empty() || matches!(part, "." | "..") || part.len() > 255)
    {
        return Err(MacosPackError::new(
            "path admission",
            "a resource path is not canonical project-relative UTF-8",
        ));
    }
    Ok(())
}

fn allowed_resource(path: &str) -> Result<(), MacosPackError> {
    relative(path)?;
    for part in path.split('/') {
        let lower = part.to_ascii_lowercase();
        if matches!(
            lower.as_str(),
            ".git"
                | ".keld"
                | ".keld-work"
                | ".ssh"
                | ".aws"
                | ".gnupg"
                | "target"
                | "tests"
                | "__tests__"
                | "test-harness"
                | "private-evidence"
                | ".npmrc"
                | ".netrc"
                | ".pypirc"
                | "credentials.json"
                | "service-account.json"
                | "id_rsa"
                | "id_ed25519"
        ) || lower == ".env"
            || lower.starts_with(".env.")
            || [".pem", ".key", ".p8", ".p12", ".pfx"]
                .into_iter()
                .any(|suffix| lower.ends_with(suffix))
            || matches!(
                lower.as_str(),
                "keld.config.ts" | "keld.build.ts" | "keld.permissions.jsonc" | "keld.boot.json"
            )
        {
            return Err(MacosPackError::new(
                "resource admission",
                format!(
                    "development, authority or private input is not an application resource: {path}"
                ),
            ));
        }
    }
    Ok(())
}

pub(super) fn open_absolute(path: &Path, directory: bool) -> Result<File, MacosPackError> {
    if !path.is_absolute() {
        return Err(MacosPackError::new(
            "input open",
            "an absolute path was required",
        ));
    }
    let mut current: File = open(
        "/",
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(|error| MacosPackError::new("input root open", error))?
    .into();
    let components: Vec<_> = path.components().collect();
    for (index, component) in components.iter().enumerate().skip(1) {
        let Component::Normal(name) = component else {
            return Err(MacosPackError::new(
                "input open",
                "path has a non-normal component",
            ));
        };
        let want_directory = directory || index + 1 < components.len();
        current = openat(&current, *name, flags(want_directory), Mode::empty())
            .map_err(|error| MacosPackError::new("no-follow input open", error))?
            .into();
    }
    Ok(current)
}

fn flags(directory: bool) -> OFlags {
    let base = OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK;
    if directory {
        base | OFlags::DIRECTORY
    } else {
        base
    }
}

fn open_below(root: &File, relative_path: &str, directory: bool) -> Result<File, MacosPackError> {
    relative(relative_path)?;
    let components: Vec<_> = relative_path.split('/').collect();
    let mut current = root
        .try_clone()
        .map_err(|error| MacosPackError::new("retain input root", error))?;
    for (index, component) in components.iter().enumerate() {
        current = openat(
            &current,
            *component,
            flags(directory || index + 1 < components.len()),
            Mode::empty(),
        )
        .map_err(|error| {
            MacosPackError::new(
                "no-follow resource open",
                format!("{relative_path}: {error}"),
            )
        })?
        .into();
    }
    Ok(current)
}

impl Capture {
    pub(super) fn plan(input: &MacosPackageInput) -> Result<Self, MacosPackError> {
        Self::plan_mode(input, false)
    }

    pub(super) fn plan_graph(input: &MacosPackageInput) -> Result<Self, MacosPackError> {
        Self::plan_mode(input, true)
    }

    fn plan_mode(input: &MacosPackageInput, graph: bool) -> Result<Self, MacosPackError> {
        let canonical = fs::canonicalize(&input.project_root)
            .map_err(|error| MacosPackError::new("project root", error))?;
        if canonical != input.project_root {
            return Err(MacosPackError::new(
                "project root",
                "project root must be the canonical invocation directory",
            ));
        }
        let root = open_absolute(&canonical, true)?;
        let mut capture = Self {
            root,
            root_path: canonical,
            members: BTreeMap::new(),
            generated: BTreeMap::new(),
            declared_roots: input.resources.clone(),
            bytes: 0,
        };
        if input.resources.len() > 256 {
            return Err(MacosPackError::new(
                "resource plan",
                "at most 256 explicit roots are supported",
            ));
        }
        let mut roots = BTreeSet::new();
        for path in &input.resources {
            allowed_resource(path)?;
            if roots.iter().any(|prior: &String| overlaps(prior, path)) {
                return Err(MacosPackError::new(
                    "resource plan",
                    "explicit resource roots overlap or repeat",
                ));
            }
            roots.insert(path.clone());
        }
        for path in roots {
            capture.visit(&path)?;
        }
        for path in std::iter::once(&input.entry).chain(input.renderer.iter()) {
            allowed_resource(path)?;
            if !capture.members.contains_key(path) {
                capture.visit(path)?;
            }
            if capture
                .members
                .get(path)
                .is_none_or(|member| member.directory)
            {
                return Err(MacosPackError::new(
                    "resource plan",
                    "entry and renderer must be regular files",
                ));
            }
        }
        if input.permissions.is_empty()
            || input.permissions.len() > 64 * 1024
            || (!graph && (input.boot.is_empty() || input.boot.len() > 64 * 1024))
            || (graph && !input.boot.is_empty())
        {
            return Err(MacosPackError::new(
                "compiled authority",
                "permissions require 1..=64 KiB; plain boot requires 1..=64 KiB, while graph boot must be deferred and empty",
            ));
        }
        if !graph {
            capture.add_generated("keld.boot.json", &input.boot)?;
        }
        capture.add_generated("keld.permissions.jsonc", &input.permissions)?;
        for MacosGeneratedFile { path, bytes } in &input.generated {
            if graph && matches!(path.as_str(), "keld.boot.json" | "keld.stage-graph.json") {
                return Err(MacosPackError::new(
                    "graph authority",
                    "graph mode compiles both fixed graph and boot leaves after capture; pre-supplied leaves are forbidden",
                ));
            }
            capture.add_generated(path, bytes)?;
        }
        Ok(capture)
    }

    fn add_generated(&mut self, path: &str, bytes: &[u8]) -> Result<(), MacosPackError> {
        relative(path)?;
        require_inert_generated(path, bytes)?;
        if bytes.len() > MAX_GENERATED_BYTES
            || self.members.len() + self.generated.len() >= MAX_MEMBERS
            || self
                .generated
                .keys()
                .any(|existing| overlaps(existing, path))
            || self.members.iter().any(|(name, member)| {
                name == path
                    || (!member.directory && path.starts_with(&format!("{name}/")))
                    || name.starts_with(&format!("{path}/"))
            })
        {
            return Err(MacosPackError::new(
                "generated leaf",
                "a generated leaf is too large, repeated, or collides with a resource",
            ));
        }
        self.bytes = self
            .bytes
            .checked_add(bytes.len() as u64)
            .filter(|total| *total <= MAX_CAPTURE_BYTES)
            .ok_or_else(|| {
                MacosPackError::new("generated leaf", "combined application byte limit exceeded")
            })?;
        self.generated.insert(path.to_owned(), bytes.to_vec());
        Ok(())
    }

    fn visit(&mut self, path: &str) -> Result<(), MacosPackError> {
        allowed_resource(path)?;
        if RESERVED.contains(&path) {
            return Err(MacosPackError::new(
                "resource plan",
                "a generated authority destination is reserved",
            ));
        }
        if self.members.len() + self.generated.len() >= MAX_MEMBERS {
            return Err(MacosPackError::new(
                "resource plan",
                "resource member limit exceeded",
            ));
        }
        let file = open_below(&self.root, path, false)?;
        let metadata = file
            .metadata()
            .map_err(|error| MacosPackError::new("resource metadata", error))?;
        if !metadata.is_file() && !metadata.is_dir() {
            return Err(MacosPackError::new(
                "resource kind",
                "special files are not application resources",
            ));
        }
        if metadata.mode() & 0o6000 != 0 {
            return Err(MacosPackError::new(
                "resource mode",
                "set-user-ID/set-group-ID resources are forbidden",
            ));
        }
        let identity = Identity::of(&metadata);
        if metadata.is_file() {
            self.bytes = self
                .bytes
                .checked_add(metadata.len())
                .filter(|total| *total <= MAX_CAPTURE_BYTES)
                .ok_or_else(|| {
                    MacosPackError::new("resource plan", "resource byte limit exceeded")
                })?;
        }
        self.members.insert(
            path.to_owned(),
            Member {
                directory: metadata.is_dir(),
                identity: identity.clone(),
            },
        );
        if metadata.is_dir() {
            let mut names = Vec::new();
            for entry in Dir::read_from(&file)
                .map_err(|error| MacosPackError::new("directory capture", error))?
            {
                let entry =
                    entry.map_err(|error| MacosPackError::new("directory capture", error))?;
                let name = entry
                    .file_name()
                    .to_str()
                    .map_err(|error| MacosPackError::new("resource name", error))?;
                if !matches!(name, "." | "..") {
                    if self.members.len() + names.len() + self.generated.len() >= MAX_MEMBERS {
                        return Err(MacosPackError::new(
                            "directory capture",
                            "resource member limit exceeded during enumeration",
                        ));
                    }
                    names.push(name.to_owned());
                }
            }
            names.sort();
            for name in names {
                self.visit(&format!("{path}/{name}"))?;
            }
            if Identity::of(
                &file
                    .metadata()
                    .map_err(|error| MacosPackError::new("directory recheck", error))?,
            ) != identity
            {
                return Err(MacosPackError::new(
                    "directory capture",
                    "a source directory changed while enumerated",
                ));
            }
        }
        Ok(())
    }

    pub(super) fn reject_output_overlap(
        &self,
        output: &Path,
        input: &MacosPackageInput,
    ) -> Result<(), MacosPackError> {
        let roots = self
            .declared_roots
            .iter()
            .chain(std::iter::once(&input.entry))
            .chain(input.renderer.iter())
            .chain(input.icon.iter());
        for relative_path in roots {
            let source = self.root_path.join(relative_path);
            if output.starts_with(&source) || source.starts_with(output) {
                return Err(MacosPackError::new(
                    "output path",
                    "output overlaps an application input",
                ));
            }
        }
        for executable in [&input.host, &input.bun] {
            let source = self.root_path.join(&executable.path);
            if output.starts_with(&source) || source.starts_with(output) {
                return Err(MacosPackError::new(
                    "output path",
                    "output overlaps an executable input",
                ));
            }
        }
        Ok(())
    }

    fn prepare_destination(
        &self,
        destination: &Path,
    ) -> Result<(BTreeSet<String>, Vec<FileReceipt>), MacosPackError> {
        // Allocate the complete empty destination tree first. Exclusive creates on
        // the actual output filesystem reject case/normalization-equivalent names
        // before any source content is copied, without a guessed Unicode fold.
        let root_mode = directory_mode(destination)?;
        let mut directories = BTreeSet::new();
        let mut files = BTreeSet::new();
        for (path, member) in &self.members {
            if member.directory {
                directories.insert(path.clone());
            } else {
                files.insert(path.clone());
            }
            collect_parents(path, &mut directories);
        }
        for path in self.generated.keys() {
            files.insert(path.clone());
            collect_parents(path, &mut directories);
        }
        let mut receipts = vec![FileReceipt::directory(".".to_owned(), root_mode)];
        for path in &directories {
            let mode = create_0755_directory(&destination.join(path)).map_err(|error| {
                MacosPackError::new("destination collision", format!("{path}: {error}"))
            })?;
            receipts.push(FileReceipt::directory(path.clone(), mode));
        }
        for path in &files {
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(destination.join(path))
                .map_err(|error| {
                    MacosPackError::new("destination collision", format!("{path}: {error}"))
                })?;
        }
        Ok((directories, receipts))
    }

    pub(super) fn materialize(
        &self,
        destination: &Path,
    ) -> Result<Vec<FileReceipt>, MacosPackError> {
        let (directories, mut receipts) = self.prepare_destination(destination)?;
        for (path, member) in &self.members {
            let source = open_below(&self.root, path, member.directory)?;
            if Identity::of(
                &source
                    .metadata()
                    .map_err(|error| MacosPackError::new("resource recheck", error))?,
            ) != member.identity
            {
                return Err(MacosPackError::new(
                    "resource recheck",
                    format!("captured resource changed: {path}"),
                ));
            }
            if !member.directory {
                receipts.push(copy_resource(
                    source,
                    &destination.join(path),
                    path,
                    &member.identity,
                )?);
            }
        }
        for (path, bytes) in &self.generated {
            let target = destination.join(path);
            let mut file = OpenOptions::new()
                .write(true)
                .open(&target)
                .map_err(|error| MacosPackError::new("generated write", error))?;
            file.write_all(bytes)
                .map_err(|error| MacosPackError::new("generated write", error))?;
            file.set_permissions(fs::Permissions::from_mode(0o644))
                .map_err(|error| MacosPackError::new("generated mode", error))?;
            file.sync_all()
                .map_err(|error| MacosPackError::new("generated flush", error))?;
            receipts.push(FileReceipt::file(
                path.clone(),
                bytes.len() as u64,
                0o644,
                hex(&Sha256::digest(bytes).into()),
            ));
        }
        // Reopen all planned leaves/roots through the retained root after capture.
        for (path, member) in &self.members {
            let source = open_below(&self.root, path, member.directory)?;
            if Identity::of(
                &source
                    .metadata()
                    .map_err(|error| MacosPackError::new("capture completion", error))?,
            ) != member.identity
            {
                return Err(MacosPackError::new(
                    "capture completion",
                    "an input identity changed during capture",
                ));
            }
        }
        // Capture completion includes directory modes: content-only file hashing
        // would hide a later umask- or chmod-driven app layout mutation.
        for path in &directories {
            directory_mode(&destination.join(path))?;
        }
        receipts.sort_by(|left, right| left.path.cmp(&right.path));
        Ok(receipts)
    }

    pub(super) fn copy_icon(
        &self,
        relative_path: &str,
        destination: &Path,
    ) -> Result<(), MacosPackError> {
        allowed_resource(relative_path)?;
        if Path::new(relative_path).extension() != Some(std::ffi::OsStr::new("icns")) {
            return Err(MacosPackError::new(
                "icon",
                "icon must name a project-relative .icns file",
            ));
        }
        let source = open_below(&self.root, relative_path, false)?;
        let metadata = source
            .metadata()
            .map_err(|error| MacosPackError::new("icon", error))?;
        if !metadata.is_file() || metadata.len() > 16 * 1024 * 1024 {
            return Err(MacosPackError::new(
                "icon",
                "icon must be a regular file no larger than 16 MiB",
            ));
        }
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(destination)
            .map_err(|error| MacosPackError::new("icon destination", error))?;
        copy_resource(source, destination, relative_path, &Identity::of(&metadata))?;
        let mut bytes = [0; 4];
        File::open(destination)
            .and_then(|mut file| file.read_exact(&mut bytes))
            .map_err(|error| MacosPackError::new("icon header", error))?;
        if bytes != *b"icns" {
            return Err(MacosPackError::new(
                "icon header",
                "icon is not an ICNS container",
            ));
        }
        Ok(())
    }
}

fn collect_parents(path: &str, directories: &mut BTreeSet<String>) {
    let mut child = path;
    while let Some((parent, _)) = child.rsplit_once('/') {
        directories.insert(parent.to_owned());
        child = parent;
    }
}

fn overlaps(left: &str, right: &str) -> bool {
    left == right
        || left
            .strip_prefix(right)
            .is_some_and(|tail| tail.starts_with('/'))
        || right
            .strip_prefix(left)
            .is_some_and(|tail| tail.starts_with('/'))
}

fn native_magic(prefix: &[u8]) -> bool {
    prefix.starts_with(b"\x7fELF")
        || prefix.starts_with(b"MZ")
        || [
            [0xfe, 0xed, 0xfa, 0xce],
            [0xce, 0xfa, 0xed, 0xfe],
            [0xfe, 0xed, 0xfa, 0xcf],
            [0xcf, 0xfa, 0xed, 0xfe],
            [0xca, 0xfe, 0xba, 0xbe],
            [0xbe, 0xba, 0xfe, 0xca],
            [0xca, 0xfe, 0xba, 0xbf],
            [0xbf, 0xba, 0xfe, 0xca],
        ]
        .iter()
        .any(|magic| prefix.starts_with(magic))
}

fn is_native_resource(path: &str, prefix: &[u8]) -> bool {
    native_magic(prefix)
        || Path::new(path)
            .extension()
            .and_then(|actual| actual.to_str())
            .is_some_and(|actual| {
                ["dylib", "so", "dll", "exe", "node"]
                    .into_iter()
                    .any(|extension| actual.eq_ignore_ascii_case(extension))
            })
}

pub(super) fn require_inert_generated(path: &str, bytes: &[u8]) -> Result<(), MacosPackError> {
    if is_native_resource(path, &bytes[..bytes.len().min(4)]) {
        return Err(MacosPackError::new(
            "generated native code",
            format!("generated input is native executable content or an addon: {path}"),
        ));
    }
    Ok(())
}

/// Read the leading four bytes without advancing the copy cursor. Partial
/// positional reads cannot hide a Mach-O/ELF magic from the shared admission.
fn read_native_prefix(source: &File) -> Result<([u8; 4], usize), MacosPackError> {
    let mut prefix = [0_u8; 4];
    let mut filled = 0_usize;
    while filled < prefix.len() {
        match source.read_at(&mut prefix[filled..], filled as u64) {
            Ok(0) => break,
            Ok(read) => filled += read,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(error) => return Err(MacosPackError::new("resource native prefix", error)),
        }
    }
    Ok((prefix, filled))
}

fn copy_resource(
    mut source: File,
    destination: &Path,
    path: &str,
    identity: &Identity,
) -> Result<FileReceipt, MacosPackError> {
    let (prefix, filled) = read_native_prefix(&source)?;
    if is_native_resource(path, &prefix[..filled]) {
        return Err(MacosPackError::new(
            "resource native code",
            format!("unclassified native code is not an inert resource: {path}"),
        ));
    }
    let mut sink = OpenOptions::new()
        .write(true)
        .open(destination)
        .map_err(|error| MacosPackError::new("resource destination", error))?;
    let mut hash = Sha256::new();
    let mut buffer = vec![0; 64 * 1024];
    let mut length = 0_u64;
    loop {
        let read = source
            .read(&mut buffer)
            .map_err(|error| MacosPackError::new("resource read", error))?;
        if read == 0 {
            break;
        }
        length += read as u64;
        if length > identity.size {
            return Err(MacosPackError::new(
                "resource read",
                "resource grew during capture",
            ));
        }
        hash.update(&buffer[..read]);
        sink.write_all(&buffer[..read])
            .map_err(|error| MacosPackError::new("resource copy", error))?;
    }
    if length != identity.size
        || Identity::of(
            &source
                .metadata()
                .map_err(|error| MacosPackError::new("resource identity", error))?,
        ) != *identity
    {
        return Err(MacosPackError::new(
            "resource identity",
            "resource changed during capture",
        ));
    }
    sink.set_permissions(fs::Permissions::from_mode(0o644))
        .map_err(|error| MacosPackError::new("resource mode", error))?;
    sink.sync_all()
        .map_err(|error| MacosPackError::new("resource flush", error))?;
    Ok(FileReceipt::file(
        path.to_owned(),
        length,
        0o644,
        hex(&hash.finalize().into()),
    ))
}

pub(super) fn copy_executable(
    root: &Path,
    input: &MacosExecutableInput,
    destination: &Path,
) -> Result<FileReceipt, MacosPackError> {
    if input.version.is_empty()
        || input.version.len() > 128
        || input.version.chars().any(char::is_control)
    {
        return Err(MacosPackError::new(
            "executable version",
            "qualified version must be nonempty, bounded and free of control characters",
        ));
    }
    let path = root.join(&input.path);
    let mut source = open_absolute(&path, false)?;
    let metadata = source
        .metadata()
        .map_err(|error| MacosPackError::new("executable metadata", error))?;
    if !metadata.is_file()
        || metadata.mode() & 0o111 == 0
        || metadata.mode() & 0o6000 != 0
        || metadata.len() > 1024 * 1024 * 1024
    {
        return Err(MacosPackError::new(
            "executable admission",
            "input must be a regular executable without set-user-ID/set-group-ID bits, at most 1 GiB",
        ));
    }
    let identity = Identity::of(&metadata);
    let mut sink = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o700)
        .open(destination)
        .map_err(|error| MacosPackError::new("executable destination", error))?;
    let mut hash = Sha256::new();
    let mut buffer = vec![0; 64 * 1024];
    let mut size = 0_u64;
    loop {
        let read = source
            .read(&mut buffer)
            .map_err(|error| MacosPackError::new("executable read", error))?;
        if read == 0 {
            break;
        }
        size += read as u64;
        if size > identity.size {
            return Err(MacosPackError::new(
                "executable capture",
                "input executable grew during capture",
            ));
        }
        hash.update(&buffer[..read]);
        sink.write_all(&buffer[..read])
            .map_err(|error| MacosPackError::new("executable copy", error))?;
    }
    let digest: [u8; 32] = hash.finalize().into();
    if size != identity.size
        || digest != input.sha256
        || Identity::of(
            &source
                .metadata()
                .map_err(|error| MacosPackError::new("executable recheck", error))?,
        ) != identity
    {
        return Err(MacosPackError::new(
            "executable qualification",
            "captured executable differs from its input pin or changed during capture",
        ));
    }
    let reopened = open_absolute(&path, false)?;
    if Identity::of(
        &reopened
            .metadata()
            .map_err(|error| MacosPackError::new("executable path recheck", error))?,
    ) != identity
    {
        return Err(MacosPackError::new(
            "executable path recheck",
            "executable source path changed during capture",
        ));
    }
    sink.set_permissions(fs::Permissions::from_mode(0o755))
        .map_err(|error| MacosPackError::new("executable mode", error))?;
    sink.sync_all()
        .map_err(|error| MacosPackError::new("executable flush", error))?;
    Ok(FileReceipt::file(
        destination
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("executable")
            .to_owned(),
        size,
        0o755,
        hex(&digest),
    ))
}

pub(super) fn digest_file(path: &Path) -> Result<[u8; 32], MacosPackError> {
    let file = open_absolute(path, false)?;
    digest_retained_file(&file)
}

/// Hash the already opened regular file without changing its shared file cursor.
pub(super) fn digest_retained_file(file: &File) -> Result<[u8; 32], MacosPackError> {
    let before = file
        .metadata()
        .map_err(|error| MacosPackError::new("artifact hash", error))?;
    if !before.is_file() || before.len() > MAX_CAPTURE_BYTES {
        return Err(MacosPackError::new(
            "artifact hash",
            "artifact must be a bounded regular file",
        ));
    }
    let mut hash = Sha256::new();
    let mut buffer = vec![0; 64 * 1024];
    let mut offset = 0_u64;
    loop {
        let read = file
            .read_at(&mut buffer, offset)
            .map_err(|error| MacosPackError::new("artifact hash", error))?;
        if read == 0 {
            break;
        }
        offset += read as u64;
        if offset > before.len() {
            return Err(MacosPackError::new(
                "artifact hash",
                "artifact grew while hashed",
            ));
        }
        hash.update(&buffer[..read]);
    }
    if offset != before.len()
        || Identity::of(&before)
            != Identity::of(
                &file
                    .metadata()
                    .map_err(|error| MacosPackError::new("artifact hash", error))?,
            )
    {
        return Err(MacosPackError::new(
            "artifact hash",
            "artifact changed while hashed",
        ));
    }
    Ok(hash.finalize().into())
}

pub(super) fn executable_mode(path: &Path) -> Result<(), MacosPackError> {
    let file = open_absolute(path, false)?;
    let metadata = file
        .metadata()
        .map_err(|error| MacosPackError::new("final executable mode", error))?;
    super::validate_macos_executable_metadata(&metadata)
}

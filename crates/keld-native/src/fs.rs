//! Host-owned retained filesystem broker (KEL-130).
//!
//! The broker prepares directory capabilities from one verified manifest and
//! keeps all later traversal relative to those retained objects. A request is
//! still authorized exactly once by `keld-guard`; the matched-scope permit is
//! borrowed only while privileged dispatch runs.

use std::collections::VecDeque;
use std::io::{self, ErrorKind, Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

#[cfg(not(target_os = "linux"))]
use cap_fs_ext::DirExt;
use cap_fs_ext::{FollowSymlinks, MetadataExt, OpenOptionsFollowExt};
use cap_std::ambient_authority;
use cap_std::fs::{Dir, File, OpenOptions};
use keld_guard::verified_manifest::VerifiedManifest;
use keld_guard::{
    DenyReason, PathScope, PathScopeKind, Principal, ScopePermit, ScopeSetError, path_scopes,
    validate_fs_component,
};
use keld_ipc::ReceivePolicy;
use keld_ipc::codec::{decode, encode};
use keld_ipc::frame::{ChannelId, FrameKind};
use keld_ipc::guard_dispatch::dispatch_privileged;
use keld_ipc::link::{handshake_server, read_validated_frame, write_frame};
use keld_ipc::{IpcError, SessionToken};
use serde::{Deserialize, Serialize};

/// Capability id for a scoped read.
pub const FS_READ_CAPABILITY: &str = "fs.read";
/// Capability id for a scoped write.
pub const FS_WRITE_CAPABILITY: &str = "fs.write";
/// kipc channel carrying [`FsRequest`]/[`FsResponse`] calls.
pub const FS_CHANNEL: ChannelId = ChannelId(2);

/// Maximum accepted UTF-8 request path length.
pub const MAX_FS_PATH_BYTES: usize = 4 * 1024;
/// Maximum bytes read or written by one call.
pub const MAX_FS_CONTENT_BYTES: usize = 8 * 1024 * 1024;
/// Maximum components processed after link expansion.
pub const MAX_FS_COMPONENTS: usize = 256;
/// Maximum links expanded by one traversal.
pub const MAX_FS_SYMLINK_EXPANSIONS: usize = 40;
/// Fixed content-I/O chunk size.
pub const FS_IO_CHUNK_BYTES: usize = 64 * 1024;
/// One non-renewable cooperative operation budget.
pub const FS_OPERATION_BUDGET: Duration = Duration::from_secs(5);

/// Request payload for the filesystem channel.
#[derive(Debug, Serialize, Deserialize)]
pub enum FsRequest {
    /// Read the regular file at `path`.
    Read {
        /// Absolute UTF-8 path checked verbatim by the guard.
        path: String,
    },
    /// Write `bytes` in place, or create a previously absent final leaf.
    Write {
        /// Absolute UTF-8 path checked verbatim by the guard.
        path: String,
        /// Bounded content bytes.
        bytes: Vec<u8>,
    },
}

/// Response payload for the filesystem channel.
#[derive(Debug, Serialize, Deserialize)]
pub enum FsResponse {
    /// Exact regular-file contents.
    Read {
        /// File contents.
        bytes: Vec<u8>,
    },
    /// The write completed before cancellation or expiry.
    Write,
}

/// Failure while retaining one manifest's filesystem authority.
#[derive(Debug)]
pub enum FsPrepareError {
    /// A filesystem scope is malformed, duplicate, or exceeds a fixed bound.
    InvalidScope {
        /// Capability containing the scope.
        capability: &'static str,
        /// Zero-based grant index.
        grant_index: usize,
        /// Exact reason the scope cannot be serviced.
        detail: String,
    },
    /// The ambient preparation open could not retain the declared anchor.
    OpenScope {
        /// Capability containing the scope.
        capability: &'static str,
        /// Zero-based grant index.
        grant_index: usize,
        /// OS failure from the one preparation open.
        source: io::Error,
    },
}

impl FsPrepareError {
    /// Stable registered error code for every preparation failure.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        "KELD-NATIVE-008"
    }
}

impl std::fmt::Display for FsPrepareError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidScope {
                capability,
                grant_index,
                detail,
            } => write!(
                f,
                "{}: `{capability}` scope {grant_index} is not serviceable — {detail}. Repair the absolute scope and start a fresh session.",
                self.code()
            ),
            Self::OpenScope {
                capability,
                grant_index,
                source,
            } => write!(
                f,
                "{}: cannot retain `{capability}` scope {grant_index} — {source}. Repair the absolute scope and start a fresh session.",
                self.code()
            ),
        }
    }
}

impl std::error::Error for FsPrepareError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidScope { .. } => None,
            Self::OpenScope { source, .. } => Some(source),
        }
    }
}

/// Cause of an incomplete write after the content-effect boundary.
#[derive(Debug)]
pub enum WriteInterruption {
    /// The OS call returned an error.
    Io(io::Error),
    /// The fixed operation budget expired.
    Deadline,
    /// The owner requested cancellation.
    Cancelled,
}

/// Errors from one retained filesystem operation.
#[derive(Debug)]
pub enum FsError {
    /// The sole guard decision denied the request.
    Denied(DenyReason),
    /// An allowed operation failed before a content effect.
    Io(io::Error),
    /// Traversal escaped, raced, or selected a forbidden alias.
    ResolvedOutOfScope {
        /// Original request spelling.
        requested: String,
        /// Exact traversal reason.
        detail: String,
    },
    /// The selected object is not a supported regular file or local directory.
    UnsupportedObject {
        /// Original request spelling.
        requested: String,
        /// Exact object/type reason.
        detail: String,
    },
    /// A fixed path, content, component, or link bound was exceeded.
    LimitExceeded {
        /// Stable limit name.
        limit: &'static str,
        /// Observed value.
        actual: u64,
        /// Maximum admitted value.
        maximum: u64,
    },
    /// The operation budget expired before any write-content effect.
    Deadline,
    /// Cancellation was observed before any write-content effect.
    Cancelled,
    /// A write failed or was interrupted after its effect boundary.
    WriteEffect {
        /// Winning interruption cause.
        cause: WriteInterruption,
        /// Sum of successful content-write return lengths.
        committed_bytes: u64,
        /// Requested content length.
        requested_bytes: u64,
    },
    /// The call presented a different immutable policy snapshot.
    SnapshotMismatch {
        /// Digest retained during preparation.
        prepared: [u8; 32],
        /// Digest presented by this call.
        presented: [u8; 32],
    },
}

impl FsError {
    /// Stable registered `KELD-*` code for this failure.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Denied(reason) => reason.code(),
            Self::Io(_) => "KELD-NATIVE-001",
            Self::ResolvedOutOfScope { .. } => "KELD-NATIVE-002",
            Self::UnsupportedObject { .. } => "KELD-NATIVE-003",
            Self::LimitExceeded { .. } => "KELD-NATIVE-004",
            Self::Deadline => "KELD-NATIVE-005",
            Self::Cancelled => "KELD-NATIVE-006",
            Self::WriteEffect { .. } => "KELD-NATIVE-007",
            Self::SnapshotMismatch { .. } => "KELD-NATIVE-008",
        }
    }
}

impl std::fmt::Display for FsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Denied(reason) => write!(f, "{reason}"),
            Self::Io(source) => write!(
                f,
                "{}: filesystem I/O failed before a confirmed content effect — {source}. Repair path, access, or storage and issue a fresh request.",
                self.code()
            ),
            Self::ResolvedOutOfScope { requested, detail } => write!(
                f,
                "{}: `{requested}` escaped or changed outside its retained scope — {detail}. Move the target beneath the granted root or use a direct approved scope.",
                self.code()
            ),
            Self::UnsupportedObject { requested, detail } => write!(
                f,
                "{}: `{requested}` is not a supported local regular file — {detail}. Use a regular file under one retained filesystem root.",
                self.code()
            ),
            Self::LimitExceeded {
                limit,
                actual,
                maximum,
            } => write!(
                f,
                "{}: filesystem {limit} limit exceeded ({actual} > {maximum}). Shorten the request or use at most 8 MiB.",
                self.code()
            ),
            Self::Deadline => write!(
                f,
                "{}: filesystem operation exceeded its five-second cooperative budget before a write effect. Diagnose the filesystem and issue a fresh request only if safe.",
                self.code()
            ),
            Self::Cancelled => write!(
                f,
                "{}: filesystem operation was cancelled before a write effect. Wait for a fresh session generation.",
                self.code()
            ),
            Self::WriteEffect {
                cause,
                committed_bytes,
                requested_bytes,
            } => {
                let cause = match cause {
                    WriteInterruption::Io(source) => format!("I/O error: {source}"),
                    WriteInterruption::Deadline => "deadline".to_owned(),
                    WriteInterruption::Cancelled => "cancellation".to_owned(),
                };
                write!(
                    f,
                    "{}: write effect may have occurred after {cause} ({committed_bytes}/{requested_bytes} content bytes acknowledged). Inspect or rewrite the target explicitly; do not auto-retry.",
                    self.code()
                )
            }
            Self::SnapshotMismatch {
                prepared,
                presented,
            } => write!(
                f,
                "{}: permissions snapshot mismatch (prepared {}, presented {}). Use the same verified snapshot and start a fresh session.",
                self.code(),
                HexDigest(prepared),
                HexDigest(presented)
            ),
        }
    }
}

impl std::error::Error for FsError {}

struct HexDigest<'a>(&'a [u8; 32]);

impl std::fmt::Display for HexDigest<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl From<&FsError> for keld_ipc::CallError {
    fn from(error: &FsError) -> Self {
        match error {
            FsError::Denied(reason) => Self::from(reason),
            _ => Self {
                code: error.code().to_owned(),
                message: error.to_string(),
            },
        }
    }
}

#[derive(Debug)]
struct RetainedGrant {
    grant_index: usize,
    kind: PathScopeKind,
    anchor: String,
    exact_leaf: Option<String>,
    root: Dir,
    root_device: u64,
}

/// Opaque, non-cloneable owner of one verified manifest's retained roots.
#[derive(Debug)]
pub struct FsBroker {
    prepared_digest: [u8; 32],
    read_grants: Vec<RetainedGrant>,
    write_grants: Vec<RetainedGrant>,
}

impl FsBroker {
    /// Retains all serviceable filesystem grants from one verified snapshot.
    ///
    /// # Errors
    ///
    /// Returns [`FsPrepareError`] before the broker is published when any scope
    /// is malformed, duplicated, excessive, or cannot be retained.
    pub fn prepare(verified: &VerifiedManifest) -> Result<Self, FsPrepareError> {
        let read_grants = prepare_capability(verified, FS_READ_CAPABILITY)?;
        let write_grants = prepare_capability(verified, FS_WRITE_CAPABILITY)?;
        Ok(Self {
            prepared_digest: verified.verified_sha256(),
            read_grants,
            write_grants,
        })
    }

    /// Reads one bounded regular file through the matched retained scope.
    ///
    /// # Errors
    ///
    /// Returns [`FsError`] on snapshot mismatch, invalid shape, guard denial,
    /// retained traversal failure, cancellation/deadline, or bounded I/O error.
    pub fn read(
        &self,
        verified: &VerifiedManifest,
        principal: Principal,
        path: &str,
        cancelled: &AtomicBool,
    ) -> Result<Vec<u8>, FsError> {
        self.verify_snapshot(verified)?;
        validate_request_shape(path)?;
        match dispatch_privileged(
            verified.manifest(),
            principal,
            FS_READ_CAPABILITY,
            path,
            |permit| {
                let progress = Progress::new(cancelled);
                self.read_allowed(permit, path, &progress)
            },
        ) {
            Ok(result) => result,
            Err(reason) => Err(FsError::Denied(reason)),
        }
    }

    /// Writes one bounded payload through the matched retained scope.
    ///
    /// Existing files are truncated and written through the same exact handle;
    /// absent leaves use create-new. No retry, rollback, or replacement occurs.
    ///
    /// # Errors
    ///
    /// Returns [`FsError`] with explicit pre-effect or effect-may-have-occurred
    /// classification.
    pub fn write(
        &self,
        verified: &VerifiedManifest,
        principal: Principal,
        path: &str,
        bytes: &[u8],
        cancelled: &AtomicBool,
    ) -> Result<(), FsError> {
        self.verify_snapshot(verified)?;
        validate_request_shape(path)?;
        if bytes.len() > MAX_FS_CONTENT_BYTES {
            return Err(limit_error(
                "content-bytes",
                bytes.len(),
                MAX_FS_CONTENT_BYTES,
            ));
        }
        match dispatch_privileged(
            verified.manifest(),
            principal,
            FS_WRITE_CAPABILITY,
            path,
            |permit| {
                let progress = Progress::new(cancelled);
                self.write_allowed(permit, path, bytes, &progress)
            },
        ) {
            Ok(result) => result,
            Err(reason) => Err(FsError::Denied(reason)),
        }
    }

    fn verify_snapshot(&self, verified: &VerifiedManifest) -> Result<(), FsError> {
        let presented = verified.verified_sha256();
        if self.prepared_digest != presented {
            return Err(FsError::SnapshotMismatch {
                prepared: self.prepared_digest,
                presented,
            });
        }
        Ok(())
    }

    fn read_allowed(
        &self,
        permit: &ScopePermit,
        path: &str,
        progress: &Progress<'_>,
    ) -> Result<Vec<u8>, FsError> {
        progress.check_read()?;
        let grant = selected_grant(&self.read_grants, permit, path)?;
        let mut file = match walk(grant, path, OpenPurpose::Read, progress)? {
            WalkResult::File(file) => file,
            WalkResult::Missing { .. } => {
                return Err(FsError::Io(io::Error::new(
                    ErrorKind::NotFound,
                    "retained final leaf does not exist",
                )));
            }
        };
        progress.check_read()?;
        let metadata = progress.finish_read_io(file.metadata())?;
        ensure_regular_and_device(&metadata, grant, path)?;
        if metadata.len() > MAX_FS_CONTENT_BYTES as u64 {
            return Err(FsError::LimitExceeded {
                limit: "content-bytes",
                actual: metadata.len(),
                maximum: MAX_FS_CONTENT_BYTES as u64,
            });
        }
        let capacity = usize::try_from(metadata.len()).map_err(|_| {
            FsError::Io(io::Error::new(
                ErrorKind::InvalidData,
                "bounded file length does not fit the target pointer width",
            ))
        })?;
        let mut output = Vec::with_capacity(capacity);
        let mut chunk = vec![0_u8; FS_IO_CHUNK_BYTES].into_boxed_slice();
        loop {
            progress.check_read()?;
            let count = progress.finish_read_io(file.read(&mut chunk))?;
            if count == 0 {
                break;
            }
            if output.len() + count > MAX_FS_CONTENT_BYTES {
                return Err(limit_error(
                    "content-bytes",
                    output.len() + count,
                    MAX_FS_CONTENT_BYTES,
                ));
            }
            output.extend_from_slice(&chunk[..count]);
        }
        Ok(output)
    }

    fn write_allowed(
        &self,
        permit: &ScopePermit,
        path: &str,
        bytes: &[u8],
        progress: &Progress<'_>,
    ) -> Result<(), FsError> {
        progress.check_write(false, 0, bytes.len(), None)?;
        let grant = selected_grant(&self.write_grants, permit, path)?;
        let resolved = walk(grant, path, OpenPurpose::Write, progress)?;
        progress.check_write(false, 0, bytes.len(), None)?;

        let mut file = match resolved {
            WalkResult::File(file) => {
                let metadata = progress.finish_read_io(file.metadata())?;
                ensure_regular_and_device(&metadata, grant, path)?;
                progress.check_write(false, 0, bytes.len(), None)?;
                let result = file.set_len(0);
                progress.check_write(true, 0, bytes.len(), result.err())?;
                file
            }
            WalkResult::Missing { parent, leaf } => {
                progress.check_write(false, 0, bytes.len(), None)?;
                let mut options = write_options();
                options.create_new(true);
                let result = create_new_file(&parent, &leaf, &options);
                match result {
                    Ok(file) => {
                        progress.check_write(true, 0, bytes.len(), None)?;
                        let metadata = match file.metadata() {
                            Ok(metadata) => {
                                progress.check_write(true, 0, bytes.len(), None)?;
                                metadata
                            }
                            Err(source) => {
                                return progress.check_write(true, 0, bytes.len(), Some(source));
                            }
                        };
                        ensure_regular_and_device(&metadata, grant, path).map_err(|error| {
                            FsError::WriteEffect {
                                cause: WriteInterruption::Io(io::Error::other(error.to_string())),
                                committed_bytes: 0,
                                requested_bytes: bytes.len() as u64,
                            }
                        })?;
                        file
                    }
                    Err(source) if source.kind() == ErrorKind::AlreadyExists => {
                        progress.check_write(false, 0, bytes.len(), None)?;
                        return Err(FsError::ResolvedOutOfScope {
                            requested: path.to_owned(),
                            detail: "final leaf appeared before create-new".to_owned(),
                        });
                    }
                    Err(source) => {
                        return progress.check_write(true, 0, bytes.len(), Some(source));
                    }
                }
            }
        };

        let mut committed = 0_u64;
        for chunk in bytes.chunks(FS_IO_CHUNK_BYTES) {
            progress.check_write(true, committed, bytes.len(), None)?;
            match file.write(chunk) {
                Ok(0) => {
                    return Err(FsError::WriteEffect {
                        cause: WriteInterruption::Io(io::Error::new(
                            ErrorKind::WriteZero,
                            "content write returned zero",
                        )),
                        committed_bytes: committed,
                        requested_bytes: bytes.len() as u64,
                    });
                }
                Ok(count) => {
                    committed += count as u64;
                    progress.check_write(true, committed, bytes.len(), None)?;
                    if count != chunk.len() {
                        return Err(FsError::WriteEffect {
                            cause: WriteInterruption::Io(io::Error::new(
                                ErrorKind::WriteZero,
                                "short content write",
                            )),
                            committed_bytes: committed,
                            requested_bytes: bytes.len() as u64,
                        });
                    }
                }
                Err(source) => {
                    return progress.check_write(true, committed, bytes.len(), Some(source));
                }
            }
        }
        progress.check_write(true, committed, bytes.len(), None)
    }
}

fn prepare_capability(
    verified: &VerifiedManifest,
    capability: &'static str,
) -> Result<Vec<RetainedGrant>, FsPrepareError> {
    let scopes = path_scopes(verified.manifest(), capability)
        .map_err(|error| prepare_scope_error(capability, &error))?;
    scopes
        .map(|scope| prepare_grant(capability, scope))
        .collect()
}

fn prepare_scope_error(capability: &'static str, error: &ScopeSetError) -> FsPrepareError {
    let grant_index = match error {
        ScopeSetError::TooMany { .. } => 64,
        ScopeSetError::Duplicate {
            duplicate_index, ..
        } => *duplicate_index,
        ScopeSetError::InvalidPath { grant_index, .. } => *grant_index,
    };
    FsPrepareError::InvalidScope {
        capability,
        grant_index,
        detail: error.to_string(),
    }
}

fn prepare_grant(
    capability: &'static str,
    scope: PathScope<'_>,
) -> Result<RetainedGrant, FsPrepareError> {
    let pattern = scope.pattern();
    let (anchor, exact_leaf) = match scope.kind() {
        PathScopeKind::Subtree => {
            let root = pattern.strip_suffix("/**").unwrap_or(pattern);
            ((if root.is_empty() { "/" } else { root }).to_owned(), None)
        }
        PathScopeKind::Exact => {
            split_exact_scope(pattern).ok_or_else(|| FsPrepareError::InvalidScope {
                capability,
                grant_index: scope.grant_index(),
                detail: "exact scope has no retained parent and final leaf".to_owned(),
            })?
        }
    };
    let root = Dir::open_ambient_dir(&anchor, ambient_authority()).map_err(|source| {
        FsPrepareError::OpenScope {
            capability,
            grant_index: scope.grant_index(),
            source,
        }
    })?;
    let metadata = root
        .dir_metadata()
        .map_err(|source| FsPrepareError::OpenScope {
            capability,
            grant_index: scope.grant_index(),
            source,
        })?;
    if !metadata.is_dir() {
        return Err(FsPrepareError::InvalidScope {
            capability,
            grant_index: scope.grant_index(),
            detail: "retained anchor is not a directory".to_owned(),
        });
    }
    Ok(RetainedGrant {
        grant_index: scope.grant_index(),
        kind: scope.kind(),
        anchor,
        exact_leaf,
        root_device: metadata.dev(),
        root,
    })
}

fn split_exact_scope(path: &str) -> Option<(String, Option<String>)> {
    let index = path.rfind('/')?;
    let leaf = path.get(index + 1..)?;
    if leaf.is_empty() {
        return None;
    }
    let parent = if index == 0 {
        "/".to_owned()
    } else if index == 2 && path.as_bytes().get(1) == Some(&b':') {
        path[..=index].to_owned()
    } else {
        path[..index].to_owned()
    };
    Some((parent, Some(leaf.to_owned())))
}

fn selected_grant<'a>(
    grants: &'a [RetainedGrant],
    permit: &ScopePermit,
    requested: &str,
) -> Result<&'a RetainedGrant, FsError> {
    grants
        .iter()
        .find(|grant| grant.grant_index == permit.grant_index())
        .ok_or_else(|| FsError::ResolvedOutOfScope {
            requested: requested.to_owned(),
            detail: "matched guard grant has no retained resource".to_owned(),
        })
}

fn validate_request_shape(path: &str) -> Result<(), FsError> {
    if path.len() > MAX_FS_PATH_BYTES {
        return Err(limit_error("path-bytes", path.len(), MAX_FS_PATH_BYTES));
    }
    if path.contains('\0') {
        return Err(FsError::LimitExceeded {
            limit: "path-nul",
            actual: 1,
            maximum: 0,
        });
    }
    Ok(())
}

fn limit_error(limit: &'static str, actual: usize, maximum: usize) -> FsError {
    FsError::LimitExceeded {
        limit,
        actual: actual as u64,
        maximum: maximum as u64,
    }
}

#[derive(Clone, Copy)]
enum OpenPurpose {
    Read,
    Write,
}

enum WalkResult {
    File(File),
    Missing { parent: Dir, leaf: String },
}

#[derive(Debug)]
enum PendingComponent {
    Current,
    Parent,
    Normal(String),
}

// One explicit state machine keeps component counting, link expansion, retained
// directory ownership, and final no-follow opening in one auditable policy owner.
#[allow(clippy::too_many_lines, clippy::needless_continue)]
fn walk(
    grant: &RetainedGrant,
    requested: &str,
    purpose: OpenPurpose,
    progress: &Progress<'_>,
) -> Result<WalkResult, FsError> {
    let relative = relative_request(grant, requested)?;
    let mut pending = relative
        .split('/')
        .filter(|component| !component.is_empty())
        .map(|component| PendingComponent::Normal(component.to_owned()))
        .collect::<VecDeque<_>>();
    if pending.is_empty() {
        return Err(FsError::UnsupportedObject {
            requested: requested.to_owned(),
            detail: "scope root is a directory, not a regular file".to_owned(),
        });
    }
    let mut directories = vec![progress.finish_read_io(grant.root.try_clone())?];
    let mut components = 0_usize;
    let mut links = 0_usize;

    while let Some(component) = pending.pop_front() {
        progress.check_read()?;
        components += 1;
        if components > MAX_FS_COMPONENTS {
            return Err(limit_error(
                "processed-components",
                components,
                MAX_FS_COMPONENTS,
            ));
        }
        match component {
            PendingComponent::Current => continue,
            PendingComponent::Parent => {
                if directories.len() == 1 {
                    return Err(FsError::ResolvedOutOfScope {
                        requested: requested.to_owned(),
                        detail: "relative link escaped above the retained root".to_owned(),
                    });
                }
                directories.pop();
                continue;
            }
            PendingComponent::Normal(name) => {
                validate_fs_component(&name).map_err(|detail| FsError::ResolvedOutOfScope {
                    requested: requested.to_owned(),
                    detail,
                })?;
                let final_component = pending.is_empty();
                let current = directories
                    .last()
                    .ok_or_else(|| FsError::ResolvedOutOfScope {
                        requested: requested.to_owned(),
                        detail: "retained directory stack became empty".to_owned(),
                    })?;
                let metadata_result = current.symlink_metadata(&name);
                progress.check_read()?;
                let metadata = match metadata_result {
                    Ok(metadata) => metadata,
                    Err(source)
                        if source.kind() == ErrorKind::NotFound
                            && final_component
                            && matches!(purpose, OpenPurpose::Write) =>
                    {
                        return Ok(WalkResult::Missing {
                            parent: progress.finish_read_io(current.try_clone())?,
                            leaf: name,
                        });
                    }
                    Err(source) => return Err(FsError::Io(source)),
                };
                if unsupported_reparse(&metadata) {
                    return Err(FsError::UnsupportedObject {
                        requested: requested.to_owned(),
                        detail: "unsupported reparse object".to_owned(),
                    });
                }
                if metadata.file_type().is_symlink() {
                    if grant.kind == PathScopeKind::Exact {
                        return Err(FsError::ResolvedOutOfScope {
                            requested: requested.to_owned(),
                            detail: "an exact-file grant never follows its final alias".to_owned(),
                        });
                    }
                    links += 1;
                    if links > MAX_FS_SYMLINK_EXPANSIONS {
                        return Err(limit_error(
                            "symlink-expansions",
                            links,
                            MAX_FS_SYMLINK_EXPANSIONS,
                        ));
                    }
                    let target = progress.finish_read_io(current.read_link_contents(&name))?;
                    let inserted = link_components(&target, requested)?;
                    for target_component in inserted.into_iter().rev() {
                        pending.push_front(target_component);
                    }
                    continue;
                }
                if final_component && !metadata.is_file() {
                    return Err(FsError::UnsupportedObject {
                        requested: requested.to_owned(),
                        detail: "target is not a regular file".to_owned(),
                    });
                }
                if final_component {
                    let options = match purpose {
                        OpenPurpose::Read => read_options(),
                        OpenPurpose::Write => write_options(),
                    };
                    let file_result = open_existing_file(current, &name, purpose, &options);
                    progress.check_read()?;
                    let file = file_result.map_err(|source| {
                        if matches!(
                            source.kind(),
                            ErrorKind::PermissionDenied | ErrorKind::InvalidInput
                        ) {
                            FsError::ResolvedOutOfScope {
                                requested: requested.to_owned(),
                                detail: "final object changed during no-follow open".to_owned(),
                            }
                        } else {
                            FsError::Io(source)
                        }
                    })?;
                    let opened = progress.finish_read_io(file.metadata())?;
                    ensure_regular_and_device(&opened, grant, requested)?;
                    return Ok(WalkResult::File(file));
                }
                if !metadata.is_dir() {
                    return Err(FsError::UnsupportedObject {
                        requested: requested.to_owned(),
                        detail: "intermediate component is not a directory".to_owned(),
                    });
                }
                let directory_result = open_child_directory(current, &name);
                progress.check_read()?;
                let directory = directory_result.map_err(|source| {
                    if matches!(
                        source.kind(),
                        ErrorKind::PermissionDenied | ErrorKind::InvalidInput
                    ) {
                        FsError::ResolvedOutOfScope {
                            requested: requested.to_owned(),
                            detail: "directory component changed during no-follow open".to_owned(),
                        }
                    } else {
                        FsError::Io(source)
                    }
                })?;
                let opened = progress.finish_read_io(directory.dir_metadata())?;
                if opened.dev() != grant.root_device {
                    return Err(FsError::UnsupportedObject {
                        requested: requested.to_owned(),
                        detail: "mount or volume boundary crossed".to_owned(),
                    });
                }
                directories.push(directory);
            }
        }
    }
    Err(FsError::Io(io::Error::new(
        ErrorKind::NotFound,
        "retained traversal produced no final object",
    )))
}

fn relative_request<'a>(grant: &'a RetainedGrant, requested: &'a str) -> Result<&'a str, FsError> {
    match grant.kind {
        PathScopeKind::Exact => {
            let leaf = grant
                .exact_leaf
                .as_deref()
                .ok_or_else(|| FsError::ResolvedOutOfScope {
                    requested: requested.to_owned(),
                    detail: "exact grant has no retained final leaf".to_owned(),
                })?;
            Ok(leaf)
        }
        PathScopeKind::Subtree => {
            if requested == grant.anchor {
                return Ok("");
            }
            requested
                .strip_prefix(&grant.anchor)
                .and_then(|suffix| suffix.strip_prefix('/'))
                .ok_or_else(|| FsError::ResolvedOutOfScope {
                    requested: requested.to_owned(),
                    detail: "request does not share the selected retained root spelling".to_owned(),
                })
        }
    }
}

fn link_components(target: &Path, requested: &str) -> Result<Vec<PendingComponent>, FsError> {
    let target = target.to_str().ok_or_else(|| FsError::ResolvedOutOfScope {
        requested: requested.to_owned(),
        detail: "link target is not UTF-8".to_owned(),
    })?;
    #[cfg(windows)]
    if Path::new(target).components().any(|component| {
        matches!(
            component,
            std::path::Component::Prefix(_) | std::path::Component::RootDir
        )
    }) {
        return Err(FsError::ResolvedOutOfScope {
            requested: requested.to_owned(),
            detail: "absolute, rooted, or prefixed link target is forbidden".to_owned(),
        });
    }
    #[cfg(not(windows))]
    if target.starts_with('/') {
        return Err(FsError::ResolvedOutOfScope {
            requested: requested.to_owned(),
            detail: "absolute, rooted, or prefixed link target is forbidden".to_owned(),
        });
    }

    let mut components = Vec::new();
    #[cfg(windows)]
    let names = target.split(['/', '\\']);
    #[cfg(not(windows))]
    let names = target.split('/');
    for name in names.filter(|name| !name.is_empty()) {
        match name {
            "." => components.push(PendingComponent::Current),
            ".." => components.push(PendingComponent::Parent),
            name => {
                validate_fs_component(name).map_err(|detail| FsError::ResolvedOutOfScope {
                    requested: requested.to_owned(),
                    detail,
                })?;
                components.push(PendingComponent::Normal(name.to_owned()));
            }
        }
    }
    Ok(components)
}

fn read_options() -> OpenOptions {
    let mut options = OpenOptions::new();
    options.read(true);
    options.follow(FollowSymlinks::No);
    set_nonblocking(&mut options);
    options
}

fn write_options() -> OpenOptions {
    let mut options = OpenOptions::new();
    options.write(true);
    options.follow(FollowSymlinks::No);
    set_nonblocking(&mut options);
    options
}

#[cfg(target_os = "linux")]
fn linux_resolve_flags() -> rustix::fs::ResolveFlags {
    rustix::fs::ResolveFlags::BENEATH
        | rustix::fs::ResolveFlags::NO_MAGICLINKS
        | rustix::fs::ResolveFlags::NO_XDEV
}

#[cfg(target_os = "linux")]
fn open_existing_file(
    directory: &Dir,
    name: &str,
    purpose: OpenPurpose,
    _options: &OpenOptions,
) -> io::Result<File> {
    let access = match purpose {
        OpenPurpose::Read => rustix::fs::OFlags::RDONLY,
        OpenPurpose::Write => rustix::fs::OFlags::WRONLY,
    };
    let descriptor = rustix::fs::openat2(
        directory,
        name,
        access
            | rustix::fs::OFlags::CLOEXEC
            | rustix::fs::OFlags::NOFOLLOW
            | rustix::fs::OFlags::NONBLOCK,
        rustix::fs::Mode::empty(),
        linux_resolve_flags(),
    )?;
    Ok(File::from_std(std::fs::File::from(descriptor)))
}

#[cfg(not(target_os = "linux"))]
fn open_existing_file(
    directory: &Dir,
    name: &str,
    _purpose: OpenPurpose,
    options: &OpenOptions,
) -> io::Result<File> {
    directory.open_with(name, options)
}

#[cfg(target_os = "linux")]
fn open_child_directory(directory: &Dir, name: &str) -> io::Result<Dir> {
    let descriptor = rustix::fs::openat2(
        directory,
        name,
        rustix::fs::OFlags::RDONLY
            | rustix::fs::OFlags::CLOEXEC
            | rustix::fs::OFlags::DIRECTORY
            | rustix::fs::OFlags::NOFOLLOW
            | rustix::fs::OFlags::NONBLOCK,
        rustix::fs::Mode::empty(),
        linux_resolve_flags(),
    )?;
    Ok(Dir::from_std_file(std::fs::File::from(descriptor)))
}

#[cfg(not(target_os = "linux"))]
fn open_child_directory(directory: &Dir, name: &str) -> io::Result<Dir> {
    directory.open_dir_nofollow(name)
}

#[cfg(target_os = "linux")]
fn create_new_file(directory: &Dir, name: &str, _options: &OpenOptions) -> io::Result<File> {
    let descriptor = rustix::fs::openat2(
        directory,
        name,
        rustix::fs::OFlags::WRONLY
            | rustix::fs::OFlags::CREATE
            | rustix::fs::OFlags::EXCL
            | rustix::fs::OFlags::CLOEXEC
            | rustix::fs::OFlags::NOFOLLOW
            | rustix::fs::OFlags::NONBLOCK,
        rustix::fs::Mode::from_raw_mode(0o666),
        linux_resolve_flags(),
    )?;
    Ok(File::from_std(std::fs::File::from(descriptor)))
}

#[cfg(not(target_os = "linux"))]
fn create_new_file(directory: &Dir, name: &str, options: &OpenOptions) -> io::Result<File> {
    directory.open_with(name, options)
}

#[cfg(unix)]
fn set_nonblocking(options: &mut OpenOptions) {
    use cap_std::fs::OpenOptionsExt;
    options.custom_flags(rustix::fs::OFlags::NONBLOCK.bits().cast_signed());
}

#[cfg(not(unix))]
fn set_nonblocking(_options: &mut OpenOptions) {}

fn ensure_regular_and_device(
    metadata: &cap_std::fs::Metadata,
    grant: &RetainedGrant,
    requested: &str,
) -> Result<(), FsError> {
    if metadata.dev() != grant.root_device {
        return Err(FsError::UnsupportedObject {
            requested: requested.to_owned(),
            detail: "mount or volume boundary crossed".to_owned(),
        });
    }
    if !metadata.is_file() {
        return Err(FsError::UnsupportedObject {
            requested: requested.to_owned(),
            detail: "target is not a regular file".to_owned(),
        });
    }
    Ok(())
}

#[cfg(windows)]
fn unsupported_reparse(metadata: &cap_std::fs::Metadata) -> bool {
    use cap_std::fs::MetadataExt as _;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
        && !metadata.file_type().is_symlink()
}

#[cfg(not(windows))]
fn unsupported_reparse(_metadata: &cap_std::fs::Metadata) -> bool {
    false
}

struct Progress<'a> {
    cancelled: &'a AtomicBool,
    deadline: Instant,
}

impl<'a> Progress<'a> {
    fn new(cancelled: &'a AtomicBool) -> Self {
        let start = Instant::now();
        let deadline = start.checked_add(FS_OPERATION_BUDGET).unwrap_or(start);
        Self {
            cancelled,
            deadline,
        }
    }

    fn cause(&self) -> Option<ProgressCause> {
        if self.cancelled.load(Ordering::Acquire) {
            Some(ProgressCause::Cancelled)
        } else if Instant::now() >= self.deadline {
            Some(ProgressCause::Deadline)
        } else {
            None
        }
    }

    fn check_read(&self) -> Result<(), FsError> {
        match self.cause() {
            Some(ProgressCause::Cancelled) => Err(FsError::Cancelled),
            Some(ProgressCause::Deadline) => Err(FsError::Deadline),
            None => Ok(()),
        }
    }

    fn finish_read_io<T>(&self, result: io::Result<T>) -> Result<T, FsError> {
        self.check_read()?;
        result.map_err(FsError::Io)
    }

    fn check_write(
        &self,
        effected: bool,
        committed: u64,
        requested: usize,
        stage_error: Option<io::Error>,
    ) -> Result<(), FsError> {
        let cause = match self.cause() {
            Some(ProgressCause::Cancelled) => Some(WriteInterruption::Cancelled),
            Some(ProgressCause::Deadline) => Some(WriteInterruption::Deadline),
            None => stage_error.map(WriteInterruption::Io),
        };
        let Some(cause) = cause else {
            return Ok(());
        };
        if effected {
            Err(FsError::WriteEffect {
                cause,
                committed_bytes: committed,
                requested_bytes: requested as u64,
            })
        } else {
            match cause {
                WriteInterruption::Cancelled => Err(FsError::Cancelled),
                WriteInterruption::Deadline => Err(FsError::Deadline),
                WriteInterruption::Io(source) => Err(FsError::Io(source)),
            }
        }
    }
}

enum ProgressCause {
    Deadline,
    Cancelled,
}

/// Serves retained filesystem calls for one already-admitted kipc session.
///
/// # Errors
///
/// Returns [`IpcError`] on handshake, framing, codec, or stream failure.
pub fn serve_fs_session<S: Read + Write>(
    stream: &mut S,
    token: &SessionToken,
    broker: &FsBroker,
    verified: &VerifiedManifest,
    principal: Principal,
    cancelled: &AtomicBool,
) -> Result<(), IpcError> {
    handshake_server(stream, token)?;
    let policy = ReceivePolicy::privileged_call_receiver(FS_CHANNEL);
    loop {
        let (header, payload) = match read_validated_frame(stream, &policy) {
            Ok(frame) => frame,
            Err(IpcError::Io(error)) if error.kind() == ErrorKind::UnexpectedEof => break,
            Err(error) => return Err(error),
        };
        let request: FsRequest = decode(&payload)?;
        let result = match request {
            FsRequest::Read { path } => broker
                .read(verified, principal, &path, cancelled)
                .map(|bytes| FsResponse::Read { bytes }),
            FsRequest::Write { path, bytes } => broker
                .write(verified, principal, &path, &bytes, cancelled)
                .map(|()| FsResponse::Write),
        };
        match result {
            Ok(response) => {
                let bytes = encode(&response)?;
                write_frame(
                    stream,
                    FrameKind::Reply,
                    0,
                    FS_CHANNEL,
                    header.corr(),
                    &bytes,
                )?;
            }
            Err(error) => {
                keld_ipc::write_call_error(
                    stream,
                    FS_CHANNEL,
                    header.corr(),
                    &keld_ipc::CallError::from(&error),
                )?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use keld_guard::parse_manifest;

    #[test]
    fn error_codes_and_messages_stay_bound() {
        let reason = match keld_guard::evaluate(
            &parse_manifest("{}").expect("manifest"),
            Principal::AppProcess,
            FS_READ_CAPABILITY,
            "/tmp/x",
        ) {
            keld_guard::Decision::Deny(reason) => reason,
            keld_guard::Decision::Allow(_) => unreachable!("empty manifest denies"),
        };
        let errors = [
            FsError::Denied(reason),
            FsError::Io(io::Error::new(ErrorKind::NotFound, "missing")),
            FsError::ResolvedOutOfScope {
                requested: "/x".to_owned(),
                detail: "escape".to_owned(),
            },
            FsError::UnsupportedObject {
                requested: "/x".to_owned(),
                detail: "directory".to_owned(),
            },
            limit_error("content-bytes", 2, 1),
            FsError::Deadline,
            FsError::Cancelled,
            FsError::WriteEffect {
                cause: WriteInterruption::Cancelled,
                committed_bytes: 1,
                requested_bytes: 2,
            },
            FsError::SnapshotMismatch {
                prepared: [1; 32],
                presented: [2; 32],
            },
        ];
        let codes = [
            "KELD-GUARD001",
            "KELD-NATIVE-001",
            "KELD-NATIVE-002",
            "KELD-NATIVE-003",
            "KELD-NATIVE-004",
            "KELD-NATIVE-005",
            "KELD-NATIVE-006",
            "KELD-NATIVE-007",
            "KELD-NATIVE-008",
        ];
        for (error, code) in errors.iter().zip(codes) {
            assert_eq!(error.code(), code);
            assert!(error.to_string().starts_with(code));
            let wire = keld_ipc::CallError::from(error);
            assert_eq!(wire.code, code);
            assert_eq!(wire.message, error.to_string());
        }
    }

    #[test]
    fn cancellation_wins_over_deadline_and_stage_error() {
        let cancelled = AtomicBool::new(true);
        let progress = Progress {
            cancelled: &cancelled,
            deadline: Instant::now(),
        };
        assert!(matches!(progress.check_read(), Err(FsError::Cancelled)));
        let error = progress
            .check_write(true, 3, 8, Some(io::Error::other("simultaneous failure")))
            .expect_err("cancel wins");
        assert!(matches!(
            error,
            FsError::WriteEffect {
                cause: WriteInterruption::Cancelled,
                committed_bytes: 3,
                requested_bytes: 8
            }
        ));

        let running = AtomicBool::new(false);
        let expired = Progress {
            cancelled: &running,
            deadline: Instant::now(),
        };
        assert!(matches!(expired.check_read(), Err(FsError::Deadline)));
        let error = expired
            .check_write(true, 4, 8, Some(io::Error::other("late failure")))
            .expect_err("deadline wins over returned error");
        assert!(matches!(
            error,
            FsError::WriteEffect {
                cause: WriteInterruption::Deadline,
                committed_bytes: 4,
                requested_bytes: 8
            }
        ));
    }
}

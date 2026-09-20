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
            let remaining = MAX_FS_CONTENT_BYTES + 1 - output.len();
            let read_length = remaining.min(chunk.len());
            let count = progress.finish_read_io(file.read(&mut chunk[..read_length]))?;
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
        #[cfg(test)]
        run_after_walk_test_hook();
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

        write_content(&mut file, bytes, progress)
    }
}

#[cfg(test)]
std::thread_local! {
    static AFTER_WALK_TEST_HOOK: std::cell::RefCell<Option<Box<dyn FnOnce()>>> =
        const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
fn run_after_walk_test_hook() {
    if let Some(hook) = AFTER_WALK_TEST_HOOK.with(|slot| slot.borrow_mut().take()) {
        hook();
    }
}

fn write_content<W: Write>(
    writer: &mut W,
    bytes: &[u8],
    progress: &Progress<'_>,
) -> Result<(), FsError> {
    let mut committed = 0_u64;
    let mut offset = 0_usize;
    while offset < bytes.len() {
        progress.check_write(true, committed, bytes.len(), None)?;
        let end = (offset + FS_IO_CHUNK_BYTES).min(bytes.len());
        match writer.write(&bytes[offset..end]) {
            Ok(0) => {
                return progress.check_write(
                    true,
                    committed,
                    bytes.len(),
                    Some(io::Error::new(
                        ErrorKind::WriteZero,
                        "content write returned zero",
                    )),
                );
            }
            Ok(count) => {
                offset += count;
                committed += count as u64;
                progress.check_write(true, committed, bytes.len(), None)?;
            }
            Err(source) => {
                return progress.check_write(true, committed, bytes.len(), Some(source));
            }
        }
    }
    progress.check_write(true, committed, bytes.len(), None)
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
            #[cfg(windows)]
            let root = if root.len() == 2
                && root.as_bytes()[0].is_ascii_uppercase()
                && root.as_bytes()[1] == b':'
            {
                format!("{root}/")
            } else {
                (if root.is_empty() { "/" } else { root }).to_owned()
            };
            #[cfg(not(windows))]
            let root = (if root.is_empty() { "/" } else { root }).to_owned();
            (root, None)
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

#[derive(Debug)]
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

fn walk(
    grant: &RetainedGrant,
    requested: &str,
    purpose: OpenPurpose,
    progress: &Progress<'_>,
) -> Result<WalkResult, FsError> {
    walk_with_observer(grant, requested, purpose, progress, |_, _| {})
}

// One explicit state machine keeps component counting, link expansion, retained
// directory ownership, and final no-follow opening in one auditable policy owner.
// The test observer runs after metadata and before open; production supplies a
// zero-sized no-op closure, so race oracles exercise the same state machine.
#[allow(clippy::too_many_lines, clippy::needless_continue)]
fn walk_with_observer(
    grant: &RetainedGrant,
    requested: &str,
    purpose: OpenPurpose,
    progress: &Progress<'_>,
    mut after_metadata: impl FnMut(&str, bool),
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
                if metadata_result.is_ok() {
                    after_metadata(&name, final_component);
                }
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
                        classify_component_open_error(
                            source,
                            requested,
                            "final object changed during no-follow open",
                        )
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
                    classify_component_open_error(
                        source,
                        requested,
                        "directory component changed during no-follow open",
                    )
                })?;
                let opened = progress.finish_read_io(directory.dir_metadata())?;
                #[cfg(windows)]
                ensure_acquired_not_reparse(&opened, requested)?;
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

fn classify_component_open_error(source: io::Error, requested: &str, detail: &str) -> FsError {
    #[cfg(target_os = "linux")]
    let crosses_device = source.raw_os_error() == Some(rustix::io::Errno::XDEV.raw_os_error());
    #[cfg(not(target_os = "linux"))]
    let crosses_device = source.kind() == ErrorKind::CrossesDevices;
    if crosses_device {
        return FsError::UnsupportedObject {
            requested: requested.to_owned(),
            detail: "mount or volume boundary crossed".to_owned(),
        };
    }
    #[cfg(target_os = "linux")]
    let namespace_race = source.raw_os_error().is_some_and(|raw| {
        raw == rustix::io::Errno::LOOP.raw_os_error()
            || raw == rustix::io::Errno::NOTDIR.raw_os_error()
            || raw == rustix::io::Errno::NOENT.raw_os_error()
    });
    #[cfg(not(target_os = "linux"))]
    let namespace_race = matches!(
        source.kind(),
        ErrorKind::NotADirectory
            | ErrorKind::NotFound
            | ErrorKind::PermissionDenied
            | ErrorKind::InvalidInput
    );
    #[cfg(all(unix, not(target_os = "linux")))]
    let namespace_race =
        namespace_race || source.raw_os_error() == Some(rustix::io::Errno::LOOP.raw_os_error());
    if namespace_race {
        FsError::ResolvedOutOfScope {
            requested: requested.to_owned(),
            detail: detail.to_owned(),
        }
    } else {
        FsError::Io(source)
    }
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
            strip_subtree_anchor(&grant.anchor, requested).ok_or_else(|| {
                FsError::ResolvedOutOfScope {
                    requested: requested.to_owned(),
                    detail: "request does not share the selected retained root spelling".to_owned(),
                }
            })
        }
    }
}

fn strip_subtree_anchor<'requested>(
    anchor: &str,
    requested: &'requested str,
) -> Option<&'requested str> {
    let suffix = requested.strip_prefix(anchor)?;
    if anchor.ends_with('/') {
        Some(suffix)
    } else {
        suffix.strip_prefix('/')
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
    #[cfg(windows)]
    ensure_acquired_not_reparse(metadata, requested)?;
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
#[cfg(test)]
std::thread_local! {
    static ACQUIRED_REPARSE_CHECKS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(windows)]
fn ensure_acquired_not_reparse(
    metadata: &cap_std::fs::Metadata,
    requested: &str,
) -> Result<(), FsError> {
    use cap_std::fs::MetadataExt as _;

    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;

    #[cfg(test)]
    ACQUIRED_REPARSE_CHECKS.with(|checks| checks.set(checks.get() + 1));

    if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(FsError::UnsupportedObject {
            requested: requested.to_owned(),
            detail: "acquired handle is an unsupported reparse object".to_owned(),
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
    #[cfg(test)]
    now: fn() -> Instant,
}

impl<'a> Progress<'a> {
    fn new(cancelled: &'a AtomicBool) -> Self {
        let start = Instant::now();
        let deadline = start.checked_add(FS_OPERATION_BUDGET).unwrap_or(start);
        Self {
            cancelled,
            deadline,
            #[cfg(test)]
            now: Instant::now,
        }
    }

    fn cause(&self) -> Option<ProgressCause> {
        if self.cancelled.load(Ordering::Acquire) {
            Some(ProgressCause::Cancelled)
        } else {
            #[cfg(test)]
            let now = (self.now)();
            #[cfg(not(test))]
            let now = Instant::now();
            if now >= self.deadline {
                Some(ProgressCause::Deadline)
            } else {
                None
            }
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
    #[cfg(windows)]
    use keld_guard::verified_manifest::{VerifiedManifest, load_verified_manifest};
    #[cfg(windows)]
    use sha2::{Digest as _, Sha256};

    std::thread_local! {
        static TEST_NOW: std::cell::Cell<Option<Instant>> = const { std::cell::Cell::new(None) };
    }

    fn test_now() -> Instant {
        TEST_NOW.with(|now| now.get().expect("test clock initialized"))
    }

    fn set_test_now(now: Instant) {
        TEST_NOW.with(|clock| clock.set(Some(now)));
    }

    #[cfg(windows)]
    fn test_verified_manifest(fixture: &Path, root: &Path) -> VerifiedManifest {
        let scope = root.to_str().expect("UTF-8 fixture").replace('\\', "/");
        let text =
            format!(r#"{{"app":{{"fs":{{"read":["{scope}/**"],"write":["{scope}/**"]}}}}}}"#);
        let path = fixture.join("permissions.jsonc");
        std::fs::write(&path, &text).expect("write verified manifest");
        let digest: [u8; 32] = Sha256::digest(text.as_bytes()).into();
        load_verified_manifest(
            std::fs::File::open(&path).expect("open verified manifest"),
            path,
            digest,
        )
        .expect("load verified manifest")
    }

    #[cfg(windows)]
    fn test_owner_handle_count() -> usize {
        let output = std::process::Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "(Get-Process -Id $env:KEL130_PARENT_PID).HandleCount",
            ])
            .env("KEL130_PARENT_PID", std::process::id().to_string())
            .output()
            .expect("query owner process handle count");
        assert!(
            output.status.success(),
            "handle census failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout)
            .expect("handle count is UTF-8")
            .trim()
            .parse()
            .expect("handle count is an integer")
    }

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
            now: Instant::now,
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
            now: Instant::now,
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

    #[test]
    fn production_write_loop_records_progress_before_selecting_terminal_cause() {
        struct FirstWriteThen<'a> {
            cancelled: &'a AtomicBool,
            deadline: Instant,
            action: AfterWrite,
            writes: usize,
        }

        #[derive(Clone, Copy)]
        enum AfterWrite {
            CancelAndExpire,
            Expire,
        }

        impl Write for FirstWriteThen<'_> {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                self.writes += 1;
                let count = bytes.len().min(3);
                match self.action {
                    AfterWrite::CancelAndExpire => {
                        self.cancelled.store(true, Ordering::Release);
                        set_test_now(self.deadline);
                    }
                    AfterWrite::Expire => set_test_now(self.deadline),
                }
                Ok(count)
            }

            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }

        let start = Instant::now();
        let deadline = start + std::time::Duration::from_secs(1);
        let cancelled = AtomicBool::new(false);
        set_test_now(start);
        let progress = Progress {
            cancelled: &cancelled,
            deadline,
            now: test_now,
        };
        let mut cancel_writer = FirstWriteThen {
            cancelled: &cancelled,
            deadline,
            action: AfterWrite::CancelAndExpire,
            writes: 0,
        };
        let error = write_content(&mut cancel_writer, b"12345678", &progress)
            .expect_err("late cancellation must preserve the first acknowledged write");
        assert_eq!(cancel_writer.writes, 1);
        assert!(matches!(
            error,
            FsError::WriteEffect {
                cause: WriteInterruption::Cancelled,
                committed_bytes: 3,
                requested_bytes: 8
            }
        ));

        cancelled.store(false, Ordering::Release);
        set_test_now(start);
        let mut deadline_writer = FirstWriteThen {
            cancelled: &cancelled,
            deadline,
            action: AfterWrite::Expire,
            writes: 0,
        };
        let error = write_content(&mut deadline_writer, b"12345678", &progress)
            .expect_err("one absolute deadline must expire after partial progress");
        assert_eq!(deadline_writer.writes, 1);
        assert!(matches!(
            error,
            FsError::WriteEffect {
                cause: WriteInterruption::Deadline,
                committed_bytes: 3,
                requested_bytes: 8
            }
        ));
    }

    #[test]
    fn production_write_loop_counts_only_successful_write_returns() {
        struct PartialThenError {
            writes: usize,
        }

        impl Write for PartialThenError {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                self.writes += 1;
                if self.writes == 1 {
                    Ok(bytes.len().min(3))
                } else {
                    Err(io::Error::other("injected write failure"))
                }
            }

            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }

        let cancelled = AtomicBool::new(false);
        let progress = Progress::new(&cancelled);
        let mut writer = PartialThenError { writes: 0 };
        let error = write_content(&mut writer, b"12345678", &progress)
            .expect_err("second write fails after one acknowledged partial write");
        assert_eq!(writer.writes, 2);
        assert!(matches!(
            error,
            FsError::WriteEffect {
                cause: WriteInterruption::Io(_),
                committed_bytes: 3,
                requested_bytes: 8
            }
        ));
    }

    #[test]
    fn volume_root_anchors_keep_descendant_suffixes() {
        assert_eq!(strip_subtree_anchor("/", "/tmp/file"), Some("tmp/file"));
        assert_eq!(
            strip_subtree_anchor("C:/", "C:/Users/file"),
            Some("Users/file")
        );
        assert_eq!(
            strip_subtree_anchor("/tmp/root", "/tmp/root/file"),
            Some("file")
        );
        assert_eq!(strip_subtree_anchor("/tmp/root", "/tmp/rooted"), None);
    }

    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    fn race_owned_root(case: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "keld-kel130-linux-race-{case}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        std::fs::create_dir(&root).expect("create fixture root");
        root
    }

    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    fn race_grant(root: &Path) -> RetainedGrant {
        let directory = Dir::open_ambient_dir(root, ambient_authority()).expect("retain root");
        let root_device = directory.dir_metadata().expect("root metadata").dev();
        #[cfg(windows)]
        let anchor = root.to_str().expect("UTF-8 fixture").replace('\\', "/");
        #[cfg(not(windows))]
        let anchor = root.to_str().expect("UTF-8 fixture").to_owned();
        RetainedGrant {
            grant_index: 0,
            kind: PathScopeKind::Subtree,
            anchor,
            exact_leaf: None,
            root: directory,
            root_device,
        }
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn assert_fresh_race_read(grant: &RetainedGrant, root: &Path, progress: &Progress<'_>) {
        let requested = root.join("fresh");
        let result = walk(
            grant,
            requested.to_str().expect("UTF-8 fixture"),
            OpenPurpose::Read,
            progress,
        )
        .expect("fresh retained read");
        let WalkResult::File(mut file) = result else {
            panic!("fresh file must exist");
        };
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).expect("read fresh file");
        assert_eq!(bytes, b"fresh");
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn unix_final_swap_is_namespace_race_and_never_reads_outside() {
        use std::os::unix::fs::symlink;
        use std::sync::{Arc, Barrier};

        let fixture = race_owned_root("final");
        let granted = fixture.join("granted");
        let outside = fixture.join("outside");
        std::fs::create_dir(&granted).expect("granted root");
        std::fs::write(granted.join("victim"), b"inside").expect("inside file");
        std::fs::write(granted.join("fresh"), b"fresh").expect("fresh file");
        std::fs::write(&outside, b"outside").expect("outside file");
        let grant = race_grant(&granted);
        let cancelled = AtomicBool::new(false);
        let progress = Progress::new(&cancelled);
        let reached = Arc::new(Barrier::new(2));
        let swapped = Arc::new(Barrier::new(2));
        let worker_reached = Arc::clone(&reached);
        let worker_swapped = Arc::clone(&swapped);
        let victim = granted.join("victim");
        let outside_for_worker = outside.clone();
        let worker = std::thread::spawn(move || {
            worker_reached.wait();
            std::fs::remove_file(&victim).expect("remove original final file");
            symlink(&outside_for_worker, &victim).expect("install outside final link");
            worker_swapped.wait();
        });
        let requested = granted.join("victim");
        let error = walk_with_observer(
            &grant,
            requested.to_str().expect("UTF-8 fixture"),
            OpenPurpose::Read,
            &progress,
            |name, final_component| {
                if name == "victim" && final_component {
                    reached.wait();
                    swapped.wait();
                }
            },
        )
        .expect_err("final swap must fail closed");
        worker.join().expect("swap worker");
        assert_eq!(error.code(), "KELD-NATIVE-002");
        assert_eq!(
            std::fs::read(&outside).expect("outside sentinel"),
            b"outside"
        );
        assert_fresh_race_read(&grant, &granted, &progress);
        drop(grant);
        std::fs::remove_dir_all(&fixture).expect("cleanup fixture");
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn unix_intermediate_swap_is_namespace_race_and_never_reads_outside() {
        use std::os::unix::fs::symlink;
        use std::sync::{Arc, Barrier};

        let fixture = race_owned_root("intermediate");
        let granted = fixture.join("granted");
        let outside = fixture.join("outside");
        std::fs::create_dir(&granted).expect("granted root");
        std::fs::create_dir(granted.join("victim")).expect("inside directory");
        std::fs::write(granted.join("victim/sentinel"), b"inside").expect("inside sentinel");
        std::fs::write(granted.join("fresh"), b"fresh").expect("fresh file");
        std::fs::create_dir(&outside).expect("outside directory");
        std::fs::write(outside.join("sentinel"), b"outside").expect("outside sentinel");
        let grant = race_grant(&granted);
        let cancelled = AtomicBool::new(false);
        let progress = Progress::new(&cancelled);
        let reached = Arc::new(Barrier::new(2));
        let swapped = Arc::new(Barrier::new(2));
        let worker_reached = Arc::clone(&reached);
        let worker_swapped = Arc::clone(&swapped);
        let victim = granted.join("victim");
        let original = granted.join("original");
        let outside_for_worker = outside.clone();
        let worker = std::thread::spawn(move || {
            worker_reached.wait();
            std::fs::rename(&victim, &original).expect("move original directory");
            symlink(&outside_for_worker, &victim).expect("install outside directory link");
            worker_swapped.wait();
        });
        let requested = granted.join("victim/sentinel");
        let error = walk_with_observer(
            &grant,
            requested.to_str().expect("UTF-8 fixture"),
            OpenPurpose::Read,
            &progress,
            |name, final_component| {
                if name == "victim" && !final_component {
                    reached.wait();
                    swapped.wait();
                }
            },
        )
        .expect_err("intermediate swap must fail closed");
        worker.join().expect("swap worker");
        assert_eq!(error.code(), "KELD-NATIVE-002");
        assert_eq!(
            std::fs::read(outside.join("sentinel")).expect("outside sentinel"),
            b"outside"
        );
        assert_fresh_race_read(&grant, &granted, &progress);
        drop(grant);
        std::fs::remove_dir_all(&fixture).expect("cleanup fixture");
    }

    #[cfg(windows)]
    #[test]
    fn windows_acquired_metadata_rejects_reparse_objects_as_003() {
        let fixture = race_owned_root("windows-acquired-reparse");
        let target = fixture.join("target");
        let alias = fixture.join("alias");
        std::fs::create_dir(&target).expect("target directory");
        std::fs::write(target.join("sentinel"), b"outside").expect("target file");
        let junction = std::process::Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "$ErrorActionPreference='Stop'; New-Item -ItemType Junction -Path $env:KEL130_LINK -Target $env:KEL130_TARGET | Out-Null",
            ])
            .env("KEL130_LINK", &alias)
            .env("KEL130_TARGET", &target)
            .output()
            .expect("junction command");
        assert!(
            junction.status.success(),
            "Windows refused the native junction fixture: {}",
            String::from_utf8_lossy(&junction.stderr)
        );
        let directory =
            Dir::open_ambient_dir(&fixture, ambient_authority()).expect("retain fixture root");
        let metadata = directory
            .symlink_metadata("alias")
            .expect("read no-follow reparse metadata");
        let error = ensure_acquired_not_reparse(&metadata, "alias")
            .expect_err("acquired-handle validation must reject every reparse attribute");

        assert_eq!(error.code(), "KELD-NATIVE-003");
        assert!(
            error
                .to_string()
                .contains("acquired handle is an unsupported reparse object")
        );
        assert_eq!(
            std::fs::read(target.join("sentinel")).expect("target sentinel"),
            b"outside"
        );
        drop(directory);
        std::fs::remove_dir(&alias).expect("remove only owned junction");
        std::fs::remove_dir_all(&fixture).expect("cleanup fixture");
    }

    #[cfg(windows)]
    #[test]
    fn windows_walk_validates_intermediate_and_final_acquired_handles() {
        let fixture = race_owned_root("windows-acquired-check-wiring");
        let granted = fixture.join("granted");
        let nested = granted.join("nested");
        std::fs::create_dir_all(&nested).expect("nested directory");
        std::fs::write(nested.join("file"), b"inside").expect("inside file");
        let grant = race_grant(&granted);
        let cancelled = AtomicBool::new(false);
        let progress = Progress::new(&cancelled);

        ACQUIRED_REPARSE_CHECKS.with(|checks| checks.set(0));
        let requested = granted
            .join("nested/file")
            .to_str()
            .expect("UTF-8 fixture")
            .replace('\\', "/");
        let result =
            walk(&grant, &requested, OpenPurpose::Read, &progress).expect("ordinary retained walk");
        assert!(matches!(&result, WalkResult::File(_)));
        assert_eq!(
            ACQUIRED_REPARSE_CHECKS.with(std::cell::Cell::get),
            2,
            "both the intermediate directory and final file handles must be checked"
        );

        drop(result);
        drop(grant);
        std::fs::remove_dir_all(&fixture).expect("cleanup fixture");
    }

    #[cfg(windows)]
    #[test]
    fn windows_blocked_call_holds_handle_until_release_child() {
        use std::sync::{Arc, mpsc};
        use std::time::Duration;

        if std::env::var_os("KELD_KEL130_BLOCKED_CALL_CHILD").is_none() {
            return;
        }

        let (watchdog_tx, watchdog_rx) = mpsc::sync_channel(0);
        let watchdog = std::thread::spawn(move || {
            if watchdog_rx.recv_timeout(Duration::from_secs(10)).is_err() {
                std::process::exit(2);
            }
        });

        let fixture = race_owned_root("windows-blocked-call");
        let root = fixture.join("granted");
        std::fs::create_dir(&root).expect("granted root");
        let path = root.join("sentinel");
        std::fs::write(&path, b"unchanged").expect("seed sentinel");
        let verified = Arc::new(test_verified_manifest(&fixture, &root));
        let baseline = test_owner_handle_count();
        let broker = Arc::new(FsBroker::prepare(&verified).expect("prepare broker"));
        let prepared = test_owner_handle_count();
        let cancelled = Arc::new(AtomicBool::new(false));

        let (thread_ready_tx, thread_ready_rx) = mpsc::sync_channel(0);
        let (start_tx, start_rx) = mpsc::sync_channel(0);
        let (call_ready_tx, call_ready_rx) = mpsc::sync_channel(0);
        let (release_tx, release_rx) = mpsc::channel();
        let (result_tx, result_rx) = mpsc::sync_channel(0);
        let thread_broker = Arc::clone(&broker);
        let thread_verified = Arc::clone(&verified);
        let thread_cancelled = Arc::clone(&cancelled);
        let requested = path.to_str().expect("UTF-8 fixture").replace('\\', "/");
        let writer = std::thread::spawn(move || {
            thread_ready_tx.send(()).expect("publish thread readiness");
            start_rx.recv().expect("start blocked call");
            AFTER_WALK_TEST_HOOK.with(|slot| {
                *slot.borrow_mut() = Some(Box::new(move || {
                    call_ready_tx.send(()).expect("publish live call handle");
                    release_rx.recv().expect("release blocked call");
                }));
            });
            let result = thread_broker.write(
                &thread_verified,
                Principal::AppProcess,
                &requested,
                b"changed",
                &thread_cancelled,
            );
            result_tx.send(result).expect("publish terminal result");
        });

        thread_ready_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("writer thread ready");
        let idle_thread = test_owner_handle_count();
        start_tx.send(()).expect("start call");
        call_ready_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("real call handle reached block");
        let live_call = test_owner_handle_count();
        assert!(
            live_call > idle_thread,
            "the blocked operation must retain a real per-call handle"
        );
        cancelled.store(true, Ordering::Release);
        assert!(matches!(
            result_rx.try_recv(),
            Err(mpsc::TryRecvError::Empty)
        ));
        release_tx.send(()).expect("release call");
        let result = result_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("terminal result after release");
        writer.join().expect("join writer");
        let error = result.expect_err("released call observes cancellation before commit");
        assert_eq!(error.code(), "KELD-NATIVE-006");
        assert_eq!(std::fs::read(&path).expect("sentinel bytes"), b"unchanged");
        assert_eq!(
            test_owner_handle_count(),
            prepared,
            "per-call handle closes before the terminal result is observed"
        );

        drop(broker);
        drop(verified);
        assert_eq!(test_owner_handle_count(), baseline);
        std::fs::remove_dir_all(&fixture).expect("cleanup fixture");
        watchdog_tx.send(()).expect("disarm watchdog");
        watchdog.join().expect("join watchdog");
    }

    #[cfg(windows)]
    #[test]
    fn windows_blocked_call_lifecycle_is_subprocess_isolated() {
        let output = std::process::Command::new(std::env::current_exe().expect("test binary"))
            .args([
                "--exact",
                "fs::tests::windows_blocked_call_holds_handle_until_release_child",
                "--nocapture",
            ])
            .env("KELD_KEL130_BLOCKED_CALL_CHILD", "1")
            .output()
            .expect("run isolated blocked-call fixture");
        assert!(
            output.status.success(),
            "blocked-call fixture failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

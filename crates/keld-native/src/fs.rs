//! Host-owned scoped `fs.read` / `fs.write` (KEL-71).
//!
//! Architecture 05 §3: every native module is Rust → guard check → kipc
//! channel → typed TS. This is the first real capability wired through
//! [`keld_ipc::guard_dispatch::dispatch_privileged`] (KEL-69) instead of
//! that ticket's test-only fixture. Bun's own `node:fs` is not the Keld
//! API — the app process is not sandboxed yet (architecture 03 §4.2, v0.3),
//! so Bun can still touch the disk directly, but a Keld app that wants the
//! host-brokered, guard-checked path uses this, not `node:fs`.
//!
//! Cross-platform by construction: `std::fs::read`/`std::fs::write` are the
//! same call on macOS/Windows/Linux, so this module satisfies architecture
//! 05 §3's "all three OS implementations" without per-platform code.

use std::io::{self, ErrorKind, Read, Write};

use keld_guard::{DenyReason, PermissionsManifest, Principal};
use keld_ipc::codec::{decode, encode};
use keld_ipc::frame::{ChannelId, FrameKind};
use keld_ipc::guard_dispatch::dispatch_privileged;
use keld_ipc::link::{handshake_server, read_frame, write_frame};
use keld_ipc::{IpcError, SessionToken};
use serde::{Deserialize, Serialize};

/// Capability id for a scoped read, checked against `app.fs.read` grants.
pub const FS_READ_CAPABILITY: &str = "fs.read";
/// Capability id for a scoped write, checked against `app.fs.write` grants.
pub const FS_WRITE_CAPABILITY: &str = "fs.write";

/// kipc channel carrying [`FsRequest`]/[`FsResponse`] `Call`/`Reply` pairs.
pub const FS_CHANNEL: ChannelId = ChannelId(2);

/// Request payload for the `fs` channel.
#[derive(Debug, Serialize, Deserialize)]
pub enum FsRequest {
    /// Read the file at `path`.
    Read {
        /// Requested path; checked against `app.fs.read` scopes verbatim
        /// (v0: literal match, no `$VAR` expansion or symlink resolution).
        path: String,
    },
    /// Write `bytes` to `path`, creating or truncating it.
    Write {
        /// Requested path; checked against `app.fs.write` scopes verbatim.
        path: String,
        /// Bytes to write.
        bytes: Vec<u8>,
    },
}

/// Response payload for the `fs` channel.
#[derive(Debug, Serialize, Deserialize)]
pub enum FsResponse {
    /// The bytes read back from disk.
    Read {
        /// File contents.
        bytes: Vec<u8>,
    },
    /// The write completed.
    Write,
}

/// Errors from a host fs operation. `KELD-NATIVE-*` codes for I/O; a deny
/// carries `keld-guard`'s own `KELD-GUARD*` code and fix text unchanged.
#[derive(Debug)]
pub enum FsError {
    /// `keld-guard::evaluate` denied the operation; `handler` never ran.
    Denied(DenyReason),
    /// The OS call itself failed (path allowed, but e.g. not found).
    Io(io::Error),
}

impl std::fmt::Display for FsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Denied(reason) => write!(f, "{reason}"),
            Self::Io(e) => write!(
                f,
                "KELD-NATIVE-001: filesystem I/O error — {e}. \
                 Check the path exists and is accessible."
            ),
        }
    }
}

impl std::error::Error for FsError {}

/// Reads `path` after a guard check. `path` is also the resource `evaluate`
/// checks — a `..` segment or an out-of-scope path is
/// [`FsError::Denied`] and the file is never opened.
///
/// # Errors
///
/// Returns [`FsError::Denied`] on a guard deny, [`FsError::Io`] if the
/// (allowed) read itself fails.
pub fn fs_read(
    manifest: &PermissionsManifest,
    principal: Principal,
    path: &str,
) -> Result<Vec<u8>, FsError> {
    match dispatch_privileged(manifest, principal, FS_READ_CAPABILITY, path, || {
        std::fs::read(path)
    }) {
        Ok(Ok(bytes)) => Ok(bytes),
        Ok(Err(e)) => Err(FsError::Io(e)),
        Err(deny) => Err(FsError::Denied(deny)),
    }
}

/// Writes `bytes` to `path` after a guard check, creating or truncating the
/// file. On [`FsError::Denied`] the file is never opened — not created, not
/// overwritten.
///
/// # Errors
///
/// Returns [`FsError::Denied`] on a guard deny, [`FsError::Io`] if the
/// (allowed) write itself fails.
pub fn fs_write(
    manifest: &PermissionsManifest,
    principal: Principal,
    path: &str,
    bytes: &[u8],
) -> Result<(), FsError> {
    match dispatch_privileged(manifest, principal, FS_WRITE_CAPABILITY, path, || {
        std::fs::write(path, bytes)
    }) {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(FsError::Io(e)),
        Err(deny) => Err(FsError::Denied(deny)),
    }
}

/// Serves `fs.read`/`fs.write` `Call`s on [`FS_CHANNEL`] for one kipc
/// session, after the v2 `HELLO` handshake.
///
/// Every `Call` goes through [`dispatch_privileged`] before touching the
/// filesystem (KEL-69) — a deny is returned as an `Err` frame carrying the
/// typed `KELD-GUARD*` text and never runs the OS call.
///
/// # Errors
///
/// Returns [`IpcError`] on handshake failure, I/O, or an unexpected frame.
pub fn serve_fs_session<S: Read + Write>(
    stream: &mut S,
    token: &SessionToken,
    manifest: &PermissionsManifest,
    principal: Principal,
) -> Result<(), IpcError> {
    handshake_server(stream, token)?;
    loop {
        let (header, payload) = match read_frame(stream) {
            Ok(frame) => frame,
            Err(IpcError::Io(e)) if e.kind() == ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e),
        };
        match header.kind {
            FrameKind::Call if header.channel == FS_CHANNEL => {
                let req: FsRequest = decode(&payload)?;
                let result = match req {
                    FsRequest::Read { path } => {
                        fs_read(manifest, principal, &path).map(|bytes| FsResponse::Read { bytes })
                    }
                    FsRequest::Write { path, bytes } => {
                        fs_write(manifest, principal, &path, &bytes).map(|()| FsResponse::Write)
                    }
                };
                match result {
                    Ok(resp) => {
                        let bytes = encode(&resp)?;
                        write_frame(stream, FrameKind::Reply, 0, FS_CHANNEL, header.corr, &bytes)?;
                    }
                    Err(err) => {
                        let bytes = encode(&err.to_string())?;
                        write_frame(stream, FrameKind::Err, 0, FS_CHANNEL, header.corr, &bytes)?;
                    }
                }
            }
            _ => {
                return Err(IpcError::Protocol {
                    detail: "unexpected frame kind in fs session",
                });
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use keld_guard::parse_manifest;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "keld-kel71-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("mkdir");
        dir
    }

    fn scope_path(p: &std::path::Path) -> String {
        p.display().to_string().replace('\\', "/")
    }

    fn manifest_for(dir: &std::path::Path) -> PermissionsManifest {
        parse_manifest(&format!(
            r#"{{"app":{{"fs":{{"read":["{scope}/**"],"write":["{scope}/**"]}}}}}}"#,
            scope = scope_path(dir)
        ))
        .expect("manifest")
    }

    #[test]
    fn write_then_read_back_identical_bytes() {
        let dir = temp_dir("rw");
        let file = scope_path(&dir.join("notes.txt"));
        let manifest = manifest_for(&dir);

        fs_write(&manifest, Principal::AppProcess, &file, b"hello kel-71").expect("write");
        let bytes = fs_read(&manifest, Principal::AppProcess, &file).expect("read");
        assert_eq!(bytes, b"hello kel-71");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_out_of_scope_is_denied_and_creates_no_file() {
        let dir = temp_dir("deny-write");
        let outside = temp_dir("deny-write-outside");
        let file = scope_path(&outside.join("secret.txt"));
        // Grants only `dir`, not `outside`.
        let manifest = manifest_for(&dir);

        let err = fs_write(&manifest, Principal::AppProcess, &file, b"nope")
            .expect_err("out-of-scope write must be denied");
        assert!(matches!(
            err,
            FsError::Denied(DenyReason::OutOfScope { .. })
        ));
        assert!(err.to_string().contains("KELD-GUARD002"));
        assert!(
            !std::path::Path::new(&file).exists(),
            "denied write must not create the file"
        );

        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&outside);
    }

    #[test]
    fn read_not_granted_is_denied() {
        let dir = temp_dir("deny-read");
        let file = scope_path(&dir.join("notes.txt"));
        std::fs::write(&file, b"pre-existing").expect("seed file directly (bypassing Keld)");
        let manifest = parse_manifest("{}").expect("empty manifest");

        let err = fs_read(&manifest, Principal::AppProcess, &file)
            .expect_err("empty manifest must deny fs.read");
        assert!(matches!(
            err,
            FsError::Denied(DenyReason::NotGranted { .. })
        ));
        assert!(err.to_string().contains("KELD-GUARD001"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn dotdot_segment_is_denied_even_inside_a_granted_scope() {
        let dir = temp_dir("dotdot");
        let manifest = manifest_for(&dir);
        // `dir/../<name>` always resolves to the shared temp root regardless
        // of `dir`'s own unique suffix — give the escaping filename itself a
        // unique suffix, or a crashed prior run (or a parallel test) could
        // leave/contend for the same real path and produce a false failure.
        let escaping_name = format!(
            "keld-kel71-dotdot-escape-{}-{}.txt",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        );
        let escaping = format!("{}/../{escaping_name}", scope_path(&dir));
        let _ = std::fs::remove_file(&escaping);

        let err = fs_write(&manifest, Principal::AppProcess, &escaping, b"nope")
            .expect_err("`..` must be denied even under a granted prefix");
        assert!(matches!(
            err,
            FsError::Denied(DenyReason::OutOfScope { .. })
        ));
        assert!(
            !std::path::Path::new(&escaping).exists(),
            "a `..` write must never touch disk"
        );

        let _ = std::fs::remove_file(&escaping);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn webview_principal_cannot_read_or_write_even_in_scope() {
        let dir = temp_dir("webview");
        let file = scope_path(&dir.join("notes.txt"));
        let manifest = manifest_for(&dir);
        let webview = Principal::Webview {
            id: 1,
            generation: 1,
        };

        let err = fs_write(&manifest, webview, &file, b"nope")
            .expect_err("webview must not inherit /app grants");
        assert!(matches!(
            err,
            FsError::Denied(DenyReason::NotAppProcess { .. })
        ));
        assert!(!std::path::Path::new(&file).exists());

        let _ = std::fs::remove_dir_all(&dir);
    }
}

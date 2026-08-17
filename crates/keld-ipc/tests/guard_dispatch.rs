//! Wire-level proof that a privileged kipc `Call` is passed through
//! `keld_guard::evaluate` before its handler runs (KEL-69).
//!
//! `MARKER_CHANNEL`/`test.marker` here is deliberately NOT a production
//! capability — it exists only in this test file, to prove
//! `keld_ipc::guard_dispatch::dispatch_privileged` wired into a real
//! session-serving loop over a real socket. The real capability
//! (`fs.read`/`fs.write`) is KEL-71's, and MUST call `dispatch_privileged`
//! the same way this test does, not reinvent the check.

#![allow(clippy::expect_used)] // extra test crate: expect is the assertion oracle

use std::fs;
use std::io::{ErrorKind, Read, Write};
use std::thread;

use keld_guard::{PermissionsManifest, Principal, parse_manifest};
use keld_ipc::codec::{decode, encode};
use keld_ipc::frame::{ChannelId, CorrelationId, FrameKind};
use keld_ipc::guard_dispatch::dispatch_privileged;
use keld_ipc::link::{handshake_client, handshake_server, read_frame, write_frame};
use keld_ipc::{IpcError, SessionToken};
use serde::{Deserialize, Serialize};

#[cfg(unix)]
type Stream = std::os::unix::net::UnixStream;
#[cfg(windows)]
type Stream = std::net::TcpStream;

#[cfg(unix)]
fn connected_pair() -> (Stream, Stream) {
    std::os::unix::net::UnixStream::pair().expect("unix pair")
}

#[cfg(windows)]
fn connected_pair() -> (Stream, Stream) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().expect("addr");
    let accept = thread::spawn(move || listener.accept().expect("accept").0);
    let client = std::net::TcpStream::connect(addr).expect("connect");
    let server = accept.join().expect("accept thread");
    (client, server)
}

const TEST_TOKEN_BYTES: [u8; 32] = [0x5A; 32];

fn test_token() -> SessionToken {
    SessionToken::from_bytes(TEST_TOKEN_BYTES)
}

const MARKER_CHANNEL: ChannelId = ChannelId(50);

#[derive(Debug, Serialize, Deserialize)]
struct MarkerRequest {
    path: String,
}

/// The KEL-69 proof server loop: one `HELLO`, then `Call`s on
/// `MARKER_CHANNEL` go through [`dispatch_privileged`] before the handler's
/// real filesystem write.
fn serve_marker_session<S: Read + Write>(
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
            FrameKind::Call if header.channel == MARKER_CHANNEL => {
                let req: MarkerRequest = decode(&payload)?;
                let outcome =
                    dispatch_privileged(manifest, principal, "test.marker", &req.path, || {
                        fs::write(&req.path, b"kel-69-marker")
                    });
                match outcome {
                    Ok(Ok(())) => {
                        write_frame(
                            stream,
                            FrameKind::Reply,
                            0,
                            MARKER_CHANNEL,
                            header.corr,
                            &[],
                        )?;
                    }
                    Ok(Err(io_err)) => {
                        let bytes = encode(&io_err.to_string())?;
                        write_frame(
                            stream,
                            FrameKind::Err,
                            0,
                            MARKER_CHANNEL,
                            header.corr,
                            &bytes,
                        )?;
                    }
                    Err(deny) => {
                        let bytes = encode(&deny.to_string())?;
                        write_frame(
                            stream,
                            FrameKind::Err,
                            0,
                            MARKER_CHANNEL,
                            header.corr,
                            &bytes,
                        )?;
                    }
                }
            }
            _ => {
                return Err(IpcError::Protocol {
                    detail: "unexpected frame kind in marker session",
                });
            }
        }
    }
    Ok(())
}

fn call_marker(stream: &mut Stream, path: &str) -> Result<Result<(), String>, IpcError> {
    handshake_client(stream, &test_token())?;
    let payload = encode(&MarkerRequest {
        path: path.to_owned(),
    })?;
    write_frame(
        stream,
        FrameKind::Call,
        0,
        MARKER_CHANNEL,
        CorrelationId(1),
        &payload,
    )?;
    let (header, reply) = read_frame(stream)?;
    match header.kind {
        FrameKind::Reply => Ok(Ok(())),
        FrameKind::Err => Ok(Err(decode::<String>(&reply)?)),
        _ => Err(IpcError::Protocol {
            detail: "unexpected reply frame kind",
        }),
    }
}

/// Manifest scopes are always forward-slash (every example in the repo's
/// manifests uses `/`, e.g. `$APPDATA/**`) — normalize a native `Path`
/// (backslash on Windows) before using it as a grant or a request `path`,
/// so the literal-string scope matcher (`keld-guard` v0: no path
/// normalization) sees the same separator on both sides. Windows `fs`
/// APIs accept `/`-separated paths interchangeably, so this string is also
/// safe to pass straight to `fs::write`.
fn scope_path(p: &std::path::Path) -> String {
    p.display().to_string().replace('\\', "/")
}

fn tmp_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "keld-kel69-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("mkdir");
    dir
}

#[test]
fn allow_manifest_runs_the_handler_real_file_is_written() {
    let dir = tmp_dir("allow");
    let marker = dir.join("marker.txt");
    let manifest = parse_manifest(&format!(
        r#"{{"app":{{"test":{{"marker":["{}/**"]}}}}}}"#,
        scope_path(&dir)
    ))
    .expect("manifest");

    let (mut client, mut server) = connected_pair();
    let marker_path = scope_path(&marker);
    let handle = thread::spawn(move || {
        serve_marker_session(&mut server, &test_token(), &manifest, Principal::AppProcess)
    });

    let result = call_marker(&mut client, &marker_path).expect("call");
    drop(client);
    handle.join().expect("server thread").expect("serve");

    assert!(result.is_ok(), "{result:?}");
    assert!(
        marker.exists(),
        "handler's real side-effect (file write) must have happened on Allow"
    );
    assert_eq!(fs::read(&marker).expect("read marker"), b"kel-69-marker");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn deny_manifest_handler_never_runs_no_file_is_written() {
    let dir = tmp_dir("deny");
    let marker = dir.join("marker.txt");
    // Empty manifest: no `test.marker` grant at all.
    let manifest = parse_manifest("{}").expect("empty manifest");

    let (mut client, mut server) = connected_pair();
    let marker_path = scope_path(&marker);
    let handle = thread::spawn(move || {
        serve_marker_session(&mut server, &test_token(), &manifest, Principal::AppProcess)
    });

    let result = call_marker(&mut client, &marker_path).expect("call");
    drop(client);
    handle.join().expect("server thread").expect("serve");

    let err = result.expect_err("empty manifest must deny");
    assert!(err.contains("KELD-GUARD001"), "{err}");
    assert!(
        !marker.exists(),
        "handler's OS side-effect (file write) must NOT happen on Deny — this is the negative \
         control: deleting the dispatch_privileged call in serve_marker_session would make this \
         assertion fail (the file would exist even though the manifest denies it)"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn webview_principal_is_denied_even_with_an_in_scope_grant() {
    let dir = tmp_dir("webview");
    let marker = dir.join("marker.txt");
    let manifest = parse_manifest(&format!(
        r#"{{"app":{{"test":{{"marker":["{}/**"]}}}}}}"#,
        scope_path(&dir)
    ))
    .expect("manifest");

    let (mut client, mut server) = connected_pair();
    let marker_path = scope_path(&marker);
    let webview = Principal::Webview {
        id: 1,
        generation: 1,
    };
    let handle =
        thread::spawn(move || serve_marker_session(&mut server, &test_token(), &manifest, webview));

    let result = call_marker(&mut client, &marker_path).expect("call");
    drop(client);
    handle.join().expect("server thread").expect("serve");

    let err = result.expect_err("webview principal must be denied");
    assert!(err.contains("KELD-GUARD006"), "{err}");
    assert!(
        !marker.exists(),
        "a webview must not inherit the /app grant even though the path is in scope for AppProcess"
    );

    let _ = fs::remove_dir_all(&dir);
}

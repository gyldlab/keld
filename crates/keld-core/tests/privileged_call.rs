//! Privileged kipc Call on the wire: evaluate before the handler (KEL-69).
//!
//! These tests send real frames over a connected pair. Matcher-only tests in
//! `keld-guard` do not close this ticket.

#![allow(clippy::expect_used, clippy::needless_pass_by_value)] // extra test crate: expect is the assertion oracle; spawn owns PathBuf-sized values for the worker

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;

use keld_core::{FS_READ_CHANNEL, PrivilegedSession, serve_privileged_session};
use keld_guard::{PermissionsManifest, Principal, parse_manifest};
use keld_ipc::codec::{decode, encode};
use keld_ipc::frame::{CorrelationId, FrameKind};
use keld_ipc::link::{handshake_client, read_frame, write_frame};
use keld_ipc::{
    AppLinkDeadlines, CallError, ECHO_CHANNEL, EchoRequest, EchoResponse, IpcError, SessionToken,
};

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

const TEST_TOKEN_BYTES: [u8; 32] = [0x69; 32];

fn test_token() -> SessionToken {
    SessionToken::from_bytes(TEST_TOKEN_BYTES)
}

fn allow_fs_read() -> PermissionsManifest {
    parse_manifest(r#"{"app":{"fs":{"read":["$APPDATA/**"]}}}"#).expect("manifest")
}

fn empty_manifest() -> PermissionsManifest {
    parse_manifest("{}").expect("empty")
}

static MARKER_SEQ: AtomicU64 = AtomicU64::new(0);

fn marker_path() -> PathBuf {
    std::env::temp_dir().join(format!(
        "keld-kel69-{}-{}",
        std::process::id(),
        MARKER_SEQ.fetch_add(1, Ordering::SeqCst)
    ))
}

fn spawn_guarded(
    mut server: Stream,
    manifest: PermissionsManifest,
    principal: Principal,
    marker: PathBuf,
) -> thread::JoinHandle<Result<(), IpcError>> {
    thread::spawn(move || {
        let mut session = PrivilegedSession {
            manifest,
            principal,
            handler: move |path: &str| {
                fs::write(&marker, path).expect("handler side-effect");
                encode(&path.to_owned()).expect("reply")
            },
        };
        serve_privileged_session(&mut server, &test_token(), &mut session)
    })
}

fn handshake_client_stream(client: &mut Stream) {
    client
        .set_app_link_deadlines(Some(keld_ipc::APP_LINK_IO_DEADLINE))
        .expect("deadline");
    handshake_client(client, &test_token()).expect("handshake");
}

fn call_fs_read(client: &mut Stream, path: &str, corr: u32) -> (FrameKind, Vec<u8>) {
    let payload = encode(&path.to_owned()).expect("encode path");
    write_frame(
        client,
        FrameKind::Call,
        0,
        FS_READ_CHANNEL,
        CorrelationId(corr),
        &payload,
    )
    .expect("write CALL");
    let (header, payload) = read_frame(client).expect("read reply");
    assert_eq!(header.corr, CorrelationId(corr));
    assert_eq!(header.channel, FS_READ_CHANNEL);
    (header.kind, payload)
}

fn call_echo(client: &mut Stream, message: &str, corr: u32) -> EchoResponse {
    let payload = encode(&EchoRequest {
        message: message.to_owned(),
        count: 1,
    })
    .expect("encode echo");
    write_frame(
        client,
        FrameKind::Call,
        0,
        ECHO_CHANNEL,
        CorrelationId(corr),
        &payload,
    )
    .expect("write echo CALL");
    let (header, payload) = read_frame(client).expect("read echo");
    assert_eq!(header.kind, FrameKind::Reply);
    assert_eq!(header.corr, CorrelationId(corr));
    assert_eq!(header.channel, ECHO_CHANNEL);
    decode(&payload).expect("decode echo")
}

#[test]
fn deny_manifest_returns_guard001_and_does_not_run_handler() {
    let marker = marker_path();
    let _ = fs::remove_file(&marker);
    let (mut client, server) = connected_pair();
    let handle = spawn_guarded(
        server,
        empty_manifest(),
        Principal::AppProcess,
        marker.clone(),
    );
    handshake_client_stream(&mut client);
    let (kind, payload) = call_fs_read(&mut client, "$APPDATA/notes.txt", 1);
    drop(client);
    handle.join().expect("server thread").expect("serve");

    assert_eq!(
        kind,
        FrameKind::Err,
        "deny must be an Err frame, not Reply or a dropped session"
    );
    let err: CallError = decode(&payload).expect("CallError");
    assert_eq!(err.code, "KELD-GUARD001");
    assert!(err.message.contains("KELD-GUARD001"), "{}", err.message);
    assert!(
        err.message.contains("keld.permissions.jsonc"),
        "deny text is API and must state the fix: {}",
        err.message
    );
    assert!(
        !marker.exists(),
        "handler side-effect must not occur on deny: {marker:?}"
    );
}

#[test]
fn out_of_scope_returns_guard002_and_does_not_run_handler() {
    let marker = marker_path();
    let _ = fs::remove_file(&marker);
    let (mut client, server) = connected_pair();
    let handle = spawn_guarded(
        server,
        allow_fs_read(),
        Principal::AppProcess,
        marker.clone(),
    );
    handshake_client_stream(&mut client);
    let (kind, payload) = call_fs_read(&mut client, "$DOCUMENTS/notes.txt", 1);
    drop(client);
    handle.join().expect("server thread").expect("serve");

    assert_eq!(kind, FrameKind::Err);
    let err: CallError = decode(&payload).expect("CallError");
    assert_eq!(err.code, "KELD-GUARD002");
    assert!(err.message.contains("KELD-GUARD002"), "{}", err.message);
    assert!(
        err.message.contains("$DOCUMENTS/notes.txt"),
        "{}",
        err.message
    );
    assert!(!marker.exists(), "handler must not run on KELD-GUARD002");
}

#[test]
fn allow_manifest_runs_handler_and_writes_side_effect() {
    let marker = marker_path();
    let _ = fs::remove_file(&marker);
    let (mut client, server) = connected_pair();
    let handle = spawn_guarded(
        server,
        allow_fs_read(),
        Principal::AppProcess,
        marker.clone(),
    );
    handshake_client_stream(&mut client);
    let (kind, payload) = call_fs_read(&mut client, "$APPDATA/notes.txt", 1);
    drop(client);
    handle.join().expect("server thread").expect("serve");

    assert_eq!(
        kind,
        FrameKind::Reply,
        "allow must be Reply — Decision::Allow from a unit stub is not this test"
    );
    let echoed: String = decode(&payload).expect("path reply");
    assert_eq!(echoed, "$APPDATA/notes.txt");
    let wrote = fs::read_to_string(&marker).expect("handler side-effect");
    assert_eq!(wrote, "$APPDATA/notes.txt");
    let _ = fs::remove_file(&marker);
}

#[test]
fn webview_principal_is_guard006_even_when_path_is_in_scope() {
    let marker = marker_path();
    let _ = fs::remove_file(&marker);
    let (mut client, server) = connected_pair();
    let handle = spawn_guarded(
        server,
        allow_fs_read(),
        Principal::Webview {
            id: 9,
            generation: 2,
        },
        marker.clone(),
    );
    handshake_client_stream(&mut client);
    let (kind, payload) = call_fs_read(&mut client, "$APPDATA/notes.txt", 1);
    drop(client);
    handle.join().expect("server thread").expect("serve");

    assert_eq!(kind, FrameKind::Err);
    let err: CallError = decode(&payload).expect("CallError");
    assert_eq!(err.code, "KELD-GUARD006");
    assert!(err.message.contains("webview"), "{}", err.message);
    assert!(
        !err.message.contains("/app/fs/read"),
        "must not tell the caller to add /app scopes: {}",
        err.message
    );
    assert!(
        !marker.exists(),
        "handler must not run for a webview principal"
    );
}

#[test]
fn echo_stays_ungated_on_empty_manifest() {
    let marker = marker_path();
    let _ = fs::remove_file(&marker);
    let (mut client, server) = connected_pair();
    let handle = spawn_guarded(
        server,
        empty_manifest(),
        Principal::AppProcess,
        marker.clone(),
    );
    handshake_client_stream(&mut client);
    let echo = call_echo(&mut client, "ungated", 1);
    drop(client);
    handle.join().expect("server thread").expect("serve");

    assert_eq!(echo.message, "ungated");
    assert_eq!(echo.count, 1);
    assert!(
        !marker.exists(),
        "echo must not invoke the privileged handler"
    );
}

#[test]
fn deny_does_not_kill_session_echo_still_works() {
    let marker = marker_path();
    let _ = fs::remove_file(&marker);
    let (mut client, server) = connected_pair();
    let handle = spawn_guarded(
        server,
        empty_manifest(),
        Principal::AppProcess,
        marker.clone(),
    );
    handshake_client_stream(&mut client);
    let (kind, payload) = call_fs_read(&mut client, "$APPDATA/notes.txt", 1);
    assert_eq!(kind, FrameKind::Err);
    let err: CallError = decode(&payload).expect("CallError");
    assert_eq!(err.code, "KELD-GUARD001");
    let echo = call_echo(&mut client, "after-deny", 2);
    drop(client);
    handle.join().expect("server thread").expect("serve");

    assert_eq!(echo.message, "after-deny");
    assert!(!marker.exists());
}

#[test]
fn malformed_privileged_payload_does_not_run_handler() {
    let marker = marker_path();
    let _ = fs::remove_file(&marker);
    let (mut client, server) = connected_pair();
    let handle = spawn_guarded(
        server,
        allow_fs_read(),
        Principal::AppProcess,
        marker.clone(),
    );
    handshake_client_stream(&mut client);
    write_frame(
        &mut client,
        FrameKind::Call,
        0,
        FS_READ_CHANNEL,
        CorrelationId(1),
        &[0xff, 0xff],
    )
    .expect("write garbage CALL");
    let serve_err = {
        let _read = read_frame(&mut client);
        drop(client);
        handle.join().expect("server thread")
    };
    let err = serve_err.expect_err("garbage payload must fail closed");
    assert!(
        err.to_string().contains("KELD-IPC-003"),
        "expected codec error, got {err}"
    );
    assert!(
        !marker.exists(),
        "handler must not run when the path cannot be decoded"
    );
}

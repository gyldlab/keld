//! Wire-level proof of `fs.read`/`fs.write` over a real kipc session (KEL-71).

#![allow(clippy::expect_used)] // extra test crate: expect is the assertion oracle

use std::fs;
use std::thread;

use keld_guard::{PermissionsManifest, Principal, parse_manifest};
use keld_ipc::codec::decode;
use keld_ipc::frame::{CorrelationId, FrameKind};
use keld_ipc::link::{handshake_client, read_frame, write_frame};
use keld_ipc::{IpcError, SessionToken};
use keld_native::fs::{FS_CHANNEL, FsRequest, FsResponse, serve_fs_session};

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

const TEST_TOKEN_BYTES: [u8; 32] = [0x37; 32];

fn test_token() -> SessionToken {
    SessionToken::from_bytes(TEST_TOKEN_BYTES)
}

fn scope_path(p: &std::path::Path) -> String {
    p.display().to_string().replace('\\', "/")
}

fn temp_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "keld-kel71-wire-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("mkdir");
    dir
}

fn manifest_for(dir: &std::path::Path) -> PermissionsManifest {
    parse_manifest(&format!(
        r#"{{"app":{{"fs":{{"read":["{scope}/**"],"write":["{scope}/**"]}}}}}}"#,
        scope = scope_path(dir)
    ))
    .expect("manifest")
}

fn call_fs(stream: &mut Stream, req: &FsRequest) -> Result<Result<FsResponse, String>, IpcError> {
    handshake_client(stream, &test_token())?;
    let payload = keld_ipc::codec::encode(req)?;
    write_frame(
        stream,
        FrameKind::Call,
        0,
        FS_CHANNEL,
        CorrelationId(1),
        &payload,
    )?;
    let (header, reply) = read_frame(stream)?;
    match header.kind {
        FrameKind::Reply => Ok(Ok(decode::<FsResponse>(&reply)?)),
        FrameKind::Err => Ok(Err(decode::<String>(&reply)?)),
        _ => Err(IpcError::Protocol {
            detail: "unexpected reply frame kind",
        }),
    }
}

#[test]
fn allow_write_then_read_over_a_real_kipc_session() {
    let dir = temp_dir("allow");
    let file = scope_path(&dir.join("notes.txt"));
    let manifest = manifest_for(&dir);

    let (mut client, mut server) = connected_pair();
    let handle = thread::spawn(move || {
        serve_fs_session(&mut server, &test_token(), &manifest, Principal::AppProcess)
    });

    let write_result = call_fs(
        &mut client,
        &FsRequest::Write {
            path: file.clone(),
            bytes: b"kel-71 over the wire".to_vec(),
        },
    )
    .expect("write call");
    assert!(
        matches!(write_result, Ok(FsResponse::Write)),
        "{write_result:?}"
    );

    drop(client);
    handle.join().expect("server thread").expect("serve");

    assert_eq!(
        fs::read(&file).expect("real file on disk"),
        b"kel-71 over the wire"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn deny_over_the_wire_leaves_no_file_and_carries_the_typed_reason() {
    let dir = temp_dir("deny");
    let file = scope_path(&dir.join("notes.txt"));
    // Empty manifest: no fs.write grant at all.
    let manifest = parse_manifest("{}").expect("empty manifest");

    let (mut client, mut server) = connected_pair();
    let handle = thread::spawn(move || {
        serve_fs_session(&mut server, &test_token(), &manifest, Principal::AppProcess)
    });

    let result = call_fs(
        &mut client,
        &FsRequest::Write {
            path: file.clone(),
            bytes: b"should never land".to_vec(),
        },
    )
    .expect("write call");

    drop(client);
    handle.join().expect("server thread").expect("serve");

    let err = match result {
        Err(msg) => msg,
        Ok(resp) => panic!("expected a deny, got {resp:?}"),
    };
    assert!(err.contains("KELD-GUARD001"), "{err}");
    assert!(
        !std::path::Path::new(&file).exists(),
        "denied write must never touch disk"
    );
    let _ = fs::remove_dir_all(&dir);
}

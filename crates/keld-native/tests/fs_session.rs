//! Wire-level proof of `fs.read`/`fs.write` over a real kipc session (KEL-71).

#![allow(clippy::expect_used)] // extra test crate: expect is the assertion oracle

use std::fs;
use std::sync::atomic::AtomicBool;
use std::thread;

use keld_guard::Principal;
use keld_guard::verified_manifest::{VerifiedManifest, load_verified_manifest};
use keld_ipc::codec::decode;
use keld_ipc::frame::{CorrelationId, FrameKind};
use keld_ipc::link::{handshake_client, read_frame, write_frame};
use keld_ipc::{CallError, IpcError, SessionToken};
use keld_native::fs::{FS_CHANNEL, FsBroker, FsRequest, FsResponse, serve_fs_session};
use sha2::{Digest, Sha256};

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

fn verified_manifest(dir: &std::path::Path, text: &str) -> VerifiedManifest {
    let path = dir.join("keld.permissions.jsonc");
    fs::write(&path, text).expect("write manifest");
    let digest: [u8; 32] = Sha256::digest(text.as_bytes()).into();
    load_verified_manifest(fs::File::open(&path).expect("open manifest"), path, digest)
        .expect("verified manifest")
}

fn manifest_for(dir: &std::path::Path) -> VerifiedManifest {
    verified_manifest(
        dir,
        &format!(
            r#"{{"app":{{"fs":{{"read":["{scope}/**"],"write":["{scope}/**"]}}}}}}"#,
            scope = scope_path(dir)
        ),
    )
}

fn call_fs(
    stream: &mut Stream,
    req: &FsRequest,
) -> Result<Result<FsResponse, CallError>, IpcError> {
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
        FrameKind::Err => Ok(Err(decode::<CallError>(&reply)?)),
        _ => Err(IpcError::Protocol {
            detail: "unexpected reply frame kind",
        }),
    }
}

/// kel133 AC2 on the privileged receiver: a hostile *authenticated* CALL
/// (correlation 0, `FLAG_RAW`, an unknown flag bit, or the echo channel) with
/// an otherwise in-scope write request closes the session with
/// `KELD-IPC-005` and touches no file — the absent file is the OS-visible
/// zero-effect oracle, independent of the guard and of the reply path.
#[test]
fn hostile_authenticated_calls_close_with_005_and_write_nothing() {
    let cases: [(&str, u16, keld_ipc::ChannelId, u32); 4] = [
        ("corr zero", 0, FS_CHANNEL, 0),
        ("flag raw", keld_ipc::frame::FLAG_RAW, FS_CHANNEL, 5),
        ("unknown flag", 1 << 2, FS_CHANNEL, 5),
        ("echo channel", 0, keld_ipc::ECHO_CHANNEL, 5),
    ];
    for (case, flags, channel, corr) in cases {
        let dir = temp_dir(&format!("hostile-{}", case.replace(' ', "-")));
        let file = scope_path(&dir.join("must-not-exist.txt"));
        let manifest = manifest_for(&dir);
        let (mut client, mut server) = connected_pair();
        let handle = thread::spawn(move || {
            let broker = FsBroker::prepare(&manifest).expect("prepare broker");
            let cancelled = AtomicBool::new(false);
            serve_fs_session(
                &mut server,
                &test_token(),
                &broker,
                &manifest,
                Principal::AppProcess,
                &cancelled,
            )
        });

        handshake_client(&mut client, &test_token()).expect("authenticate");
        let payload = keld_ipc::codec::encode(&FsRequest::Write {
            path: file.clone(),
            bytes: b"hostile".to_vec(),
        })
        .expect("encode");
        let write = write_frame(
            &mut client,
            FrameKind::Call,
            flags,
            channel,
            CorrelationId(corr),
            &payload,
        );
        if let Err(error) = write {
            assert!(
                matches!(
                    &error,
                    IpcError::Io(source)
                        if matches!(
                            source.kind(),
                            std::io::ErrorKind::BrokenPipe
                                | std::io::ErrorKind::ConnectionReset
                        )
                ),
                "{case}: hostile header write failed unexpectedly: {error}"
            );
        }

        let err = handle
            .join()
            .expect("server thread")
            .expect_err("hostile CALL must tear the fs session down");
        assert!(
            err.to_string().contains("KELD-IPC-005"),
            "{case}: expected 005, got {err}"
        );
        assert!(
            !std::path::Path::new(&file).exists(),
            "{case}: hostile CALL must never reach the broker"
        );
        let _ = fs::remove_dir_all(&dir);
    }
}

#[test]
fn allow_write_then_read_over_a_real_kipc_session() {
    let dir = temp_dir("allow");
    let file = scope_path(&dir.join("notes.txt"));
    let manifest = manifest_for(&dir);

    let (mut client, mut server) = connected_pair();
    let handle = thread::spawn(move || {
        let broker = FsBroker::prepare(&manifest).expect("prepare broker");
        let cancelled = AtomicBool::new(false);
        serve_fs_session(
            &mut server,
            &test_token(),
            &broker,
            &manifest,
            Principal::AppProcess,
            &cancelled,
        )
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

/// The only `From<&FsError>` arm with no end-to-end coverage: a granted path
/// whose file does not exist. The OS failure must reach the peer as the
/// broker's own registered code in the `code` FIELD — not as the guard's code,
/// and not as text to be parsed.
#[test]
fn allowed_read_of_a_missing_file_carries_the_broker_code_on_the_wire() {
    let dir = temp_dir("io-failure");
    let missing = scope_path(&dir.join("does-not-exist.txt"));
    // Granted scope: the guard allows, so the failure can only come from the OS.
    let manifest = manifest_for(&dir);

    let (mut client, mut server) = connected_pair();
    let handle = thread::spawn(move || {
        let broker = FsBroker::prepare(&manifest).expect("prepare broker");
        let cancelled = AtomicBool::new(false);
        serve_fs_session(
            &mut server,
            &test_token(),
            &broker,
            &manifest,
            Principal::AppProcess,
            &cancelled,
        )
    });

    let result = call_fs(&mut client, &FsRequest::Read { path: missing }).expect("call");
    drop(client);
    handle.join().expect("server thread").expect("serve");

    let err = result.expect_err("reading a missing file must fail");
    assert_eq!(err.code, "KELD-NATIVE-001", "{err:?}");
    assert_ne!(
        err.code, "KELD-GUARD001",
        "an OS failure must not be reported as a policy denial"
    );
    assert!(
        err.message.starts_with("KELD-NATIVE-001"),
        "message must lead with the code: {}",
        err.message
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn deny_over_the_wire_leaves_no_file_and_carries_the_typed_reason() {
    let dir = temp_dir("deny");
    let file = scope_path(&dir.join("notes.txt"));
    // Empty manifest: no fs.write grant at all.
    let manifest = verified_manifest(&dir, "{}");

    let (mut client, mut server) = connected_pair();
    let handle = thread::spawn(move || {
        let broker = FsBroker::prepare(&manifest).expect("prepare empty broker");
        let cancelled = AtomicBool::new(false);
        serve_fs_session(
            &mut server,
            &test_token(),
            &broker,
            &manifest,
            Principal::AppProcess,
            &cancelled,
        )
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
    // The peer reads the code as a field — no string parsing (KEL-102 wire contract).
    assert_eq!(err.code, "KELD-GUARD001", "{err:?}");
    assert!(
        err.message.contains("keld.permissions.jsonc"),
        "the actionable fix must survive onto the wire: {}",
        err.message
    );
    assert!(
        !std::path::Path::new(&file).exists(),
        "denied write must never touch disk"
    );
    let _ = fs::remove_dir_all(&dir);
}

fn vector_hex(value: &str) -> Vec<u8> {
    if value == "-" {
        return Vec::new();
    }
    assert_eq!(value.len() % 2, 0, "even hex width");
    (0..value.len())
        .step_by(2)
        .map(|offset| u8::from_str_radix(&value[offset..offset + 2], 16).expect("fixture hex"))
        .collect()
}

fn assert_native_fs_semantic_vector(
    name: &str,
    direction: &str,
    variant: &str,
    path: &str,
    content: &[u8],
    wire: &[u8],
) -> Result<(), &'static str> {
    match (direction, variant) {
        ("request", "Read") => {
            let value = FsRequest::Read {
                path: path.to_owned(),
            };
            assert_eq!(
                keld_ipc::codec::encode(&value).expect("encode Read"),
                wire,
                "{name}"
            );
            let FsRequest::Read { path: decoded } = decode(wire).expect("decode Read") else {
                return Err("request variant changed");
            };
            assert_eq!(decoded, path, "{name}");
        }
        ("request", "Write") => {
            let value = FsRequest::Write {
                path: path.to_owned(),
                bytes: content.to_vec(),
            };
            assert_eq!(
                keld_ipc::codec::encode(&value).expect("encode Write"),
                wire,
                "{name}"
            );
            let FsRequest::Write {
                path: decoded,
                bytes,
            } = decode(wire).expect("decode Write")
            else {
                return Err("request variant changed");
            };
            assert_eq!(decoded, path, "{name}");
            assert_eq!(bytes, content, "{name}");
        }
        ("response", "Read") => {
            let value = FsResponse::Read {
                bytes: content.to_vec(),
            };
            assert_eq!(
                keld_ipc::codec::encode(&value).expect("encode Read result"),
                wire,
                "{name}"
            );
            let FsResponse::Read { bytes } = decode(wire).expect("decode Read result") else {
                return Err("response variant changed");
            };
            assert_eq!(bytes, content, "{name}");
        }
        ("response", "Write") => {
            assert_eq!(
                keld_ipc::codec::encode(&FsResponse::Write).expect("encode Write result"),
                wire,
                "{name}"
            );
            assert!(
                matches!(
                    decode::<FsResponse>(wire).expect("decode Write result"),
                    FsResponse::Write
                ),
                "{name}"
            );
        }
        _ => return Err("unsupported semantic vector"),
    }
    Ok(())
}

/// KEL-140 AC2: immutable semantic bytes are shared with generated TypeScript.
/// Variant or field reordering must fail this oracle rather than updating it.
#[test]
fn native_fs_payloads_match_shared_semantic_vectors_and_decode_strictly() {
    let fixture = include_str!("fixtures/fs-payload-v0.tsv");
    let mut names = Vec::new();
    for row in fixture
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
    {
        let fields: Vec<_> = row.split('\t').collect();
        assert_eq!(fields.len(), 6, "{row}");
        let [name, direction, variant, path, content, wire] = fields.as_slice() else {
            panic!("six-column fixture");
        };
        names.push(*name);
        let content = vector_hex(content);
        let wire = vector_hex(wire);
        assert_native_fs_semantic_vector(name, direction, variant, path, &content, &wire)
            .expect(name);
        for end in 0..wire.len() {
            let rejected = if *direction == "request" {
                decode::<FsRequest>(&wire[..end]).expect_err("truncated request")
            } else {
                decode::<FsResponse>(&wire[..end]).expect_err("truncated response")
            };
            assert!(
                matches!(rejected, IpcError::Codec(_)),
                "{name}, prefix {end}: {rejected}"
            );
        }
        let mut trailing = wire;
        trailing.push(0);
        let rejected = if *direction == "request" {
            decode::<FsRequest>(&trailing).expect_err("trailing request")
        } else {
            decode::<FsResponse>(&trailing).expect_err("trailing response")
        };
        assert!(matches!(rejected, IpcError::Codec(_)), "{name}: {rejected}");
    }
    assert_eq!(
        names,
        [
            "read-path",
            "read-unicode",
            "write-empty",
            "write-binary",
            "write-unicode",
            "write-long",
            "read-empty-result",
            "read-binary-result",
            "read-unicode-result",
            "read-long-result",
            "write-result"
        ]
    );
}

#[test]
fn native_fs_payload_codec_rejects_unknown_variants_and_malformed_lengths_or_utf8() {
    for wire in [&[2][..], &[0x80][..], &[0xff, 0xff, 0xff, 0xff, 0x10][..]] {
        assert!(
            matches!(decode::<FsRequest>(wire), Err(IpcError::Codec(_))),
            "request {wire:?}"
        );
        assert!(
            matches!(decode::<FsResponse>(wire), Err(IpcError::Codec(_))),
            "response {wire:?}"
        );
    }
    assert!(
        matches!(decode::<FsRequest>(&[0, 1, 0xff]), Err(IpcError::Codec(_))),
        "invalid UTF-8 path"
    );
    assert!(
        matches!(decode::<FsResponse>(&[0, 5, 0]), Err(IpcError::Codec(_))),
        "claimed vector exceeds payload"
    );
}

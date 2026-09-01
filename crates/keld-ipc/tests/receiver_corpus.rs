//! Canonical receiver-semantics corpus runner (KEL-133 spec §4).
//!
//! `tests/fixtures/receiver-semantics-v0.tsv` is the single owner of the
//! hostile/positive vector semantics shared by Rust and Bun consumers. This
//! runner feeds every frame row through the *production* staged pipeline —
//! header syntax → envelope cap → shared semantic validator → payload
//! codec/token — and every trace row through the deadline model, then prints
//! the fixture's SHA-256 so both test suites can be compared and the digest
//! recorded in the T1 artifact. Consumers must not copy the semantic table;
//! they load this file (spec §3 criterion 10).

#![allow(clippy::expect_used, clippy::panic)] // extra test crate: expect/panic are the assertion oracles against corpus rows

use std::io::Cursor;
use std::sync::OnceLock;

use keld_ipc::link::{read_frame, read_validated_frame};
use keld_ipc::receive::{ReceivePolicy, validate_received_header};
use keld_ipc::{
    CallError, ChannelId, CorrelationId, EchoRequest, EchoResponse, FrameHeader, HEADER_LEN,
    IpcError, LifecycleEvent, LifecycleRequest, LifecycleResponse, SessionToken,
};

const CORPUS: &str = include_str!("fixtures/receiver-semantics-v0.tsv");
const CORPUS_PATH: &str = "tests/fixtures/receiver-semantics-v0.tsv";

/// Fixture token declared in the corpus version row (`0x01..0x20`). Not a
/// secret: the corpus must never contain one (spec §4).
fn fixture_token() -> SessionToken {
    let mut bytes = [0u8; 32];
    for (i, b) in bytes.iter_mut().enumerate() {
        *b = u8::try_from(i + 1).expect("fixture token byte");
    }
    SessionToken::from_bytes(bytes)
}

struct Row<'a> {
    id: &'a str,
    policy: &'a str,
    header_or_trace: &'a str,
    payload_hex: &'a str,
    expected_code: &'a str,
    link_action: &'a str,
    handler_effects: u32,
}

fn rows() -> &'static Vec<Row<'static>> {
    static ROWS: OnceLock<Vec<Row<'static>>> = OnceLock::new();
    ROWS.get_or_init(|| {
        let mut lines = CORPUS.lines();
        let version = lines.next().expect("corpus has a version row");
        assert!(
            version.starts_with("receiver-semantics-v0\tv1\t"),
            "corpus format is closed and versioned in the first row: {version}"
        );
        assert!(
            version.contains("app_link_io_deadline_ms=5000"),
            "trace rows depend on the declared stall limit"
        );
        lines
            .map(|line| {
                let mut cols = line.split('\t');
                let mut next = || cols.next().expect("seven tab-separated columns");
                let row = Row {
                    id: next(),
                    policy: next(),
                    header_or_trace: next(),
                    payload_hex: next(),
                    expected_code: next(),
                    link_action: next(),
                    handler_effects: next().parse().expect("handler_effects is a count"),
                };
                assert!(cols.next().is_none(), "exactly seven columns: {line}");
                row
            })
            .collect()
    })
}

fn unhex(hex: &str) -> Vec<u8> {
    if hex == "-" {
        return Vec::new();
    }
    assert!(hex.len().is_multiple_of(2), "even hex length: {hex}");
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).expect("lowercase hex"))
        .collect()
}

fn policy_by_name(name: &str) -> ReceivePolicy {
    let (base, arg) = match name.split_once(':') {
        Some((base, arg)) => (base, Some(arg)),
        None => (name, None),
    };
    let corr = || CorrelationId(arg.expect("policy arg").parse().expect("corr id"));
    match base {
        "server-pre-auth-hello" => ReceivePolicy::server_pre_auth_hello(),
        "client-await-hello" => ReceivePolicy::client_await_hello(),
        "echo-receiver" => ReceivePolicy::echo_receiver(),
        "echo-reply-waiter" => ReceivePolicy::echo_reply_waiter(corr()),
        "lifecycle-receiver" => ReceivePolicy::lifecycle_receiver(),
        "lifecycle-event-receiver" => ReceivePolicy::lifecycle_event_receiver(),
        "lifecycle-reply-waiter" => ReceivePolicy::lifecycle_reply_waiter(corr()),
        "privileged-fs-receiver" => ReceivePolicy::privileged_call_receiver(ChannelId(
            arg.expect("policy arg").parse().expect("channel id"),
        )),
        "primary-app-receiver" => ReceivePolicy::primary_app_receiver(),
        other => panic!("unknown corpus policy: {other}"),
    }
}

/// The `KELD-IPC-*` code of an error, taken from the production `Display`
/// (`lib.rs`) so this runner cannot drift into a second code table.
fn code_of(err: &IpcError) -> String {
    let text = err.to_string();
    let Some(code) = text.get(..12) else {
        panic!("error text must start with a KELD-IPC code: {text}");
    };
    code.to_owned()
}

/// Stage 2 for an admitted frame: exactly what the live receiver does next —
/// token comparison on a HELLO policy, the declared channel codec elsewhere.
fn stage_two(policy_name: &str, header: FrameHeader, payload: &[u8]) -> Result<(), IpcError> {
    let base = policy_name.split(':').next().expect("policy base");
    match (base, header.kind) {
        ("server-pre-auth-hello" | "client-await-hello", _) => {
            let peer = SessionToken::try_from_slice(payload)?;
            if peer == fixture_token() {
                Ok(())
            } else {
                Err(IpcError::HelloAuth {
                    detail: "HELLO session token mismatch",
                })
            }
        }
        ("primary-app-receiver", keld_ipc::FrameKind::Call) => Ok(()),
        ("echo-receiver", keld_ipc::FrameKind::Call) => {
            keld_ipc::codec::decode::<EchoRequest>(payload).map(|_| ())
        }
        ("echo-reply-waiter", keld_ipc::FrameKind::Reply) => {
            keld_ipc::codec::decode::<EchoResponse>(payload).map(|_| ())
        }
        ("lifecycle-receiver", keld_ipc::FrameKind::Call) => {
            keld_ipc::codec::decode::<LifecycleRequest>(payload).map(|_| ())
        }
        ("lifecycle-event-receiver", keld_ipc::FrameKind::Event) => {
            keld_ipc::codec::decode::<LifecycleEvent>(payload).map(|_| ())
        }
        ("lifecycle-reply-waiter", keld_ipc::FrameKind::Reply) => {
            keld_ipc::codec::decode::<LifecycleResponse>(payload).map(|_| ())
        }
        ("lifecycle-reply-waiter", keld_ipc::FrameKind::Err) => {
            keld_ipc::codec::decode::<CallError>(payload).map(|_| ())
        }
        // PING carries no payload; the future privileged channel declares its
        // codec under KEL-102/T3, not here.
        (_, keld_ipc::FrameKind::Ping) | ("privileged-fs-receiver", _) => Ok(()),
        (base, kind) => panic!("corpus stage-two has no rule for {base}/{kind:?}"),
    }
}

/// Every frame row reproduces its expected code through the production
/// pipeline, with the payload untouched on pre-payload rejections.
#[test]
fn every_frame_row_reproduces_its_expected_code() {
    let mut checked = 0usize;
    for row in rows() {
        if row.policy.starts_with("trace:") {
            continue;
        }
        let policy = policy_by_name(row.policy);
        let header_bytes = unhex(row.header_or_trace);
        let payload = unhex(row.payload_hex);

        if row.expected_code == "ok-header" {
            // Boundary row: the declared envelope admits at header stage; the
            // corpus does not materialize a MAX_FRAME_LEN payload.
            let header =
                FrameHeader::decode(header_bytes.as_slice().try_into().expect("16-byte header"))
                    .expect("boundary header decodes");
            assert!(
                usize::try_from(header.len).expect("len fits") <= keld_ipc::MAX_FRAME_LEN,
                "{}: boundary row must be within the envelope cap",
                row.id
            );
            validate_received_header(&policy, header)
                .unwrap_or_else(|e| panic!("{}: boundary header must admit: {e}", row.id));
            checked += 1;
            continue;
        }

        let mut stream = Cursor::new([header_bytes.as_slice(), payload.as_slice()].concat());
        let outcome = read_validated_frame(&mut stream, &policy)
            .map_err(|e| code_of(&e))
            .and_then(|(_validated, body)| {
                // Semantic rejection must precede the payload read; admitted
                // frames must consume exactly header + payload.
                assert_eq!(
                    stream.position(),
                    (HEADER_LEN + body.len()) as u64,
                    "{}: admitted frame consumes exactly its bytes",
                    row.id
                );
                stage_two(
                    row.policy,
                    FrameHeader::decode(
                        &unhex(row.header_or_trace)
                            .as_slice()
                            .try_into()
                            .expect("header"),
                    )
                    .expect("already decoded"),
                    &body,
                )
                .map_err(|e| code_of(&e))
            });

        match (row.expected_code, outcome) {
            ("ok", Ok(())) => {}
            (expected, Err(code)) if expected == code => {
                if code == "KELD-IPC-005" || code == "KELD-IPC-002" || code == "KELD-IPC-004" {
                    assert!(
                        stream.position() <= HEADER_LEN as u64,
                        "{}: pre-payload rejection must not consume payload bytes",
                        row.id
                    );
                }
                assert_eq!(
                    row.handler_effects, 0,
                    "{}: a rejected frame can have no handler effect",
                    row.id
                );
            }
            (expected, got) => panic!("{}: expected {expected}, got {got:?}", row.id),
        }
        checked += 1;
    }
    assert!(
        checked >= 55,
        "corpus must keep its coverage: {checked} frame rows"
    );
}

/// Deadline-trace rows: the virtual-clock model of spec §4's deadline rules.
/// `real` socket integration proves the same outcomes; this model is what Bun
/// consumers replicate so both agree on expiry arithmetic.
#[test]
fn every_trace_row_reproduces_its_expiry() {
    const STALL_LIMIT_MS: u64 = 5000; // app_link_io_deadline_ms in the version row
    let mut checked = 0usize;
    for row in rows() {
        let Some(params) = row.policy.strip_prefix("trace:") else {
            continue;
        };
        let (kind, ms) = params.split_once("-deadline-ms=").expect("trace parameter");
        let deadline_ms: u64 = ms.parse().expect("deadline ms");
        assert!(matches!(kind, "generation" | "session"), "{kind}");

        // Ordered byte arrivals on a virtual clock.
        let mut arrivals: Vec<(u64, usize)> = Vec::new();
        if row.header_or_trace != "-" && !row.header_or_trace.is_empty() {
            for action in row.header_or_trace.split(';') {
                let (at, hex) = action
                    .strip_prefix("at")
                    .and_then(|a| a.split_once('='))
                    .expect("atN=hex action");
                arrivals.push((at.parse().expect("virtual ms"), hex.len() / 2));
            }
        }

        // Spec deadline model: the enclosing absolute deadline never renews;
        // the first byte starts the started-frame stall clock; idle polls
        // start nothing. Expiry is the earliest applicable absolute instant.
        let expiry = match arrivals.first() {
            Some((first_byte_at, _)) => deadline_ms.min(first_byte_at + STALL_LIMIT_MS),
            None => deadline_ms,
        };
        let needed: usize = HEADER_LEN + 32; // every v1 trace is a HELLO admission
        let mut got = 0usize;
        let mut completed_at = None;
        for (at, len) in &arrivals {
            if *at >= expiry {
                break; // a byte after expiry never arrives: the link is closed
            }
            got += len;
            if got >= needed {
                completed_at = Some(*at);
                break;
            }
        }

        match (row.expected_code, completed_at) {
            ("ok", Some(at)) => assert!(at < expiry, "{}: admitted before expiry", row.id),
            ("KELD-IPC-006", None) => {
                if let Some(close_at) = row.link_action.strip_prefix("close-at-") {
                    let close_at: u64 = close_at.parse().expect("close-at ms");
                    assert_eq!(
                        expiry, close_at,
                        "{}: expiry instant must match the recorded action",
                        row.id
                    );
                }
            }
            (expected, got) => panic!("{}: expected {expected}, completion {got:?}", row.id),
        }
        checked += 1;
    }
    assert_eq!(checked, 6, "all six trace rows evaluated");
}

/// AC11 golden binding: the corpus's positive payload bytes are exactly what
/// the live codec produces today. A codec or field-order change fails here
/// before it can silently rewrite the corpus.
#[test]
fn positive_vectors_match_the_live_codec_bytes() {
    let by_id = |id: &str| {
        rows()
            .iter()
            .find(|r| r.id == id)
            .unwrap_or_else(|| panic!("corpus row {id}"))
    };
    assert_eq!(
        unhex(by_id("echo-call-valid").payload_hex),
        keld_ipc::codec::encode(&EchoRequest {
            message: "kipc".to_owned(),
            count: 3,
        })
        .expect("encode"),
    );
    assert_eq!(
        unhex(by_id("lifecycle-event-ready").payload_hex),
        keld_ipc::codec::encode(&LifecycleEvent::Ready).expect("encode"),
    );
    assert_eq!(
        unhex(by_id("lifecycle-call-quit").payload_hex),
        keld_ipc::codec::encode(&LifecycleRequest::Quit).expect("encode"),
    );
    assert_eq!(
        unhex(by_id("lifecycle-err-callerror").payload_hex),
        keld_ipc::codec::encode(&CallError {
            code: "KELD-GUARD001".to_owned(),
            message: "KELD-GUARD001: denied".to_owned(),
        })
        .expect("encode"),
    );
    // The HELLO positive rows carry the declared fixture token bytes.
    assert_eq!(
        unhex(by_id("hello-valid").payload_hex),
        fixture_token().as_bytes(),
    );
    // And read_frame (syntax layer) still accepts the raw positive bytes
    // unchanged — the validator inserts no byte-level change (AC11).
    let full = [
        unhex(by_id("echo-call-valid").header_or_trace),
        unhex(by_id("echo-call-valid").payload_hex),
    ]
    .concat();
    let (header, payload) = read_frame(&mut Cursor::new(full)).expect("legacy read accepts");
    assert_eq!(header.corr, CorrelationId(7));
    assert_eq!(payload, unhex(by_id("echo-call-valid").payload_hex));
}

/// One owner, one digest (spec §3 criterion 10): printed here and by the Bun
/// suite; recorded in the T1 artifact. Also pins that the on-disk fixture is
/// byte-identical to the embedded copy this runner validated.
#[test]
fn corpus_digest_is_printed_and_disk_matches_embedded() {
    use sha2::{Digest, Sha256};
    let disk = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(CORPUS_PATH))
        .expect("fixture file readable");
    assert_eq!(
        disk,
        CORPUS.as_bytes(),
        "on-disk fixture must match the embedded corpus"
    );
    let digest = Sha256::digest(&disk);
    println!("receiver-semantics-v0.tsv sha256={digest:x}");
}

//! Physical-macOS AC9 proof at the production echo stream owner.
//!
//! The TSV owns malformed bytes and expected codes. This module observes the
//! host result separately from peer EOF; its half-close is not retry evidence.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use keld_ipc::link::{AppLinkDeadlines, handshake_client};
use keld_ipc::{CorrelationId, EchoRequest, IpcError, echo_invoke, parse_app_link};

use super::EchoServer;

const CHILD_TEST: &str = "echo_link::ac9_macos_tests::truncated_frame_child";
const CHILD_ENV: &str = "KELD_AC9_TRUNCATED_FRAME_CHILD";
const ROW_IDS: [&str; 2] = ["truncated-header-8", "truncated-payload"];
const KILL_SWITCH: Duration = Duration::from_secs(20);
const CORPUS: &str = include_str!("../../../keld-ipc/tests/fixtures/receiver-semantics-v0.tsv");

struct CorpusCase {
    id: &'static str,
    bytes: Vec<u8>,
    expected_code: &'static str,
}

// This selects fixture inputs; all wire parsing stays in the production reader.
fn corpus_case(id: &'static str) -> CorpusCase {
    let mut matching = CORPUS
        .lines()
        .skip(1)
        .filter(|line| line.split('\t').next() == Some(id));
    let columns: Vec<_> = matching
        .next()
        .expect("canonical corpus row exists")
        .split('\t')
        .collect();
    assert!(matching.next().is_none(), "canonical row id is unique");
    assert_eq!(columns.len(), 7, "canonical corpus has seven columns");
    assert_eq!(columns[1], "echo-receiver", "use the declared session");
    assert_eq!(columns[5], "close", "canonical truncated frame is terminal");
    assert_eq!(columns[6], "0", "canonical rejected frame has no effects");
    let mut bytes = Vec::new();
    for hex in &columns[2..4] {
        if *hex == "-" {
            continue;
        }
        assert!(hex.len().is_multiple_of(2), "fixture hex has whole bytes");
        for offset in (0..hex.len()).step_by(2) {
            bytes.push(u8::from_str_radix(&hex[offset..offset + 2], 16).expect("fixture hex"));
        }
    }
    CorpusCase {
        id,
        bytes,
        expected_code: columns[4],
    }
}

// Emergency cleanup is only a kill switch. The scenario separately requires
// observed child completion, so this Drop cannot turn a stuck child into a pass.
struct ChildOwner(Child);

impl Drop for ChildOwner {
    fn drop(&mut self) {
        if !matches!(self.0.try_wait(), Ok(Some(_))) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

#[test]
fn authenticated_truncated_corpus_rows_preserve_host_io_error() {
    let capture = tempfile::tempdir().expect("private child capture");
    let stdout_path = capture.path().join("stdout.log");
    let stderr_path = capture.path().join("stderr.log");
    let child = Command::new(std::env::current_exe().expect("current test binary"))
        .args(["--exact", CHILD_TEST, "--ignored", "--nocapture"])
        .env(CHILD_ENV, "1")
        .stdout(Stdio::from(
            File::create(&stdout_path).expect("child stdout"),
        ))
        .stderr(Stdio::from(
            File::create(&stderr_path).expect("child stderr"),
        ))
        .spawn()
        .expect("spawn exact AC9 child");
    let mut child = ChildOwner(child);
    let deadline = Instant::now() + KILL_SWITCH;
    let status = loop {
        if let Some(status) = child.0.try_wait().expect("observe child exit") {
            break status;
        }
        assert!(Instant::now() < deadline, "AC9 child exceeded kill switch");
        thread::yield_now();
    };
    let stdout = fs::read_to_string(stdout_path).expect("captured child stdout");
    let stderr = fs::read_to_string(stderr_path).expect("captured child stderr");
    println!("AC9_CHILD_EXIT {status}\n{stdout}");
    eprintln!("{stderr}");
    for id in ROW_IDS {
        let marker = format!("AC9_OBSERVATION row={id} ");
        assert_eq!(
            stdout.matches(&marker).count(),
            1,
            "exact child must observe {id}; zero selected tests cannot pass"
        );
    }
    assert!(
        stdout.contains("AC9_OBSERVATIONS_COMPLETE rows=2"),
        "child must complete both independent observations"
    );
    assert!(status.success(), "production host-result oracle failed");
}

#[test]
#[ignore = "subprocess entry; run only through the bounded AC9 parent"]
fn truncated_frame_child() {
    assert_eq!(
        std::env::var(CHILD_ENV).as_deref(),
        Ok("1"),
        "child entry requires its explicit parent"
    );
    let mut failures = Vec::new();
    for id in ROW_IDS {
        let case = corpus_case(id);
        let (ready_tx, ready_rx) = mpsc::channel();
        let mut server = EchoServer::start(&ready_tx).expect("production echo owner");
        ready_rx
            .recv_timeout(KILL_SWITCH)
            .expect("production listener bound");
        let link = server.link();
        let (endpoint, token) = parse_app_link(&link).expect("production app link");
        let mut peer = UnixStream::connect(endpoint).expect("real Unix peer");
        peer.set_app_link_deadlines(Some(Duration::from_secs(5)))
            .expect("bounded peer I/O");
        handshake_client(&mut peer, &token).expect("production HELLO");
        let request = EchoRequest {
            message: format!("AC9 healthy control {id}"),
            count: 1,
        };
        let response =
            echo_invoke(&mut peer, &request, CorrelationId(1)).expect("healthy admitted echo");
        assert_eq!(response.message, request.message);
        assert_eq!(response.count, request.count);
        println!("AC9_HEALTHY row={id} authenticated_echo=passed");

        peer.write_all(&case.bytes)
            .expect("send canonical truncated bytes");
        // Deliver real EOF after the canonical prefix; retain the read side.
        // This local half-close does not prove a product retry prohibition.
        peer.shutdown(Shutdown::Write).expect("peer write-half EOF");
        let mut byte = [0u8; 1];
        let peer_bytes = peer
            .read(&mut byte)
            .expect("observe production peer outcome");
        assert_eq!(peer_bytes, 0, "production closes with no response bytes");

        // EchoServer's production worker owns/drops the accepted stream.
        // Keep the server object alive: neither shutdown nor its Drop supplies
        // the peer-close or worker-result observations.
        let outcome = server
            .handle
            .take()
            .expect("production worker handle")
            .join()
            .expect("production worker did not panic");
        let matches_contract = match &outcome {
            Err(IpcError::Io(error)) => {
                error.kind() == std::io::ErrorKind::UnexpectedEof
                    && outcome
                        .as_ref()
                        .expect_err("matched I/O failure")
                        .to_string()
                        .starts_with(case.expected_code)
            }
            _ => false,
        };
        println!(
            "AC9_OBSERVATION row={} expected_host={} actual_host={outcome:?} peer_read_bytes={peer_bytes} production_worker_joined=true before_server_drop=true retry_eligibility=not_observed",
            case.id, case.expected_code
        );
        if !matches_contract {
            failures.push(format!(
                "{} expected {}, observed {outcome:?}",
                case.id, case.expected_code
            ));
        }
        drop(server);
    }
    println!("AC9_OBSERVATIONS_COMPLETE rows=2");
    assert!(
        failures.is_empty(),
        "AC9 must retain the host I/O failure after a partial frame: {}",
        failures.join("; ")
    );
}

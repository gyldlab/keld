//! Closed joint-fixture evidence transport; it carries no lifecycle authority.
use super::super::native_generation_fixture::{Inputs, ObjectIdentity};
use super::super::native_observer_setup::OBSERVATION_BOUND;
use serde::Deserialize;
use serde_json::Value;
use std::fs::{self, File};
use std::io::{Read as _, Seek as _, SeekFrom, Write as _};
use std::net::TcpStream;
use std::os::unix::fs::MetadataExt as _;
use std::process::Command;
use std::thread;
use std::time::Instant;

const MAX_REPORT_BYTES: u64 = 32 * 1024;
const MAX_REPORTS: usize = 8;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct JointInputs {
    schema: String,
    pub(super) page_identity: ObjectIdentity,
    pub(super) report_identity: ObjectIdentity,
    pub(super) census_identity: ObjectIdentity,
    pub(super) observer_port: u16,
    pub(super) title: String,
}

impl JointInputs {
    pub(super) fn supplied(inputs: &Inputs) -> Self {
        let path = std::env::var_os("KELD_KEL143_FIXTURE_RECEIPT").expect("Root receipt");
        let raw: Value = serde_json::from_slice(&fs::read(path).expect("actual receipt bytes"))
            .expect("actual receipt JSON");
        let joint: Self = serde_json::from_value(raw["jointRenderer"].clone())
            .expect("named Host joint fixture receipt");
        assert_eq!(joint.schema, "keld.kel143.joint-native-renderer/v1");
        assert_eq!(
            joint.page_identity.path,
            inputs.stage_root.join("index.html")
        );
        assert_eq!(joint.title, "KEL140 Public FS Acceptance");
        assert_ne!(joint.observer_port, 0);
        for object in [
            &joint.page_identity,
            &joint.report_identity,
            &joint.census_identity,
        ] {
            object.verify(&object.path);
        }
        joint
    }

    pub(super) fn window_ids(&self, expected: Option<&[u32]>) -> Vec<u32> {
        let expectation = expected.map_or_else(
            || "snapshot".to_owned(),
            |ids| {
                format!(
                    "exact:{}",
                    ids.iter().map(u32::to_string).collect::<Vec<_>>().join(",")
                )
            },
        );
        let output = Command::new(&self.census_identity.path)
            .args([
                "query",
                &std::process::id().to_string(),
                &self.title,
                &expectation,
                &OBSERVATION_BOUND.as_secs().to_string(),
                if expected.is_some() {
                    "all"
                } else {
                    "on-screen"
                },
            ])
            .output()
            .expect("existing compiled native-window census");
        assert!(
            output.status.success() && output.stderr.is_empty(),
            "native window census: {output:?}"
        );
        let mut ids: Vec<u32> = std::str::from_utf8(&output.stdout)
            .expect("native window IDs")
            .lines()
            .map(|id| id.parse().expect("actual numeric WindowServer identity"))
            .collect();
        ids.sort_unstable();
        ids
    }

    pub(super) fn admit_after(&self) {
        // This is only ordinary fixture data sent to the existing /observe owner.
        // Actual native release/preflight/G2 installation already happened in Core.
        let body = b"{\"phase\":\"joint-g2-released\"}";
        let mut stream = TcpStream::connect(("127.0.0.1", self.observer_port))
            .expect("existing fixture observer");
        stream
            .set_read_timeout(Some(OBSERVATION_BOUND))
            .expect("fixture read bound");
        stream
            .set_write_timeout(Some(OBSERVATION_BOUND))
            .expect("fixture write bound");
        write!(stream, "POST /observe HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len())
            .expect("closed fixture report header");
        stream.write_all(body).expect("closed fixture report bytes");
        let mut reply = Vec::new();
        stream
            .take(4097)
            .read_to_end(&mut reply)
            .expect("existing observation acknowledgment");
        assert!(
            reply.len() <= 4096 && reply.starts_with(b"HTTP/1.1 200"),
            "fixture report acknowledgment"
        );
    }
}

pub(super) struct PageReports {
    file: File,
    delivered: usize,
    pub(super) rows: Vec<Value>,
}

impl PageReports {
    pub(super) fn open(joint: &JointInputs) -> Self {
        let file = File::open(&joint.report_identity.path).expect("pin Host-owned report artifact");
        let meta = file.metadata().expect("pinned report identity");
        assert_eq!(
            (meta.dev(), meta.ino()),
            (joint.report_identity.dev, joint.report_identity.ino)
        );
        Self {
            file,
            delivered: 0,
            rows: Vec::new(),
        }
    }

    fn poll(&mut self) {
        assert!(self.file.metadata().expect("report length").len() <= MAX_REPORT_BYTES);
        self.file
            .seek(SeekFrom::Start(0))
            .expect("rewind pinned report");
        let mut bytes = Vec::new();
        std::io::Read::by_ref(&mut self.file)
            .take(MAX_REPORT_BYTES + 1)
            .read_to_end(&mut bytes)
            .expect("actual report bytes");
        assert!(bytes.len() as u64 <= MAX_REPORT_BYTES);
        let complete: Vec<_> = bytes
            .split_inclusive(|byte| *byte == b'\n')
            .filter(|row| row.ends_with(b"\n"))
            .collect();
        assert!(complete.len() <= MAX_REPORTS && complete.len() >= self.delivered);
        for row in &complete[self.delivered..] {
            assert!(row.len() <= 4096, "bounded named report");
            let value: Value =
                serde_json::from_slice(row).expect("actual page/app/fixture JSON report");
            assert_ne!(value["phase"], "error", "actual page failed: {value}");
            println!("KELD_KEL143_JOINT_REPORT {value}");
            self.rows.push(value);
        }
        self.delivered = complete.len();
    }

    pub(super) fn wait(&mut self, phase: &str) -> Value {
        let deadline = Instant::now() + super::super::GUARDIAN_OWNER_REPLY_DEADLINE;
        loop {
            self.poll();
            let found: Vec<_> = self
                .rows
                .iter()
                .filter(|row| row["phase"] == phase)
                .collect();
            assert!(found.len() <= 1, "duplicate observed phase {phase}");
            if let Some(row) = found.first() {
                return (*row).clone();
            }
            assert!(Instant::now() < deadline, "actual report {phase} absent");
            thread::yield_now();
        }
    }

    pub(super) fn wait_two_ready(&mut self) {
        let deadline = Instant::now() + super::super::GUARDIAN_OWNER_REPLY_DEADLINE;
        loop {
            self.poll();
            let ready: Vec<_> = self
                .rows
                .iter()
                .filter(|row| row["phase"] == "app-ready")
                .collect();
            assert!(
                ready.len() <= 2,
                "only two actual public whenReady receipts"
            );
            if ready.len() == 2 {
                assert_ne!(ready[0]["pid"], ready[1]["pid"]);
                assert_ne!(ready[0]["endpoint"], ready[1]["endpoint"]);
                return;
            }
            assert!(
                Instant::now() < deadline,
                "actual G2 whenReady receipt absent"
            );
            thread::yield_now();
        }
    }

    pub(super) fn assert_unique_page(&mut self) {
        self.poll();
        assert_eq!(
            self.rows
                .iter()
                .filter(|row| row["phase"] == "joint-page-ready")
                .count(),
            1
        );
        assert_eq!(
            self.rows
                .iter()
                .filter(|row| row["phase"] == "joint-pending-settled")
                .count(),
            1
        );
    }
}

pub(super) fn assert_document(report: &Value, initial: &Value) {
    for key in ["documentNonce", "javascriptToken", "initial"] {
        assert_eq!(report[key], initial[key], "unchanged actual document {key}");
    }
    for key in ["documentNonce", "javascriptToken"] {
        let text = report[key].as_str().expect("document-created nonce");
        assert_eq!(text.len(), 32);
        assert!(text.bytes().all(|byte| byte.is_ascii_hexdigit()));
    }
}

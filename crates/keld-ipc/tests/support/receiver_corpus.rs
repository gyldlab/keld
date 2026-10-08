//! Single data loader for the canonical receiver corpus; no protocol policy.
// Different test consumers use different fixture columns and constants.
#![allow(dead_code)]

use std::sync::OnceLock;

pub const CORPUS: &str = include_str!("../fixtures/receiver-semantics-v0.tsv");
pub const CORPUS_PATH: &str = "tests/fixtures/receiver-semantics-v0.tsv";
/// One owner, one digest: the Bun suite asserts this same constant, so a
/// corpus edit is an explicit, reviewed change to both consumers.
pub const CORPUS_SHA256: &str = "0cebb6e00c15a03028c6a29c725eb0e607ff66ee1cae04e227d18b9e49d0213e";

/// Fixture token declared in the corpus version row (`0x01..0x20`). Not a
/// secret: the corpus must never contain one (spec §4).
pub fn fixture_token_bytes() -> [u8; 32] {
    let mut bytes = [0u8; 32];
    for (i, b) in bytes.iter_mut().enumerate() {
        *b = u8::try_from(i + 1).expect("fixture token byte");
    }
    bytes
}

pub struct Row<'a> {
    pub id: &'a str,
    pub policy: &'a str,
    pub header_or_trace: &'a str,
    pub payload_hex: &'a str,
    pub expected_code: &'a str,
    pub link_action: &'a str,
    pub handler_effects: u32,
}

pub fn rows() -> &'static Vec<Row<'static>> {
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

pub fn unhex(hex: &str) -> Vec<u8> {
    if hex == "-" {
        return Vec::new();
    }
    assert!(hex.len().is_multiple_of(2), "even hex length: {hex}");
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).expect("lowercase hex"))
        .collect()
}

pub const TRUNCATED_ROWS: [&str; 2] = ["truncated-header-8", "truncated-payload"];
pub const MACOS_ECHO_ROWS: [&str; 7] = [
    "truncated-header-8",
    "truncated-payload",
    "bad-magic",
    "echo-call-trailing-byte",
    "oversized-envelope",
    "oversized-wins-over-semantics",
    "reply-to-echo-receiver",
];

pub fn row(id: &str) -> &'static Row<'static> {
    let mut found = rows().iter().filter(|row| row.id == id);
    let row = found.next().expect("canonical row exists");
    assert!(found.next().is_none(), "canonical row id is unique");
    row
}

impl Row<'_> {
    pub fn bytes(&self) -> Vec<u8> {
        assert!(!self.policy.starts_with("trace:"), "frame fixture required");
        [unhex(self.header_or_trace), unhex(self.payload_hex)].concat()
    }
}

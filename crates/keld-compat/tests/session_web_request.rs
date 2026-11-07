//! GH-470 F05-T1 listener slice: the named/default session bindings exist, but
//! GH-565 D4 refuses defaultSession with KELD-COMPAT-013 owned by GH-553.
//! The two cells remain RED against pinned Electron v44.4.5 web-request.md.
//! Linux execution proves only this platform-neutral refusal oracle, not macOS
//! listener acceptance. GH-553 owns any later recorded-listener divergence.
//!
//! Independent negative controls: absent exports falsify binding presence; plain
//! `TypeError` or a wrong typed field falsifies the refusal; successful registration
//! falsifies the outcome; calling app.whenReady falsifies zero Host attempts even
//! if the property still throws correctly. The recorder never opens a socket.

#![allow(clippy::expect_used, clippy::panic)] // extra test crate: expect/panic are assertion oracles

use std::path::{Path, PathBuf};
use std::process::Command;

/// The Bun fixture that records refusal fields and attempted Host connections.
const FIXTURE: &str = "web_request_absent.ts";
/// Prefix of every marker line the fixture prints.
const MARKER_PREFIX: &str = "GH470_";

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crates/keld-compat -> workspace root")
        .to_path_buf()
}

fn fixtures_dir() -> PathBuf {
    workspace_root().join("packages/@keld/electron/fixtures")
}

/// The fixture's `GH470_<name>=<value>` lines from one Bun run that exited 0.
struct Markers(Vec<(String, String)>);

impl Markers {
    fn observe() -> Self {
        let fixtures = fixtures_dir();
        assert!(
            fixtures.join("tsconfig.json").is_file(),
            "missing tsconfig.json paths alias at {}",
            fixtures.display()
        );
        let output = Command::new("bun")
            .arg(format!("./{FIXTURE}"))
            .current_dir(&fixtures)
            .env("KELD_APP_LINK", "gh470-recorder-only")
            .output()
            .expect("spawn bun — bun must be on PATH");
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        assert!(
            output.status.success(),
            "{FIXTURE} failed ({}): stdout={stdout} stderr={}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
        let markers = stdout
            .lines()
            .filter_map(|line| line.strip_prefix(MARKER_PREFIX)?.split_once('='))
            .map(|(name, value)| (name.to_owned(), value.to_owned()))
            .collect();
        Self(markers)
    }

    /// The one value recorded under `name`; a missing or repeated marker fails closed.
    fn one(&self, name: &str) -> &str {
        let mut values = self
            .0
            .iter()
            .filter(|(recorded, _)| recorded == name)
            .map(|(_, value)| value.as_str());
        let value = values
            .next()
            .unwrap_or_else(|| panic!("marker {MARKER_PREFIX}{name} missing: {:?}", self.0));
        assert!(
            values.next().is_none(),
            "marker {MARKER_PREFIX}{name} repeated: {:?}",
            self.0
        );
        value
    }
}

/// Both cells assert the current refusal, not successful listener support. Literal
/// fields come from GH-565 D4; the fixture only observes the actual thrown value.
fn assert_registration_refused(markers: &Markers, operation: &str) {
    assert_eq!(markers.one("SESSION_NAMED"), "object");
    assert_eq!(markers.one("SESSION_DEFAULT"), "object");
    assert_eq!(markers.one(operation), "threw:UnsupportedMemberError");
    for (field, expected) in [
        ("CODE", "string:KELD-COMPAT-013"),
        ("MEMBER", "string:session.defaultSession"),
        ("TICKET", "string:GH-553"),
        ("KIND", "string:property"),
    ] {
        assert_eq!(markers.one(&format!("{operation}_{field}")), expected);
    }
    assert_eq!(markers.one("HOST_CONNECTS"), "0");
}

/// Cell `web-request.on-before-request.file-url-filter`: draw.io's filtered listener.
#[test]
fn web_request_on_before_request_file_url_filter_default_session_refuses_typed() {
    assert_registration_refused(&Markers::observe(), "ON_BEFORE_REQUEST");
}

/// Cell `web-request.on-headers-received.response-headers`: draw.io's unfiltered listener.
#[test]
fn web_request_on_headers_received_default_session_refuses_typed() {
    assert_registration_refused(&Markers::observe(), "ON_HEADERS_RECEIVED");
}

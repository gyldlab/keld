//! Conformance (GH-470, the F05-T1 listener slice): present-day observation of draw.io's
//! two `session.defaultSession.webRequest` registrations against `@keld/electron`.
//!
//! Oracle: `docs/api/web-request.md` at the Electron v44.4.5 pin, cited per cell in
//! `fixtures/session-corpus/corpus.json` (corpus `electron-session-v1`). Both cells are
//! red until GH-553 exports `session` (gh532 rule 3), so each mapped test asserts today's
//! behaviour, which the Bun fixture `web_request_absent.ts` records through the
//! `electron` alias with no host: the facade has no `session` export, and draw.io's two
//! registration shapes (drawio-desktop@2edf9fb `src/main/electron.js:994` and `:1010`)
//! throw a `TypeError`. GH-553 inverts both tests and switches each cell's key to its
//! `intentional_divergence` (listener recorded, never invoked; the file:// read policy
//! and the response headers are host-owned, GH-473).
//!
//! Negative control: a stub `session.defaultSession.webRequest` in the fixture whose two
//! methods return `undefined` makes both tests fail at their `threw:TypeError` assertion
//! (the marker then reads `returned:undefined`). A fixture that exits non-zero, prints a
//! marker twice or omits one fails closed.

#![allow(clippy::expect_used, clippy::panic)] // extra test crate: expect/panic are assertion oracles

use std::path::{Path, PathBuf};
use std::process::Command;

/// The import-only Bun fixture that records the observations.
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
            .env_remove("KELD_APP_LINK")
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

/// Cell `web-request.on-before-request.file-url-filter`: draw.io registers
/// `onBeforeRequest({urls: ['file://*']}, listener)` (`electron.js:1010`).
#[test]
fn web_request_on_before_request_file_url_filter_today_no_session_export_registration_throws_type_error()
 {
    let markers = Markers::observe();
    assert_eq!(
        markers.one("SESSION_NAMED"),
        "undefined",
        "the facade exports no `session` binding yet (GH-553)"
    );
    assert_eq!(
        markers.one("SESSION_DEFAULT"),
        "undefined",
        "the default export carries no `session` yet (GH-553)"
    );
    assert_eq!(
        markers.one("ON_BEFORE_REQUEST"),
        "threw:TypeError",
        "draw.io's onBeforeRequest({{urls: ['file://*']}}, listener) registration must throw today"
    );
}

/// Cell `web-request.on-headers-received.response-headers`: draw.io registers
/// `onHeadersReceived(listener)` without a filter (`electron.js:994`).
#[test]
fn web_request_on_headers_received_today_no_session_export_registration_throws_type_error() {
    let markers = Markers::observe();
    assert_eq!(
        markers.one("SESSION_NAMED"),
        "undefined",
        "the facade exports no `session` binding yet (GH-553)"
    );
    assert_eq!(
        markers.one("ON_HEADERS_RECEIVED"),
        "threw:TypeError",
        "draw.io's onHeadersReceived(listener) registration must throw today"
    );
}

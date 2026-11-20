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
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Test-owned output files avoid pipe backpressure and inherited-pipe EOF waits.
struct ProbeOwner {
    root: PathBuf,
    child: Option<Child>,
}

impl Drop for ProbeOwner {
    fn drop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            // Also reap on assertion/unwind paths; a completed child tolerates kill.
            let _ = child.kill();
            let _ = child.wait();
        }
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn bounded_output(command: &mut Command, limit: Duration) -> (Output, bool) {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("gh470-{}-{nonce}", std::process::id()));
    // Exclusive creation fails closed on collision; never reuse an existing path.
    std::fs::create_dir(&root).expect("private probe output directory");
    let mut owner = ProbeOwner { root, child: None };
    let stdout_path = owner.root.join("stdout");
    let stderr_path = owner.root.join("stderr");
    let stdout = std::fs::File::create_new(&stdout_path).expect("probe stdout");
    let stderr = std::fs::File::create_new(&stderr_path).expect("probe stderr");
    owner.child = Some(
        command
            .stdin(Stdio::null())
            .stdout(stdout)
            .stderr(stderr)
            .spawn()
            .expect("spawn probe"),
    );
    // Release Command's parent-side file copies before scratch cleanup.
    command.stdout(Stdio::null()).stderr(Stdio::null());
    let child = owner.child.as_mut().expect("owned probe");
    let deadline = Instant::now() + limit;
    let (status, timed_out) = loop {
        if let Some(status) = child.try_wait().expect("poll probe") {
            break (status, false);
        }
        if Instant::now() >= deadline {
            // Exit can race the deadline; wait still reaps after a failed kill.
            let _ = child.kill();
            break (child.wait().expect("reap timed-out probe"), true);
        }
        // Observe exit, never use a sleep as synchronization.
        std::thread::yield_now();
    };
    // Both exit paths reaped the child before output is read or resources drop.
    owner.child.take();
    (
        Output {
            status,
            stdout: std::fs::read(stdout_path).expect("read probe stdout"),
            stderr: std::fs::read(stderr_path).expect("read probe stderr"),
        },
        timed_out,
    )
}

#[test]
fn probe_child_control() {
    match std::env::var("GH470_CHILD_CONTROL").as_deref() {
        Ok("hang") => loop {
            std::thread::park();
        },
        Ok("success") => {
            println!("GH470_CONTROL=ok");
            eprintln!("GH470_DIAGNOSTIC=ok");
        }
        _ => {}
    }
}

#[test]
fn probe_deadline_reaps_non_exiting_child_and_preserves_success_output() {
    for (mode, timeout) in [("hang", true), ("success", false)] {
        let mut command = Command::new(std::env::current_exe().expect("test executable"));
        command
            .args(["--exact", "probe_child_control", "--nocapture"])
            .env("GH470_CHILD_CONTROL", mode);
        let (output, timed_out) = bounded_output(&mut command, Duration::from_secs(2));
        assert_eq!(timed_out, timeout);
        if timeout {
            assert!(!output.status.success());
        } else {
            assert!(output.status.success());
            assert!(String::from_utf8_lossy(&output.stdout).contains("GH470_CONTROL=ok"));
            assert!(String::from_utf8_lossy(&output.stderr).contains("GH470_DIAGNOSTIC=ok"));
        }
    }
}

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
        let (output, timed_out) = bounded_output(
            Command::new("bun")
                .arg(format!("./{FIXTURE}"))
                .current_dir(&fixtures)
                .env("KELD_APP_LINK", "gh470-recorder-only"),
            Duration::from_secs(8),
        );
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        assert!(
            !timed_out && output.status.success(),
            "{FIXTURE} failed ({}; timed_out={timed_out}): stdout={stdout} stderr={}",
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

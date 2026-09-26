use crate::support::EVENT_DEADLINE;
use crate::support::TITLE;
use crate::support::control::wait_child_output;
use crate::support::process::await_process_gone;
use std::fs;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Read;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::process::Child;
use std::process::Command;
use std::process::Stdio;
use std::sync::mpsc;
use std::sync::mpsc::Receiver;
use std::thread;
use std::thread::JoinHandle;
use std::time::Instant;

pub(crate) fn compile_native_window_census(root: &Path) -> PathBuf {
    let source = root.join("native-window-census.swift");
    let executable = root.join("native-window-census");
    fs::write(
        &source,
        include_str!("../../../fixtures/native_window_census.swift"),
    )
    .expect("write native-window census");
    let output = Command::new("/usr/bin/xcrun")
        .args(["swiftc", "-O"])
        .arg(&source)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("compile native-window census");
    assert!(output.status.success(), "compile native census: {output:?}");
    executable
}

/// An external CoreGraphics observer is armed before spawn, when the target PID
/// is not yet known. Only the authenticated PID supplied at the original check
/// phase can consume its on-screen history, and those exact IDs must still live.
pub(crate) struct NativeWindowObserver {
    child: Option<Child>,
    events: Receiver<Result<String, String>>,
    reader: Option<JoinHandle<()>>,
}

impl NativeWindowObserver {
    pub(crate) fn arm(executable: &Path) -> Self {
        let mut child = Command::new(executable)
            .args(["observe", TITLE, "1", &EVENT_DEADLINE.as_secs().to_string()])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("start prearmed native-window observer");
        let stdout = child.stdout.take().expect("native-window observer stdout");
        let (sender, events) = mpsc::sync_channel(2);
        let reader = thread::spawn(move || {
            let mut stdout = BufReader::new(stdout);
            // Exactly two bounded records: readiness and one result. A broken
            // helper cannot grow the parent's buffer or block Drop on send.
            for _ in 0..2 {
                let mut line = String::new();
                let result = match stdout.by_ref().take(257).read_line(&mut line) {
                    Ok(0) => Err(String::from("native-window observer EOF")),
                    Ok(bytes) if bytes > 256 || !line.ends_with('\n') => {
                        Err(String::from("invalid native-window observer record"))
                    }
                    Ok(_) => Ok(line.trim_end().to_owned()),
                    Err(error) => Err(error.to_string()),
                };
                let failed = result.is_err();
                if sender.send(result).is_err() || failed {
                    break;
                }
            }
        });
        let observer = Self {
            child: Some(child),
            events,
            reader: Some(reader),
        };
        assert_eq!(
            observer.events.recv_timeout(EVENT_DEADLINE),
            Ok(Ok(String::from("READY"))),
            "native-window observer must be armed before host spawn"
        );
        observer
    }

    pub(crate) fn expect_initial(&mut self, pid: u32, observation: &str) -> Vec<u32> {
        // Precollection has its own lifetime. The existing check-phase deadline
        // starts here and includes command delivery, result reception and exit.
        let deadline = Instant::now() + EVENT_DEADLINE;
        writeln!(
            self.child
                .as_mut()
                .expect("live native-window observer")
                .stdin
                .as_mut()
                .expect("native-window observer command pipe"),
            "{pid}"
        )
        .expect("bind native-window history to authenticated PID");
        let event = self
            .events
            .recv_timeout(deadline.saturating_duration_since(Instant::now()));
        assert!(
            event.is_ok() && Instant::now() < deadline,
            "CoreGraphics presentation ({observation}): {event:?}; target-PID rows: {}",
            native_window_rows(pid)
        );
        let output = wait_child_output(
            self.child
                .take()
                .expect("native-window observer exit owner"),
            deadline.saturating_duration_since(Instant::now()),
        );
        assert!(
            output.status.success() && Instant::now() < deadline,
            "CoreGraphics presentation ({observation}): {event:?}; {output:?}; target-PID rows: {}",
            native_window_rows(pid)
        );
        let line = event
            .expect("received native-window event")
            .expect("native-window result");
        let windows: Vec<u32> = line
            .strip_prefix("WINDOWS ")
            .expect("native-window identity record")
            .split(',')
            .map(|id| id.parse().expect("numeric native-window identity"))
            .collect();
        assert_eq!(windows.len(), 1, "initial native-window count");
        windows
    }
}

impl Drop for NativeWindowObserver {
    fn drop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

#[test]
fn native_window_observer_history_retains_only_presented_live_identity() {
    let root = tempfile::tempdir().expect("native-window history fixture");
    let executable = compile_native_window_census(root.path());
    let output = Command::new(executable)
        .arg("history-regressions")
        .output()
        .expect("run fixed native-window history snapshots");
    assert!(output.status.success(), "history regressions: {output:?}");
    assert!(String::from_utf8_lossy(&output.stdout).starts_with("PASS native-window history:"));
}

#[test]
fn native_window_observer_reaps_on_assertion_unwind_and_parent_eof() {
    let root = tempfile::tempdir().expect("native-window cleanup fixture");
    let executable = compile_native_window_census(root.path());
    let observer = NativeWindowObserver::arm(&executable);
    let pid = observer.child.as_ref().expect("observer child").id();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        let _observer = observer;
        panic!("injected post-arm assertion failure");
    }));
    assert!(result.is_err(), "negative control must unwind");
    await_process_gone(pid);

    let mut observer = NativeWindowObserver::arm(&executable);
    let child = observer.child.as_mut().expect("observer child");
    let pid = child.id();
    drop(child.stdin.take());
    let output = wait_child_output(observer.child.take().expect("EOF observer"), EVENT_DEADLINE);
    assert_eq!(output.status.code(), Some(2), "parent EOF: {output:?}");
    assert_eq!(
        observer.events.recv_timeout(EVENT_DEADLINE),
        Ok(Err(String::from("native-window observer EOF")))
    );
    drop(observer);
    await_process_gone(pid);
}

pub(crate) fn native_windows(pid: u32, title: &str) -> Vec<u32> {
    query_native_windows(
        pid,
        title,
        NativeWindowScope::OnScreen,
        NativeWindowExpectation::Snapshot,
        "snapshot",
    )
}

/// Captures every CoreGraphics row owned by the target PID only when an existing
/// assertion fails. This is diagnostic evidence: initial launch retains its
/// title/layer/on-screen oracle, while recovery compares the recorded identity
/// through the all-window census. Both retain the same observation deadline.
pub(crate) fn native_window_rows(pid: u32) -> String {
    const SCRIPT: &str = r#"
import CoreGraphics
import Foundation
let wantedPID = Int(CommandLine.arguments[1])!
let onScreen = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as? [[String: Any]] ?? []
let onScreenIDs = Set(onScreen.compactMap { ($0[kCGWindowNumber as String] as? NSNumber)?.uint32Value })
let rows = CGWindowListCopyWindowInfo([.excludeDesktopElements], kCGNullWindowID) as? [[String: Any]] ?? []
let targetRows = rows.compactMap { row -> [String: Any]? in
  guard (row[kCGWindowOwnerPID as String] as? NSNumber)?.intValue == wantedPID else { return nil }
  let number = (row[kCGWindowNumber as String] as? NSNumber)?.uint32Value ?? 0
  let title = row[kCGWindowName as String] as? String
  return [
    "id": number,
    "layer": (row[kCGWindowLayer as String] as? NSNumber)?.intValue ?? -1,
    "on_screen": onScreenIDs.contains(number),
    "title": title ?? "<unavailable>",
    "title_available": title != nil,
  ]
}
let payload: [String: Any] = [
  "capture_preflight": CGPreflightScreenCaptureAccess(),
  "pid": wantedPID,
  "rows": targetRows,
]
let data = try! JSONSerialization.data(withJSONObject: payload, options: [.sortedKeys])
print(String(data: data, encoding: .utf8)!)
"#;
    match Command::new("/usr/bin/xcrun")
        .args(["swift", "-e", SCRIPT, &pid.to_string()])
        .output()
    {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).trim().to_owned()
        }
        Ok(output) => format!("diagnostic failed: {output:?}"),
        Err(error) => format!("diagnostic launch failed: {error}"),
    }
}

/// Recovery preserves the already-observed native window identity. Visibility is
/// a separate launch contract, so this check deliberately uses the all-window
/// census while keeping the existing bounded observation deadline.
pub(crate) fn await_same_native_windows(pid: u32, title: &str, expected: &[u32]) -> Vec<u32> {
    assert!(
        !expected.is_empty(),
        "same-window recovery needs an initially observed native window"
    );
    query_native_windows(
        pid,
        title,
        NativeWindowScope::All,
        NativeWindowExpectation::Exact(expected),
        "recovery-same-window",
    )
}

pub(crate) fn await_no_native_windows(pid: u32, title: &str) {
    let windows = query_native_windows(
        pid,
        title,
        NativeWindowScope::All,
        NativeWindowExpectation::Exact(&[]),
        "initial-navigation-window-release",
    );
    assert!(
        windows.is_empty(),
        "native window remained after startup rollback: {windows:?}"
    );
}

#[derive(Clone, Copy)]
pub(crate) enum NativeWindowScope {
    OnScreen,
    All,
}

#[derive(Clone, Copy)]
pub(crate) enum NativeWindowExpectation<'a> {
    Snapshot,
    Exact(&'a [u32]),
}

pub(crate) fn query_native_windows(
    pid: u32,
    title: &str,
    scope: NativeWindowScope,
    expectation: NativeWindowExpectation<'_>,
    observation: &str,
) -> Vec<u32> {
    const SCRIPT: &str = include_str!("../../../fixtures/native_window_census.swift");
    let expectation_arg = match expectation {
        NativeWindowExpectation::Snapshot => String::from("snapshot"),
        NativeWindowExpectation::Exact(windows) => format!(
            "exact:{}",
            windows
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(",")
        ),
    };
    let scope_arg = match scope {
        NativeWindowScope::OnScreen => "on-screen",
        NativeWindowScope::All => "all",
    };
    let timeout_arg = EVENT_DEADLINE.as_secs().to_string();
    let output = Command::new("/usr/bin/xcrun")
        .args([
            "swift",
            "-e",
            SCRIPT,
            "query",
            &pid.to_string(),
            title,
            &expectation_arg,
            &timeout_arg,
            scope_arg,
        ])
        .output()
        .expect("run native CoreGraphics census");
    assert!(
        output.status.success(),
        "CoreGraphics census ({observation}): {output:?}; target-PID CoreGraphics rows: {}",
        native_window_rows(pid)
    );
    String::from_utf8(output.stdout)
        .expect("CoreGraphics output UTF-8")
        .lines()
        .map(|line| line.parse().expect("CGWindowID is numeric"))
        .collect()
}

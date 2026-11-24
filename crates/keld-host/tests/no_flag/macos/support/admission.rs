#[cfg(test)]
use crate::support::EVENT_DEADLINE;
use crate::support::control::ReapOnDrop;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub(crate) struct NativeAbsenceWatcher {
    executable: PathBuf,
}

pub(crate) struct PolicyReadFault {
    pub(crate) library: PathBuf,
    pub(crate) marker: PathBuf,
}

impl PolicyReadFault {
    pub(crate) fn compile(root: &Path) -> Self {
        const SOURCE: &str = r#"
#include <errno.h>
#include <fcntl.h>
#include <limits.h>
#include <stdlib.h>
#include <string.h>
#include <sys/syscall.h>
#include <unistd.h>

#define DYLD_INTERPOSE(replacement, replacee) \
  __attribute__((used)) static struct { const void *replacement_ptr; const void *replacee_ptr; } \
  interpose_##replacee __attribute__((section("__DATA,__interpose"))) = { \
    (const void *)(unsigned long)&replacement, (const void *)(unsigned long)&replacee \
  };

static ssize_t fault_read(int fd, void *buffer, size_t count) {
  char path[PATH_MAX];
  const char *suffix = "/keld.permissions.jsonc";
  if (fcntl(fd, F_GETPATH, path) == 0) {
    size_t path_len = strlen(path);
    size_t suffix_len = strlen(suffix);
    if (path_len >= suffix_len && strcmp(path + path_len - suffix_len, suffix) == 0) {
      const char *marker = getenv("KELD_T2_READ_FAULT_MARKER");
      if (marker != NULL) {
        int marker_fd = open(marker, O_WRONLY | O_CREAT | O_TRUNC, 0600);
        if (marker_fd >= 0) {
          (void)write(marker_fd, "faulted\n", 8);
          (void)close(marker_fd);
        }
      }
      errno = EIO;
      return -1;
    }
  }
  return (ssize_t)syscall(SYS_read, fd, buffer, count);
}

DYLD_INTERPOSE(fault_read, read)
"#;
        let source = root.join("kel102-policy-read-fault.c");
        let library = root.join("kel102-policy-read-fault.dylib");
        let marker = root.join("kel102-policy-read-fault.marker");
        fs::write(&source, SOURCE).expect("write policy read-fault interposer");
        let output = Command::new("/usr/bin/clang")
            .args(["-dynamiclib", "-O2", "-o"])
            .arg(&library)
            .arg(&source)
            .output()
            .expect("compile policy read-fault interposer");
        assert!(output.status.success(), "compile interposer: {output:?}");
        Self { library, marker }
    }
}

const NATIVE_ABSENCE_SOURCE: &str = r#"
import CoreGraphics
import Darwin
import Foundation

func invalidArguments() -> Never {
  FileHandle.standardError.write(Data("invalid native absence watcher arguments\n".utf8))
  exit(2)
}

let args = CommandLine.arguments
guard args.count >= 4, let target = Int32(args[1]), target > 0,
      let rootCount = Int(args[2]), (1...3).contains(rootCount),
      args.count >= 3 + rootCount else { invalidArguments() }
let prefix = ("kb-" + String(target, radix: 16) + "-").utf8
let roots = args[3..<(3 + rootCount)]
let baselines = args.dropFirst(3 + rootCount)
guard roots.allSatisfy({ $0.hasPrefix("/") && !$0.utf8.contains(10) && !$0.utf8.contains(13) }),
      roots.reduce(0, { $0 + $1.utf8.count + 1 }) <= 8192,
      baselines.reduce(0, { $0 + $1.utf8.count + 1 }) <= 8192 else {
  invalidArguments()
}
var windows = Set<UInt32>()
var children = Set<Int>()
var sessions = Set<String>()
// Compare bytes, not Swift's Unicode-canonical String equality. Arguments
// are opaque keys from Rust: six integers followed by the full basename.
func isBaselineKey(_ key: String) -> Bool {
  let fields = key.split(separator: " ", maxSplits: 6, omittingEmptySubsequences: false)
  return fields.count == 7 && UInt64(fields[0]) != nil && UInt64(fields[1]) != nil
    && Int64(fields[2]) != nil && UInt32(fields[3]).map { $0 < 1_000_000_000 } == true
    && UInt64(fields[4]) != nil && UInt32(fields[5]).map { $0 < 1_000_000_000 } == true
    && fields[6].utf8.starts(with: prefix) && !fields[6].utf8.contains(47)
}
guard baselines.allSatisfy(isBaselineKey) else { invalidArguments() }
let staleSessions = Set(baselines.map { Data($0.utf8) })

func isPrelaunchDirectory(_ path: String, name: String) -> Bool {
  var info = stat()
  guard lstat(path, &info) == 0,
        (info.st_mode & mode_t(S_IFMT)) == mode_t(S_IFDIR),
        info.st_birthtimespec.tv_sec > 0
          || (info.st_birthtimespec.tv_sec == 0 && info.st_birthtimespec.tv_nsec > 0) else {
    return false
  }
  let device = UInt64(bitPattern: Int64(info.st_dev))
  let changed = info.st_ctimespec
  let born = info.st_birthtimespec
  let key = "\(device) \(info.st_ino) \(changed.tv_sec) \(changed.tv_nsec) \(born.tv_sec) \(born.tv_nsec) \(name)"
  return staleSessions.contains(Data(key.utf8))
}

func sessionNames(_ root: String, optional: Bool) -> [String] {
  // Only an optional fallback root that is absent before enumeration may be
  // skipped. Follow root aliases, but fail closed on an incomplete scan.
  if optional {
    var info = stat()
    if stat(root, &info) != 0 && errno == ENOENT { return [] }
  }
  do {
    return try FileManager.default.contentsOfDirectory(atPath: root)
  } catch {
    FileHandle.standardError.write(Data("native absence watcher could not scan session root: \(error)\n".utf8))
    exit(3)
  }
}

func processCensusFailure(_ reason: String) -> Never {
  FileHandle.standardError.write(Data("native absence watcher process census failed: \(reason)\n".utf8))
  exit(4)
}

func parseProcessID(_ field: Substring) -> Int32? {
  guard !field.isEmpty, field.utf8.allSatisfy({ (48...57).contains($0) }) else { return nil }
  return Int32(field)
}

func sample(requireTarget: Bool = false) {
  let rows = CGWindowListCopyWindowInfo([.excludeDesktopElements], kCGNullWindowID) as! [[String: Any]]
  for row in rows {
    let owner = (row[kCGWindowOwnerPID as String] as? NSNumber)?.int32Value
    if owner == target, let number = row[kCGWindowNumber as String] as? NSNumber {
      windows.insert(number.uint32Value)
    }
  }
  for (index, root) in roots.enumerated() {
    for name in sessionNames(root, optional: index != 0) where name.utf8.starts(with: prefix) {
      let path = root + "/" + name
      // A matching name was observed. Even if it vanishes before lstat, do
      // not erase that transient evidence unless its identity was proven.
      if !isPrelaunchDirectory(path, name: name) { sessions.insert(path) }
    }
  }
  let task = Process()
  task.executableURL = URL(fileURLWithPath: "/bin/ps")
  task.arguments = ["-axo", "ppid=,pid="]
  let pipe = Pipe()
  task.standardOutput = pipe
  task.standardError = FileHandle.nullDevice
  do {
    try task.run()
  } catch {
    processCensusFailure("could not launch ps: \(error)")
  }
  // Drain ps before waiting: its output may exceed the pipe capacity.
  let output = pipe.fileHandleForReading.readDataToEndOfFile()
  task.waitUntilExit()
  guard task.terminationReason == .exit, task.terminationStatus == 0 else {
    processCensusFailure("ps did not exit successfully (status \(task.terminationStatus))")
  }
  guard let text = String(data: output, encoding: .utf8), text.last == "\n" else {
    processCensusFailure("invalid process census encoding or incomplete output")
  }
  // This is a sample, not an exhaustive lifecycle trace.
  var sawTarget = false
  for line in text.dropLast().split(separator: "\n", omittingEmptySubsequences: false) {
    let fields = line.split(whereSeparator: { $0 == " " || $0 == "\t" })
    guard fields.count == 2, let parent = parseProcessID(fields[0]), let child = parseProcessID(fields[1]) else {
      processCensusFailure("invalid process census row")
    }
    if child == target { sawTarget = true }
    if parent == target { children.insert(Int(child)) }
  }
  if requireTarget && !sawTarget {
    processCensusFailure("initial process census did not contain the stopped target")
  }
}

// The Rust parent owns the stopped child, its resume, and the terminal
// observation. stdin carries exactly one final-sample request, never a PID.
func finalSampleRequested() -> Bool {
  var descriptor = pollfd(fd: STDIN_FILENO, events: Int16(POLLIN), revents: 0)
  while true {
    let result = Darwin.poll(&descriptor, 1, 0)
    if result == 0 { return false }
    if result < 0 && errno == EINTR { continue }
    guard result == 1, (descriptor.revents & Int16(POLLIN)) != 0 else {
      FileHandle.standardError.write(Data("native absence watcher lost final-sample control\n".utf8))
      exit(5)
    }
    var command: UInt8 = 0
    let bytes = Darwin.read(STDIN_FILENO, &command, 1)
    if bytes < 0 && errno == EINTR { continue }
    guard bytes == 1, command == 70 else {
      FileHandle.standardError.write(Data("native absence watcher invalid final-sample control\n".utf8))
      exit(5)
    }
    return true
  }
}

// Rust retains the stopped child until this first sample completes.
sample(requireTarget: true)
print("READY")
fflush(stdout)
while !finalSampleRequested() { sample() }
sample()
for value in windows.sorted() { print("WINDOW \(value)") }
for value in children.sorted() { print("CHILD \(value)") }
for value in sessions.sorted() { print("SESSION \(value)") }
print("DONE")
"#;

impl NativeAbsenceWatcher {
    pub(crate) fn compile(root: &Path) -> Self {
        Self::compile_source(root, NATIVE_ABSENCE_SOURCE)
    }

    fn compile_source(root: &Path, swift: &str) -> Self {
        let source = root.join("kel96-native-absence.swift");
        let executable = root.join("kel96-native-absence");
        fs::write(&source, swift).expect("write native absence watcher");
        let output = Command::new("/usr/bin/xcrun")
            .args([
                "swiftc",
                "-O",
                source.to_str().expect("watcher source UTF-8"),
                "-o",
            ])
            .arg(&executable)
            .output()
            .expect("compile native absence watcher");
        assert!(output.status.success(), "compile watcher: {output:?}");
        Self { executable }
    }

    pub(crate) fn spawn(&self, pid: u32, roots: &[String], baseline: Vec<&str>) -> ReapOnDrop {
        ReapOnDrop(
            Command::new(&self.executable)
                .arg(pid.to_string())
                .args(roots)
                .args(baseline)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .expect("start native absence watcher"),
        )
    }
}

#[test]
fn native_absence_watcher_reports_only_post_census_session_directories() {
    use crate::support::control::observe_owned_stopped_child;
    use crate::support::process::{SessionDirCensus, await_process_state};

    let fixture = tempfile::tempdir().expect("create native watcher fixture");
    let watcher = NativeAbsenceWatcher::compile(fixture.path());
    let mut shell = Command::new("/bin/sh");
    shell.args(["-c", "kill -STOP $$; exit 0"]);
    let child = ReapOnDrop(shell.spawn().expect("start inert fixture process"));
    let pid = child.0.id();
    await_process_state(pid, 'T');

    let stale = fixture.path().join(format!("kb-{pid:x}-prelaunch"));
    let stale_unicode = fixture
        .path()
        .join(format!("kb-{pid:x}-\u{0301} prelaunch"));
    let new = fixture
        .path()
        .join(format!("kb-{pid:x}-\u{0301} post-census"));
    fs::create_dir(&stale).expect("plant the ordinary prelaunch directory");
    fs::create_dir(&stale_unicode).expect("plant the Unicode prelaunch directory");
    let census = SessionDirCensus::before_launch_in(vec![fixture.path().to_path_buf()]);
    let roots = census
        .watcher_roots()
        .expect("encode the private fixture root");
    fs::create_dir(&new).expect("plant the ordinary post-census directory");

    let native = watcher.spawn(pid, &roots, census.watcher_baseline(pid));
    let (child_output, output) = observe_owned_stopped_child(child, native, EVENT_DEADLINE, || {});
    assert!(
        child_output.status.success(),
        "fixture process failed: {child_output:?}"
    );
    assert!(output.status.success(), "native watcher failed: {output:?}");
    let observations = String::from_utf8(output.stdout).expect("watcher output UTF-8");
    assert_eq!(
        observations,
        format!("READY\nSESSION {}\nDONE\n", new.display())
    );
}

#[test]
fn native_absence_watcher_rejects_incomplete_process_census() {
    use crate::support::control::wait_owned_child_output_observing;
    use crate::support::process::{SessionDirCensus, await_process_state};
    use std::io::Write as _;

    let fixture = tempfile::tempdir().expect("create private census fixture");
    let source = NATIVE_ABSENCE_SOURCE
        .replace("/bin/ps", "/bin/sh")
        .replace(
            r#"task.arguments = ["-axo", "ppid=,pid="]"#,
            r#"task.arguments = ["-c", "/bin/cat \"$1\"; exit \"$(/bin/cat \"$2\")\"", "sample-fixture", roots.first! + "/rows", roots.first! + "/status"]"#,
        );
    assert!(
        source.contains("sample-fixture"),
        "inject only the ps invocation"
    );
    let watcher = NativeAbsenceWatcher::compile_source(fixture.path(), &source);
    // The synthetic ps fixture never controls a process. An owned stopped
    // shell supplies the real PID for independent window/directory sampling.
    let target = ReapOnDrop(
        Command::new("/bin/sh")
            .args(["-c", "kill -STOP $$"])
            .spawn()
            .expect("start inert census target"),
    );
    let pid = target.0.id();
    await_process_state(pid, 'T');
    let roots = SessionDirCensus::before_launch_in(vec![fixture.path().to_path_buf()])
        .watcher_roots()
        .expect("encode the private fixture root");
    let target_only = format!(" 0 {pid}\n");
    let unrelated_only = format!(" 0 {}\n", pid + 1);
    let observed_child = format!(" 0 {pid}\n {pid} 174\n");
    let malformed = format!(" 0 {pid}\n malformed\n");
    let incomplete = format!(" 0 {pid}");
    for (rows, status, expected) in [
        (
            target_only.as_bytes(),
            "0",
            Some(b"READY\nDONE\n".as_slice()),
        ),
        (unrelated_only.as_bytes(), "0", None),
        (
            observed_child.as_bytes(),
            "0",
            Some(b"READY\nCHILD 174\nDONE\n"),
        ),
        (observed_child.as_bytes(), "1", None),
        (malformed.as_bytes(), "0", None),
        (incomplete.as_bytes(), "0", None),
        (b"", "0", None),
        (b"\xff\n", "0", None),
    ] {
        fs::write(fixture.path().join("rows"), rows).expect("write sample bytes");
        fs::write(fixture.path().join("status"), status).expect("write sample status");
        let mut child = watcher.spawn(pid, &roots, Vec::new());
        if expected.is_some() {
            child
                .0
                .stdin
                .as_mut()
                .expect("owned sampler control input")
                .write_all(b"F")
                .expect("request final sample");
        }
        let output = wait_owned_child_output_observing(child, EVENT_DEADLINE, || {});
        if let Some(expected) = expected {
            assert!(output.status.success(), "{output:?}");
            assert_eq!(output.stdout, expected);
        } else {
            assert_eq!(output.status.code(), Some(4), "{output:?}");
            assert!(output.stdout.is_empty(), "partial sample was reported");
            assert!(
                String::from_utf8_lossy(&output.stderr).contains("process census failed"),
                "{output:?}"
            );
        }
        if rows == unrelated_only.as_bytes() {
            assert!(
                String::from_utf8_lossy(&output.stderr)
                    .contains("did not contain the stopped target"),
                "{output:?}",
            );
        }
    }
    // No watcher owns this PID or resumes the synthetic census target.
    await_process_state(pid, 'T');
}

#[test]
fn native_absence_samples_do_not_claim_between_sample_directory_coverage() {
    use crate::support::control::observe_owned_stopped_child;
    use crate::support::process::{FixtureDirectoryWrites, SessionDirCensus, await_process_state};
    use std::io::{ErrorKind, Write as _};

    let fixture = tempfile::tempdir().expect("create private sampler fixture");
    let observed = fixture.path().join("observed");
    fs::create_dir(&observed).expect("create private scan root");
    // Keep the real initial and final sample paths. Block only the injected
    // observer's intervening loop until the owner requests its final sample.
    let loop_source = "while !finalSampleRequested() { sample() }";
    assert_eq!(NATIVE_ABSENCE_SOURCE.matches(loop_source).count(), 1);
    let source = NATIVE_ABSENCE_SOURCE.replace(
        loop_source,
        "guard getchar() == 70 else { invalidArguments() }",
    );
    let watcher = NativeAbsenceWatcher::compile_source(fixture.path(), &source);
    let census = SessionDirCensus::before_launch_in(vec![observed.clone()]);
    let roots = census.watcher_roots().expect("encode owned root");
    let mut child = ReapOnDrop(
        Command::new("/bin/sh")
            .args([
                "-c",
                r#"set -eu
kill -STOP $$
name=$(printf 'kb-%x-between-samples' "$$")
path=$1/$name
/bin/mkdir "$path"
IFS= read -r release
[ "$release" = "retire" ]
/bin/rmdir "$path""#,
                "between-sample-fixture",
            ])
            .arg(&observed)
            .stdin(Stdio::piped())
            .spawn()
            .expect("start inert directory fixture"),
    );
    let pid = child.0.id();
    await_process_state(pid, 'T');
    let path = observed.join(format!("kb-{pid:x}-between-samples"));
    let mut release = child.0.stdin.take();
    assert_eq!(
        fs::symlink_metadata(&path)
            .expect_err("fixture starts absent")
            .kind(),
        ErrorKind::NotFound,
    );
    let writes = FixtureDirectoryWrites::arm(&observed);
    assert!(
        !writes.take_write(),
        "private directory must start without an event"
    );
    let native = watcher.spawn(pid, &roots, census.watcher_baseline(pid));
    let (child_output, native_output) =
        observe_owned_stopped_child(child, native, EVENT_DEADLINE, || {
            let Some(waiting) = release.as_mut() else {
                return;
            };
            match fs::symlink_metadata(&path) {
                Ok(info) => {
                    assert!(
                        info.file_type().is_dir(),
                        "owned fixture must be a directory"
                    );
                    if !writes.take_write() {
                        return;
                    }
                    // The child cannot remove the exact PID-matched directory
                    // until this independent observation releases it.
                    waiting
                        .write_all(b"retire\n")
                        .expect("release owned directory");
                    release.take();
                }
                Err(error) if error.kind() == ErrorKind::NotFound => {}
                Err(error) => panic!("inspect owned fixture directory: {error}"),
            }
        });
    assert!(child_output.status.success(), "{child_output:?}");
    assert!(
        release.is_none(),
        "creation was never observed independently"
    );
    assert_eq!(
        fs::symlink_metadata(&path)
            .expect_err("owned directory was retired")
            .kind(),
        ErrorKind::NotFound,
    );
    // The first event was consumed before removal was permitted. The held
    // private directory must report the independent retirement event as well.
    assert!(
        writes.take_write(),
        "retirement generated no directory event"
    );
    // A sample-only oracle is deliberately blind to this verified lifecycle.
    assert_eq!(native_output.stdout, b"READY\nDONE\n");
}

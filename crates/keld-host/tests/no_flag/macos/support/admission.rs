use crate::support::EVENT_DEADLINE;
use crate::support::control::{ReapOnDrop, wait_owned_child_output_observing};
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
let prefix = "kb-" + String(target, radix: 16) + "-"
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
    && fields[6].hasPrefix(prefix) && !fields[6].contains("/")
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

func sample() {
  let rows = CGWindowListCopyWindowInfo([.excludeDesktopElements], kCGNullWindowID) as! [[String: Any]]
  for row in rows {
    let owner = (row[kCGWindowOwnerPID as String] as? NSNumber)?.int32Value
    if owner == target, let number = row[kCGWindowNumber as String] as? NSNumber {
      windows.insert(number.uint32Value)
    }
  }
  for (index, root) in roots.enumerated() {
    for name in sessionNames(root, optional: index != 0) where name.hasPrefix(prefix) {
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
  try! task.run()
  task.waitUntilExit()
  let text = String(data: pipe.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8)!
  for line in text.split(separator: "\n") {
    let fields = line.split(whereSeparator: { $0 == " " || $0 == "\t" })
    if fields.count == 2, Int(fields[0]) == Int(target), let child = Int(fields[1]) {
      children.insert(child)
    }
  }
}

sample()
print("READY")
fflush(stdout)
_ = kill(target, SIGCONT)
while kill(target, 0) == 0 {
  sample()
}
sample()
for value in windows.sorted() { print("WINDOW \(value)") }
for value in children.sorted() { print("CHILD \(value)") }
for value in sessions.sorted() { print("SESSION \(value)") }
print("DONE")
"#;

impl NativeAbsenceWatcher {
    pub(crate) fn compile(root: &Path) -> Self {
        let source = root.join("kel96-native-absence.swift");
        let executable = root.join("kel96-native-absence");
        fs::write(&source, NATIVE_ABSENCE_SOURCE).expect("write native absence watcher");
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
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .expect("start native absence watcher"),
        )
    }
}

/// Detect an observation failure while the host may still be suspended.
pub(crate) fn assert_watcher_not_failed(watcher: &mut Option<ReapOnDrop>, case: &str) {
    if watcher
        .as_mut()
        .expect("owned native watcher")
        .0
        .try_wait()
        .expect("inspect watcher exit")
        .is_some_and(|status| !status.success())
    {
        let output = wait_owned_child_output_observing(
            watcher.take().expect("owned native watcher"),
            EVENT_DEADLINE,
            || {},
        );
        panic!("{case}: watcher failed before host completion: {output:?}");
    }
}

#[test]
fn native_absence_watcher_reports_only_post_census_session_directories() {
    use crate::support::process::{SessionDirCensus, await_process_state};

    let fixture = tempfile::tempdir().expect("create native watcher fixture");
    let watcher = NativeAbsenceWatcher::compile(fixture.path());
    let mut shell = Command::new("/bin/sh");
    shell.args(["-c", "kill -STOP $$; exit 0"]);
    let child = ReapOnDrop(shell.spawn().expect("start inert fixture process"));
    let pid = child.0.id();
    await_process_state(pid, 'T');

    let stale = fixture.path().join(format!("kb-{pid:x}-prelaunch"));
    let new = fixture.path().join(format!("kb-{pid:x}-post-census"));
    fs::create_dir(&stale).expect("plant the ordinary prelaunch directory");
    let census = SessionDirCensus::before_launch_in(vec![fixture.path().to_path_buf()]);
    let roots = census
        .watcher_roots()
        .expect("encode the private fixture root");
    fs::create_dir(&new).expect("plant the ordinary post-census directory");

    // The watcher samples synchronously before it resumes the fixture.
    let mut native = Some(watcher.spawn(pid, &roots, census.watcher_baseline(pid)));
    let output = wait_owned_child_output_observing(child, EVENT_DEADLINE, || {
        assert_watcher_not_failed(&mut native, "native watcher regression");
    });
    assert!(
        output.status.success(),
        "fixture process failed: {output:?}"
    );
    let output = wait_owned_child_output_observing(
        native.expect("owned native watcher"),
        EVENT_DEADLINE,
        || {},
    );
    assert!(output.status.success(), "native watcher failed: {output:?}");
    let observations = String::from_utf8(output.stdout).expect("watcher output UTF-8");
    assert_eq!(
        observations,
        format!("READY\nSESSION {}\nDONE\n", new.display())
    );
}

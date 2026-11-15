use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

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

impl NativeAbsenceWatcher {
    pub(crate) fn compile(root: &Path) -> Self {
        const SOURCE: &str = r#"
import CoreGraphics
import Darwin
import Foundation

let target = Int32(CommandLine.arguments[1])!
let prefix = "kb-" + String(target, radix: 16) + "-"
let roots = [FileManager.default.temporaryDirectory.path, "/tmp", "/var/tmp"]
var windows = Set<UInt32>()
var children = Set<Int>()
var sessions = Set<String>()

func sample() {
  let rows = CGWindowListCopyWindowInfo([.excludeDesktopElements], kCGNullWindowID) as! [[String: Any]]
  for row in rows {
    let owner = (row[kCGWindowOwnerPID as String] as? NSNumber)?.int32Value
    if owner == target, let number = row[kCGWindowNumber as String] as? NSNumber {
      windows.insert(number.uint32Value)
    }
  }
  for root in roots {
    for name in (try? FileManager.default.contentsOfDirectory(atPath: root)) ?? [] where name.hasPrefix(prefix) {
      sessions.insert(root + "/" + name)
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
        let source = root.join("kel96-native-absence.swift");
        let executable = root.join("kel96-native-absence");
        fs::write(&source, SOURCE).expect("write native absence watcher");
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

    pub(crate) fn spawn(&self, pid: u32) -> Child {
        Command::new(&self.executable)
            .arg(pid.to_string())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("start native absence watcher")
    }
}

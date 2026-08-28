//! KEL-96/T1b real-macOS no-flag host/window/session acceptance.
#![cfg(target_os = "macos")]
#![allow(clippy::expect_used, clippy::panic)] // extra test crate: assertions are the oracle
#![allow(clippy::zombie_processes)] // cleanup owns host plus the enrolled Bun process group

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::os::unix::fs::{PermissionsExt, symlink};
use std::os::unix::net::{UnixListener, UnixStream};
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const TITLE: &str = "KEL96 T1b Fixture";
const MARKER: &str = "KEL96_T1B_EXACT_RENDERER_7e2d9b";
const EVENT_DEADLINE: Duration = Duration::from_secs(15);
const PROCESS_DEADLINE: Duration = Duration::from_secs(5);

#[test]
fn private_guardian_discriminator_without_authenticated_handoff_spawns_nothing() {
    let temp = tempfile::tempdir().expect("private-role fixture");
    let marker = temp.path().join("spawned");
    let entry = temp.path().join("entry.ts");
    fs::write(
        &entry,
        format!(
            "await Bun.write({}, 'spawned');\n",
            serde_json::to_string(&marker).expect("marker JSON")
        ),
    )
    .expect("private-role entry");
    let output = Command::new(env!("CARGO_BIN_EXE_keld-host"))
        .arg(keld_runtime::macos_guardian::SUPERVISED_GUARDIAN_ARG)
        .arg(temp.path())
        .arg("entry.ts")
        .arg("1")
        .arg("1")
        .env(
            "KELD_APP_LINK",
            "/tmp/forged#aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        )
        .stdin(Stdio::null())
        .output()
        .expect("invoke private discriminator without bootstrap");

    assert!(
        !output.status.success(),
        "forged private role became success"
    );
    assert!(!marker.exists(), "private argv discriminator spawned Bun");
    let stderr = String::from_utf8(output.stderr).expect("private-role stderr UTF-8");
    assert!(stderr.contains("KELD-RUNTIME-003"), "{stderr}");
    assert!(stderr.contains("registration bootstrap"), "{stderr}");
}

#[test]
fn every_invalid_boot_class_fails_before_transient_window_listener_or_bun() {
    let fixture = ProductFixture::new("invalid");
    let watcher = NativeAbsenceWatcher::compile(fixture.root.path());
    for invalid in InvalidBoot::ALL {
        let stage = fixture.stage();
        invalid.apply(stage.root(), fixture.root.path());
        assert_invalid_stage_is_resource_free(
            &stage,
            &watcher,
            &fixture
                .root
                .path()
                .join(format!("invalid-{}.sock", invalid.name())),
            invalid.name(),
        );
    }
}

#[test]
fn stalled_initial_navigation_rolls_back_window_link_and_process_group() {
    let fixture = ProductFixture::new("navigation-timeout");
    let blocker = NavigationBlocker::bind();
    fs::write(
        fixture.project.join("index.html"),
        format!(
            "<!doctype html><title>{TITLE}</title><img src=\"http://127.0.0.1:{}/never\">\n",
            blocker.port
        ),
    )
    .expect("stalled renderer");
    let stage = fixture.stage();
    let control_path = fixture.root.path().join("navigation-timeout.sock");
    let listener = UnixListener::bind(&control_path).expect("bind navigation control");
    listener
        .set_nonblocking(true)
        .expect("nonblocking navigation control");
    let child = Command::new(stage.host())
        .env("KELD_T1B_CONTROL", &control_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("launch navigation-timeout host");
    let host_pid = child.id();
    let control = accept_before(&listener, Instant::now() + EVENT_DEADLINE);
    control
        .set_read_timeout(Some(EVENT_DEADLINE))
        .expect("control deadline");
    let mut observations = BufReader::new(control);
    let hello = read_control_line(&mut observations);
    let mut fields = hello.split_whitespace();
    assert_eq!(fields.next(), Some("HELLO"), "{hello}");
    let bun_pid = parse_pid(fields.next(), &hello);
    let app_link = fields.next().expect("navigation app link");
    let session_dir = PathBuf::from(app_link.rsplit_once('#').expect("app link token").0)
        .parent()
        .expect("session directory")
        .to_path_buf();
    let descendant = read_control_line(&mut observations);
    let descendant_pid = parse_pid(descendant.split_whitespace().nth(1), &descendant);
    blocker
        .connected
        .recv_timeout(EVENT_DEADLINE)
        .expect("WKWebView requested stalled resource");
    let output = wait_child_output(child, EVENT_DEADLINE);
    blocker
        .release
        .send(())
        .expect("release blocked navigation");
    blocker.handle.join().expect("navigation blocker joins");

    assert!(!output.status.success(), "stalled navigation became Ready");
    let stderr = String::from_utf8(output.stderr).expect("navigation stderr UTF-8");
    assert!(stderr.contains("KELD-CORE-037"), "{stderr}");
    assert!(stderr.contains("initial renderer navigation"), "{stderr}");
    await_process_gone(bun_pid);
    await_process_gone(descendant_pid);
    assert!(
        !session_dir.exists(),
        "navigation rollback left app-link locator"
    );
    assert!(
        native_windows(host_pid, TITLE).is_empty(),
        "navigation rollback left native window"
    );
}

#[test]
fn no_flag_host_owns_real_window_session_death_reap_and_ordered_quit() {
    let fixture = ProductFixture::new("product");

    let mut killed = fixture.launch_cycle("host-death");
    killed.assert_live_product();
    let host_status = killed
        .host
        .as_mut()
        .expect("live host")
        .kill()
        .and_then(|()| killed.host.as_mut().expect("live host").wait())
        .expect("SIGKILL only the no-flag host");
    assert_eq!(
        host_status.signal(),
        Some(9),
        "host-only death must be SIGKILL"
    );
    killed.host.take();
    killed.expect_line("LINK_EOF");
    await_process_gone(killed.bun_pid);
    await_process_gone(killed.descendant_pid);
    await_process_gone(killed.guardian_pid);
    assert!(
        !killed.session_dir.exists(),
        "host death left the app-link locator"
    );
    killed.group_gone = true;

    let mut self_terminated = fixture.launch_cycle("self-termination");
    self_terminated.assert_live_product();
    self_terminated
        .control_writer
        .write_all(b"EXIT0\n")
        .expect("request unrequested status-zero Bun exit");
    let output = self_terminated.wait_host();
    assert!(
        !output.status.success(),
        "unrequested status-zero Bun exit became host success"
    );
    let stderr = String::from_utf8(output.stderr).expect("self-termination stderr UTF-8");
    assert!(stderr.contains("KELD-CORE-033"), "{stderr}");
    assert!(stderr.contains("KELD-RUNTIME-012"), "{stderr}");
    await_process_gone(self_terminated.bun_pid);
    await_process_gone(self_terminated.descendant_pid);
    await_process_gone(self_terminated.guardian_pid);
    assert!(
        !self_terminated.session_dir.exists(),
        "self-termination left the app-link locator"
    );
    self_terminated.group_gone = true;

    let mut guardian_failed = fixture.launch_cycle("guardian-failure");
    guardian_failed.assert_live_product();
    kill_pid(guardian_failed.guardian_pid);
    let output = guardian_failed.wait_host();
    assert!(
        !output.status.success(),
        "guardian death became host success"
    );
    let stderr = String::from_utf8(output.stderr).expect("guardian-failure stderr UTF-8");
    assert!(stderr.contains("KELD-CORE-033"), "{stderr}");
    assert!(stderr.contains("KELD-RUNTIME-013"), "{stderr}");
    await_process_gone(guardian_failed.bun_pid);
    await_process_gone(guardian_failed.descendant_pid);
    await_process_gone(guardian_failed.guardian_pid);
    assert!(
        native_windows(guardian_failed.host_pid, TITLE).is_empty(),
        "guardian failure left a native window"
    );
    guardian_failed.group_gone = true;

    let mut orderly = fixture.launch_cycle("relaunch-orderly");
    orderly.assert_live_product();
    orderly
        .control_writer
        .write_all(b"QUIT\n")
        .expect("request fixture app.quit");
    orderly.expect_line("QUIT_REPLY");
    orderly.expect_line("LINK_EOF");
    let output = orderly.wait_host();
    assert!(
        output.status.success(),
        "ordered no-flag host exit: {output:?}"
    );
    let stderr = String::from_utf8(output.stderr).expect("host stderr UTF-8");
    assert!(
        !stderr.contains("pre-alpha"),
        "no-flag product launch returned through the old banner: {stderr}"
    );
    await_process_gone(orderly.bun_pid);
    await_process_gone(orderly.descendant_pid);
    await_process_gone(orderly.guardian_pid);
    assert!(
        !orderly.session_dir.exists(),
        "Quit left the app-link locator"
    );
    assert!(
        native_windows(orderly.host_pid, TITLE).is_empty(),
        "host exit left a native window"
    );
    orderly.group_gone = true;
}

struct ProductFixture {
    root: tempfile::TempDir,
    project: PathBuf,
    link_source: String,
    harness: &'static str,
}

impl ProductFixture {
    fn new(name: &str) -> Self {
        let root = tempfile::tempdir().expect("fixture root");
        let project = root.path().join(name);
        fs::create_dir_all(project.join("src")).expect("fixture source directory");
        let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("keld-host crate beneath workspace");
        let link_source = fs::read_to_string(repo.join("packages/@keld/electron/src/link.ts"))
            .expect("reuse canonical KEL-72 TypeScript link owner");
        Self {
            root,
            project,
            link_source,
            harness: include_str!("fixtures/t1b_harness.ts"),
        }
    }

    fn stage(&self) -> keld_cli::boot::DevBootStage {
        let mut entry = self.link_source.clone();
        entry.push_str(self.harness);
        fs::write(self.project.join("src/main.ts"), entry).expect("fixture entry");
        if !self.project.join("index.html").exists() {
            fs::write(
                self.project.join("index.html"),
                format!("<!doctype html><title>{TITLE}</title><p id=marker>{MARKER}</p>\n"),
            )
            .expect("fallback renderer");
        }
        fs::write(
            self.project.join("keld.config.ts"),
            format!(
                "export default {{\n  name: \"{TITLE}\",\n  entry: \"src/main.ts\",\n  renderer: \"index.html\",\n}} as const;\n"
            ),
        )
        .expect("fixture config");
        keld_cli::boot::stage_dev_boot(&self.project, Path::new(env!("CARGO_BIN_EXE_keld-host")))
            .expect("compile owner-private no-flag stage")
    }

    fn launch_cycle(&self, cycle: &str) -> LiveCycle {
        let beacon = Beacon::bind(MARKER);
        fs::write(
            self.project.join("index.html"),
            format!(
                "<!doctype html><title>{TITLE}</title><p id=marker>{MARKER}</p><img src=\"http://127.0.0.1:{}/{MARKER}\">\n",
                beacon.port()
            ),
        )
        .expect("renderer with exact beacon");
        let stage = self.stage();
        let control_path = self.root.path().join(format!("{cycle}.sock"));
        let listener = UnixListener::bind(&control_path).expect("bind fixture control");
        listener
            .set_nonblocking(true)
            .expect("nonblocking fixture control");
        let substitution_cwd = self.root.path().join("substitution-cwd");
        fs::create_dir_all(&substitution_cwd).expect("substitution cwd");
        fs::write(
            substitution_cwd.join("keld.boot.json"),
            b"environment and cwd must not select this descriptor",
        )
        .expect("substitution descriptor");
        let child = Command::new(stage.host())
            .current_dir(&substitution_cwd)
            .env("KELD_T1B_CONTROL", &control_path)
            .env("KELD_BOOT_PATH", substitution_cwd.join("keld.boot.json"))
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("launch staged no-flag host");
        let host_pid = child.id();
        let control = accept_before(&listener, Instant::now() + EVENT_DEADLINE);
        control
            .set_read_timeout(Some(EVENT_DEADLINE))
            .expect("control read deadline");
        let control_reader = BufReader::new(control.try_clone().expect("control reader clone"));
        let mut cycle = LiveCycle {
            host: Some(child),
            host_pid,
            guardian_pid: 0,
            bun_pid: 0,
            descendant_pid: 0,
            session_dir: PathBuf::new(),
            control_reader,
            control_writer: control,
            beacon: Some(beacon),
            group_gone: false,
        };
        let hello = cycle.next_line();
        let mut fields = hello.split_whitespace();
        assert_eq!(fields.next(), Some("HELLO"), "{hello}");
        cycle.bun_pid = parse_pid(fields.next(), &hello);
        let app_link = fields
            .next()
            .unwrap_or_else(|| panic!("missing app link: {hello}"));
        let endpoint = PathBuf::from(
            app_link
                .rsplit_once('#')
                .unwrap_or_else(|| panic!("invalid app link: {hello}"))
                .0,
        );
        cycle.session_dir = endpoint.parent().expect("session directory").to_path_buf();
        cycle.guardian_pid = parent_process(cycle.bun_pid);
        let descendant = cycle.next_line();
        let mut descendant_fields = descendant.split_whitespace();
        assert_eq!(descendant_fields.next(), Some("DESCENDANT"), "{descendant}");
        cycle.descendant_pid = parse_pid(descendant_fields.next(), &descendant);
        cycle.expect_line("READY");
        cycle.expect_line("ECHO1");
        cycle.expect_line("ECHO2");
        cycle.beacon.take().expect("beacon owner").assert_exact();
        cycle
    }
}

#[derive(Clone, Copy)]
enum InvalidBoot {
    MissingBoot,
    UnreadableBoot,
    DirectoryBoot,
    SymlinkBoot,
    Malformed,
    Duplicate,
    Unknown,
    Version,
    NonUtf8,
    Oversize,
    UnsafePath,
    BadDigest,
    MissingEntry,
    DirectoryEntry,
    SymlinkEntry,
    UnreadableEntry,
    MissingRenderer,
    DirectoryRenderer,
    SymlinkRenderer,
    UnreadableRenderer,
    MissingPermissions,
    DirectoryPermissions,
    SymlinkPermissions,
    UnreadablePermissions,
    WrongRootMode,
}

impl InvalidBoot {
    const ALL: [Self; 25] = [
        Self::MissingBoot,
        Self::UnreadableBoot,
        Self::DirectoryBoot,
        Self::SymlinkBoot,
        Self::Malformed,
        Self::Duplicate,
        Self::Unknown,
        Self::Version,
        Self::NonUtf8,
        Self::Oversize,
        Self::UnsafePath,
        Self::BadDigest,
        Self::MissingEntry,
        Self::DirectoryEntry,
        Self::SymlinkEntry,
        Self::UnreadableEntry,
        Self::MissingRenderer,
        Self::DirectoryRenderer,
        Self::SymlinkRenderer,
        Self::UnreadableRenderer,
        Self::MissingPermissions,
        Self::DirectoryPermissions,
        Self::SymlinkPermissions,
        Self::UnreadablePermissions,
        Self::WrongRootMode,
    ];

    const fn name(self) -> &'static str {
        match self {
            Self::MissingBoot => "missing-boot",
            Self::UnreadableBoot => "unreadable-boot",
            Self::DirectoryBoot => "directory-boot",
            Self::SymlinkBoot => "symlink-boot",
            Self::Malformed => "malformed-json",
            Self::Duplicate => "duplicate-field",
            Self::Unknown => "unknown-field",
            Self::Version => "unknown-version",
            Self::NonUtf8 => "non-utf8",
            Self::Oversize => "oversize",
            Self::UnsafePath => "unsafe-path",
            Self::BadDigest => "bad-digest",
            Self::MissingEntry => "missing-entry",
            Self::DirectoryEntry => "directory-entry",
            Self::SymlinkEntry => "symlink-entry",
            Self::UnreadableEntry => "unreadable-entry",
            Self::MissingRenderer => "missing-renderer",
            Self::DirectoryRenderer => "directory-renderer",
            Self::SymlinkRenderer => "symlink-renderer",
            Self::UnreadableRenderer => "unreadable-renderer",
            Self::MissingPermissions => "missing-permissions",
            Self::DirectoryPermissions => "directory-permissions",
            Self::SymlinkPermissions => "symlink-permissions",
            Self::UnreadablePermissions => "unreadable-permissions",
            Self::WrongRootMode => "wrong-root-mode",
        }
    }

    fn apply(self, root: &Path, fixture_root: &Path) {
        let boot = root.join("keld.boot.json");
        let entry = root.join("src/main.ts");
        let renderer = root.join("index.html");
        let permissions = root.join("keld.permissions.jsonc");
        match self {
            Self::MissingBoot => fs::remove_file(boot).expect("remove boot"),
            Self::UnreadableBoot => unreadable(&boot),
            Self::DirectoryBoot => replace_with_directory(&boot),
            Self::SymlinkBoot => replace_with_symlink(&boot, fixture_root, "outside-boot"),
            Self::Malformed => replace_boot(&boot, b"{not schema v1}"),
            Self::Duplicate => replace_boot(
                &boot,
                br#"{"schema":1,"schema":1,"name":"x","entry":"src/main.ts","renderer":"index.html","permissions":{"file":"keld.permissions.jsonc","content_sha256":"sha256:ca3d163bab055381827226140568f3bef7eaac187cebd76878e0b63e9e442356"}}"#,
            ),
            Self::Unknown => mutate_boot(&boot, |document| document["unknown"] = 1.into()),
            Self::Version => mutate_boot(&boot, |document| document["schema"] = 2.into()),
            Self::NonUtf8 => replace_boot(&boot, &[0xff]),
            Self::Oversize => replace_boot(&boot, &vec![b' '; 64 * 1024 + 1]),
            Self::UnsafePath => {
                mutate_boot(&boot, |document| document["entry"] = "../escape.ts".into());
            }
            Self::BadDigest => mutate_boot(&boot, |document| {
                document["permissions"]["content_sha256"] = "SHA256:BAD".into();
            }),
            Self::MissingEntry => fs::remove_file(entry).expect("remove entry"),
            Self::DirectoryEntry => replace_with_directory(&entry),
            Self::SymlinkEntry => replace_with_symlink(&entry, fixture_root, "outside-entry"),
            Self::UnreadableEntry => unreadable(&entry),
            Self::MissingRenderer => fs::remove_file(renderer).expect("remove renderer"),
            Self::DirectoryRenderer => replace_with_directory(&renderer),
            Self::SymlinkRenderer => {
                replace_with_symlink(&renderer, fixture_root, "outside-renderer");
            }
            Self::UnreadableRenderer => unreadable(&renderer),
            Self::MissingPermissions => {
                fs::remove_file(permissions).expect("remove permissions");
            }
            Self::DirectoryPermissions => replace_with_directory(&permissions),
            Self::SymlinkPermissions => {
                replace_with_symlink(&permissions, fixture_root, "outside-permissions");
            }
            Self::UnreadablePermissions => unreadable(&permissions),
            Self::WrongRootMode => {
                fs::set_permissions(root, fs::Permissions::from_mode(0o755))
                    .expect("set invalid root mode");
            }
        }
    }
}

fn replace_boot(path: &Path, bytes: &[u8]) {
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).expect("make boot writable");
    fs::write(path, bytes).expect("replace boot bytes");
}

fn mutate_boot(path: &Path, mutate: impl FnOnce(&mut serde_json::Value)) {
    let mut document: serde_json::Value =
        serde_json::from_slice(&fs::read(path).expect("read boot")).expect("parse staged boot");
    mutate(&mut document);
    replace_boot(
        path,
        &serde_json::to_vec(&document).expect("serialize mutated boot"),
    );
}

fn unreadable(path: &Path) {
    fs::set_permissions(path, fs::Permissions::from_mode(0o000)).expect("make target unreadable");
}

fn replace_with_directory(path: &Path) {
    fs::remove_file(path).expect("remove file before directory substitution");
    fs::create_dir(path).expect("create directory substitution");
}

fn replace_with_symlink(path: &Path, fixture_root: &Path, name: &str) {
    let outside = fixture_root.join(name);
    fs::write(&outside, b"outside substitution").expect("outside substitution target");
    fs::remove_file(path).expect("remove file before symlink substitution");
    symlink(outside, path).expect("create symlink substitution");
}

struct NativeAbsenceWatcher {
    executable: PathBuf,
}

impl NativeAbsenceWatcher {
    fn compile(root: &Path) -> Self {
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
  let rows = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as! [[String: Any]]
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

    fn spawn(&self, pid: u32) -> Child {
        Command::new(&self.executable)
            .arg(pid.to_string())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("start native absence watcher")
    }
}

fn assert_invalid_stage_is_resource_free(
    stage: &keld_cli::boot::DevBootStage,
    watcher: &NativeAbsenceWatcher,
    control_path: &Path,
    case: &str,
) {
    let listener = UnixListener::bind(control_path).expect("bind invalid control observer");
    listener
        .set_nonblocking(true)
        .expect("nonblocking invalid control observer");
    let child = Command::new("/bin/sh")
        .args(["-c", "kill -STOP $$; exec \"$1\"", "kel96-invalid"])
        .arg(stage.host())
        .env("KELD_T1B_CONTROL", control_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start suspended invalid host");
    let host_pid = child.id();
    await_process_state(host_pid, 'T');
    let native = watcher.spawn(host_pid);
    let output = wait_child_output(child, EVENT_DEADLINE);
    assert!(
        !output.status.success(),
        "{case}: invalid boot became success"
    );
    let native_output = native
        .wait_with_output()
        .expect("wait native absence watcher");
    assert!(
        native_output.status.success(),
        "{case}: watcher failed: {native_output:?}"
    );
    let observations = String::from_utf8(native_output.stdout).expect("watcher output UTF-8");
    assert_eq!(
        observations, "READY\nDONE\n",
        "{case}: transient resource: {observations}"
    );
    let stderr = String::from_utf8(output.stderr).expect("typed stderr UTF-8");
    assert!(
        stderr.contains("KELD-CORE-035") || stderr.contains("KELD-CORE-036"),
        "{case}: {stderr}"
    );
    assert!(
        stderr.contains("[startup-resource-attempts listener=0 child=0 window=0]"),
        "{case}: internal pre-resource ledger was not empty: {stderr}"
    );
    let lower = stderr.to_ascii_lowercase();
    assert!(
        ["regenerate", "restore", "write", "set", "launch"]
            .iter()
            .any(|action| lower.contains(action)),
        "{case}: missing fix: {stderr}"
    );
    assert!(
        matches!(listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock),
        "{case}: Bun reached the control observer"
    );
    assert!(
        session_dirs_for(host_pid).is_empty(),
        "{case}: app-link directory remains"
    );
    let _ = fs::remove_file(control_path);
}

fn await_process_state(pid: u32, wanted: char) {
    let deadline = Instant::now() + PROCESS_DEADLINE;
    loop {
        let output = Command::new("/bin/ps")
            .args(["-o", "state=", "-p", &pid.to_string()])
            .output()
            .expect("inspect process state");
        if String::from_utf8(output.stdout)
            .expect("process state UTF-8")
            .trim()
            .starts_with(wanted)
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "process {pid} never reached state {wanted}"
        );
        thread::yield_now();
    }
}

struct LiveCycle {
    host: Option<Child>,
    host_pid: u32,
    guardian_pid: u32,
    bun_pid: u32,
    descendant_pid: u32,
    session_dir: PathBuf,
    control_reader: BufReader<UnixStream>,
    control_writer: UnixStream,
    beacon: Option<Beacon>,
    group_gone: bool,
}

impl LiveCycle {
    fn assert_live_product(&self) {
        assert_ne!(self.host_pid, self.guardian_pid);
        assert_ne!(self.guardian_pid, self.bun_pid);
        assert_eq!(parent_process(self.guardian_pid), self.host_pid);
        assert_eq!(parent_process(self.bun_pid), self.guardian_pid);
        assert_eq!(process_group(self.bun_pid), self.bun_pid);
        assert_eq!(process_group(self.descendant_pid), self.bun_pid);
        let windows = native_windows(self.host_pid, TITLE);
        assert_eq!(
            windows.len(),
            1,
            "exact host-owned native window: {windows:?}"
        );
        assert!(
            host_unix_sockets(self.host_pid) > 0,
            "host owns no authenticated Unix app-link descriptor"
        );
        assert!(
            host_unix_sockets(self.bun_pid) > 0,
            "Bun owns no authenticated Unix app-link descriptor"
        );
        assert!(
            !self.session_dir.exists(),
            "authenticated one-use app-link locator must already be revoked"
        );
        eprintln!(
            "KEL96_T1B_EVIDENCE host={} window={} guardian={} bun={} descendant={} pgid={} link_dir={} marker={}",
            self.host_pid,
            windows[0],
            self.guardian_pid,
            self.bun_pid,
            self.descendant_pid,
            process_group(self.bun_pid),
            self.session_dir.display(),
            MARKER
        );
    }

    fn next_line(&mut self) -> String {
        let mut line = String::new();
        let read = self
            .control_reader
            .read_line(&mut line)
            .expect("read fixture observation");
        assert_ne!(read, 0, "fixture control reached EOF before expected event");
        let line = line.trim_end().to_owned();
        assert!(!line.starts_with("ERROR "), "fixture error: {line}");
        line
    }

    fn expect_line(&mut self, expected: &str) {
        assert_eq!(self.next_line(), expected);
    }

    fn wait_host(&mut self) -> Output {
        let mut child = self.host.take().expect("live host");
        let deadline = Instant::now() + EVENT_DEADLINE;
        loop {
            if child.try_wait().expect("inspect no-flag host").is_some() {
                return child
                    .wait_with_output()
                    .expect("collect no-flag host output");
            }
            assert!(
                Instant::now() < deadline,
                "no-flag host did not exit after Quit"
            );
            thread::yield_now();
        }
    }
}

impl Drop for LiveCycle {
    fn drop(&mut self) {
        if let Some(host) = self.host.as_mut() {
            let _ = host.kill();
            let _ = host.wait();
        }
        if !self.group_gone && self.bun_pid != 0 {
            let _ = Command::new("/bin/kill")
                .args(["-KILL", &format!("-{}", self.bun_pid)])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
    }
}

struct NavigationBlocker {
    port: u16,
    connected: Receiver<()>,
    release: mpsc::Sender<()>,
    handle: JoinHandle<()>,
}

impl NavigationBlocker {
    fn bind() -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind navigation blocker");
        let port = listener.local_addr().expect("blocker address").port();
        let (connected_tx, connected) = mpsc::channel();
        let (release, release_rx) = mpsc::channel();
        let handle = thread::spawn(move || {
            let (_stream, _) = listener.accept().expect("accept blocked navigation");
            connected_tx.send(()).expect("report blocked navigation");
            release_rx.recv().expect("release blocked navigation");
        });
        Self {
            port,
            connected,
            release,
            handle,
        }
    }
}

fn kill_pid(pid: u32) {
    let status = Command::new("/bin/kill")
        .args(["-KILL", &pid.to_string()])
        .status()
        .expect("kill one process");
    assert!(status.success(), "kill {pid}: {status:?}");
}

struct Beacon {
    port: u16,
    request: Receiver<Vec<u8>>,
    handle: Option<JoinHandle<()>>,
}

impl Beacon {
    fn bind(marker: &'static str) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind renderer beacon");
        let port = listener.local_addr().expect("beacon address").port();
        let (request_tx, request) = mpsc::channel();
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept renderer beacon");
            stream
                .set_read_timeout(Some(EVENT_DEADLINE))
                .expect("beacon read deadline");
            let mut bytes = Vec::new();
            let mut chunk = [0_u8; 1024];
            while !bytes.windows(4).any(|window| window == b"\r\n\r\n") {
                let read = stream.read(&mut chunk).expect("read renderer beacon");
                if read == 0 {
                    request_tx
                        .send(bytes)
                        .expect("report closed renderer beacon");
                    return;
                }
                bytes.extend_from_slice(&chunk[..read]);
            }
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                .expect("respond renderer beacon");
            request_tx.send(bytes).expect("report renderer request");
            assert!(!marker.is_empty());
        });
        Self {
            port,
            request,
            handle: Some(handle),
        }
    }

    const fn port(&self) -> u16 {
        self.port
    }

    fn assert_exact(mut self) {
        let request = self
            .request
            .recv_timeout(EVENT_DEADLINE)
            .expect("WKWebView did not render the exact fixture beacon");
        let request = String::from_utf8_lossy(&request);
        assert!(request.starts_with(&format!("GET /{MARKER} ")), "{request}");
        self.handle
            .take()
            .expect("beacon thread")
            .join()
            .expect("beacon thread joins");
    }
}

impl Drop for Beacon {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            let _ = TcpStream::connect(("127.0.0.1", self.port));
            let _ = handle.join();
        }
    }
}

fn accept_before(listener: &UnixListener, deadline: Instant) -> UnixStream {
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                stream
                    .set_nonblocking(false)
                    .expect("normalize accepted fixture control");
                return stream;
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(
                    Instant::now() < deadline,
                    "Bun did not connect the fixture control"
                );
                thread::yield_now();
            }
            Err(error) => panic!("accept fixture control: {error}"),
        }
    }
}

fn read_control_line(reader: &mut BufReader<UnixStream>) -> String {
    let mut line = String::new();
    let read = reader.read_line(&mut line).expect("read control line");
    assert_ne!(read, 0, "control EOF before observation");
    line.trim_end().to_owned()
}

fn wait_child_output(mut child: Child, timeout: Duration) -> Output {
    let deadline = Instant::now() + timeout;
    loop {
        if child.try_wait().expect("inspect child exit").is_some() {
            return child.wait_with_output().expect("collect child output");
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let output = child.wait_with_output().expect("collect timed-out child");
            panic!("child exceeded exit deadline: {output:?}");
        }
        thread::yield_now();
    }
}

fn parse_pid(field: Option<&str>, line: &str) -> u32 {
    field
        .unwrap_or_else(|| panic!("missing pid: {line}"))
        .parse()
        .unwrap_or_else(|error| panic!("invalid pid ({error}): {line}"))
}

fn parent_process(pid: u32) -> u32 {
    process_number(pid, "ppid")
}

fn process_group(pid: u32) -> u32 {
    process_number(pid, "pgid")
}

fn process_number(pid: u32, field: &str) -> u32 {
    let output = Command::new("/bin/ps")
        .args(["-o", &format!("{field}="), "-p", &pid.to_string()])
        .output()
        .expect("inspect process relation");
    assert!(output.status.success(), "ps {field} for {pid}: {output:?}");
    String::from_utf8(output.stdout)
        .expect("ps output UTF-8")
        .trim()
        .parse()
        .expect("ps relation is numeric")
}

fn process_exists(pid: u32) -> bool {
    Command::new("/bin/kill")
        .args(["-0", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn await_process_gone(pid: u32) {
    let deadline = Instant::now() + PROCESS_DEADLINE;
    while process_exists(pid) {
        assert!(Instant::now() < deadline, "process {pid} survived cleanup");
        thread::yield_now();
    }
}

fn host_unix_sockets(pid: u32) -> usize {
    let output = Command::new("/usr/sbin/lsof")
        .args(["-n", "-P", "-a", "-p", &pid.to_string(), "-U", "-Fn"])
        .output()
        .expect("enumerate Unix descriptors");
    assert!(
        output.status.success(),
        "lsof Unix descriptors for {pid}: {output:?}"
    );
    String::from_utf8(output.stdout)
        .expect("lsof output UTF-8")
        .lines()
        .filter(|line| line.starts_with('n'))
        .count()
}

fn native_windows(pid: u32, title: &str) -> Vec<u32> {
    const SCRIPT: &str = r"
import CoreGraphics
import Foundation
let wantedPID = Int(CommandLine.arguments[1])!
let wantedTitle = CommandLine.arguments[2]
let rows = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as! [[String: Any]]
for row in rows {
  let owner = (row[kCGWindowOwnerPID as String] as? NSNumber)?.intValue
  let name = row[kCGWindowName as String] as? String
  let layer = (row[kCGWindowLayer as String] as? NSNumber)?.intValue
  if owner == wantedPID && name == wantedTitle && layer == 0 {
    print((row[kCGWindowNumber as String] as! NSNumber).uint32Value)
  }
}
";
    let output = Command::new("/usr/bin/xcrun")
        .args(["swift", "-e", SCRIPT, &pid.to_string(), title])
        .output()
        .expect("run native CoreGraphics census");
    assert!(output.status.success(), "CoreGraphics census: {output:?}");
    String::from_utf8(output.stdout)
        .expect("CoreGraphics output UTF-8")
        .lines()
        .map(|line| line.parse().expect("CGWindowID is numeric"))
        .collect()
}

fn session_dirs_for(pid: u32) -> Vec<PathBuf> {
    let prefix = format!("kb-{pid:x}-");
    [
        std::env::temp_dir(),
        PathBuf::from("/tmp"),
        PathBuf::from("/var/tmp"),
    ]
    .into_iter()
    .flat_map(|base| {
        fs::read_dir(base)
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
    })
    .filter(|entry| entry.file_name().to_string_lossy().starts_with(&prefix))
    .map(|entry| entry.path())
    .collect()
}

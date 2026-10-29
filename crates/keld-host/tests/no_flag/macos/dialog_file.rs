//! Real macOS Host file Open/Save sheet and actual OS Cancel acceptance.
//! This is not a mock, an implicit FS grant or installed Draw.io product proof.

use crate::support::EVENT_DEADLINE;
use crate::support::control::{accept_before, parse_pid, read_control_line, wait_child_output};
use crate::support::dev_cycle::ShippingLaunchCleanup;
use crate::support::native_window::native_window_rows;
use crate::support::process::{await_process_gone, process_group};
use crate::support::product::ProductFixture;
use std::fs;
use std::io::{BufReader, Write as _};
use std::os::unix::net::{UnixListener, UnixStream};
use std::os::unix::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::Instant;

#[path = "dialog_file_selection.rs"]
mod selection;

#[cfg(debug_assertions)]
#[path = "dialog_startup.rs"]
mod startup;

const FILE_ROLE: &str = r#"
import { createConnection } from "node:net";
import { createInterface } from "node:readline";
const socket = process.env.KELD_T1B_CONTROL;
if (!socket) throw new Error("no file dialog control socket");
const control = createConnection({ path: socket });
await new Promise<void>((resolve, reject) => {
  control.once("connect", resolve);
  control.once("error", reject);
});
function report(s: string): void { control.write(`${s}\n`); }
report(`ROLE_BOUND ${process.pid} ${process.ppid}`);
app.on("window-all-closed", () => {});
await app.whenReady();
const win = new BrowserWindow({ width: 640, height: 480 });
report(`CREATED ${process.pid} ${win.id}`);
const filters = [
  { name: "draw.io", extensions: ["drawio", "xml"] },
  { name: "VSDX", extensions: ["vsdx"] },
  { name: "Mermaid", extensions: ["mmd", "mermaid"] },
  { name: "All Files", extensions: ["*"] },
];
for await (const command of createInterface({ input: control })) {
  if (command === "OPEN_CANCEL") {
    const pending = dialog.showOpenDialog(win, {
      filters, properties: ["openFile", "openDirectory", "multiSelections"],
    });
    void pending.then((value) => report(`OPEN_RESULT ${value.canceled} ${value.filePaths.length}`),
      (error) => report(`OPEN_ERROR ${error?.code ?? String(error)}`));
    report("SUBMITTED");
  } else if (command === "SAVE_CANCEL") {
    const pending = dialog.showSaveDialog(win, {
      defaultPath: "diagram.drawio", filters, securityScopedBookmarks: false,
    });
    void pending.then((value) => report(`SAVE_RESULT ${value.canceled} ${value.filePath === ""}`),
      (error) => report(`SAVE_ERROR ${error?.code ?? String(error)}`));
    report("SUBMITTED");
  } else if (command === "SAVE_SELECT") {
    const pending = dialog.showSaveDialog(win, {
      defaultPath: process.env.KELD_T1B_SAVE_DEST!, filters,
      securityScopedBookmarks: false,
    });
    void pending.then((value) => report(`SAVE_SELECTED ${value.canceled} ${value.filePath}`),
      (error) => report(`SAVE_ERROR ${error?.code ?? String(error)}`));
    report("SUBMITTED");
  } else if (command === "OPEN_SELECT") {
    const pending = dialog.showOpenDialog(win, {
      defaultPath: process.env.KELD_T1B_OPEN_PATH!,
      filters,
      properties: ["openFile"],
    });
    void pending.then((value) => report(`OPEN_SELECTED ${value.canceled} ${value.filePaths.join("|")}`),
      (error) => report(`OPEN_ERROR ${error?.code ?? String(error)}`));
    report("SUBMITTED");
  } else if (command === "MESSAGE_NEXT") {
    const pending = dialog.showMessageBox(win, {
      message: "Healthy next", type: "question", buttons: ["Save", "Cancel"],
      defaultId: 0, cancelId: 1,
    });
    void pending.then((value) => report(`MESSAGE_RESULT ${value.response}`),
      (error) => report(`MESSAGE_ERROR ${error?.code ?? String(error)}`));
    report("SUBMITTED");
  } else if (command === "FINISH") {
    win.destroy();
    await app.quit();
    report("QUIT_REPLY");
    break;
  } else {
    throw new Error(`unknown file dialog command ${command}`);
  }
}
control.end();
"#;

fn stage(fixture: &ProductFixture) -> keld_cli::boot::DevBootStage {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root");
    let electron = serde_json::to_string(
        &repo
            .join("packages/@keld/electron/src/index.ts")
            .to_string_lossy(),
    )
    .expect("canonical Electron source path");
    let source = format!("import {{ app, BrowserWindow, dialog }} from {electron};\n{FILE_ROLE}");
    crate::renderer_bridge::bundle_public_api_entry(
        &fixture.project,
        repo,
        "gh478-file-native",
        &source,
    );
    fs::write(fixture.project.join("keld.config.ts"),
        "export default { name: \"GH478 file\", entry: \"src/main.ts\", initialWindow: \"none\" } as const;\n",
    ).expect("real Host file fixture configuration");
    fs::write(
        fixture.project.join("keld.permissions.jsonc"),
        r#"{"app":{"system":["dialog"]}}"#,
    )
    .expect("verified exact permission to present native UI only");
    keld_cli::boot::stage_dev_boot(&fixture.project, Path::new(env!("CARGO_BIN_EXE_keld-host")))
        .expect("real no-flag Host file dialog stage")
}

fn visible_windows(pid: u32) -> Vec<serde_json::Value> {
    let rows = native_window_rows(pid);
    let document: serde_json::Value =
        serde_json::from_str(&rows).expect("actual CoreGraphics JSON");
    assert_eq!(document["capture_preflight"], true, "{rows}");
    assert_eq!(document["pid"], pid, "observed exact Host process");
    document["rows"]
        .as_array()
        .expect("OS window rows")
        .iter()
        .filter(|row| row["on_screen"] == true && row["layer"] == 0)
        .cloned()
        .collect()
}

struct FilePaths {
    _accessible_directory: Option<tempfile::TempDir>,
    directory: PathBuf,
    source: PathBuf,
    destination: PathBuf,
}

impl FilePaths {
    fn new(fixture: &ProductFixture, directory_hint: bool) -> Self {
        // The system file-panel process must enumerate this owned directory.
        // Retain TempDir until the Host exits; never modify a shared directory.
        let accessible_directory =
            std::env::var_os("KELD_TEST_ACCESSIBLE_OPEN_PARENT").map(|parent| {
                tempfile::Builder::new()
                    .prefix("keld-gh478-visible-open-")
                    .tempdir_in(parent)
                    .expect("create owned OS-visible native file selection fixture")
            });
        let directory = accessible_directory.as_ref().map_or_else(
            || fixture.root.path().join("keld-gh478-visible-open-files"),
            |dir| dir.path().to_path_buf(),
        );
        if accessible_directory.is_none() {
            fs::create_dir(&directory).expect("fixture-owned visible directory");
        }
        let source = directory.join("gh478-fixture.drawio");
        fs::write(
            &source,
            b"<mxfile><diagram>GH478 positive</diagram></mxfile>",
        )
        .expect("one fixture-local real Open source");
        let destination = if directory_hint {
            directory.join("gh478-chosen.drawio")
        } else {
            fixture.root.path().join("gh478-chosen.drawio")
        };
        assert!(!destination.exists(), "Save is path-only, not a write");
        Self {
            _accessible_directory: accessible_directory,
            directory,
            source,
            destination,
        }
    }
}

fn compile_ax(fixture: &ProductFixture, action: &str) -> PathBuf {
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("tests/fixtures/macos_dialog_ax_{action}.swift"));
    let executable = fixture.root.path().join(format!("ax-{action}"));
    let output = Command::new("/usr/bin/swiftc")
        .arg(source)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("compile independent exact-Host native observer");
    assert!(
        output.status.success(),
        "{action} compilation failed: {output:?}"
    );
    executable
}

fn run_ax(command: &mut Command) -> Output {
    let child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("launch exact-Host native observer");
    wait_child_output(child, EVENT_DEADLINE)
}

struct NativeDialog {
    host_pid: u32,
    baseline: Vec<serde_json::Value>,
    reader: BufReader<UnixStream>,
    writer: UnixStream,
    button_helper: PathBuf,
}

impl NativeDialog {
    fn submit(&mut self, operation: &str) {
        writeln!(self.writer, "{operation}").expect("one authenticated user-directed request");
        assert_eq!(read_control_line(&mut self.reader), "SUBMITTED");
    }

    fn await_detached(&self, operation: &str) {
        let deadline = Instant::now() + EVENT_DEADLINE;
        while visible_windows(self.host_pid) != self.baseline {
            assert!(
                Instant::now() < deadline,
                "{operation} left a native sheet behind"
            );
            std::thread::yield_now();
        }
    }

    fn press(&mut self, operation: &str, expected: &str, button: &str) {
        self.submit(operation);
        let activation = run_ax(
            Command::new(&self.button_helper)
                .arg(self.host_pid.to_string())
                .arg(button),
        );
        assert!(
            activation.status.success(),
            "{operation} native click failed: {activation:?}"
        );
        let result = read_control_line(&mut self.reader);
        assert_eq!(result, expected, "{operation}: {result}");
        self.await_detached(operation);
    }
}

fn run_real_host_files(
    include_positive_selection: bool,
    cancel_cycles: usize,
    directory_hint: bool,
) {
    assert!(
        cancel_cycles >= 2,
        "native repeated proof must cover Open and Save"
    );
    let fixture = ProductFixture::new("gh478-real-open-save");
    let paths = FilePaths::new(&fixture, directory_hint);
    let staged = stage(&fixture);
    let socket = fixture.root.path().join("native-file.sock");
    let listener = UnixListener::bind(&socket).expect("owned single control listener");
    listener.set_nonblocking(true).expect("bounded accept");
    let child = Command::new(staged.host())
        .process_group(0)
        .current_dir(staged.root())
        .env("KELD_T1B_CONTROL", &socket)
        .env(
            "KELD_T1B_SAVE_DEST",
            if directory_hint {
                &paths.directory
            } else {
                &paths.destination
            },
        )
        .env(
            "KELD_T1B_OPEN_PATH",
            if directory_hint {
                &paths.directory
            } else {
                &paths.source
            },
        )
        .env("KELD_DEV_LEASE", "stdin-v1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("actual shipping macOS Host");
    let mut cleanup = ShippingLaunchCleanup::new(child);
    let host = cleanup.cli.as_mut().expect("Host owner");
    let host_pid = host.id();
    let _lease = host.stdin.take().expect("retained genuine Host lease");
    let connection = accept_before(&listener, Instant::now() + EVENT_DEADLINE);
    connection
        .set_read_timeout(Some(EVENT_DEADLINE))
        .expect("bounded socket read");
    let mut reader = BufReader::new(connection.try_clone().expect("reader clone"));
    let bound = read_control_line(&mut reader);
    assert!(bound.starts_with("ROLE_BOUND "), "{bound}");
    let bun_pid = parse_pid(bound.split_whitespace().nth(1), &bound);
    cleanup.record_authenticated_groups(process_group(host_pid), process_group(bun_pid));
    let created = read_control_line(&mut reader);
    assert!(
        created.starts_with(&format!("CREATED {bun_pid} 1")),
        "{created}"
    );
    let baseline = visible_windows(host_pid);
    assert_eq!(baseline.len(), 1, "one real Host-owned parent");
    let mut dialog = NativeDialog {
        host_pid,
        baseline,
        reader,
        writer: connection,
        button_helper: compile_ax(&fixture, "press"),
    };
    for index in 0..=cancel_cycles {
        // Exactly cancel_cycles physical panels, then a healthy Message on the
        // same authenticated Bun role and WorkerLink.
        let (operation, expected, button) = if index == cancel_cycles {
            ("MESSAGE_NEXT", "MESSAGE_RESULT 0", "Save")
        } else if index % 2 == 0 {
            ("OPEN_CANCEL", "OPEN_RESULT true 0", "Cancel")
        } else {
            ("SAVE_CANCEL", "SAVE_RESULT true true", "Cancel")
        };
        dialog.press(operation, expected, button);
    }
    if include_positive_selection {
        selection::selected_paths(&mut dialog, &fixture, &paths, directory_hint);
        dialog.press("MESSAGE_NEXT", "MESSAGE_RESULT 0", "Save");
    }
    dialog
        .writer
        .write_all(b"FINISH\n")
        .expect("quit real native Host");
    assert_eq!(read_control_line(&mut dialog.reader), "QUIT_REPLY");
    drop(dialog);
    let output = wait_child_output(cleanup.release(), EVENT_DEADLINE);
    assert!(
        output.status.success(),
        "file dialogs failed Host cleanup: {output:?}"
    );
    await_process_gone(bun_pid);
}

#[test]
fn real_host_open_save_cancel_and_next_message_use_one_authenticated_dialog_link() {
    run_real_host_files(false, 2, false);
}

#[test]
fn real_host_open_and_save_positive_paths_are_native_and_save_does_not_write() {
    run_real_host_files(true, 2, false);
}

#[test]
fn real_host_directory_default_paths_keep_folder_and_save_filename_distinct() {
    // The independent AX helper must find the fixture's exact directory and
    // file row. Save additionally checks its initial name before editing it;
    // neither helper navigates to repair an incorrectly chosen directory.
    run_real_host_files(true, 2, true);
}

#[test]
fn real_host_257_file_panels_cancel_then_message_healthy_on_same_link() {
    run_real_host_files(false, 257, false);
}

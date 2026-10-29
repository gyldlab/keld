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
use std::os::unix::net::UnixListener;
use std::os::unix::process::CommandExt as _;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Instant;

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
report(`ROLE_BOUND ${process.pid}`);
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

fn run_real_host_files(include_positive_selection: bool, cancel_cycles: usize) {
    assert!(
        cancel_cycles >= 2,
        "native repeated proof must cover Open and Save"
    );
    let fixture = ProductFixture::new("gh478-real-open-save");
    // The macOS file-panel XPC process must be able to enumerate the fixture.
    // Test an owned, visible temporary directory outside the per-process
    // private Darwin temp root when the harness explicitly supplies one.
    // TempDir owns and removes only this exact newly created test directory.
    let accessible_source_directory =
        std::env::var_os("KELD_TEST_ACCESSIBLE_OPEN_PARENT").map(|parent| {
            tempfile::Builder::new()
                .prefix("keld-gh478-visible-open-")
                .tempdir_in(parent)
                .expect("create owned OS-visible native file selection fixture")
        });
    let visible_directory = accessible_source_directory
        .as_ref()
        .map(|dir| dir.path().to_path_buf())
        .unwrap_or_else(|| fixture.root.path().join("gh478-visible-files"));
    if accessible_source_directory.is_none() {
        fs::create_dir(&visible_directory).expect("fixture-owned visible directory");
    }
    let positive_source = visible_directory.join("gh478-fixture.drawio");
    fs::write(
        &positive_source,
        b"<mxfile><diagram>GH478 positive</diagram></mxfile>",
    )
    .expect("one fixture-local real Open source");
    let save_destination = fixture.root.path().join("gh478-chosen.drawio");
    assert!(!save_destination.exists(), "Save is path-only, not a write");
    let staged = stage(&fixture);
    let socket = fixture.root.path().join("native-file.sock");
    let listener = UnixListener::bind(&socket).expect("owned single control listener");
    listener.set_nonblocking(true).expect("bounded accept");
    let child = Command::new(staged.host())
        .process_group(0)
        .current_dir(staged.root())
        .env("KELD_T1B_CONTROL", &socket)
        .env("KELD_T1B_SAVE_DEST", &save_destination)
        .env("KELD_T1B_OPEN_PATH", &positive_source)
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
    let mut writer = connection;
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
    let ax =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/macos_dialog_ax_press.swift");
    let clicker = fixture.root.path().join("ax-press");
    let compiled = Command::new("/usr/bin/swiftc")
        .arg(&ax)
        .arg("-o")
        .arg(&clicker)
        .output()
        .expect("build independent OS UI clicker");
    assert!(compiled.status.success(), "AX compile failed: {compiled:?}");
    for index in 0..=cancel_cycles {
        // Run exactly cancel_cycles real native file panels, followed by one
        // healthy Message on the *same* authenticated Bun role/WorkerLink.
        let (operation, expected, button) = if index == cancel_cycles {
            ("MESSAGE_NEXT", "MESSAGE_RESULT 0", "Save")
        } else if index % 2 == 0 {
            ("OPEN_CANCEL", "OPEN_RESULT true 0", "Cancel")
        } else {
            ("SAVE_CANCEL", "SAVE_RESULT true true", "Cancel")
        };
        writeln!(writer, "{operation}").expect("one authenticated user-directed request");
        assert_eq!(read_control_line(&mut reader), "SUBMITTED");
        let clicked = Command::new(&clicker)
            .arg(host_pid.to_string())
            .arg(button)
            .output()
            .expect("real AppKit accessible button");
        assert!(
            clicked.status.success(),
            "{operation} native click failed: {clicked:?}"
        );
        let result = read_control_line(&mut reader);
        assert_eq!(result, expected, "{operation}: {result}");
        let deadline = Instant::now() + EVENT_DEADLINE;
        while visible_windows(host_pid) != baseline {
            assert!(
                Instant::now() < deadline,
                "{operation} left a native sheet behind"
            );
            std::thread::yield_now();
        }
    }
    if include_positive_selection {
        let selection_source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/macos_dialog_ax_select_file.swift");
        let selection_helper = fixture.root.path().join("ax-select-file");
        let built = Command::new("/usr/bin/swiftc")
            .arg(&selection_source)
            .arg("-o")
            .arg(&selection_helper)
            .output()
            .expect("compile exact-Host native file selector");
        assert!(
            built.status.success(),
            "native selector compilation failed: {built:?}"
        );
        for (operation, prefix, button, selected) in [
            (
                "SAVE_SELECT",
                "SAVE_SELECTED false ",
                "Save",
                &save_destination,
            ),
            (
                "OPEN_SELECT",
                "OPEN_SELECTED false ",
                "Open",
                &positive_source,
            ),
        ] {
            eprintln!("GH478_POSITIVE_PHASE {operation} begin");
            writeln!(writer, "{operation}").expect("genuine file selection requested");
            assert_eq!(read_control_line(&mut reader), "SUBMITTED");
            eprintln!("GH478_POSITIVE_PHASE {operation} submitted");
            if operation == "OPEN_SELECT"
                && let (Ok(helper), Ok(capture_path)) = (
                    std::env::var("KELD_TEST_WINDOW_ID_HELPER"),
                    std::env::var("KELD_TEST_CAPTURE_PATH"),
                )
            {
                let sheet = Command::new(&clicker)
                    .arg(host_pid.to_string())
                    .arg("Cancel")
                    .arg("--observe")
                    .output()
                    .expect("wait for actual Open sheet before diagnostic capture");
                assert!(sheet.status.success(), "Open sheet missing: {sheet:?}");
                let ids = Command::new(helper)
                    .arg(host_pid.to_string())
                    .output()
                    .expect("query only this Host's current native window ID");
                let stdout = String::from_utf8(ids.stdout).expect("window ID output UTF-8");
                let number = stdout
                    .split_whitespace()
                    .find_map(|token| token.strip_prefix("WINDOW_ID="));
                if let Some(number) = number {
                    let result = Command::new("/usr/sbin/screencapture")
                        .args(["-x", "-o", "-l", number, &capture_path])
                        .output()
                        .expect("capture exact Host-owned window for native acceptance review");
                    eprintln!(
                        "GH478_SCREENSHOT_EXIT={} PATH={capture_path}",
                        result.status
                    );
                }
            }
            let click = if operation == "OPEN_SELECT" {
                Command::new(&selection_helper)
                    .arg(host_pid.to_string())
                    .arg(selected)
                    .output()
                    .expect("real OS file selection through exact Host")
            } else {
                Command::new(&clicker)
                    .arg(host_pid.to_string())
                    .arg(button)
                    .output()
                    .expect("actual native file chooser button")
            };
            assert!(
                click.status.success(),
                "{operation} UI selection unavailable: {click:?}"
            );
            eprintln!("GH478_POSITIVE_PHASE {operation} button pressed");
            let result = read_control_line(&mut reader);
            eprintln!("GH478_POSITIVE_PHASE {operation} result {result}");
            assert!(result.starts_with(prefix), "native result: {result}");
            let value = Path::new(result.trim_start_matches(prefix));
            assert_eq!(
                value.file_name(),
                selected.file_name(),
                "real OS filename expected"
            );
            assert_eq!(
                value.parent().and_then(|p| p.canonicalize().ok()),
                selected.parent().and_then(|p| p.canonicalize().ok()),
                "real path must remain in dedicated test fixture, not a guessed directory",
            );
            assert!(
                !save_destination.exists(),
                "native Save only returned a path; no file created"
            );
            let deadline = Instant::now() + EVENT_DEADLINE;
            while visible_windows(host_pid) != baseline {
                assert!(
                    Instant::now() < deadline,
                    "native chooser did not physically detach"
                );
                std::thread::yield_now();
            }
        }
    }
    writer
        .write_all(b"FINISH\n")
        .expect("quit real native Host");
    assert_eq!(read_control_line(&mut reader), "QUIT_REPLY");
    drop(reader);
    drop(writer);
    let output = wait_child_output(cleanup.release(), EVENT_DEADLINE);
    assert!(
        output.status.success(),
        "file dialogs failed Host cleanup: {output:?}"
    );
    await_process_gone(bun_pid);
}

#[test]
fn real_host_open_save_cancel_and_next_message_use_one_authenticated_dialog_link() {
    run_real_host_files(false, 2);
}

#[test]
fn real_host_open_and_save_positive_paths_are_native_and_save_does_not_write() {
    run_real_host_files(true, 2);
}

#[test]
fn real_host_257_file_panels_cancel_then_message_healthy_on_same_link() {
    run_real_host_files(false, 257);
}

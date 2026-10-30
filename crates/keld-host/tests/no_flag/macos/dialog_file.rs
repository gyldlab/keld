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
  } else if (command === "OPEN_SWITCH_VSDX" || command === "OPEN_SWITCH_MERMAID" || command === "OPEN_SWITCH_ALL") {
    const vsdx = command === "OPEN_SWITCH_VSDX";
    const mermaid = command === "OPEN_SWITCH_MERMAID";
    const pending = dialog.showOpenDialog(win, {
      defaultPath: vsdx ? process.env.KELD_T1B_VSDX_PATH! :
        mermaid ? process.env.KELD_T1B_MERMAID_PATH! : process.env.KELD_T1B_XML_SVG_PATH!,
      filters, properties: ["openFile"],
    });
    void pending.then((value) => report(`${vsdx ? "VSDX_SELECTED" : mermaid ? "MERMAID_SELECTED" : "ALL_SELECTED"} ${value.canceled} ${value.filePaths.join("|")}`),
      (error) => report(`OPEN_ERROR ${error?.code ?? String(error)}`));
    report("SUBMITTED");
  } else if (command === "OPEN_MIXED_FILE" || command === "OPEN_MIXED_DIRECTORY" || command === "OPEN_MIXED_BOTH") {
    const pending = dialog.showOpenDialog(win, {
      defaultPath: process.env.KELD_T1B_MIXED_PARENT!,
      filters: [{ name: "All Files", extensions: ["*"] }],
      properties: ["openFile", "openDirectory", "multiSelections"],
    });
    void pending.then((value) => report(`MIXED_SELECTED ${value.canceled} ${value.filePaths.join("|")}`),
      (error) => report(`OPEN_ERROR ${error?.code ?? String(error)}`));
    report("SUBMITTED");
  } else if (command === "OPEN_XML_SVG") {
    const pending = dialog.showOpenDialog(win, {
      defaultPath: process.env.KELD_T1B_XML_SVG_PATH!,
      filters: [{ name: "XML", extensions: ["xml"] }],
      properties: ["openFile"],
    });
    void pending.then((value) => report(value.canceled
      ? `OPEN_RESULT true ${value.filePaths.length}`
      : `UNEXPECTED_SVG ${value.filePaths.join("|")}`),
      (error) => report(`OPEN_ERROR ${error?.code ?? String(error)}`));
    report("SUBMITTED");
  } else if (command === "OPEN_XML_ONLY") {
    const pending = dialog.showOpenDialog(win, {
      defaultPath: process.env.KELD_T1B_XML_PATH!,
      filters: [{ name: "XML", extensions: ["xml"] }],
      properties: ["openFile"],
    });
    void pending.then((value) => report(`XML_SELECTED ${value.canceled} ${value.filePaths.join("|")}`),
      (error) => report(`OPEN_ERROR ${error?.code ?? String(error)}`));
    report("SUBMITTED");
  } else if (command === "OPEN_DUPLICATE_NAMES") {
    const pending = dialog.showOpenDialog(win, {
      defaultPath: process.env.KELD_T1B_XML_PATH!,
      filters: [
        { name: "Duplicate", extensions: ["drawio"] },
        { name: "Other", extensions: ["vsdx"] },
        { name: "Duplicate", extensions: ["xml"] },
        { name: "All Files", extensions: ["*"] },
      ],
      properties: ["openFile"],
    });
    void pending.then((value) => report(`DUPLICATE_SELECTED ${value.canceled} ${value.filePaths.join("|")}`),
      (error) => report(`OPEN_ERROR ${error?.code ?? String(error)}`));
    report("SUBMITTED");
  } else if (command === "OPEN_XML_SVG_ALLFILES") {
    const pending = dialog.showOpenDialog(win, {
      defaultPath: process.env.KELD_T1B_XML_SVG_PATH!,
      filters: [{ name: "All Files", extensions: ["*"] }],
      properties: ["openFile"],
    });
    void pending.then((value) => report(`SVG_CONTROL_SELECTED ${value.canceled} ${value.filePaths.join("|")}`),
      (error) => report(`OPEN_ERROR ${error?.code ?? String(error)}`));
    report("SUBMITTED");
  } else if (command === "OPEN_INVALID_FILTER" || command === "OPEN_LATE_INVALID_FILTER") {
    const filters = command === "OPEN_INVALID_FILTER"
      ? [{ name: "Invalid", extensions: ["a b"] }]
      : [
          { name: "Valid", extensions: ["drawio"] },
          { name: "Late invalid", extensions: ["a b"] },
        ];
    const pending = dialog.showOpenDialog(win, {
      filters, properties: ["openFile"],
    });
    void pending.then((value) => report(`UNEXPECTED_OPEN ${value.filePaths.length}`),
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
    _accessible_directory: tempfile::TempDir,
    _svg_directory: tempfile::TempDir,
    _xml_directory: tempfile::TempDir,
    _vsdx_directory: tempfile::TempDir,
    _mermaid_directory: tempfile::TempDir,
    _mixed_directory: tempfile::TempDir,
    directory: PathBuf,
    source: PathBuf,
    svg: PathBuf,
    xml: PathBuf,
    vsdx: PathBuf,
    mermaid: PathBuf,
    mixed_file: PathBuf,
    mixed_folder: PathBuf,
    destination: PathBuf,
}

impl FilePaths {
    fn new(fixture: &ProductFixture, directory_hint: bool) -> Self {
        // macOS NSOpenPanel hides entries in process-private /var/folders
        // temporary locations. Put only fixture-owned directories under a
        // visible HOME or explicit parent and retain them through Host exit.
        let visible_parent = std::env::var_os("KELD_TEST_ACCESSIBLE_OPEN_PARENT")
            .or_else(|| std::env::var_os("HOME"))
            .map(PathBuf::from)
            .expect("real macOS file picker requires HOME or a visible test parent");
        assert!(
            visible_parent.is_absolute() && visible_parent.is_dir(),
            "real macOS picker test parent must be an existing absolute directory"
        );
        let accessible_directory = tempfile::Builder::new()
            .prefix("keld-gh478-visible-open-")
            .tempdir_in(&visible_parent)
            .expect("create owned OS-visible native file selection fixture");
        let directory = accessible_directory.path().to_path_buf();
        let source = directory.join("gh478-fixture.drawio");
        fs::write(
            &source,
            b"<mxfile><diagram>GH478 positive</diagram></mxfile>",
        )
        .expect("one fixture-local real Open source");
        let svg_directory = tempfile::Builder::new()
            .prefix("keld-gh478-visible-open-")
            .tempdir_in(&visible_parent)
            .expect("owned OS-visible SVG negative fixture directory");
        let svg = svg_directory.path().join("gh478-widened.svg");
        fs::write(&svg, b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>")
            .expect("one XML-conforming but extension-mismatched fixture");
        let xml_directory = tempfile::Builder::new()
            .prefix("keld-gh478-visible-open-")
            .tempdir_in(&visible_parent)
            .expect("owned OS-visible XML positive fixture directory");
        let xml = xml_directory.path().join("gh478-allowed.xml");
        fs::write(&xml, b"<mxfile/>").expect("one exact XML fixture");
        let vsdx_directory = tempfile::Builder::new()
            .prefix("keld-gh478-visible-open-")
            .tempdir_in(&visible_parent)
            .expect("owned OS-visible VSDX fixture directory");
        let vsdx = vsdx_directory.path().join("gh478-format.vsdx");
        fs::write(&vsdx, b"GH478 format group selection sentinel").expect("one exact VSDX fixture");
        let mermaid_directory = tempfile::Builder::new()
            .prefix("keld-gh478-visible-open-")
            .tempdir_in(&visible_parent)
            .expect("owned OS-visible Mermaid fixture directory");
        let mermaid = mermaid_directory.path().join("gh478-flow.mmd");
        fs::write(&mermaid, b"flowchart LR; A --> B").expect("one exact Mermaid fixture");
        let mixed_directory = tempfile::Builder::new()
            .prefix("keld-gh478-visible-open-")
            .tempdir_in(&visible_parent)
            .expect("owned OS-visible mixed selection directory");
        let mixed_file = mixed_directory.path().join("gh478-mixed.drawio");
        let mixed_folder = mixed_directory.path().join("gh478-mixed-folder");
        fs::write(&mixed_file, b"<mxfile>real mixed selection</mxfile>")
            .expect("owned mixed-selection file");
        fs::create_dir(&mixed_folder).expect("owned mixed-selection ordinary directory");
        let destination = if directory_hint {
            directory.join("gh478-chosen.drawio")
        } else {
            fixture.root.path().join("gh478-chosen.drawio")
        };
        assert!(!destination.exists(), "Save is path-only, not a write");
        Self {
            _accessible_directory: accessible_directory,
            _svg_directory: svg_directory,
            _xml_directory: xml_directory,
            _vsdx_directory: vsdx_directory,
            _mermaid_directory: mermaid_directory,
            _mixed_directory: mixed_directory,
            directory,
            source,
            svg,
            xml,
            vsdx,
            mermaid,
            mixed_file,
            mixed_folder,
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
    reject_widened_svg: bool,
    verify_xml_rows: bool,
    verify_filter_switching: bool,
    verify_mixed_selection: bool,
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
        .env("KELD_T1B_XML_SVG_PATH", &paths.svg)
        .env("KELD_T1B_XML_PATH", &paths.xml)
        .env("KELD_T1B_VSDX_PATH", &paths.vsdx)
        .env("KELD_T1B_MERMAID_PATH", &paths.mermaid)
        .env(
            "KELD_T1B_MIXED_PARENT",
            paths.mixed_file.parent().expect("owned mixed parent"),
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
    if reject_widened_svg {
        selection::reject_widened_svg(&mut dialog, &fixture, &paths.svg);
        dialog.press("MESSAGE_NEXT", "MESSAGE_RESULT 0", "Save");
    }
    if verify_xml_rows {
        selection::verify_native_xml_positive(&mut dialog, &fixture, &paths.xml);
        dialog.press("MESSAGE_NEXT", "MESSAGE_RESULT 0", "Save");
    }
    if verify_filter_switching {
        selection::verify_native_filter_switching(&mut dialog, &fixture, &paths);
        dialog.press("MESSAGE_NEXT", "MESSAGE_RESULT 0", "Save");
    }
    if verify_mixed_selection {
        selection::verify_native_mixed_selection(&mut dialog, &fixture, &paths);
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
fn real_host_invalid_native_filter_keeps_dialog_custody_and_healthy_next_message() {
    // Deliberately reuse the actual Host/Guard/Bun launch, not a mocked panel
    // or fabricated native completion. Each rejected filter must deliver the
    // original correlation, retain zero sheets, and permit the next Message.
    let fixture = ProductFixture::new("gh478-invalid-native-filter");
    let _paths = FilePaths::new(&fixture, false);
    let staged = stage(&fixture);
    let socket = fixture.root.path().join("native-file.sock");
    let listener = UnixListener::bind(&socket).expect("owned control socket");
    listener.set_nonblocking(true).expect("bounded accept");
    let child = Command::new(staged.host())
        .process_group(0)
        .current_dir(staged.root())
        .env("KELD_T1B_CONTROL", &socket)
        .env("KELD_DEV_LEASE", "stdin-v1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("actual shipping Host for invalid filter");
    let mut cleanup = ShippingLaunchCleanup::new(child);
    let host = cleanup.cli.as_mut().expect("Host owner");
    let host_pid = host.id();
    let _lease = host.stdin.take().expect("retained Host lease");
    let connection = accept_before(&listener, Instant::now() + EVENT_DEADLINE);
    connection
        .set_read_timeout(Some(EVENT_DEADLINE))
        .expect("bounded control reads");
    let mut reader = BufReader::new(connection.try_clone().expect("control reader"));
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
    assert_eq!(baseline.len(), 1);
    let mut dialog = NativeDialog {
        host_pid,
        baseline,
        reader,
        writer: connection,
        button_helper: compile_ax(&fixture, "press"),
    };
    for operation in ["OPEN_INVALID_FILTER", "OPEN_LATE_INVALID_FILTER"] {
        dialog.submit(operation);
        assert_eq!(
            read_control_line(&mut dialog.reader),
            "OPEN_ERROR KELD-NATIVE-016",
            "{operation} must refuse on original correlation"
        );
        dialog.await_detached(operation);
        dialog.press("MESSAGE_NEXT", "MESSAGE_RESULT 0", "Save");
    }
    dialog.writer.write_all(b"FINISH\n").expect("quit Host");
    assert_eq!(read_control_line(&mut dialog.reader), "QUIT_REPLY");
    drop(dialog);
    let output = wait_child_output(cleanup.release(), EVENT_DEADLINE);
    assert!(
        output.status.success(),
        "Host cleanup after refusals: {output:?}"
    );
    await_process_gone(bun_pid);
}

#[test]
fn real_host_open_save_cancel_and_next_message_use_one_authenticated_dialog_link() {
    run_real_host_files(false, 2, false, false, false, false, false);
}

#[test]
fn real_host_open_and_save_positive_paths_are_native_and_save_does_not_write() {
    run_real_host_files(true, 2, false, false, false, false, false);
}

#[test]
fn real_host_directory_default_paths_keep_folder_and_save_filename_distinct() {
    // The independent AX helper must find the fixture's exact directory and
    // file row. Save additionally checks its initial name before editing it;
    // neither helper navigates to repair an incorrectly chosen directory.
    run_real_host_files(true, 2, true, false, false, false, false);
}

#[test]
fn real_host_xml_filter_refuses_svg_and_next_message_survives() {
    run_real_host_files(false, 2, false, true, false, false, false);
}

#[test]
fn real_host_xml_only_selects_exact_native_xml_path() {
    run_real_host_files(false, 2, false, false, true, false, false);
}

#[test]
fn real_host_native_filter_accessory_switches_vsdx_and_all_files() {
    run_real_host_files(false, 2, false, false, false, true, false);
}

#[test]
fn real_host_native_open_with_files_and_directories_selects_each_and_both() {
    run_real_host_files(false, 2, false, false, false, false, true);
}

#[test]
fn real_host_257_file_panels_cancel_then_message_healthy_on_same_link() {
    run_real_host_files(false, 257, false, false, false, false, false);
}

//! Actual macOS Host and native Message integration acceptance.
//! Compiled only with the explicit native test fixture feature.

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

const ROLE: &str = r#"
import { createConnection } from "node:net";
import { createInterface } from "node:readline";
const socketPath = process.env.KELD_T1B_CONTROL;
if (!socketPath) throw new Error("native fixture control unavailable");
const control = createConnection({ path: socketPath });
await new Promise<void>((resolve, reject) => {
  control.once("connect", resolve);
  control.once("error", reject);
});
function report(message: string): void { control.write(`${message}\n`); }
report(`ROLE_BOUND ${process.pid}`);
app.on("window-all-closed", () => {});
try { await app.whenReady(); } catch (error) {
  report(`READY_ERROR ${String(error)}`);
  throw error;
}
const win = new BrowserWindow({ width: 640, height: 480 });
report(`CREATED ${process.pid} ${win.id}`);
for await (const command of createInterface({ input: control })) {
  if (command === "SHOW") {
    const pending = dialog.showMessageBox(win, {
      message: "Unsaved changes", type: "question", title: "Confirm",
      buttons: ["Save", "Cancel", "Discard Changes"],
      defaultId: 0, cancelId: 1, noLink: true, normalizeAccessKeys: true,
    });
    void pending.then(result => report(`SELECTED ${result.response} ${result.checkboxChecked}`),
      error => report(`ERR ${error?.code ?? String(error)}`));
    report("SUBMITTED");
  } else if (command === "FINISH") {
    win.destroy();
    await app.quit();
    report("QUIT_REPLY");
    break;
  } else {
    throw new Error(`unknown control request ${command}`);
  }
}
control.end();
"#;

/// A separate genuine same-WorkerLink controller. It issues a Message and a
/// fresh Cancel CALL while the actual `AppKit` sheet remains visible, so the
/// Host must answer `c_cancel` and `c_original` independently after native drain.
const CANCEL_ROLE: &str = r#"
import { createConnection } from "node:net";
import { createInterface } from "node:readline";
const link = await WorkerLink.open({
  link: process.env.KELD_APP_LINK!,
  receive: { eventChannels: [LIFECYCLE_CHANNEL, WINDOW_CHANNEL], callReceivers: [] },
});
let readyResolve: () => void = () => {};
const ready = new Promise<void>((resolve) => { readyResolve = resolve; });
link.setStateApplier(LIFECYCLE_CHANNEL, (payload) => {
  if (payload.length !== 1) throw new Error("bad lifecycle fact");
  if (payload[0] === 0) readyResolve();
  else if (payload[0] !== 1) throw new Error("unknown lifecycle fact");
});
let created: WindowEvent | undefined;
link.setStateApplier(WINDOW_CHANNEL, (payload) => {
  const event = decodeWindowEvent(payload);
  if (event.variant === "Created") created = event;
});
const control = createConnection({ path: process.env.KELD_T1B_CONTROL! });
await new Promise<void>((resolve, reject) => {
  control.once("connect", resolve);
  control.once("error", reject);
});
function report(message: string): void { control.write(`${message}\n`); }
report(`BOUND ${process.pid}`);
let window: WindowRef | undefined;
function messageRequest(nonce: bigint): Uint8Array {
  if (!window) throw new Error("Host-minted parent missing");
  return encodeDialogRequest({
    variant: "Message", nonce, parent: window,
    options: {
      message:"Unsaved changes",kind:"Question",
      buttons:["Save","Cancel","Discard Changes"],
      default_id:0,cancel_id:1,normalize_access_keys:true,
      title:"Confirm",detail:null,no_link:true,
      checkbox_label:null,checkbox_checked:null,
    },
  });
}
for await (const cmd of createInterface({ input: control })) {
  if (cmd === "CREATE") {
    await ready;
    const subscribed = await link.call(LIFECYCLE_CHANNEL, Uint8Array.of(1, WINDOW_CHANNEL), 5000);
    if (subscribed[0] !== 1) throw new Error("window subscription refused");
    const result = decodeWindowResponse(await link.call(WINDOW_CHANNEL,
      encodeWindowRequest({ variant:"Create", width:640, height:480, x:null, y:null }), 5000));
    if (result.variant !== "Created") throw new Error("native window Create refused");
    window = result.window;
    if (created?.variant !== "Created" || created.window.id !== window.id ||
        created.window.incarnation !== window.incarnation) {
      throw new Error("window Created native fact/order mismatch");
    }
    report(`CREATED ${window.id}`);
  } else if (cmd === "SHOW") {
    const original = link.call(DIALOG_CHANNEL, messageRequest(1n), 30000);
    void original.then(() => report("ORIGINAL_UNEXPECTED_REPLY"),
      error => report(`ORIGINAL_ERROR ${isCallError(error) ? error.code : "untyped"}`));
    report("SUBMITTED");
  } else if (cmd === "SECOND_MESSAGE") {
    let status = "unexpected-success";
    try {
      await link.call(DIALOG_CHANNEL, messageRequest(2n), 5000);
    } catch (error) { status = isCallError(error) ? error.code : "untyped"; }
    report(`SECOND_BUSY ${status}`);
  } else if (cmd === "FOREIGN_CANCEL") {
    let status = "accepted";
    try {
      await link.call(DIALOG_CHANNEL,encodeDialogRequest({variant:"Cancel",nonce:999n}),5000);
    } catch (error) { status = isCallError(error) ? error.code : "untyped"; }
    report(`FOREIGN_CANCEL ${status}`);
  } else if (cmd === "CANCEL") {
    const ack = decodeDialogReply(await link.call(DIALOG_CHANNEL,
      encodeDialogRequest({variant:"Cancel",nonce:1n}),5000));
    if (ack.variant !== "Cancel") throw new Error("original and Cancel correlations aliased");
    report(`CANCEL_ACK ${ack.disposition}`);
  } else if (cmd === "DESTROY_ACTIVE") {
    if (!window) throw new Error("Host-minted parent required");
    const destroyed = link.call(WINDOW_CHANNEL,
      encodeWindowRequest({variant:"Destroy",window}),10000);
    void destroyed.then(payload => {
      const reply = decodeWindowResponse(payload);
      report(`DESTROY_RESULT ${reply.variant}`);
    }, error => report(`DESTROY_ERROR ${isCallError(error) ? error.code : "untyped"}`));
    report("DESTROY_SUBMITTED");
  } else if (cmd === "NEXT") {
    try {
      const pending = link.call(DIALOG_CHANNEL, messageRequest(2n), 10000);
      report("NEXT_SUBMITTED");
      const response = decodeDialogReply(await pending);
      if (response.variant !== "Message") throw new Error("wrong result channel");
      report(`NEXT_RESULT ${response.response}`);
    } catch (error) {
      report(`NEXT_ERROR ${isCallError(error) ? error.code : String(error)}`);
    }
  } else if (cmd === "FINISH") {
    if (!window) throw new Error("native parent absent at Quit");
    try {
      const destroyed = decodeWindowResponse(await link.call(WINDOW_CHANNEL,
        encodeWindowRequest({variant:"Destroy",window}),5000));
      if (destroyed.variant !== "Destroyed") throw new Error("native parent not released");
      report("DESTROYED");
      await quitAndCloseLink(link, APP_LINK_IO_DEADLINE_MS);
      report("QUIT_REPLY");
    } catch (error) {
      report(`FINISH_ERROR ${isCallError(error) ? error.code : String(error)}`);
      break;
    }
    break;
  } else if (cmd === "QUIT_ONLY") {
    await quitAndCloseLink(link, APP_LINK_IO_DEADLINE_MS);
    report("QUIT_REPLY");
    break;
  } else { throw new Error(`unknown Cancel control ${cmd}`); }
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
    let source = format!("import {{ app, BrowserWindow, dialog }} from {electron};\n{ROLE}");
    crate::renderer_bridge::bundle_public_api_entry(
        &fixture.project,
        repo,
        "gh478-real-message",
        &source,
    );
    fs::write(
        fixture.project.join("keld.config.ts"),
        "export default { name: \"GH478 native Message\", entry: \"src/main.ts\", initialWindow: \"none\" } as const;\n",
    )
    .expect("facade boot configuration");
    fs::write(
        fixture.project.join("keld.permissions.jsonc"),
        r#"{"app":{"system":["dialog"]}}"#,
    )
    .expect("exact verified dialog policy source");
    keld_cli::boot::stage_dev_boot(&fixture.project, Path::new(env!("CARGO_BIN_EXE_keld-host")))
        .expect("actual verified Host stage")
}

fn stage_cancel(fixture: &ProductFixture) -> keld_cli::boot::DevBootStage {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root");
    let transport = serde_json::to_string(
        &repo
            .join("packages/@keld/kipc/src/transport.ts")
            .to_string_lossy(),
    )
    .expect("canonical WorkerLink");
    let window_codec = serde_json::to_string(
        &repo
            .join("packages/@keld/api/src/window-codec.ts")
            .to_string_lossy(),
    )
    .expect("canonical window codec");
    let dialog_codec = serde_json::to_string(
        &repo
            .join("packages/@keld/api/src/dialog.generated.ts")
            .to_string_lossy(),
    )
    .expect("generated native Dialog wire");
    let source = format!(
        "import {{ WorkerLink, LIFECYCLE_CHANNEL, WINDOW_CHANNEL, DIALOG_CHANNEL, isCallError, quitAndCloseLink, APP_LINK_IO_DEADLINE_MS }} from {transport};\n\
         import {{ encodeWindowRequest, decodeWindowResponse, decodeWindowEvent }} from {window_codec};\n\
         import type {{ WindowRef, WindowEvent }} from {window_codec};\n\
         import {{ encodeDialogRequest, decodeDialogReply }} from {dialog_codec};\n{CANCEL_ROLE}"
    );
    crate::renderer_bridge::bundle_public_api_entry(
        &fixture.project,
        repo,
        "gh478-native-cancel",
        &source,
    );
    fs::write(
        fixture.project.join("keld.config.ts"),
        "export default { name: \"GH478 Cancel\", entry: \"src/main.ts\", initialWindow: \"none\" } as const;\n",
    ).expect("native facade boot");
    fs::write(
        fixture.project.join("keld.permissions.jsonc"),
        r#"{"app":{"system":["dialog"]}}"#,
    )
    .expect("verified role-specific dialog permission");
    keld_cli::boot::stage_dev_boot(&fixture.project, Path::new(env!("CARGO_BIN_EXE_keld-host")))
        .expect("native Host stage with one original and Cancel link")
}

fn visible_windows(pid: u32) -> Vec<serde_json::Value> {
    let rows = native_window_rows(pid);
    let document: serde_json::Value = serde_json::from_str(&rows).expect("real CoreGraphics JSON");
    assert_eq!(document["capture_preflight"], true, "{rows}");
    assert_eq!(document["pid"], pid, "exact Host process");
    document["rows"]
        .as_array()
        .expect("OS window observations")
        .iter()
        .filter(|row| row["on_screen"] == true && row["layer"] == 0)
        .cloned()
        .collect()
}

fn run_real_message_cycles(cycles: usize) {
    let fixture = ProductFixture::new("gh478-native-message");
    let staged = stage(&fixture);
    let socket = fixture.root.path().join("message.sock");
    let listener = UnixListener::bind(&socket).expect("owned control listener");
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
        .expect("real macOS Host");
    let mut cleanup = ShippingLaunchCleanup::new(child);
    let host = cleanup.cli.as_mut().expect("live host process");
    let host_pid = host.id();
    let _lease = host.stdin.take().expect("retained dev lease");
    let connection = accept_before(&listener, Instant::now() + EVENT_DEADLINE);
    connection
        .set_read_timeout(Some(EVENT_DEADLINE))
        .expect("bounded receipt read");
    let mut reader = BufReader::new(connection.try_clone().expect("reader clone"));
    let mut writer = connection;
    let bound = read_control_line(&mut reader);
    assert!(bound.starts_with("ROLE_BOUND "), "{bound}");
    let created = read_control_line(&mut reader);
    let parts: Vec<_> = created.split_whitespace().collect();
    assert_eq!(parts.len(), 3, "{created}");
    assert_eq!(parts[0], "CREATED", "{created}");
    assert_eq!(parts[2], "1", "Host issued first genuine window");
    let bun_pid = parse_pid(Some(parts[1]), &created);
    cleanup.record_authenticated_groups(process_group(host_pid), process_group(bun_pid));
    let baseline = visible_windows(host_pid);
    assert_eq!(baseline.len(), 1, "one real Host window");

    let ax =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/macos_dialog_ax_press.swift");
    // Compile once in the private fixture tree. A 257+1 native stress test
    // must not launch a fresh Swift compiler for every physical button.
    let clicker = fixture.root.path().join("ax-press");
    let build = Command::new("/usr/bin/swiftc")
        .arg(&ax)
        .arg("-o")
        .arg(&clicker)
        .output()
        .expect("compile private OS-accessibility fixture");
    assert!(
        build.status.success(),
        "AX fixture compiler failed: {build:?}"
    );
    let buttons = ["Save", "Cancel", "Discard Changes"];
    for index in 0..cycles {
        let selection = index % buttons.len();
        let label = buttons[selection];
        writer
            .write_all(b"SHOW\n")
            .expect("request actual native Message");
        assert_eq!(read_control_line(&mut reader), "SUBMITTED");
        let click = Command::new(&clicker)
            .arg(host_pid.to_string())
            .arg(label)
            .output()
            .expect("OS accessibility one-button click");
        assert!(
            click.status.success(),
            "{label} button not pressed: {click:?}"
        );
        let result = read_control_line(&mut reader);
        assert_eq!(result, format!("SELECTED {selection} false"), "{result}");
        let until = Instant::now() + EVENT_DEADLINE;
        while visible_windows(host_pid) != baseline {
            assert!(
                Instant::now() < until,
                "native sheet remained after actual click"
            );
            std::thread::yield_now();
        }
    }
    writer
        .write_all(b"FINISH\n")
        .expect("finish genuine Host window");
    assert_eq!(read_control_line(&mut reader), "QUIT_REPLY");
    drop(reader);
    drop(writer);
    let output = wait_child_output(cleanup.release(), EVENT_DEADLINE);
    assert!(
        output.status.success(),
        "Host/native Message failed: {output:?}"
    );
    await_process_gone(bun_pid);
}

#[test]
fn public_message_uses_authenticated_host_and_real_native_selection() {
    run_real_message_cycles(3);
}

#[test]
fn real_host_257_message_cycles_keep_one_link_and_a_healthy_next_request() {
    run_real_message_cycles(258);
}

fn launch_cancel_host(
    fixture: &ProductFixture,
) -> (
    ShippingLaunchCleanup,
    std::process::ChildStdin,
    BufReader<std::os::unix::net::UnixStream>,
    std::os::unix::net::UnixStream,
    u32,
    u32,
    Vec<serde_json::Value>,
) {
    let staged = stage_cancel(fixture);
    let socket = fixture.root.path().join("cancel.sock");
    let listener = UnixListener::bind(&socket).expect("owned control listener");
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
        .expect("real no-flag macOS Host");
    let mut cleanup = ShippingLaunchCleanup::new(child);
    let host = cleanup.cli.as_mut().expect("live Host");
    let host_pid = host.id();
    let lease = host.stdin.take().expect("retained dev lease");
    let connection = accept_before(&listener, Instant::now() + EVENT_DEADLINE);
    connection
        .set_read_timeout(Some(EVENT_DEADLINE))
        .expect("bounded role read");
    let mut reader = BufReader::new(connection.try_clone().expect("reader clone"));
    let mut writer = connection;
    let bound = read_control_line(&mut reader);
    assert!(bound.starts_with("BOUND "), "{bound}");
    eprintln!("GH478_CANCEL_PHASE bound");
    let bun_pid = parse_pid(bound.split_whitespace().nth(1), &bound);
    cleanup.record_authenticated_groups(process_group(host_pid), process_group(bun_pid));
    writer
        .write_all(b"CREATE\n")
        .expect("native Create via authenticated link");
    assert_eq!(read_control_line(&mut reader), "CREATED 1");
    eprintln!("GH478_CANCEL_PHASE created");
    let baseline = visible_windows(host_pid);
    assert_eq!(baseline.len(), 1, "Host owns one genuine native window");
    (cleanup, lease, reader, writer, host_pid, bun_pid, baseline)
}

fn compile_cancel_ax_observer(fixture: &ProductFixture) -> std::path::PathBuf {
    let ax =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/macos_dialog_ax_press.swift");
    let observe = fixture.root.path().join("ax-observe");
    let built = Command::new("/usr/bin/swiftc")
        .arg(&ax)
        .arg("-o")
        .arg(&observe)
        .output()
        .expect("compile exact OS-observation tool");
    assert!(built.status.success(), "OS observer compile: {built:?}");
    observe
}

#[test]
fn real_host_cancel_at_native_capacity_retires_original_and_cancel_separately() {
    let fixture = ProductFixture::new("gh478-native-cancel");
    let (cleanup, _lease, mut reader, mut writer, host_pid, bun_pid, baseline) =
        launch_cancel_host(&fixture);

    // Compile the one exact-Host AX observer once; it does not click or
    // fabricate a native completion. The observed sheet must remain visible
    // after a foreign-nonce Cancel before the matching Cancel is sent.
    let observe = compile_cancel_ax_observer(&fixture);
    let visible = || {
        Command::new(&observe)
            .arg(host_pid.to_string())
            .arg("Save")
            .arg("--observe")
            .output()
            .expect("query the exact Host's native button")
    };
    writer
        .write_all(b"SHOW\n")
        .expect("start original native Message");
    assert_eq!(read_control_line(&mut reader), "SUBMITTED");
    eprintln!("GH478_CANCEL_PHASE original_submitted");
    let first = visible();
    assert!(
        first.status.success(),
        "first actual native sheet absent: {first:?}"
    );
    eprintln!("GH478_CANCEL_PHASE native_visible");
    writer
        .write_all(b"SECOND_MESSAGE\n")
        .expect("overlapping original");
    assert_eq!(
        read_control_line(&mut reader),
        "SECOND_BUSY KELD-NATIVE-013",
        "another Message cannot reserve a second native panel"
    );
    writer
        .write_all(b"FOREIGN_CANCEL\n")
        .expect("unmatched fresh Cancel call");
    assert_eq!(
        read_control_line(&mut reader),
        "FOREIGN_CANCEL KELD-NATIVE-009",
        "unknown nonce must not touch the active panel"
    );
    eprintln!("GH478_CANCEL_PHASE foreign_refused");
    let still_visible = visible();
    assert!(
        still_visible.status.success(),
        "foreign Cancel removed a real native sheet: {still_visible:?}"
    );
    writer
        .write_all(b"CANCEL\n")
        .expect("matching same-link Cancel call");
    let first_terminal = read_control_line(&mut reader);
    let second_terminal = read_control_line(&mut reader);
    eprintln!("GH478_CANCEL_PHASE both_terminal_frames");
    let mut terminals = [first_terminal, second_terminal];
    terminals.sort();
    assert_eq!(
        terminals,
        ["CANCEL_ACK Requested", "ORIGINAL_ERROR KELD-NATIVE-015"],
        "fresh Cancel and original correlations require independent terminal receipts"
    );
    let deadline = Instant::now() + EVENT_DEADLINE;
    while visible_windows(host_pid) != baseline {
        assert!(
            Instant::now() < deadline,
            "cancelled native panel did not physically drain"
        );
        std::thread::yield_now();
    }
    writer
        .write_all(b"NEXT\n")
        .expect("same-link healthy follow-up Message");
    assert_eq!(read_control_line(&mut reader), "NEXT_SUBMITTED");
    let next_button = Command::new(&observe)
        .arg(host_pid.to_string())
        .arg("Save")
        .output()
        .expect("real follow-up button action");
    if !next_button.status.success() {
        let next_error = read_control_line(&mut reader);
        panic!("same-link native follow-up refused: {next_error}; OS: {next_button:?}");
    }
    assert_eq!(read_control_line(&mut reader), "NEXT_RESULT 0");
    writer
        .write_all(b"FINISH\n")
        .expect("destroy the Host-minted parent and quit");
    assert_eq!(read_control_line(&mut reader), "DESTROYED");
    assert_eq!(read_control_line(&mut reader), "QUIT_REPLY");
    drop(reader);
    drop(writer);
    let output = wait_child_output(cleanup.release(), EVENT_DEADLINE);
    assert!(
        output.status.success(),
        "host retained canceled native owner: {output:?}"
    );
    await_process_gone(bun_pid);
}

#[test]
fn native_message_parent_destroy_retires_sheet_and_original_without_ghost_window() {
    let fixture = ProductFixture::new("gh478-native-parent-retirement");
    let (cleanup, _lease, mut reader, mut writer, host_pid, bun_pid, baseline) =
        launch_cancel_host(&fixture);
    assert_eq!(baseline.len(), 1);
    let clicker = compile_cancel_ax_observer(&fixture);
    writer
        .write_all(b"SHOW\n")
        .expect("present a real parented sheet");
    assert_eq!(read_control_line(&mut reader), "SUBMITTED");
    let visible = Command::new(&clicker)
        .arg(host_pid.to_string())
        .arg("Save")
        .arg("--observe")
        .output()
        .expect("independent OS native window observation");
    assert!(
        visible.status.success(),
        "sheet was not physically parented: {visible:?}"
    );
    writer
        .write_all(b"DESTROY_ACTIVE\n")
        .expect("Destroy authenticated parent while sheet visible");
    assert_eq!(read_control_line(&mut reader), "DESTROY_SUBMITTED");
    // The original may complete before or after Window Destroy, but neither
    // may claim a user selection after its actual native parent retired.
    let first = read_control_line(&mut reader);
    let second = read_control_line(&mut reader);
    let mut results = [first, second];
    results.sort();
    assert_eq!(
        results,
        ["DESTROY_RESULT Destroyed", "ORIGINAL_ERROR KELD-NATIVE-012"],
        "the original correlation needs typed ParentRetired, not a fabricated click"
    );
    let until = Instant::now() + EVENT_DEADLINE;
    while !visible_windows(host_pid).is_empty() {
        assert!(
            Instant::now() < until,
            "destroyed native parent retained a visible sheet"
        );
        std::thread::yield_now();
    }
    writer
        .write_all(b"QUIT_ONLY\n")
        .expect("Quit current authenticated role");
    assert_eq!(read_control_line(&mut reader), "QUIT_REPLY");
    drop(reader);
    drop(writer);
    let output = wait_child_output(cleanup.release(), EVENT_DEADLINE);
    assert!(
        output.status.success(),
        "Host failed to reap destroyed parent: {output:?}"
    );
    await_process_gone(bun_pid);
}

//! Actual no-flag schema-two Host, authenticated window transport and `AppKit`
//! window creation. This is a native protocol acceptance, NOT yet a public
//! Electron `BrowserWindow` constructor or draw.io compatibility claim.

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
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Instant;

const ROLE: &str = r#"
import { createConnection } from "node:net";
import { createInterface } from "node:readline";

const path = process.env.KELD_T1B_CONTROL;
const endpoint = process.env.KELD_APP_LINK;
if (!path || !endpoint) throw new Error("missing Host-owned endpoint or test control");
const link = await WorkerLink.open({
  link: endpoint,
  receive: { eventChannels: [LIFECYCLE_CHANNEL, WINDOW_CHANNEL], callReceivers: [] },
});
let readyResolve: () => void = () => {};
const ready = new Promise<void>((resolve) => { readyResolve = resolve; });
let created: WindowEvent | undefined;
let createdCount = 0;
link.setStateApplier(LIFECYCLE_CHANNEL, (payload) => {
  if (payload.length !== 1 || payload[0] !== 0) throw new Error("invalid Ready payload");
  readyResolve();
});
link.setStateApplier(WINDOW_CHANNEL, (payload) => {
  const fact = decodeWindowEvent(payload);
  if (fact.variant === "Created") { created = fact; createdCount += 1; }
});

const control = createConnection({ path });
await new Promise<void>((resolve, reject) => {
  control.once("connect", resolve);
  control.once("error", reject);
});
function report(message: string): void { control.write(`${message}\n`); }
report(`BOUND ${process.pid}`);
let window: WindowRef | undefined;
for await (const command of createInterface({ input: control })) {
  if (command === "PRE_SUB") {
    let code = "returned";
    try {
      await link.call(WINDOW_CHANNEL,
        encodeWindowRequest({ variant: "Create", width: 640, height: 480, x: null, y: null }), 5000);
    } catch (error) {
      code = isCallError(error) ? error.code : "untyped";
    }
    report(`PRE_SUB ${code}`);
  } else if (command === "SUBSCRIBE") {
    const response = await link.call(LIFECYCLE_CHANNEL, Uint8Array.of(1, WINDOW_CHANNEL), 5000);
    if (response.length !== 1 || response[0] !== 1) throw new Error("Subscribe not accepted");
    report(`SUBSCRIBED ${createdCount}`);
  } else if (command === "READY") {
    await ready;
    report("READY");
  } else if (command === "CREATE") {
    const response = decodeWindowResponse(await link.call(WINDOW_CHANNEL,
      encodeWindowRequest({ variant: "Create", width: 640, height: 480, x: null, y: null }), 5000));
    if (response.variant !== "Created") throw new Error(`unexpected ${response.variant}`);
    window = response.window;
    if (created?.variant !== "Created" || created.window.id !== window.id ||
        created.window.incarnation !== window.incarnation || createdCount !== 1) {
      throw new Error("host Created reply did not follow its matching native fact");
    }
    report(`CREATED ${window.id}:${window.incarnation} ${created.state.bounds.width}x${created.state.bounds.height}`);
  } else if (command === "STALE") {
    if (!window) throw new Error("missing host-created window");
    let code = "returned";
    try {
      await link.call(WINDOW_CHANNEL, encodeWindowRequest({
        variant: "SetMaximized", window: { id: window.id, incarnation: window.incarnation + 1n },
        maximized: true,
      }), 5000);
    } catch (error) {
      code = isCallError(error) ? error.code : "untyped";
    }
    report(`STALE ${code}`);
  } else if (command === "QUIT") {
    await quitAndCloseLink(link, APP_LINK_IO_DEADLINE_MS);
    report("QUIT_REPLY");
    break;
  } else {
    throw new Error(`unknown control request ${command}`);
  }
}
control.end();
"#;

/// First actual public Electron constructor and cancelable-close proof; the
/// app-created `WebContents` remains intentionally without draw.io preload.
const ELECTRON_WINDOW_ROLE: &str = r#"
import { createConnection } from "node:net";
import { createInterface } from "node:readline";
const path = process.env.KELD_T1B_CONTROL;
if (!path) throw new Error("missing Host acceptance control");
const seen: string[] = [];
const onWebContents = () => { seen.push("contents"); };
const onWindow = () => { seen.push("window"); };
app.on("web-contents-created", onWebContents);
app.on("browser-window-created", onWindow);
app.on("window-all-closed", () => {}); // preserve Host session for native census
let readyCount = 0;
app.on("ready", () => { readyCount += 1; });
const early = app.isReady();
await earlyQuit;
await app.whenReady();
for (const prefs of [
  { nodeIntegration: true },
  { nodeIntegrationInWorker: true },
  { nodeIntegrationInSubFrames: true },
  { contextIsolation: false },
  { webSecurity: false },
  { allowRunningInsecureContent: true },
]) {
  let code = "accepted";
  try {
    new BrowserWindow({ webPreferences: prefs });
  } catch (error) {
    code = error instanceof Error && error.message.startsWith("KELD-COMPAT-010:")
      ? "KELD-COMPAT-010" : "untyped";
  }
  if (code !== "KELD-COMPAT-010" || BrowserWindow.getAllWindows().length !== 0) {
    throw new Error("unsafe webPreferences reached native Create or bypassed triage");
  }
}
// A successful native Create must publish its wrapper without consulting the
// application-realm Map membership method after the Host effect commits.
const originalMapHas = Map.prototype.has;
const originalWindowCall = LifecycleLink.prototype.callWindowBlocking;
let membershipHooks = 0;
let observedNativeCreate = false;
let win: InstanceType<typeof BrowserWindow>;
LifecycleLink.prototype.callWindowBlocking = function (request) {
  const response = Reflect.apply(originalWindowCall, this, [request]);
  if (request.variant === "Create" && response.variant === "Created") {
    observedNativeCreate = true;
    Map.prototype.has = function () {
      membershipHooks += 1;
      throw new Error("application Map.has entered native-window publication");
    };
  }
  return response;
};
try {
  win = new BrowserWindow({
    width: 640, height: 480,
    webPreferences: { contextIsolation: true, nodeIntegration: false, webSecurity: true },
  });
} finally {
  Map.prototype.has = originalMapHas;
  LifecycleLink.prototype.callWindowBlocking = originalWindowCall;
}
if (!observedNativeCreate || membershipHooks !== 0) {
  throw new Error("native Create was not published without an application membership hook");
}
if (early || !app.isReady() || readyCount !== 1) {
  throw new Error("native Ready/Subscribe completion barrier failed");
}
assertStatePhase();
if (win.id !== 1 || win.isDestroyed() || BrowserWindow.getAllWindows()[0] !== win ||
    win.getSize().join("x") !== "640x480" || seen.join(",") !== "contents,window") {
  throw new Error("public constructor failed host ID, native mirror or event order");
}
const control = createConnection({ path });
await new Promise<void>((resolve, reject) => {
  control.once("connect", resolve);
  control.once("error", reject);
});
function report(line: string): void { control.write(`${line}\n`); }
report(`PUBLIC_CREATED ${process.pid} ${win.id} ${win.getSize().join("x")} ${seen.join(",")}`);
let closeCount = 0;
let closedCount = 0;
let replacement: InstanceType<typeof BrowserWindow> | undefined;
const veto = (event: { preventDefault(): void }) => {
  closeCount += 1;
  event.preventDefault();
  report(`CLOSE_EVENT ${closeCount}`);
};
win.on("close", veto);
win.on("closed", () => {
  closedCount += 1;
  if (!win.isDestroyed() || !win.webContents.isDestroyed()) {
    throw new Error("normal-close tombstones were not visible inside closed");
  }
  report(`CLOSED ${closedCount}`);
});
const lines = createInterface({ input: control });
for await (const command of lines) {
  if (command === "VETO") {
    win.close();
  } else if (command === "VERIFY_VETO") {
    const ref = getWindowSnapshots()[0]?.window;
    if (!ref) throw new Error("Host mirror lost a vetoed window");
    const stale = await invokeWindow({
      variant: "CloseReply", window: ref, close_seq: 1n, verdict: "Allow",
    });
    const afterVeto = getWindowSnapshots()[0];
    report(`VETO_VERIFIED ${stale.variant} ${win.isDestroyed()} ${closeCount} ${closedCount} ${afterVeto?.pending_close === null} ${getDispatchedPendingWindowCloses().length}`);
  } else if (command === "ID_TAMPER") {
    // Independent shipping-Host negative: a second live BrowserWindow must
    // never be able to delete the first window's registered close listeners.
    const contents = win.webContents;
    const forgedRelease = Reflect.get(contents, "dispose");
    if (typeof forgedRelease === "function") Reflect.apply(forgedRelease, contents, []);
    if (contents.isDestroyed() || win.isDestroyed()) {
      throw new Error("caller forged WebContents destroyed without a native Host fact");
    }
    const attacker = new BrowserWindow({ width: 320, height: 240 });
    const before = BrowserWindow.getAllWindows();
    if (before.length !== 2 || before[0] !== win || before[1] !== attacker) {
      throw new Error("two-window mutation fixture was not physically created");
    }
    if (Reflect.set(attacker, "id", win.id) ||
        Reflect.get(attacker, "id") === win.id) {
      throw new Error("caller changed a Host-owned BrowserWindow id");
    }
    attacker.destroy();
    if (BrowserWindow.getAllWindows().length !== 1 ||
        BrowserWindow.getAllWindows()[0] !== win || win.isDestroyed()) {
      throw new Error("attacker Destroy evicted the surviving window registry entry");
    }
    report(`ID_TAMPER_SAFE ${win.id} ${closeCount}`);
  } else if (command === "QUIT_WHILE_OPEN") {
    let verdict = "unexpected-success";
    try {
      await app.quit();
    } catch (error) {
      if (error instanceof Error && error.message.startsWith("KELD-COMPAT-012:")) {
        verdict = "KELD-COMPAT-012";
      } else {
        verdict = "untyped";
      }
    }
    report(`QUIT_DENIED ${verdict} ${!win.isDestroyed()} ${BrowserWindow.getAllWindows().length}`);
  } else if (command === "NATIVE_OUT_OF_BAND_CLOSE") {
    // In the dedicated profile-test-hooks binary ONLY, the trusted UI-loop
    // adapter maps this one setter wake into actual NSWindow.close() on main.
    // The ordinary shipping binary never compiles in that test injection.
    win.maximize();
    report(`NATIVE_EXTERNAL_TRIGGER ${win.id} ${closeCount} ${closedCount}`);
  } else if (command === "ALLOW") {
    win.off("close", veto);
    win.close();
  } else if (command === "SECOND") {
    replacement = new BrowserWindow({ width: 320, height: 240, x: 37, y: 91 });
    report(`SECOND ${replacement.id} ${replacement.getSize().join("x")} ${replacement.getPosition().join(",")}`);
  } else if (command === "DESTROY") {
    if (!replacement) throw new Error("missing second native window");
    const second = replacement;
    let closeOnDestroy = 0;
    let closedOnDestroy = 0;
    let contentsDestroyed = 0;
    const contents = second.webContents;
    second.on("close", () => { closeOnDestroy += 1; });
    second.on("closed", () => {
      closedOnDestroy += 1;
      if (!second.isDestroyed() || second.webContents.isDestroyed()) {
        throw new Error("Destroy did not stage its two separate tombstones");
      }
    });
    contents.on("destroyed", () => { contentsDestroyed += 1; });
    second.destroy();
    let readAfterDestroy = "allowed";
    try { void second.webContents; } catch (error) {
      if (error instanceof Error && error.message.includes("Object has been destroyed")) {
        readAfterDestroy = "refused";
      }
    }
    report(`DESTROY ${closeOnDestroy} ${closedOnDestroy} ${readAfterDestroy}`);
    await Promise.resolve();
    report(`CONTENTS_DESTROYED ${contentsDestroyed} ${contents.isDestroyed()}`);
  } else if (command === "DESTROY_ORDER") {
    const order: string[] = [];
    // The generic fixture's normal-close listener requires both tombstones
    // inside closed. Explicit destroy() must leave webContents live until
    // after return, so use its separate, actual Electron E1 oracle here.
    win.removeAllListeners("closed");
    win.on("closed", () => { order.push("closed"); });
    app.on("window-all-closed", () => { order.push("all"); });
    win.destroy();
    order.push("destroy-return");
    report(`DESTROY_ORDER ${order.join(",")}`);
    await app.quit();
    report("QUIT_REPLY");
    break;
  } else if (command === "QUIT") {
    await app.quit();
    report("QUIT_REPLY");
    break;
  } else if (command === "QUIT_AND_CREATE") {
    // The role had no live native windows at the Quit check. A second public
    // constructor in the *same JS turn* must never sneak between that check
    // and actual terminal link admission, bypassing its new window veto.
    const quitting = app.quit();
    let raced = "window-created-after-quit";
    try {
      new BrowserWindow({ width: 320, height: 240 });
    } catch (error) {
      raced = error instanceof Error &&
        (error.message.startsWith("KELD-CORE-045:") || error.message.startsWith("KELD-IPC-"))
        ? "window-refused" : "untyped-refusal";
    }
    await quitting;
    report(`QUIT_ATOMIC ${raced}`);
    break;
  } else {
    throw new Error(`unknown public-window request ${command}`);
  }
}

control.end();
"#;
#[test]
fn public_destroy_fulfills_closed_then_all_closed_before_sync_return() {
    let fixture = ProductFixture::new("facade-destroy-order");
    let staged = stage_electron(&fixture);
    let listener_path = fixture.root.path().join("destroy-order.sock");
    let listener = UnixListener::bind(&listener_path).expect("isolated control");
    listener
        .set_nonblocking(true)
        .expect("bounded control accept");
    let child = Command::new(staged.host())
        .process_group(0)
        .current_dir(staged.root())
        .env("KELD_T1B_CONTROL", &listener_path)
        .env("KELD_DEV_LEASE", "stdin-v1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("launch production no-flag macOS Host");
    let mut cleanup = ShippingLaunchCleanup::new(child);
    let host = cleanup.cli.as_mut().expect("live Host");
    let host_pid = host.id();
    let _lease = host.stdin.take().expect("retain dev lease");
    let control = accept_before(&listener, Instant::now() + EVENT_DEADLINE);
    control
        .set_read_timeout(Some(EVENT_DEADLINE))
        .expect("bounded replies");
    let mut reader = BufReader::new(control.try_clone().expect("reader clone"));
    let mut writer = control;
    let created = read_control_line(&mut reader);
    let fields: Vec<_> = created.split_whitespace().collect();
    assert_eq!(fields.first().copied(), Some("PUBLIC_CREATED"), "{created}");
    let bun_pid = parse_pid(fields.get(1).copied(), &created);
    cleanup.record_authenticated_groups(process_group(host_pid), process_group(bun_pid));
    assert_eq!(visible_app_windows(host_pid).len(), 1);
    writer
        .write_all(b"DESTROY_ORDER\n")
        .expect("atomic native Destroy");
    assert_eq!(
        read_control_line(&mut reader),
        "DESTROY_ORDER closed,all,destroy-return"
    );
    assert_eq!(read_control_line(&mut reader), "QUIT_REPLY");
    drop(reader);
    drop(writer);
    let output = wait_child_output(cleanup.release(), EVENT_DEADLINE);
    assert!(
        output.status.success(),
        "destroy/last-window order failed: {output:?}"
    );
    await_visible_native_close(host_pid);
    await_process_gone(bun_pid);
}

fn census(pid: u32) -> Vec<serde_json::Value> {
    let rows = native_window_rows(pid);
    let value: serde_json::Value = serde_json::from_str(&rows).expect("CoreGraphics JSON census");
    assert_eq!(
        value["capture_preflight"], true,
        "Screen Recording unavailable: {rows}"
    );
    assert_eq!(value["pid"], pid, "wrong CoreGraphics process: {rows}");
    value["rows"]
        .as_array()
        .expect("window rows")
        .iter()
        .filter(|window| window["layer"] == 0)
        .cloned()
        .collect()
}

fn visible_app_windows(pid: u32) -> Vec<serde_json::Value> {
    census(pid)
        .into_iter()
        .filter(|window| window["on_screen"] == true)
        .collect()
}

/// Independent process/SDK oracle: CoreGraphics sees the actual compositor
/// bounds, and `NSScreen` reports the real `AppKit` point-space display frame.
/// Neither value comes from Keld's `WindowSnapshot` or fake UI-port receipts.
fn actual_native_frame_witness(host_pid: u32) -> serde_json::Value {
    const SWIFT: &str = r#"
import AppKit
import CoreGraphics
import Foundation
let pid = Int32(ProcessInfo.processInfo.environment["KELD_WINDOW_WITNESS_PID"]!)!
let all = CGWindowListCopyWindowInfo([.optionAll, .excludeDesktopElements], kCGNullWindowID) as! [[String: Any]]
let rows = all.compactMap { row -> [String: Any]? in
  guard (row[kCGWindowOwnerPID as String] as? NSNumber)?.int32Value == pid,
        (row[kCGWindowLayer as String] as? NSNumber)?.intValue == 0,
        (row[kCGWindowIsOnscreen as String] as? NSNumber)?.boolValue == true,
        let bounds = row[kCGWindowBounds as String] as? [String: Any] else { return nil }
  return ["bounds": bounds, "title": row[kCGWindowName as String] as? String ?? ""]
}
let primary = NSScreen.screens.first!
let frame = primary.frame
let output: [String: Any] = [
  "rows": rows,
  "scale": primary.backingScaleFactor,
  "primaryPointsHigh": frame.size.height,
  "cgPixelsHigh": CGDisplayPixelsHigh(CGMainDisplayID()),
]
let data = try! JSONSerialization.data(withJSONObject: output, options: [.sortedKeys])
print(String(data: data, encoding: .utf8)!)
"#;
    let output = Command::new("/usr/bin/swift")
        .arg("-e")
        .arg(SWIFT)
        .env("KELD_WINDOW_WITNESS_PID", host_pid.to_string())
        .output()
        .expect("independent native CG/AppKit placement observer");
    assert!(
        output.status.success(),
        "Swift CG/AppKit observer failed: {output:?}"
    );
    serde_json::from_slice(&output.stdout).expect("native placement witness JSON")
}

fn await_visible_native_census(pid: u32, expected: &[serde_json::Value]) {
    let deadline = Instant::now() + EVENT_DEADLINE;
    loop {
        let observed = visible_app_windows(pid);
        if observed == expected {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "native compositor did not reach the expected window census: expected={expected:?}, actual={observed:?}"
        );
        // Observe real CoreGraphics state, not a timer that guesses when
        // AppKit has removed the published window from the compositor.
        std::thread::yield_now();
    }
}

fn await_visible_native_close(pid: u32) {
    await_visible_native_census(pid, &[]);
}

fn stage(fixture: &ProductFixture) -> keld_cli::boot::DevBootStage {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root");
    let transport = serde_json::to_string(
        &repo
            .join("packages/@keld/kipc/src/transport.ts")
            .to_string_lossy(),
    )
    .expect("absolute transport import");
    let codec = serde_json::to_string(
        &repo
            .join("packages/@keld/api/src/window-codec.ts")
            .to_string_lossy(),
    )
    .expect("absolute window codec import");
    let source = format!(
        "import {{ WorkerLink, LIFECYCLE_CHANNEL, WINDOW_CHANNEL, isCallError, quitAndCloseLink, APP_LINK_IO_DEADLINE_MS }} from {transport};\n\
         import {{ encodeWindowRequest, decodeWindowResponse, decodeWindowEvent }} from {codec};\n\
         import type {{ WindowRef, WindowEvent }} from {codec};\n{ROLE}"
    );
    crate::renderer_bridge::bundle_public_api_entry(
        &fixture.project,
        repo,
        "gh449-native-window",
        &source,
    );
    fs::write(
        fixture.project.join("keld.config.ts"),
        "export default { name: \"KELD native Window acceptance\", entry: \"src/main.ts\", initialWindow: \"none\" } as const;\n",
    )
    .expect("strict native schema-two config");
    keld_cli::boot::stage_dev_boot(&fixture.project, Path::new(env!("CARGO_BIN_EXE_keld-host")))
        .expect("compile real Host stage")
}

fn stage_electron(fixture: &ProductFixture) -> keld_cli::boot::DevBootStage {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root");
    let electron = serde_json::to_string(
        &repo
            .join("packages/@keld/electron/src/index.ts")
            .to_string_lossy(),
    )
    .expect("absolute Electron import");
    let app = serde_json::to_string(&repo.join("packages/@keld/api/src/app.ts").to_string_lossy())
        .expect("absolute API import");
    let api_link = serde_json::to_string(
        &repo
            .join("packages/@keld/api/src/link.ts")
            .to_string_lossy(),
    )
    .expect("absolute lifecycle adapter import");
    let transport = serde_json::to_string(
        &repo
            .join("packages/@keld/kipc/src/transport.ts")
            .to_string_lossy(),
    )
    .expect("absolute canonical transport");
    // This dependency executes before the public API bundle initializes. Its
    // hooks must not be captured by a second source-path transport instance.
    fs::write(
        fixture.project.join("src/state-phase-probe.ts"),
        include_str!("../../../../../packages/@keld/api/fixtures/window-state-probe.ts"),
    )
    .expect("early application dependency state-phase observer");
    fs::write(
        fixture.project.join("src/state-phase-observer.ts"),
        format!(
            "import {{ WorkerLink, WINDOW_CHANNEL, FrameKind }} from {transport};\n\
         import {{ observeStateAppliers }} from \"./state-phase-probe.ts\";\n\
         observeStateAppliers(WorkerLink, WINDOW_CHANNEL, FrameKind);\n"
        ),
    )
    .expect("observer after early dependency and before API");
    let source = format!(
        "import {{ assertStatePhase }} from \"./state-phase-probe.ts\";\n\
         import \"./state-phase-observer.ts\";\n\
         import {{ getWindowSnapshots, getDispatchedPendingWindowCloses, invokeWindow }} from {app};\n\
         import {{ LifecycleLink }} from {api_link};\n\
         delete process.env.KELD_NATIVE_WINDOW_FACADE;\n\
         const {{ app, BrowserWindow }} = await import({electron});\n\
         if (app.isReady()) throw new Error(\"native pre-import Quit probe missed pre-Ready boundary\");\n\
         const earlyQuit = app.quit().then(() => {{\n\
           throw new Error(\"pre-import environment deletion authorized forceful native Quit\");\n\
         }}, (error) => {{\n\
           if (!(error instanceof Error) || !error.message.startsWith(\"KELD-COMPAT-012:\")) throw error;\n\
         }});\n{ELECTRON_WINDOW_ROLE}"
    );
    crate::renderer_bridge::bundle_public_api_entry(
        &fixture.project,
        repo,
        "gh449-electron-window",
        &source,
    );
    fs::write(
        fixture.project.join("keld.config.ts"),
        "export default { name: \"KELD public Window acceptance\", entry: \"src/main.ts\", initialWindow: \"none\" } as const;\n",
    )
    .expect("strict schema-two facade declaration");
    keld_cli::boot::stage_dev_boot(&fixture.project, Path::new(env!("CARGO_BIN_EXE_keld-host")))
        .expect("compile actual Electron window stage")
}

/** Separate acceptance atom: Host-minted follow-up window and Destroy tombstones. */
fn assert_followup_create_destroy(
    reader: &mut BufReader<UnixStream>,
    writer: &mut UnixStream,
    host_pid: u32,
) {
    writer
        .write_all(b"SECOND\n")
        .expect("create second host window");
    assert_eq!(
        read_control_line(reader),
        "SECOND 3 320x240 37,91",
        "the destroyed attacker consumes ID 2; Host IDs must never be recycled"
    );
    assert_eq!(
        visible_app_windows(host_pid).len(),
        1,
        "second native window absent"
    );
    let geometry = actual_native_frame_witness(host_pid);
    let rows = geometry["rows"].as_array().expect("native CGWindow rows");
    assert_eq!(
        rows.len(),
        1,
        "actual physical compositor window: {geometry}"
    );
    let bounds = &rows[0]["bounds"];
    assert_eq!(bounds["X"].as_f64(), Some(37.0), "native X: {geometry}");
    assert_eq!(
        bounds["Y"].as_f64(),
        Some(91.0),
        "native top-left Y: {geometry}"
    );
    assert_eq!(
        bounds["Width"].as_f64(),
        Some(320.0),
        "native width: {geometry}"
    );
    assert_eq!(
        bounds["Height"].as_f64(),
        Some(240.0),
        "native height: {geometry}"
    );
    writer
        .write_all(b"DESTROY\n")
        .expect("synchronous native Destroy");
    assert_eq!(
        read_control_line(reader),
        "DESTROY 0 1 refused",
        "Destroy must skip close and synchronously emit closed after native release"
    );
    assert_eq!(
        read_control_line(reader),
        "CONTENTS_DESTROYED 1 true",
        "webContents tombstone belongs after Destroy returns"
    );
    await_visible_native_close(host_pid);
}

#[test]
fn public_electron_browser_window_uses_native_host_and_vetoes_unsaved_close() {
    let fixture = ProductFixture::new("facade-electron-window");
    let staged = stage_electron(&fixture);
    let listener_path = fixture.root.path().join("electron-window.sock");
    let listener = UnixListener::bind(&listener_path).expect("private control listener");
    listener.set_nonblocking(true).expect("bounded accept");
    let child = Command::new(staged.host())
        .process_group(0)
        .current_dir(staged.root())
        .env("KELD_T1B_CONTROL", &listener_path)
        .env("KELD_DEV_LEASE", "stdin-v1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("launch real macOS no-flag Host");
    let mut cleanup = ShippingLaunchCleanup::new(child);
    let host = cleanup.cli.as_mut().expect("live Host");
    let host_pid = host.id();
    let _lease = host.stdin.take().expect("retain dev lease");
    let stream = accept_before(&listener, Instant::now() + EVENT_DEADLINE);
    stream
        .set_read_timeout(Some(EVENT_DEADLINE))
        .expect("bounded control read");
    let mut reader = BufReader::new(stream.try_clone().expect("control clone"));
    let mut writer = stream;
    let created = read_control_line(&mut reader);
    let parts: Vec<_> = created.split_whitespace().collect();
    assert_eq!(parts.first().copied(), Some("PUBLIC_CREATED"), "{created}");
    let bun_pid = parse_pid(parts.get(1).copied(), &created);
    cleanup.record_authenticated_groups(process_group(host_pid), process_group(bun_pid));
    assert_eq!(
        &parts[2..],
        ["1", "640x480", "contents,window"],
        "{created}"
    );
    let initial = visible_app_windows(host_pid);
    assert_eq!(
        initial.len(),
        1,
        "public Create must construct a real visible native app window"
    );

    writer
        .write_all(b"VETO\n")
        .expect("request cancelable close");
    assert_eq!(read_control_line(&mut reader), "CLOSE_EVENT 1");
    writer
        .write_all(b"VERIFY_VETO\n")
        .expect("probe applied verdict");
    assert_eq!(
        read_control_line(&mut reader),
        "VETO_VERIFIED ReplyStale false 1 0 true 0",
        "a stale Allow must not override the first Cancel verdict"
    );
    assert_eq!(
        visible_app_windows(host_pid),
        initial,
        "veto must retain the same native window"
    );
    writer
        .write_all(b"ID_TAMPER\n")
        .expect("attempt cross-window ownership mutation");
    assert_eq!(
        read_control_line(&mut reader),
        "ID_TAMPER_SAFE 1 1",
        "one destroyed window must never evict the surviving window's close listener"
    );
    await_visible_native_census(host_pid, &initial);

    writer
        .write_all(b"QUIT_WHILE_OPEN\n")
        .expect("attempt unsupported non-vetoable application Quit");
    assert_eq!(
        read_control_line(&mut reader),
        "QUIT_DENIED KELD-COMPAT-012 true 1",
        "app.quit must not bypass close veto while a native document is live"
    );
    assert_eq!(visible_app_windows(host_pid), initial);

    writer
        .write_all(b"ALLOW\n")
        .expect("accept next close attempt");
    assert_eq!(read_control_line(&mut reader), "CLOSED 1");
    await_visible_native_close(host_pid);
    assert_followup_create_destroy(&mut reader, &mut writer, host_pid);
    writer
        .write_all(b"QUIT_AND_CREATE\n")
        .expect("atomic authenticated Quit");
    assert_eq!(
        read_control_line(&mut reader),
        "QUIT_ATOMIC window-refused",
        "a native Create raced into the gap between facade Quit check and actual link close"
    );
    drop(writer);
    let output = wait_child_output(cleanup.release(), EVENT_DEADLINE);
    assert!(
        output.status.success(),
        "public BrowserWindow Host failed: {output:?}"
    );
    await_process_gone(bun_pid);
}

/// Only compiled by the explicit debug fixture feature. The tested UI path
/// is the actual Tao `windowWillClose:` -> Destroyed, not a fake native fact,
/// synthetic Rust event or cancelable AX/red-button request.
#[cfg(feature = "profile-test-hooks")]
#[test]
fn external_appkit_close_fails_closed_instead_of_leaving_a_ghost_browser_window() {
    let fixture = ProductFixture::new("facade-external-native-close");
    let staged = stage_electron(&fixture);
    let listener_path = fixture.root.path().join("external-window.sock");
    let listener = UnixListener::bind(&listener_path).expect("private control listener");
    listener.set_nonblocking(true).expect("bounded accept");
    let child = Command::new(staged.host())
        .process_group(0)
        .current_dir(staged.root())
        .env("KELD_T1B_CONTROL", &listener_path)
        .env("KELD_T1B_UNEXPECTED_NATIVE_CLOSE", "1")
        .env("KELD_DEV_LEASE", "stdin-v1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("launch real debug-profile macOS Host");
    let mut cleanup = ShippingLaunchCleanup::new(child);
    let host = cleanup.cli.as_mut().expect("live Host");
    let host_pid = host.id();
    let _lease = host.stdin.take().expect("retain dev lease");
    let stream = accept_before(&listener, Instant::now() + EVENT_DEADLINE);
    stream
        .set_read_timeout(Some(EVENT_DEADLINE))
        .expect("bounded control read");
    let mut reader = BufReader::new(stream.try_clone().expect("control clone"));
    let mut writer = stream;
    let created = read_control_line(&mut reader);
    let parts: Vec<_> = created.split_whitespace().collect();
    assert_eq!(parts.first().copied(), Some("PUBLIC_CREATED"), "{created}");
    let bun_pid = parse_pid(parts.get(1).copied(), &created);
    cleanup.record_authenticated_groups(process_group(host_pid), process_group(bun_pid));
    assert_eq!(
        &parts[2..],
        ["1", "640x480", "contents,window"],
        "{created}"
    );
    assert_eq!(visible_app_windows(host_pid).len(), 1);

    writer
        .write_all(b"NATIVE_OUT_OF_BAND_CLOSE\n")
        .expect("inject actual AppKit close on UI thread");
    assert_eq!(
        read_control_line(&mut reader),
        "NATIVE_EXTERNAL_TRIGGER 1 0 0",
        "test stimulus did not traverse the real native setter command"
    );
    // `NSWindow.close()` makes no request to the Bun veto listener. The
    // accepted outcome is a Host fatal with cleanup, never a live ghost or
    // a fabricated successful user `closed`/LastWindowClosed event.
    drop(reader);
    drop(writer);
    let output = wait_child_output(cleanup.release(), EVENT_DEADLINE);
    assert!(
        !output.status.success(),
        "unmanaged native close was silently accepted: {output:?}"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unexpected out-of-band native NSWindow close for registry view"),
        "Host must fail closed specifically on Tao Destroyed, not an unrelated crash: {stderr}"
    );
    await_visible_native_close(host_pid);
    await_process_gone(bun_pid);
}

#[test]
fn native_schema_two_subscribe_creates_one_real_app_window_without_authority_leak() {
    let fixture = ProductFixture::new("facade-native-create");
    let staged = stage(&fixture);
    let listener_path = fixture.root.path().join("native-create.sock");
    let listener = UnixListener::bind(&listener_path).expect("private control listener");
    listener.set_nonblocking(true).expect("bounded accept");
    let child = Command::new(staged.host())
        .process_group(0)
        .current_dir(staged.root())
        .env("KELD_T1B_CONTROL", &listener_path)
        .env("KELD_DEV_LEASE", "stdin-v1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("launch actual macOS shipping Host");
    let mut cleanup = ShippingLaunchCleanup::new(child);
    let host = cleanup.cli.as_mut().expect("live Host");
    let host_pid = host.id();
    let _lease = host.stdin.take().expect("keep Host dev lease");
    let stream = accept_before(&listener, Instant::now() + EVENT_DEADLINE);
    stream
        .set_read_timeout(Some(EVENT_DEADLINE))
        .expect("bounded control reader");
    let mut reader = BufReader::new(stream.try_clone().expect("control clone"));
    let mut writer = stream;
    let bound = read_control_line(&mut reader);
    let parts: Vec<_> = bound.split_whitespace().collect();
    assert_eq!(parts.first().copied(), Some("BOUND"), "{bound}");
    let bun_pid = parse_pid(parts.get(1).copied(), &bound);
    cleanup.record_authenticated_groups(process_group(host_pid), process_group(bun_pid));

    writer.write_all(b"PRE_SUB\n").expect("unsubscribed create");
    assert_eq!(read_control_line(&mut reader), "PRE_SUB KELD-CORE-045");
    assert!(
        census(host_pid).is_empty(),
        "refused Create allocated a window"
    );
    writer
        .write_all(b"SUBSCRIBE\n")
        .expect("subscribe to window events");
    assert_eq!(read_control_line(&mut reader), "SUBSCRIBED 0");
    writer.write_all(b"READY\n").expect("raw Init Ready");
    assert_eq!(read_control_line(&mut reader), "READY");
    assert!(
        census(host_pid).is_empty(),
        "Init Ready allocated a native anchor"
    );

    writer.write_all(b"CREATE\n").expect("authenticated Create");
    assert_eq!(read_control_line(&mut reader), "CREATED 1:1 640x480");
    let created_rows = visible_app_windows(host_pid);
    assert_eq!(
        created_rows.len(),
        1,
        "Create must allocate exactly one visible app window: {created_rows:?}"
    );
    assert_eq!(created_rows[0]["title"], "Keld", "actual native app title");
    writer
        .write_all(b"STALE\n")
        .expect("forged window generation");
    assert_eq!(read_control_line(&mut reader), "STALE KELD-CORE-038");
    let stale_rows = visible_app_windows(host_pid);
    assert_eq!(
        stale_rows, created_rows,
        "stale caller mutated native census: before {created_rows:?}, after {stale_rows:?}"
    );

    writer
        .write_all(b"QUIT\n")
        .expect("orderly authenticated Quit");
    assert_eq!(read_control_line(&mut reader), "QUIT_REPLY");
    drop(writer);
    let output = wait_child_output(cleanup.release(), EVENT_DEADLINE);
    assert!(
        output.status.success(),
        "native Host shutdown failed: {output:?}"
    );
    await_process_gone(bun_pid);
}

// F2 atoms A/B/C/E: actual shipping Host, no sidecar or an independently
// resolved ordinary package copy, and actual env mutation before FIRST import.
#[test]
fn native_boot_without_sidecar_refuses_late_import_pre_ready_quit() {
    assert_late_package_boot_mode(false, false);
}

#[test]
fn native_boot_preload_does_not_mask_a_distinct_late_package_copy() {
    assert_late_package_boot_mode(true, false);
}

#[test]
fn recovered_native_window_survives_late_package_pre_ready_quit() {
    assert_late_package_boot_mode(true, true);
}

const LATE_PACKAGE_BOOT_MODE_ROLE: &str = r#"
import { createConnection } from "node:net";
import { createInterface } from "node:readline";
import { existsSync, writeFileSync } from "node:fs";
const control = createConnection({ path: process.env.KELD_T1B_CONTROL! });
await new Promise<void>((resolve, reject) => { control.once("connect", resolve); control.once("error", reject); });
const report = (message: string) => control.write(`${message}\n`);
report(`BOUND ${process.pid}`);
try {
const generationPath = process.env.KELD_F2_GENERATION_FILE!;
const successor = existsSync(generationPath);
writeFileSync(generationPath, "started");
// The recovery control's first role keeps the original environment, so the
// unfixed implementation can create a real survivor before the downgrade.
if (process.env.KELD_F2_RECOVER !== "1" || successor) {
  // Reflect observes the actual environment even under expression substitution.
  if (process.env.KELD_F2_HAS_SIDECAR === "1") {
    Reflect.set(process.env, "KELD_NATIVE_WINDOW_FACADE", "0");
    if (Reflect.get(process.env, "KELD_NATIVE_WINDOW_FACADE") !== "0") throw new Error("actual env was not zeroed");
  } else {
    Reflect.deleteProperty(process.env, "KELD_NATIVE_WINDOW_FACADE");
    if (Reflect.get(process.env, "KELD_NATIVE_WINDOW_FACADE") !== undefined) throw new Error("actual env was not deleted");
  }
}
const first = await import("./packages/@keld/kipc/src/transport.ts");
if (process.env.KELD_F2_HAS_SIDECAR === "1") {
  const preloaded = await import("./kipc-transport.ts");
  if (first === preloaded || first.WorkerLink === preloaded.WorkerLink) throw new Error("distinct package copy collapsed into preload");
}
const { app, BrowserWindow } = await import("./packages/@keld/electron/src/index.ts");
if (app.isReady()) throw new Error("pre-Ready boundary missed");
let code = "accepted";
try { await app.quit(); } catch (error) {
  code = error instanceof Error ? error.message.split(":")[0] : "untyped";
}
report(`PREQUIT ${code}`);
if (code !== "KELD-COMPAT-012") throw new Error("late package import bypassed native Quit barrier");
app.on("window-all-closed", () => {});
await app.whenReady();
const win = BrowserWindow.getAllWindows()[0] ?? new BrowserWindow({ width: 320, height: 240 });
let veto = true;
win.on("close", (event) => { if (veto) event.preventDefault(); });
report(`CREATED ${win.id}`);
for await (const command of createInterface({ input: control })) {
  if (command === "CRASH") {
    process.exit(71);
  } else if (command === "VETO") {
    win.close();
    report(`VETO ${win.isDestroyed()} ${BrowserWindow.getAllWindows().length}`);
  } else if (command === "QUIT_WHILE_OPEN") {
    let refusal = "accepted";
    try { await app.quit(); } catch (error) { refusal = error instanceof Error ? error.message.split(":")[0] : "untyped"; }
    report(`QUIT ${refusal}`);
  } else if (command === "FINISH") {
    veto = false;
    win.destroy();
    await app.quit();
    report("FINISHED");
    break;
  }
}
control.end();
} catch (error) {
  report(`F2_ERROR ${String(error)}`);
  throw error;
}
"#;

fn stage_late_package(
    fixture: &ProductFixture,
    with_sidecar: bool,
) -> keld_cli::boot::DevBootStage {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root");
    fs::write(
        fixture.project.join("src/main.ts"),
        LATE_PACKAGE_BOOT_MODE_ROLE,
    )
    .expect("ordinary unbundled entry");
    if with_sidecar {
        fs::copy(
            repo.join("packages/@keld/kipc/src/transport.ts"),
            fixture.project.join("src/kipc-transport.ts"),
        )
        .expect("optional byte-identical sidecar");
    }
    fs::write(fixture.project.join("keld.config.ts"),
        "export default { name: \"F2 late package\", entry: \"src/main.ts\", initialWindow: \"none\" } as const;\n")
        .expect("native schema two config");
    let staged = keld_cli::boot::stage_dev_boot(
        &fixture.project,
        Path::new(env!("CARGO_BIN_EXE_keld-host")),
    )
    .expect("actual Host stage");
    crate::renderer_bridge::copy_public_package_sources(repo, &staged.root().join("src/packages"));
    staged
}

fn assert_late_package_boot_mode(with_sidecar: bool, recover: bool) {
    let fixture = ProductFixture::new("f2-late-package");
    let staged = stage_late_package(&fixture, with_sidecar);
    let listener_path = fixture.root.path().join("f2.sock");
    let listener = UnixListener::bind(&listener_path).expect("late import control");
    listener.set_nonblocking(true).expect("bounded accept");
    let child = Command::new(staged.host())
        .process_group(0)
        .current_dir(staged.root())
        .env("KELD_T1B_CONTROL", &listener_path)
        .env("KELD_DEV_LEASE", "stdin-v1")
        .env("KELD_NATIVE_WINDOW_FACADE", "unknown-caller-value")
        .env("KELD_F2_HAS_SIDECAR", if with_sidecar { "1" } else { "0" })
        .env("KELD_F2_RECOVER", if recover { "1" } else { "0" })
        .env(
            "KELD_F2_GENERATION_FILE",
            fixture.root.path().join("generation"),
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("shipping no-flag Host");
    let mut cleanup = ShippingLaunchCleanup::new(child);
    let host = cleanup.cli.as_mut().expect("live Host");
    let host_pid = host.id();
    let _lease = host.stdin.take().expect("retain dev lease");
    let stream = accept_before(&listener, Instant::now() + EVENT_DEADLINE);
    stream
        .set_read_timeout(Some(EVENT_DEADLINE))
        .expect("bounded read");
    let mut reader = BufReader::new(stream.try_clone().expect("control clone"));
    let mut writer = stream;
    let bound = read_control_line(&mut reader);
    let parts: Vec<_> = bound.split_whitespace().collect();
    assert_eq!(parts.first().copied(), Some("BOUND"), "{bound}");
    let mut bun_pid = parse_pid(parts.get(1).copied(), &bound);
    cleanup.record_authenticated_groups(process_group(host_pid), process_group(bun_pid));
    assert_eq!(read_control_line(&mut reader), "PREQUIT KELD-COMPAT-012");
    assert_eq!(read_control_line(&mut reader), "CREATED 1");
    let initial = visible_app_windows(host_pid);
    assert_eq!(initial.len(), 1, "real native Create");
    if recover {
        writer
            .write_all(b"CRASH\n")
            .expect("retire first authenticated generation");
        let successor = accept_before(&listener, Instant::now() + EVENT_DEADLINE);
        successor
            .set_read_timeout(Some(EVENT_DEADLINE))
            .expect("successor read deadline");
        reader = BufReader::new(successor.try_clone().expect("successor reader"));
        writer = successor;
        let bound = read_control_line(&mut reader);
        let parts: Vec<_> = bound.split_whitespace().collect();
        assert_eq!(parts.first().copied(), Some("BOUND"), "{bound}");
        let successor_pid = parse_pid(parts.get(1).copied(), &bound);
        assert_ne!(bun_pid, successor_pid, "recovery must mint a fresh process");
        cleanup.record_authenticated_groups(process_group(host_pid), process_group(successor_pid));
        assert_eq!(
            read_control_line(&mut reader),
            "PREQUIT KELD-COMPAT-012",
            "pre-Subscribe successor Quit must never destroy the surviving native window"
        );
        assert_eq!(
            read_control_line(&mut reader),
            "CREATED 1",
            "authenticated replay adopts the survivor"
        );
        await_visible_native_census(host_pid, &initial);
        await_process_gone(bun_pid);
        bun_pid = successor_pid;
    }
    writer.write_all(b"VETO\n").expect("native close veto");
    assert_eq!(read_control_line(&mut reader), "VETO false 1");
    await_visible_native_census(host_pid, &initial);
    writer
        .write_all(b"QUIT_WHILE_OPEN\n")
        .expect("native Quit barrier");
    assert_eq!(read_control_line(&mut reader), "QUIT KELD-COMPAT-012");
    assert_eq!(visible_app_windows(host_pid), initial);
    writer
        .write_all(b"FINISH\n")
        .expect("orderly close and Quit");
    assert_eq!(read_control_line(&mut reader), "FINISHED");
    drop(writer);
    let output = wait_child_output(cleanup.release(), EVENT_DEADLINE);
    assert!(
        output.status.success(),
        "late package Host failed: {output:?}"
    );
    await_visible_native_close(host_pid);
    await_process_gone(bun_pid);
    assert!(!staged.root().exists(), "dev stage survived native cleanup");
    eprintln!(
        "F2_NATIVE_EVIDENCE host={host_pid} final_bun={bun_pid} sidecar={with_sidecar} recovered={recover} same_native_rows={initial:?} quit=KELD-COMPAT-012 stage_deleted=true"
    );
}

#[test]
fn renderer_boot_keeps_legacy_ready_and_quit_after_parent_and_late_env_one() {
    let fixture = ProductFixture::new("f2-legacy-package");
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root");
    fs::write(fixture.project.join("src/main.ts"), r#"
import { createConnection } from "node:net";
import { createInterface } from "node:readline";
Reflect.set(process.env, "KELD_NATIVE_WINDOW_FACADE", "1");
if (Reflect.get(process.env, "KELD_NATIVE_WINDOW_FACADE") !== "1") throw new Error("actual env was not set");
const transport = await import("./packages/@keld/kipc/src/transport.ts");
const { app } = await import("./packages/@keld/electron/src/index.ts");
await app.whenReady();
const control = createConnection({ path: process.env.KELD_T1B_CONTROL! });
await new Promise<void>((resolve, reject) => { control.once("connect", resolve); control.once("error", reject); });
control.write(`LEGACY_READY ${process.pid} ${transport.nativeFacadeBootSelected}\n`);
for await (const command of createInterface({ input: control })) {
  if (command !== "QUIT") throw new Error(`unexpected ${command}`);
  await app.quit();
  control.write("LEGACY_QUIT_ACCEPTED\n");
  break;
}
control.end();
"#).expect("ordinary legacy package entry");
    fs::write(
        fixture.project.join("index.html"),
        "<!doctype html><title>F2 legacy</title>",
    )
    .expect("legacy renderer");
    fs::write(fixture.project.join("keld.config.ts"),
        "export default { name: \"F2 legacy\", entry: \"src/main.ts\", renderer: \"index.html\" } as const;\n")
        .expect("schema one config");
    let staged = keld_cli::boot::stage_dev_boot(
        &fixture.project,
        Path::new(env!("CARGO_BIN_EXE_keld-host")),
    )
    .expect("legacy shipping stage");
    crate::renderer_bridge::copy_public_package_sources(repo, &staged.root().join("src/packages"));
    let listener_path = fixture.root.path().join("legacy.sock");
    let listener = UnixListener::bind(&listener_path).expect("legacy control");
    listener.set_nonblocking(true).expect("bounded accept");
    let child = Command::new(staged.host())
        .process_group(0)
        .current_dir(staged.root())
        .env("KELD_T1B_CONTROL", &listener_path)
        .env("KELD_DEV_LEASE", "stdin-v1")
        .env("KELD_NATIVE_WINDOW_FACADE", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("shipping renderer Host");
    let mut cleanup = ShippingLaunchCleanup::new(child);
    let host = cleanup.cli.as_mut().expect("live legacy Host");
    let host_pid = host.id();
    let _lease = host.stdin.take().expect("retain dev lease");
    let stream = accept_before(&listener, Instant::now() + EVENT_DEADLINE);
    stream
        .set_read_timeout(Some(EVENT_DEADLINE))
        .expect("bounded legacy read");
    let mut reader = BufReader::new(stream.try_clone().expect("legacy control clone"));
    let mut writer = stream;
    let ready = read_control_line(&mut reader);
    let parts: Vec<_> = ready.split_whitespace().collect();
    assert_eq!(parts.first().copied(), Some("LEGACY_READY"), "{ready}");
    assert_eq!(parts.get(2).copied(), Some("false"), "{ready}");
    let bun_pid = parse_pid(parts.get(1).copied(), &ready);
    cleanup.record_authenticated_groups(process_group(host_pid), process_group(bun_pid));
    assert_eq!(
        visible_app_windows(host_pid).len(),
        1,
        "legacy renderer is a real window"
    );
    writer.write_all(b"QUIT\n").expect("forceful legacy Quit");
    assert_eq!(read_control_line(&mut reader), "LEGACY_QUIT_ACCEPTED");
    drop(writer);
    let output = wait_child_output(cleanup.release(), EVENT_DEADLINE);
    assert!(output.status.success(), "legacy Host failed: {output:?}");
    await_visible_native_close(host_pid);
    await_process_gone(bun_pid);
    assert!(!staged.root().exists(), "legacy stage cleanup");
}

#[test]
fn no_sidecar_runtime_config_is_refused_before_the_shipping_entry_executes() {
    for config in ["bunfig.toml", ".bunfig.toml"] {
        let fixture = ProductFixture::new("f2-config-admission");
        let marker = fixture.root.path().join("entry-executed");
        fs::write(
            fixture.project.join("src/main.ts"),
            "await Bun.write(process.env.KELD_F2_ENTRY_MARKER!, 'executed');\n",
        )
        .expect("independent entry execution marker");
        fs::write(fixture.project.join("keld.config.ts"),
            "export default { name: \"F2 config\", entry: \"src/main.ts\", initialWindow: \"none\" } as const;\n")
            .expect("schema two config");
        let staged = keld_cli::boot::stage_dev_boot(
            &fixture.project,
            Path::new(env!("CARGO_BIN_EXE_keld-host")),
        )
        .expect("no-sidecar stage");
        fs::write(
            staged.root().join(config),
            "[define]\n\"process.env.KELD_NATIVE_WINDOW_FACADE\" = '\"0\"'\n",
        )
        .expect("hostile staged runtime config");
        let mut child = Command::new(staged.host())
            .process_group(0)
            .current_dir(staged.root())
            .env("KELD_DEV_LEASE", "stdin-v1")
            .env("KELD_F2_ENTRY_MARKER", &marker)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("shipping Host config admission");
        let host_pid = child.id();
        let _lease = child.stdin.take().expect("retain startup lease");
        let output = wait_child_output(child, EVENT_DEADLINE);
        assert!(
            !output.status.success(),
            "runtime config executed: {output:?}"
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains(&format!("staged runtime config {config}")),
            "{stderr}"
        );
        assert!(
            !marker.exists(),
            "original entry executed before config refusal"
        );
        assert!(
            visible_app_windows(host_pid).is_empty(),
            "refusal allocated a native window"
        );
    }
}

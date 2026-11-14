use crate::support::EVENT_DEADLINE;
use crate::support::dev_cycle::ShippingLaunchCleanup;
use crate::support::native_window::{NativeWindowObserver, compile_native_window_census};
use crate::support::product::ProductFixture;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::os::unix::process::ExitStatusExt as _;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

const TITLE: &str = "KEL142 Renderer Bridge Acceptance";
const RESPONSE_PATH: &str = "/KEL142_RENDERED_renderer-click_42";
/// Requested by the page right after its click listener is installed (GH-652).
const ARMED_PATH: &str = "/KEL142_ARMED";
/// Backoff of the beacon's nonblocking accept poll; the test's receive
/// deadlines, not this interval, bound each request.
const BEACON_ACCEPT_POLL: Duration = Duration::from_millis(10);
const BEACON_GIF: &[u8] = b"GIF89a\x01\x00\x01\x00\x80\x00\x00\x00\x00\x00\xff\xff\xff!\xf9\x04\x01\x00\x00\x00\x00,\x00\x00\x00\x00\x01\x00\x01\x00\x00\x02\x02D\x01\x00;";
// Reuse GH-652's notification drain and fresh front-to-back window census.
// The geometric candidate is a posting prerequisite, not delivery proof.
// Keep the existing 10-second helper budget; actual page/handler oracles and
// all existing failure-pipe ownership remain with the callers.
pub(crate) const CLICK_SCRIPT: &str = r#"
import AppKit
import CoreGraphics
import Foundation
let pid = pid_t(CommandLine.arguments[1])!
let title = CommandLine.arguments[2]
// Query the actual poster. An emitted CLICKED line alone proves no delivery.
if !CGPreflightPostEventAccess() {
  let message = "CLICK_POSTER_UNAVAILABLE poster=\(ProcessInfo.processInfo.processIdentifier) target=\(pid) post_event_access=false\n"
  FileHandle.standardError.write(Data(message.utf8))
  exit(5)
}
let deadline = Date().addingTimeInterval(10)
let clickPulseMicroseconds: useconds_t = 30_000
final class Active { var pid: pid_t = -1; var name = "none" }
let active = Active()
let workspace = NSWorkspace.shared
// Register before the first read, so an activation in between is still delivered.
let observer = workspace.notificationCenter.addObserver(
  forName: NSWorkspace.didActivateApplicationNotification, object: nil, queue: .main
) { note in
  let app = note.userInfo?[NSWorkspace.applicationUserInfoKey] as? NSRunningApplication
  active.pid = app?.processIdentifier ?? -1
  active.name = app?.localizedName ?? "unknown"
  // Ends the run-loop pass that delivered it, so a blocked wait re-checks now.
  CFRunLoopStop(CFRunLoopGetMain())
}
active.pid = workspace.frontmostApplication?.processIdentifier ?? -1
active.name = workspace.frontmostApplication?.localizedName ?? "none"
// Zero-timeout passes until one delivers nothing, so `active` is current.
func drainActivations() {
  var result: CFRunLoopRunResult
  repeat {
    if Date() >= deadline { return }
    result = CFRunLoopRunInMode(.defaultMode, 0, true)
  } while result == .handledSource || result == .stopped
}
func census() -> [[String: Any]] {
  CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID)
    as! [[String: Any]]
}
func int(_ row: [String: Any], _ key: CFString) -> Int { (row[key as String] as? NSNumber)?.intValue ?? -1 }
func frame(_ row: [String: Any]) -> CGRect {
  var rect = CGRect.zero
  precondition(CGRectMakeWithDictionaryRepresentation(row[kCGWindowBounds as String] as! CFDictionary, &rect))
  return rect
}
func hostWindow(_ rows: [[String: Any]]) -> [String: Any]? {
  rows.first { int($0, kCGWindowOwnerPID) == Int(pid) && $0[kCGWindowName as String] as? String == title && int($0, kCGWindowLayer) == 0 }
}
func hitAt(_ point: CGPoint, _ rows: [[String: Any]]) -> [String: Any]? {
  rows.first { int($0, kCGWindowLayer) == 0 && (($0[kCGWindowAlpha as String] as? NSNumber)?.doubleValue ?? 1) > 0 && frame($0).contains(point) }
}
func describe(_ row: [String: Any]?) -> String {
  guard let row else { return "none" }
  return "pid=\(int(row, kCGWindowOwnerPID)) owner=\((row[kCGWindowOwnerName as String] as? String) ?? "?") title=\((row[kCGWindowName as String] as? String) ?? "?")"
}
func failAtDeadline() -> Never {
  drainActivations()
  let rows = census()
  let host = hostWindow(rows)
  let hit = host.flatMap { hitAt(CGPoint(x: frame($0).midX, y: frame($0).midY), rows) }
  print("CLICK_TARGET_NOT_HOST active_pid=\(active.pid) active_name=\(active.name) hit=\(describe(hit)) host=\(describe(host))")
  exit(3)
}
while hostWindow(census()) == nil {
  if Date() >= deadline { print("NOT_PRESENTED pid=\(pid) title=\(title)"); exit(3) }
  sched_yield()
}
while true {
  drainActivations()
  if Date() >= deadline { failAtDeadline() }
  if active.pid != pid {
    if Date() >= deadline { failAtDeadline() }
    // Blocks until the next activation notification or the deadline.
    _ = CFRunLoopRunInMode(.defaultMode, deadline.timeIntervalSinceNow, true)
    continue
  }
  let rows = census()
  if let row = hostWindow(rows) {
    let point = CGPoint(x: frame(row).midX, y: frame(row).midY)
    if let hit = hitAt(point, rows), int(hit, kCGWindowNumber) == int(row, kCGWindowNumber) {
      // The deadline bounds selection/admission. Once down is admitted, always
      // release it; an OS scheduling delay must not leave pressed input behind.
      if deadline.timeIntervalSinceNow <= TimeInterval(clickPulseMicroseconds) / 1_000_000 { failAtDeadline() }
      CGEvent(mouseEventSource: nil, mouseType: .mouseMoved, mouseCursorPosition: point, mouseButton: .left)!.post(tap: .cghidEventTap)
      CGEvent(mouseEventSource: nil, mouseType: .leftMouseDown, mouseCursorPosition: point, mouseButton: .left)!.post(tap: .cghidEventTap)
      usleep(clickPulseMicroseconds)
      CGEvent(mouseEventSource: nil, mouseType: .leftMouseUp, mouseCursorPosition: point, mouseButton: .left)!.post(tap: .cghidEventTap)
      print("CLICKED window=\(int(row, kCGWindowNumber)) x=\(point.x) y=\(point.y) active=\(active.pid)")
      exit(0)
    }
  }
  if Date() >= deadline { failAtDeadline() }
  // Active but not yet the hit target: the next window-server census decides.
  sched_yield()
}
"#;

fn renderer_html(port: u16) -> String {
    format!(
        r#"<!doctype html>
<html>
<head>
<meta charset="utf-8">
<title>{TITLE}</title>
<style>
html,body{{margin:0;width:100%;height:100%;background:#111;color:#eee}}
#go{{position:fixed;inset:0;border:0;background:#163;color:white;font:28px system-ui}}
#result{{font:28px system-ui;padding:48px}}
</style>
<script>
"use strict";
const initial = {{
  present: !!window.keld && typeof window.keld.invoke === "function",
  frozen: Object.isFrozen(window.keld) && Object.isFrozen(window.keld?.invoke),
  descriptor: Object.getOwnPropertyDescriptor(window, "keld"),
  nativeHidden: window.webkit?.messageHandlers?.__keld_wv_link_v1 === undefined,
}};
function enc(message, count) {{
  const text = new TextEncoder().encode(message);
  return new Uint8Array([text.length, ...text, count]);
}}
function dec(bytes) {{
  const n = bytes[0];
  const message = new TextDecoder().decode(bytes.slice(1, 1 + n));
  return {{ message, count: bytes[1 + n] }};
}}
</script>
</head>
<body>
<button id="go">Run renderer → Bun Echo</button>
<div id="result"></div>
<script>
const iframe = document.createElement("iframe");
iframe.src = "about:blank";
iframe.hidden = true;
document.body.append(iframe);
go.addEventListener("click", async () => {{
  const subframeClean = iframe.contentWindow.keld === undefined;
  const descriptorSafe = initial.descriptor?.writable === false && initial.descriptor?.configurable === false;
  if (!initial.present || !initial.frozen || !initial.nativeHidden || !subframeClean || !descriptorSafe) {{
    result.textContent = "bridge-isolation-failed";
    return;
  }}

  let localRejects = 0;
  for (const attempt of [
    () => window.keld.invoke(2, new Uint8Array()),
    () => window.keld.invoke(1, new Uint8Array(), {{}}),
    () => window.keld.invoke(1, new Uint8Array(4097)),
  ]) {{
    try {{ await attempt(); }} catch {{ localRejects += 1; }}
  }}
  window.postMessage({{
    __keldRelay: "invoke-v1",
    request: 77,
    channel: 2,
    payload: [1],
  }}, "*");
  await new Promise((resolve) => setTimeout(resolve, 0));

  const source = new Uint8Array(enc("renderer-click", 42));
  const pending = window.keld.invoke(1, source);
  try {{
    await window.keld.invoke(1, new Uint8Array([1]));
  }} catch {{
    localRejects += 1;
  }}
  source.fill(0);
  const response = dec(await pending);
  if (localRejects !== 4) {{
    result.textContent = "local-rejection-contract-failed:" + localRejects;
    return;
  }}
  result.textContent = response.message + ":" + response.count;
  const pixel = document.createElement("img");
  pixel.alt = result.textContent;
  pixel.src = "http://127.0.0.1:{port}/KEL142_RENDERED_" +
    encodeURIComponent(response.message) + "_" + response.count;
  result.append(pixel);
}}, {{ once: true }});
// GH-652: an on-screen window does not mean this document has run. The
// harness posts its OS click only after this request reports the listener.
const armed = new Image();
armed.src = "http://127.0.0.1:{port}{ARMED_PATH}";
</script>
</body>
</html>
"#
    )
}

fn bundle_api_entry(project: &Path, repo: &Path) {
    let api = serde_json::to_string(
        &repo
            .join("packages/@keld/api/src/index.ts")
            .to_string_lossy(),
    )
    .expect("API import path JSON");
    let entry = format!(
        r#"import {{ app, channels, echoChannel }} from {api};
let handlerCalls = 0;
channels.handle(echoChannel, async (request) => {{
  handlerCalls += 1;
  console.error("KELD_KEL142_BUN_HANDLER call=" + handlerCalls + " message=" + JSON.stringify(request.message) + " count=" + request.count);
  return {{ message: request.message, count: request.count }};
}});
await app.whenReady();
console.error("KELD_KEL142_BUN_READY");
await new Promise(() => {{}});
"#
    );
    bundle_public_api_entry(project, repo, "kel142", &entry);
}

/// Uses the established public-index app composition and external transport.
/// Callers provide complete source, including their public `@keld/api` import.
pub(crate) fn bundle_public_api_entry(
    project: &Path,
    repo: &Path,
    fixture_name: &str,
    entry: &str,
) {
    let source = project.join(format!("src/{fixture_name}-entry.ts"));
    let output = project.join("src/main.ts");
    fs::write(&source, entry).expect("write public API fixture entry");
    // GH-527 §4.2: the transport is the transport Worker's entry, so it stays
    // its own staged file (`src/kipc-transport.ts`, which the dev stage copies
    // beside `src/main.ts`); the bundle imports it rather than inlining it.
    let build = project.join(format!("{fixture_name}-build.ts"));
    fs::write(&build, KIPC_SIDECAR_BUILD).expect("write KEL-142 bundle script");
    let result = Command::new("bun")
        .arg(&build)
        .arg(&source)
        .arg(&output)
        .output()
        .expect("bundle exact @keld/api fixture");
    assert!(result.status.success(), "bun build failed: {result:?}");
    fs::copy(
        repo.join("packages/@keld/kipc/src/transport.ts"),
        project.join("src/kipc-transport.ts"),
    )
    .expect("stage the canonical transport beside the bundle");
    let bundled = fs::read_to_string(&output).expect("read KEL-142 bundle");
    assert!(
        bundled.contains("from \"./kipc-transport.ts\"") && !bundled.contains("class WorkerLink"),
        "the bundle must import the staged transport, never inline it"
    );
    fs::remove_file(source).expect("remove bundle-only source");
    fs::remove_file(build).expect("remove bundle-only script");
}

/// Stages one ordinary source-package graph without bundling or copying KIPC
/// a second time. The API's declared dependency resolves to the same staged
/// KIPC package, distinct from the optional Host-preloaded transport sidecar.
pub(crate) fn copy_public_package_sources(repo: &Path, destination: &Path) {
    fn copy_directory(source: &Path, destination: &Path) {
        fs::create_dir_all(destination).expect("package source destination");
        for entry in fs::read_dir(source).expect("canonical package source directory") {
            let entry = entry.expect("canonical source entry");
            let target = destination.join(entry.file_name());
            if entry.file_type().expect("canonical source type").is_dir() {
                copy_directory(&entry.path(), &target);
            } else {
                fs::copy(entry.path(), &target).expect("byte-identical package source");
            }
        }
    }
    for package in ["api", "electron", "kipc"] {
        let original = repo.join(format!("packages/@keld/{package}"));
        let staged = destination.join(format!("@keld/{package}"));
        copy_directory(&original.join("src"), &staged.join("src"));
        fs::copy(original.join("package.json"), staged.join("package.json"))
            .expect("byte-identical package manifest");
    }
    let api: serde_json::Value = serde_json::from_slice(
        &fs::read(destination.join("@keld/api/package.json")).expect("staged API manifest"),
    )
    .expect("actual API manifest JSON");
    let kipc: serde_json::Value = serde_json::from_slice(
        &fs::read(destination.join("@keld/kipc/package.json")).expect("staged KIPC manifest"),
    )
    .expect("actual KIPC manifest JSON");
    assert_eq!(
        api["dependencies"]["@keld/kipc"], "link:../kipc",
        "the staged dependency must be the actual local SDK package edge"
    );
    assert_eq!(
        kipc["name"], "@keld/kipc",
        "the declared package edge must select canonical KIPC"
    );
    // The real package manager supplies this local, declared edge. The late
    // source-copy fixture must do the same rather than silently depend on the
    // caller's ambient node_modules or create another transport instance.
    let link_parent = destination.join("@keld/api/node_modules/@keld");
    fs::create_dir_all(&link_parent).expect("API dependency parent");
    let linked_kipc = link_parent.join("kipc");
    std::os::unix::fs::symlink("../../../kipc", &linked_kipc)
        .expect("one API-to-staged-KIPC package link");
    assert_eq!(
        fs::canonicalize(&linked_kipc).expect("resolved API KIPC package"),
        fs::canonicalize(destination.join("@keld/kipc")).expect("staged KIPC package"),
        "API must reuse its one staged KIPC package"
    );
}

/// Bundles an `@keld/api` entry with the kipc transport left external as
/// `./kipc-transport.ts` (GH-527 §4.2: the Worker's entry is that file).
const KIPC_SIDECAR_BUILD: &str =
    include_str!("../../../../../packages/@keld/api/fixtures/staged-api-bundle.ts");

/// The page's beacon server and the handles the test keeps for it.
struct RenderBeacon {
    port: u16,
    requests: mpsc::Receiver<Vec<u8>>,
    /// Dropping this ends the server thread at its next accept poll.
    stop: mpsc::Sender<()>,
    thread: thread::JoinHandle<()>,
}

/// Serves exactly the page's two requests, in arrival order: the armed listener
/// report, then the rendered Echo response. Accept is nonblocking, so the thread
/// never parks past the test: it ends once `stop` is dropped, and the test's
/// `recv_timeout(EVENT_DEADLINE)` bounds each request. A connection that closes
/// before sending a byte (a `WebKit` speculative preconnect) is ignored.
fn spawn_render_beacon() -> RenderBeacon {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind KEL-142 render beacon");
    let port = listener.local_addr().expect("render beacon address").port();
    listener
        .set_nonblocking(true)
        .expect("render beacon nonblocking accept");
    let (tx, requests) = mpsc::channel();
    let (stop, stopped) = mpsc::channel::<()>();
    let thread = thread::spawn(move || {
        let mut served = 0;
        while served < 2 {
            if let Err(mpsc::TryRecvError::Disconnected) = stopped.try_recv() {
                return;
            }
            match listener.accept() {
                Ok((stream, _)) => {
                    if let Some(request) = serve_beacon_request(stream) {
                        tx.send(request).expect("report render beacon");
                        served += 1;
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::park_timeout(BEACON_ACCEPT_POLL);
                }
                Err(error) => panic!("accept KEL-142 render beacon: {error}"),
            }
        }
    });
    RenderBeacon {
        port,
        requests,
        stop,
        thread,
    }
}

/// Reads one request's headers and answers with the beacon GIF. Returns `None`
/// for a connection that closed before sending any byte.
fn serve_beacon_request(mut stream: std::net::TcpStream) -> Option<Vec<u8>> {
    stream
        .set_nonblocking(false)
        .expect("render beacon blocking stream");
    stream
        .set_read_timeout(Some(EVENT_DEADLINE))
        .expect("render beacon deadline");
    let mut request = Vec::new();
    let mut chunk = [0_u8; 512];
    while !request.windows(4).any(|window| window == b"\r\n\r\n") {
        let read = stream.read(&mut chunk).expect("read render beacon");
        if read == 0 && request.is_empty() {
            return None;
        }
        assert_ne!(read, 0, "renderer beacon closed before headers");
        request.extend_from_slice(&chunk[..read]);
        assert!(
            request.len() <= 8192,
            "renderer beacon headers are unbounded"
        );
    }
    let header = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: image/gif\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        BEACON_GIF.len()
    );
    stream
        .write_all(header.as_bytes())
        .expect("beacon response header");
    stream.write_all(BEACON_GIF).expect("beacon response gif");
    Some(request)
}

/// Waits for the page's armed report; the window can be on screen before it.
fn expect_armed(requests: &mpsc::Receiver<Vec<u8>>) {
    let armed = requests
        .recv_timeout(EVENT_DEADLINE)
        .expect("renderer never reported its click listener armed");
    let armed = String::from_utf8_lossy(&armed);
    assert!(
        armed.starts_with(&format!("GET {ARMED_PATH} ")),
        "unexpected armed beacon: {armed}"
    );
}

#[test]
fn real_pointer_roundtrip_uses_isolated_bridge_and_keld_api_handler() {
    let fixture = ProductFixture::new("kel142-renderer-bridge");
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root");
    let RenderBeacon {
        port,
        requests: beacon_rx,
        stop: beacon_stop,
        thread: beacon_thread,
    } = spawn_render_beacon();

    bundle_api_entry(&fixture.project, repo);
    fs::write(fixture.project.join("index.html"), renderer_html(port)).expect("renderer HTML");
    fs::write(
        fixture.project.join("keld.config.ts"),
        format!(
            "export default {{\n  name: {TITLE:?},\n  entry: \"src/main.ts\",\n  renderer: \"index.html\",\n}} as const;\n"
        ),
    )
    .expect("renderer config");

    let stage = keld_cli::boot::stage_dev_boot(
        &fixture.project,
        Path::new(env!("CARGO_BIN_EXE_keld-host")),
    )
    .expect("stage KEL-142 real renderer fixture");
    let census = compile_native_window_census(fixture.root.path());
    let mut observer = NativeWindowObserver::arm_for_title(&census, TITLE);
    let child = Command::new(stage.host())
        .current_dir(stage.root())
        .env("KELD_DEV_LEASE", "stdin-v1")
        .env("KELD_KEL142_ACCEPTANCE_REPORT", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("launch KEL-142 staged host");
    let mut cleanup = ShippingLaunchCleanup::new(child);
    let host = cleanup.cli.as_mut().expect("KEL-142 host cleanup owner");
    let _lease = host.stdin.take().expect("retain dev lease");
    let host_pid = host.id();
    let mut click_output = None;
    // Retain the child owner and pipes outside the failed-oracle unwind.
    let observation = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let windows = observer.expect_initial(host_pid, "kel142-renderer-bridge");
        assert_eq!(windows.len(), 1, "one host-owned WKWebView window");
        // GH-652: click only once the page reports its listener installed.
        expect_armed(&beacon_rx);
        click_output = Some(
            Command::new("/usr/bin/xcrun")
                .args(["swift", "-e", CLICK_SCRIPT, &host_pid.to_string(), TITLE])
                .output()
                .expect("post real CoreGraphics pointer input"),
        );
        let click = click_output.as_ref().expect("click output retained");
        assert!(click.status.success(), "OS-visible click failed: {click:?}");
        let request = beacon_rx
            .recv_timeout(EVENT_DEADLINE)
            .expect("typed Echo response was not rendered into the beacon image");
        let request = String::from_utf8_lossy(&request);
        assert!(
            request.starts_with(&format!("GET {RESPONSE_PATH} ")),
            "unexpected rendered-response beacon: {request}"
        );
        beacon_thread.join().expect("render beacon joins");
        windows
    }));
    // Ends the beacon thread if the observation failed before both requests.
    drop(beacon_stop);
    let (windows, output) =
        finish_renderer_observation(cleanup, host_pid, click_output.as_ref(), observation);
    let click = click_output.expect("successful observation retained click output");
    assert!(
        output.status.success() || output.status.signal() == Some(9),
        "unexpected KEL-142 acceptance cleanup status: {output:?}"
    );
    let stderr = String::from_utf8(output.stderr).expect("KEL-142 stderr UTF-8");
    assert_single_admitted_roundtrip(&stderr);
    let click_evidence = String::from_utf8_lossy(&click.stdout);
    eprintln!(
        "KELD_KEL142_REAL_ACCEPTANCE host={host_pid} window={} path={RESPONSE_PATH} input={}",
        windows[0],
        click_evidence.trim()
    );
}

// The host's acceptance report must show exactly one bound, admitted renderer
// call reaching exactly one @keld/api handler invocation with the typed payload.
fn assert_single_admitted_roundtrip(stderr: &str) {
    assert!(stderr.contains("KELD_KEL142_BIND webview="), "{stderr}");
    assert_eq!(
        stderr
            .matches("KELD_KEL142_RENDERER_ADMIT webview=")
            .count(),
        1,
        "local rejects or forged relay reached host admission: {stderr}"
    );
    assert_eq!(
        stderr.matches("KELD_KEL142_KIPC_CALL webview=").count(),
        1,
        "renderer click produced more than one KIPC call: {stderr}"
    );
    assert_eq!(
        stderr.matches("KELD_KEL142_BUN_HANDLER").count(),
        1,
        "renderer negatives reached the @keld/api handler: {stderr}"
    );
    assert!(
        stderr.contains("KELD_KEL142_BUN_HANDLER call=1 message=\"renderer-click\" count=42"),
        "{stderr}"
    );
}

// Observation failure still fails the test after actual owned-child evidence is
// captured. Use the existing bounded pipe/reap owner, not a detached reader.
fn finish_renderer_observation(
    mut cleanup: ShippingLaunchCleanup,
    host_pid: u32,
    click: Option<&std::process::Output>,
    observation: std::thread::Result<Vec<u32>>,
) -> (Vec<u32>, std::process::Output) {
    if observation.is_err() {
        if let Some(click) = click {
            eprintln!(
                "KELD_KEL142_CLICK_EVIDENCE status={:?} stdout={:?} stderr={:?}",
                click.status,
                String::from_utf8_lossy(&click.stdout),
                String::from_utf8_lossy(&click.stderr),
            );
        } else {
            eprintln!("KELD_KEL142_CLICK_EVIDENCE poster_not_started=true");
        }
    }
    let child = cleanup.cli.as_mut().expect("KEL-142 host cleanup owner");
    if child.try_wait().expect("probe KEL-142 host exit").is_none() {
        child
            .kill()
            .expect("stop KEL-142 acceptance host after evidence");
    }
    let output = crate::support::control::wait_child_output(cleanup.release(), EVENT_DEADLINE);
    match observation {
        Ok(windows) => (windows, output),
        Err(failure) => {
            eprintln!(
                "KELD_KEL142_FAILURE_EVIDENCE host={host_pid} status={:?} stdout={:?} stderr={:?}",
                output.status,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr),
            );
            std::panic::resume_unwind(failure);
        }
    }
}

#[test]
fn failed_pointer_observation_preserves_child_and_click_evidence() {
    for (fault, marker) in [
        ("poster", "KEL652_POSTER_FAULT"),
        ("beacon", "KEL652_BEACON_FAULT"),
    ] {
        let child = Command::new(std::env::current_exe().expect("host test executable"))
            .args([
                "--exact",
                "renderer_bridge::pointer_failure_evidence_fixture_process",
                "--ignored",
                "--nocapture",
            ])
            .env("KEL652_EVIDENCE_FAULT", fault)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("run isolated failed-observation fixture");
        let output = crate::support::control::wait_child_output(child, EVENT_DEADLINE);
        assert!(
            !output.status.success(),
            "failed {fault} oracle became a pass"
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        for expected in [
            marker,
            "KELD_KEL142_CLICK_EVIDENCE status=",
            "KEL652_CLICK_STDOUT",
            "KEL652_CLICK_STDERR",
            "KELD_KEL142_FAILURE_EVIDENCE host=",
            "KEL652_CHILD_STDOUT",
            "KEL652_CHILD_STDERR",
        ] {
            assert!(stderr.contains(expected), "missing {expected}: {output:?}");
        }
    }
}

#[test]
#[ignore = "isolated failed-observation fixture invoked by its parent"]
fn pointer_failure_evidence_fixture_process() {
    let fault = std::env::var("KEL652_EVIDENCE_FAULT").expect("failure fixture mode");
    let click_exit = match fault.as_str() {
        "poster" => "5",
        "beacon" => "0",
        _ => panic!("unknown failure fixture mode: {fault}"),
    };
    let root = tempfile::tempdir().expect("failure-evidence fixture root");
    let ready = root.path().join("ready");
    let child = Command::new("/bin/sh")
        .args([
            "-c",
            "printf KEL652_CHILD_STDOUT; printf KEL652_CHILD_STDERR >&2; printf ready > \"$1\"; read hold",
            "kel652-evidence-child",
        ])
        .arg(&ready)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("launch pipe-evidence child");
    let cleanup = ShippingLaunchCleanup::new(child);
    let host_pid = cleanup.cli.as_ref().expect("child cleanup owner").id();
    let deadline = std::time::Instant::now() + EVENT_DEADLINE;
    while !ready.exists() {
        assert!(
            std::time::Instant::now() < deadline,
            "pipe child never became ready"
        );
        thread::yield_now();
    }
    let click = Command::new("/bin/sh")
        .args([
            "-c",
            "printf KEL652_CLICK_STDOUT; printf KEL652_CLICK_STDERR >&2; exit \"$1\"",
            "kel652-evidence-click",
        ])
        .arg(click_exit)
        .output()
        .expect("run failed poster fixture");
    let observation = std::panic::catch_unwind(|| {
        assert!(click.status.success(), "KEL652_POSTER_FAULT");
        let (_tx, rx) = mpsc::channel::<Vec<u8>>();
        rx.recv_timeout(std::time::Duration::from_millis(1))
            .expect("KEL652_BEACON_FAULT");
        Vec::new()
    });
    let _ = finish_renderer_observation(cleanup, host_pid, Some(&click), observation);
}

#[test]
fn staged_source_packages_resolve_one_canonical_kipc_and_refuse_a_missing_edge() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root");
    let root = tempfile::tempdir().expect("isolated staged package graph");
    copy_public_package_sources(repo, root.path());
    let api = root.path().join("@keld/api");
    let probe = || {
        let child = Command::new("bun")
            .args(["--no-install", "--eval", r#"
import { strict as assert } from "node:assert";
import { realpathSync } from "node:fs";
import { fileURLToPath } from "node:url";
import * as declared from "@keld/kipc";
import * as sibling from "../kipc/src/transport.ts";
import * as apiLink from "./src/link.ts";
assert.equal(realpathSync(fileURLToPath(import.meta.resolve("@keld/kipc"))), realpathSync("../kipc/src/transport.ts"));
assert.equal(declared, sibling);
assert.equal(declared.WorkerLink, sibling.WorkerLink);
for (const name of ["FrameReader", "DrainSignal", "RECEIVE_POLICIES", "isCallError"])
  assert.equal(apiLink[name], declared[name], "staged API must consume canonical KIPC");
console.log("CANONICAL_STAGED_KIPC");
"#])
            .current_dir(&api)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("execute actual staged package imports");
        crate::support::control::wait_child_output(child, EVENT_DEADLINE)
    };
    let intact = probe();
    assert!(intact.status.success(), "staged import failed: {intact:?}");
    assert_eq!(intact.stdout, b"CANONICAL_STAGED_KIPC\n");
    for package in ["api", "electron", "kipc"] {
        assert_eq!(
            fs::read(repo.join(format!("packages/@keld/{package}/package.json")))
                .expect("original manifest"),
            fs::read(root.path().join(format!("@keld/{package}/package.json")))
                .expect("staged manifest"),
            "staging must preserve the actual source manifest"
        );
    }
    // The sole mutation removes the declared dependency, leaving actual source
    // files intact. No ambient package or alternate transport may hide it.
    let edge = api.join("node_modules/@keld/kipc");
    let removed = api.join("node_modules/@keld/kipc-removed");
    fs::rename(&edge, &removed).expect("remove only the declared package link");
    let missing = probe();
    assert!(
        !missing.status.success(),
        "missing package edge was accepted"
    );
    assert!(
        String::from_utf8_lossy(&missing.stderr).contains("@keld/kipc"),
        "missing edge failed for an unrelated reason: {missing:?}"
    );
    fs::rename(&removed, &edge).expect("restore original package link");
    let restored = probe();
    assert!(
        restored.status.success(),
        "restored graph failed: {restored:?}"
    );
    assert_eq!(restored.stdout, b"CANONICAL_STAGED_KIPC\n");
}

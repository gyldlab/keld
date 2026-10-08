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

const TITLE: &str = "KEL142 Renderer Bridge Acceptance";
const RESPONSE_PATH: &str = "/KEL142_RENDERED_renderer-click_42";
const BEACON_GIF: &[u8] = b"GIF89a\x01\x00\x01\x00\x80\x00\x00\x00\x00\x00\xff\xff\xff!\xf9\x04\x01\x00\x00\x00\x00,\x00\x00\x00\x00\x01\x00\x01\x00\x00\x02\x02D\x01\x00;";
const CLICK_SCRIPT: &str = r#"
import CoreGraphics
import Foundation
let pid = Int(CommandLine.arguments[1])!
let title = CommandLine.arguments[2]
let deadline = Date().addingTimeInterval(10)
while true {
  let rows = CGWindowListCopyWindowInfo(
    [.optionOnScreenOnly, .excludeDesktopElements],
    kCGNullWindowID
  ) as! [[String: Any]]
  if let row = rows.first(where: {
    ($0[kCGWindowOwnerPID as String] as? NSNumber)?.intValue == pid &&
    $0[kCGWindowName as String] as? String == title &&
    ($0[kCGWindowLayer as String] as? NSNumber)?.intValue == 0
  }) {
    let bounds = row[kCGWindowBounds as String] as! CFDictionary
    var rect = CGRect.zero
    precondition(CGRectMakeWithDictionaryRepresentation(bounds, &rect))
    let point = CGPoint(x: rect.midX, y: rect.midY)
    CGEvent(mouseEventSource: nil, mouseType: .mouseMoved, mouseCursorPosition: point, mouseButton: .left)!.post(tap: .cghidEventTap)
    CGEvent(mouseEventSource: nil, mouseType: .leftMouseDown, mouseCursorPosition: point, mouseButton: .left)!.post(tap: .cghidEventTap)
    usleep(30_000)
    CGEvent(mouseEventSource: nil, mouseType: .leftMouseUp, mouseCursorPosition: point, mouseButton: .left)!.post(tap: .cghidEventTap)
    print("CLICKED window=\((row[kCGWindowNumber as String] as! NSNumber).uint32Value) x=\(point.x) y=\(point.y)")
    exit(0)
  }
  if Date() >= deadline { exit(3) }
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
</script>
</body>
</html>
"#
    )
}

fn bundle_api_entry(project: &Path, repo: &Path) {
    let source = project.join("src/kel142-entry.ts");
    let output = project.join("src/main.ts");
    let api = serde_json::to_string(
        &repo
            .join("packages/@keld/api/src/index.ts")
            .to_string_lossy(),
    )
    .expect("API import path JSON");
    fs::write(
        &source,
        format!(
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
        ),
    )
    .expect("write KEL-142 API entry");
    // GH-527 §4.2: the transport is the transport Worker's entry, so it stays
    // its own staged file (`src/kipc-transport.ts`, which the dev stage copies
    // beside `src/main.ts`); the bundle imports it rather than inlining it.
    let build = project.join("kel142-build.ts");
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

/// Bundles an `@keld/api` entry with the kipc transport left external as
/// `./kipc-transport.ts` (GH-527 §4.2: the Worker's entry is that file).
const KIPC_SIDECAR_BUILD: &str = r#"const [entry, out] = process.argv.slice(2);
const result = await Bun.build({
  entrypoints: [entry],
  target: "bun",
  format: "esm",
  plugins: [{
    name: "kipc-transport-sidecar",
    setup(build) {
      build.onResolve({ filter: /[\\/]kipc[\\/]src[\\/]transport\.ts$/ }, () => ({
        path: "./kipc-transport.ts",
        external: true,
      }));
    },
  }],
});
if (!result.success) {
  console.error(result.logs);
  process.exit(1);
}
await Bun.write(out, result.outputs[0]);
"#;

fn spawn_render_beacon() -> (u16, mpsc::Receiver<Vec<u8>>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind KEL-142 render beacon");
    let port = listener.local_addr().expect("render beacon address").port();
    let (tx, rx) = mpsc::channel();
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept KEL-142 render beacon");
        stream
            .set_read_timeout(Some(EVENT_DEADLINE))
            .expect("render beacon deadline");
        let mut request = Vec::new();
        let mut chunk = [0_u8; 512];
        while !request.windows(4).any(|window| window == b"\r\n\r\n") {
            let read = stream.read(&mut chunk).expect("read render beacon");
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
        tx.send(request).expect("report render beacon");
    });
    (port, rx, handle)
}

#[test]
fn real_pointer_roundtrip_uses_isolated_bridge_and_keld_api_handler() {
    let fixture = ProductFixture::new("kel142-renderer-bridge");
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root");
    let (port, beacon_rx, beacon_thread) = spawn_render_beacon();

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
    let windows = observer.expect_initial(host_pid, "kel142-renderer-bridge");
    assert_eq!(windows.len(), 1, "one host-owned WKWebView window");

    let click = Command::new("/usr/bin/xcrun")
        .args(["swift", "-e", CLICK_SCRIPT, &host_pid.to_string(), TITLE])
        .output()
        .expect("post real CoreGraphics pointer input");
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

    let mut child = cleanup.release();
    if child.try_wait().expect("probe KEL-142 host exit").is_none() {
        child
            .kill()
            .expect("stop KEL-142 acceptance host after evidence");
    }
    let output = crate::support::control::wait_child_output(child, EVENT_DEADLINE);
    assert!(
        output.status.success() || output.status.signal() == Some(9),
        "unexpected KEL-142 acceptance cleanup status: {output:?}"
    );
    let stderr = String::from_utf8(output.stderr).expect("KEL-142 stderr UTF-8");
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
    let click_evidence = String::from_utf8_lossy(&click.stdout);
    eprintln!(
        "KELD_KEL142_REAL_ACCEPTANCE host={host_pid} window={} path={RESPONSE_PATH} input={}",
        windows[0],
        click_evidence.trim()
    );
}

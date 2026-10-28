//! Real shipping no-flag Host acceptance for schema-two rendererless Ready.
//! This deliberately reuses the existing authenticated HELLO / echo / Quit
//! fixture, changing only the compiler-selected initial-window declaration.

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
use std::time::{Duration, Instant};

fn assert_zero_native_app_windows(host_pid: u32) {
    // The title-filtered positive-window observer cannot prove zero. Query
    // every CoreGraphics window for the exact PID, including hidden/untitled
    // windows; unavailable Screen Recording is never interpreted as empty.
    let rows = native_window_rows(host_pid);
    let observed: serde_json::Value =
        serde_json::from_str(&rows).expect("native process-wide census is available");
    assert_eq!(observed["capture_preflight"], true, "{rows}");
    assert_eq!(observed["pid"], host_pid);
    let app_windows: Vec<_> = observed["rows"]
        .as_array()
        .expect("native window rows")
        .iter()
        .filter(|row| row["layer"] == 0)
        .collect();
    assert!(
        app_windows.is_empty(),
        "facade Ready allocated an app window: {rows}"
    );
}

#[test]
fn shipping_schema_two_host_emits_ready_without_an_initial_native_window() {
    let fixture = ProductFixture::new("facade-ready");
    fs::write(fixture.project.join("src/main.ts"), &fixture.entry_source)
        .expect("existing authenticated raw lifecycle fixture");
    fs::write(
        fixture.project.join("keld.config.ts"),
        "export default { name: \"KELD facade Ready\", entry: \"src/main.ts\", initialWindow: \"none\" } as const;\n",
    )
    .expect("explicit facade config");
    let stage = keld_cli::boot::stage_dev_boot(
        &fixture.project,
        Path::new(env!("CARGO_BIN_EXE_keld-host")),
    )
    .expect("compile rendererless stage");
    assert!(!stage.root().join("index.html").exists());
    let descriptor: serde_json::Value = serde_json::from_slice(
        &fs::read(stage.root().join("keld.boot.json")).expect("staged descriptor"),
    )
    .expect("strict descriptor JSON");
    assert_eq!(descriptor["schema"], 2);
    assert!(descriptor.get("renderer").is_none());

    let control_path = fixture.root.path().join("facade-ready.sock");
    let listener = UnixListener::bind(&control_path).expect("test-only control");
    listener
        .set_nonblocking(true)
        .expect("bounded nonblocking accept");
    let child = Command::new(stage.host())
        .process_group(0)
        .current_dir(stage.root())
        .env("KELD_T1B_CONTROL", &control_path)
        .env("KELD_T4_SKIP_DESCENDANT", "1")
        .env("KELD_DEV_LEASE", "stdin-v1")
        .env("KELD_T2_EXIT_ON_LINK_EOF", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("launch shipping no-flag host");
    let mut cleanup = ShippingLaunchCleanup::new(child);
    let host = cleanup.cli.as_mut().expect("live host");
    let host_pid = host.id();
    let _lease = host.stdin.take().expect("retain host dev lease");
    let control = accept_before(&listener, Instant::now() + EVENT_DEADLINE);
    control
        .set_read_timeout(Some(EVENT_DEADLINE))
        .expect("bound lifecycle records");
    let mut reader = BufReader::new(control.try_clone().expect("control reader"));
    let mut writer = control;
    let hello = read_control_line(&mut reader);
    let parts: Vec<_> = hello.split_whitespace().collect();
    assert_eq!(parts.first().copied(), Some("HELLO"), "{hello}");
    let bun_pid = parse_pid(parts.get(1).copied(), &hello);
    assert!(parts.get(2).is_some_and(|locator| locator.contains('#')));
    cleanup.record_authenticated_groups(process_group(host_pid), process_group(bun_pid));
    assert_eq!(read_control_line(&mut reader), "DESCENDANT 0");
    assert_eq!(read_control_line(&mut reader), "READY");
    assert_eq!(read_control_line(&mut reader), "ECHO1");
    assert_eq!(read_control_line(&mut reader), "ECHO2");

    assert_zero_native_app_windows(host_pid);

    // This is a duration oracle, not a synchronization sleep: an Init that
    // already happened disarms the entire startup deadline, even with no view.
    std::thread::sleep(keld_wv::INITIAL_MILESTONE_DEADLINE + Duration::from_secs(1));
    assert!(
        cleanup
            .cli
            .as_mut()
            .expect("live host")
            .try_wait()
            .expect("inspect live windowless host")
            .is_none(),
        "the already-Init windowless Host exited at the old startup deadline"
    );
    assert_zero_native_app_windows(host_pid);

    writer
        .write_all(b"QUIT\n")
        .expect("request authenticated app Quit");
    assert_eq!(read_control_line(&mut reader), "QUIT_REPLY");
    assert_eq!(read_control_line(&mut reader), "LINK_EOF");
    drop(writer);
    let output = wait_child_output(cleanup.release(), EVENT_DEADLINE);
    assert!(
        output.status.success(),
        "native facade host failed: {output:?}"
    );
    await_process_gone(bun_pid);
    assert!(
        !stage.root().exists(),
        "successful facade Host did not clean the stage after native release"
    );
}

/// AC15/AC20: a real child stalls the main thread after the native-loop entry
/// callback arms the deadline, before tao can deliver its Init. The coordinator
/// must independently end the process. This fixture is absent from shipping
/// release binaries and cannot be mistaken for a real AppKit scheduling stall.
#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
#[test]
fn shipping_schema_two_stalled_init_exits_through_the_binary_terminal_once() {
    use std::io::Read as _;

    let fixture = ProductFixture::new("facade-stalled-init");
    fs::write(fixture.project.join("src/main.ts"), &fixture.entry_source)
        .expect("authenticated raw lifecycle source");
    fs::write(
        fixture.project.join("keld.config.ts"),
        "export default { name: 'Stalled Init', entry: 'src/main.ts', initialWindow: 'none' } as const;\n",
    )
    .expect("facade boot selection");
    let stage = keld_cli::boot::stage_dev_boot(
        &fixture.project,
        Path::new(env!("CARGO_BIN_EXE_keld-host")),
    )
    .expect("compile private rendererless stage");
    let control_path = fixture.root.path().join("stalled-init.sock");
    let listener = UnixListener::bind(&control_path).expect("bind child control");
    listener.set_nonblocking(true).expect("nonblocking accept");
    let child = Command::new(stage.host())
        .process_group(0)
        .current_dir(stage.root())
        .env("KELD_T1B_CONTROL", &control_path)
        .env("KELD_T4_SKIP_DESCENDANT", "1")
        .env("KELD_T1B_STALL_BEFORE_INIT", "1")
        .env("KELD_DEV_LEASE", "stdin-v1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("launch shipping Host debug probe");
    let mut cleanup = ShippingLaunchCleanup::new(child);
    let host = cleanup.cli.as_mut().expect("owned child host");
    let host_pid = host.id();
    let _lease = host.stdin.take().expect("hold lease through timeout");
    let control = accept_before(&listener, Instant::now() + EVENT_DEADLINE);
    control
        .set_read_timeout(Some(EVENT_DEADLINE + Duration::from_secs(10)))
        .expect("bounded pre-Init control");
    let mut reader = BufReader::new(control);
    let hello = read_control_line(&mut reader);
    let parts: Vec<_> = hello.split_whitespace().collect();
    assert_eq!(parts.first().copied(), Some("HELLO"), "{hello}");
    let bun_pid = parse_pid(parts.get(1).copied(), &hello);
    cleanup.record_authenticated_groups(process_group(host_pid), process_group(bun_pid));
    assert_eq!(read_control_line(&mut reader), "DESCENDANT 0");
    // No readiness token is permitted while the genuine Tao Init is absent.
    let mut remaining = String::new();
    reader
        .read_to_string(&mut remaining)
        .expect("observe child EOF");
    assert!(
        !remaining.lines().any(|line| line == "READY"),
        "{remaining}"
    );

    let output = wait_child_output(cleanup.release(), EVENT_DEADLINE + Duration::from_secs(10));
    assert!(!output.status.success(), "stalled Init exited successfully");
    let stderr = String::from_utf8(output.stderr).expect("host error UTF-8");
    assert_eq!(
        stderr.matches("KELD_T1B_TERMINAL_ENTERED").count(),
        1,
        "{stderr}"
    );
    assert_eq!(
        stderr
            .matches("the UI loop did not deliver its first turn within 15 s")
            .count(),
        1,
        "{stderr}"
    );
    assert!(stderr.contains("KELD-CORE-037"), "{stderr}");
    assert!(!stderr.contains("COORDINATOR_RETURNED"), "{stderr}");
    assert!(!remaining.contains("READY"), "stalled Init published Ready");
    assert!(
        stage.root().exists(),
        "unproved native custody deleted the dev stage"
    );
    await_process_gone(bun_pid);
}

/// Real Bun entry follows the public Electron singleton, not raw lifecycle
/// framing. It must remain a separate source graph from the raw-host fixture.
const ELECTRON_FACADE_READY_ENTRY: &str = r#"
const previouslyReady = app.isReady();
let readyEvents = 0;
app.on("ready", () => { readyEvents += 1; });
await app.whenReady();
if (previouslyReady || !app.isReady() || readyEvents !== 1) {
  throw new Error("facade Ready/whenReady/isReady disagreement " +
    JSON.stringify({ previouslyReady, readyNow: app.isReady(), readyEvents }));
}
const controlPath = process.env.KELD_T1B_CONTROL;
if (!controlPath) throw new Error("missing test-only control path");
let buffered = "";
const socket = await Bun.connect({
  unix: controlPath,
  socket: {
    binaryType: "uint8array",
    data(socket, chunk) {
      buffered += new TextDecoder().decode(chunk);
      if (!buffered.includes("QUIT\n")) return;
      buffered = "";
      void app.quit().then(() => {
        socket.write(new TextEncoder().encode("QUIT_REPLY\n"));
      }).catch((error) => {
        socket.write(new TextEncoder().encode("QUIT_ERROR " + String(error) + "\n"));
        process.exitCode = 1;
      });
    },
    drain() {},
    error(_socket, error) { throw error; },
  },
});
socket.write(new TextEncoder().encode(
  "FACADE_READY " + process.pid + " eventCount=" + readyEvents + "\n"
));
await new Promise(() => {});
"#;

#[test]
fn electron_facade_ready_and_when_ready_agree_on_the_real_zero_window_host() {
    let fixture = ProductFixture::new("facade-public-ready");
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("keld-host beneath workspace");
    let electron = serde_json::to_string(
        &repo
            .join("packages/@keld/electron/src/index.ts")
            .to_string_lossy(),
    )
    .expect("absolute Electron facade import");
    let source = format!("import {{ app }} from {electron};\n{ELECTRON_FACADE_READY_ENTRY}");
    crate::renderer_bridge::bundle_public_api_entry(
        &fixture.project,
        repo,
        "gh446-facade",
        &source,
    );
    fs::write(
        fixture.project.join("keld.config.ts"),
        "export default { name: \"KELD facade public Ready\", entry: \"src/main.ts\", initialWindow: \"none\" } as const;\n",
    )
    .expect("facade config");
    let stage = keld_cli::boot::stage_dev_boot(
        &fixture.project,
        Path::new(env!("CARGO_BIN_EXE_keld-host")),
    )
    .expect("compile facade stage");
    assert!(!stage.root().join("index.html").exists());
    let path = fixture.root.path().join("facade-public-ready.sock");
    let listener = UnixListener::bind(&path).expect("public-ready control");
    listener
        .set_nonblocking(true)
        .expect("bounded control accept");
    let child = Command::new(stage.host())
        .process_group(0)
        .current_dir(stage.root())
        .env("KELD_T1B_CONTROL", &path)
        .env("KELD_DEV_LEASE", "stdin-v1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("launch actual no-flag facade Host");
    let mut cleanup = ShippingLaunchCleanup::new(child);
    let host = cleanup.cli.as_mut().expect("live Host");
    let host_pid = host.id();
    let _lease = host.stdin.take().expect("retain Host dev lease");
    let control = accept_before(&listener, Instant::now() + EVENT_DEADLINE);
    control
        .set_read_timeout(Some(EVENT_DEADLINE))
        .expect("bounded facade control");
    let mut reader = BufReader::new(control.try_clone().expect("control reader"));
    let mut writer = control;
    let ready = read_control_line(&mut reader);
    let fields: Vec<_> = ready.split_whitespace().collect();
    assert_eq!(fields.first().copied(), Some("FACADE_READY"), "{ready}");
    let bun_pid = parse_pid(fields.get(1).copied(), &ready);
    assert_eq!(fields.get(2).copied(), Some("eventCount=1"), "{ready}");
    cleanup.record_authenticated_groups(process_group(host_pid), process_group(bun_pid));
    assert_zero_native_app_windows(host_pid);

    writer.write_all(b"QUIT\n").expect("facade Quit request");
    assert_eq!(read_control_line(&mut reader), "QUIT_REPLY");
    drop(writer);
    let output = wait_child_output(cleanup.release(), EVENT_DEADLINE);
    assert!(output.status.success(), "facade Host failed: {output:?}");
    await_process_gone(bun_pid);
}

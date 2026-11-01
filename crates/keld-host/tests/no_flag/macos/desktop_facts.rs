//! Actual Host Init -> desktop replay -> public Ready -> first native window.
//! `AppKit` is observed independently; this is not an original draw.io acceptance.
use crate::renderer_bridge::bundle_public_api_entry;
use crate::support::EVENT_DEADLINE;
use crate::support::control::{accept_before, parse_pid, read_control_line, wait_child_output};
use crate::support::dev_cycle::ShippingLaunchCleanup;
use crate::support::native_window::{
    NativeWindowObserver, compile_native_window_census, native_window_rows,
};
use crate::support::process::{await_process_gone, process_group};
use crate::support::product::ProductFixture;
use std::fs;
use std::io::{BufReader, Write};
use std::os::unix::net::UnixListener;
use std::os::unix::process::CommandExt as _;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Instant;

fn independent_desktop(root: &Path) -> serde_json::Value {
    let script = root.join("desktop-observer.swift");
    fs::write(
        &script,
        include_str!("../../fixtures/desktop_observer.swift"),
    )
    .expect("owned independent native observer");
    let child = Command::new("/usr/bin/xcrun")
        .arg("swift")
        .arg(&script)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("run independent AppKit observer");
    let output = wait_child_output(child, EVENT_DEADLINE);
    assert!(
        output.status.success(),
        "native observer failed: {output:?}"
    );
    assert!(
        output.stderr.is_empty(),
        "native observer diagnosed failure: {output:?}"
    );
    serde_json::from_slice(&output.stdout).expect("independent native desktop JSON")
}

#[test]
fn shipping_desktop_facts_match_appkit_before_first_native_window() {
    let fixture = ProductFixture::new("gh481-desktop-native");
    let expected = independent_desktop(fixture.root.path());
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root");
    let facade = serde_json::to_string(&repo.join("packages/@keld/electron/src/index.ts"))
        .expect("actual public facade import");
    let source = format!(
        "import {{ app, BrowserWindow, screen, nativeTheme }} from {facade};\n{}",
        include_str!("../../fixtures/desktop_facts_role.ts"),
    );
    bundle_public_api_entry(&fixture.project, repo, "gh481-desktop", &source);
    fs::write(fixture.project.join("keld.config.ts"),
        "export default { name: 'GH481 desktop fixture', entry: 'src/main.ts', initialWindow: 'none' } as const;\n")
        .expect("explicit rendererless fixture");
    let stage = keld_cli::boot::stage_dev_boot(
        &fixture.project,
        Path::new(env!("CARGO_BIN_EXE_keld-host")),
    )
    .expect("shipping source producer");
    let path = fixture.root.path().join("desktop.sock");
    let listener = UnixListener::bind(&path).expect("owned control socket");
    listener.set_nonblocking(true).expect("bounded accept");
    let census = compile_native_window_census(fixture.root.path());
    // Registry-created windows currently use the native owner's fixed title;
    // the project name is a launch fact, not a window-title implementation.
    let mut presentation = NativeWindowObserver::arm_for_title(&census, "Keld");
    let child = Command::new(stage.host())
        .process_group(0)
        .current_dir(stage.root())
        .env("KELD_T1B_CONTROL", &path)
        .env("KELD_DEV_LEASE", "stdin-v1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("shipping no-flag Host");
    let mut cleanup = ShippingLaunchCleanup::new(child);
    let host = cleanup.cli.as_mut().expect("live Host");
    let host_pid = host.id();
    let lease = host.stdin.take().expect("retained dev lease");
    let control = accept_before(&listener, Instant::now() + EVENT_DEADLINE);
    control
        .set_read_timeout(Some(EVENT_DEADLINE))
        .expect("bounded control records");
    let mut reader = BufReader::new(control.try_clone().expect("control reader"));
    let mut writer = control;
    let line = read_control_line(&mut reader);
    let mut fields = line.splitn(3, ' ');
    assert_eq!(fields.next(), Some("DESKTOP"), "{line}");
    let bun_pid = parse_pid(fields.next(), &line);
    cleanup.record_authenticated_groups(process_group(host_pid), process_group(bun_pid));
    let observed: serde_json::Value =
        serde_json::from_str(fields.next().expect("desktop data")).expect("facade desktop JSON");
    assert_eq!(
        observed, expected,
        "Host desktop differs from independent AppKit capture"
    );
    let rows = native_window_rows(host_pid);
    let rows: serde_json::Value = serde_json::from_str(&rows).expect("native window census");
    assert_eq!(
        rows["capture_preflight"], true,
        "census is not available: {rows}"
    );
    assert_eq!(rows["pid"], host_pid);
    assert!(
        rows["rows"]
            .as_array()
            .expect("native rows")
            .iter()
            .all(|row| row["layer"] != 0),
        "desktop/Ready created a hidden or visible native window: {rows}"
    );
    writer
        .write_all(b"CREATE\n")
        .expect("first app-owned window");
    assert_eq!(read_control_line(&mut reader), "CREATED 1");
    let native_ids = presentation.expect_initial(host_pid, "desktop-before-first-window");
    assert_eq!(native_ids.len(), 1);
    writer.write_all(b"QUIT\n").expect("real Destroy then Quit");
    assert_eq!(read_control_line(&mut reader), "QUIT_REPLY");
    drop(writer);
    let output = wait_child_output(cleanup.release(), EVENT_DEADLINE);
    drop(lease);
    assert!(
        output.status.success(),
        "native desktop Host failed: {output:?}"
    );
    await_process_gone(bun_pid);
    assert!(
        !stage.root().exists(),
        "Host left its private source stage behind"
    );
    assert_eq!(
        independent_desktop(fixture.root.path()),
        expected,
        "desktop changed during the test; before/after qualification is unavailable"
    );
    eprintln!(
        "GH481_NATIVE_DESKTOP host={host_pid} bun={bun_pid} native_windows={native_ids:?} facts={observed}"
    );
}

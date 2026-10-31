//! GH1037 verifies today's multi-file limits without changing staging behavior.
//! The two native cases are independent: a missing Bun import cannot suppress
//! the valid role used to observe relative renderer assets. Neither needs focus,
//! a physical click, accessibility input or a foreground-window oracle.

#[path = "multifile_staging/fixture.rs"]
mod fixture;

use crate::support::EVENT_DEADLINE;
use crate::support::control::{accept_before, parse_pid, read_control_line, wait_child_output};
use crate::support::dev_cycle::ShippingLaunchCleanup;
use crate::support::process::{
    await_process_gone, parent_process, process_group, session_dirs_for,
};
use crate::support::renderer::Beacon;
use std::io::{BufReader, Write as _};
use std::os::unix::net::UnixListener;
use std::os::unix::process::CommandExt as _;
use std::process::{Command, Stdio};
use std::time::Instant;

#[test]
fn shipping_stage_reports_missing_sibling_import_before_app_entry_evaluates() {
    let mut fixture = fixture::project("gh1037-sibling");
    fixture::assert_original_inputs_exist(&fixture);
    fixture.entry_source = format!(
        "import {{ siblingMarker }} from \"./sibling.ts\";\nconsole.error(siblingMarker);\n{}",
        fixture.entry_source
    );
    let stage = fixture.stage();
    assert!(
        !stage.root().join("src/sibling.ts").exists(),
        "update this explicit current-limit fixture when multi-file staging is implemented"
    );
    let mut host = Command::new(stage.host())
        .process_group(0)
        .current_dir(stage.root())
        .env("KELD_DEV_LEASE", "stdin-v1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("shipping no-flag Host for a real sibling import");
    let lease = host
        .stdin
        .take()
        .expect("retain dev lease until Host exits");
    let host_pid = host.id();
    let output = wait_child_output(host, EVENT_DEADLINE);
    drop(lease);
    let stderr = String::from_utf8(output.stderr).expect("Host/Bun stderr UTF-8");
    eprintln!(
        "GH1037_SIBLING host={host_pid} status={}\n{stderr}",
        output.status
    );
    assert!(
        !output.status.success(),
        "missing sibling became Host success"
    );
    assert!(
        stderr.contains("Cannot find module './sibling.ts'"),
        "{stderr}"
    );
    assert!(stderr.contains("KELD-CORE-037"), "{stderr}");
    assert!(
        !stderr.contains("GH1037_SIBLING_EVALUATED"),
        "the application entry or missing sibling evaluated: {stderr}"
    );
    assert!(
        session_dirs_for(host_pid).is_empty(),
        "Host left an app-link locator"
    );
    assert!(
        !stage.root().exists(),
        "failed startup left its private stage"
    );
}

#[test]
fn shipping_stage_reports_relative_asset_failures_with_live_renderer_controls() {
    let fixture = fixture::project("gh1037-renderer-assets");
    fixture::assert_original_inputs_exist(&fixture);
    let mut beacon = Beacon::bind("GH1037_ASSETS");
    fixture::renderer(&fixture, beacon.port());
    // This role has no sibling import, so the first case's failure cannot mask
    // the renderer observation. It uses the existing canonical lifecycle fixture.
    let stage = fixture.stage();
    let control_path = fixture.root.path().join("assets.sock");
    let listener = UnixListener::bind(&control_path).expect("own fixture control");
    listener
        .set_nonblocking(true)
        .expect("bounded fixture accept");
    let child = Command::new(stage.host())
        .process_group(0)
        .current_dir(stage.root())
        .env("KELD_DEV_LEASE", "stdin-v1")
        .env("KELD_T1B_CONTROL", &control_path)
        .env("KELD_T4_SKIP_DESCENDANT", "1")
        .env("KELD_T2_EXIT_ON_LINK_EOF", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("shipping no-flag Host for native asset observation");
    let mut cleanup = ShippingLaunchCleanup::new(child);
    let host = cleanup.cli.as_mut().expect("retained Host cleanup owner");
    let host_pid = host.id();
    let lease = host
        .stdin
        .take()
        .expect("retain dev lease through Host exit");
    let control = accept_before(&listener, Instant::now() + EVENT_DEADLINE);
    control
        .set_read_timeout(Some(EVENT_DEADLINE))
        .expect("bounded control read");
    let mut reader = BufReader::new(control.try_clone().expect("fixture reader"));
    let mut writer = control;
    let hello = read_control_line(&mut reader);
    let fields: Vec<_> = hello.split_whitespace().collect();
    assert_eq!(fields.first().copied(), Some("HELLO"), "{hello}");
    let bun_pid = parse_pid(fields.get(1).copied(), &hello);
    let guardian_pid = parent_process(bun_pid);
    cleanup.record_authenticated_groups(process_group(host_pid), process_group(bun_pid));
    for expected in ["DESCENDANT 0", "READY", "ECHO1", "ECHO2"] {
        assert_eq!(read_control_line(&mut reader), expected);
    }

    let request = beacon
        .request
        .recv_timeout(EVENT_DEADLINE)
        .expect("the native renderer must report actual CSS/image effects");
    let request = String::from_utf8(request).expect("renderer observation UTF-8");
    let request_line = request.lines().next().expect("renderer request line");
    eprintln!(
        "GH1037_ASSETS host={host_pid} guardian={guardian_pid} bun={bun_pid} staged_css={} staged_image={} {request_line}",
        stage.root().join("relative.css").exists(),
        stage.root().join("relative.svg").exists()
    );
    // CSS's original file would yield 7-31-91 and the relative SVG width7.
    // The inline CSS/data-image controls independently prove measurement ran.
    assert!(
        request_line.starts_with("GET /GH1037_ASSETS/error/error/0/11-22-33/load/3/19-73-109/"),
        "current relative-asset outcomes or positive renderer controls changed: {request_line}"
    );
    beacon
        .handle
        .take()
        .expect("renderer observer owner")
        .join()
        .expect("observer joins");

    writer
        .write_all(b"QUIT\n")
        .expect("actual authenticated Quit request");
    assert_eq!(read_control_line(&mut reader), "QUIT_REPLY");
    assert_eq!(read_control_line(&mut reader), "LINK_EOF");
    drop(writer);
    let output = wait_child_output(cleanup.release(), EVENT_DEADLINE);
    drop(lease);
    assert!(
        output.status.success(),
        "Host orderly exit failed: {output:?}"
    );
    await_process_gone(bun_pid);
    await_process_gone(guardian_pid);
    assert!(
        !stage.root().exists(),
        "healthy Host left its private stage"
    );
}

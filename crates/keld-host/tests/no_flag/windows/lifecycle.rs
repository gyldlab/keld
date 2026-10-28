//! Native ordered lifecycle and healthy relaunch contract.

use crate::support::control::{
    accept_control_or_host_failure, read_control_line_or_host_failure, read_ready_or_host_failure,
};
use crate::support::process::process_exists;
use crate::support::product::{ProductFixture, StageFixture};
use crate::support::product_cycle::run_product_cycle;
use crate::support::signed_process::SignedStateProcessGuard;
use crate::support::window::wait_for_host_window;
use crate::{PRODUCT_DEADLINE, dev_stage_command, ui_end_kill_switch};
use std::io::{BufReader, Read as _};
use std::net::TcpListener;
use std::path::Path;
use std::process::Stdio;
use std::time::Instant;

#[test]
fn windows_no_flag_host_owns_window_two_calls_ordered_quit_and_relaunch() {
    let fixture = ProductFixture::new();
    let first = run_product_cycle(&fixture, "first");
    let second = run_product_cycle(&fixture, "second");

    assert_ne!(
        first.host_pid, second.host_pid,
        "relaunch must be a new host process"
    );
    assert_ne!(
        first.bun_pid, second.bun_pid,
        "relaunch must be a new Bun process"
    );
    assert_ne!(
        first.app_link, second.app_link,
        "relaunch must mint fresh authority"
    );
}

#[test]
fn windows_legacy_api_ready_ignores_inherited_native_facade_hint() {
    run_legacy_api_ready(Some("1"));
}

#[test]
fn windows_legacy_api_ready_without_native_facade_hint() {
    run_legacy_api_ready(None);
}

#[test]
fn windows_legacy_api_ready_with_zero_native_facade_hint() {
    run_legacy_api_ready(Some("0"));
}

fn run_legacy_api_ready(inherited_hint: Option<&str>) {
    let fixture = StageFixture::legacy_api_ready();
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("legacy API control listener");
    let stage = keld_cli::boot::stage_dev_boot(
        &fixture.project,
        Path::new(env!("CARGO_BIN_EXE_keld-host")),
    )
    .expect("stage actual legacy Windows Host");
    let mut command = dev_stage_command(stage.root(), stage.host());
    command.env_remove("KELD_NATIVE_WINDOW_FACADE");
    if let Some(hint) = inherited_hint {
        command.env("KELD_NATIVE_WINDOW_FACADE", hint);
    }
    let mut host = SignedStateProcessGuard::new(
        command
            .env(
                "KELD_T1B_CONTROL",
                listener
                    .local_addr()
                    .expect("control address")
                    .port()
                    .to_string(),
            )
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("launch actual legacy Windows Host"),
    );
    let host_pid = host.host_pid();
    let control = accept_control_or_host_failure(
        &listener,
        host.child_mut(),
        Instant::now() + PRODUCT_DEADLINE,
    );
    drop(listener);
    let mut reader = BufReader::new(control);
    let mode = read_control_line_or_host_failure(&mut reader, host.child_mut(), "API launch mode");
    let mode: serde_json::Value = serde_json::from_str(
        mode.strip_prefix("MODE ")
            .expect("actual role launch-mode record"),
    )
    .expect("launch-mode JSON");
    let bun_pid =
        u32::try_from(mode["pid"].as_u64().expect("actual Bun PID")).expect("Bun PID fits u32");
    host.observe_bun(bun_pid);
    let ready = read_ready_or_host_failure(&mut reader, host.child_mut(), "actual app.whenReady");
    // Read the real API result before mode assertions so the unfixed hint=1
    // case retains its original Subscribe refusal code and message.
    assert_eq!(
        ready, "READY",
        "parent hint={inherited_hint:?}; mode={mode}"
    );
    assert_eq!(mode["hint"], "0", "parent hint={inherited_hint:?}");
    assert_eq!(mode["selected"], false, "parent hint={inherited_hint:?}");
    let window = wait_for_host_window(host_pid, Instant::now() + PRODUCT_DEADLINE);
    assert_ne!(
        window["handle"].as_u64().expect("numeric native HWND"),
        0,
        "{window}"
    );

    // Retire only after public Ready and an actual HWND. This uses the existing
    // lease-loss tail, independently of the separately tracked Windows Quit tail.
    drop(host.child_mut().stdin.take());
    let status = host.wait(Instant::now() + ui_end_kill_switch());
    assert!(status.success(), "lease retirement failed: {status}");
    assert!(
        !process_exists(host_pid),
        "owning Host/window survived retirement"
    );
    assert!(
        !process_exists(bun_pid),
        "legacy API Bun survived retirement"
    );
    reader
        .get_mut()
        .set_read_timeout(Some(PRODUCT_DEADLINE))
        .expect("control EOF deadline");
    let mut trailing = Vec::new();
    reader
        .read_to_end(&mut trailing)
        .expect("control socket EOF after Bun reap");
    assert!(
        trailing.is_empty(),
        "unexpected control bytes: {trailing:?}"
    );
    drop(reader);
    // A direct staged Host has no CLI cleanup sentinel. Release the stage's
    // directory guards before explicitly closing this test-owned workspace.
    drop(stage);
    fixture.close_fixture();
}

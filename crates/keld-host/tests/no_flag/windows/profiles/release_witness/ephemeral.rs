//! The dev-stage profile must outlive an independently observed non-family child.

use super::*;
use crate::dev_stage_command;
use std::path::{Path, PathBuf};
use std::process::Stdio;

#[test]
#[ignore = "debug profile-hook Host and native D16 helper required"]
fn ephemeral_renderer_close_waits_for_non_family_release() {
    ephemeral_release(false);
}

#[test]
#[ignore = "debug profile-hook Host and native D16 helper required"]
fn ephemeral_zero_window_quit_waits_for_non_family_release() {
    ephemeral_release(true);
}

fn ephemeral_release(facade: bool) {
    let fixture = if facade {
        ProductFixture::schema_two()
    } else {
        ProductFixture::new()
    };
    let stage = keld_cli::boot::stage_dev_boot(
        &fixture.project,
        Path::new(env!("CARGO_BIN_EXE_keld-host")),
    )
    .expect("existing dev-stage producer");
    let root = ProfileTestRoot::new();
    assert_profile_test_root_feature(stage.host(), root.path());
    let helper = fixture_env("KELD_T2_HELD_DESCENDANT_HELPER");
    let control = TcpListener::bind(("127.0.0.1", 0)).expect("dev control listener");
    let mut command = dev_stage_command(stage.root(), stage.host());
    command
        .env(
            "KELD_T1B_CONTROL",
            control
                .local_addr()
                .expect("control address")
                .port()
                .to_string(),
        )
        .env("KELD_PROFILE_TEST_ROOT", root.path())
        .env(ENVIRONMENT_DESCENDANT, &helper)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut host = SeamedHost::from_child(command.spawn().expect("staged Host"));
    host.start_live_stderr();
    let (mut reader, mut writer, bun_pid, _) = accept_ready_generation(&control, &mut host.host);
    let bun = open_process_for_wait(bun_pid, false);
    let (helper_pid, helper_handle, cursor) = observe_environment_child(&mut host, 0);
    let profile = one_ephemeral_profile(root.path());
    if facade {
        writer.write_all(b"QUIT\n").expect("zero-controller Quit");
        writer.flush().expect("flush Quit");
    } else {
        let window = wait_for_host_window(host.pid, Instant::now() + PRODUCT_DEADLINE);
        post_close(window["handle"].as_u64().expect("native HWND"));
        assert_eq!(read_control_line(&mut reader), "LAST_WINDOW_CLOSED");
    }
    assert_eq!(read_control_line(&mut reader), "QUIT_REPLY");
    assert_eq!(read_control_line(&mut reader), "LINK_EOF");
    let (counts, _) = observe_loop_end(&mut host, cursor, u8::from(!facade));
    let held = hold_ephemeral_profile(&host, &helper_handle, &profile);
    let (status, stdout, stderr) = host.finish();
    assert!(
        status.success(),
        "staged release: {status}\n{stdout}\n{stderr}"
    );
    assert_process_signaled(&bun, "staged Bun");
    assert!(!profile.exists(), "ephemeral root survived proved release");
    println!(
        "KELD_T2_EPHEMERAL_RELEASE facade={facade} helper={helper_pid} at_risk=1 hold_ms={} counts={counts} deleted=true",
        held.as_millis()
    );
    drop(stage);
    root.remove();
}

fn one_ephemeral_profile(root: &Path) -> PathBuf {
    let base = root.join("Keld").join("ephemeral").join("v1");
    let mut paths = std::fs::read_dir(base)
        .expect("ephemeral namespace")
        .map(|entry| {
            let entry = entry.expect("ephemeral entry");
            assert!(entry.file_type().expect("ephemeral kind").is_dir());
            entry.path()
        })
        .collect::<Vec<_>>();
    assert_eq!(paths.len(), 1, "one run-owned profile");
    let profile = paths.pop().expect("profile");
    assert!(profile.join("webview2").is_dir());
    profile
}

fn hold_ephemeral_profile(
    host: &SeamedHost,
    helper: &std::os::windows::io::OwnedHandle,
    profile: &Path,
) -> Duration {
    let host_handle = open_process_for_wait(host.pid, false);
    assert_eq!(wait_for_process_signal(helper, 0), WAIT_TIMEOUT);
    assert!(
        profile.join("webview2").is_dir(),
        "profile disappeared at the live-member gate"
    );
    let start = Instant::now();
    assert_eq!(
        wait_for_process_signal(&host_handle, 2000),
        WAIT_TIMEOUT,
        "Host exited while its non-family child still held release"
    );
    assert!(start.elapsed() >= HOLD);
    assert_eq!(wait_for_process_signal(helper, 0), WAIT_TIMEOUT);
    assert!(
        profile.join("webview2").is_dir(),
        "profile deleted before helper exit"
    );
    terminate_test_process(helper);
    assert_process_signaled(helper, "ephemeral D16 helper");
    start.elapsed()
}

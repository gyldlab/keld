//! AC26(d,e): the same independent controller oracles on renderer and --hello
//! product loops. No `BrowserWindow` mirror exists on these boot kinds.

use super::fixture::{
    assert_retired, capture, census, ephemeral_profile_dir, retained_descendants,
};
use super::host_observation::{self, Pages};
use crate::facade_ready::stage_boot;
use crate::support::control::{accept_ready_generation, read_control_line};
use crate::support::process::{
    PIPE_DRAIN_KILL_SWITCH, assert_process_signaled, pipe_text_by, read_pipe_on_thread,
};
use crate::support::product::ProductFixture;
use crate::support::profile_run::{ProfileTestRoot, assert_profile_test_root_feature};
use crate::support::signed_process::SignedStateProcessGuard;
use crate::support::window::native_control::PhysicalCoordinates;
use crate::support::window::{app_top_level_windows, post_close};
use crate::{PRODUCT_DEADLINE, dev_stage_command, ui_end_kill_switch};
use std::io::Write as _;
use std::net::TcpListener;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Instant;

#[test]
fn windows_legacy_window_keeps_the_host_window_obligations() {
    let _physical_coordinates = PhysicalCoordinates::enter();
    let fixture = ProductFixture::new();
    let stage = stage_boot(&fixture.project);
    let root = ProfileTestRoot::new();
    assert_profile_test_root_feature(stage.host(), root.path());
    let control = TcpListener::bind(("127.0.0.1", 0)).expect("legacy control");
    let mut host = SignedStateProcessGuard::new(
        dev_stage_command(stage.root(), stage.host())
            .env(
                "KELD_T1B_CONTROL",
                control
                    .local_addr()
                    .expect("control address")
                    .port()
                    .to_string(),
            )
            .env("KELD_PROFILE_TEST_ROOT", root.path())
            .env("TEMP", root.path().parent().expect("test temp parent"))
            .env("TMP", root.path().parent().expect("test temp parent"))
            .env("KELD_TEST_WINDOWS_PAGE_READBACK", "50")
            .env("KELD_TEST_WINDOWS_T2_OBSERVE", "1")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("actual renderer-boot Host"),
    );
    let stdout = read_pipe_on_thread(host.child_mut().stdout.take().expect("Host stdout"));
    let stderr = read_pipe_on_thread(host.child_mut().stderr.take().expect("Host stderr"));
    let (mut reader, mut writer, bun_pid, _) = accept_ready_generation(&control, host.child_mut());
    host.observe_bun(bun_pid);
    let pid = host.host_pid();
    let native = capture(census(pid, 1)[0], pid);
    let retained = retained_descendants(host.child_mut());
    assert!(retained.iter().any(|(pid, _)| *pid == bun_pid));
    let profile = ephemeral_profile_dir(root.path());
    let mut pages = Pages::default();
    let moved = host_observation::moved(&stderr, &mut pages, 1, native.hwnd, pid);
    host_observation::minimized(&stderr, &mut pages, 1, native.hwnd, pid);
    let restore_input = host_observation::prepare_restore_input(native.hwnd, pid);
    host_observation::restored(
        &stderr,
        &mut pages,
        1,
        native.hwnd,
        pid,
        moved.native.window.outer_rect,
        &restore_input,
    );
    drop(restore_input);
    host_observation::focus_cycle(&stderr, &mut pages, 1, native.hwnd, pid);
    println!("KELD_GH1188_AC26 route=renderer controller_oracles_complete=true");

    writer.write_all(b"QUIT\n").expect("legacy raw Quit");
    writer.flush().expect("flush legacy Quit");
    assert_eq!(read_control_line(&mut reader), "QUIT_REPLY");
    assert_eq!(read_control_line(&mut reader), "LINK_EOF");
    let status = host.wait(Instant::now() + ui_end_kill_switch());
    let drained = Instant::now() + PIPE_DRAIN_KILL_SWITCH;
    let trace = pipe_text_by(&stderr, drained);
    let output = pipe_text_by(&stdout, drained);
    assert!(
        status.success(),
        "legacy host obligations/release failed:\n{output}\n{trace}"
    );
    for (pid, handle) in retained {
        assert_process_signaled(&handle, &format!("legacy descendant {pid}"));
    }
    assert_retired(&native);
    assert_eq!(app_top_level_windows(pid), []);
    assert!(
        !profile.exists(),
        "legacy controller test left an owned profile"
    );
    println!("KELD_GH1188_AC26_RENDERER_EXIT\n{trace}");
    root.remove();
}

#[test]
fn windows_hello_window_keeps_the_host_window_obligations() {
    let _physical_coordinates = PhysicalCoordinates::enter();
    let root = ProfileTestRoot::new();
    let image = Path::new(env!("CARGO_BIN_EXE_keld-host"));
    assert_profile_test_root_feature(image, root.path());
    let mut host = SignedStateProcessGuard::new(
        Command::new(image)
            .arg("--hello")
            .env("KELD_PROFILE_TEST_ROOT", root.path())
            .env("TEMP", root.path().parent().expect("test temp parent"))
            .env("TMP", root.path().parent().expect("test temp parent"))
            .env("KELD_TEST_WINDOWS_PAGE_READBACK", "50")
            .env("KELD_TEST_WINDOWS_T2_OBSERVE", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("actual --hello Host route"),
    );
    let stdout = read_pipe_on_thread(host.child_mut().stdout.take().expect("hello stdout"));
    let stderr = read_pipe_on_thread(host.child_mut().stderr.take().expect("hello stderr"));
    let pid = host.host_pid();
    let hwnd = census(pid, 1)[0];
    let mut pages = Pages::default();
    pages.wait(&stderr, 1, Instant::now() + PRODUCT_DEADLINE, |page| {
        page["visibilityState"] == "visible"
    });
    let native = capture(hwnd, pid);
    assert!(
        !native.children.is_empty(),
        "hello has no actual controller hosting windows"
    );
    let retained = retained_descendants(host.child_mut());
    assert!(!retained.is_empty(), "hello has no browser process at risk");
    let profile = ephemeral_profile_dir(root.path());
    let moved = host_observation::moved(&stderr, &mut pages, 1, native.hwnd, pid);
    host_observation::minimized(&stderr, &mut pages, 1, native.hwnd, pid);
    let restore_input = host_observation::prepare_restore_input(native.hwnd, pid);
    host_observation::restored(
        &stderr,
        &mut pages,
        1,
        native.hwnd,
        pid,
        moved.native.window.outer_rect,
        &restore_input,
    );
    drop(restore_input);
    host_observation::focus_cycle(&stderr, &mut pages, 1, native.hwnd, pid);
    println!("KELD_GH1188_AC26 route=hello controller_oracles_complete=true");

    post_close(native.hwnd as u64);
    let status = host.wait(Instant::now() + ui_end_kill_switch());
    let drained = Instant::now() + PIPE_DRAIN_KILL_SWITCH;
    let trace = pipe_text_by(&stderr, drained);
    let output = pipe_text_by(&stdout, drained);
    assert!(
        status.success(),
        "hello obligations/release failed:\n{output}\n{trace}"
    );
    for (pid, handle) in retained {
        assert_process_signaled(&handle, &format!("hello descendant {pid}"));
    }
    assert_retired(&native);
    assert_eq!(app_top_level_windows(pid), []);
    assert!(
        !profile.exists(),
        "hello controller test left an owned profile"
    );
    println!("KELD_GH1188_AC26_HELLO_EXIT\n{trace}");
    root.remove();
}

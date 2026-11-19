//! D7 native release proofs. Debug-hook observations do not claim a release certificate.

use super::sessions::{LifecycleObservation, SeamedHost, read_lifecycle_record};
use crate::support::control::{accept_ready_generation, read_control_line};
use crate::support::installed::{INSTALLER_ENV, InstalledApp, SIGNED_HOST_A_P1, fixture_env};
use crate::support::process::{
    assert_process_signaled, open_process_for_wait, process_tree_report, terminate_test_process,
    wait_for_process_signal,
};
use crate::support::product::ProductFixture;
use crate::support::profile_run::{ProfileTestRoot, assert_profile_test_root_feature};
use crate::support::window::{post_close, wait_for_host_window};
use crate::{PRODUCT_DEADLINE, startup_kill_switch, ui_end_kill_switch};
use std::io::Write as _;
use std::net::TcpListener;
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::WAIT_TIMEOUT;

mod ephemeral;
mod family_recovery;
mod handover;

const ENVIRONMENT_DESCENDANT: &str = "KELD_TEST_WINDOWS_ENVIRONMENT_DESCENDANT";
const HOLD: Duration = Duration::from_secs(2);

#[test]
#[ignore = "signed profile-hook Host, installer and native D16 helper required"]
fn persistent_renderer_close_waits_for_non_family_release() {
    installed_release(false);
}

#[test]
#[ignore = "signed profile-hook Host, installer and native D16 helper required"]
fn persistent_zero_window_quit_waits_for_non_family_release() {
    installed_release(true);
}

fn installed_release(facade: bool) {
    let fixture = if facade {
        ProductFixture::schema_two()
    } else {
        ProductFixture::new()
    };
    let installed = InstalledApp::install(
        &fixture.project,
        &fixture_env(SIGNED_HOST_A_P1),
        &fixture_env(INSTALLER_ENV),
    );
    let root = ProfileTestRoot::new();
    assert_profile_test_root_feature(installed.host(), root.path());
    let helper = fixture_env("KELD_T2_HELD_DESCENDANT_HELPER");
    let helper = helper.to_str().expect("native test helper path is Unicode");
    assert!(
        std::path::Path::new(helper).is_file(),
        "D16 helper is absent"
    );
    let control = TcpListener::bind(("127.0.0.1", 0)).expect("native installed control");
    let seams = [(ENVIRONMENT_DESCENDANT, helper)];
    let mut host = SeamedHost::launch(&installed, root.path(), &control, &seams);
    host.start_live_stderr();
    let (mut reader, mut writer, bun_pid, _) = accept_ready_generation(&control, &mut host.host);
    let bun = open_process_for_wait(bun_pid, false);
    let (helper_pid, helper_handle, cursor) = observe_environment_child(&mut host, 0);
    assert_eq!(
        read_lifecycle_record(root.path()),
        LifecycleObservation::running_under(host.pid)
    );
    if facade {
        writer.write_all(b"QUIT\n").expect("zero-window Quit");
        writer.flush().expect("flush Quit");
    } else {
        let window = wait_for_host_window(host.pid, Instant::now() + PRODUCT_DEADLINE);
        post_close(window["handle"].as_u64().expect("native HWND"));
        assert_eq!(read_control_line(&mut reader), "LAST_WINDOW_CLOSED");
    }
    assert_eq!(read_control_line(&mut reader), "QUIT_REPLY");
    assert_eq!(read_control_line(&mut reader), "LINK_EOF");
    let (counts, held) =
        hold_persistent_release(&mut host, root.path(), &helper_handle, cursor, facade);
    let pid = host.pid;
    let (status, stdout, stderr) = host.finish();
    assert!(
        status.success(),
        "signed Host failed release: {status}\n{stdout}\n{stderr}"
    );
    assert_process_signaled(&bun, "Bun after release");
    assert_eq!(
        read_lifecycle_record(root.path()),
        LifecycleObservation::idle()
    );
    println!(
        "KELD_T2_INSTALLED_RELEASE facade={facade} host={pid} helper={helper_pid} at_risk=1 hold_ms={} counts={counts} after=idle",
        held.as_millis()
    );
    assert_same_profile_relaunch(&installed, root.path(), &control);
    drop(installed);
    root.remove();
}

fn assert_same_profile_relaunch(
    installed: &InstalledApp,
    root: &std::path::Path,
    control: &TcpListener,
) {
    // Observe Idle before the successor; inline quarantine must not hide failure.
    let mut relaunched = SeamedHost::launch(installed, root, control, &[]);
    let (mut reader, mut writer, _, _) = accept_ready_generation(control, &mut relaunched.host);
    writer.write_all(b"QUIT\n").expect("relaunch Quit");
    writer.flush().expect("flush relaunch Quit");
    assert_eq!(read_control_line(&mut reader), "QUIT_REPLY");
    assert_eq!(read_control_line(&mut reader), "LINK_EOF");
    let (status, stdout, stderr) = relaunched.finish();
    assert!(
        status.success(),
        "same-profile relaunch: {status}\n{stdout}\n{stderr}"
    );
    assert_eq!(read_lifecycle_record(root), LifecycleObservation::idle());
}

fn hold_persistent_release(
    host: &mut SeamedHost,
    root: &std::path::Path,
    helper_handle: &std::os::windows::io::OwnedHandle,
    cursor: usize,
    facade: bool,
) -> (String, Duration) {
    let host_handle = open_process_for_wait(host.pid, false);
    let (counts, _) = observe_loop_end(host, cursor, u8::from(!facade));
    assert_eq!(
        wait_for_process_signal(helper_handle, 0),
        WAIT_TIMEOUT,
        "helper must be alive at the release gate"
    );
    assert_eq!(
        read_lifecycle_record(root).phase,
        "stopping",
        "read is quiescent while helper blocks the census"
    );
    let held = Instant::now();
    assert_eq!(
        wait_for_process_signal(
            &host_handle,
            u32::try_from(HOLD.as_millis()).expect("hold fits wait")
        ),
        WAIT_TIMEOUT,
        "Host finished release while a non-family member remained live"
    );
    assert!(held.elapsed() >= HOLD);
    assert_eq!(wait_for_process_signal(helper_handle, 0), WAIT_TIMEOUT);
    assert_eq!(
        read_lifecycle_record(root).phase,
        "stopping",
        "Idle cannot precede helper exit"
    );
    terminate_test_process(helper_handle);
    assert_process_signaled(helper_handle, "D16 helper");
    (counts, held.elapsed())
}

/// Actual Host-child membership at a caller-controlled pre-terminal edge.
fn observe_environment_child(
    host: &mut SeamedHost,
    after: usize,
) -> (u32, std::os::windows::io::OwnedHandle, usize) {
    let (line, cursor) = host.wait_stderr_line(
        "KELD_T2_ENVIRONMENT_DESCENDANT pid=",
        after,
        Instant::now() + startup_kill_switch(),
    );
    let pid: u32 = line
        .strip_prefix("KELD_T2_ENVIRONMENT_DESCENDANT pid=")
        .expect("native helper receipt")
        .parse()
        .expect("helper pid");
    let handle = open_process_for_wait(pid, true);
    let tree = process_tree_report(host.pid);
    assert!(
        tree.contains(&format!(":{pid}<")),
        "helper absent before terminal: {tree}"
    );
    assert_eq!(wait_for_process_signal(&handle, 0), WAIT_TIMEOUT);
    (pid, handle, cursor)
}

fn observe_loop_end(host: &mut SeamedHost, after: usize, controllers: u8) -> (String, usize) {
    let (counts, cursor) = host.wait_stderr_line(
        "KELD_T2_LOOP_EXIT ",
        after,
        Instant::now() + ui_end_kill_switch(),
    );
    assert!(
        counts.contains(&format!(
            "environments=1 controllers={controllers} armed_by=Quit"
        )),
        "{counts}"
    );
    let (_, cursor) = host.wait_stderr_line(
        "KELD_T2_RELEASE_WITNESS entered",
        cursor,
        Instant::now() + PRODUCT_DEADLINE,
    );
    (counts, cursor)
}

//! Handover must not publish `LastWindowClosed` or construct a successor before release.

use super::super::sessions::engine_constructions;
use super::*;
use crate::support::control::try_read_control_line;
use crate::support::renderer::HeldRendererRequest;

#[test]
#[ignore = "signed profile-hook Host, installer and native D16 helper required"]
fn renderer_handover_waits_for_non_family_release_before_the_second_engine() {
    let fixture = ProductFixture::new();
    let held_renderer = HeldRendererRequest::bind();
    held_renderer.write_index(&fixture.project);
    let installed = InstalledApp::install(
        &fixture.project,
        &fixture_env(SIGNED_HOST_A_P1),
        &fixture_env(INSTALLER_ENV),
    );
    let root = ProfileTestRoot::new();
    assert_profile_test_root_feature(installed.host(), root.path());
    let helper = fixture_env("KELD_T2_HELD_DESCENDANT_HELPER");
    let helper = helper.to_str().expect("fixture helper path");
    let control = TcpListener::bind(("127.0.0.1", 0)).expect("handover control");
    let mut host = SeamedHost::launch(
        &installed,
        root.path(),
        &control,
        &[
            (ENVIRONMENT_DESCENDANT, helper),
            ("KELD_TEST_WINDOWS_HANDOVER", "1"),
        ],
    );
    host.start_live_stderr();
    // The existing held subresource blocks NavigationCompleted and Ready. The
    // native helper census is therefore strictly before the auto-handover terminal.
    held_renderer.await_connection(&mut host.host);
    let (helper_pid, helper_handle, cursor) = observe_environment_child(&mut host, 0);
    assert!(
        host.stderr_line_if_present("KELD_T2_LOOP_EXIT ", 0)
            .is_none(),
        "terminal preceded the controlled renderer release"
    );
    held_renderer.release();
    let (mut reader, _, bun_pid, _) = accept_ready_generation(&control, &mut host.host);
    let bun = open_process_for_wait(bun_pid, false);
    let (counts, cursor) = observe_loop_end(&mut host, cursor, 1);
    let held = hold_handover(&mut host, &mut reader, root.path(), &helper_handle, cursor);
    assert_eq!(read_control_line(&mut reader), "LAST_WINDOW_CLOSED");
    assert_eq!(read_control_line(&mut reader), "QUIT_REPLY");
    assert_eq!(read_control_line(&mut reader), "LINK_EOF");
    let (mut second_reader, mut second_writer, second_pid, _) =
        accept_ready_generation(&control, &mut host.host);
    assert_ne!(second_pid, bun_pid);
    host.assert_same_live_process();
    assert_process_signaled(&bun, "first handover generation");
    let (_, second_helper, _) = observe_environment_child(&mut host, cursor);
    terminate_test_process(&second_helper);
    assert_process_signaled(&second_helper, "second-session fixture child");
    second_writer
        .write_all(b"QUIT\n")
        .expect("second-session Quit");
    second_writer.flush().expect("flush second Quit");
    assert_eq!(read_control_line(&mut second_reader), "QUIT_REPLY");
    assert_eq!(read_control_line(&mut second_reader), "LINK_EOF");
    let (status, stdout, stderr) = host.finish();
    assert!(
        status.success(),
        "handover release: {status}\n{stdout}\n{stderr}"
    );
    assert_eq!(
        read_lifecycle_record(root.path()),
        LifecycleObservation::idle()
    );
    let (folders, environments) = engine_constructions(&stdout);
    assert_eq!((folders.len(), environments), (2, 2));
    assert_eq!(stderr.matches("KELD_T2_ENGINE_BEGIN").count(), 2);
    println!(
        "KELD_T2_HANDOVER_RELEASE helper={helper_pid} at_risk=1 hold_ms={} counts={counts} successor_after_release=true",
        held.as_millis()
    );
    drop(installed);
    root.remove();
}

fn hold_handover(
    host: &mut SeamedHost,
    reader: &mut std::io::BufReader<std::net::TcpStream>,
    root: &std::path::Path,
    helper: &std::os::windows::io::OwnedHandle,
    cursor: usize,
) -> Duration {
    assert_eq!(wait_for_process_signal(helper, 0), WAIT_TIMEOUT);
    assert_eq!(read_lifecycle_record(root).phase, "stopping");
    let held = Instant::now();
    let before_release = try_read_control_line(reader, Instant::now() + HOLD);
    assert!(
        before_release.is_err(),
        "application received a handover event before release: {before_release:?}"
    );
    assert!(
        held.elapsed() >= HOLD,
        "socket ended early, not a full live hold"
    );
    host.assert_same_live_process();
    assert_eq!(wait_for_process_signal(helper, 0), WAIT_TIMEOUT);
    assert_eq!(read_lifecycle_record(root).phase, "stopping");
    assert!(
        host.stderr_line_if_present("KELD_T2_ENGINE_BEGIN", cursor)
            .is_none(),
        "a second native engine began before non-family release"
    );
    terminate_test_process(helper);
    assert_process_signaled(helper, "handover D16 helper");
    held.elapsed()
}

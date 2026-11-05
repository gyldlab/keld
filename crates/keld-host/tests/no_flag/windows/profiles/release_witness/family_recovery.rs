//! The signed zero-window Host excludes every real Bun generation, not only the last.

use super::super::sessions::{LifecycleObservation, SeamedHost, read_lifecycle_record};
use super::observe_loop_end;
use crate::support::control::accept_ready_generation_with_descendant;
use crate::support::control::read_control_line;
use crate::support::installed::{INSTALLER_ENV, InstalledApp, SIGNED_HOST_A_P1, fixture_env};
use crate::support::process::{
    assert_process_signaled, open_process_for_wait, process_tree_report, wait_for_process_signal,
};
use crate::support::product::ProductFixture;
use crate::support::profile_run::{ProfileTestRoot, assert_profile_test_root_feature};
use std::io::Write as _;
use std::net::TcpListener;
use windows_sys::Win32::Foundation::WAIT_TIMEOUT;

#[test]
#[ignore = "signed profile-hook/root Host and installer required"]
fn persistent_quit_excludes_the_live_bun_family() {
    family_quit(false);
}

#[test]
#[ignore = "signed profile-hook/root Host and installer required"]
fn persistent_quit_excludes_retired_and_replacement_bun_families() {
    family_quit(true);
}

fn family_quit(restart: bool) {
    let fixture = ProductFixture::schema_two();
    let installed = InstalledApp::install(
        &fixture.project,
        &fixture_env(SIGNED_HOST_A_P1),
        &fixture_env(INSTALLER_ENV),
    );
    let root = ProfileTestRoot::new();
    assert_profile_test_root_feature(installed.host(), root.path());
    let control = TcpListener::bind(("127.0.0.1", 0)).expect("generation control");
    let helper = fixture_env("KELD_T2_HELD_DESCENDANT_HELPER");
    let mut host = SeamedHost::launch(
        &installed,
        root.path(),
        &control,
        &[
            ("KELD_T4_JOB_DESCENDANT", "1"),
            (
                "KELD_T4_JOB_DESCENDANT_PROGRAM",
                helper.to_str().expect("native helper path"),
            ),
        ],
    );
    host.start_live_stderr();
    let (mut reader, mut writer, first_pid, first_link, descendant_pid) =
        accept_ready_generation_with_descendant(&control, &mut host.host);
    assert_ne!(descendant_pid, 0, "a real inherited descendant is required");
    let first = open_process_for_wait(first_pid, false);
    let descendant = open_process_for_wait(descendant_pid, false);
    assert_eq!(wait_for_process_signal(&descendant, 0), WAIT_TIMEOUT);
    let mut retained = vec![(first_pid, first), (descendant_pid, descendant)];
    let tree = process_tree_report(host.pid);
    assert!(
        tree.contains(&format!(":{descendant_pid}<")),
        "first-generation descendant absent while its leader is live: {tree}"
    );
    if restart {
        writer.write_all(b"CRASH\n").expect("actual Bun crash");
        writer.flush().expect("flush crash");
        assert_process_signaled(&retained[0].1, "first Bun generation");
        let (replacement_reader, replacement_writer, pid, link, child) =
            accept_ready_generation_with_descendant(&control, &mut host.host);
        assert_ne!(pid, first_pid);
        assert_ne!(link, first_link);
        assert_ne!(child, 0);
        assert_ne!(child, descendant_pid);
        host.assert_same_live_process();
        assert_eq!(
            wait_for_process_signal(&retained[1].1, 0),
            WAIT_TIMEOUT,
            "old descendant must still be alive when replacement is authenticated"
        );
        retained.push((pid, open_process_for_wait(pid, false)));
        retained.push((child, open_process_for_wait(child, false)));
        reader = replacement_reader;
        writer = replacement_writer;
    }
    // A current parent-PID census cannot trace an orphan through its exited
    // leader. Keep the independent handle captured while that leader was live.
    assert_eq!(
        wait_for_process_signal(&retained[1].1, 0),
        WAIT_TIMEOUT,
        "retired-generation descendant must still be alive immediately before Quit"
    );
    assert_eq!(
        read_lifecycle_record(root.path()),
        LifecycleObservation::running_under(host.pid)
    );
    writer.write_all(b"QUIT\n").expect("generation Quit");
    writer.flush().expect("flush Quit");
    assert_eq!(read_control_line(&mut reader), "QUIT_REPLY");
    assert_eq!(read_control_line(&mut reader), "LINK_EOF");
    let (counts, _) = observe_loop_end(&mut host, 0, 0, "Quit");
    let (status, stdout, stderr) = host.finish();
    let phase = read_lifecycle_record(root.path());
    println!(
        "KELD_T2_SIGNED_FAMILIES restart={restart} retained={} counts={counts} phase={phase:?} result={status}",
        retained.len()
    );
    assert!(
        status.success(),
        "family exclusion failed: {status}\n{stdout}\n{stderr}"
    );
    for (pid, handle) in retained {
        assert_process_signaled(&handle, &format!("retained generation member {pid}"));
    }
    assert_eq!(phase, LifecycleObservation::idle());
    drop(installed);
    root.remove();
}

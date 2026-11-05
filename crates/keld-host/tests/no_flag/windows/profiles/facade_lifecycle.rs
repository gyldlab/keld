//! Persistent facade lifecycle on an installed package (GH-1188).

use super::sessions::{SeamedHost, read_lifecycle_record};
use crate::startup_kill_switch;
use crate::support::control::{parse_hello, read_control_line};
use crate::support::installed::{INSTALLER_ENV, InstalledApp, SIGNED_HOST_A_P1, fixture_env};
use crate::support::process::process_tree_report;
use crate::support::product::ProductFixture;
use crate::support::profile_run::{ProfileTestRoot, assert_profile_test_root_feature};
use std::io::{BufReader, Write as _};
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::{Duration, Instant};

/// Bun's control connection, or `None` when the host exited without one: a refusal
/// reaches no Bun. Only the launch's own startup bounds this wait.
fn control_or_host_exit(control: &TcpListener, host: &mut SeamedHost) -> Option<TcpStream> {
    control
        .set_nonblocking(true)
        .expect("nonblocking control accept");
    let deadline = Instant::now() + startup_kill_switch();
    loop {
        match control.accept() {
            Ok((stream, _)) => {
                stream.set_nonblocking(false).expect("blocking control");
                stream
                    .set_read_timeout(Some(startup_kill_switch()))
                    .expect("control read deadline");
                return Some(stream);
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(error) => panic!("control accept: {error}"),
        }
        if host
            .host
            .try_wait()
            .expect("observe the installed host")
            .is_some()
        {
            return None;
        }
        assert!(
            Instant::now() < deadline,
            "neither a refusal nor a control connection; {}",
            process_tree_report(host.pid)
        );
        thread::park_timeout(Duration::from_millis(10));
    }
}

/// AC23: one cell for every head from T1 on, launched without the test-admission env,
/// on an `InstalledApp` persistent project that never builds a window. Either the launch
/// is refused `KELD-CORE-034` before any `READY` (T1's required outcome), or it reaches
/// `READY`, the fixture's `Quit` exits 0 and the record reads `idle` (T2's required
/// outcome, whose zero-controller and descendant-release controls land with T2's
/// readbacks). Any other outcome is RED: a T1 head that drops the admission check exits
/// `KELD-WV-009` with the record `starting` (NC 1).
#[test]
#[ignore = "requires KELD_KEL254_SIGNED_HOST_A_P1 (a kel135-profile-test-root host) and KELD_KEL254_INSTALLER_FIXTURE"]
fn windows_product_facade_admission_waits_for_the_release_proof() {
    let fixture = ProductFixture::schema_two();
    let installed = InstalledApp::install(
        &fixture.project,
        &fixture_env(SIGNED_HOST_A_P1),
        &fixture_env(INSTALLER_ENV),
    );
    let root = ProfileTestRoot::new();
    assert_profile_test_root_feature(installed.host(), root.path());
    let control = TcpListener::bind(("127.0.0.1", 0)).expect("bind installed facade control");
    let mut host = SeamedHost::launch(&installed, root.path(), &control, &[]);
    let Some(stream) = control_or_host_exit(&control, &mut host) else {
        let (status, stdout, stderr) = host.finish();
        assert!(
            !status.success(),
            "exited 0 with no Bun: {stdout}\n{stderr}"
        );
        assert!(stderr.contains("KELD-CORE-034"), "{stdout}\n{stderr}");
        assert!(
            stderr.contains("schema 2 is supported on macOS only"),
            "{stderr}"
        );
        println!(
            "KELD_GH1188_AC23 outcome=refused stderr={}",
            stderr.trim_end()
        );
        drop(installed);
        root.remove();
        return;
    };
    let mut writer = stream.try_clone().expect("control writer");
    let mut reader = BufReader::new(stream);
    let (bun_pid, _) = parse_hello(&read_control_line(&mut reader));
    assert_eq!(read_control_line(&mut reader), "DESCENDANT 0");
    assert_eq!(read_control_line(&mut reader), "READY");
    assert_eq!(read_control_line(&mut reader), "ECHO1");
    assert_eq!(read_control_line(&mut reader), "ECHO2");
    writer.write_all(b"QUIT\n").expect("facade Quit");
    writer.flush().expect("flush facade Quit");
    assert_eq!(read_control_line(&mut reader), "QUIT_REPLY");
    assert_eq!(read_control_line(&mut reader), "LINK_EOF");
    let (status, stdout, stderr) = host.finish();
    let record = read_lifecycle_record(root.path());
    println!(
        "KELD_GH1188_AC23 outcome=ready bun_pid={bun_pid} status={status} phase={}",
        record.phase
    );
    assert!(
        status.success(),
        "Quit of a product facade boot failed with the record {record:?}\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert_eq!(record.phase, "idle", "{stdout}\n{stderr}");
    drop(installed);
    root.remove();
}

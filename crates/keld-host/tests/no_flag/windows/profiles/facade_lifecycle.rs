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

/// AC23 at T2: the product boot must reach Ready, then Quit must write Idle.
/// The T1 CORE034 refusal is no longer a passing alternative. This changes that
/// explicit contract in the same integration as Running and the required witness.
#[test]
#[ignore = "signed profile-hook/root Host and installer required"]
fn windows_product_facade_admission_waits_for_the_release_proof() {
    use super::sessions::LifecycleObservation;
    use crate::support::process::{assert_process_signaled, open_process_for_wait};
    let fixture = ProductFixture::schema_two();
    let installed = InstalledApp::install(
        &fixture.project,
        &fixture_env(SIGNED_HOST_A_P1),
        &fixture_env(INSTALLER_ENV),
    );
    let root = ProfileTestRoot::new();
    assert_profile_test_root_feature(installed.host(), root.path());
    let control = TcpListener::bind(("127.0.0.1", 0)).expect("installed facade control");
    let mut host = SeamedHost::launch(&installed, root.path(), &control, &[]);
    host.start_live_stderr();
    let Some(stream) = control_or_host_exit(&control, &mut host) else {
        let (status, stdout, stderr) = host.finish();
        drop(installed);
        root.remove();
        panic!("T2 product facade refused before Ready: {status}\n{stdout}\n{stderr}");
    };
    let mut writer = stream.try_clone().expect("control writer");
    let mut reader = BufReader::new(stream);
    let (bun_pid, _) = parse_hello(&read_control_line(&mut reader));
    assert_eq!(read_control_line(&mut reader), "DESCENDANT 0");
    assert_eq!(read_control_line(&mut reader), "READY");
    assert_eq!(read_control_line(&mut reader), "ECHO1");
    assert_eq!(read_control_line(&mut reader), "ECHO2");
    let census = process_tree_report(host.pid);
    let descendants = census
        .split_once(" descendants=")
        .expect("native pre-Quit census")
        .1;
    let retained = descendants
        .split(',')
        .filter(|value| !value.is_empty())
        .map(|entry| {
            let (_, pid) = entry.rsplit_once(':').expect("native descendant name");
            let (pid, _) = pid.split_once('<').expect("native descendant parent");
            let pid: u32 = pid.parse().expect("native descendant id");
            (pid, open_process_for_wait(pid, false))
        })
        .collect::<Vec<_>>();
    assert!(
        retained.iter().any(|(pid, _)| *pid == bun_pid),
        "Bun absent from pre-Quit census: {census}"
    );
    assert_eq!(read_lifecycle_record(root.path()).phase, "running");
    writer.write_all(b"QUIT\n").expect("facade Quit");
    writer.flush().expect("flush Quit");
    assert_eq!(read_control_line(&mut reader), "QUIT_REPLY");
    assert_eq!(read_control_line(&mut reader), "LINK_EOF");
    let (status, stdout, stderr) = host.finish();
    assert!(
        status.success(),
        "product facade Quit failed: {status}\n{stdout}\n{stderr}"
    );
    assert!(
        stderr.contains("KELD_T2_LOOP_EXIT environments=1 controllers=0 armed_by=Quit"),
        "zero-controller proof missing: {stderr}"
    );
    assert!(
        stderr.contains("KELD_T2_RELEASE_WITNESS completed=true"),
        "{stderr}"
    );
    for (pid, handle) in &retained {
        assert_process_signaled(handle, &format!("pre-Quit descendant {pid}"));
    }
    // Observe Idle after every retained descendant exit, before any recovery launch.
    assert_eq!(
        read_lifecycle_record(root.path()),
        LifecycleObservation::idle()
    );
    println!(
        "KELD_GH1188_AC23 outcome=ready bun_pid={bun_pid} phase=idle environments=1 controllers=0 descendants_released={}",
        retained.len()
    );
    drop(installed);
    root.remove();
}

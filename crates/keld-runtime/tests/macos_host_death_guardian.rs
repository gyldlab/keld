//! Real-macOS proof for the proposed host-death guardian contract.
#![cfg(target_os = "macos")]
#![allow(clippy::expect_used, clippy::panic)] // extra test crate: expect/panic are assertion oracles
#![allow(clippy::zombie_processes)] // the leader is killed with its descendant group; the controller proves both PIDs gone

use std::env;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::fd::OwnedFd;
use std::os::unix::fs::{FileTypeExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use keld_ipc::link::handshake_client;
use keld_ipc::{BootstrapListener, parse_app_link};
use keld_runtime::macos_guardian;

const ROLE_ENV: &str = "KELD_TEST_MACOS_REAPER_ROLE";
const CONTROL_ENV: &str = "KELD_TEST_MACOS_REAPER_CONTROL";
const APP_LINK_ENV: &str = "KELD_TEST_MACOS_REAPER_APP_LINK";
const APP_LINK_LEAK_ENV: &str = "KELD_TEST_MACOS_REAPER_APP_LINK_LEAK";
const EOF_WITNESS_ENV: &str = "KELD_TEST_MACOS_REAPER_EOF_WITNESS";
const REGISTERED_LINK_ENV: &str = "KELD_TEST_MACOS_REAPER_REGISTERED_LINK";
const TEST_EXE_ENV: &str = "KELD_TEST_MACOS_REAPER_EXE";
const ROLE_TEST: &str = "macos_host_death_guardian_role";
const EVENT_DEADLINE: Duration = Duration::from_secs(10);
const PROCESS_GONE_DEADLINE: Duration = Duration::from_secs(5);

static NEXT_PATH_ID: AtomicU64 = AtomicU64::new(0);

// The integration-target test IDs and subprocess --exact selector live here.
// Concrete role processes, independent observations and owned harness cleanup
// are separate Rust modules; no include!-based pseudo-extraction.
#[path = "macos_host_death_guardian/readiness.rs"]
mod readiness;
#[path = "macos_host_death_guardian/roles.rs"]
mod roles;
#[path = "macos_host_death_guardian/support.rs"]
mod support;
#[path = "macos_host_death_guardian/witness.rs"]
mod witness;
use self::readiness::{ReadyState, await_ready};
use self::roles::{run_descendant, run_guardian, run_host, run_link_peer, spawn_host};
use self::support::{CycleCleanup, Fixture, accept_line_before, await_process_gone, kill_pid};
use self::witness::EofWitness;

#[test]
fn macos_host_death_guardian_role() {
    let Some(role) = env::var_os(ROLE_ENV) else {
        return;
    };
    let control = PathBuf::from(env::var_os(CONTROL_ENV).expect("control path"));
    match role.to_str().expect("UTF-8 role") {
        "host" => run_host(&control),
        "guardian" => run_guardian(&control),
        "descendant" => run_descendant(&control),
        "peer" => run_link_peer(&control),
        "leaker" => thread::park(),
        other => panic!("unknown macOS reaper role: {other}"),
    }
}

#[test]
fn guardian_reaps_the_enrolled_group_after_host_sigkill_and_allows_relaunch() {
    let fixture = Fixture::new();
    let listener = UnixListener::bind(&fixture.control).expect("bind controller socket");
    listener
        .set_nonblocking(true)
        .expect("make controller socket nonblocking");

    run_host_death_cycle(&listener, &fixture.control, false);
    run_host_death_cycle(&listener, &fixture.control, false);
}

#[test]
fn app_link_eof_witness_detects_a_leaked_server_descriptor() {
    let fixture = Fixture::new();
    let listener = UnixListener::bind(&fixture.control).expect("bind controller socket");
    listener
        .set_nonblocking(true)
        .expect("make controller socket nonblocking");

    run_host_death_cycle(&listener, &fixture.control, true);
}

#[test]
fn guardian_death_is_fatal_to_the_live_host_and_leaves_no_group() {
    if env::var_os(ROLE_ENV).is_some() {
        return;
    }
    let fixture = Fixture::new();
    let listener = UnixListener::bind(&fixture.control).expect("bind controller socket");
    listener
        .set_nonblocking(true)
        .expect("make controller socket nonblocking");
    let cycle_started = Instant::now();
    let host = spawn_host(&fixture.control, false);
    let host_pid = host.id();
    let mut cleanup = CycleCleanup::new(host);
    let ready = await_ready(
        &listener,
        &fixture.control,
        host_pid,
        cycle_started,
        &mut cleanup,
        None,
        false,
    );

    kill_pid(ready.guardian_pid);
    let deadline = Instant::now() + EVENT_DEADLINE;
    let mut fatal = false;
    let mut link_eof = false;
    while !fatal || !link_eof {
        let line = accept_line_before(&listener, deadline);
        if line == "LINK_EOF" {
            link_eof = true;
        } else if line.starts_with(&format!("FATAL {} KELD-RUNTIME-013:", ready.leader_pid)) {
            fatal = true;
        } else {
            panic!("unexpected guardian-death event: {line}");
        }
    }

    let mut host = cleanup.host.take().expect("live host child");
    let host_status = host.wait().expect("wait fatal-session host");
    assert!(
        host_status.success(),
        "host failed to surface guardian death: {host_status:?}"
    );
    let mut peer = cleanup.peer.take().expect("live app-link peer");
    let peer_status = peer.wait().expect("wait revoked app-link peer");
    assert!(
        peer_status.success(),
        "app-link peer failed: {peer_status:?}"
    );
    assert!(
        !ready.session_dir.exists(),
        "fatal guardian exit must leave no stale app-link locator"
    );
    assert!(
        !ready.registered_link.exists(),
        "fatal guardian exit must revoke the registered socket"
    );
    await_process_gone(ready.leader_pid);
    await_process_gone(ready.descendant_pid);
    await_process_gone(ready.guardian_pid);
    cleanup.group_gone = true;
}

fn run_host_death_cycle(listener: &UnixListener, control: &Path, leak_app_link: bool) {
    let mut eof_witness = EofWitness::bind(control);
    let cycle_started = Instant::now();
    let host = spawn_host(control, leak_app_link);
    let host_pid = host.id();
    let mut cleanup = CycleCleanup::new(host);
    let ready = await_ready(
        listener,
        control,
        host_pid,
        cycle_started,
        &mut cleanup,
        Some(eof_witness.path()),
        leak_app_link,
    );
    eof_witness.accept_before(Instant::now() + EVENT_DEADLINE);

    let mut host = cleanup.host.take().expect("live host child");
    host.kill().expect("SIGKILL host only");
    let host_status = host.wait().expect("wait for killed host");
    assert_eq!(
        host_status.signal(),
        Some(9),
        "controller must kill only the host with SIGKILL: {host_status:?}"
    );

    await_cleanup(
        listener,
        &ready,
        &mut cleanup,
        &mut eof_witness,
        leak_app_link,
    );
    await_process_gone(ready.leader_pid);
    await_process_gone(ready.descendant_pid);
    await_process_gone(ready.guardian_pid);
    cleanup.group_gone = true;
}

fn await_cleanup(
    listener: &UnixListener,
    ready: &ReadyState,
    cleanup: &mut CycleCleanup,
    eof_witness: &mut EofWitness,
    leak_app_link: bool,
) {
    let cleanup_deadline = Instant::now() + EVENT_DEADLINE;
    let mut guardian_revoked = false;
    let mut reaped = false;
    while !guardian_revoked || !reaped {
        let line = accept_line_before(listener, cleanup_deadline);
        match line.as_str() {
            "GUARDIAN_REVOKED" => {
                assert!(
                    !ready.registered_link.exists(),
                    "guardian revocation must unlink the registered socket"
                );
                assert!(
                    !ready
                        .registered_link
                        .parent()
                        .expect("registered socket has owner directory")
                        .exists(),
                    "guardian revocation must remove the registered owner directory"
                );
                guardian_revoked = true;
            }
            "LINK_EOF" => {
                panic!("peer reported EOF before the controller released its delayed report")
            }
            value if value == format!("REAPED {}", ready.leader_pid) => {
                assert!(
                    guardian_revoked,
                    "registered-resource revocation must precede reap completion"
                );
                reaped = true;
            }
            other => panic!("unexpected cleanup event: {other}"),
        }
    }
    if leak_app_link {
        // REAPED is the synchronization boundary: the guardian finished while
        // the independent leaker still owns a server descriptor, so a real
        // peer EOF cannot already be present.
        eof_witness.assert_no_eof_observed();
        let leaker = cleanup
            .leaker
            .take()
            .expect("live app-link descriptor leaker");
        kill_pid(leaker);
        await_process_gone(leaker);
    }
    // Actual socket EOF is proved on this persistent channel. The separate
    // LINK_EOF controller connection is deliberately released only after
    // REAPED, proving its arrival order is not the cleanup oracle.
    eof_witness.await_eof_observed();
    eof_witness.release_link_eof_report();
    let link_eof_deadline = Instant::now() + EVENT_DEADLINE;
    assert_eq!(
        accept_line_before(listener, link_eof_deadline),
        "LINK_EOF",
        "peer must report its witnessed EOF after the controller releases it"
    );
    let stale = UnixStream::connect(&ready.endpoint).expect_err("revoked locator must stay closed");
    assert!(
        matches!(
            stale.kind(),
            std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused
        ),
        "unexpected stale locator error: {stale}"
    );
    let registered_stale = UnixStream::connect(&ready.registered_link)
        .expect_err("revoked registered socket must stay closed");
    assert_eq!(
        registered_stale.kind(),
        std::io::ErrorKind::NotFound,
        "registered locator must be unlinked, not merely unlistened"
    );
    let mut peer = cleanup.peer.take().expect("live app-link peer");
    let peer_status = peer.wait().expect("wait revoked app-link peer");
    assert!(
        peer_status.success(),
        "app-link peer failed: {peer_status:?}"
    );
}

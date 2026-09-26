//! KEL-96/T1b real-macOS no-flag host/window/session acceptance.
#![cfg(target_os = "macos")]
#![allow(clippy::expect_used, clippy::panic)] // extra test crate: assertions are the oracle
#![allow(clippy::zombie_processes)] // cleanup owns host plus the enrolled Bun process group
#![allow(unsafe_code)] // test-only macOS kill(2) group cleanup; local SAFETY proof is at the call

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::os::fd::OwnedFd;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::os::unix::net::{UnixDatagram, UnixListener, UnixStream};
use std::os::unix::process::{CommandExt as _, ExitStatusExt};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Output, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// Dark background for fixture renderers, so a test run does not flash
/// white windows across the operator's desktop. Cosmetic only: no test
/// asserts on it, and the beacon/marker contracts are unchanged.
const DARK_BG: &str = "<style>html,body{background:#111;color:#eee}</style>";
const TITLE: &str = "KEL96 T1b Fixture";
const MARKER: &str = "KEL96_T1B_EXACT_RENDERER_7e2d9b";
const FORWARDED_LOG: &str = "KEL96_T2_FORWARDED_LOG";
const EVENT_DEADLINE: Duration = Duration::from_secs(15);
#[cfg(feature = "profile-test-hooks")]
const MEDIA_PROMPT_DEADLINE: Duration = Duration::from_mins(2);
const PROCESS_DEADLINE: Duration = Duration::from_secs(5);

unsafe extern "C" {
    #[link_name = "kill"]
    fn kill_process(pid: i32, signal: i32) -> i32;
}

#[test]
fn keld_dev_helper_process() {
    let Some(project) = std::env::var_os("KELD_T2_HELPER_PROJECT") else {
        return;
    };
    keld_cli::dev::run_dev(Path::new(&project)).expect("shipping keld dev helper");
}

/// Fixture child for the Unix-descriptor census tests.
///
/// Opens exactly one Unix listener of its own, then holds the Unix descriptor
/// the harness passed down as stdin until the harness closes the far end. That
/// read is the release signal, so the census always observes a live process
/// instead of racing its exit. Returns immediately when the harness did not
/// select it, like [`keld_dev_helper_process`].
///
/// `KELD_T2_CENSUS_ANONYMOUS_SOCKET` additionally opens an unbound socket, which
/// `lsof` reports without an identity. It is opened before the listener so that
/// the listener's path, which is what the harness waits on, still marks the point
/// where every descriptor this fixture owns is open.
///
/// The harness may also hand down the same inherited socket twice, on stdin and
/// stdout, to stand in for a `dup` of an inherited descriptor.
#[test]
fn unix_descriptor_census_fixture_process() {
    let Some(owned) = std::env::var_os("KELD_T2_CENSUS_OWNED_SOCKET") else {
        return;
    };
    let _anonymous = std::env::var_os("KELD_T2_CENSUS_ANONYMOUS_SOCKET")
        .map(|_| UnixDatagram::unbound().expect("census fixture unbound Unix socket"));
    let _owned = UnixListener::bind(Path::new(&owned)).expect("census fixture Unix listener");
    let mut released = Vec::new();
    std::io::stdin()
        .read_to_end(&mut released)
        .expect("hold the inherited Unix descriptor until the harness releases it");
}

/// KEL-222: the Unix-descriptor census charges a descriptor to the process that
/// opened it, never to a process that merely inherited one.
///
/// A shell that leaves a Unix socket without `FD_CLOEXEC` leaks it through every
/// `exec` into the process under test, so counting `lsof -U` rows answers "does
/// this pid hold any Unix descriptor from any source" instead of "did this pid
/// open one". This test reproduces that leak deliberately: the harness keeps its
/// own copy of an accepted Unix stream and hands a duplicate down as the child's
/// stdin, while the child opens one listener of its own. Both expected
/// identities are paths this test chose before the census ran, so neither is
/// read back out of the census under validation.
#[test]
fn unix_descriptor_census_charges_only_self_opened_sockets() {
    let _isolation = census_isolation();
    let fixture = tempfile::tempdir().expect("census fixture root");
    let leaked_path = fixture.path().join("leaked.sock");
    let owned_path = fixture.path().join("owned.sock");
    let leaked_identity = leaked_path.to_str().expect("UTF-8 fixture path").to_owned();
    let owned_identity = owned_path.to_str().expect("UTF-8 fixture path").to_owned();

    // `lsof` names an accepted peer by the listener's bound `sun_path`, so the
    // leaked descriptor's identity is a path this test already knows.
    let leaked_listener = UnixListener::bind(&leaked_path).expect("bind harness leak socket");
    let far_end = UnixStream::connect(&leaked_path).expect("connect harness leak socket");
    let (harness_copy, _) = leaked_listener
        .accept()
        .expect("accept harness leak socket");
    let inherited = harness_copy
        .try_clone()
        .expect("duplicate the leaked descriptor for the child");

    let child = Command::new(std::env::current_exe().expect("current test executable"))
        .args(["--exact", "unix_descriptor_census_fixture_process"])
        .env("KELD_T2_CENSUS_OWNED_SOCKET", &owned_path)
        .stdin(Stdio::from(OwnedFd::from(inherited)))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("launch Unix-descriptor census fixture");
    let child_pid = child.id();
    let deadline = Instant::now() + PROCESS_DEADLINE;
    while !owned_path.exists() {
        assert!(
            Instant::now() < deadline,
            "census fixture never bound its own Unix listener"
        );
        thread::yield_now();
    }

    // The fixture is only meaningful while the leak is real: prove the child
    // holds the inherited descriptor before asserting that it is not charged.
    let observed = unix_socket_identities(child_pid);
    assert!(
        observed.contains(&leaked_identity),
        "fixture leaked no harness Unix descriptor into the child: {observed:?}"
    );
    assert_eq!(
        unix_sockets_not_inherited_from_harness(child_pid),
        vec![owned_identity],
        "census must charge the child only the listener it bound itself: {observed:?}"
    );

    // Closing the far end is the child's release signal; the harness keeps its
    // own copy and the listener until scope end, so the leaked identity stays in
    // the harness table for the whole census above.
    drop(far_end);
    let output = wait_child_output(child, PROCESS_DEADLINE);
    assert!(output.status.success(), "census fixture failed: {output:?}");
    await_process_gone(child_pid);
}

/// KEL-222: a copy of a socket the harness does not hold is charged.
///
/// A target holding two descriptors on a socket the harness holds once looks the
/// same to `lsof` whether it `dup`ed an inherited descriptor or opened the socket
/// itself and passed a copy up over `SCM_RIGHTS`. The census charges the extra
/// copy, because the dangerous reading of an ambiguous picture is the one where a
/// process opened an app link. This pins that choice using the half of the
/// ambiguity that needs no `SCM_RIGHTS` support to build. A census that excused
/// every copy of a harness-held address charges nothing here and passes.
///
/// Fixture integrity is checked by socket address, and the harness's own copy is
/// counted rather than assumed: by name alone this fixture would look correct
/// while handing down two *different* sockets that share one `sun_path`, and it
/// would then be testing nothing.
///
/// The second copy lands on stderr rather than stdout because the child's test
/// harness writes its result line to stdout after the harness has closed the
/// socket's far end, which would fail the child on `EPIPE`.
#[test]
fn unix_descriptor_census_charges_a_copy_the_harness_does_not_hold() {
    let _isolation = census_isolation();
    let fixture = tempfile::tempdir().expect("census fixture root");
    let leaked_path = fixture.path().join("leaked.sock");
    let owned_path = fixture.path().join("owned.sock");
    let leaked_identity = leaked_path.to_str().expect("UTF-8 fixture path").to_owned();
    let owned_identity = owned_path.to_str().expect("UTF-8 fixture path").to_owned();

    let leaked_listener = UnixListener::bind(&leaked_path).expect("bind harness leak socket");
    let far_end = UnixStream::connect(&leaked_path).expect("connect harness leak socket");
    let (harness_copy, _) = leaked_listener
        .accept()
        .expect("accept harness leak socket");
    // The listener reports the same `sun_path` as the peer it accepted, so leaving
    // it open makes this fixture's own integrity check ambiguous — the very
    // ambiguity the census refuses to resolve by name.
    drop(leaked_listener);
    let first = harness_copy
        .try_clone()
        .expect("duplicate the leaked descriptor for the child");
    let second = harness_copy
        .try_clone()
        .expect("duplicate the leaked descriptor a second time");

    let child = Command::new(std::env::current_exe().expect("current test executable"))
        .args(["--exact", "unix_descriptor_census_fixture_process"])
        .env("KELD_T2_CENSUS_OWNED_SOCKET", &owned_path)
        // stdin and stderr, not stdout: the test harness writes its result line to
        // stdout, and the socket's peer is closed before the child exits.
        .stdin(Stdio::from(OwnedFd::from(first)))
        .stdout(Stdio::null())
        .stderr(Stdio::from(OwnedFd::from(second)))
        .spawn()
        .expect("launch Unix-descriptor census fixture");
    let child_pid = child.id();
    let deadline = Instant::now() + PROCESS_DEADLINE;
    while !owned_path.exists() {
        assert!(
            Instant::now() < deadline,
            "census fixture never bound its own Unix listener"
        );
        thread::yield_now();
    }

    // The fixture only means anything while the child holds two descriptors on one
    // socket and the harness holds exactly one.
    let harness_copies: Vec<String> = unix_descriptors(std::process::id())
        .into_iter()
        .filter(|descriptor| descriptor.identity == leaked_identity)
        .map(|descriptor| descriptor.socket)
        .collect();
    assert_eq!(
        harness_copies.len(),
        1,
        "harness must hold exactly one descriptor on the leaked socket: {harness_copies:?}"
    );
    let leaked_socket = harness_copies
        .into_iter()
        .next()
        .expect("the harness copy just counted");
    let child_table = unix_descriptors(child_pid);
    let copies = child_table
        .iter()
        .filter(|descriptor| descriptor.socket == leaked_socket)
        .count();
    let rendered: Vec<String> = child_table
        .iter()
        .map(|descriptor| {
            format!(
                "f{} d{} n{}",
                descriptor.descriptor, descriptor.socket, descriptor.identity
            )
        })
        .collect();
    assert_eq!(
        copies, 2,
        "fixture did not receive one socket twice: {rendered:?}"
    );
    assert_eq!(
        unix_sockets_not_inherited_from_harness(child_pid),
        vec![leaked_identity, owned_identity],
        "census must charge the copy the harness does not hold: {rendered:?}"
    );

    drop(far_end);
    let output = wait_child_output(child, PROCESS_DEADLINE);
    assert!(output.status.success(), "census fixture failed: {output:?}");
    await_process_gone(child_pid);
}

/// KEL-222: an unbound Unix socket is charged to whoever opened it, because the
/// census cannot prove it was inherited.
///
/// `lsof` reports every unbound Unix socket with the same placeholder instead of
/// a `sun_path` or a peer address, so that string cannot show that two
/// descriptors are the same kernel object. Subtracting it would let a process
/// that opened its own anonymous socket look clean whenever the harness happened
/// to hold one too — a silent loss of detection power in exactly the direction
/// this issue exists to close. The expected placeholder is written literally
/// here, so this test pins what `lsof` was measured to print rather than agreeing
/// with whatever the census believes.
#[test]
fn unix_descriptor_census_charges_anonymous_sockets_it_cannot_attribute() {
    let _isolation = census_isolation();
    let harness_anonymous = UnixDatagram::unbound().expect("harness unbound Unix socket");
    assert!(
        unix_socket_identities(std::process::id())
            .iter()
            .any(|identity| identity == "->(none)"),
        "harness holds no anonymous Unix socket, so this test collides with nothing"
    );

    let fixture = tempfile::tempdir().expect("census fixture root");
    let owned_path = fixture.path().join("owned.sock");
    let owned_identity = owned_path.to_str().expect("UTF-8 fixture path").to_owned();
    let mut child = Command::new(std::env::current_exe().expect("current test executable"))
        .args(["--exact", "unix_descriptor_census_fixture_process"])
        .env("KELD_T2_CENSUS_OWNED_SOCKET", &owned_path)
        .env("KELD_T2_CENSUS_ANONYMOUS_SOCKET", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("launch Unix-descriptor census fixture");
    let child_pid = child.id();
    let deadline = Instant::now() + PROCESS_DEADLINE;
    while !owned_path.exists() {
        assert!(
            Instant::now() < deadline,
            "census fixture never bound its own Unix listener"
        );
        thread::yield_now();
    }

    let observed = unix_socket_identities(child_pid);
    assert!(
        observed.iter().any(|identity| identity == "->(none)"),
        "fixture opened no anonymous Unix socket, so there is nothing to attribute: {observed:?}"
    );
    let mut charged = unix_sockets_not_inherited_from_harness(child_pid);
    charged.sort();
    let mut expected = vec!["->(none)".to_owned(), owned_identity];
    expected.sort();
    assert_eq!(
        charged, expected,
        "census must charge the child the anonymous socket it opened itself: {observed:?}"
    );

    drop(child.stdin.take().expect("census fixture release lease"));
    let output = wait_child_output(child, PROCESS_DEADLINE);
    assert!(output.status.success(), "census fixture failed: {output:?}");
    await_process_gone(child_pid);
    drop(harness_anonymous);
}

/// KEL-222: a `sun_path` is not an identity either, because a socket keeps
/// reporting it after that path is unlinked.
///
/// The harness holds an accepted socket still named by a path that no longer
/// exists on disk, and the child then binds a brand-new listener at that same
/// path. `lsof` prints one name for both, so a name-keyed census excuses the
/// child's own listener; keyed on the socket address the two never collide. This
/// is the second of the two collisions that made name-keying unsound, and it is
/// the one that survives even if `->(none)` is special-cased.
#[test]
fn unix_descriptor_census_charges_a_listener_rebound_on_a_released_path() {
    let _isolation = census_isolation();
    let fixture = tempfile::tempdir().expect("census fixture root");
    let shared_path = fixture.path().join("released.sock");
    let shared_identity = shared_path.to_str().expect("UTF-8 fixture path").to_owned();

    // Keep an accepted end, then release the name: the descriptor still reports it.
    let listener = UnixListener::bind(&shared_path).expect("bind harness release socket");
    let far_end = UnixStream::connect(&shared_path).expect("connect harness release socket");
    let (harness_stale, _) = listener.accept().expect("accept harness release socket");
    drop(listener);
    fs::remove_file(&shared_path).expect("release the fixture path");
    assert!(
        unix_socket_identities(std::process::id()).contains(&shared_identity),
        "harness lost the stale name, so this test collides with nothing"
    );

    let mut child = Command::new(std::env::current_exe().expect("current test executable"))
        .args(["--exact", "unix_descriptor_census_fixture_process"])
        .env("KELD_T2_CENSUS_OWNED_SOCKET", &shared_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("launch Unix-descriptor census fixture");
    let child_pid = child.id();
    let deadline = Instant::now() + PROCESS_DEADLINE;
    while !shared_path.exists() {
        assert!(
            Instant::now() < deadline,
            "census fixture never rebound the released path"
        );
        thread::yield_now();
    }

    let observed = unix_socket_identities(child_pid);
    assert!(
        observed.contains(&shared_identity),
        "fixture did not rebind the released path: {observed:?}"
    );
    assert_eq!(
        unix_sockets_not_inherited_from_harness(child_pid),
        vec![shared_identity],
        "census must charge the listener the child bound itself: {observed:?}"
    );

    drop(child.stdin.take().expect("census fixture release lease"));
    let output = wait_child_output(child, PROCESS_DEADLINE);
    assert!(output.status.success(), "census fixture failed: {output:?}");
    await_process_gone(child_pid);
    drop((harness_stale, far_end));
}

/// KEL-222: an empty census must prove the process still has a descriptor table.
///
/// `lsof` exits 0 with no rows for a live process owning no Unix socket and also for
/// one whose table has already been torn down on the way out, and zero is the
/// *passing* value for the CLI. [`unix_descriptors`] therefore corroborates an empty
/// census with [`process_has_open_descriptors`]. The exiting window cannot be entered
/// on demand, so what is pinned here is the corroboration: it answers yes for a live
/// process and no for one that is gone. A stub that always answers yes fails this
/// test, and the census then accepts teardown as evidence.
///
/// This test does not call [`unix_descriptors`], so deleting the empty-result reject
/// stays green here;
/// [`empty_unix_census_of_a_process_without_a_descriptor_table_is_rejected`] is the
/// load-bearing path.
#[test]
fn unix_descriptor_census_requires_a_live_descriptor_table() {
    let _isolation = census_isolation();
    assert!(
        process_has_open_descriptors(std::process::id()),
        "this harness is running, so it has a descriptor table"
    );
    let child = Command::new("/usr/bin/true")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("launch a fixture that exits immediately");
    let pid = child.id();
    let output = wait_child_output(child, PROCESS_DEADLINE);
    assert!(output.status.success(), "fixture failed: {output:?}");
    await_process_gone(pid);
    assert!(
        !process_has_open_descriptors(pid),
        "{pid} has exited and been reaped, so it has no descriptor table"
    );
}

/// KEL-222: an empty census of a process without a descriptor table is rejected.
///
/// This is the reject that closes the exiting-window false pass, and it has to be
/// pinned where it lives rather than in a helper beside it. An earlier attempt
/// extracted the assert and called it directly, which pinned the assert and not the
/// census: deleting the census's call to it left all eight tests green. Driving
/// [`unix_descriptors_reported`] with the output instead reaches the same decision
/// the census makes, so deleting the reject turns this red.
///
/// A reaped pid supplies the state that matters — no table — and the empty output
/// supplies the census result that `lsof` gives for both a clean process and one
/// being torn down. The live half of the test is the negative control: the same
/// empty output for a process that does have a table must be accepted, or the
/// reject would just be a ban on empty censuses.
#[test]
fn empty_unix_census_of_a_process_without_a_descriptor_table_is_rejected() {
    let _isolation = census_isolation();
    assert!(
        unix_descriptors_reported(std::process::id(), "").is_empty(),
        "an empty census of this live harness is a result, not an error"
    );

    let child = Command::new("/usr/bin/true")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("launch a fixture that exits immediately");
    let pid = child.id();
    let output = wait_child_output(child, PROCESS_DEADLINE);
    assert!(output.status.success(), "fixture failed: {output:?}");
    await_process_gone(pid);
    let rejected = std::panic::catch_unwind(|| unix_descriptors_reported(pid, ""));
    assert!(
        rejected.is_err(),
        "an empty census of reaped {pid} is teardown and must be rejected"
    );
    let censused = std::panic::catch_unwind(|| unix_descriptors(pid));
    assert!(
        censused.is_err(),
        "censusing reaped {pid} must fail on lsof's exit status too"
    );
}

/// KEL-222: `lsof` output that cannot be trusted fails loudly, never quietly.
///
/// The parser's own guards, driven with the output they exist to reject. A record
/// `lsof` starts and never names would otherwise be dropped in silence, and
/// dropping one of the target's descriptors is the direction that passes. An
/// address that is not one matters because two empty strings compare equal and
/// would excuse each other. Neither shape has been seen in real output, which is
/// why they are pinned here rather than left to a comment.
#[test]
fn untrustworthy_lsof_output_is_rejected() {
    let _isolation = census_isolation();
    let pid = std::process::id();
    assert_eq!(
        unix_descriptors_reported(pid, "f3\nd0xabc\nn/tmp/one.sock\n").len(),
        1,
        "a well-formed record is still accepted"
    );
    for (shape, rendered) in [
        (
            "a record left unnamed",
            "f3\nd0xabc\nf5\nd0xdef\nn/tmp/two.sock\n",
        ),
        (
            "the last record left unnamed",
            "f3\nd0xabc\nn/tmp/one.sock\nf5\nd0xdef\n",
        ),
        ("an address that is not one", "f3\nd\nn/tmp/one.sock\n"),
    ] {
        let rejected = std::panic::catch_unwind(|| unix_descriptors_reported(pid, rendered));
        assert!(rejected.is_err(), "{shape} must be rejected: {rendered:?}");
    }
}

/// KEL-222: owning no Unix descriptor is an empty census, not a census failure.
///
/// Zero is the *passing* value for the CLI, so the census must distinguish "this
/// process owns none" from "the census could not run". This pins that: the child
/// closes every Unix descriptor this harness could leak into it, discovered from
/// the harness's own census rather than hard-coded, so it provably owns none, and
/// reports readiness on its own pipe so the census never races the `exec`.
///
/// What it pins is the `lsof` exit contract, not attribution. The second
/// assertion below cannot fail once the first passes, because the charged set is
/// always a subset of the observed one — it is kept as a statement of the
/// relationship, not as independent detection. Attribution is proved by
/// [`unix_descriptor_census_charges_only_self_opened_sockets`] and the two
/// collision tests, each of which fails against a census that gets it wrong.
#[test]
fn unix_descriptor_census_of_a_process_without_unix_descriptors_is_empty() {
    let _isolation = census_isolation();
    let mut closes = String::new();
    for record in unix_descriptors(std::process::id()) {
        // 0, 1 and 2 are replaced by the spawn's own stdio redirection.
        if !matches!(record.descriptor.as_str(), "0" | "1" | "2") {
            closes.push_str("exec ");
            closes.push_str(&record.descriptor);
            closes.push_str(">&-; ");
        }
    }
    let mut child = Command::new("/bin/sh")
        .arg("-c")
        .arg(format!("{closes}echo READY; exec /bin/cat"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("launch descriptor-free census fixture");
    let child_pid = child.id();
    // A fixture that never reports must fail this test, not hang it.
    let readiness = child.stdout.take().expect("census fixture readiness pipe");
    let (sender, reported) = mpsc::channel();
    let reader = thread::spawn(move || {
        let mut line = String::new();
        let _ = sender.send(BufReader::new(readiness).read_line(&mut line).map(|_| line));
    });
    let ready = reported
        .recv_timeout(PROCESS_DEADLINE)
        .expect("descriptor-free fixture reports readiness within its deadline")
        .expect("read descriptor-free fixture readiness");
    assert_eq!(ready.trim_end(), "READY");
    reader.join().expect("readiness reader joins");

    let observed = unix_socket_identities(child_pid);
    assert!(
        observed.is_empty(),
        "descriptor-free fixture still owns Unix descriptors: {observed:?}"
    );
    let opened = unix_sockets_not_inherited_from_harness(child_pid);
    assert!(
        opened.is_empty(),
        "descriptor-free fixture was charged Unix descriptors: {opened:?}"
    );

    drop(child.stdin.take().expect("census fixture release lease"));
    let output = wait_child_output(child, PROCESS_DEADLINE);
    assert!(output.status.success(), "census fixture failed: {output:?}");
    await_process_gone(child_pid);
}

#[test]
fn shipping_keld_dev_delegates_to_host_and_cli_death_reaps_the_session() {
    let fixture = ProductFixture::new("t2-cli-delegation");
    let _prepared_project = fixture.stage();
    let helper = prepare_keld_dev_helper(&fixture);
    let baseline_stages = dev_stage_count(&fixture.project);

    let mut killed = ShippingDevCycle::launch(&fixture, &helper, "t2-cli");
    let killed_evidence = killed.evidence();
    killed.kill_cli_and_expect_lease_shutdown();
    assert_eq!(dev_stage_count(&fixture.project), baseline_stages);

    let mut signaled = ShippingDevCycle::launch(&fixture, &helper, "t2-cli-sigint");
    let signaled_evidence = signaled.evidence();
    signaled.signal_cli_group_and_expect_lease_shutdown("INT", 2);
    assert_eq!(dev_stage_count(&fixture.project), baseline_stages);

    let mut hung_up = ShippingDevCycle::launch(&fixture, &helper, "t2-cli-sighup");
    let hung_up_evidence = hung_up.evidence();
    hung_up.signal_cli_group_and_expect_lease_shutdown("HUP", 1);
    assert_eq!(dev_stage_count(&fixture.project), baseline_stages);

    let mut failed = ShippingDevCycle::launch(&fixture, &helper, "t2-cli-host-failure");
    let failed_evidence = failed.evidence();
    failed.self_terminate_and_expect_verbatim_error();
    assert_eq!(dev_stage_count(&fixture.project), baseline_stages);

    let stages_before_orderly = dev_stage_count(&fixture.project);
    let mut orderly = ShippingDevCycle::launch(&fixture, &helper, "t2-cli-relaunch");
    assert_eq!(dev_stage_count(&fixture.project), stages_before_orderly + 1);
    let orderly_evidence = orderly.evidence();
    orderly.quit_and_expect_success();
    assert_eq!(dev_stage_count(&fixture.project), stages_before_orderly);

    eprintln!(
        "KEL96_T2_EVIDENCE killed={killed_evidence} sigint={signaled_evidence} sighup={hung_up_evidence} failed={failed_evidence} relaunch={orderly_evidence} marker={MARKER}"
    );
}

#[test]
fn shipping_keld_dev_lease_loss_reaps_the_recovered_generation() {
    let fixture = ProductFixture::new("t3-cli-lease-after-recovery");
    let _prepared_project = fixture.stage();
    let helper = prepare_keld_dev_helper(&fixture);
    let baseline_stages = dev_stage_count(&fixture.project);
    let mut cycle = ShippingDevCycle::launch(&fixture, &helper, "t3-cli-recovery");
    cycle.crash_and_recover();
    cycle.kill_cli_and_expect_recovered_lease_shutdown();
    assert_eq!(dev_stage_count(&fixture.project), baseline_stages);
}

#[test]
fn private_guardian_discriminator_without_authenticated_handoff_spawns_nothing() {
    let temp = tempfile::tempdir().expect("private-role fixture");
    let marker = temp.path().join("spawned");
    let entry = temp.path().join("entry.ts");
    fs::write(
        &entry,
        format!(
            "await Bun.write({}, 'spawned');\n",
            serde_json::to_string(&marker).expect("marker JSON")
        ),
    )
    .expect("private-role entry");
    let output = Command::new(env!("CARGO_BIN_EXE_keld-host"))
        .arg(keld_runtime::macos_guardian::SUPERVISED_GUARDIAN_ARG)
        .arg(temp.path())
        .arg("entry.ts")
        .arg("1")
        .arg("1")
        .env(
            "KELD_APP_LINK",
            "/tmp/forged#aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        )
        .stdin(Stdio::null())
        .output()
        .expect("invoke private discriminator without bootstrap");

    assert!(
        !output.status.success(),
        "forged private role became success"
    );
    assert!(!marker.exists(), "private argv discriminator spawned Bun");
    let stderr = String::from_utf8(output.stderr).expect("private-role stderr UTF-8");
    assert!(stderr.contains("KELD-RUNTIME-003"), "{stderr}");
    assert!(stderr.contains("registration bootstrap"), "{stderr}");
}

#[test]
fn invalid_dev_lease_contract_fails_before_app_resources() {
    let fixture = ProductFixture::new("invalid-dev-lease");
    for (value, expected) in [
        ("unsupported", "unsupported KELD_DEV_LEASE"),
        ("stdin-v1", "requires the CLI-owned pipe reader"),
    ] {
        let stage = fixture.stage();
        let child = Command::new(stage.host())
            .env("KELD_DEV_LEASE", value)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("launch invalid dev lease host");
        let pid = child.id();
        let output = wait_child_output(child, EVENT_DEADLINE);
        assert!(!output.status.success(), "invalid lease became success");
        let stderr = String::from_utf8(output.stderr).expect("invalid lease stderr UTF-8");
        assert!(stderr.contains("KELD-CORE-037"), "{stderr}");
        assert!(stderr.contains(expected), "{stderr}");
        assert!(native_windows(pid, TITLE).is_empty());
        assert!(
            session_dirs_for(pid).is_empty(),
            "invalid lease created an app-link session"
        );
    }
}

#[test]
fn every_invalid_boot_class_fails_before_transient_window_listener_or_bun() {
    let fixture = ProductFixture::new("invalid");
    let watcher = NativeAbsenceWatcher::compile(fixture.root.path());
    for invalid in InvalidBoot::ALL {
        let stage = fixture.stage();
        invalid.apply(stage.root(), fixture.root.path());
        assert_invalid_stage_is_resource_free(
            &stage,
            &watcher,
            &fixture
                .root
                .path()
                .join(format!("invalid-{}.sock", invalid.name())),
            invalid.name(),
            invalid.expected_code(),
            None,
        );
    }
}

#[test]
fn every_invalid_policy_class_fails_before_transient_window_listener_or_bun() {
    let fixture = ProductFixture::new("invalid-policy");
    let watcher = NativeAbsenceWatcher::compile(fixture.root.path());
    for invalid in InvalidPolicy::ALL {
        let stage = fixture.stage();
        invalid.apply(stage.root());
        assert_invalid_stage_is_resource_free(
            &stage,
            &watcher,
            &fixture
                .root
                .path()
                .join(format!("invalid-policy-{}.sock", invalid.name())),
            invalid.name(),
            invalid.expected_code(),
            None,
        );
    }
}

#[test]
fn retained_policy_read_failure_is_guard004_and_resource_free() {
    let fixture = ProductFixture::new("policy-read-failure");
    let watcher = NativeAbsenceWatcher::compile(fixture.root.path());
    let fault = PolicyReadFault::compile(fixture.root.path());
    let stage = fixture.stage();
    assert_invalid_stage_is_resource_free(
        &stage,
        &watcher,
        &fixture.root.path().join("policy-read-failure.sock"),
        "retained-read-failure",
        "KELD-GUARD004",
        Some(&fault),
    );
    assert!(
        fault.marker.exists(),
        "read fault never reached the retained permissions handle"
    );
}

#[test]
fn stalled_initial_navigation_rolls_back_window_link_and_process_group() {
    let fixture = ProductFixture::new("navigation-timeout");
    let blocker = NavigationBlocker::bind();
    fs::write(
        fixture.project.join("index.html"),
        format!(
            "<!doctype html>{DARK_BG}<title>{TITLE}</title><img src=\"http://127.0.0.1:{}/never\">\n",
            blocker.port
        ),
    )
    .expect("stalled renderer");
    let stage = fixture.stage();
    let control_path = fixture.root.path().join("navigation-timeout.sock");
    let listener = UnixListener::bind(&control_path).expect("bind navigation control");
    listener
        .set_nonblocking(true)
        .expect("nonblocking navigation control");
    let mut child = Command::new(stage.host())
        .env("KELD_DEV_LEASE", "stdin-v1")
        .env("KELD_T1B_CONTROL", &control_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("launch navigation-timeout host");
    let dev_lease_writer = child.stdin.take();
    let host_pid = child.id();
    let control = accept_before(&listener, Instant::now() + EVENT_DEADLINE);
    control
        .set_read_timeout(Some(EVENT_DEADLINE))
        .expect("control deadline");
    let mut observations = BufReader::new(control);
    let hello = read_control_line(&mut observations);
    let mut fields = hello.split_whitespace();
    assert_eq!(fields.next(), Some("HELLO"), "{hello}");
    let bun_pid = parse_pid(fields.next(), &hello);
    let app_link = fields.next().expect("navigation app link");
    let session_dir = PathBuf::from(app_link.rsplit_once('#').expect("app link token").0)
        .parent()
        .expect("session directory")
        .to_path_buf();
    let descendant = read_control_line(&mut observations);
    let descendant_pid = parse_pid(descendant.split_whitespace().nth(1), &descendant);
    blocker
        .connected
        .recv_timeout(EVENT_DEADLINE)
        .expect("WKWebView requested stalled resource");
    await_no_native_windows(host_pid, TITLE);
    drop(dev_lease_writer);
    let output = wait_child_output(child, EVENT_DEADLINE);
    blocker
        .release
        .send(())
        .expect("release blocked navigation");
    blocker.handle.join().expect("navigation blocker joins");

    assert!(!output.status.success(), "stalled navigation became Ready");
    let stderr = String::from_utf8(output.stderr).expect("navigation stderr UTF-8");
    assert!(stderr.contains("KELD-CORE-037"), "{stderr}");
    assert!(stderr.contains("initial renderer navigation"), "{stderr}");
    await_process_gone(bun_pid);
    await_process_gone(descendant_pid);
    assert!(
        !session_dir.exists(),
        "navigation rollback left app-link locator"
    );
    assert!(
        native_windows(host_pid, TITLE).is_empty(),
        "navigation rollback left native window"
    );
}

#[test]
fn pre_ready_bun_crash_is_startup_failure_not_a_recovered_window() {
    let fixture = ProductFixture::new("t3-pre-ready-crash");
    let stage = fixture.stage();
    let attempt_marker = fixture.root.path().join("pre-ready-attempt");
    let mut child = Command::new(stage.host())
        .env("KELD_DEV_LEASE", "stdin-v1")
        .env("KELD_T3_CRASH_BEFORE_HELLO", "1")
        .env("KELD_T3_PRE_READY_MARKER", &attempt_marker)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("launch pre-Ready crash host");
    let _dev_lease_writer = child.stdin.take();
    let host_pid = child.id();
    let output = wait_child_output(child, EVENT_DEADLINE);
    assert!(!output.status.success(), "pre-Ready crash became success");
    let stderr = String::from_utf8(output.stderr).expect("pre-Ready stderr UTF-8");
    assert!(stderr.contains("KELD-CORE-037"), "{stderr}");
    assert!(
        stderr.contains("before its initial authenticated generation bound"),
        "{stderr}"
    );
    assert!(
        !stderr.contains("KELD-RUNTIME-002"),
        "pre-Ready crash restarted to breaker: {stderr}"
    );
    assert!(native_windows(host_pid, TITLE).is_empty());
    assert!(session_dirs_for(host_pid).is_empty());
    let attempts = fs::read_dir(fixture.root.path())
        .expect("list pre-Ready attempts")
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("pre-ready-attempt.")
        })
        .count();
    assert_eq!(attempts, 1, "pre-Ready failure provisioned a successor");
}

#[test]
fn no_flag_host_owns_real_window_session_death_reap_and_ordered_quit() {
    let fixture = ProductFixture::new("product");

    let mut killed = fixture.launch_cycle("host-death");
    killed.assert_live_product();
    let host_status = killed
        .host
        .as_mut()
        .expect("live host")
        .kill()
        .and_then(|()| killed.host.as_mut().expect("live host").wait())
        .expect("SIGKILL only the no-flag host");
    assert_eq!(
        host_status.signal(),
        Some(9),
        "host-only death must be SIGKILL"
    );
    killed.host.take();
    killed.expect_line("LINK_EOF");
    await_process_gone(killed.bun_pid);
    await_process_gone(killed.descendant_pid);
    await_process_gone(killed.guardian_pid);
    assert!(
        !killed.session_dir.exists(),
        "host death left the app-link locator"
    );
    killed.group_gone = true;

    let mut self_terminated = fixture.launch_cycle("self-termination");
    self_terminated.assert_live_product();
    self_terminated
        .control_writer
        .write_all(b"EXIT0\n")
        .expect("request unrequested status-zero Bun exit");
    let output = self_terminated.wait_host();
    assert!(
        !output.status.success(),
        "unrequested status-zero Bun exit became host success"
    );
    let stderr = String::from_utf8(output.stderr).expect("self-termination stderr UTF-8");
    assert!(stderr.contains("KELD-CORE-033"), "{stderr}");
    assert!(stderr.contains("KELD-RUNTIME-012"), "{stderr}");
    await_process_gone(self_terminated.bun_pid);
    await_process_gone(self_terminated.descendant_pid);
    await_process_gone(self_terminated.guardian_pid);
    assert!(
        !self_terminated.session_dir.exists(),
        "self-termination left the app-link locator"
    );
    self_terminated.group_gone = true;

    let mut guardian_failed = fixture.launch_cycle("guardian-failure");
    guardian_failed.assert_live_product();
    kill_pid(guardian_failed.guardian_pid);
    let output = guardian_failed.wait_host();
    assert!(
        !output.status.success(),
        "guardian death became host success"
    );
    let stderr = String::from_utf8(output.stderr).expect("guardian-failure stderr UTF-8");
    assert!(stderr.contains("KELD-CORE-033"), "{stderr}");
    assert!(stderr.contains("KELD-RUNTIME-013"), "{stderr}");
    await_process_gone(guardian_failed.bun_pid);
    await_process_gone(guardian_failed.descendant_pid);
    await_process_gone(guardian_failed.guardian_pid);
    assert!(
        native_windows(guardian_failed.host_pid, TITLE).is_empty(),
        "guardian failure left a native window"
    );
    guardian_failed.group_gone = true;

    let mut orderly = fixture.launch_cycle("relaunch-orderly");
    orderly.assert_live_product();
    orderly
        .control_writer
        .write_all(b"QUIT\n")
        .expect("request fixture app.quit");
    orderly.expect_line("QUIT_REPLY");
    orderly.expect_line("LINK_EOF");
    let output = orderly.wait_host();
    assert!(
        output.status.success(),
        "ordered no-flag host exit: {output:?}"
    );
    let stderr = String::from_utf8(output.stderr).expect("host stderr UTF-8");
    assert!(
        !stderr.contains("pre-alpha"),
        "no-flag product launch returned through the old banner: {stderr}"
    );
    await_process_gone(orderly.bun_pid);
    await_process_gone(orderly.descendant_pid);
    await_process_gone(orderly.guardian_pid);
    assert!(
        !orderly.session_dir.exists(),
        "Quit left the app-link locator"
    );
    assert!(
        native_windows(orderly.host_pid, TITLE).is_empty(),
        "host exit left a native window"
    );
    orderly.group_gone = true;
}

#[test]
fn no_flag_host_recovers_a_fresh_generation_in_the_same_native_window() {
    let fixture = ProductFixture::new("t3-generation-recovery");
    let mut recovery = RecoveryCycle::launch(&fixture, "recovery-quit");
    let first = recovery.crash_and_recover();
    let second = recovery.current_evidence();
    recovery.quit_and_expect_success();

    eprintln!(
        "KEL96_T3_EVIDENCE host={} window={} guardian={} first_bun={} second_bun={} old_link={} new_link={} marker={MARKER}",
        recovery.host_pid,
        recovery.window[0],
        first.guardian_pid,
        first.bun_pid,
        second.bun_pid,
        first.app_link,
        second.app_link,
    );
}

#[test]
fn recovered_generation_is_the_target_of_host_and_guardian_death_cleanup() {
    let fixture = ProductFixture::new("t3-death-after-recovery");

    let mut host_death = RecoveryCycle::launch(&fixture, "host-death-g2");
    host_death.crash_and_recover();
    host_death.kill_host_and_expect_current_group_reaped();

    let mut guardian_death = RecoveryCycle::launch(&fixture, "guardian-death-g2");
    guardian_death.crash_and_recover();
    guardian_death.kill_guardian_and_expect_current_group_reaped();
}

#[test]
fn live_child_link_loss_restarts_through_the_generation_owner() {
    let fixture = ProductFixture::new("t3-link-loss");
    let mut cycle = RecoveryCycle::launch(&fixture, "link-loss");
    cycle.close_link_and_recover();
    cycle.quit_and_expect_success();
}

#[test]
fn third_generation_crash_trips_breaker_without_a_fourth_generation() {
    let fixture = ProductFixture::new("t3-crash-loop");
    let mut cycle = RecoveryCycle::launch(&fixture, "crash-loop");
    cycle.crash_and_recover();
    cycle.crash_and_recover();
    {
        let current = cycle.current.as_mut().expect("third generation");
        current
            .writer
            .write_all(b"CRASH_ACKED\n")
            .expect("request acknowledged threshold crash");
        current.expect_line("CRASH_ACK");
    }
    let third = cycle.current_evidence();
    let output = cycle.wait_host();
    assert!(
        !output.status.success(),
        "acknowledged crash loop became success: status={:?}",
        output.status
    );
    let stderr = String::from_utf8(output.stderr).expect("crash-loop stderr UTF-8");
    eprintln!(
        "KEL260_BREAKER host={} guardian={} bun={} descendant={} status={} stderr={stderr}",
        cycle.host_pid, third.guardian_pid, third.bun_pid, third.descendant_pid, output.status
    );
    assert!(stderr.contains("KELD-CORE-033"), "{stderr}");
    assert!(stderr.contains("KELD-RUNTIME-002"), "{stderr}");
    assert!(
        matches!(cycle.listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock),
        "crash-loop threshold provisioned a fourth generation"
    );
    cycle.assert_current_group_gone();
    assert!(
        !third.endpoint.exists(),
        "crash loop left the app-link endpoint"
    );

    let mut relaunched = RecoveryCycle::launch(&fixture, "after-crash-loop");
    relaunched.quit_and_expect_success();
}

struct RecoveryCycle {
    host: Option<Child>,
    dev_lease_writer: Option<ChildStdin>,
    host_pid: u32,
    listener: UnixListener,
    window: Vec<u32>,
    current: Option<RecoveryGeneration>,
    process_groups: Vec<u32>,
}

#[derive(Clone)]
struct RecoveryEvidence {
    guardian_pid: u32,
    bun_pid: u32,
    descendant_pid: u32,
    app_link: String,
    endpoint: PathBuf,
    token: String,
}

impl RecoveryCycle {
    fn launch(fixture: &ProductFixture, name: &str) -> Self {
        let beacon = Beacon::bind(MARKER);
        fs::write(
            fixture.project.join("index.html"),
            format!(
                "<!doctype html>{DARK_BG}<title>{TITLE}</title><p id=marker>{MARKER}</p><img src=\"http://127.0.0.1:{}/{MARKER}\">\n",
                beacon.port()
            ),
        )
        .expect("T3 renderer with exact beacon");
        let stage = fixture.stage();
        let control_path = fixture.root.path().join(format!("{name}.sock"));
        let listener = UnixListener::bind(&control_path).expect("bind T3 fixture control");
        listener
            .set_nonblocking(true)
            .expect("nonblocking T3 fixture control");
        let mut presentation = fixture.observe_initial_window();
        let mut command = Command::new(stage.host());
        command
            .env("KELD_DEV_LEASE", "stdin-v1")
            .stdin(Stdio::piped())
            .env("KELD_T1B_CONTROL", &control_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command.spawn().expect("launch T3 no-flag host");
        let dev_lease_writer = child.stdin.take();
        let host_pid = child.id();
        let mut cleanup = ShippingLaunchCleanup::new(child);
        let mut current = RecoveryGeneration::accept(&listener, "initial");
        current.expect_ready_and_echoes();
        beacon.assert_exact();
        assert_eq!(parent_process(current.guardian_pid), host_pid);
        assert_eq!(process_group(current.bun_pid), current.bun_pid);
        assert_eq!(process_group(current.descendant_pid), current.bun_pid);
        let first_group = current.bun_pid;
        cleanup.bun_group = Some(first_group);
        let mut cycle = Self {
            host: Some(cleanup.release()),
            dev_lease_writer,
            host_pid,
            listener,
            window: Vec::new(),
            current: Some(current),
            process_groups: vec![first_group],
        };
        let window = presentation.expect_initial(host_pid, "initial T3 native window");
        assert_eq!(window.len(), 1, "initial T3 native window: {window:?}");
        cycle.window = window;
        cycle
    }

    fn crash_and_recover(&mut self) -> RecoveryEvidence {
        self.trigger_and_recover(b"CRASH\n")
    }

    fn close_link_and_recover(&mut self) -> RecoveryEvidence {
        self.trigger_and_recover(b"CLOSE_LINK\n")
    }

    fn trigger_and_recover(&mut self, command: &[u8]) -> RecoveryEvidence {
        let mut retired = self.current.take().expect("live generation");
        let evidence = retired.evidence();
        retired
            .writer
            .write_all(command)
            .expect("terminate current T3 generation");
        let mut successor = match RecoveryGeneration::try_accept(&self.listener, "replacement") {
            Ok(successor) => successor,
            Err(error) => {
                let output = self.wait_host();
                panic!("{error}; host output: {output:?}");
            }
        };
        successor.expect_ready_and_echoes();
        assert_eq!(
            evidence.guardian_pid, successor.guardian_pid,
            "recovery replaced the persistent guardian"
        );
        assert_ne!(
            evidence.bun_pid, successor.bun_pid,
            "Bun generation was reused"
        );
        assert_ne!(
            evidence.app_link, successor.app_link,
            "successor reused the retired endpoint/token"
        );
        assert_ne!(
            evidence.endpoint, successor.endpoint,
            "successor reused endpoint"
        );
        assert_ne!(evidence.token, successor.token(), "successor reused token");
        assert_eq!(
            await_same_native_windows(self.host_pid, TITLE, &self.window),
            self.window,
            "Bun recovery replaced or closed the host-owned native window; target-PID CoreGraphics rows: {}",
            native_window_rows(self.host_pid)
        );
        assert!(
            UnixStream::connect(&evidence.endpoint).is_err(),
            "retired generation endpoint accepted a stale reconnect"
        );
        await_process_gone(evidence.bun_pid);
        await_process_gone(evidence.descendant_pid);
        assert!(
            process_exists(self.host_pid) && process_exists(evidence.guardian_pid),
            "recoverable Bun crash terminated the host or guardian"
        );
        self.process_groups.push(successor.bun_pid);
        self.current = Some(successor);
        evidence
    }

    fn current_evidence(&self) -> RecoveryEvidence {
        self.current
            .as_ref()
            .expect("current generation")
            .evidence()
    }

    fn quit_and_expect_success(&mut self) {
        let current = self.current.as_mut().expect("current generation");
        current
            .writer
            .write_all(b"QUIT\n")
            .expect("Quit T3 generation");
        current.expect_line("QUIT_REPLY");
        current.expect_line("LINK_EOF");
        let output = self.wait_host();
        assert!(
            output.status.success(),
            "T3 orderly exit failed: {output:?}"
        );
        assert!(
            matches!(self.listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock),
            "accepted Quit provisioned a successor generation"
        );
        self.assert_current_group_gone();
    }

    fn kill_host_and_expect_current_group_reaped(&mut self) {
        let status = self
            .host
            .as_mut()
            .expect("live T3 host")
            .kill()
            .and_then(|()| self.host.as_mut().expect("live T3 host").wait())
            .expect("SIGKILL only recovered host");
        assert_eq!(status.signal(), Some(9));
        self.host.take();
        self.current
            .as_mut()
            .expect("current generation")
            .expect_line("LINK_EOF");
        self.assert_current_group_gone();
    }

    fn kill_guardian_and_expect_current_group_reaped(&mut self) {
        let current = self.current_evidence();
        kill_pid(current.guardian_pid);
        let output = self.wait_host();
        assert!(!output.status.success(), "guardian death became success");
        let stderr = String::from_utf8(output.stderr).expect("guardian-death stderr UTF-8");
        assert!(stderr.contains("KELD-CORE-033"), "{stderr}");
        assert!(stderr.contains("KELD-RUNTIME-013"), "{stderr}");
        self.assert_current_group_gone();
    }

    fn wait_host(&mut self) -> Output {
        self.wait_host_observing(|| {})
    }

    fn wait_host_observing(&mut self, observe: impl FnMut()) -> Output {
        // Observing exit must not inject CLI death: lease EOF starts accepted
        // shutdown and can overtake even an acknowledged Bun crash.
        let output = wait_child_output_observing(
            self.host.take().expect("live T3 host"),
            EVENT_DEADLINE,
            observe,
        );
        drop(self.dev_lease_writer.take());
        output
    }

    fn assert_current_group_gone(&mut self) {
        if let Some(current) = &self.current {
            await_process_gone(current.bun_pid);
            await_process_gone(current.descendant_pid);
            await_process_gone(current.guardian_pid);
        }
        assert!(native_windows(self.host_pid, TITLE).is_empty());
        self.process_groups.clear();
    }
}

impl Drop for RecoveryCycle {
    fn drop(&mut self) {
        if let Some(host) = self.host.as_mut() {
            let _ = host.kill();
            let _ = host.wait();
        }
        for group in &self.process_groups {
            let _ = signal_process_group("-KILL", *group);
        }
    }
}

#[test]
fn recovery_wait_preserves_the_cli_lease_until_child_exit() {
    assert_wait_preserves_the_cli_lease(|child, lease, listener, observe| {
        let mut cycle = RecoveryCycle {
            host_pid: child.id(),
            dev_lease_writer: lease,
            host: Some(child),
            listener,
            window: Vec::new(),
            current: None,
            process_groups: Vec::new(),
        };
        cycle.wait_host_observing(observe)
    });
}

#[test]
fn live_wait_preserves_the_cli_lease_until_child_exit() {
    assert_wait_preserves_the_cli_lease(|child, lease, _listener, observe| {
        let (reader, writer) = UnixStream::pair().expect("unused live-cycle control pair");
        let mut cycle = LiveCycle {
            host_pid: child.id(),
            dev_lease_writer: lease,
            host: Some(child),
            guardian_pid: 0,
            bun_pid: 0,
            descendant_pid: 0,
            session_dir: PathBuf::new(),
            control_reader: BufReader::new(reader),
            control_writer: writer,
            beacon: None,
            presentation: None,
            group_gone: true,
        };
        cycle.wait_host_observing(observe)
    });
}

fn assert_wait_preserves_the_cli_lease(
    mut wait: impl FnMut(Child, Option<ChildStdin>, UnixListener, &mut dyn FnMut()) -> Output,
) {
    // The control byte orders the independent pipe observation after entry
    // into the wait. No child exit or elapsed delay can stand in for that edge.
    const PROBE: &str = r#"
import os
import socket
import sys

with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as control:
    control.connect(sys.argv[1])
    assert control.recv(1) == b"P"
    os.set_blocking(0, False)
    try:
        observed = os.read(0, 1)
    except BlockingIOError:
        print("LEASE_LIVE")
        sys.exit(0)
    assert observed == b"", repr(observed)
    print("LEASE_EOF")
    sys.exit(17)
"#;
    let root = tempfile::tempdir().expect("lease probe root");
    // Include deliberate EOF and a healthy follow-up to check both the probe
    // and resource reuse independently of the recovery helper's implementation.
    for (index, release_lease) in [false, true, false].into_iter().enumerate() {
        let path = root.path().join(format!("lease-{index}.sock"));
        let listener = UnixListener::bind(&path).expect("lease probe control");
        listener
            .set_nonblocking(true)
            .expect("nonblocking lease probe control");
        let mut child = Command::new("/usr/bin/python3")
            .args(["-c", PROBE])
            .arg(&path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("start independent lease probe");
        let host_pid = child.id();
        let mut lease = child.stdin.take();
        let mut control = Some(accept_before(&listener, Instant::now() + EVENT_DEADLINE));
        if release_lease {
            drop(lease.take());
        }
        let output = wait(child, lease, listener, &mut || {
            if let Some(mut control) = control.take() {
                control.write_all(b"P").expect("request lease observation");
            }
        });
        assert_eq!(
            output.status.code(),
            Some(if release_lease { 17 } else { 0 }),
            "wait changed the requested lease state: {output:?}"
        );
        assert_eq!(
            output.stdout,
            if release_lease {
                b"LEASE_EOF\n".as_slice()
            } else {
                b"LEASE_LIVE\n".as_slice()
            }
        );
        assert!(output.stderr.is_empty(), "{output:?}");
        await_process_gone(host_pid);
    }
}

struct RecoveryGeneration {
    guardian_pid: u32,
    bun_pid: u32,
    descendant_pid: u32,
    app_link: String,
    endpoint: PathBuf,
    reader: BufReader<UnixStream>,
    writer: UnixStream,
}

impl RecoveryGeneration {
    fn accept(listener: &UnixListener, label: &str) -> Self {
        Self::try_accept(listener, label).unwrap_or_else(|error| panic!("{error}"))
    }

    fn try_accept(listener: &UnixListener, label: &str) -> Result<Self, String> {
        let deadline = Instant::now() + EVENT_DEADLINE;
        let stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    if Instant::now() >= deadline {
                        return Err(format!("{label}: Bun did not connect the fixture control"));
                    }
                    thread::yield_now();
                }
                Err(error) => return Err(format!("{label}: accept fixture control: {error}")),
            }
        };
        stream
            .set_nonblocking(false)
            .map_err(|error| format!("{label}: normalize T3 control stream: {error}"))?;
        stream
            .set_read_timeout(Some(EVENT_DEADLINE))
            .map_err(|error| format!("{label}: T3 control deadline: {error}"))?;
        let mut reader = BufReader::new(
            stream
                .try_clone()
                .map_err(|error| format!("{label}: T3 control reader: {error}"))?,
        );
        let hello = read_control_line(&mut reader);
        let mut hello_fields = hello.split_whitespace();
        assert_eq!(hello_fields.next(), Some("HELLO"), "{label}: {hello}");
        let bun_pid = parse_pid(hello_fields.next(), &hello);
        let app_link = hello_fields
            .next()
            .unwrap_or_else(|| panic!("{label}: missing app link: {hello}"))
            .to_owned();
        let endpoint = PathBuf::from(
            app_link
                .rsplit_once('#')
                .unwrap_or_else(|| panic!("{label}: invalid app link: {app_link}"))
                .0,
        );
        let descendant = read_control_line(&mut reader);
        let descendant_pid = parse_pid(descendant.split_whitespace().nth(1), &descendant);
        let guardian_pid = parent_process(bun_pid);
        Ok(Self {
            guardian_pid,
            bun_pid,
            descendant_pid,
            app_link,
            endpoint,
            reader,
            writer: stream,
        })
    }

    fn expect_ready_and_echoes(&mut self) {
        self.expect_line("READY");
        self.expect_line("ECHO1");
        self.expect_line("ECHO2");
    }

    fn expect_line(&mut self, expected: &str) {
        assert_eq!(read_control_line(&mut self.reader), expected);
    }

    fn evidence(&self) -> RecoveryEvidence {
        RecoveryEvidence {
            guardian_pid: self.guardian_pid,
            bun_pid: self.bun_pid,
            descendant_pid: self.descendant_pid,
            app_link: self.app_link.clone(),
            endpoint: self.endpoint.clone(),
            token: self.token().to_owned(),
        }
    }

    fn token(&self) -> &str {
        self.app_link
            .rsplit_once('#')
            .expect("recovery app link token")
            .1
    }
}

#[test]
fn dev_lease_bytes_are_non_authority_and_only_eof_stops_the_host() {
    let fixture = ProductFixture::new("dev-lease-data");
    let (mut cycle, mut lease_writer) = fixture.launch_leased_cycle("lease-data");
    cycle.assert_live_product();
    let (written_tx, written_rx) = mpsc::channel();
    let writer_thread = thread::spawn(move || {
        let result = lease_writer.write_all(&vec![b'x'; 1024 * 1024]);
        written_tx
            .send((lease_writer, result))
            .expect("return lease writer");
    });
    let (lease_writer, write_result) = match written_rx.recv_timeout(PROCESS_DEADLINE) {
        Ok(result) => result,
        Err(error) => {
            if let Some(host) = cycle.host.as_mut() {
                let _ = host.kill();
                let _ = host.wait();
            }
            let _ = signal_process_group("-KILL", cycle.bun_pid);
            cycle.group_gone = true;
            writer_thread.join().expect("lease writer joins after kill");
            panic!("host did not drain liveness-only bytes: {error}");
        }
    };
    writer_thread.join().expect("lease writer joins");
    write_result.expect("write liveness-only lease bytes");
    cycle
        .control_writer
        .write_all(b"ECHO3\n")
        .expect("request post-data echo");
    cycle.expect_line("ECHO3");

    drop(lease_writer);
    cycle.expect_line("LINK_EOF");
    let output = cycle.wait_host();
    assert!(output.status.success(), "lease-loss shutdown: {output:?}");
    await_process_gone(cycle.bun_pid);
    await_process_gone(cycle.descendant_pid);
    await_process_gone(cycle.guardian_pid);
    assert!(native_windows(cycle.host_pid, TITLE).is_empty());
    cycle.group_gone = true;
}

fn prepare_keld_dev_helper(fixture: &ProductFixture) -> PathBuf {
    let helper_dir = fixture.root.path().join("t2-cli-bin");
    fs::create_dir(&helper_dir).expect("T2 helper directory");
    let helper = helper_dir.join("keld-dev-helper");
    fs::copy(
        std::env::current_exe().expect("current test executable"),
        &helper,
    )
    .expect("copy T2 helper executable");
    fs::set_permissions(&helper, fs::Permissions::from_mode(0o700))
        .expect("make T2 helper executable");
    let developer_host = helper_dir.join("keld-host");
    fs::copy(env!("CARGO_BIN_EXE_keld-host"), &developer_host)
        .expect("copy developer host beside CLI helper");
    fs::set_permissions(&developer_host, fs::Permissions::from_mode(0o500))
        .expect("make developer host executable");
    helper
}

fn dev_stage_count(project: &Path) -> usize {
    fs::read_dir(project.join(".keld/dev"))
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_dir())
        .count()
}

struct ShippingDevCycle {
    cli: Option<Child>,
    cli_pid: u32,
    host_pid: u32,
    guardian_pid: u32,
    bun_pid: u32,
    descendant_pid: u32,
    session_dir: PathBuf,
    listener: UnixListener,
    control_reader: BufReader<UnixStream>,
    control_writer: UnixStream,
    group_gone: bool,
}

/// Owns a shipping launch until its authenticated process tree is complete.
/// A post-launch assertion may panic before [`ShippingDevCycle`] exists; this
/// guard keeps that failure path from orphaning the CLI lease, host/guardian,
/// or supervised Bun group.
struct ShippingLaunchCleanup {
    cli: Option<Child>,
    host_group: Option<u32>,
    bun_group: Option<u32>,
}

impl ShippingLaunchCleanup {
    fn new(cli: Child) -> Self {
        Self {
            cli: Some(cli),
            host_group: None,
            bun_group: None,
        }
    }

    fn record_authenticated_groups(&mut self, host_group: u32, bun_group: u32) {
        let test_group = process_group(std::process::id());
        self.host_group = (host_group != 0 && host_group != test_group).then_some(host_group);
        self.bun_group = (bun_group != 0 && bun_group != test_group).then_some(bun_group);
    }

    fn release(mut self) -> Child {
        self.host_group = None;
        self.bun_group = None;
        self.cli.take().expect("shipping CLI cleanup owner")
    }
}

impl Drop for ShippingLaunchCleanup {
    fn drop(&mut self) {
        if let Some(cli) = self.cli.as_mut()
            && cli.try_wait().ok().flatten().is_none()
        {
            let _ = cli.kill();
            let _ = cli.wait();
        }
        for group in [self.host_group, self.bun_group].into_iter().flatten() {
            let _ = signal_process_group("-TERM", group);
        }
    }
}

#[test]
fn shipping_launch_cleanup_reaps_each_owned_process_group() {
    let cli = Command::new("/bin/sleep")
        .arg("60")
        .process_group(0)
        .spawn()
        .expect("launch disposable CLI group");
    let cli_pid = cli.id();
    let mut cleanup = ShippingLaunchCleanup::new(cli);
    let mut host = Command::new("/bin/sh")
        .args(["-c", "sleep 60 & child=$!; printf '%s\\n' \"$child\"; wait"])
        .process_group(0)
        .stdout(Stdio::piped())
        .spawn()
        .expect("launch disposable host group");
    let host_pid = host.id();
    cleanup.host_group = Some(host_pid);
    let mut descendant_line = String::new();
    let descendant_read = BufReader::new(host.stdout.take().expect("host child PID pipe"))
        .read_line(&mut descendant_line)
        .expect("read host child PID");
    assert_ne!(descendant_read, 0, "host child PID missing");
    let host_descendant_pid = descendant_line
        .trim()
        .parse::<u32>()
        .expect("numeric host child PID");
    let bun = Command::new("/bin/sleep")
        .arg("60")
        .process_group(0)
        .spawn()
        .expect("launch disposable Bun group");
    let bun_pid = bun.id();
    cleanup.bun_group = Some(bun_pid);
    assert_eq!(process_group(cli_pid), cli_pid);
    assert_eq!(process_group(host_pid), host_pid);
    assert_eq!(process_group(bun_pid), bun_pid);

    drop(cleanup);
    assert!(
        wait_child_output(host, PROCESS_DEADLINE)
            .status
            .signal()
            .is_some()
    );
    assert!(
        wait_child_output(bun, PROCESS_DEADLINE)
            .status
            .signal()
            .is_some()
    );
    await_process_gone(cli_pid);
    await_process_gone(host_pid);
    await_process_gone(host_descendant_pid);
    await_process_gone(bun_pid);
}

#[test]
fn single_pid_termination_cannot_substitute_for_group_cleanup() {
    let leader = Command::new("/bin/sh")
        .args(["-c", "sleep 60 & child=$!; printf '%s\\n' \"$child\"; wait"])
        .process_group(0)
        .stdout(Stdio::piped())
        .spawn()
        .expect("launch disposable process-group leader");
    let leader_pid = leader.id();
    let mut cleanup = ShippingLaunchCleanup::new(leader);
    cleanup.host_group = Some(leader_pid);
    let mut descendant_line = String::new();
    let descendant_read = BufReader::new(
        cleanup
            .cli
            .as_mut()
            .expect("leader cleanup owner")
            .stdout
            .take()
            .expect("leader child PID pipe"),
    )
    .read_line(&mut descendant_line)
    .expect("read leader child PID");
    assert_ne!(descendant_read, 0, "leader child PID missing");
    let descendant_pid = descendant_line
        .trim()
        .parse::<u32>()
        .expect("numeric leader child PID");
    assert_eq!(process_group(leader_pid), leader_pid);

    kill_pid(leader_pid);
    let _ = cleanup
        .cli
        .as_mut()
        .expect("leader cleanup owner")
        .wait()
        .expect("reap disposable leader");
    assert!(
        process_exists(descendant_pid),
        "single-PID termination unexpectedly reaped its group descendant"
    );
    signal_process_group("-KILL", leader_pid).expect("kill disposable descendant group");
    await_process_gone(descendant_pid);
    cleanup.host_group = None;
    cleanup.cli.take();
}

#[test]
fn process_group_cleanup_rejects_non_group_broadcast_values() {
    for group in [0, 1] {
        let error = validate_process_group(group)
            .expect_err("invalid process group must be rejected before kill(2)");
        assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
    }
}

impl ShippingDevCycle {
    fn launch(fixture: &ProductFixture, helper: &Path, name: &str) -> Self {
        let beacon = Beacon::bind(MARKER);
        fs::write(
            fixture.project.join("index.html"),
            format!(
                "<!doctype html>{DARK_BG}<title>{TITLE}</title><p id=marker>{MARKER}</p><img src=\"http://127.0.0.1:{}/{MARKER}\">\n",
                beacon.port()
            ),
        )
        .expect("T2 renderer with exact beacon");
        let control_path = fixture.root.path().join(format!("{name}.sock"));
        let listener = UnixListener::bind(&control_path).expect("bind T2 fixture control");
        listener
            .set_nonblocking(true)
            .expect("nonblocking T2 fixture control");
        let mut presentation = fixture.observe_initial_window();
        let cli = Command::new(helper)
            .args(["--exact", "keld_dev_helper_process", "--nocapture"])
            .process_group(0)
            .current_dir(&fixture.project)
            .env("KELD_T2_HELPER_PROJECT", &fixture.project)
            .env("KELD_T1B_CONTROL", &control_path)
            .env("KELD_T2_EXIT_ON_LINK_EOF", "1")
            .env(
                "KELD_T2_HIGH_VOLUME_LOG",
                if name == "t2-cli-relaunch" { "1" } else { "0" },
            )
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("launch shipping keld dev helper");
        let cli_pid = cli.id();
        let mut cleanup = ShippingLaunchCleanup::new(cli);
        let control = accept_before(&listener, Instant::now() + EVENT_DEADLINE);
        control
            .set_read_timeout(Some(EVENT_DEADLINE))
            .expect("T2 control read deadline");
        let mut control_reader = BufReader::new(control.try_clone().expect("T2 control reader"));
        let hello = read_control_line(&mut control_reader);
        let mut hello_fields = hello.split_whitespace();
        assert_eq!(hello_fields.next(), Some("HELLO"), "{hello}");
        let bun_pid = parse_pid(hello_fields.next(), &hello);
        let app_link = hello_fields.next().expect("T2 app link");
        let session_dir = PathBuf::from(app_link.rsplit_once('#').expect("T2 app link token").0)
            .parent()
            .expect("T2 session directory")
            .to_path_buf();
        let descendant = read_control_line(&mut control_reader);
        let descendant_pid = parse_pid(descendant.split_whitespace().nth(1), &descendant);
        let guardian_pid = parent_process(bun_pid);
        let host_pid = parent_process(guardian_pid);
        let owns_expected_tree =
            parent_process(host_pid) == cli_pid && guardian_pid != cli_pid && host_pid != cli_pid;
        assert!(
            owns_expected_tree,
            "shipping keld dev did not delegate CLI {cli_pid} -> host {host_pid} -> guardian {guardian_pid} -> Bun {bun_pid}"
        );
        cleanup.record_authenticated_groups(host_pid, bun_pid);
        let mut cycle = Self {
            cli: Some(cleanup.release()),
            cli_pid,
            host_pid,
            guardian_pid,
            bun_pid,
            descendant_pid,
            session_dir,
            listener,
            control_reader,
            control_writer: control,
            group_gone: false,
        };
        assert_eq!(process_group(bun_pid), bun_pid);
        assert_eq!(process_group(descendant_pid), bun_pid);
        assert_eq!(process_group(cli_pid), cli_pid);
        assert_eq!(process_group(host_pid), host_pid);
        assert_eq!(process_group(guardian_pid), host_pid);
        assert_eq!(read_control_line(&mut cycle.control_reader), "READY");
        assert_eq!(read_control_line(&mut cycle.control_reader), "ECHO1");
        assert_eq!(read_control_line(&mut cycle.control_reader), "ECHO2");
        beacon.assert_exact();
        assert_eq!(presentation.expect_initial(cycle.host_pid, name).len(), 1);
        assert!(native_windows(cli_pid, TITLE).is_empty());
        assert!(
            !unix_sockets_not_inherited_from_harness(cycle.host_pid).is_empty(),
            "host owns no Unix app-link descriptor of its own"
        );
        let cli_sockets = unix_sockets_not_inherited_from_harness(cli_pid);
        assert!(
            cli_sockets.is_empty(),
            "CLI {cli_pid} owns Unix descriptors it did not inherit from this harness: \
             {cli_sockets:?}"
        );
        if name == "t2-cli" {
            assert_lease_descriptor_ownership(
                cli_pid,
                cycle.host_pid,
                cycle.guardian_pid,
                cycle.bun_pid,
            );
        }
        cycle
    }

    fn evidence(&self) -> String {
        format!(
            "{}/{}/{}/{}/{}",
            self.cli_pid, self.host_pid, self.guardian_pid, self.bun_pid, self.descendant_pid
        )
    }

    fn crash_and_recover(&mut self) {
        let old_guardian = self.guardian_pid;
        let old_bun = self.bun_pid;
        let old_descendant = self.descendant_pid;
        let old_link = self.session_dir.clone();
        let window = query_native_windows(
            self.host_pid,
            TITLE,
            NativeWindowScope::All,
            NativeWindowExpectation::Snapshot,
            "shipping-recovery-before",
        );
        // Initial presentation was proved by launch(). During recovery, a
        // Space change can remove a live window from the on-screen list.
        assert!(
            !window.is_empty(),
            "recovery requires a live window identity"
        );
        self.control_writer
            .write_all(b"CRASH\n")
            .expect("crash shipping generation");
        let mut successor = RecoveryGeneration::accept(&self.listener, "shipping replacement");
        successor.expect_ready_and_echoes();
        assert_eq!(successor.guardian_pid, old_guardian);
        assert_ne!(successor.bun_pid, old_bun);
        assert_eq!(
            query_native_windows(
                self.host_pid,
                TITLE,
                NativeWindowScope::All,
                NativeWindowExpectation::Snapshot,
                "shipping-recovery-after",
            ),
            window
        );
        assert!(
            !old_link.exists(),
            "retired shipping link directory remains"
        );
        await_process_gone(old_bun);
        await_process_gone(old_descendant);
        self.guardian_pid = successor.guardian_pid;
        self.bun_pid = successor.bun_pid;
        self.descendant_pid = successor.descendant_pid;
        self.session_dir = successor
            .endpoint
            .parent()
            .expect("successor session directory")
            .to_path_buf();
        self.control_reader = successor.reader;
        self.control_writer = successor.writer;
    }

    fn kill_cli_and_expect_lease_shutdown(&mut self) {
        let cli = self.cli.as_mut().expect("live shipping CLI");
        cli.kill().expect("SIGKILL only the shipping CLI");
        let status = cli.wait().expect("wait killed shipping CLI");
        assert_eq!(status.signal(), Some(9));
        assert_eq!(read_control_line(&mut self.control_reader), "LINK_EOF");
        self.assert_group_gone();
        assert!(
            !self.session_dir.exists(),
            "CLI death left app-link locator"
        );
    }

    fn kill_cli_and_expect_recovered_lease_shutdown(&mut self) {
        let cli = self.cli.as_mut().expect("live recovered shipping CLI");
        cli.kill().expect("SIGKILL only the recovered shipping CLI");
        let status = cli.wait().expect("wait recovered shipping CLI");
        assert_eq!(status.signal(), Some(9));
        let mut line = String::new();
        let read = self
            .control_reader
            .read_line(&mut line)
            .expect("read recovered lease-loss control");
        if read != 0 {
            assert_eq!(
                line.trim_end(),
                "LINK_EOF",
                "lease loss fabricated another event"
            );
        }
        self.assert_group_gone();
        assert!(
            !self.session_dir.exists(),
            "recovered CLI death left app-link locator"
        );
    }

    fn signal_cli_group_and_expect_lease_shutdown(&mut self, signal_name: &str, number: i32) {
        signal_process_group(&format!("-{signal_name}"), self.cli_pid)
            .unwrap_or_else(|error| panic!("group {signal_name} failed: {error}"));
        let status = self
            .cli
            .as_mut()
            .expect("live shipping CLI")
            .wait()
            .expect("wait signaled shipping CLI");
        assert_eq!(status.signal(), Some(number));
        assert!(
            process_exists(self.host_pid),
            "{signal_name} killed the staged host"
        );
        assert_eq!(read_control_line(&mut self.control_reader), "LINK_EOF");
        self.assert_group_gone();
        assert!(
            !self.session_dir.exists(),
            "group {signal_name} left app-link locator"
        );
    }

    fn self_terminate_and_expect_verbatim_error(&mut self) {
        self.control_writer
            .write_all(b"EXIT0\n")
            .expect("request unrequested status-zero exit");
        let output = wait_child_output(self.cli.take().expect("live shipping CLI"), EVENT_DEADLINE);
        assert!(!output.status.success(), "dead app became CLI success");
        let stderr = String::from_utf8(output.stderr).expect("CLI failure stderr UTF-8");
        assert!(stderr.contains("KELD-CORE-033"), "{stderr}");
        assert!(stderr.contains("KELD-RUNTIME-012"), "{stderr}");
        assert!(stderr.contains("KELD-CLI-048"), "{stderr}");
        assert!(!stderr.contains("KELD-CLI-031"), "{stderr}");
        assert!(!stderr.contains("keld doctor"), "{stderr}");
        self.assert_group_gone();
    }

    fn quit_and_expect_success(&mut self) {
        self.control_writer
            .write_all(b"QUIT\n")
            .expect("request T2 relaunch Quit");
        assert_eq!(read_control_line(&mut self.control_reader), "QUIT_REPLY");
        assert_eq!(read_control_line(&mut self.control_reader), "LINK_EOF");
        let output = wait_child_output(self.cli.take().expect("live shipping CLI"), EVENT_DEADLINE);
        assert!(
            output.status.success(),
            "shipping keld dev orderly exit failed: {output:?}"
        );
        let mut forwarded = String::from_utf8(output.stdout).expect("CLI stdout UTF-8");
        forwarded.push_str(&String::from_utf8(output.stderr).expect("CLI stderr UTF-8"));
        assert!(
            forwarded.contains(FORWARDED_LOG),
            "shipping CLI did not forward host/Bun output: {forwarded}"
        );
        self.assert_group_gone();
    }

    fn assert_group_gone(&mut self) {
        await_process_gone(self.host_pid);
        await_process_gone(self.guardian_pid);
        await_process_gone(self.bun_pid);
        await_process_gone(self.descendant_pid);
        assert!(native_windows(self.host_pid, TITLE).is_empty());
        self.group_gone = true;
    }
}

impl Drop for ShippingDevCycle {
    fn drop(&mut self) {
        if let Some(cli) = self.cli.as_mut()
            && cli.try_wait().ok().flatten().is_none()
        {
            let _ = cli.kill();
            let _ = cli.wait();
        }
        if !self.group_gone && self.bun_pid != 0 {
            let _ = signal_process_group("-KILL", self.bun_pid);
        }
    }
}

struct ProductFixture {
    root: tempfile::TempDir,
    project: PathBuf,
    link_source: String,
    harness: &'static str,
    native_census: OnceLock<PathBuf>,
}

impl ProductFixture {
    fn new(name: &str) -> Self {
        let root = tempfile::tempdir().expect("fixture root");
        let project = root.path().join(name);
        fs::create_dir_all(project.join("src")).expect("fixture source directory");
        let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("keld-host crate beneath workspace");
        let link_source = fs::read_to_string(repo.join("packages/@keld/electron/src/link.ts"))
            .expect("reuse canonical KEL-72 TypeScript link owner")
            .replace("../../kipc/src/transport.ts", "./kipc-transport.ts");
        fs::copy(
            repo.join("packages/@keld/kipc/src/transport.ts"),
            project.join("src/kipc-transport.ts"),
        )
        .expect("canonical kipc transport beside the concatenated LifecycleLink");
        Self {
            root,
            project,
            link_source,
            harness: include_str!("fixtures/t1b_harness.ts"),
            native_census: OnceLock::new(),
        }
    }

    fn observe_initial_window(&self) -> NativeWindowObserver {
        let executable = self
            .native_census
            .get_or_init(|| compile_native_window_census(self.root.path()));
        NativeWindowObserver::arm(executable)
    }

    fn stage(&self) -> keld_cli::boot::DevBootStage {
        let mut entry = self.link_source.clone();
        entry.push_str(self.harness);
        fs::write(self.project.join("src/main.ts"), entry).expect("fixture entry");
        if !self.project.join("index.html").exists() {
            fs::write(
                self.project.join("index.html"),
                format!(
                    "<!doctype html>{DARK_BG}<title>{TITLE}</title><p id=marker>{MARKER}</p>\n"
                ),
            )
            .expect("fallback renderer");
        }
        fs::write(
            self.project.join("keld.config.ts"),
            format!(
                "export default {{\n  name: \"{TITLE}\",\n  entry: \"src/main.ts\",\n  renderer: \"index.html\",\n}} as const;\n"
            ),
        )
        .expect("fixture config");
        keld_cli::boot::stage_dev_boot(&self.project, Path::new(env!("CARGO_BIN_EXE_keld-host")))
            .expect("compile owner-private no-flag stage")
    }

    fn launch_cycle(&self, cycle: &str) -> LiveCycle {
        self.launch_cycle_inner(cycle)
    }

    fn launch_leased_cycle(&self, cycle: &str) -> (LiveCycle, ChildStdin) {
        let mut cycle = self.launch_cycle_inner(cycle);
        let lease = cycle.dev_lease_writer.take().expect("leased cycle writer");
        (cycle, lease)
    }

    fn launch_cycle_inner(&self, cycle: &str) -> LiveCycle {
        let beacon = Beacon::bind(MARKER);
        fs::write(
            self.project.join("index.html"),
            format!(
                "<!doctype html>{DARK_BG}<title>{TITLE}</title><p id=marker>{MARKER}</p><img src=\"http://127.0.0.1:{}/{MARKER}\">\n",
                beacon.port()
            ),
        )
        .expect("renderer with exact beacon");
        let stage = self.stage();
        let control_path = self.root.path().join(format!("{cycle}.sock"));
        let listener = UnixListener::bind(&control_path).expect("bind fixture control");
        listener
            .set_nonblocking(true)
            .expect("nonblocking fixture control");
        let substitution_cwd = self.root.path().join("substitution-cwd");
        fs::create_dir_all(&substitution_cwd).expect("substitution cwd");
        fs::write(
            substitution_cwd.join("keld.boot.json"),
            b"environment and cwd must not select this descriptor",
        )
        .expect("substitution descriptor");
        let mut command = Command::new(stage.host());
        command
            .current_dir(&substitution_cwd)
            .env("KELD_T1B_CONTROL", &control_path)
            .env("KELD_BOOT_PATH", substitution_cwd.join("keld.boot.json"))
            .env("KELD_DEV_LEASE", "stdin-v1")
            .env("KELD_T2_EXIT_ON_LINK_EOF", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let presentation = self.observe_initial_window();
        let mut child = command.spawn().expect("launch staged no-flag host");
        let dev_lease_writer = child.stdin.take();
        let host_pid = child.id();
        let control = accept_before(&listener, Instant::now() + EVENT_DEADLINE);
        control
            .set_read_timeout(Some(EVENT_DEADLINE))
            .expect("control read deadline");
        let control_reader = BufReader::new(control.try_clone().expect("control reader clone"));
        let mut cycle = LiveCycle {
            host: Some(child),
            dev_lease_writer,
            host_pid,
            guardian_pid: 0,
            bun_pid: 0,
            descendant_pid: 0,
            session_dir: PathBuf::new(),
            control_reader,
            control_writer: control,
            beacon: Some(beacon),
            presentation: Some(presentation),
            group_gone: false,
        };
        let hello = cycle.next_line();
        let mut fields = hello.split_whitespace();
        assert_eq!(fields.next(), Some("HELLO"), "{hello}");
        cycle.bun_pid = parse_pid(fields.next(), &hello);
        let app_link = fields
            .next()
            .unwrap_or_else(|| panic!("missing app link: {hello}"));
        let endpoint = PathBuf::from(
            app_link
                .rsplit_once('#')
                .unwrap_or_else(|| panic!("invalid app link: {hello}"))
                .0,
        );
        cycle.session_dir = endpoint.parent().expect("session directory").to_path_buf();
        cycle.guardian_pid = parent_process(cycle.bun_pid);
        let descendant = cycle.next_line();
        let mut descendant_fields = descendant.split_whitespace();
        assert_eq!(descendant_fields.next(), Some("DESCENDANT"), "{descendant}");
        cycle.descendant_pid = parse_pid(descendant_fields.next(), &descendant);
        cycle.expect_line("READY");
        cycle.expect_line("ECHO1");
        cycle.expect_line("ECHO2");
        cycle.beacon.take().expect("beacon owner").assert_exact();
        cycle
    }
}

#[derive(Clone, Copy)]
enum InvalidBoot {
    MissingBoot,
    UnreadableBoot,
    DirectoryBoot,
    SymlinkBoot,
    Malformed,
    Duplicate,
    Unknown,
    Version,
    NonUtf8,
    Oversize,
    EmptyName,
    UnsafePath,
    BadDigest,
    WrongPermissionsFile,
    MissingEntry,
    DirectoryEntry,
    SymlinkEntry,
    UnreadableEntry,
    MissingRenderer,
    DirectoryRenderer,
    SymlinkRenderer,
    UnreadableRenderer,
    MissingPermissions,
    DirectoryPermissions,
    SymlinkPermissions,
    UnreadablePermissions,
    WrongRootMode,
}

impl InvalidBoot {
    const ALL: [Self; 27] = [
        Self::MissingBoot,
        Self::UnreadableBoot,
        Self::DirectoryBoot,
        Self::SymlinkBoot,
        Self::Malformed,
        Self::Duplicate,
        Self::Unknown,
        Self::Version,
        Self::NonUtf8,
        Self::Oversize,
        Self::EmptyName,
        Self::UnsafePath,
        Self::BadDigest,
        Self::WrongPermissionsFile,
        Self::MissingEntry,
        Self::DirectoryEntry,
        Self::SymlinkEntry,
        Self::UnreadableEntry,
        Self::MissingRenderer,
        Self::DirectoryRenderer,
        Self::SymlinkRenderer,
        Self::UnreadableRenderer,
        Self::MissingPermissions,
        Self::DirectoryPermissions,
        Self::SymlinkPermissions,
        Self::UnreadablePermissions,
        Self::WrongRootMode,
    ];

    const fn name(self) -> &'static str {
        match self {
            Self::MissingBoot => "missing-boot",
            Self::UnreadableBoot => "unreadable-boot",
            Self::DirectoryBoot => "directory-boot",
            Self::SymlinkBoot => "symlink-boot",
            Self::Malformed => "malformed-json",
            Self::Duplicate => "duplicate-field",
            Self::Unknown => "unknown-field",
            Self::Version => "unknown-version",
            Self::NonUtf8 => "non-utf8",
            Self::Oversize => "oversize",
            Self::EmptyName => "empty-name",
            Self::UnsafePath => "unsafe-path",
            Self::BadDigest => "bad-digest",
            Self::WrongPermissionsFile => "wrong-permissions-file",
            Self::MissingEntry => "missing-entry",
            Self::DirectoryEntry => "directory-entry",
            Self::SymlinkEntry => "symlink-entry",
            Self::UnreadableEntry => "unreadable-entry",
            Self::MissingRenderer => "missing-renderer",
            Self::DirectoryRenderer => "directory-renderer",
            Self::SymlinkRenderer => "symlink-renderer",
            Self::UnreadableRenderer => "unreadable-renderer",
            Self::MissingPermissions => "missing-permissions",
            Self::DirectoryPermissions => "directory-permissions",
            Self::SymlinkPermissions => "symlink-permissions",
            Self::UnreadablePermissions => "unreadable-permissions",
            Self::WrongRootMode => "wrong-root-mode",
        }
    }

    const fn expected_code(self) -> &'static str {
        match self {
            Self::Malformed
            | Self::Duplicate
            | Self::Unknown
            | Self::Version
            | Self::NonUtf8
            | Self::Oversize
            | Self::EmptyName
            | Self::BadDigest
            | Self::WrongPermissionsFile => "KELD-CORE-035",
            Self::MissingBoot
            | Self::UnreadableBoot
            | Self::DirectoryBoot
            | Self::SymlinkBoot
            | Self::UnsafePath
            | Self::MissingEntry
            | Self::DirectoryEntry
            | Self::SymlinkEntry
            | Self::UnreadableEntry
            | Self::MissingRenderer
            | Self::DirectoryRenderer
            | Self::SymlinkRenderer
            | Self::UnreadableRenderer
            | Self::MissingPermissions
            | Self::DirectoryPermissions
            | Self::SymlinkPermissions
            | Self::UnreadablePermissions
            | Self::WrongRootMode => "KELD-CORE-036",
        }
    }

    fn apply(self, root: &Path, fixture_root: &Path) {
        let boot = root.join("keld.boot.json");
        let entry = root.join("src/main.ts");
        let renderer = root.join("index.html");
        let permissions = root.join("keld.permissions.jsonc");
        match self {
            Self::MissingBoot => fs::remove_file(boot).expect("remove boot"),
            Self::UnreadableBoot => unreadable(&boot),
            Self::DirectoryBoot => replace_with_directory(&boot),
            Self::SymlinkBoot => replace_with_symlink(&boot, fixture_root, "outside-boot"),
            Self::Malformed => replace_boot(&boot, b"{not schema v1}"),
            Self::Duplicate => replace_boot(
                &boot,
                br#"{"schema":1,"schema":1,"name":"x","entry":"src/main.ts","renderer":"index.html","permissions":{"file":"keld.permissions.jsonc","content_sha256":"sha256:ca3d163bab055381827226140568f3bef7eaac187cebd76878e0b63e9e442356"}}"#,
            ),
            Self::Unknown => mutate_boot(&boot, |document| document["unknown"] = 1.into()),
            Self::Version => mutate_boot(&boot, |document| document["schema"] = 2.into()),
            Self::NonUtf8 => replace_boot(&boot, &[0xff]),
            Self::Oversize => replace_boot(&boot, &vec![b' '; 64 * 1024 + 1]),
            Self::EmptyName => mutate_boot(&boot, |document| document["name"] = "".into()),
            Self::UnsafePath => {
                mutate_boot(&boot, |document| document["entry"] = "../escape.ts".into());
            }
            Self::BadDigest => mutate_boot(&boot, |document| {
                document["permissions"]["content_sha256"] = "SHA256:BAD".into();
            }),
            Self::WrongPermissionsFile => mutate_boot(&boot, |document| {
                document["permissions"]["file"] = "other.permissions.jsonc".into();
            }),
            Self::MissingEntry => fs::remove_file(entry).expect("remove entry"),
            Self::DirectoryEntry => replace_with_directory(&entry),
            Self::SymlinkEntry => replace_with_symlink(&entry, fixture_root, "outside-entry"),
            Self::UnreadableEntry => unreadable(&entry),
            Self::MissingRenderer => fs::remove_file(renderer).expect("remove renderer"),
            Self::DirectoryRenderer => replace_with_directory(&renderer),
            Self::SymlinkRenderer => {
                replace_with_symlink(&renderer, fixture_root, "outside-renderer");
            }
            Self::UnreadableRenderer => unreadable(&renderer),
            Self::MissingPermissions => {
                fs::remove_file(permissions).expect("remove permissions");
            }
            Self::DirectoryPermissions => replace_with_directory(&permissions),
            Self::SymlinkPermissions => {
                replace_with_symlink(&permissions, fixture_root, "outside-permissions");
            }
            Self::UnreadablePermissions => unreadable(&permissions),
            Self::WrongRootMode => {
                fs::set_permissions(root, fs::Permissions::from_mode(0o755))
                    .expect("set invalid root mode");
            }
        }
    }
}

#[derive(Clone, Copy)]
enum InvalidPolicy {
    Malformed,
    NonUtf8,
    DigestMismatch,
    Oversized,
    DuplicateKeys,
}

impl InvalidPolicy {
    const ALL: [Self; 5] = [
        Self::Malformed,
        Self::NonUtf8,
        Self::DigestMismatch,
        Self::Oversized,
        Self::DuplicateKeys,
    ];

    const fn name(self) -> &'static str {
        match self {
            Self::Malformed => "malformed",
            Self::NonUtf8 => "non-utf8",
            Self::DigestMismatch => "digest-mismatch",
            Self::Oversized => "oversized",
            Self::DuplicateKeys => "duplicate-keys",
        }
    }

    const fn expected_code(self) -> &'static str {
        match self {
            Self::Malformed | Self::NonUtf8 | Self::DuplicateKeys => "KELD-GUARD005",
            Self::DigestMismatch => "KELD-GUARD016",
            Self::Oversized => "KELD-GUARD017",
        }
    }

    fn apply(self, root: &Path) {
        let policy = root.join("keld.permissions.jsonc");
        let boot = root.join("keld.boot.json");
        fs::set_permissions(&policy, fs::Permissions::from_mode(0o600))
            .expect("make policy writable");
        match self {
            Self::Malformed => {
                fs::write(&policy, b"{nope}\n").expect("write malformed policy");
                set_policy_digest(
                    &boot,
                    "ed4d18e4d7f58b800fafc0e89f02e9b76eca431e8a8314df677d02cee467920e",
                );
            }
            Self::NonUtf8 => {
                fs::write(&policy, [0xff]).expect("write non-UTF-8 policy");
                set_policy_digest(
                    &boot,
                    "a8100ae6aa1940d0b663bb31cd466142ebbdbd5187131b92d93818987832eb89",
                );
            }
            Self::DigestMismatch => {
                fs::write(&policy, b"{not the described bytes}\n")
                    .expect("write digest-mismatched policy");
            }
            Self::Oversized => {
                fs::write(&policy, vec![b' '; 64 * 1024 + 1]).expect("write oversized policy");
            }
            Self::DuplicateKeys => {
                fs::write(
                    &policy,
                    br#"{"app":{"fs":{"read":[],"read":["/outside/**"]}}}"#,
                )
                .expect("write duplicate-key policy");
                set_policy_digest(
                    &boot,
                    "06d74274d5d6a0351deac64c75ad477105c8316f61a0e6e62eb59e3e0f73e0d1",
                );
            }
        }
    }
}

fn set_policy_digest(boot: &Path, hex: &str) {
    mutate_boot(boot, |document| {
        document["permissions"]["content_sha256"] = format!("sha256:{hex}").into();
    });
}

fn replace_boot(path: &Path, bytes: &[u8]) {
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).expect("make boot writable");
    fs::write(path, bytes).expect("replace boot bytes");
}

fn mutate_boot(path: &Path, mutate: impl FnOnce(&mut serde_json::Value)) {
    let mut document: serde_json::Value =
        serde_json::from_slice(&fs::read(path).expect("read boot")).expect("parse staged boot");
    mutate(&mut document);
    replace_boot(
        path,
        &serde_json::to_vec(&document).expect("serialize mutated boot"),
    );
}

fn unreadable(path: &Path) {
    fs::set_permissions(path, fs::Permissions::from_mode(0o000)).expect("make target unreadable");
}

fn replace_with_directory(path: &Path) {
    fs::remove_file(path).expect("remove file before directory substitution");
    fs::create_dir(path).expect("create directory substitution");
}

fn replace_with_symlink(path: &Path, fixture_root: &Path, name: &str) {
    let outside = fixture_root.join(name);
    fs::write(&outside, b"outside substitution").expect("outside substitution target");
    fs::remove_file(path).expect("remove file before symlink substitution");
    symlink(outside, path).expect("create symlink substitution");
}

struct NativeAbsenceWatcher {
    executable: PathBuf,
}

struct PolicyReadFault {
    library: PathBuf,
    marker: PathBuf,
}

impl PolicyReadFault {
    fn compile(root: &Path) -> Self {
        const SOURCE: &str = r#"
#include <errno.h>
#include <fcntl.h>
#include <limits.h>
#include <stdlib.h>
#include <string.h>
#include <sys/syscall.h>
#include <unistd.h>

#define DYLD_INTERPOSE(replacement, replacee) \
  __attribute__((used)) static struct { const void *replacement_ptr; const void *replacee_ptr; } \
  interpose_##replacee __attribute__((section("__DATA,__interpose"))) = { \
    (const void *)(unsigned long)&replacement, (const void *)(unsigned long)&replacee \
  };

static ssize_t fault_read(int fd, void *buffer, size_t count) {
  char path[PATH_MAX];
  const char *suffix = "/keld.permissions.jsonc";
  if (fcntl(fd, F_GETPATH, path) == 0) {
    size_t path_len = strlen(path);
    size_t suffix_len = strlen(suffix);
    if (path_len >= suffix_len && strcmp(path + path_len - suffix_len, suffix) == 0) {
      const char *marker = getenv("KELD_T2_READ_FAULT_MARKER");
      if (marker != NULL) {
        int marker_fd = open(marker, O_WRONLY | O_CREAT | O_TRUNC, 0600);
        if (marker_fd >= 0) {
          (void)write(marker_fd, "faulted\n", 8);
          (void)close(marker_fd);
        }
      }
      errno = EIO;
      return -1;
    }
  }
  return (ssize_t)syscall(SYS_read, fd, buffer, count);
}

DYLD_INTERPOSE(fault_read, read)
"#;
        let source = root.join("kel102-policy-read-fault.c");
        let library = root.join("kel102-policy-read-fault.dylib");
        let marker = root.join("kel102-policy-read-fault.marker");
        fs::write(&source, SOURCE).expect("write policy read-fault interposer");
        let output = Command::new("/usr/bin/clang")
            .args(["-dynamiclib", "-O2", "-o"])
            .arg(&library)
            .arg(&source)
            .output()
            .expect("compile policy read-fault interposer");
        assert!(output.status.success(), "compile interposer: {output:?}");
        Self { library, marker }
    }
}

impl NativeAbsenceWatcher {
    fn compile(root: &Path) -> Self {
        const SOURCE: &str = r#"
import CoreGraphics
import Darwin
import Foundation

let target = Int32(CommandLine.arguments[1])!
let prefix = "kb-" + String(target, radix: 16) + "-"
let roots = [FileManager.default.temporaryDirectory.path, "/tmp", "/var/tmp"]
var windows = Set<UInt32>()
var children = Set<Int>()
var sessions = Set<String>()

func sample() {
  let rows = CGWindowListCopyWindowInfo([.excludeDesktopElements], kCGNullWindowID) as! [[String: Any]]
  for row in rows {
    let owner = (row[kCGWindowOwnerPID as String] as? NSNumber)?.int32Value
    if owner == target, let number = row[kCGWindowNumber as String] as? NSNumber {
      windows.insert(number.uint32Value)
    }
  }
  for root in roots {
    for name in (try? FileManager.default.contentsOfDirectory(atPath: root)) ?? [] where name.hasPrefix(prefix) {
      sessions.insert(root + "/" + name)
    }
  }
  let task = Process()
  task.executableURL = URL(fileURLWithPath: "/bin/ps")
  task.arguments = ["-axo", "ppid=,pid="]
  let pipe = Pipe()
  task.standardOutput = pipe
  task.standardError = FileHandle.nullDevice
  try! task.run()
  task.waitUntilExit()
  let text = String(data: pipe.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8)!
  for line in text.split(separator: "\n") {
    let fields = line.split(whereSeparator: { $0 == " " || $0 == "\t" })
    if fields.count == 2, Int(fields[0]) == Int(target), let child = Int(fields[1]) {
      children.insert(child)
    }
  }
}

sample()
print("READY")
fflush(stdout)
_ = kill(target, SIGCONT)
while kill(target, 0) == 0 {
  sample()
}
sample()
for value in windows.sorted() { print("WINDOW \(value)") }
for value in children.sorted() { print("CHILD \(value)") }
for value in sessions.sorted() { print("SESSION \(value)") }
print("DONE")
"#;
        let source = root.join("kel96-native-absence.swift");
        let executable = root.join("kel96-native-absence");
        fs::write(&source, SOURCE).expect("write native absence watcher");
        let output = Command::new("/usr/bin/xcrun")
            .args([
                "swiftc",
                "-O",
                source.to_str().expect("watcher source UTF-8"),
                "-o",
            ])
            .arg(&executable)
            .output()
            .expect("compile native absence watcher");
        assert!(output.status.success(), "compile watcher: {output:?}");
        Self { executable }
    }

    fn spawn(&self, pid: u32) -> Child {
        Command::new(&self.executable)
            .arg(pid.to_string())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("start native absence watcher")
    }
}

fn assert_invalid_stage_is_resource_free(
    stage: &keld_cli::boot::DevBootStage,
    watcher: &NativeAbsenceWatcher,
    control_path: &Path,
    case: &str,
    expected_code: &str,
    read_fault: Option<&PolicyReadFault>,
) {
    let listener = UnixListener::bind(control_path).expect("bind invalid control observer");
    listener
        .set_nonblocking(true)
        .expect("nonblocking invalid control observer");
    let mut command = Command::new("/bin/sh");
    if let Some(fault) = read_fault {
        command
            .args([
                "-c",
                "kill -STOP $$; DYLD_INSERT_LIBRARIES=\"$2\" KELD_T2_READ_FAULT_MARKER=\"$3\" exec \"$1\"",
                "kel102-policy-read-fault",
            ])
            .arg(stage.host())
            .arg(&fault.library)
            .arg(&fault.marker);
    } else {
        command
            .args(["-c", "kill -STOP $$; exec \"$1\"", "kel96-invalid"])
            .arg(stage.host());
    }
    command
        .env("KELD_T1B_CONTROL", control_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let child = command.spawn().expect("start suspended invalid host");
    let host_pid = child.id();
    await_process_state(host_pid, 'T');
    let native = watcher.spawn(host_pid);
    let mut forbidden_control = false;
    let output = wait_child_output_observing(child, EVENT_DEADLINE, || match listener.accept() {
        Ok((mut stream, _)) => {
            forbidden_control = true;
            stream
                .write_all(b"QUIT\n")
                .expect("stop forbidden app through its owned lifecycle path");
        }
        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
        Err(error) => panic!("inspect invalid-stage control: {error}"),
    });
    assert!(!forbidden_control, "{case}: Bun entered before preflight");
    assert!(
        !output.status.success(),
        "{case}: invalid boot became success"
    );
    let native_output = native
        .wait_with_output()
        .expect("wait native absence watcher");
    assert!(
        native_output.status.success(),
        "{case}: watcher failed: {native_output:?}"
    );
    let observations = String::from_utf8(native_output.stdout).expect("watcher output UTF-8");
    assert_eq!(
        observations, "READY\nDONE\n",
        "{case}: transient resource: {observations}"
    );
    let stderr = String::from_utf8(output.stderr).expect("typed stderr UTF-8");
    assert!(stderr.contains(expected_code), "{case}: {stderr}");
    assert!(
        stderr.contains("[startup-resource-attempts listener=0 child=0 window=0]"),
        "{case}: internal pre-resource ledger was not empty: {stderr}"
    );
    let lower = stderr.to_ascii_lowercase();
    assert!(
        ["regenerate", "restore", "write", "set", "launch"]
            .iter()
            .any(|action| lower.contains(action)),
        "{case}: missing fix: {stderr}"
    );
    assert!(
        matches!(listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock),
        "{case}: Bun reached the control observer"
    );
    assert!(
        session_dirs_for(host_pid).is_empty(),
        "{case}: app-link directory remains"
    );
    let _ = fs::remove_file(control_path);
}

fn await_process_state(pid: u32, wanted: char) {
    let deadline = Instant::now() + PROCESS_DEADLINE;
    loop {
        let output = Command::new("/bin/ps")
            .args(["-o", "state=", "-p", &pid.to_string()])
            .output()
            .expect("inspect process state");
        if String::from_utf8(output.stdout)
            .expect("process state UTF-8")
            .trim()
            .starts_with(wanted)
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "process {pid} never reached state {wanted}"
        );
        thread::yield_now();
    }
}

struct LiveCycle {
    host: Option<Child>,
    dev_lease_writer: Option<ChildStdin>,
    host_pid: u32,
    guardian_pid: u32,
    bun_pid: u32,
    descendant_pid: u32,
    session_dir: PathBuf,
    control_reader: BufReader<UnixStream>,
    control_writer: UnixStream,
    beacon: Option<Beacon>,
    presentation: Option<NativeWindowObserver>,
    group_gone: bool,
}

impl LiveCycle {
    fn assert_live_product(&mut self) {
        assert_ne!(self.host_pid, self.guardian_pid);
        assert_ne!(self.guardian_pid, self.bun_pid);
        assert_eq!(parent_process(self.guardian_pid), self.host_pid);
        assert_eq!(parent_process(self.bun_pid), self.guardian_pid);
        assert_eq!(process_group(self.bun_pid), self.bun_pid);
        assert_eq!(process_group(self.descendant_pid), self.bun_pid);
        let windows = self
            .presentation
            .take()
            .expect("prearmed initial-presentation observer")
            .expect_initial(self.host_pid, "initial-presentation");
        assert_eq!(
            windows.len(),
            1,
            "exact host-owned native window: {windows:?}"
        );
        assert!(
            !unix_sockets_not_inherited_from_harness(self.host_pid).is_empty(),
            "host owns no authenticated Unix app-link descriptor"
        );
        assert!(
            !unix_sockets_not_inherited_from_harness(self.bun_pid).is_empty(),
            "Bun owns no authenticated Unix app-link descriptor"
        );
        assert!(
            !self.session_dir.exists(),
            "authenticated one-use app-link locator must already be revoked"
        );
        eprintln!(
            "KEL96_T1B_EVIDENCE host={} window={} guardian={} bun={} descendant={} pgid={} link_dir={} marker={}",
            self.host_pid,
            windows[0],
            self.guardian_pid,
            self.bun_pid,
            self.descendant_pid,
            process_group(self.bun_pid),
            self.session_dir.display(),
            MARKER
        );
    }

    fn next_line(&mut self) -> String {
        let mut line = String::new();
        let read = self
            .control_reader
            .read_line(&mut line)
            .expect("read fixture observation");
        assert_ne!(read, 0, "fixture control reached EOF before expected event");
        let line = line.trim_end().to_owned();
        assert!(!line.starts_with("ERROR "), "fixture error: {line}");
        line
    }

    fn expect_line(&mut self, expected: &str) {
        assert_eq!(self.next_line(), expected);
    }

    fn wait_host(&mut self) -> Output {
        self.wait_host_observing(|| {})
    }

    fn wait_host_observing(&mut self, mut observe: impl FnMut()) -> Output {
        let mut child = self.host.take().expect("live host");
        let deadline = Instant::now() + EVENT_DEADLINE;
        loop {
            if child.try_wait().expect("inspect no-flag host").is_some() {
                // Waiting must not inject CLI death before the observed host exit.
                // Explicit lease-loss scenarios release their separate writer first.
                drop(self.dev_lease_writer.take());
                return child
                    .wait_with_output()
                    .expect("collect no-flag host output");
            }
            observe();
            assert!(
                Instant::now() < deadline,
                "no-flag host did not exit after Quit"
            );
            thread::yield_now();
        }
    }
}

impl Drop for LiveCycle {
    fn drop(&mut self) {
        if let Some(host) = self.host.as_mut() {
            let _ = host.kill();
            let _ = host.wait();
        }
        if !self.group_gone && self.bun_pid != 0 {
            let _ = signal_process_group("-KILL", self.bun_pid);
        }
    }
}

struct NavigationBlocker {
    port: u16,
    connected: Receiver<()>,
    release: mpsc::Sender<()>,
    handle: JoinHandle<()>,
}

impl NavigationBlocker {
    fn bind() -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind navigation blocker");
        let port = listener.local_addr().expect("blocker address").port();
        let (connected_tx, connected) = mpsc::channel();
        let (release, release_rx) = mpsc::channel();
        let handle = thread::spawn(move || {
            let (_stream, _) = listener.accept().expect("accept blocked navigation");
            connected_tx.send(()).expect("report blocked navigation");
            release_rx.recv().expect("release blocked navigation");
        });
        Self {
            port,
            connected,
            release,
            handle,
        }
    }
}

fn kill_pid(pid: u32) {
    let status = Command::new("/bin/kill")
        .args(["-KILL", &pid.to_string()])
        .status()
        .expect("kill one process");
    assert!(status.success(), "kill {pid}: {status:?}");
}

fn validate_process_group(group: u32) -> std::io::Result<i32> {
    let test_group = process_group(std::process::id());
    if group <= 1 || group == test_group {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "refused to signal an invalid or test-runner process group",
        ));
    }
    i32::try_from(group)
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "group exceeds pid_t"))
}

fn signal_process_group(signal: &str, group: u32) -> std::io::Result<()> {
    let group = validate_process_group(group)?;
    let signal = match signal {
        "-HUP" => 1,
        "-INT" => 2,
        "-TERM" => 15,
        "-KILL" => 9,
        _ => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "unknown signal",
            ));
        }
    };
    // SAFETY: macOS kill(2) interprets a negative pid below -1 as exactly that
    // process group. `group` is neither 0, 1 nor the test runner's group, was
    // observed from this fixture's verified process tree, and the caller
    // restricts signals to the four named constants.
    let result = unsafe { kill_process(-group, signal) };
    if result == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

struct Beacon {
    port: u16,
    request: Receiver<Vec<u8>>,
    handle: Option<JoinHandle<()>>,
}

impl Beacon {
    fn bind(marker: &'static str) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind renderer beacon");
        let port = listener.local_addr().expect("beacon address").port();
        let (request_tx, request) = mpsc::channel();
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept renderer beacon");
            stream
                .set_read_timeout(Some(EVENT_DEADLINE))
                .expect("beacon read deadline");
            let mut bytes = Vec::new();
            let mut chunk = [0_u8; 1024];
            while !bytes.windows(4).any(|window| window == b"\r\n\r\n") {
                let read = stream.read(&mut chunk).expect("read renderer beacon");
                if read == 0 {
                    request_tx
                        .send(bytes)
                        .expect("report closed renderer beacon");
                    return;
                }
                bytes.extend_from_slice(&chunk[..read]);
            }
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                .expect("respond renderer beacon");
            request_tx.send(bytes).expect("report renderer request");
            assert!(!marker.is_empty());
        });
        Self {
            port,
            request,
            handle: Some(handle),
        }
    }

    const fn port(&self) -> u16 {
        self.port
    }

    fn assert_exact(mut self) {
        let request = self
            .request
            .recv_timeout(EVENT_DEADLINE)
            .expect("WKWebView did not render the exact fixture beacon");
        let request = String::from_utf8_lossy(&request);
        assert!(request.starts_with(&format!("GET /{MARKER} ")), "{request}");
        self.handle
            .take()
            .expect("beacon thread")
            .join()
            .expect("beacon thread joins");
    }
}

impl Drop for Beacon {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            let _ = TcpStream::connect(("127.0.0.1", self.port));
            let _ = handle.join();
        }
    }
}

fn accept_before(listener: &UnixListener, deadline: Instant) -> UnixStream {
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                stream
                    .set_nonblocking(false)
                    .expect("normalize accepted fixture control");
                return stream;
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(
                    Instant::now() < deadline,
                    "Bun did not connect the fixture control"
                );
                thread::yield_now();
            }
            Err(error) => panic!("accept fixture control: {error}"),
        }
    }
}

fn read_control_line(reader: &mut BufReader<UnixStream>) -> String {
    let mut line = String::new();
    let read = reader.read_line(&mut line).expect("read control line");
    assert_ne!(read, 0, "control EOF before observation");
    line.trim_end().to_owned()
}

fn wait_child_output(child: Child, timeout: Duration) -> Output {
    wait_child_output_observing(child, timeout, || {})
}

fn wait_child_output_observing(
    mut child: Child,
    timeout: Duration,
    mut observe: impl FnMut(),
) -> Output {
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let stdout_reader = thread::spawn(move || read_child_pipe(stdout));
    let stderr_reader = thread::spawn(move || read_child_pipe(stderr));
    let deadline = Instant::now() + timeout;
    let (status, timed_out) = loop {
        if let Some(status) = child.try_wait().expect("inspect child exit") {
            break (status, false);
        }
        observe();
        if Instant::now() >= deadline {
            let _ = child.kill();
            break (child.wait().expect("wait timed-out child"), true);
        }
        thread::yield_now();
    };
    let output = Output {
        status,
        stdout: stdout_reader.join().expect("stdout reader joins"),
        stderr: stderr_reader.join().expect("stderr reader joins"),
    };
    assert!(
        !timed_out,
        "child exceeded exit deadline (status={})",
        output.status
    );
    output
}

fn read_child_pipe(pipe: Option<impl Read>) -> Vec<u8> {
    let Some(mut pipe) = pipe else {
        return Vec::new();
    };
    let mut bytes = Vec::new();
    pipe.read_to_end(&mut bytes).expect("drain child pipe");
    bytes
}

fn parse_pid(field: Option<&str>, line: &str) -> u32 {
    field
        .unwrap_or_else(|| panic!("missing pid: {line}"))
        .parse()
        .unwrap_or_else(|error| panic!("invalid pid ({error}): {line}"))
}

fn parent_process(pid: u32) -> u32 {
    process_number(pid, "ppid")
}

fn process_group(pid: u32) -> u32 {
    process_number(pid, "pgid")
}

fn process_number(pid: u32, field: &str) -> u32 {
    let output = Command::new("/bin/ps")
        .args(["-o", &format!("{field}="), "-p", &pid.to_string()])
        .output()
        .expect("inspect process relation");
    assert!(output.status.success(), "ps {field} for {pid}: {output:?}");
    String::from_utf8(output.stdout)
        .expect("ps output UTF-8")
        .trim()
        .parse()
        .expect("ps relation is numeric")
}

fn process_exists(pid: u32) -> bool {
    Command::new("/bin/kill")
        .args(["-0", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn await_process_gone(pid: u32) {
    let deadline = Instant::now() + PROCESS_DEADLINE;
    while process_exists(pid) {
        assert!(Instant::now() < deadline, "process {pid} survived cleanup");
        thread::yield_now();
    }
}

/// One Unix-domain descriptor: its number, the kernel socket behind it, and the
/// name `lsof` prints for it.
struct UnixDescriptor {
    descriptor: String,
    /// `lsof`'s `d` field: the address of the socket object itself. Two live
    /// sockets never share one, and `exec` preserves it, so this is what
    /// identifies a descriptor across processes.
    socket: String,
    /// `lsof`'s `n` field: the bound `sun_path` for a listener and for each peer
    /// it accepted, `->0x<address>` naming the *peer* socket for a connected or
    /// paired endpoint, or `->(none)` when there is neither. Human-readable, and
    /// deliberately not used to decide identity.
    identity: String,
}

/// The `ps` state letters for `pid`, or why they could not be read.
fn process_state(pid: u32) -> String {
    let output = Command::new("/bin/ps")
        .args(["-o", "state=", "-p", &pid.to_string()])
        .output()
        .expect("inspect process state");
    let state = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if state.is_empty() {
        format!("unreadable (ps exit {:?})", output.status.code())
    } else {
        state
    }
}

/// Whether `lsof` can still see any open descriptor at all on `pid`.
///
/// Not restricted to Unix sockets, and not parsed: `cwd` and the mapped executable
/// count, so any process whose descriptor table exists answers yes. That makes this
/// the corroboration an empty Unix census needs — see [`unix_descriptors`].
fn process_has_open_descriptors(pid: u32) -> bool {
    let output = Command::new("/usr/sbin/lsof")
        .args(["-n", "-P", "-p", &pid.to_string(), "-Ff"])
        .output()
        .expect("enumerate open descriptors");
    output.status.success()
        && String::from_utf8_lossy(&output.stdout)
            .lines()
            .any(|line| line.starts_with('f'))
}

/// Every Unix-domain descriptor open on `pid`.
///
/// `lsof` exits 1 when it cannot report on the pid at all. Measured on macOS `lsof`
/// 4.91, that covers a pid that never existed, a zombie, and a pid owned by another
/// user — the last with empty stderr, so an uninspectable process cannot masquerade
/// as a clean one.
///
/// Exiting 0 with no rows does *not* mean the same thing twice over, and that is the
/// trap this function has to close. A live process owning no Unix socket reports it
/// that way, and so does a process in the window after `exit` but before it becomes
/// a zombie, when its descriptor table is already gone but its proc entry is not.
/// Zero is the *passing* value for the CLI, so the two must not be conflated:
/// measured, an unguarded census returned an empty result in 32 of 300 racing trials
/// for a target that provably owned a listener it had opened itself — a silent false
/// pass of exactly the assertion this oracle exists to make.
///
/// So an empty census has to prove the process still has a table to be empty of.
/// [`process_has_open_descriptors`] is that proof, and it is not circular: it asks a
/// wider question than the census does, and `cwd` and the mapped executable answer
/// it for any live process. Measured across 83 racing trials that produced an empty
/// Unix census, it rejected all 83, and it stayed silent for live processes both with
/// and without Unix sockets. The race cannot be reproduced deterministically, so
/// [`unix_descriptor_census_requires_a_live_descriptor_table`] pins the corroboration
/// itself, and
/// [`empty_unix_census_of_a_process_without_a_descriptor_table_is_rejected`] pins the
/// census applying it. The exiting window still cannot be entered on demand, so a
/// reaped pid with an empty report stands in for it: same absent table, same
/// earlier `lsof` exit-1 assert, not this empty-result path.
fn unix_descriptors(pid: u32) -> Vec<UnixDescriptor> {
    let output = Command::new("/usr/sbin/lsof")
        .args(["-n", "-P", "-a", "-p", &pid.to_string(), "-U", "-Fdfn"])
        .output()
        .expect("enumerate Unix descriptors");
    assert!(
        output.status.success(),
        "lsof cannot report Unix descriptors for {pid} in state {}: {output:?}",
        process_state(pid)
    );
    let rendered = String::from_utf8(output.stdout).expect("lsof output UTF-8");
    unix_descriptors_reported(pid, &rendered)
}

/// The descriptors `lsof` reported for `pid` in `rendered`, or a panic if what it
/// reported cannot be trusted.
///
/// Split from [`unix_descriptors`] at the process boundary so every decision this
/// census makes is reachable from a test that supplies the output itself. Without
/// that seam the empty-result reject below was unpinned: deleting it left every
/// census test green, because the only input that distinguishes teardown from a
/// clean process is a pid whose table is gone, and a census of one cannot be
/// arranged on demand.
fn unix_descriptors_reported(pid: u32, rendered: &str) -> Vec<UnixDescriptor> {
    let mut descriptor = None;
    let mut socket = None;
    let mut descriptors = Vec::new();
    for line in rendered.lines() {
        if let Some(fd) = line.strip_prefix('f') {
            // A record `lsof` starts and never names would otherwise be dropped
            // without a word, and dropping a target's descriptor is the direction
            // that passes. No real `lsof` output has done this; it is not left to
            // chance because the cost of being wrong is silence.
            assert!(
                descriptor.is_none(),
                "lsof left descriptor {descriptor:?} of {pid} unnamed: {rendered:?}"
            );
            descriptor = Some(fd.to_owned());
            socket = None;
        } else if let Some(address) = line.strip_prefix('d') {
            // Two empty addresses would compare equal and excuse each other.
            assert!(
                address.starts_with("0x"),
                "lsof gave {pid} a socket address that is not one: {address:?}"
            );
            socket = Some(address.to_owned());
        } else if let Some(name) = line.strip_prefix('n') {
            descriptors.push(UnixDescriptor {
                descriptor: descriptor
                    .take()
                    .expect("lsof numbers a descriptor it names"),
                socket: socket.take().expect("lsof addresses a descriptor it names"),
                identity: name.to_owned(),
            });
        }
    }
    assert!(
        descriptor.is_none(),
        "lsof left the last descriptor of {pid} unnamed: {rendered:?}"
    );
    if descriptors.is_empty() {
        assert!(
            process_has_open_descriptors(pid),
            "{pid} has no descriptor table in state {}, so an empty Unix census is \
             teardown rather than evidence",
            process_state(pid)
        );
    }
    descriptors
}

/// One printable name per Unix-domain descriptor open on `pid`.
fn unix_socket_identities(pid: u32) -> Vec<String> {
    unix_descriptors(pid)
        .into_iter()
        .map(|descriptor| descriptor.identity)
        .collect()
}

/// Serialises the census fixtures against each other.
///
/// macOS `std` has no atomic `SOCK_CLOEXEC`: it creates a socket and then sets
/// `FD_CLOEXEC` in a second call, so a `posix_spawn` on another thread inside that
/// window captures the descriptor by number. A socket that leaks into another test's
/// child that way is charged to that child as soon as its real owner closes it,
/// because the census can no longer attribute it to the harness. Measured on these
/// tests sharing one process, that is one failure in 300 runs, and the panic names a
/// socket from a different fixture's temporary directory.
///
/// The mandated gate runs every test in its own process, where the interleaving
/// cannot happen at all; this lock buys a shared-process run the same isolation. It
/// is uncontended under the gate. Poisoning is ignored deliberately: a panicking
/// census test has already failed the run, and refusing the lock afterwards would
/// replace that failure with a less informative one.
static CENSUS_ISOLATION: Mutex<()> = Mutex::new(());

fn census_isolation() -> MutexGuard<'static, ()> {
    CENSUS_ISOLATION
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

/// Every Unix-domain socket open on this harness, by kernel address.
fn harness_unix_sockets() -> Vec<String> {
    unix_descriptors(std::process::id())
        .into_iter()
        .map(|descriptor| descriptor.socket)
        .collect()
}

/// The Unix-domain descriptors open on `pid` that it did not inherit from this
/// harness, named for a human.
///
/// A raw `lsof -U` count is not an oracle for app-link ownership. A Unix socket
/// the launching shell left without `FD_CLOEXEC` reaches every process under test
/// through `exec`, so the raw count charges the CLI for descriptors it never
/// opened (KEL-222). Subtracting the harness's own table removes them.
///
/// Keyed on the socket object's own address, never on the printable name, because
/// names are not identities. Measured on macOS `lsof` 4.91: every socket that is
/// neither bound nor connected is named `->(none)`, and a socket keeps reporting
/// its `sun_path` after that path is unlinked, so a replacement bound to the same
/// path reports the same name. Either would let a name-keyed subtraction excuse a
/// descriptor `pid` opened itself — the silent false pass this oracle exists to
/// prevent. An address is unique among live sockets and `exec` preserves it, so a
/// match means the same kernel object.
///
/// One harness descriptor excuses one target descriptor, not every copy of it.
/// Counting is not a refinement here, it is the whole choice: a target holding two
/// descriptors on a socket the harness holds once is the same picture to `lsof`
/// whether the target `dup`ed something it inherited or opened the socket itself
/// and passed a copy up. Both are constructible, and neither happens in this
/// product — nothing here passes descriptors over `SCM_RIGHTS`, and a shell leak
/// reaches harness and child alike so their counts match and nothing is charged.
/// Given an ambiguous picture the count rule takes the reading that fails loudly:
/// excusing every copy silently absolves a socket the target may have opened, and
/// that silence is the defect KEL-222 exists to remove. It costs a loud
/// over-charge in the `dup` reading, which is investigable.
///
/// The harness table is read on both sides of the target census, and only what
/// both readings agree on can excuse anything. macOS reuses freed socket addresses,
/// at a rate that varies far too much between sessions to quote as a figure —
/// repeated measurements of the same 90 stream-pair ends have returned anywhere
/// from 15 to 74 of them — so reuse has to be assumed rather than treated as rare.
/// One reading would then excuse a socket `pid` opened at an address whose previous
/// tenant the harness had just closed.
///
/// The cost is that a harness close between the two readings over-charges a
/// genuinely inherited descriptor. That is not hypothetical, and an earlier version
/// of this comment was wrong to say no thread here closes a Unix socket: when these
/// tests share one process they close each other's, and it cost one failure in 300
/// runs until [`CENSUS_ISOLATION`] serialised them. It is still the loud direction,
/// which is why the reading stays doubled: the alternative excuses a socket the
/// target may have opened and says nothing. Neither behaviour can be pinned without
/// a test whose timing decides the result, so both are argued from measurement.
///
/// What survives is what `pid` did not inherit *from this harness*, which is why
/// the name says that and not "opened itself". The two coincide only for a direct
/// child, whose whole ancestry is the harness: that is the CLI, and the CLI owning
/// none is the assertion this oracle exists for. For a deeper descendant they
/// diverge — Bun is the guardian's child, so an intermediate ancestor can pass
/// down a socket the harness never held, and a grandchild handed a `socketpair`
/// that way is charged both of its ends. That over-charges, which is safe for "the
/// CLI owns none" and unsafe for the positive "this process owns one" assertions,
/// since an injected descriptor satisfies them. Closing it needs those assertions
/// to name the app link instead of counting anything, and is recorded on KEL-222
/// rather than half-done here.
fn unix_sockets_not_inherited_from_harness(pid: u32) -> Vec<String> {
    let before = harness_unix_sockets();
    let observed = unix_descriptors(pid);
    let mut after = harness_unix_sockets();
    // One entry per socket both readings agree the harness held, and no more
    // copies of it than the second reading still shows.
    let mut inheritable = Vec::new();
    for address in before {
        if let Some(index) = after.iter().position(|held| *held == address) {
            after.swap_remove(index);
            inheritable.push(address);
        }
    }
    let mut charged = Vec::new();
    for descriptor in observed {
        match inheritable
            .iter()
            .position(|held| *held == descriptor.socket)
        {
            Some(index) => {
                inheritable.swap_remove(index);
            }
            None => charged.push(descriptor.identity),
        }
    }
    charged
}

fn lsof_stdin(pid: u32) -> String {
    let output = Command::new("/usr/sbin/lsof")
        .args(["-a", "-p", &pid.to_string(), "-d", "0", "-FDifnat"])
        .output()
        .expect("inspect stdin with lsof");
    assert!(output.status.success(), "lsof stdin failed: {output:?}");
    String::from_utf8(output.stdout).expect("lsof stdin UTF-8")
}

fn lsof_all(pid: u32) -> String {
    let output = Command::new("/usr/sbin/lsof")
        .args(["-a", "-p", &pid.to_string(), "-FDifnat"])
        .output()
        .expect("inspect process descriptors with lsof");
    assert!(output.status.success(), "lsof process failed: {output:?}");
    String::from_utf8(output.stdout).expect("lsof process UTF-8")
}

fn assert_lease_descriptor_ownership(cli_pid: u32, host_pid: u32, guardian_pid: u32, bun_pid: u32) {
    let cli_fds = lsof_all(cli_pid);
    let host_lease = pipe_identity(host_pid, "0");
    let lease_writers: Vec<&str> = pipe_descriptors(&cli_fds)
        .into_iter()
        .filter(|fd| {
            let candidate = pipe_identity(cli_pid, fd);
            candidate.0 == host_lease.1 && candidate.1 == host_lease.0
        })
        .collect();
    assert_eq!(
        lease_writers.len(),
        1,
        "CLI must own exactly one writer reciprocal to host fd 0: {cli_fds}"
    );
    let host_stdin = lsof_stdin(host_pid);
    assert!(host_stdin.contains("tPIPE"), "host stdin: {host_stdin}");
    let lease_peer = host_stdin
        .lines()
        .find(|line| line.starts_with("n->"))
        .expect("host lease pipe peer");
    let guardian_fds = lsof_all(guardian_pid);
    let bun_fds = lsof_all(bun_pid);
    for (pid, snapshot) in [(guardian_pid, &guardian_fds), (bun_pid, &bun_fds)] {
        for fd in pipe_descriptors(snapshot) {
            let candidate = pipe_identity(pid, fd);
            assert!(
                candidate != host_lease
                    && (candidate.0, candidate.1) != (host_lease.1, host_lease.0),
                "process {pid} inherited a dev-lease endpoint on fd {fd}: {snapshot}"
            );
        }
    }
    assert!(
        !guardian_fds.lines().any(|line| line == lease_peer),
        "guardian inherited the host lease reader: {guardian_fds}"
    );
    assert!(
        !bun_fds.lines().any(|line| line == lease_peer),
        "Bun inherited the host lease reader: {bun_fds}"
    );
    let guardian_stdin = lsof_stdin(guardian_pid);
    assert!(
        guardian_stdin.contains("tPIPE") && !guardian_stdin.contains(lease_peer),
        "guardian stdin must be its distinct authenticated bootstrap pipe: {guardian_stdin}"
    );
    let bun_stdin = lsof_stdin(bun_pid);
    assert!(
        bun_stdin.contains("tCHR") && bun_stdin.contains("n/dev/null"),
        "Bun stdin is not null: {bun_stdin}"
    );
}

fn pipe_descriptors(snapshot: &str) -> Vec<&str> {
    let mut current = None;
    let mut pipes = Vec::new();
    for line in snapshot.lines() {
        if let Some(fd) = line.strip_prefix('f') {
            current = Some(fd);
        } else if line == "tPIPE"
            && let Some(fd) = current
        {
            pipes.push(fd);
        }
    }
    pipes
}

fn pipe_identity(pid: u32, fd: &str) -> (u64, u64) {
    // macOS `proc_pidfdinfo(PROC_PIDFDPIPEINFO)` exposes the kernel pipe
    // handle/peer-handle pair, so the oracle can match opposite endpoints
    // without inferring identity from descriptor numbers or `lsof` names.
    const SCRIPT: &str = r#"
import Darwin
let pid = Int32(CommandLine.arguments[1])!
let fd = Int32(CommandLine.arguments[2])!
var info = pipe_fdinfo()
let size = proc_pidfdinfo(pid, fd, PROC_PIDFDPIPEINFO, &info, Int32(MemoryLayout<pipe_fdinfo>.size))
guard size == MemoryLayout<pipe_fdinfo>.size else { exit(2) }
print("\(info.pipeinfo.pipe_handle) \(info.pipeinfo.pipe_peerhandle)")
"#;
    let output = Command::new("/usr/bin/xcrun")
        .args(["swift", "-e", SCRIPT, &pid.to_string(), fd])
        .output()
        .expect("inspect macOS pipe identity");
    assert!(output.status.success(), "pipe identity failed: {output:?}");
    let rendered = String::from_utf8(output.stdout).expect("pipe identity UTF-8");
    let mut fields = rendered.split_whitespace();
    let handle = fields
        .next()
        .expect("pipe handle")
        .parse()
        .expect("numeric pipe handle");
    let peer = fields
        .next()
        .expect("pipe peer handle")
        .parse()
        .expect("numeric pipe peer handle");
    assert!(
        fields.next().is_none(),
        "unexpected pipe identity: {rendered}"
    );
    (handle, peer)
}

fn compile_native_window_census(root: &Path) -> PathBuf {
    let source = root.join("native-window-census.swift");
    let executable = root.join("native-window-census");
    fs::write(&source, include_str!("fixtures/native_window_census.swift"))
        .expect("write native-window census");
    let output = Command::new("/usr/bin/xcrun")
        .args(["swiftc", "-O"])
        .arg(&source)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("compile native-window census");
    assert!(output.status.success(), "compile native census: {output:?}");
    executable
}

/// An external CoreGraphics observer is armed before spawn, when the target PID
/// is not yet known. Only the authenticated PID supplied at the original check
/// phase can consume its on-screen history, and those exact IDs must still live.
struct NativeWindowObserver {
    child: Option<Child>,
    events: Receiver<Result<String, String>>,
    reader: Option<JoinHandle<()>>,
}

impl NativeWindowObserver {
    fn arm(executable: &Path) -> Self {
        let mut child = Command::new(executable)
            .args(["observe", TITLE, "1", &EVENT_DEADLINE.as_secs().to_string()])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("start prearmed native-window observer");
        let stdout = child.stdout.take().expect("native-window observer stdout");
        let (sender, events) = mpsc::sync_channel(2);
        let reader = thread::spawn(move || {
            let mut stdout = BufReader::new(stdout);
            // Exactly two bounded records: readiness and one result. A broken
            // helper cannot grow the parent's buffer or block Drop on send.
            for _ in 0..2 {
                let mut line = String::new();
                let result = match stdout.by_ref().take(257).read_line(&mut line) {
                    Ok(0) => Err(String::from("native-window observer EOF")),
                    Ok(bytes) if bytes > 256 || !line.ends_with('\n') => {
                        Err(String::from("invalid native-window observer record"))
                    }
                    Ok(_) => Ok(line.trim_end().to_owned()),
                    Err(error) => Err(error.to_string()),
                };
                let failed = result.is_err();
                if sender.send(result).is_err() || failed {
                    break;
                }
            }
        });
        let observer = Self {
            child: Some(child),
            events,
            reader: Some(reader),
        };
        assert_eq!(
            observer.events.recv_timeout(EVENT_DEADLINE),
            Ok(Ok(String::from("READY"))),
            "native-window observer must be armed before host spawn"
        );
        observer
    }

    fn expect_initial(&mut self, pid: u32, observation: &str) -> Vec<u32> {
        // Precollection has its own lifetime. The existing check-phase deadline
        // starts here and includes command delivery, result reception and exit.
        let deadline = Instant::now() + EVENT_DEADLINE;
        writeln!(
            self.child
                .as_mut()
                .expect("live native-window observer")
                .stdin
                .as_mut()
                .expect("native-window observer command pipe"),
            "{pid}"
        )
        .expect("bind native-window history to authenticated PID");
        let event = self
            .events
            .recv_timeout(deadline.saturating_duration_since(Instant::now()));
        assert!(
            event.is_ok() && Instant::now() < deadline,
            "CoreGraphics presentation ({observation}): {event:?}; target-PID rows: {}",
            native_window_rows(pid)
        );
        let output = wait_child_output(
            self.child
                .take()
                .expect("native-window observer exit owner"),
            deadline.saturating_duration_since(Instant::now()),
        );
        assert!(
            output.status.success() && Instant::now() < deadline,
            "CoreGraphics presentation ({observation}): {event:?}; {output:?}; target-PID rows: {}",
            native_window_rows(pid)
        );
        let line = event
            .expect("received native-window event")
            .expect("native-window result");
        let windows: Vec<u32> = line
            .strip_prefix("WINDOWS ")
            .expect("native-window identity record")
            .split(',')
            .map(|id| id.parse().expect("numeric native-window identity"))
            .collect();
        assert_eq!(windows.len(), 1, "initial native-window count");
        windows
    }
}

impl Drop for NativeWindowObserver {
    fn drop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

#[test]
fn native_window_observer_history_retains_only_presented_live_identity() {
    let root = tempfile::tempdir().expect("native-window history fixture");
    let executable = compile_native_window_census(root.path());
    let output = Command::new(executable)
        .arg("history-regressions")
        .output()
        .expect("run fixed native-window history snapshots");
    assert!(output.status.success(), "history regressions: {output:?}");
    assert!(String::from_utf8_lossy(&output.stdout).starts_with("PASS native-window history:"));
}

#[test]
fn native_window_observer_reaps_on_assertion_unwind_and_parent_eof() {
    let root = tempfile::tempdir().expect("native-window cleanup fixture");
    let executable = compile_native_window_census(root.path());
    let observer = NativeWindowObserver::arm(&executable);
    let pid = observer.child.as_ref().expect("observer child").id();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        let _observer = observer;
        panic!("injected post-arm assertion failure");
    }));
    assert!(result.is_err(), "negative control must unwind");
    await_process_gone(pid);

    let mut observer = NativeWindowObserver::arm(&executable);
    let child = observer.child.as_mut().expect("observer child");
    let pid = child.id();
    drop(child.stdin.take());
    let output = wait_child_output(observer.child.take().expect("EOF observer"), EVENT_DEADLINE);
    assert_eq!(output.status.code(), Some(2), "parent EOF: {output:?}");
    assert_eq!(
        observer.events.recv_timeout(EVENT_DEADLINE),
        Ok(Err(String::from("native-window observer EOF")))
    );
    drop(observer);
    await_process_gone(pid);
}

fn native_windows(pid: u32, title: &str) -> Vec<u32> {
    query_native_windows(
        pid,
        title,
        NativeWindowScope::OnScreen,
        NativeWindowExpectation::Snapshot,
        "snapshot",
    )
}

/// Captures every CoreGraphics row owned by the target PID only when an existing
/// assertion fails. This is diagnostic evidence: initial launch retains its
/// title/layer/on-screen oracle, while recovery compares the recorded identity
/// through the all-window census. Both retain the same observation deadline.
fn native_window_rows(pid: u32) -> String {
    const SCRIPT: &str = r#"
import CoreGraphics
import Foundation
let wantedPID = Int(CommandLine.arguments[1])!
let onScreen = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as? [[String: Any]] ?? []
let onScreenIDs = Set(onScreen.compactMap { ($0[kCGWindowNumber as String] as? NSNumber)?.uint32Value })
let rows = CGWindowListCopyWindowInfo([.excludeDesktopElements], kCGNullWindowID) as? [[String: Any]] ?? []
let targetRows = rows.compactMap { row -> [String: Any]? in
  guard (row[kCGWindowOwnerPID as String] as? NSNumber)?.intValue == wantedPID else { return nil }
  let number = (row[kCGWindowNumber as String] as? NSNumber)?.uint32Value ?? 0
  let title = row[kCGWindowName as String] as? String
  return [
    "id": number,
    "layer": (row[kCGWindowLayer as String] as? NSNumber)?.intValue ?? -1,
    "on_screen": onScreenIDs.contains(number),
    "title": title ?? "<unavailable>",
    "title_available": title != nil,
  ]
}
let payload: [String: Any] = [
  "capture_preflight": CGPreflightScreenCaptureAccess(),
  "pid": wantedPID,
  "rows": targetRows,
]
let data = try! JSONSerialization.data(withJSONObject: payload, options: [.sortedKeys])
print(String(data: data, encoding: .utf8)!)
"#;
    match Command::new("/usr/bin/xcrun")
        .args(["swift", "-e", SCRIPT, &pid.to_string()])
        .output()
    {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).trim().to_owned()
        }
        Ok(output) => format!("diagnostic failed: {output:?}"),
        Err(error) => format!("diagnostic launch failed: {error}"),
    }
}

/// Recovery preserves the already-observed native window identity. Visibility is
/// a separate launch contract, so this check deliberately uses the all-window
/// census while keeping the existing bounded observation deadline.
fn await_same_native_windows(pid: u32, title: &str, expected: &[u32]) -> Vec<u32> {
    assert!(
        !expected.is_empty(),
        "same-window recovery needs an initially observed native window"
    );
    query_native_windows(
        pid,
        title,
        NativeWindowScope::All,
        NativeWindowExpectation::Exact(expected),
        "recovery-same-window",
    )
}

fn await_no_native_windows(pid: u32, title: &str) {
    let windows = query_native_windows(
        pid,
        title,
        NativeWindowScope::All,
        NativeWindowExpectation::Exact(&[]),
        "initial-navigation-window-release",
    );
    assert!(
        windows.is_empty(),
        "native window remained after startup rollback: {windows:?}"
    );
}

#[derive(Clone, Copy)]
enum NativeWindowScope {
    OnScreen,
    All,
}

#[derive(Clone, Copy)]
enum NativeWindowExpectation<'a> {
    Snapshot,
    Exact(&'a [u32]),
}

fn query_native_windows(
    pid: u32,
    title: &str,
    scope: NativeWindowScope,
    expectation: NativeWindowExpectation<'_>,
    observation: &str,
) -> Vec<u32> {
    const SCRIPT: &str = include_str!("fixtures/native_window_census.swift");
    let expectation_arg = match expectation {
        NativeWindowExpectation::Snapshot => String::from("snapshot"),
        NativeWindowExpectation::Exact(windows) => format!(
            "exact:{}",
            windows
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(",")
        ),
    };
    let scope_arg = match scope {
        NativeWindowScope::OnScreen => "on-screen",
        NativeWindowScope::All => "all",
    };
    let timeout_arg = EVENT_DEADLINE.as_secs().to_string();
    let output = Command::new("/usr/bin/xcrun")
        .args([
            "swift",
            "-e",
            SCRIPT,
            "query",
            &pid.to_string(),
            title,
            &expectation_arg,
            &timeout_arg,
            scope_arg,
        ])
        .output()
        .expect("run native CoreGraphics census");
    assert!(
        output.status.success(),
        "CoreGraphics census ({observation}): {output:?}; target-PID CoreGraphics rows: {}",
        native_window_rows(pid)
    );
    String::from_utf8(output.stdout)
        .expect("CoreGraphics output UTF-8")
        .lines()
        .map(|line| line.parse().expect("CGWindowID is numeric"))
        .collect()
}

fn session_dirs_for(pid: u32) -> Vec<PathBuf> {
    let prefix = format!("kb-{pid:x}-");
    [
        std::env::temp_dir(),
        PathBuf::from("/tmp"),
        PathBuf::from("/var/tmp"),
    ]
    .into_iter()
    .flat_map(|base| {
        fs::read_dir(base)
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
    })
    .filter(|entry| entry.file_name().to_string_lossy().starts_with(&prefix))
    .map(|entry| entry.path())
    .collect()
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
#[test]
#[ignore = "requires local Apple code-signing identities and a real signed host fixture"]
fn kel135_macos_package_identity_uses_only_validated_running_signature_facts() {
    let fixture = ProductFixture::new("kel135-signed-profile-host");
    let stage = fixture.stage();
    let app_root = fixture.root.path().join("signed-apps");
    fs::create_dir(&app_root).expect("create signed fixture parent");
    fs::set_permissions(&app_root, fs::Permissions::from_mode(0o700))
        .expect("protect signed fixture parent");
    let signing_hashes = valid_macos_codesign_hashes();
    assert!(
        !signing_hashes.is_empty(),
        "no valid Apple code-signing identity is available"
    );

    let app_a = build_signed_profile_app(
        stage.root(),
        &app_root,
        "ProfileAppA",
        "dev.keld.fixture.profile.a",
        &signing_hashes[0],
    );
    let app_b = build_signed_profile_app(
        stage.root(),
        &app_root,
        "ProfileAppB",
        "dev.keld.fixture.profile.b",
        &signing_hashes[0],
    );
    let identity_a = run_signed_identity_report(&app_a);
    let identity_b = run_signed_identity_report(&app_b);
    assert_eq!(identity_a["team_id"], identity_b["team_id"]);
    assert_ne!(
        identity_a["signing_identifier"],
        identity_b["signing_identifier"]
    );
    assert_ne!(
        identity_a["profile_identity"],
        identity_b["profile_identity"]
    );
    assert_ne!(identity_a["store_uuid"], identity_b["store_uuid"]);

    let mut other_publisher = None;
    for (index, signer) in signing_hashes.iter().skip(1).enumerate() {
        let candidate = build_signed_profile_app(
            stage.root(),
            &app_root,
            &format!("OtherPublisher{index}"),
            "dev.keld.fixture.profile.a",
            signer,
        );
        let observed = run_signed_identity_report(&candidate);
        if observed["team_id"] != identity_a["team_id"] {
            other_publisher = Some((candidate, observed));
            break;
        }
    }
    let (_other_publisher_app, identity_other) = other_publisher
        .expect("a valid different-publisher signing fixture is required for KEL-135/T3");
    assert_eq!(
        identity_a["signing_identifier"],
        identity_other["signing_identifier"]
    );
    assert_ne!(identity_a["team_id"], identity_other["team_id"]);
    assert_ne!(
        identity_a["profile_identity"],
        identity_other["profile_identity"]
    );
    assert_ne!(identity_a["store_uuid"], identity_other["store_uuid"]);

    let ad_hoc_app = build_signed_profile_app(
        stage.root(),
        &app_root,
        "ProfileAppAdHoc",
        &format!("dev.keld.fixture.profile.adhoc.{}", std::process::id()),
        "-",
    );
    let ad_hoc_verification = Command::new("/usr/bin/codesign")
        .args(["--verify", "--deep", "--strict"])
        .arg(&ad_hoc_app)
        .output()
        .expect("verify ad-hoc control signature");
    assert!(
        ad_hoc_verification.status.success(),
        "ad-hoc control must have an intact signature: {ad_hoc_verification:?}"
    );
    let ad_hoc_output = Command::new(signed_host_executable(&ad_hoc_app))
        .arg("--keld-profile-identity-fixture-v1")
        .output()
        .expect("run ad-hoc identity negative control");
    let ad_hoc_text = format!(
        "{}{}",
        String::from_utf8_lossy(&ad_hoc_output.stdout),
        String::from_utf8_lossy(&ad_hoc_output.stderr)
    );
    assert!(!ad_hoc_output.status.success());
    assert!(
        !ad_hoc_text.contains("KELD_KEL135_SIGNED_IDENTITY"),
        "non-Apple ad-hoc signature disclosed package identity: {ad_hoc_text}"
    );

    let invalid_app = app_root.join("ProfileAppInvalid");
    fs::create_dir(&invalid_app).expect("create invalid-signature fixture directory");
    fs::set_permissions(&invalid_app, fs::Permissions::from_mode(0o700))
        .expect("protect invalid-signature fixture directory");
    copy_profile_fixture_tree(&app_a, &invalid_app);
    let invalid_executable = signed_host_executable(&invalid_app);
    fs::set_permissions(&invalid_executable, fs::Permissions::from_mode(0o700))
        .expect("enable signed Mach-O mutation control");
    fs::OpenOptions::new()
        .append(true)
        .open(&invalid_executable)
        .and_then(|mut file| file.write_all(&[0]))
        .expect("mutate signed Mach-O bytes");
    let invalid_verify = Command::new("/usr/bin/codesign")
        .args(["--verify", "--strict"])
        .arg(&invalid_executable)
        .output()
        .expect("verify tampered signed app");
    assert!(
        !invalid_verify.status.success(),
        "tampered app signature verified"
    );
    let invalid_output = Command::new(&invalid_executable)
        .arg("--keld-profile-identity-fixture-v1")
        .output()
        .expect("launch tampered app identity control");
    let invalid_text = format!(
        "{}{}",
        String::from_utf8_lossy(&invalid_output.stdout),
        String::from_utf8_lossy(&invalid_output.stderr)
    );
    assert!(
        !invalid_text.contains("KELD_KEL135_SIGNED_IDENTITY"),
        "invalid signature disclosed an app identity: {invalid_text}"
    );
    eprintln!(
        "KELD_KEL135_MACOS_PACKAGE_IDENTITY macos={} team={} app_a_identifier={} app_b_identifier={} other_publisher_team={} other_publisher_identifier={} app_a_profile={} app_b_profile={} other_publisher_profile={} invalid_signature_rejected=true ad_hoc_rejected=true",
        sw_vers_value("-productVersion"),
        identity_a["team_id"],
        identity_a["signing_identifier"],
        identity_b["signing_identifier"],
        identity_other["team_id"],
        identity_other["signing_identifier"],
        identity_a["profile_identity"],
        identity_b["profile_identity"],
        identity_other["profile_identity"],
    );
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
#[test]
#[ignore = "requires local Apple code-signing identities and a real signed host fixture"]
fn kel135_macos_signed_profiles_isolate_same_origin_state_across_launches() {
    let fixture = ProductFixture::new("kel135-signed-profile-ab");
    let stage = fixture.stage();
    let app_root = fixture.root.path().join("signed-apps");
    fs::create_dir(&app_root).expect("create signed fixture parent");
    fs::set_permissions(&app_root, fs::Permissions::from_mode(0o700))
        .expect("protect signed fixture parent");
    let signing_hashes = valid_macos_codesign_hashes();
    let signer = signing_hashes
        .first()
        .expect("a valid Apple code-signing identity is required");
    let fixture_id = format!("{}", std::process::id());
    let app_a = build_signed_profile_app(
        stage.root(),
        &app_root,
        "ProfileAppA",
        &format!("dev.keld.fixture.profile.a.{fixture_id}"),
        signer,
    );
    let app_b = build_signed_profile_app(
        stage.root(),
        &app_root,
        "ProfileAppB",
        &format!("dev.keld.fixture.profile.b.{fixture_id}"),
        signer,
    );
    let identity_a = run_signed_identity_report(&app_a);
    let identity_b = run_signed_identity_report(&app_b);
    assert_eq!(identity_a["team_id"], identity_b["team_id"]);
    assert_ne!(
        identity_a["signing_identifier"],
        identity_b["signing_identifier"]
    );
    assert_ne!(identity_a["store_uuid"], identity_b["store_uuid"]);

    let support_root = fixture.root.path().join("application-support");
    fs::create_dir(&support_root).expect("create isolated support root");
    fs::set_permissions(&support_root, fs::Permissions::from_mode(0o700))
        .expect("protect isolated support root");
    let mut origin = ProfileOrigin::new();

    let seeded = origin.run_profile(
        &app_a,
        &support_root,
        "seed",
        "app-a-seed",
        Some("keld-kel135-A-state"),
    );
    assert_eq!(
        seeded.get("local"),
        Some(&String::from("keld-kel135-A-state"))
    );
    assert_eq!(
        seeded.get("cookie"),
        Some(&String::from("keld-kel135-A-state"))
    );
    assert_eq!(
        seeded.get("idb"),
        Some(&String::from("keld-kel135-A-state"))
    );
    assert_eq!(
        seeded.get("cache"),
        Some(&String::from("keld-kel135-A-state"))
    );
    assert_eq!(seeded.get("sw").map(String::as_str), Some("true"));

    let persisted_a = origin.run_profile(&app_a, &support_root, "read", "app-a-read", None);
    for key in ["local", "cookie", "idb", "cache", "sw"] {
        assert_eq!(
            persisted_a.get(key),
            seeded.get(key),
            "same app {key} state"
        );
    }

    let isolated_b = origin.run_profile(&app_b, &support_root, "read", "app-b-read", None);
    assert_eq!(isolated_b.get("local").map(String::as_str), Some(""));
    assert_eq!(isolated_b.get("cookie").map(String::as_str), Some(""));
    assert_eq!(isolated_b.get("idb").map(String::as_str), Some(""));
    assert_eq!(isolated_b.get("cache").map(String::as_str), Some(""));
    assert_eq!(isolated_b.get("sw").map(String::as_str), Some("false"));

    let log_a = fs::read_to_string(support_root.join("app-a-read.log"))
        .expect("read signed app A report log");
    let log_b = fs::read_to_string(support_root.join("app-b-read.log"))
        .expect("read signed app B report log");
    assert_store_report_matches(&log_a, &identity_a["store_uuid"]);
    assert_store_report_matches(&log_b, &identity_b["store_uuid"]);
    let purge_crash = run_signed_purge_crash_probe(&app_a, &support_root);
    assert_eq!(purge_crash.status.code(), Some(86));
    assert!(
        String::from_utf8_lossy(&purge_crash.stderr).contains("after_removal_callback=true"),
        "purge interruption did not occur after WebKit's removal barrier"
    );
    let purge_a = run_signed_purge_report(&app_a, &support_root);
    let purge_b = run_signed_purge_report(&app_b, &support_root);
    assert!(purge_a.contains(&format!("store_uuid={}", identity_a["store_uuid"])));
    assert!(purge_a.contains("store_absent=true"));
    assert!(purge_b.contains(&format!("store_uuid={}", identity_b["store_uuid"])));
    assert!(purge_b.contains("store_absent=true"));
    eprintln!(
        "KELD_KEL135_MACOS_APP_AB os={} webkit={} origin={} app_a_team={} app_a_identifier={} app_a_uuid={} app_b_team={} app_b_identifier={} app_b_uuid={} state=localStorage,cookie,indexedDB,CacheStorage,serviceWorker lifecycle=clean-stop purge=callback-and-enumeration-and-recovery negative_control=same-origin-empty",
        sw_vers_value("-productVersion"),
        webkit_version(),
        origin.address,
        identity_a["team_id"],
        identity_a["signing_identifier"],
        identity_a["store_uuid"],
        identity_b["team_id"],
        identity_b["signing_identifier"],
        identity_b["store_uuid"],
    );
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
fn exercise_fsynced_purge_phase_recovery(
    stage: &keld_cli::boot::DevBootStage,
    app_root: &Path,
    support_root: &Path,
    signer: &str,
    origin: &mut ProfileOrigin,
    phase: &str,
) -> String {
    let app = build_signed_profile_app(
        stage.root(),
        app_root,
        &format!("ProfileAppPurgeRecovery-{phase}"),
        &format!(
            "dev.keld.fixture.profile.purge.{}.{}",
            std::process::id(),
            phase.to_ascii_lowercase()
        ),
        signer,
    );
    let identity = run_signed_identity_report(&app);
    let phase_support = support_root.join(phase);
    fs::create_dir(&phase_support).expect("create isolated purge recovery metadata root");
    fs::set_permissions(&phase_support, fs::Permissions::from_mode(0o700))
        .expect("protect isolated purge recovery metadata root");
    let nonce = format!("keld-kel135-purge-{phase}");
    let seeded = origin.run_profile(
        &app,
        &phase_support,
        "seed",
        &format!("purge-{phase}-seed"),
        Some(&nonce),
    );
    assert_eq!(seeded.get("local"), Some(&nonce));
    let seed_log = fs::read_to_string(phase_support.join(format!("purge-{phase}-seed.log")))
        .expect("read purge recovery seed store report");
    assert_store_report_matches(&seed_log, &identity["store_uuid"]);
    let interrupted = run_signed_purge_phase_crash_probe(&app, &phase_support, phase);
    assert_eq!(
        interrupted.status.code(),
        Some(88),
        "phase {phase} missed crash"
    );
    assert!(
        String::from_utf8_lossy(&interrupted.stderr)
            .contains(&format!("after_fsynced_phase={phase}")),
        "phase {phase} crash was not after its durable intent write"
    );
    let resumed = run_signed_purge_report(&app, &phase_support);
    assert!(resumed.contains(&format!("store_uuid={}", identity["store_uuid"])));
    assert!(resumed.contains("store_absent=true"));
    let presence = run_signed_store_presence_report(&app);
    assert!(presence.contains(&format!("store_uuid={}", identity["store_uuid"])));
    assert!(presence.contains("present=false"));
    format!("{phase}:crash88,recovered,enumerated-absent")
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
#[test]
#[ignore = "requires Apple signing and real WKWebsiteDataStore purge callbacks"]
fn kel135_macos_purge_recovers_after_fsynced_later_phase_writes() {
    let fixture = ProductFixture::new("kel135-signed-purge-later-phase-recovery");
    let stage = fixture.stage();
    let app_root = fixture.root.path().join("signed-apps");
    fs::create_dir(&app_root).expect("create signed purge recovery fixture parent");
    fs::set_permissions(&app_root, fs::Permissions::from_mode(0o700))
        .expect("protect signed purge recovery fixture parent");
    let signer = valid_macos_codesign_hashes()
        .into_iter()
        .next()
        .expect("a valid Apple code-signing identity is required");
    let support_root = fixture.root.path().join("application-support");
    fs::create_dir(&support_root).expect("create isolated purge recovery support root");
    fs::set_permissions(&support_root, fs::Permissions::from_mode(0o700))
        .expect("protect isolated purge recovery support root");
    let mut origin = ProfileOrigin::new();
    let results = [
        "StoreAbsent",
        "ReverseRemoved",
        "ForwardRemoved",
        "Inactive",
    ]
    .into_iter()
    .map(|phase| {
        exercise_fsynced_purge_phase_recovery(
            &stage,
            &app_root,
            &support_root,
            &signer,
            &mut origin,
            phase,
        )
    })
    .collect::<Vec<_>>();
    eprintln!(
        "KELD_KEL135_MACOS_PURGE_PHASE_RECOVERY os={} webkit={} phases={} exact_enumeration=true",
        sw_vers_value("-productVersion"),
        webkit_version(),
        results.join(","),
    );
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
#[test]
#[ignore = "requires a second standard macOS account, an authenticated sudo session, and a signed host fixture"]
fn kel135_macos_second_user_cannot_read_same_signed_profile_state() {
    let username = std::env::var("KELD_KEL135_SECOND_USER")
        .expect("set KELD_KEL135_SECOND_USER to the temporary standard macOS login");
    assert!(
        !username.is_empty()
            && username
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_'),
        "second-user login name must be lowercase ASCII, digits, or underscore"
    );
    let second_uid = account_numeric_value(&username);
    let first_uid = current_account_numeric_id();
    assert!(
        first_uid != second_uid,
        "second user UID must differ from the current user"
    );
    assert!(second_uid >= 501, "second user is a system/service account");
    let groups = account_groups(&username);
    assert!(
        !groups.split_whitespace().any(|group| group == "admin"),
        "second account must be an ordinary non-admin user"
    );
    let home = account_home(&username);
    let current_home = std::env::var_os("HOME").map(PathBuf::from);
    assert!(
        home.starts_with("/Users/") && Some(&home) != current_home.as_ref(),
        "second account must have its own local home directory: {}",
        home.display()
    );

    let sudo_probe = run_as_local_user(&username, "/usr/bin/id", &["-u"], &[]);
    assert!(
        sudo_probe.status.success(),
        "authenticate this Mac's administrator account in Terminal with `sudo -v`, then rerun this acceptance row"
    );
    let run_as_uid_matches = String::from_utf8_lossy(&sudo_probe.stdout)
        .trim()
        .parse::<u32>()
        .ok()
        == Some(second_uid);
    assert!(
        run_as_uid_matches,
        "run-as identity must match the isolated standard user"
    );

    let fixture = ProductFixture::new("kel135-second-user-profile");
    let stage = fixture.stage();
    let shared_temp = tempfile::Builder::new()
        .prefix("keld-kel135-second-user-")
        .tempdir_in("/Users/Shared")
        .expect("create cross-user-readable signed fixture directory");
    fs::set_permissions(shared_temp.path(), fs::Permissions::from_mode(0o755))
        .expect("allow the standard test account to traverse its private fixture directory");
    let first_profile_temp = tempfile::Builder::new()
        .prefix("keld-kel135-first-profile-")
        .tempdir()
        .expect("create persistent first-user test metadata root");
    fs::set_permissions(first_profile_temp.path(), fs::Permissions::from_mode(0o700))
        .expect("protect first-user profile metadata root");
    let signer = valid_macos_codesign_hashes()
        .into_iter()
        .next()
        .expect("an Apple Development signing identity is required");
    let app = build_signed_profile_app(
        stage.root(),
        shared_temp.path(),
        "ProfileAppSecondUser",
        &format!(
            "dev.keld.fixture.profile.second-user.{}",
            std::process::id()
        ),
        &signer,
    );
    make_signed_fixture_readable_by_standard_users(&app);
    let first_identity = run_signed_identity_report(&app);

    let mut origin = ProfileOrigin::new();
    let user_fixture_root = home
        .join("Library/Caches/Keld/KEL-135")
        .join(format!("second-user-{}", std::process::id()));
    let user_temp = user_fixture_root.join("tmp");
    let user_profile_root = user_temp.join("profile");
    let mut preflight_cleanup = MacSecondUserPreflightCleanup {
        username: username.clone(),
        user_fixture_root: user_fixture_root.clone(),
        armed: true,
    };
    let create_roots = run_as_local_user(
        &username,
        "/bin/mkdir",
        &["-p", path_text(&user_temp), path_text(&user_profile_root)],
        &[],
    );
    assert!(
        create_roots.status.success(),
        "create owner-private second-user profile roots: {create_roots:?}"
    );
    let second_codesign = run_as_local_user(
        &username,
        "/usr/bin/codesign",
        &["--verify", "--deep", "--strict", path_text(&app)],
        &[],
    );
    assert!(
        second_codesign.status.success(),
        "second user cannot verify the readable signed fixture: {second_codesign:?}"
    );
    let second_identity = run_signed_identity_report_as_user(&username, &user_temp, &app);
    for field in [
        "team_id",
        "signing_identifier",
        "profile_identity",
        "store_uuid",
    ] {
        assert_eq!(
            first_identity.get(field),
            second_identity.get(field),
            "the OS user is the only identity dimension changed"
        );
    }

    let shared = shared_temp.keep();
    let first_profile_root = first_profile_temp.keep();
    eprintln!(
        "KELD_KEL135_MACOS_SECOND_USER_RECOVERY app={} first_profile_root={} second_profile_root={}",
        app.display(),
        first_profile_root.display(),
        user_profile_root.display(),
    );
    let mut cleanup = MacSecondUserProfileCleanup {
        username: username.clone(),
        user_temp: user_temp.clone(),
        user_fixture_root: user_fixture_root.clone(),
        user_profile_root: user_profile_root.clone(),
        app: app.clone(),
        first_profile_root: first_profile_root.clone(),
        shared_fixture_root: shared.clone(),
        first_profile_purged: false,
        second_profile_purged: false,
        armed: true,
    };
    preflight_cleanup.armed = false;
    let seeded = origin.run_profile(
        &app,
        &first_profile_root,
        "seed",
        "same-user-seed",
        Some("keld-kel135-second-user-state"),
    );
    for key in ["local", "cookie", "idb", "cache"] {
        assert_eq!(
            seeded.get(key).map(String::as_str),
            Some("keld-kel135-second-user-state"),
            "same-user positive control for {key}"
        );
    }
    assert_eq!(seeded.get("sw").map(String::as_str), Some("true"));

    let (second_user_state, second_user_output) = origin.run_profile_as_user(
        &username,
        &user_temp,
        &user_profile_root,
        &app,
        "read",
        "other-user-read",
        None,
    );
    for key in ["local", "cookie", "idb", "cache"] {
        assert_eq!(
            second_user_state.get(key).map(String::as_str),
            Some(""),
            "second standard user's same-origin {key} must start empty"
        );
    }
    assert_eq!(
        second_user_state.get("sw").map(String::as_str),
        Some("false")
    );
    let second_user_log = format!(
        "{}{}",
        String::from_utf8_lossy(&second_user_output.stdout),
        String::from_utf8_lossy(&second_user_output.stderr)
    );
    assert_store_report_matches(&second_user_log, &first_identity["store_uuid"]);

    let second_purge =
        run_signed_purge_report_as_user(&username, &user_temp, &user_profile_root, &app);
    assert!(
        second_purge.status.success()
            && String::from_utf8_lossy(&second_purge.stdout)
                .contains(&format!("store_uuid={}", first_identity["store_uuid"]))
            && String::from_utf8_lossy(&second_purge.stdout).contains("store_absent=true"),
        "second user's exact-identity purge failed: {second_purge:?}"
    );
    cleanup.second_profile_purged = true;
    let first_after = origin.run_profile(
        &app,
        &first_profile_root,
        "read",
        "same-user-after-other-user-purge",
        None,
    );
    for key in ["local", "cookie", "idb", "cache"] {
        assert_eq!(
            first_after.get(key),
            seeded.get(key),
            "second-user access and purge must not alter the first user's {key}"
        );
    }
    assert_eq!(first_after.get("sw"), seeded.get("sw"));
    let first_purge = run_signed_purge_report(&app, &first_profile_root);
    assert!(first_purge.contains("store_absent=true"));
    cleanup.first_profile_purged = true;

    let user_root_cleanup = run_as_local_user(
        &username,
        "/bin/rm",
        &["-rf", "--", path_text(&user_fixture_root)],
        &[],
    );
    assert!(
        user_root_cleanup.status.success(),
        "remove second-user test roots: {user_root_cleanup:?}"
    );
    fs::remove_dir_all(&first_profile_root)
        .expect("remove first-user profile metadata only after WebKit purge verification");
    fs::remove_dir_all(&shared)
        .expect("remove signed fixture after both users pass exact-profile purge");
    cleanup.armed = false;
    eprintln!(
        "KELD_KEL135_MACOS_SECOND_USER_SAFE_TO_DELETE_ACCOUNT exact_purge_a=true exact_purge_b=true second_user_test_root_removed=true"
    );
    eprintln!(
        "KELD_KEL135_MACOS_SECOND_USER os={} webkit={} origin={} team={} identifier={} profile_identity={} store_uuid={} uid_a_and_b_distinct=true state=localStorage,cookie,IndexedDB,CacheStorage,serviceWorker user_b_same_origin=empty user_a_after_b=preserved lifecycle=clean-stop purge=both-user-exact-identity platform_path_acl_claim=none",
        sw_vers_value("-productVersion"),
        webkit_version(),
        origin.address,
        first_identity["team_id"],
        first_identity["signing_identifier"],
        first_identity["profile_identity"],
        first_identity["store_uuid"],
    );
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
struct MacSecondUserPreflightCleanup {
    username: String,
    user_fixture_root: PathBuf,
    armed: bool,
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
impl Drop for MacSecondUserPreflightCleanup {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let removed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run_as_local_user(
                &self.username,
                "/bin/rm",
                &["-rf", "--", path_text(&self.user_fixture_root)],
                &[],
            )
            .status
            .success()
        }));
        if matches!(removed, Ok(true)) {
            eprintln!(
                "KELD_KEL135_MACOS_SECOND_USER_SAFE_TO_DELETE_ACCOUNT preflight_no_webkit_store=true"
            );
        } else {
            eprintln!(
                "KELD_KEL135_MACOS_SECOND_USER_CLEANUP_INCOMPLETE preflight_test_root_retained=true"
            );
        }
    }
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
struct MacSecondUserProfileCleanup {
    username: String,
    user_temp: PathBuf,
    user_fixture_root: PathBuf,
    user_profile_root: PathBuf,
    app: PathBuf,
    first_profile_root: PathBuf,
    shared_fixture_root: PathBuf,
    first_profile_purged: bool,
    second_profile_purged: bool,
    armed: bool,
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
impl Drop for MacSecondUserProfileCleanup {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let cleanup = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let second_ok = if self.second_profile_purged {
                true
            } else {
                let second = run_signed_purge_report_as_user(
                    &self.username,
                    &self.user_temp,
                    &self.user_profile_root,
                    &self.app,
                );
                second.status.success()
                    && String::from_utf8_lossy(&second.stdout).contains("store_absent=true")
            };
            let first_ok = if self.first_profile_purged {
                true
            } else {
                run_signed_purge_report(&self.app, &self.first_profile_root)
                    .contains("store_absent=true")
            };
            let roots_removed = if second_ok && first_ok {
                let user_root_removed = run_as_local_user(
                    &self.username,
                    "/bin/rm",
                    &["-rf", "--", path_text(&self.user_fixture_root)],
                    &[],
                )
                .status
                .success();
                let first_root_removed = if self.first_profile_root.exists() {
                    fs::remove_dir_all(&self.first_profile_root).is_ok()
                } else {
                    true
                };
                let shared_root_removed = if self.shared_fixture_root.exists() {
                    fs::remove_dir_all(&self.shared_fixture_root).is_ok()
                } else {
                    true
                };
                user_root_removed && first_root_removed && shared_root_removed
            } else {
                false
            };
            second_ok && first_ok && roots_removed
        }));
        match cleanup {
            Ok(true) => eprintln!(
                "KELD_KEL135_MACOS_SECOND_USER_SAFE_TO_DELETE_ACCOUNT exact_purge_a=true exact_purge_b=true second_user_test_root_removed=true failure_cleanup=true"
            ),
            _ => eprintln!(
                "KELD_KEL135_MACOS_SECOND_USER_CLEANUP_INCOMPLETE retain_account_and_profile_metadata=true"
            ),
        }
    }
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
#[test]
fn kel135_macos_dev_profiles_are_ephemeral_across_launches() {
    let fixture = ProductFixture::new("kel135-dev-ephemeral-profile");
    let stage = fixture.stage();
    let support_root = fixture.root.path().join("application-support");
    fs::create_dir(&support_root).expect("create isolated support root");
    fs::set_permissions(&support_root, fs::Permissions::from_mode(0o700))
        .expect("protect isolated support root");
    let mut origin = ProfileOrigin::new();

    let seeded = origin.run_ephemeral_profile(
        stage.host(),
        &support_root,
        "seed",
        "dev-seed",
        Some("keld-kel135-dev-state"),
    );
    assert_eq!(
        seeded.get("local").map(String::as_str),
        Some("keld-kel135-dev-state")
    );
    assert_eq!(
        seeded.get("cookie").map(String::as_str),
        Some("keld-kel135-dev-state")
    );
    assert_eq!(
        seeded.get("idb").map(String::as_str),
        Some("keld-kel135-dev-state")
    );
    assert_eq!(
        seeded.get("cache").map(String::as_str),
        Some("keld-kel135-dev-state")
    );
    assert_eq!(seeded.get("sw").map(String::as_str), Some("true"));

    let next_launch =
        origin.run_ephemeral_profile(stage.host(), &support_root, "read", "dev-read", None);
    for key in ["local", "cookie", "idb", "cache"] {
        assert_eq!(
            next_launch.get(key).map(String::as_str),
            Some(""),
            "dev {key}"
        );
    }
    assert_eq!(next_launch.get("sw").map(String::as_str), Some("false"));
    let first_log = fs::read_to_string(support_root.join("dev-seed.log"))
        .expect("read first ephemeral store evidence");
    let second_log = fs::read_to_string(support_root.join("dev-read.log"))
        .expect("read second ephemeral store evidence");
    assert_ephemeral_store_report(&first_log);
    assert_ephemeral_store_report(&second_log);
    eprintln!(
        "KELD_KEL135_MACOS_DEV_EPHEMERAL os={} webkit={} origin={} first_launch_state=stored next_launch_state=empty store_identifier=none persistent=false",
        sw_vers_value("-productVersion"),
        webkit_version(),
        origin.address,
    );
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
#[test]
#[ignore = "requires the real macOS WKWebView event loop"]
fn kel135_macos_fatal_fixture_command_reports_failure_after_cleanup() {
    let fixture = ProductFixture::new("kel135-macos-fatal-profile-fixture");
    let stage = fixture.stage();
    let support_root = fixture.root.path().join("application-support");
    fs::create_dir(&support_root).expect("create fatal fixture support root");
    fs::set_permissions(&support_root, fs::Permissions::from_mode(0o700))
        .expect("protect fatal fixture support root");
    let mut origin = ProfileOrigin::new();
    let log_path = support_root.join("fatal-fixture.log");
    let log = fs::File::create(&log_path).expect("create fatal fixture log");
    let stderr = log.try_clone().expect("clone fatal fixture log");
    let mut child = Command::new(stage.host())
        .arg("--keld-profile-webview-fixture-v1")
        .env("KELD_PROFILE_TEST_ROOT", &support_root)
        .env("KELD_PROFILE_FIXTURE_EPHEMERAL", "1")
        .env("KELD_PROFILE_FIXTURE_FATAL_ON_STDIN", "1")
        .env("KELD_PROFILE_ACCEPTANCE_REPORT", "1")
        .env(
            "KELD_PROFILE_FIXTURE_URL",
            format!("http://{}/read", origin.address),
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(stderr))
        .spawn()
        .expect("launch real macOS fatal fixture host");
    origin
        .wait_for_report("read", None)
        .expect("fatal fixture rendered before command");
    drop(child.stdin.take());
    let deadline = Instant::now() + PROCESS_DEADLINE;
    let status = loop {
        if let Some(status) = child.try_wait().expect("poll fatal fixture host") {
            break status;
        }
        assert!(Instant::now() < deadline, "fatal fixture did not exit");
        thread::sleep(Duration::from_millis(20));
    };
    assert!(!status.success(), "Fatal was reported as a successful Quit");
    let output = fs::read_to_string(&log_path).expect("read fatal fixture result");
    assert_ephemeral_store_report(&output);
    assert!(output.contains("fatal app session command"), "{output}");
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
#[test]
#[ignore = "requires local Apple code-signing identities and a real signed host fixture"]
fn kel135_macos_same_signed_profile_rejects_concurrent_owner() {
    let fixture = ProductFixture::new("kel135-signed-profile-concurrency");
    let stage = fixture.stage();
    let app_root = fixture.root.path().join("signed-apps");
    fs::create_dir(&app_root).expect("create signed fixture parent");
    fs::set_permissions(&app_root, fs::Permissions::from_mode(0o700))
        .expect("protect signed fixture parent");
    let signer = valid_macos_codesign_hashes()
        .into_iter()
        .next()
        .expect("a valid Apple code-signing identity is required");
    let app = build_signed_profile_app(
        stage.root(),
        &app_root,
        "ProfileAppConcurrent",
        &format!("dev.keld.fixture.profile.concurrent.{}", std::process::id()),
        &signer,
    );
    let identity = run_signed_identity_report(&app);
    let support_root = fixture.root.path().join("application-support");
    fs::create_dir(&support_root).expect("create isolated support root");
    fs::set_permissions(&support_root, fs::Permissions::from_mode(0o700))
        .expect("protect isolated support root");
    let mut origin = ProfileOrigin::new();

    let mut first = spawn_profile_host(
        signed_host_executable(&app),
        &support_root,
        &origin.address,
        "seed",
        "concurrency-owner",
    );
    let state = origin
        .wait_for_report("seed", Some("keld-kel135-concurrency-state"))
        .expect("owning app rendered the shared origin");
    assert_eq!(
        state.get("local").map(String::as_str),
        Some("keld-kel135-concurrency-state")
    );

    let second = Command::new(signed_host_executable(&app))
        .arg("--keld-profile-webview-fixture-v1")
        .env("KELD_PROFILE_TEST_ROOT", &support_root)
        .env("KELD_PROFILE_ACCEPTANCE_REPORT", "1")
        .env(
            "KELD_PROFILE_FIXTURE_URL",
            format!("http://{}/unused", origin.address),
        )
        .stdin(Stdio::null())
        .output()
        .expect("launch competing same-identity host");
    assert!(!second.status.success(), "second same-profile host started");
    let second_error = String::from_utf8_lossy(&second.stderr);
    assert!(
        second_error.contains("already in use"),
        "competing host failed for an unexpected reason: {second_error}"
    );
    stop_profile_host(&mut first);
    let log = fs::read_to_string(support_root.join("concurrency-owner.log"))
        .expect("read active owner WebKit store evidence");
    assert_store_report_matches(&log, &identity["store_uuid"]);
    let purge = run_signed_purge_report(&app, &support_root);
    assert!(purge.contains("store_absent=true"));
    eprintln!(
        "KELD_KEL135_MACOS_CONCURRENCY os={} webkit={} team={} identifier={} uuid={} first_owner=active second_owner=rejected_by_lock cleanup=exact-identity-purge",
        sw_vers_value("-productVersion"),
        webkit_version(),
        identity["team_id"],
        identity["signing_identifier"],
        identity["store_uuid"],
    );
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
struct MacProfilePurgeCleanup {
    apps: Vec<PathBuf>,
    support_root: PathBuf,
    fixture_root: PathBuf,
    armed: bool,
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
#[derive(Clone, Copy, Default)]
struct MediaRestartProof {
    seed: MediaSeedProof,
    continuity: MediaContinuityProof,
    denial: MediaDenialProof,
    same_origin: bool,
    nonce_survived: bool,
    allow_capture: bool,
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
#[derive(Clone, Copy, Default)]
struct MediaSeedProof {
    qualified_capture: bool,
    site_prompt_observed: bool,
    same_page_repeat_capture: bool,
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
#[derive(Clone, Copy, Default)]
struct MediaContinuityProof {
    clean_restart: bool,
    same_signed_identity: bool,
    same_store: bool,
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
#[derive(Clone, Copy, Default)]
struct MediaDenialProof {
    tcc_authorized: bool,
    guarded_callback: bool,
    no_capture_or_prompt: bool,
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
impl MediaRestartProof {
    fn is_complete(&self) -> bool {
        self.seed.qualified_capture
            && self.seed.site_prompt_observed
            && self.seed.same_page_repeat_capture
            && self.continuity.clean_restart
            && self.continuity.same_signed_identity
            && self.continuity.same_store
            && self.denial.tcc_authorized
            && self.same_origin
            && self.nonce_survived
            && self.denial.guarded_callback
            && self.denial.no_capture_or_prompt
            && self.allow_capture
    }
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
#[derive(Clone)]
struct MediaRecordedRun {
    report: std::collections::BTreeMap<String, String>,
    log: String,
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
struct MediaScenario {
    kind: &'static str,
    track: &'static str,
    seed: MediaRecordedRun,
    denied: MediaRecordedRun,
    allowed: MediaRecordedRun,
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
fn same_present_run_fact(runs: &[&MediaRecordedRun], key: &str) -> bool {
    let Some(first) = runs[0].report.get(key).filter(|value| !value.is_empty()) else {
        return false;
    };
    runs.iter().all(|run| run.report.get(key) == Some(first))
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
fn same_verified_signed_run(runs: &[&MediaRecordedRun]) -> bool {
    [
        "host_executable",
        "host_sha256",
        "host_cdhash",
        "signed_team_id",
        "signed_signing_identifier",
        "signed_profile_identity",
        "signed_store_uuid",
    ]
    .iter()
    .all(|key| same_present_run_fact(runs, key))
        && runs.iter().all(|run| {
            run.report
                .get("signed_signature_validated_before_identity_read")
                .map(String::as_str)
                == Some("true")
        })
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
fn distinct_clean_host_runs(runs: &[&MediaRecordedRun]) -> bool {
    let pids = runs
        .iter()
        .map(|run| run.report.get("host_pid").filter(|value| !value.is_empty()))
        .collect::<Vec<_>>();
    pids.iter().all(Option::is_some)
        && pids
            .iter()
            .enumerate()
            .all(|(index, pid)| pids.iter().skip(index + 1).all(|other| pid != other))
        && runs
            .iter()
            .all(|run| run.report.get("host_clean_exit").map(String::as_str) == Some("true"))
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
fn media_callback_matches(log: &str, kind: &str, response: &str) -> bool {
    log.matches("KELD_KEL135_MEDIA_CALLBACK").count() == 1
        && log.contains(&format!(
            "KELD_KEL135_MEDIA_CALLBACK kind={kind} response={response}"
        ))
        && log.contains("principal=Webview {")
        && log.contains("guard_decision=Some(Deny(")
        && log.contains("policy=PermissionsManifest { app: {} }")
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
fn media_qualified_label(
    kind: &str,
    track: &str,
    report: &std::collections::BTreeMap<String, String>,
) -> Option<String> {
    let expected_live = format!("resolved-{track}-live");
    if report.get("media").map(String::as_str) != Some(expected_live.as_str()) {
        return None;
    }
    if kind == "camera" && report.get("frame_progress").map(String::as_str) != Some("progressed") {
        return None;
    }
    let label = report
        .get("device_label_hex")
        .filter(|value| !value.is_empty())?;
    let label = media_label_from_hex(label);
    let required_class = if kind == "camera" {
        "os-virtual-camo"
    } else {
        "physical-builtin"
    };
    (selected_media_source_class(kind, &label) == required_class).then_some(label)
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
fn media_restart_proof(
    kind: &str,
    track: &str,
    nonce: &str,
    seed: &MediaRecordedRun,
    restarted: &MediaRecordedRun,
    allowed: &MediaRecordedRun,
) -> MediaRestartProof {
    let seed_report = &seed.report;
    let restart_report = &restarted.report;
    let allow_report = &allowed.report;
    let seed_label = media_qualified_label(kind, track, seed_report);
    let allow_label = media_qualified_label(kind, track, allow_report);
    let expected_live = format!("resolved-{track}-live");
    let seed_sheet_count = media_probe_sheet_count(seed_report);
    let runs = [seed, restarted, allowed];
    let same_app = same_verified_signed_run(&runs);
    let same_store = same_present_run_fact(&runs, "store_actual_identifier")
        && same_present_run_fact(&runs, "store_profile_identity")
        && seed_report
            .get("store_actual_identifier")
            .is_some_and(|value| value != "none")
        && runs.iter().all(|run| {
            run.report.get("store_persistent").map(String::as_str) == Some("true")
                && run.report.get("store_actual_identifier")
                    == run.report.get("store_expected_store_uuid")
                && run.report.get("store_actual_identifier") == run.report.get("signed_store_uuid")
                && run.report.get("store_profile_identity")
                    == run.report.get("signed_profile_identity")
        });
    let same_origin = same_present_run_fact(&runs, "origin");
    MediaRestartProof {
        seed: MediaSeedProof {
            qualified_capture: seed_label.is_some()
                && media_callback_matches(&seed.log, kind, "prompt"),
            site_prompt_observed: seed_sheet_count > 0,
            same_page_repeat_capture: seed_report.get("media_repeat").map(String::as_str)
                == Some(expected_live.as_str())
                && seed_report.get("repeat_label_hex") == seed_report.get("device_label_hex")
                && (kind != "camera"
                    || seed_report.get("repeat_frame_progress").map(String::as_str)
                        == Some("progressed"))
                && seed.log.matches("KELD_KEL135_MEDIA_CALLBACK").count() == 1,
        },
        continuity: MediaContinuityProof {
            clean_restart: distinct_clean_host_runs(&runs),
            same_signed_identity: same_app,
            same_store,
        },
        same_origin,
        nonce_survived: seed_report.get("local").map(String::as_str) == Some(nonce)
            && restart_report.get("local").map(String::as_str) == Some(nonce)
            && allow_report.get("local").map(String::as_str) == Some(nonce),
        denial: MediaDenialProof {
            tcc_authorized: media_probe_tcc_authorized(restart_report, kind)
                && media_probe_tcc_authorized(allow_report, kind),
            guarded_callback: media_callback_matches(&restarted.log, kind, "deny"),
            no_capture_or_prompt: restart_report.get("media").map(String::as_str)
                == Some("error-NotAllowedError")
                && restart_report
                    .get("device_label_hex")
                    .is_some_and(String::is_empty)
                && restart_report.get("media_repeat").map(String::as_str) == Some("not-requested")
                && seed_sheet_count > 0
                && media_probe_sheet_count(restart_report) == 0,
        },
        allow_capture: allow_label.is_some()
            && seed_label == allow_label
            && media_callback_matches(&allowed.log, kind, "allow")
            && media_probe_sheet_count(allow_report) == 0,
    }
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
#[derive(Clone, Copy, Default)]
struct DevMediaProof {
    seed: DevSeedProof,
    continuity: DevContinuityProof,
    fresh: DevFreshProof,
    denial: DevDenialProof,
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
#[derive(Clone, Copy, Default)]
struct DevSeedProof {
    live_track: bool,
    nonce_committed: bool,
    second_view_saw_nonce: bool,
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
#[derive(Clone, Copy, Default)]
struct DevContinuityProof {
    same_signed_identity: bool,
    same_origin: bool,
    clean_restart: bool,
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
#[derive(Clone, Copy, Default)]
struct DevFreshProof {
    ephemeral: bool,
    nonce_absent: bool,
    tcc_authorized: bool,
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
#[derive(Clone, Copy, Default)]
struct DevDenialProof {
    guarded_callback: bool,
    no_capture: bool,
    no_prompt: bool,
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
impl DevMediaProof {
    fn is_complete(&self) -> bool {
        self.seed.live_track
            && self.seed.nonce_committed
            && self.seed.second_view_saw_nonce
            && self.continuity.same_signed_identity
            && self.continuity.same_origin
            && self.continuity.clean_restart
            && self.fresh.ephemeral
            && self.fresh.nonce_absent
            && self.fresh.tcc_authorized
            && self.denial.guarded_callback
            && self.denial.no_capture
            && self.denial.no_prompt
    }
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
fn dev_media_proof(
    kind: &str,
    track: &str,
    nonce: &str,
    seed: &MediaRecordedRun,
    restarted: &MediaRecordedRun,
) -> DevMediaProof {
    let seed_report = &seed.report;
    let restart_report = &restarted.report;
    let expected_live = format!("resolved-{track}-live");
    let runs = [seed, restarted];
    DevMediaProof {
        seed: DevSeedProof {
            live_track: media_qualified_label(kind, track, seed_report).is_some()
                && media_callback_matches(&seed.log, kind, "allow")
                && seed_report.get("media_repeat").map(String::as_str)
                    == Some(expected_live.as_str())
                && seed_report.get("repeat_label_hex") == seed_report.get("device_label_hex")
                && (kind != "camera"
                    || seed_report.get("repeat_frame_progress").map(String::as_str)
                        == Some("progressed")),
            nonce_committed: seed_report.get("local").map(String::as_str) == Some(nonce),
            second_view_saw_nonce: seed_report.get("reuse_local").map(String::as_str)
                == Some(nonce)
                && seed_report.get("reuse_media").map(String::as_str) == Some("not-requested"),
        },
        continuity: DevContinuityProof {
            same_signed_identity: same_verified_signed_run(&runs),
            same_origin: same_present_run_fact(&runs, "origin"),
            clean_restart: distinct_clean_host_runs(&runs),
        },
        fresh: DevFreshProof {
            ephemeral: seed_report.get("store_persistent").map(String::as_str) == Some("false")
                && restart_report.get("store_persistent").map(String::as_str) == Some("false")
                && seed_report
                    .get("store_actual_identifier")
                    .map(String::as_str)
                    == Some("none")
                && restart_report
                    .get("store_actual_identifier")
                    .map(String::as_str)
                    == Some("none")
                && seed_report.get("store_mode").map(String::as_str) == Some("ephemeral-dev")
                && restart_report.get("store_mode").map(String::as_str) == Some("ephemeral-dev"),
            nonce_absent: restart_report.get("local").map(String::as_str) == Some(""),
            tcc_authorized: media_probe_tcc_authorized(restart_report, kind),
        },
        denial: DevDenialProof {
            guarded_callback: media_callback_matches(&restarted.log, kind, "deny"),
            no_capture: restart_report.get("media").map(String::as_str)
                == Some("error-NotAllowedError")
                && restart_report
                    .get("device_label_hex")
                    .is_some_and(String::is_empty),
            no_prompt: media_probe_sheet_count(restart_report) == 0,
        },
    }
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
fn media_probe_sheet_count(report: &std::collections::BTreeMap<String, String>) -> usize {
    assert_eq!(
        report.get("probe_scope").map(String::as_str),
        Some("process-wide")
    );
    report
        .get("probe_sheet_count")
        .expect("signed-host sheet count")
        .parse()
        .expect("numeric signed-host sheet count")
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
fn media_probe_tcc_authorized(
    report: &std::collections::BTreeMap<String, String>,
    kind: &str,
) -> bool {
    let key = match kind {
        "camera" => "probe_camera_tcc",
        "microphone" => "probe_microphone_tcc",
        _ => panic!("unknown media kind in TCC probe"),
    };
    report.get(key).map(String::as_str) == Some("3")
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
fn media_label_from_hex(value: &str) -> String {
    let bytes = value.as_bytes();
    assert_eq!(bytes.len() % 2, 0, "device label hex has an odd length");
    let decoded = bytes
        .chunks_exact(2)
        .map(|pair| {
            let pair = std::str::from_utf8(pair).expect("ASCII device label hex");
            u8::from_str_radix(pair, 16).expect("valid device label hex")
        })
        .collect::<Vec<_>>();
    String::from_utf8(decoded).expect("UTF-8 device label")
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
fn camo_extension_matches_reviewed_version() -> bool {
    let extension = "/Applications/Camo Studio.app/Contents/Library/SystemExtensions/com.reincubate.macos.cam.avextension.systemextension/Contents/Info.plist";
    let read_plist = |key| {
        Command::new("/usr/libexec/PlistBuddy")
            .args(["-c", &format!("Print :{key}"), extension])
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
    };
    let extensions = Command::new("/usr/bin/systemextensionsctl")
        .arg("list")
        .output()
        .ok()
        .filter(|output| output.status.success());
    read_plist("CFBundleShortVersionString").as_deref() == Some("2.4.0")
        && read_plist("CFBundleVersion").as_deref() == Some("17515")
        && extensions.is_some_and(|output| {
            String::from_utf8_lossy(&output.stdout).lines().any(|line| {
                line.trim_start().starts_with('*')
                    && line.contains("Q248YREB53")
                    && line.contains("com.reincubate.macos.cam.avextension (2.4.0/17515)")
                    && line.contains("[activated enabled]")
            })
        })
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
fn selected_media_source_class(kind: &str, label: &str) -> &'static str {
    let output = Command::new("/usr/sbin/system_profiler")
        .args(["SPCameraDataType", "SPAudioDataType", "-json"])
        .output()
        .expect("read independent OS media-device inventory");
    assert!(
        output.status.success(),
        "system_profiler media census failed"
    );
    let inventory: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("parse OS media-device inventory");
    match kind {
        "camera" => {
            let camera = inventory["SPCameraDataType"]
                .as_array()
                .and_then(|devices| {
                    devices
                        .iter()
                        .find(|device| device["_name"].as_str() == Some(label))
                })
                .expect("selected camera label appears in OS device inventory");
            if label == "Camo Camera"
                && camera["spcamera_unique-id"].as_str() == Some("Camo")
                && camo_extension_matches_reviewed_version()
            {
                "os-virtual-camo"
            } else {
                "camera-inventory-matched-unclassified"
            }
        }
        "microphone" => {
            let audio = inventory["SPAudioDataType"]
                .as_array()
                .into_iter()
                .flatten()
                .flat_map(|section| section["_items"].as_array().into_iter().flatten())
                .find(|device| {
                    device["_name"].as_str() == Some(label)
                        && device["coreaudio_device_input"].as_u64().unwrap_or(0) > 0
                })
                .expect("selected microphone label appears as an OS input device");
            match audio["coreaudio_device_transport"].as_str() {
                Some("coreaudio_device_type_builtin") => "physical-builtin",
                Some("coreaudio_device_type_virtual") => "os-virtual",
                _ => "audio-inventory-matched-unclassified",
            }
        }
        _ => panic!("unknown media source kind"),
    }
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
#[test]
fn kel135_macos_media_restart_oracle_rejects_missing_boundaries() {
    let complete = MediaRestartProof {
        seed: MediaSeedProof {
            qualified_capture: true,
            site_prompt_observed: true,
            same_page_repeat_capture: true,
        },
        continuity: MediaContinuityProof {
            clean_restart: true,
            same_signed_identity: true,
            same_store: true,
        },
        denial: MediaDenialProof {
            tcc_authorized: true,
            guarded_callback: true,
            no_capture_or_prompt: true,
        },
        same_origin: true,
        nonce_survived: true,
        allow_capture: true,
    };
    assert!(complete.is_complete());
    let query_only = MediaRestartProof {
        nonce_survived: true,
        ..MediaRestartProof::default()
    };
    assert!(!query_only.is_complete());
    let mut missing_seed = complete;
    missing_seed.seed.qualified_capture = false;
    assert!(!missing_seed.is_complete());
    let mut wrong_store = complete;
    wrong_store.continuity.same_store = false;
    assert!(!wrong_store.is_complete());
    let mut retained_page = complete;
    retained_page.continuity.clean_restart = false;
    assert!(!retained_page.is_complete());
    let mut allow_response = complete;
    allow_response.denial.no_capture_or_prompt = false;
    assert!(!allow_response.is_complete());
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
impl Drop for MacProfilePurgeCleanup {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let cleanup = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut all_purged = true;
            for app in &self.apps {
                let purged = std::panic::catch_unwind(|| {
                    run_signed_purge_report(app, &self.support_root).contains("store_absent=true")
                })
                .unwrap_or(false);
                all_purged &= purged;
            }
            if !all_purged {
                return false;
            }
            fs::remove_dir_all(&self.fixture_root).is_ok()
        }));
        match cleanup {
            Ok(true) => eprintln!(
                "KELD_KEL135_MACOS_MEDIA_FAILURE_CLEANUP exact_purge=true fixture_removed=true"
            ),
            _ => eprintln!(
                "KELD_KEL135_MACOS_MEDIA_FAILURE_CLEANUP incomplete=true retained_root={}",
                self.fixture_root.display()
            ),
        }
    }
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
#[test]
#[ignore = "requires Apple signing, live camera/microphone devices, and operator acceptance of the test site's permission prompt"]
fn kel135_macos_saved_media_grant_is_tested_against_restart_policy() {
    let fixture = ProductFixture::new("kel135-signed-profile-saved-media");
    let stage = fixture.stage();
    let app_root = fixture.root.path().join("signed-apps");
    fs::create_dir(&app_root).expect("create signed fixture parent");
    fs::set_permissions(&app_root, fs::Permissions::from_mode(0o700))
        .expect("protect signed fixture parent");
    let signer = valid_macos_codesign_hashes()
        .into_iter()
        .next()
        .expect("a valid Apple code-signing identity is required");
    let app = build_signed_profile_app(
        stage.root(),
        &app_root,
        "ProfileAppSavedMedia",
        &format!("dev.keld.fixture.profile.media.{}", std::process::id()),
        &signer,
    );
    let identity = run_signed_identity_report(&app);
    let support_root = fixture.root.path().join("application-support");
    fs::create_dir(&support_root).expect("create isolated support root");
    fs::set_permissions(&support_root, fs::Permissions::from_mode(0o700))
        .expect("protect isolated support root");
    let fixture_root = fixture.root.keep();
    let mut cleanup = MacProfilePurgeCleanup {
        apps: vec![app.clone()],
        support_root: support_root.clone(),
        fixture_root: fixture_root.clone(),
        armed: true,
    };
    let mut origin = ProfileOrigin::new();
    let omitted = origin.run_media_profile(
        &app,
        &support_root,
        "media-nonce-omitted",
        "media-nonce-omitted",
        None,
        None,
    );
    assert_eq!(omitted.get("local").map(String::as_str), Some(""));
    assert_eq!(
        omitted.get("media").map(String::as_str),
        Some("not-requested")
    );
    let omitted_log = fs::read_to_string(support_root.join("media-nonce-omitted.log"))
        .expect("read omitted-seed control log");
    assert_store_report_matches(&omitted_log, &identity["store_uuid"]);
    let nonce = format!("keld-kel135-media-{}", std::process::id());
    let mut evidence = Vec::new();
    let mut scenarios = Vec::new();

    for (kind, track) in [("camera", "video"), ("microphone", "audio")] {
        let seeded = origin.run_media_profile(
            &app,
            &support_root,
            &format!("media-seed-{kind}"),
            &format!("media-seed-{kind}"),
            Some(&nonce),
            Some("prompt"),
        );
        assert_eq!(seeded.get("local"), Some(&nonce));
        assert_eq!(
            seeded.get("media").map(String::as_str),
            Some(&format!("resolved-{track}-live")[..]),
            "qualified live {kind} seed"
        );
        assert_eq!(
            seeded.get("media_repeat").map(String::as_str),
            Some(&format!("resolved-{track}-live")[..]),
            "same-page {kind} grant reuse control"
        );
        let selected_label = seeded
            .get("device_label_hex")
            .filter(|label| !label.is_empty())
            .expect("seed capture reported its selected device label");
        let selected_label = media_label_from_hex(selected_label);
        assert_eq!(
            seeded
                .get("repeat_label_hex")
                .map(|label| media_label_from_hex(label)),
            Some(selected_label.clone()),
            "same-page {kind} repeat selected a different device"
        );
        let selected_class = selected_media_source_class(kind, &selected_label);
        let qualified_class = if kind == "camera" {
            "os-virtual-camo"
        } else {
            "physical-builtin"
        };
        assert_eq!(
            selected_class, qualified_class,
            "selected {kind} source requires independent qualification: {selected_label}"
        );
        if kind == "camera" {
            assert_eq!(
                seeded.get("frame_progress").map(String::as_str),
                Some("progressed"),
                "camera seed did not deliver advancing video frames"
            );
            assert_eq!(
                seeded.get("repeat_frame_progress").map(String::as_str),
                Some("progressed"),
                "same-page camera repeat did not deliver advancing video frames"
            );
        }
        assert!(
            media_probe_sheet_count(&seeded) > 0,
            "public Prompt seed did not produce an observable AppKit sheet for {kind}"
        );
        let seed_log = fs::read_to_string(support_root.join(format!("media-seed-{kind}.log")))
            .expect("read saved-media seed callback evidence");
        assert!(
            seed_log.contains(&format!(
                "KELD_KEL135_MEDIA_CALLBACK kind={kind} response=prompt"
            )),
            "seed callback did not defer {kind} to WebKit's user permission prompt"
        );
        let seed_callback_count = seed_log.matches("KELD_KEL135_MEDIA_CALLBACK").count();
        assert_eq!(
            seed_callback_count, 1,
            "same-page {kind} repeat unexpectedly invoked a new delegate decision"
        );
        assert_store_report_matches(&seed_log, &identity["store_uuid"]);

        let denied = origin.run_media_profile(
            &app,
            &support_root,
            &format!("media-deny-{kind}"),
            &format!("media-deny-{kind}"),
            None,
            None,
        );
        assert_eq!(denied.get("local"), Some(&nonce));
        assert_eq!(
            denied.get("media").map(String::as_str),
            Some("error-NotAllowedError"),
            "restarted Keld policy denies after {kind} Allow seed"
        );
        assert!(
            media_probe_tcc_authorized(&denied, kind),
            "signed denying host lacked pre-request {kind} TCC authorization"
        );
        assert_eq!(
            media_probe_sheet_count(&denied),
            0,
            "restarted {kind} denial presented an AppKit permission sheet"
        );
        let deny_log = fs::read_to_string(support_root.join(format!("media-deny-{kind}.log")))
            .expect("read saved-media deny callback evidence");
        assert!(
            deny_log.contains(&format!(
                "KELD_KEL135_MEDIA_CALLBACK kind={kind} response=deny"
            )),
            "Keld's denying callback did not run for saved {kind}"
        );
        assert_eq!(
            deny_log.matches("KELD_KEL135_MEDIA_CALLBACK").count(),
            1,
            "restarted {kind} request did not reach the guarded callback exactly once"
        );
        assert!(
            deny_log.contains("principal=Webview {")
                && deny_log.contains("guard_decision=Some(Deny(")
                && deny_log.contains("policy=PermissionsManifest { app: {} }"),
            "restarted {kind} denial lacks requesting principal or guard decision provenance"
        );
        let allow_phase = format!("media-allow-{kind}");
        let allowed = origin.run_media_profile(
            &app,
            &support_root,
            &allow_phase,
            &allow_phase,
            None,
            Some("allow"),
        );
        assert_eq!(allowed.get("local"), Some(&nonce));
        assert!(
            media_probe_tcc_authorized(&allowed, kind),
            "signed Allow counterfactual host lacked pre-request {kind} TCC authorization"
        );
        assert_eq!(
            allowed.get("media").map(String::as_str),
            Some(&format!("resolved-{track}-live")[..]),
            "fixture Allow must falsify the restarted {kind} denial oracle"
        );
        let allowed_label = allowed
            .get("device_label_hex")
            .filter(|label| !label.is_empty())
            .expect("Allow capture reported its selected device label");
        let allowed_label = media_label_from_hex(allowed_label);
        assert_eq!(
            selected_media_source_class(kind, &allowed_label),
            selected_class,
            "Allow control used a different source class"
        );
        if kind == "camera" {
            assert_eq!(
                allowed.get("frame_progress").map(String::as_str),
                Some("progressed"),
                "Allow control camera capture did not deliver advancing video frames"
            );
        }
        let allow_log = fs::read_to_string(support_root.join(format!("{allow_phase}.log")))
            .expect("read Allow counterfactual log");
        assert_store_report_matches(&allow_log, &identity["store_uuid"]);
        assert!(
            allow_log.contains(&format!(
                "KELD_KEL135_MEDIA_CALLBACK kind={kind} response=allow"
            )) && allow_log.contains("guard_decision=Some(Deny("),
            "Allow counterfactual did not override a real guarded denial"
        );

        let ephemeral = origin.run_ephemeral_media_profile(
            stage.host(),
            &support_root,
            &format!("media-control-{kind}"),
            &format!("media-control-{kind}"),
        );
        assert_eq!(
            ephemeral.get("media").map(String::as_str),
            Some("error-NotAllowedError"),
            "fresh ephemeral {kind} control must deny through Keld's callback"
        );
        let seed_permission = seeded
            .get("permission_after")
            .map_or("missing", String::as_str);
        let restart_permission = denied
            .get("permission_before")
            .map_or("missing", String::as_str);
        let fresh_permission = ephemeral
            .get("permission_before")
            .map_or("missing", String::as_str);
        let control_log =
            fs::read_to_string(support_root.join(format!("media-control-{kind}.log")))
                .expect("read fresh ephemeral media permission control");
        assert_ephemeral_store_report(&control_log);
        assert_store_report_matches(&deny_log, &identity["store_uuid"]);
        evidence.push(format!(
            "{kind}=live-track,seed-label:{selected_label},allow-label:{allowed_label},source-class:{selected_class},seed-after:{seed_permission},restart-before:{restart_permission},fresh-profile-before:{fresh_permission},restart-deny"
        ));
        scenarios.push(MediaScenario {
            kind,
            track,
            seed: MediaRecordedRun {
                report: seeded,
                log: seed_log,
            },
            denied: MediaRecordedRun {
                report: denied,
                log: deny_log,
            },
            allowed: MediaRecordedRun {
                report: allowed,
                log: allow_log,
            },
        });
    }

    let mut wrong_origin = ProfileOrigin::new();
    assert_ne!(wrong_origin.address, origin.address);
    let changed_origin = wrong_origin.run_media_profile(
        &app,
        &support_root,
        "media-nonce-wrong-origin",
        "media-nonce-wrong-origin",
        None,
        None,
    );
    assert_eq!(changed_origin.get("local").map(String::as_str), Some(""));
    let changed_origin_log = fs::read_to_string(support_root.join("media-nonce-wrong-origin.log"))
        .expect("read changed-origin control log");
    assert_store_report_matches(&changed_origin_log, &identity["store_uuid"]);

    let other_app = build_signed_profile_app(
        stage.root(),
        &app_root,
        "ProfileAppOtherMedia",
        &format!(
            "dev.keld.fixture.profile.other-media.{}",
            std::process::id()
        ),
        &signer,
    );
    cleanup.apps.push(other_app.clone());
    let other_identity = run_signed_identity_report(&other_app);
    assert_ne!(other_identity["store_uuid"], identity["store_uuid"]);
    let changed_profile = origin.run_media_profile(
        &other_app,
        &support_root,
        "media-nonce-wrong-profile",
        "media-nonce-wrong-profile",
        None,
        None,
    );
    assert_eq!(changed_profile.get("local").map(String::as_str), Some(""));
    let changed_profile_log =
        fs::read_to_string(support_root.join("media-nonce-wrong-profile.log"))
            .expect("read changed-profile control log");
    assert_store_report_matches(&changed_profile_log, &other_identity["store_uuid"]);

    let omitted = MediaRecordedRun {
        report: omitted,
        log: omitted_log,
    };
    let changed_origin = MediaRecordedRun {
        report: changed_origin,
        log: changed_origin_log,
    };
    let changed_profile = MediaRecordedRun {
        report: changed_profile,
        log: changed_profile_log,
    };

    let other_purge = run_signed_purge_report(&other_app, &support_root);
    assert!(other_purge.contains("store_absent=true"));
    let purge = run_signed_purge_report(&app, &support_root);
    assert!(purge.contains("store_absent=true"));
    cleanup.armed = false;
    fs::remove_dir_all(&fixture_root)
        .expect("remove media fixture after exact WebKit purge verification");
    for scenario in &scenarios {
        let kind = scenario.kind;
        let evaluate = |seed: &MediaRecordedRun, restarted: &MediaRecordedRun| {
            media_restart_proof(
                kind,
                scenario.track,
                &nonce,
                seed,
                restarted,
                &scenario.allowed,
            )
        };
        assert!(
            evaluate(&scenario.seed, &scenario.denied).is_complete(),
            "{kind} restart-denial proof is incomplete"
        );
        let omitted_result = evaluate(&omitted, &scenario.denied);
        assert!(!omitted_result.seed.qualified_capture && !omitted_result.is_complete());
        let changed_origin_result = evaluate(&scenario.seed, &changed_origin);
        assert!(!changed_origin_result.same_origin && !changed_origin_result.is_complete());
        let changed_profile_result = evaluate(&scenario.seed, &changed_profile);
        assert!(
            !changed_profile_result.continuity.same_store && !changed_profile_result.is_complete()
        );
        let retained_page_result = evaluate(&scenario.seed, &scenario.seed);
        assert!(
            !retained_page_result.continuity.clean_restart && !retained_page_result.is_complete()
        );
        let allow_result = evaluate(&scenario.seed, &scenario.allowed);
        assert!(
            !allow_result.denial.no_capture_or_prompt && !allow_result.is_complete(),
            "fixture Allow {kind} capture falsely passed the denial oracle"
        );
    }
    eprintln!(
        "KELD_KEL135_MACOS_MEDIA_SAVED_GRANT macos_media_contract=public-grant-restart-v1 os={} webkit={} team={} identifier={} uuid={} origin={} nonce_store=indexeddb nonce_survived=true seed_decision=WKPermissionDecisionPrompt site_prompt_observed=true post_restart_policy=deny denial_site_prompt_absent=true tcc_status=authorized allow_counterfactual=captured controls=omitted-seed,changed-origin,changed-store,retained-page,allow-response-rejected results={}",
        sw_vers_value("-productVersion"),
        webkit_version(),
        identity["team_id"],
        identity["signing_identifier"],
        identity["store_uuid"],
        origin.address,
        evidence.join(","),
    );
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
fn run_persistent_media_query_modes(
    origin: &mut ProfileOrigin,
    app: &Path,
    support_root: &Path,
    store_uuid: &str,
    nonce: &str,
) -> (Vec<String>, (String, String)) {
    let mut states = Vec::new();
    let mut baseline = None;
    for (policy, seed) in [("deny", Some(nonce)), ("prompt", None), ("allow", None)] {
        let phase = format!("query-{policy}");
        let report = origin.run_media_profile(
            app,
            support_root,
            &phase,
            &phase,
            seed,
            if policy == "deny" { None } else { Some(policy) },
        );
        assert_eq!(
            report.get("local").map(String::as_str),
            Some(nonce),
            "{policy} lost persistent nonce"
        );
        assert_eq!(
            report.get("media").map(String::as_str),
            Some("not-requested")
        );
        let log = fs::read_to_string(support_root.join(format!("{phase}.log")))
            .expect("read signed query-only log");
        assert_store_report_matches(&log, store_uuid);
        assert!(
            !log.contains("KELD_KEL135_MEDIA_CALLBACK"),
            "query-only {policy} unexpectedly invoked a capture permission callback"
        );
        let camera = report.get("camera").expect("camera query result");
        let microphone = report.get("microphone").expect("microphone query result");
        for state in [camera.as_str(), microphone.as_str()] {
            assert!(
                matches!(state, "prompt" | "granted" | "denied"),
                "invalid permission query state: {state}"
            );
        }
        let pair = (camera.clone(), microphone.clone());
        if let Some(expected) = &baseline {
            assert_eq!(&pair, expected, "callback mode changed query state");
        } else {
            baseline = Some(pair);
        }
        states.push(format!("{policy}:camera={camera},microphone={microphone}"));
    }
    (states, baseline.expect("persistent query baseline"))
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
#[test]
#[ignore = "requires a local Apple signing identity and a real signed WKWebView host"]
fn kel135_macos_media_permission_query_is_observed_without_capture() {
    let fixture = ProductFixture::new("kel135-signed-profile-media-query");
    let stage = fixture.stage();
    let app_root = fixture.root.path().join("signed-apps");
    fs::create_dir(&app_root).expect("create signed fixture parent");
    fs::set_permissions(&app_root, fs::Permissions::from_mode(0o700))
        .expect("protect signed fixture parent");
    let signer = valid_macos_codesign_hashes()
        .into_iter()
        .next()
        .expect("a valid Apple code-signing identity is required");
    let app = build_signed_profile_app(
        stage.root(),
        &app_root,
        "ProfileAppMediaQuery",
        &format!(
            "dev.keld.fixture.profile.media-query.{}",
            std::process::id()
        ),
        &signer,
    );
    let identity = run_signed_identity_report(&app);
    let support_root = fixture.root.path().join("application-support");
    fs::create_dir(&support_root).expect("create isolated support root");
    fs::set_permissions(&support_root, fs::Permissions::from_mode(0o700))
        .expect("protect isolated support root");
    let fixture_root = fixture.root.keep();
    let mut cleanup = MacProfilePurgeCleanup {
        apps: vec![app.clone()],
        support_root: support_root.clone(),
        fixture_root: fixture_root.clone(),
        armed: true,
    };
    let mut origin = ProfileOrigin::new();
    let nonce = format!("keld-kel135-query-{}", std::process::id());
    let (mut states, baseline) = run_persistent_media_query_modes(
        &mut origin,
        &app,
        &support_root,
        &identity["store_uuid"],
        &nonce,
    );

    let ephemeral = origin.run_ephemeral_media_profile(
        stage.host(),
        &support_root,
        "query-ephemeral",
        "query-ephemeral",
    );
    assert_eq!(ephemeral.get("local").map(String::as_str), Some(""));
    assert_eq!(
        ephemeral.get("media").map(String::as_str),
        Some("not-requested")
    );
    let ephemeral_log = fs::read_to_string(support_root.join("query-ephemeral.log"))
        .expect("read ephemeral query-only log");
    assert_ephemeral_store_report(&ephemeral_log);
    assert!(
        !ephemeral_log.contains("KELD_KEL135_MEDIA_CALLBACK"),
        "ephemeral query unexpectedly invoked a capture permission callback: {ephemeral_log}"
    );
    let camera = ephemeral
        .get("camera")
        .expect("ephemeral camera query result");
    let microphone = ephemeral
        .get("microphone")
        .expect("ephemeral microphone query result");
    assert_eq!(
        &(camera.clone(), microphone.clone()),
        &baseline,
        "ephemeral and persistent query states differ before any capture request"
    );
    states.push(format!("ephemeral:camera={camera},microphone={microphone}"));
    let purge = run_signed_purge_report(&app, &support_root);
    assert!(purge.contains("store_absent=true"));
    cleanup.armed = false;
    fs::remove_dir_all(&fixture_root).expect("remove query fixture after exact WebKit purge");
    eprintln!(
        "KELD_KEL135_MACOS_MEDIA_QUERY os={} webkit={} team={} identifier={} uuid={} origin={} persistent_nonce=true ephemeral_nonce=false callback_count=0 capture=none states={}",
        sw_vers_value("-productVersion"),
        webkit_version(),
        identity["team_id"],
        identity["signing_identifier"],
        identity["store_uuid"],
        origin.address,
        states.join(","),
    );
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
fn checked_dev_seed_report(
    report: &std::collections::BTreeMap<String, String>,
    log: &str,
    nonce: &str,
    kind: &str,
    track: &str,
) -> (&'static str, String) {
    assert_eq!(report.get("local").map(String::as_str), Some(nonce));
    assert_eq!(report.get("reuse_local").map(String::as_str), Some(nonce));
    assert_eq!(
        report.get("reuse_media").map(String::as_str),
        Some("not-requested")
    );
    let live = format!("resolved-{track}-live");
    for field in ["media", "media_repeat"] {
        assert_eq!(report.get(field).map(String::as_str), Some(live.as_str()));
    }
    if kind == "camera" {
        for field in ["frame_progress", "repeat_frame_progress"] {
            assert_eq!(report.get(field).map(String::as_str), Some("progressed"));
        }
    }
    let label = media_label_from_hex(report.get("device_label_hex").expect("dev device label"));
    assert!(!label.is_empty());
    let source_class = selected_media_source_class(kind, &label);
    assert_eq!(
        source_class,
        if kind == "camera" {
            "os-virtual-camo"
        } else {
            "physical-builtin"
        },
        "dev {kind} selected an unqualified source"
    );
    assert_ephemeral_store_report(log);
    assert!(media_callback_matches(log, kind, "allow"));
    assert_eq!(media_probe_sheet_count(report), 0);
    (source_class, label)
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
fn run_dev_media_kind(
    origin: &mut ProfileOrigin,
    app: &Path,
    support_root: &Path,
    nonce: &str,
    kind: &str,
    track: &str,
) -> String {
    let seed_phase = format!("media-seed-{kind}");
    let seeded = origin.run_profile_executable(
        signed_host_executable(app),
        support_root,
        &seed_phase,
        &format!("dev-{seed_phase}"),
        Some(nonce),
        true,
        None,
        Some("allow-reuse"),
    );
    let seed_log = fs::read_to_string(support_root.join(format!("dev-{seed_phase}.log")))
        .expect("read dev seed log");
    let (source_class, seed_label) =
        checked_dev_seed_report(&seeded, &seed_log, nonce, kind, track);

    let deny_phase = format!("media-deny-{kind}");
    let denied = origin.run_profile_executable(
        signed_host_executable(app),
        support_root,
        &deny_phase,
        &format!("dev-{deny_phase}"),
        None,
        true,
        None,
        None,
    );
    assert_eq!(denied.get("local").map(String::as_str), Some(""));
    assert_eq!(
        denied.get("media").map(String::as_str),
        Some("error-NotAllowedError")
    );
    assert!(media_probe_tcc_authorized(&denied, kind));
    assert_eq!(media_probe_sheet_count(&denied), 0);
    let deny_log = fs::read_to_string(support_root.join(format!("dev-{deny_phase}.log")))
        .expect("read dev denial log");
    assert_ephemeral_store_report(&deny_log);
    assert_eq!(deny_log.matches("KELD_KEL135_MEDIA_CALLBACK").count(), 1);
    assert!(
        deny_log.contains(&format!("kind={kind} response=deny"))
            && deny_log.contains("principal=Webview {")
            && deny_log.contains("guard_decision=Some(Deny(")
            && deny_log.contains("policy=PermissionsManifest { app: {} }")
    );

    let seed_run = MediaRecordedRun {
        report: seeded,
        log: seed_log,
    };
    let denied_run = MediaRecordedRun {
        report: denied,
        log: deny_log,
    };
    assert!(
        dev_media_proof(kind, track, nonce, &seed_run, &denied_run).is_complete(),
        "dev {kind} fresh-launch denial proof is incomplete"
    );
    let mut reused_store_run = seed_run.clone();
    reused_store_run.report.insert(
        String::from("local"),
        seed_run.report["reuse_local"].clone(),
    );
    reused_store_run.report.insert(
        String::from("media"),
        seed_run.report["reuse_media"].clone(),
    );
    let reused_result = dev_media_proof(kind, track, nonce, &seed_run, &reused_store_run);
    assert!(
        !reused_result.fresh.nonce_absent && !reused_result.is_complete(),
        "reusing dev A's nonpersistent store falsely passed the fresh-store oracle"
    );
    let retained_result = dev_media_proof(kind, track, nonce, &seed_run, &seed_run);
    assert!(
        !retained_result.continuity.clean_restart && !retained_result.is_complete(),
        "retaining dev A's granted page falsely passed the lifecycle oracle"
    );
    format!(
        "{kind}=source:{source_class},label:{seed_label},seed-live,second-view-nonce,fresh-deny"
    )
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
#[test]
#[ignore = "requires a local Apple signing identity and live camera/microphone devices"]
fn kel135_macos_dev_media_grants_do_not_survive_fresh_ephemeral_launch() {
    let fixture = ProductFixture::new("kel135-signed-dev-media-ephemeral");
    let stage = fixture.stage();
    let app_root = fixture.root.path().join("signed-apps");
    fs::create_dir(&app_root).expect("create signed dev media fixture parent");
    let signer = valid_macos_codesign_hashes()
        .into_iter()
        .next()
        .expect("a valid Apple code-signing identity is required");
    let app = build_signed_profile_app(
        stage.root(),
        &app_root,
        "ProfileAppDevMedia",
        &format!("dev.keld.fixture.profile.dev-media.{}", std::process::id()),
        &signer,
    );
    let identity = run_signed_identity_report(&app);
    let support_root = fixture.root.path().join("application-support");
    fs::create_dir(&support_root).expect("create isolated support root");
    fs::set_permissions(&support_root, fs::Permissions::from_mode(0o700))
        .expect("protect isolated support root");
    let mut origin = ProfileOrigin::new();
    let nonce = format!("keld-kel135-dev-media-{}", std::process::id());
    let evidence = [("camera", "video"), ("microphone", "audio")]
        .into_iter()
        .map(|(kind, track)| {
            run_dev_media_kind(&mut origin, &app, &support_root, &nonce, kind, track)
        })
        .collect::<Vec<_>>();
    eprintln!(
        "KELD_KEL135_MACOS_DEV_MEDIA macos_media_contract=public-grant-restart-v1 os={} webkit={} team={} identifier={} origin={} nonce_store=indexeddb seed_store=ephemeral second_view_store=reused fresh_store=ephemeral fresh_nonce_absent=true fresh_denial=guarded/no-track/no-sheet controls=reused-store,retained-page results={}",
        sw_vers_value("-productVersion"),
        webkit_version(),
        identity["team_id"],
        identity["signing_identifier"],
        origin.address,
        evidence.join(","),
    );
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
#[test]
#[ignore = "requires local Apple code-signing identities and a real signed host fixture"]
fn kel135_macos_binding_recovers_after_process_crash() {
    let fixture = ProductFixture::new("kel135-signed-profile-binding-recovery");
    let stage = fixture.stage();
    let app_root = fixture.root.path().join("signed-apps");
    fs::create_dir(&app_root).expect("create signed fixture parent");
    fs::set_permissions(&app_root, fs::Permissions::from_mode(0o700))
        .expect("protect signed fixture parent");
    let signer = valid_macos_codesign_hashes()
        .into_iter()
        .next()
        .expect("a valid Apple code-signing identity is required");
    let app = build_signed_profile_app(
        stage.root(),
        &app_root,
        "ProfileAppBindingRecovery",
        &format!("dev.keld.fixture.profile.binding.{}", std::process::id()),
        &signer,
    );
    let identity = run_signed_identity_report(&app);
    let support_root = fixture.root.path().join("application-support");
    fs::create_dir(&support_root).expect("create isolated support root");
    fs::set_permissions(&support_root, fs::Permissions::from_mode(0o700))
        .expect("protect isolated support root");
    let interrupted = Command::new(signed_host_executable(&app))
        .arg("--keld-profile-webview-fixture-v1")
        .env("KELD_PROFILE_TEST_ROOT", &support_root)
        .env("KELD_PROFILE_TEST_BINDING_CRASH_AFTER_REVERSE", "1")
        .env("KELD_PROFILE_FIXTURE_URL", "http://127.0.0.1:1/unused")
        .stdin(Stdio::null())
        .output()
        .expect("launch binding interruption fixture");
    assert_eq!(interrupted.status.code(), Some(87));
    assert!(
        String::from_utf8_lossy(&interrupted.stderr).contains("after_reverse_record=true"),
        "binding interruption did not follow a durable reverse record"
    );

    let mut origin = ProfileOrigin::new();
    let seeded = origin.run_profile(
        &app,
        &support_root,
        "seed",
        "binding-recovered-seed",
        Some("keld-kel135-binding-recovered"),
    );
    assert_eq!(
        seeded.get("local").map(String::as_str),
        Some("keld-kel135-binding-recovered")
    );
    let report = fs::read_to_string(support_root.join("binding-recovered-seed.log"))
        .expect("read recovered binding store report");
    assert_store_report_matches(&report, &identity["store_uuid"]);
    let purge = run_signed_purge_report(&app, &support_root);
    assert!(purge.contains("store_absent=true"));
    let mut recovered_phases = vec!["after-reverse-record"];
    for (phase, expected_status) in [("store-created", 88), ("store-verified", 89)] {
        let phase_app = build_signed_profile_app(
            stage.root(),
            &app_root,
            &format!("ProfileAppBindingRecovery-{phase}"),
            &format!(
                "dev.keld.fixture.profile.binding.{}.{}",
                std::process::id(),
                phase
            ),
            &signer,
        );
        let phase_identity = run_signed_identity_report(&phase_app);
        let phase_support = support_root.join(phase);
        fs::create_dir(&phase_support).expect("create binding-recovery support root");
        fs::set_permissions(&phase_support, fs::Permissions::from_mode(0o700))
            .expect("protect binding-recovery support root");
        let interrupted = Command::new(signed_host_executable(&phase_app))
            .arg("--keld-profile-webview-fixture-v1")
            .env("KELD_PROFILE_TEST_ROOT", &phase_support)
            .env("KELD_PROFILE_TEST_BINDING_CRASH_AFTER", phase)
            .env("KELD_PROFILE_FIXTURE_URL", "http://127.0.0.1:1/unused")
            .stdin(Stdio::null())
            .output()
            .expect("launch binding phase interruption fixture");
        assert_eq!(interrupted.status.code(), Some(expected_status));
        assert!(
            String::from_utf8_lossy(&interrupted.stderr).contains(&format!("after={phase}")),
            "binding interruption missed {phase}"
        );
        let phase_state = origin.run_profile_after_test_boot_change(
            &phase_app,
            &phase_support,
            "seed",
            "binding-phase-recovered",
            Some("keld-kel135-binding-phase"),
        );
        assert_eq!(
            phase_state.get("local").map(String::as_str),
            Some("keld-kel135-binding-phase"),
            "binding recovery after {phase}"
        );
        let report = fs::read_to_string(phase_support.join("binding-phase-recovered.log"))
            .expect("read recovered phase store report");
        assert_store_report_matches(&report, &phase_identity["store_uuid"]);
        assert!(run_signed_purge_report(&phase_app, &phase_support).contains("store_absent=true"));
        recovered_phases.push(phase);
    }
    eprintln!(
        "KELD_KEL135_MACOS_BINDING_RECOVERY os={} webkit={} team={} identifier={} uuid={} crash_phases={} result=recovered-and-opened purge=exact-identity",
        sw_vers_value("-productVersion"),
        webkit_version(),
        identity["team_id"],
        identity["signing_identifier"],
        identity["store_uuid"],
        recovered_phases.join(","),
    );
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
#[test]
#[ignore = "two-phase real reboot test; run prepare, restart macOS, then resume"]
fn kel135_macos_crash_quarantine_recovers_only_after_real_reboot() {
    assert!(
        std::env::var_os("KELD_PROFILE_TEST_BOOT_UUID").is_none(),
        "real reboot acceptance cannot use the synthetic boot UUID hook"
    );
    let phase = std::env::var("KELD_KEL135_REBOOT_PHASE")
        .expect("set KELD_KEL135_REBOOT_PHASE to prepare or resume");
    let run_root = PathBuf::from(
        std::env::var_os("KELD_KEL135_REBOOT_ROOT")
            .expect("set KELD_KEL135_REBOOT_ROOT below the persistent TMPDIR"),
    );
    let temp_root = std::env::temp_dir()
        .canonicalize()
        .expect("canonicalize configured TMPDIR");
    assert!(
        run_root.is_absolute(),
        "reboot fixture root must be absolute"
    );
    if phase == "prepare" {
        fs::create_dir(&run_root).expect("create durable reboot fixture root once");
        fs::set_permissions(&run_root, fs::Permissions::from_mode(0o700))
            .expect("protect durable reboot fixture root");
    } else {
        assert_eq!(phase, "resume", "unknown reboot fixture phase");
        assert!(
            run_root.is_dir(),
            "resume requires the preserved prepare root"
        );
    }
    let canonical_root = run_root.canonicalize().expect("canonicalize reboot root");
    assert!(canonical_root.starts_with(temp_root));
    let manifest_path = canonical_root.join("manifest.json");
    let support_root = canonical_root.join("application-support");
    let project_root = canonical_root.join("project");
    let mut fixture = ProductFixture::new("kel135-reboot-runner");
    fixture.project = project_root.clone();
    fs::create_dir_all(project_root.join("src"))
        .expect("create persistent reboot fixture project and entry directory");
    let stage = fixture.stage();

    if phase == "prepare" {
        fs::create_dir(&support_root).expect("create persistent profile metadata root");
        fs::set_permissions(&support_root, fs::Permissions::from_mode(0o700))
            .expect("protect persistent profile metadata root");
        let app_root = canonical_root.join("signed-apps");
        fs::create_dir(&app_root).expect("create durable signed fixture parent");
        fs::set_permissions(&app_root, fs::Permissions::from_mode(0o700))
            .expect("protect durable signed fixture parent");
        let signer = valid_macos_codesign_hashes()
            .into_iter()
            .next()
            .expect("an Apple Development signing identity is required");
        let bundle_id = format!("dev.keld.fixture.profile.reboot.{}", std::process::id());
        let app = build_signed_profile_app(
            stage.root(),
            &app_root,
            "ProfileAppRebootRecovery",
            &bundle_id,
            &signer,
        );
        let identity = run_signed_identity_report(&app);
        let os_boot_before = system_boot_uuid_hex();
        let mut origin = ProfileOrigin::new();
        let nonce = String::from("keld-kel135-reboot-preserved-state");
        let mut owner = spawn_profile_host(
            signed_host_executable(&app),
            &support_root,
            &origin.address,
            "seed",
            "reboot-crash-owner",
        );
        let seeded = origin
            .wait_for_report("seed", Some(&nonce))
            .expect("signed profile owner rendered its seed page");
        for key in ["local", "cookie", "idb", "cache"] {
            assert_eq!(seeded.get(key).map(String::as_str), Some(nonce.as_str()));
        }
        assert_eq!(seeded.get("sw").map(String::as_str), Some("true"));
        let owner_log = fs::read_to_string(support_root.join("reboot-crash-owner.log"))
            .expect("read active owner store evidence");
        assert_store_report_matches(&owner_log, &identity["store_uuid"]);
        let prior_boot = boot_uuid_from_log(&owner_log);
        assert!(
            prior_boot == os_boot_before,
            "Keld boot readback must equal the independent sysctl oracle"
        );
        let crashed = kill_profile_host(&mut owner);
        assert_eq!(crashed.signal(), Some(9), "owner must terminate by SIGKILL");

        let same_boot = Command::new(signed_host_executable(&app))
            .arg("--keld-profile-webview-fixture-v1")
            .env("KELD_PROFILE_TEST_ROOT", &support_root)
            .env("KELD_PROFILE_ACCEPTANCE_REPORT", "1")
            .env_remove("KELD_PROFILE_TEST_BOOT_UUID")
            .env(
                "KELD_PROFILE_FIXTURE_URL",
                format!("http://{}/unused", origin.address),
            )
            .stdin(Stdio::null())
            .output()
            .expect("launch same-boot quarantine negative control");
        assert!(!same_boot.status.success());
        let same_boot_text = String::from_utf8_lossy(&same_boot.stderr);
        assert!(
            same_boot_text.contains("recovery state cannot be proven"),
            "same-boot quarantine must report unproven recovery"
        );
        assert!(
            same_boot_text.contains("startup-resource-attempts listener=0 child=0 window=0"),
            "same-boot quarantine must fail before app resources"
        );

        let manifest = std::collections::BTreeMap::from([
            (String::from("app"), app.display().to_string()),
            (
                String::from("support_root"),
                support_root.display().to_string(),
            ),
            (String::from("origin"), origin.address.clone()),
            (String::from("team_id"), identity["team_id"].clone()),
            (
                String::from("signing_identifier"),
                identity["signing_identifier"].clone(),
            ),
            (
                String::from("profile_identity"),
                identity["profile_identity"].clone(),
            ),
            (String::from("store_uuid"), identity["store_uuid"].clone()),
            (String::from("nonce"), nonce),
            (String::from("boot_uuid_hex"), prior_boot.clone()),
        ]);
        let encoded = serde_json::to_vec(&manifest).expect("encode durable reboot manifest");
        fs::write(&manifest_path, encoded).expect("persist reboot resume manifest");
        fs::set_permissions(&manifest_path, fs::Permissions::from_mode(0o600))
            .expect("protect reboot resume manifest");
        eprintln!(
            "KELD_KEL135_MACOS_REBOOT_PREPARED os={} team={} identifier={} uuid={} boot_identity_recorded=true same_boot=quarantined owner_exit=SIGKILL root={} next=physically-restart-macOS",
            sw_vers_value("-productVersion"),
            identity["team_id"],
            identity["signing_identifier"],
            identity["store_uuid"],
            canonical_root.display(),
        );
        return;
    }

    let manifest: std::collections::BTreeMap<String, String> =
        serde_json::from_slice(&fs::read(&manifest_path).expect("read prepare manifest"))
            .expect("decode prepare manifest");
    let app = PathBuf::from(&manifest["app"]);
    let support_root = PathBuf::from(&manifest["support_root"]);
    let signer = valid_macos_codesign_hashes()
        .into_iter()
        .next()
        .expect("an Apple Development signing identity is required");
    refresh_signed_profile_app(stage.root(), &app, &signer);
    let identity = run_signed_identity_report(&app);
    let os_boot_after = system_boot_uuid_hex();
    let os_boot_time = system_boot_time_seconds();
    for key in [
        "team_id",
        "signing_identifier",
        "profile_identity",
        "store_uuid",
    ] {
        assert_eq!(
            identity[key], manifest[key],
            "stable reboot identity field {key}"
        );
    }
    let mut origin = ProfileOrigin::bind(&manifest["origin"]);
    let state = origin.run_profile(&app, &support_root, "read", "reboot-recovered-read", None);
    for key in ["local", "cookie", "idb", "cache"] {
        assert_eq!(
            state.get(key).map(String::as_str),
            Some(manifest["nonce"].as_str())
        );
    }
    assert_eq!(state.get("sw").map(String::as_str), Some("true"));
    let report = fs::read_to_string(support_root.join("reboot-recovered-read.log"))
        .expect("read post-reboot selected store report");
    assert_store_report_matches(&report, &manifest["store_uuid"]);
    let next_boot = boot_uuid_from_log(&report);
    assert!(
        next_boot == os_boot_after,
        "Keld boot readback must equal the independent sysctl oracle"
    );
    let previous_boot_evidence = if let Some(previous_boot) = manifest.get("boot_uuid_hex") {
        assert!(
            next_boot != *previous_boot,
            "recovered boot must differ from the retained pre-reboot boot"
        );
        "uuid-distinct"
    } else {
        let owner_log_mtime = fs::metadata(support_root.join("reboot-crash-owner.log"))
            .expect("read pre-reboot owner log metadata")
            .modified()
            .expect("read pre-reboot owner log timestamp")
            .duration_since(std::time::UNIX_EPOCH)
            .expect("owner log timestamp follows UNIX epoch")
            .as_secs();
        assert!(
            os_boot_time > owner_log_mtime,
            "independent kern.boottime must postdate the SIGKILL owner log"
        );
        "legacy-prepare-omitted-uuid-boot-time-proven"
    };
    let purge = run_signed_purge_report(&app, &support_root);
    assert!(purge.contains("store_absent=true"));
    drop(origin);
    eprintln!(
        "KELD_KEL135_MACOS_REBOOT_RECOVERY os={} webkit={} team={} identifier={} uuid={} boot_uuid_matches_sysctl=true previous_boot_evidence={} kern_boottime_epoch={} boot_transition=real-reboot same_boot_quarantine=passed state=all-five-preserved purge=exact-identity",
        sw_vers_value("-productVersion"),
        webkit_version(),
        identity["team_id"],
        identity["signing_identifier"],
        identity["store_uuid"],
        previous_boot_evidence,
        os_boot_time,
    );
    fs::remove_dir_all(&canonical_root).expect("remove completed isolated reboot fixture root");
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
fn valid_macos_codesign_hashes() -> Vec<String> {
    let output = Command::new("/usr/bin/security")
        .args(["find-identity", "-v", "-p", "codesigning"])
        .output()
        .expect("query local code-signing identities");
    assert!(
        output.status.success(),
        "security find-identity failed: {output:?}"
    );
    String::from_utf8(output.stdout)
        .expect("valid signing identity output is UTF-8")
        .lines()
        .filter(|line| !line.contains("CSSMERR_"))
        .filter_map(|line| {
            let rest = line.split_once(") ")?.1;
            let candidate = rest.split_whitespace().next()?;
            (candidate.len() == 40
                && candidate
                    .as_bytes()
                    .iter()
                    .all(|byte| byte.is_ascii_hexdigit()))
            .then(|| candidate.to_owned())
        })
        .collect()
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
fn build_signed_profile_app(
    stage_root: &Path,
    app_parent: &Path,
    app_name: &str,
    bundle_id: &str,
    signer: &str,
) -> PathBuf {
    let app = app_parent.join(format!("{app_name}.app"));
    fs::create_dir(&app).expect("create signed host fixture directory");
    fs::set_permissions(&app, fs::Permissions::from_mode(0o700))
        .expect("protect signed host fixture directory");
    let contents = app.join("Contents");
    let macos = contents.join("MacOS");
    fs::create_dir_all(&macos).expect("create signed app executable directory");
    fs::set_permissions(&contents, fs::Permissions::from_mode(0o700))
        .expect("protect signed app contents directory");
    fs::set_permissions(&macos, fs::Permissions::from_mode(0o700))
        .expect("protect signed app executable directory");
    fs::copy(stage_root.join("keld-host"), macos.join("keld-host"))
        .expect("copy KEL-135 host executable into app bundle");
    let signed_executable = signed_host_executable(&app);
    fs::set_permissions(&signed_executable, fs::Permissions::from_mode(0o700))
        .expect("make signed app executable runnable");
    let info = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\"><dict><key>CFBundleExecutable</key><string>keld-host</string><key>CFBundleIdentifier</key><string>{bundle_id}</string><key>CFBundleName</key><string>{app_name}</string><key>CFBundlePackageType</key><string>APPL</string><key>CFBundleVersion</key><string>1</string><key>CFBundleShortVersionString</key><string>1.0</string><key>NSCameraUsageDescription</key><string>KEL-135 acceptance fixture validates saved camera grant isolation.</string><key>NSMicrophoneUsageDescription</key><string>KEL-135 acceptance fixture validates saved microphone grant isolation.</string></dict></plist>"
    );
    fs::write(contents.join("Info.plist"), info).expect("write signed app bundle metadata");
    let signature = Command::new("/usr/bin/codesign")
        .args(["--force", "--deep", "--sign", signer, "--timestamp=none"])
        .arg("--identifier")
        .arg(bundle_id)
        .arg(&app)
        .output()
        .expect("sign KEL-135 host fixture app");
    assert!(signature.status.success(), "codesign failed: {signature:?}");
    let verification = Command::new("/usr/bin/codesign")
        .args(["--verify", "--deep", "--strict"])
        .arg(&app)
        .output()
        .expect("verify KEL-135 host app");
    assert!(
        verification.status.success(),
        "signed host executable did not verify: {verification:?}"
    );
    app
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
fn signed_host_executable(app: &Path) -> PathBuf {
    app.join("Contents/MacOS/keld-host")
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
fn signed_media_executable_facts(executable: &Path) -> Option<(String, String)> {
    let bundle = executable
        .ancestors()
        .find(|path| path.extension().is_some_and(|extension| extension == "app"))?;
    let verified = Command::new("/usr/bin/codesign")
        .args(["--verify", "--deep", "--strict"])
        .arg(bundle)
        .output()
        .expect("verify signed media fixture bundle before launch");
    assert!(
        verified.status.success(),
        "signed media fixture lost validity"
    );
    let digest = Command::new("/usr/bin/shasum")
        .args(["-a", "256"])
        .arg(executable)
        .output()
        .expect("hash signed media fixture executable");
    assert!(digest.status.success(), "signed executable hash failed");
    let sha256 = String::from_utf8_lossy(&digest.stdout)
        .split_whitespace()
        .next()
        .expect("signed executable SHA-256")
        .to_owned();
    let details = Command::new("/usr/bin/codesign")
        .args(["-d", "--verbose=4"])
        .arg(bundle)
        .output()
        .expect("read signed media fixture CDHash");
    assert!(details.status.success(), "signed media CDHash read failed");
    let cdhash = String::from_utf8_lossy(&details.stderr)
        .lines()
        .find_map(|line| line.strip_prefix("CDHash="))
        .expect("signed media fixture CDHash")
        .to_owned();
    Some((sha256, cdhash))
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
fn configure_profile_media_mode(
    command: &mut Command,
    mode: Option<&str>,
    phase: &str,
    address: &str,
    log_name: &str,
) {
    match mode {
        Some("allow" | "allow-reuse") => {
            command.env("KELD_PROFILE_TEST_MEDIA_SEED_ALLOW", "1");
            if mode == Some("allow-reuse") {
                eprintln!(
                    "KELD_KEL135_MEDIA_ALLOW_READY phase={phase} action=if-macOS-TCC-prompts-click-Allow deadline_seconds=120"
                );
            }
        }
        Some("prompt" | "prompt-reuse") => {
            if let Some(kind) = phase.strip_prefix("media-seed-") {
                eprintln!(
                    "KELD_KEL135_MEDIA_PROMPT_READY kind={kind} action=click-Allow-in-profile-fixture-window deadline_seconds=120"
                );
            } else {
                assert!(
                    phase.starts_with("query-"),
                    "unexpected prompt fixture phase"
                );
            }
            command
                .env("KELD_PROFILE_TEST_MEDIA_SEED_ALLOW", "1")
                .env("KELD_PROFILE_TEST_MEDIA_SEED_PROMPT", "1");
        }
        None => {}
        Some(_) => panic!("unknown KEL-135 media fixture mode"),
    }
    if matches!(mode, Some("prompt-reuse" | "allow-reuse")) {
        command.env(
            "KELD_PROFILE_FIXTURE_SECOND_URL",
            format!("http://{address}/media-nonce-reuse?run={log_name}"),
        );
    }
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
fn attach_profile_run_evidence(
    report: &mut std::collections::BTreeMap<String, String>,
    output: &str,
    phase: &str,
    executable: &Path,
    signed_facts: Option<(String, String)>,
) {
    if let Some((sha256, cdhash)) = signed_facts {
        assert_eq!(
            signed_media_executable_facts(executable),
            Some((sha256.clone(), cdhash.clone())),
            "signed media executable changed during its fixture launch"
        );
        report.insert(String::from("host_sha256"), sha256.clone());
        report.insert(String::from("host_cdhash"), cdhash.clone());
        let identity = output
            .lines()
            .find(|line| line.starts_with("KELD_KEL135_SIGNED_IDENTITY "))
            .expect("current signed media host emitted its validated identity");
        for field in identity.split_whitespace().skip(1) {
            if let Some((key, value)) = field.split_once('=') {
                report.insert(format!("signed_{key}"), value.to_owned());
            }
        }
        eprintln!(
            "KELD_KEL135_SIGNED_MEDIA_RUN phase={phase} sha256={sha256} cdhash={cdhash} {identity}"
        );
    }
    let store = output
        .lines()
        .find(|line| line.starts_with("KELD_KEL135_STORE "))
        .expect("read selected WebKit store report");
    for field in store.split_whitespace().skip(1) {
        if let Some((key, value)) = field.split_once('=') {
            report.insert(format!("store_{key}"), value.to_owned());
        }
    }
    if phase.starts_with("media-") || phase.starts_with("query-") {
        let probe = output
            .lines()
            .find(|line| line.starts_with("KELD_KEL135_MEDIA_PROBE "))
            .expect("read signed-host AppKit/TCC media probe result");
        for field in probe.split_whitespace().skip(1) {
            if let Some((key, value)) = field.split_once('=') {
                report.insert(format!("probe_{key}"), value.to_owned());
            }
        }
        eprintln!("KELD_KEL135_MEDIA_PROBE_RESULT phase={phase} {probe}");
    }
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
fn refresh_signed_profile_app(stage_root: &Path, app: &Path, signer: &str) {
    let executable = signed_host_executable(app);
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o700))
        .expect("make signed reboot fixture executable replaceable");
    fs::copy(stage_root.join("keld-host"), &executable)
        .expect("update reboot fixture executable with the current Keld build");
    let signature = Command::new("/usr/bin/codesign")
        .args(["--force", "--deep", "--sign", signer, "--timestamp=none"])
        .arg(app)
        .output()
        .expect("re-sign reboot fixture with its same code identity");
    assert!(
        signature.status.success(),
        "re-sign fixture failed: {signature:?}"
    );
    let verification = Command::new("/usr/bin/codesign")
        .args(["--verify", "--deep", "--strict"])
        .arg(app)
        .output()
        .expect("verify refreshed reboot fixture signature");
    assert!(
        verification.status.success(),
        "refreshed fixture signature failed: {verification:?}"
    );
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
struct ProfileOrigin {
    listener: TcpListener,
    address: String,
    pending_reports: std::collections::BTreeMap<String, std::collections::BTreeMap<String, String>>,
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
fn spawn_profile_host(
    executable: PathBuf,
    support_root: &Path,
    address: &str,
    phase: &str,
    log_name: &str,
) -> Child {
    let log = fs::File::create(support_root.join(format!("{log_name}.log")))
        .expect("create profile evidence log");
    let stderr = log.try_clone().expect("clone evidence log");
    Command::new(executable)
        .arg("--keld-profile-webview-fixture-v1")
        .env("KELD_PROFILE_TEST_ROOT", support_root)
        .env("KELD_PROFILE_ACCEPTANCE_REPORT", "1")
        .env_remove("KELD_PROFILE_TEST_BOOT_UUID")
        .env_remove("KELD_PROFILE_FIXTURE_FATAL_ON_STDIN")
        .env(
            "KELD_PROFILE_FIXTURE_URL",
            format!("http://{address}/{phase}?run={log_name}"),
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(stderr))
        .spawn()
        .expect("launch signed profile owner")
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
fn stop_profile_host(child: &mut Child) {
    drop(child.stdin.take());
    let deadline = Instant::now() + PROCESS_DEADLINE;
    loop {
        match child.try_wait().expect("wait for signed profile owner") {
            Some(status) => {
                assert!(status.success(), "signed profile owner failed: {status}");
                return;
            }
            None if Instant::now() < deadline => thread::sleep(Duration::from_millis(20)),
            None => {
                let _ = child.kill();
                let status = child.wait().expect("reap stuck signed profile owner");
                panic!("signed profile owner did not stop cleanly: {status}");
            }
        }
    }
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
fn kill_profile_host(child: &mut Child) -> std::process::ExitStatus {
    child.kill().expect("SIGKILL active profile host");
    child.wait().expect("reap crashed profile host")
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
impl ProfileOrigin {
    fn new() -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind KEL-135 local origin");
        listener
            .set_nonblocking(true)
            .expect("make KEL-135 origin pollable");
        let address = listener
            .local_addr()
            .expect("local origin address")
            .to_string();
        Self {
            listener,
            address,
            pending_reports: std::collections::BTreeMap::new(),
        }
    }

    fn bind(address: &str) -> Self {
        let listener = TcpListener::bind(address)
            .expect("rebind the exact loopback origin saved before reboot");
        listener
            .set_nonblocking(true)
            .expect("make reboot origin pollable");
        let address = listener
            .local_addr()
            .expect("rebound local origin address")
            .to_string();
        Self {
            listener,
            address,
            pending_reports: std::collections::BTreeMap::new(),
        }
    }

    fn run_profile(
        &mut self,
        app: &Path,
        support_root: &Path,
        phase: &str,
        log_name: &str,
        seed: Option<&str>,
    ) -> std::collections::BTreeMap<String, String> {
        self.run_profile_executable(
            signed_host_executable(app),
            support_root,
            phase,
            log_name,
            seed,
            false,
            None,
            None,
        )
    }

    fn run_profile_as_user(
        &mut self,
        username: &str,
        user_temp: &Path,
        profile_root: &Path,
        app: &Path,
        phase: &str,
        log_name: &str,
        seed: Option<&str>,
    ) -> (std::collections::BTreeMap<String, String>, Output) {
        let url = format!("http://{}/{phase}?run={log_name}", self.address);
        let executable = signed_host_executable(app);
        let environment = [
            ("TMPDIR", user_temp.to_string_lossy().into_owned()),
            (
                "KELD_PROFILE_TEST_ROOT",
                profile_root.to_string_lossy().into_owned(),
            ),
            ("KELD_PROFILE_ACCEPTANCE_REPORT", String::from("1")),
            ("KELD_PROFILE_FIXTURE_URL", url),
        ];
        let mut child = run_as_local_user_command(
            username,
            executable.as_os_str(),
            &["--keld-profile-webview-fixture-v1"],
            &environment,
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("launch signed profile host as second standard user");
        let Some(report) = self.wait_for_report(phase, seed) else {
            if let Some(status) = child.try_wait().expect("inspect second-user profile host") {
                let _output = wait_child_output(child, PROCESS_DEADLINE);
                panic!(
                    "second-user host exited before the browser report (status={status}); private output suppressed"
                );
            }
            let _ = child.kill();
            let output = wait_child_output(child, PROCESS_DEADLINE);
            panic!(
                "second-user host did not report {phase} state (status={}); private output suppressed",
                output.status
            );
        };
        drop(child.stdin.take());
        let output = wait_child_output(child, PROCESS_DEADLINE);
        assert!(
            output.status.success(),
            "second-user profile host failed (status={})",
            output.status
        );
        (report, output)
    }

    fn run_ephemeral_profile(
        &mut self,
        executable: &Path,
        support_root: &Path,
        phase: &str,
        log_name: &str,
        seed: Option<&str>,
    ) -> std::collections::BTreeMap<String, String> {
        self.run_profile_executable(
            executable.to_owned(),
            support_root,
            phase,
            log_name,
            seed,
            true,
            None,
            None,
        )
    }

    fn run_profile_after_test_boot_change(
        &mut self,
        app: &Path,
        support_root: &Path,
        phase: &str,
        log_name: &str,
        seed: Option<&str>,
    ) -> std::collections::BTreeMap<String, String> {
        self.run_profile_executable(
            signed_host_executable(app),
            support_root,
            phase,
            log_name,
            seed,
            false,
            Some("11111111-2222-4333-8444-555555555555"),
            None,
        )
    }

    fn run_media_profile(
        &mut self,
        app: &Path,
        support_root: &Path,
        phase: &str,
        log_name: &str,
        seed: Option<&str>,
        media_mode: Option<&str>,
    ) -> std::collections::BTreeMap<String, String> {
        self.run_profile_executable(
            signed_host_executable(app),
            support_root,
            phase,
            log_name,
            seed,
            false,
            None,
            media_mode,
        )
    }

    fn run_ephemeral_media_profile(
        &mut self,
        executable: &Path,
        support_root: &Path,
        phase: &str,
        log_name: &str,
    ) -> std::collections::BTreeMap<String, String> {
        self.run_profile_executable(
            executable.to_owned(),
            support_root,
            phase,
            log_name,
            None,
            true,
            None,
            None,
        )
    }

    fn run_profile_executable(
        &mut self,
        executable: PathBuf,
        support_root: &Path,
        phase: &str,
        log_name: &str,
        seed: Option<&str>,
        dev_ephemeral: bool,
        boot_uuid: Option<&str>,
        media_mode: Option<&str>,
    ) -> std::collections::BTreeMap<String, String> {
        let log_path = support_root.join(format!("{log_name}.log"));
        let log = fs::File::create(&log_path).expect("create profile evidence log");
        let stderr = log.try_clone().expect("clone evidence log");
        let url = format!("http://{}/{phase}?run={log_name}", self.address);
        let executable_path = executable.display().to_string();
        let signed_facts = if phase.starts_with("media-") || phase.starts_with("query-") {
            signed_media_executable_facts(&executable)
        } else {
            None
        };
        let executable_for_check = executable.clone();
        let mut command = Command::new(executable);
        command
            .arg("--keld-profile-webview-fixture-v1")
            .env("KELD_PROFILE_TEST_ROOT", support_root)
            .env("KELD_PROFILE_ACCEPTANCE_REPORT", "1")
            .env_remove("KELD_PROFILE_TEST_BOOT_UUID")
            .env_remove("KELD_PROFILE_TEST_MEDIA_SEED_ALLOW")
            .env_remove("KELD_PROFILE_TEST_MEDIA_SEED_PROMPT")
            .env_remove("KELD_PROFILE_FIXTURE_EPHEMERAL")
            .env_remove("KELD_PROFILE_FIXTURE_SECOND_URL")
            .env_remove("KELD_PROFILE_FIXTURE_SIGNED_ATTEST")
            .env_remove("KELD_PROFILE_FIXTURE_FATAL_ON_STDIN")
            .env("KELD_PROFILE_FIXTURE_URL", &url)
            .stdin(Stdio::piped())
            .stdout(Stdio::from(log))
            .stderr(Stdio::from(stderr));
        if signed_facts.is_some() {
            command.env("KELD_PROFILE_FIXTURE_SIGNED_ATTEST", "1");
        }
        if dev_ephemeral {
            command.env("KELD_PROFILE_FIXTURE_EPHEMERAL", "1");
        }
        if let Some(boot_uuid) = boot_uuid {
            command.env("KELD_PROFILE_TEST_BOOT_UUID", boot_uuid);
        }
        configure_profile_media_mode(&mut command, media_mode, phase, &self.address, log_name);
        let mut child = command.spawn().expect("launch signed KEL-135 profile host");
        let host_pid = child.id().to_string();
        let report_deadline =
            if matches!(media_mode, Some("prompt" | "prompt-reuse" | "allow-reuse"))
                && phase.starts_with("media-seed-")
            {
                MEDIA_PROMPT_DEADLINE
            } else {
                EVENT_DEADLINE
            };
        let Some(mut report) = self.wait_for_report_with_timeout(phase, seed, report_deadline)
        else {
            if let Some(status) = child.try_wait().expect("inspect failed profile host") {
                panic!(
                    "signed profile host exited before browser report (status={status}); private log retained for inspection"
                );
            }
            let _ = child.kill();
            let status = child.wait().expect("reap timed-out signed profile host");
            panic!(
                "signed profile host did not report browser state (phase={phase}, status={status}); private log retained for inspection"
            );
        };
        if matches!(media_mode, Some("prompt-reuse" | "allow-reuse")) {
            let reuse = self
                .wait_for_report_with_timeout("media-nonce-reuse", None, EVENT_DEADLINE)
                .expect("second view reported same-store nonce without capture");
            report.insert(
                String::from("reuse_local"),
                reuse.get("local").cloned().unwrap_or_default(),
            );
            report.insert(
                String::from("reuse_media"),
                reuse.get("media").cloned().unwrap_or_default(),
            );
        }
        stop_profile_host(&mut child);
        report.insert(String::from("host_pid"), host_pid);
        report.insert(String::from("host_executable"), executable_path);
        report.insert(String::from("host_clean_exit"), String::from("true"));
        report.insert(String::from("origin"), self.address.clone());
        let output = fs::read_to_string(&log_path).expect("read signed profile host log");
        attach_profile_run_evidence(
            &mut report,
            &output,
            phase,
            &executable_for_check,
            signed_facts,
        );
        report
    }

    fn wait_for_report(
        &mut self,
        phase: &str,
        seed: Option<&str>,
    ) -> Option<std::collections::BTreeMap<String, String>> {
        self.wait_for_report_with_timeout(phase, seed, EVENT_DEADLINE)
    }

    fn wait_for_report_with_timeout(
        &mut self,
        phase: &str,
        seed: Option<&str>,
        timeout: Duration,
    ) -> Option<std::collections::BTreeMap<String, String>> {
        if let Some(report) = self.pending_reports.remove(phase) {
            return Some(report);
        }
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            match self.listener.accept() {
                Ok((mut stream, _)) => {
                    stream
                        .set_read_timeout(Some(Duration::from_secs(1)))
                        .expect("bound origin request read");
                    let mut request = Vec::new();
                    let mut byte = [0_u8; 1];
                    while request.len() < 8192 {
                        match stream.read(&mut byte) {
                            Ok(0) => break,
                            Ok(_) => {
                                request.push(byte[0]);
                                if byte[0] == b'\n' {
                                    break;
                                }
                            }
                            Err(error)
                                if matches!(
                                    error.kind(),
                                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                                ) =>
                            {
                                break;
                            }
                            Err(error) => panic!("read KEL-135 origin request: {error}"),
                        }
                    }
                    if request.is_empty() {
                        continue;
                    }
                    let request = String::from_utf8_lossy(&request);
                    let line = request.lines().next().unwrap_or_default();
                    eprintln!("KELD_KEL135_ORIGIN_REQUEST {line}");
                    let path = line.split_whitespace().nth(1).unwrap_or("/");
                    if let Some(query) = path.strip_prefix("/report?") {
                        let fields = query
                            .split('&')
                            .filter_map(|part| part.split_once('='))
                            .map(|(key, value)| (key.to_owned(), value.to_owned()))
                            .collect::<std::collections::BTreeMap<_, _>>();
                        if fields.get("phase").map(String::as_str) == Some(phase) {
                            write_profile_http(&mut stream, 204, "");
                            return Some(fields);
                        }
                        write_profile_http(&mut stream, 204, "");
                        if let Some(other_phase) = fields.get("phase") {
                            assert!(
                                self.pending_reports.len() < 4,
                                "too many unmatched fixture reports"
                            );
                            self.pending_reports.insert(other_phase.clone(), fields);
                        }
                    } else if path.starts_with("/script-started?") {
                        write_profile_http(&mut stream, 204, "");
                    } else if path.starts_with("/favicon") {
                        write_profile_http(&mut stream, 204, "");
                    } else if path.starts_with("/sw.js") {
                        write_profile_http_type(
                            &mut stream,
                            200,
                            "application/javascript; charset=utf-8",
                            service_worker_script(),
                        );
                    } else {
                        let request_phase = path
                            .trim_start_matches('/')
                            .split_once('?')
                            .map_or(path.trim_start_matches('/'), |(route, _)| route);
                        let page_seed =
                            if request_phase.starts_with("media-seed-") || request_phase == phase {
                                seed
                            } else {
                                None
                            };
                        let html = profile_origin_html(request_phase, page_seed);
                        write_profile_http(&mut stream, 200, &html);
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(error) => panic!("accept KEL-135 local origin request: {error}"),
            }
        }
        None
    }
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
fn profile_nonce_script() -> &'static str {
    r#"async function profileNonce(database,key,value){
 const request=indexedDB.open(database,1);
 const db=await new Promise((resolve,reject)=>{request.onupgradeneeded=()=>request.result.createObjectStore("state");request.onsuccess=()=>resolve(request.result);request.onerror=()=>reject(request.error);});
 if(value)await new Promise((resolve,reject)=>{const tx=db.transaction("state","readwrite");tx.objectStore("state").put(value,key);tx.oncomplete=resolve;tx.onerror=()=>reject(tx.error);});
 const current=await new Promise((resolve,reject)=>{const tx=db.transaction("state","readonly");const read=tx.objectStore("state").get(key);read.onsuccess=()=>resolve(read.result||"");read.onerror=()=>reject(read.error);});
 db.close();return current;
}"#
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
fn query_origin_html(phase: &str, seed: &str, nonce_script: &str) -> String {
    format!(
        r#"<!doctype html><meta charset="utf-8"><title>KEL-135 media query {phase}</title>
<script>
const phase={phase:?}, value={seed:?}, key="keld-kel135-query-nonce";
{nonce_script}
async function state(name){{try{{return (await navigator.permissions.query({{name}})).state;}}catch(error){{return "error-"+error.name;}}}}
async function run(){{const local=await profileNonce("keld-kel135-query",key,value),camera=await state("camera"),microphone=await state("microphone");await fetch("/report?"+new URLSearchParams({{phase,local,media:"not-requested",camera,microphone}}));}}
run().catch(error=>fetch("/report?"+new URLSearchParams({{phase,local:"",media:"error-"+error.name,camera:"unavailable",microphone:"unavailable"}})));
</script>"#
    )
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
fn profile_origin_html(phase: &str, seed: Option<&str>) -> String {
    let seed = seed.unwrap_or_default();
    let nonce_script = profile_nonce_script();
    if phase.starts_with("query-") {
        return query_origin_html(phase, seed, nonce_script);
    }
    if phase.starts_with("media-nonce-") {
        return format!(
            r#"<!doctype html><meta charset="utf-8"><title>KEL-135 media nonce {phase}</title>
<script>
const phase={phase:?}, key="keld-kel135-media-nonce";
{nonce_script}
async function run(){{
 const channel=new BroadcastChannel("keld-kel135-media-nonce");
 const committed=new Promise(resolve=>{{channel.onmessage=resolve;}});
 let local=await profileNonce("keld-kel135-media",key,"");
 if(!local&&phase==="media-nonce-reuse"){{await committed;local=await profileNonce("keld-kel135-media",key,"");}}
 channel.close();
 await fetch("/report?"+new URLSearchParams({{phase,local,media:"not-requested"}}));
}}
run().catch(error=>fetch("/report?"+new URLSearchParams({{phase,local:"",media:"error-"+error.name}})));
</script>"#
        );
    }
    if let Some(media_phase) = phase.strip_prefix("media-") {
        let Some((mode, kind)) = media_phase.split_once('-') else {
            panic!("invalid media fixture phase");
        };
        let constraint = match kind {
            "camera" => "{video:true}",
            "microphone" => "{audio:true}",
            _ => panic!("invalid media fixture kind"),
        };
        return format!(
            r#"<!doctype html><meta charset="utf-8"><title>KEL-135 media {phase}</title>
<script>
const phase={phase:?}, mode={mode:?}, kind={kind:?}, value={seed:?}, key="keld-kel135-media-nonce";
{nonce_script}
let nonce="";
fetch("/script-started?phase="+encodeURIComponent(phase)+"&media="+Boolean(navigator.mediaDevices&&navigator.mediaDevices.getUserMedia)).catch(()=>{{}});
async function permissionState(){{try{{if(!navigator.permissions||!navigator.permissions.query)return "unavailable";return (await navigator.permissions.query({{name:kind}})).state;}}catch(error){{return "error-"+error.name;}}}}
async function capture(){{
 const stream=await navigator.mediaDevices.getUserMedia({constraint});const tracks=stream.getTracks(),expected=kind==="camera"?"video":"audio";
 const live=tracks.length===1&&tracks[0].kind===expected&&tracks[0].readyState==="live";
 const deviceIdPresent=Boolean(tracks[0]&&tracks[0].getSettings().deviceId),label=tracks[0]?.label||"";
 let frameProgress="not-applicable";
 try{{
  if(live&&expected==="video"){{
   const video=document.createElement("video");video.muted=true;video.playsInline=true;video.style.display="none";video.srcObject=stream;document.documentElement.append(video);
   try{{
    await video.play();
    frameProgress=await new Promise(resolve=>{{
     if(typeof video.requestVideoFrameCallback!=="function"){{resolve("unsupported");return;}}
     const timeout=setTimeout(()=>resolve("timeout"),10000);let first=null;
     const onFrame=(_,metadata)=>{{if(first!==null&&metadata.presentedFrames>first){{clearTimeout(timeout);resolve("progressed");}}else{{first=metadata.presentedFrames;video.requestVideoFrameCallback(onFrame);}}}};
     video.requestVideoFrameCallback(onFrame);
    }});
   }}finally{{video.pause();video.srcObject=null;video.remove();}}
  }}
 }}finally{{for(const track of tracks)track.stop();}}
 return {{result:live?"resolved-"+expected+"-live":"invalid-track-state",deviceIdPresent,label,frameProgress}};
}}
const labelHex=label=>Array.from(new TextEncoder().encode(label),byte=>byte.toString(16).padStart(2,"0")).join("");
async function run(){{
 nonce=await profileNonce("keld-kel135-media",key,mode==="seed"?value:"");
 if(mode==="seed"){{const channel=new BroadcastChannel("keld-kel135-media-nonce");channel.postMessage("committed");channel.close();}}
 const permissionBefore=await permissionState();let permissionAfter="unavailable",media="",mediaRepeat="not-requested",deviceIdPresent="unavailable",deviceLabel="",repeatLabel="",frameProgress="unavailable",repeatFrameProgress="not-requested";
 try{{const first=await capture();media=first.result;deviceIdPresent=String(first.deviceIdPresent);deviceLabel=first.label;frameProgress=first.frameProgress;permissionAfter=await permissionState();if(mode==="seed"){{try{{const repeat=await capture();mediaRepeat=repeat.result;repeatLabel=repeat.label;repeatFrameProgress=repeat.frameProgress;}}catch(error){{mediaRepeat="error-"+error.name;}}}}}}
 catch(error){{media="error-"+error.name;permissionAfter=await permissionState();}}
 await fetch("/report?"+new URLSearchParams({{phase,local:nonce,media,media_repeat:mediaRepeat,device_id_present:deviceIdPresent,device_label_hex:labelHex(deviceLabel),repeat_label_hex:labelHex(repeatLabel),frame_progress:frameProgress,repeat_frame_progress:repeatFrameProgress,permission_before:permissionBefore,permission_after:permissionAfter}}));
}}
run().catch(error=>fetch("/report?"+new URLSearchParams({{phase,local:nonce,media:"error-"+error.name,media_repeat:"unavailable",permission_before:"unavailable",permission_after:"unavailable"}})));
</script>"#
        );
    }
    format!(
        r#"<!doctype html><meta charset="utf-8"><title>KEL-135 {phase}</title>
<script>
const phase={phase:?}, value={seed:?}, key="keld-kel135-profile-state";
const report=(local,cookie,idb,cache,sw)=>fetch("/report?"+new URLSearchParams({{phase,local,cookie,idb,cache,sw}}));
const cookie=()=>{{const item=document.cookie.split("; ").find(part=>part.startsWith("keld_kel135="));return item?item.slice("keld_kel135=".length):"";}};
function openDb(){{return new Promise((resolve,reject)=>{{const request=indexedDB.open("keld-kel135-profile",1);request.onupgradeneeded=()=>request.result.createObjectStore("state");request.onsuccess=()=>resolve(request.result);request.onerror=()=>reject(request.error);}});}}
async function readCache(){{try{{const cache=await caches.open("keld-kel135-profile");const response=await cache.match("/keld-cache-state");return response?await response.text():"";}}catch{{return "";}}}}
async function run(){{
 if(phase==="seed"){{
  localStorage.setItem(key,value);document.cookie="keld_kel135="+value+"; path=/; max-age=3600; SameSite=Lax";
  const db=await openDb();await new Promise((resolve,reject)=>{{const tx=db.transaction("state","readwrite");tx.objectStore("state").put(value,"value");tx.oncomplete=resolve;tx.onerror=()=>reject(tx.error);}});db.close();
  const cache=await caches.open("keld-kel135-profile");await cache.put("/keld-cache-state",new Response(value));
  const registration=await navigator.serviceWorker.register("/sw.js");await navigator.serviceWorker.ready;
  report(localStorage.getItem(key)||"",cookie(),value,await readCache(),registration.active?"true":"false");
 }} else {{
  let idb="";try{{const db=await openDb();idb=await new Promise((resolve,reject)=>{{const tx=db.transaction("state","readonly");const request=tx.objectStore("state").get("value");request.onsuccess=()=>resolve(request.result||"");request.onerror=()=>reject(request.error);}});db.close();}}catch{{}}
  const registrations=await navigator.serviceWorker.getRegistrations();const sw=registrations.some(registration=>Boolean(registration.active));
  report(localStorage.getItem(key)||"",cookie(),idb,await readCache(),sw?"true":"false");
 }}
}}
run().catch(()=>report("ERROR","ERROR","ERROR","ERROR","ERROR"));
</script>"#
    )
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
fn write_profile_http(stream: &mut TcpStream, status: u16, body: &str) {
    write_profile_http_type(stream, status, "text/html; charset=utf-8", body);
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
fn write_profile_http_type(stream: &mut TcpStream, status: u16, content_type: &str, body: &str) {
    let reason = if status == 200 {
        "OK"
    } else if status == 204 {
        "No Content"
    } else {
        "Bad Request"
    };
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream
        .write_all(response.as_bytes())
        .expect("write KEL-135 origin response");
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
fn service_worker_script() -> &'static str {
    "self.addEventListener('install', event => event.waitUntil(self.skipWaiting())); self.addEventListener('activate', event => event.waitUntil(self.clients.claim()));"
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
fn assert_store_report_matches(log: &str, store_uuid: &str) {
    let report = log
        .lines()
        .find(|line| line.contains("KELD_KEL135_STORE"))
        .expect("selected WKWebsiteDataStore report");
    assert!(
        report.contains(&format!("expected_store_uuid={store_uuid}")),
        "{report}"
    );
    assert!(
        report.contains(&format!("actual_identifier={store_uuid}")),
        "{report}"
    );
    assert!(report.contains("persistent=true"), "{report}");
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
fn assert_ephemeral_store_report(log: &str) {
    let report = log
        .lines()
        .find(|line| line.contains("KELD_KEL135_STORE"))
        .expect("selected ephemeral WKWebsiteDataStore report");
    assert!(report.contains("mode=ephemeral-dev"), "{report}");
    assert!(report.contains("actual_identifier=none"), "{report}");
    assert!(report.contains("persistent=false"), "{report}");
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
fn boot_uuid_from_log(log: &str) -> String {
    log.lines()
        .find_map(|line| line.strip_prefix("KELD_KEL135_LIFECYCLE boot_uuid_hex="))
        .expect("real current kern.bootsessionuuid evidence")
        .to_owned()
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
fn system_boot_uuid_hex() -> String {
    let output = Command::new("/usr/sbin/sysctl")
        .args(["-n", "kern.bootsessionuuid"])
        .output()
        .expect("read independent current boot UUID");
    assert!(output.status.success(), "sysctl boot UUID query failed");
    let value = std::str::from_utf8(&output.stdout)
        .expect("sysctl boot UUID is UTF-8")
        .trim()
        .replace('-', "")
        .to_ascii_lowercase();
    assert_eq!(value.len(), 32, "sysctl must return one canonical UUID");
    assert!(value.bytes().all(|byte| byte.is_ascii_hexdigit()));
    value
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
fn system_boot_time_seconds() -> u64 {
    let output = Command::new("/usr/sbin/sysctl")
        .args(["kern.boottime"])
        .output()
        .expect("read independent kernel boot time");
    assert!(
        output.status.success(),
        "sysctl boottime failed: {output:?}"
    );
    let report = String::from_utf8(output.stdout).expect("sysctl boottime is UTF-8");
    report
        .split_once("sec = ")
        .and_then(|(_, suffix)| {
            suffix
                .split(|character: char| !character.is_ascii_digit())
                .next()
        })
        .and_then(|value| value.parse::<u64>().ok())
        .expect("sysctl boottime has a seconds field")
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
fn run_signed_purge_report(app: &Path, support_root: &Path) -> String {
    let output = Command::new(signed_host_executable(app))
        .arg("--keld-profile-purge-fixture-v1")
        .env("KELD_PROFILE_TEST_ROOT", support_root)
        .output()
        .expect("launch signed exact-identity purge process");
    assert!(
        output.status.success(),
        "signed exact-identity purge failed: {output:?}"
    );
    String::from_utf8(output.stdout).expect("purge report is UTF-8")
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
fn run_signed_purge_crash_probe(app: &Path, support_root: &Path) -> Output {
    Command::new(signed_host_executable(app))
        .arg("--keld-profile-purge-fixture-v1")
        .env("KELD_PROFILE_TEST_ROOT", support_root)
        .env("KELD_PROFILE_TEST_PURGE_CRASH_AFTER_CALLBACK", "1")
        .output()
        .expect("launch purge recovery interruption fixture")
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
fn run_signed_purge_phase_crash_probe(app: &Path, support_root: &Path, phase: &str) -> Output {
    Command::new(signed_host_executable(app))
        .arg("--keld-profile-purge-fixture-v1")
        .env("KELD_PROFILE_TEST_ROOT", support_root)
        .env("KELD_PROFILE_TEST_PURGE_CRASH_AFTER_PHASE", phase)
        .output()
        .expect("launch fsynced purge phase interruption fixture")
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
fn run_signed_store_presence_report(app: &Path) -> String {
    let output = Command::new(signed_host_executable(app))
        .arg("--keld-profile-presence-fixture-v1")
        .output()
        .expect("enumerate signed app's exact WebKit store registry");
    assert!(
        output.status.success(),
        "signed store enumeration failed: {output:?}"
    );
    String::from_utf8(output.stdout).expect("store presence report is UTF-8")
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
fn webkit_version() -> String {
    let output = Command::new("/usr/libexec/PlistBuddy")
        .args([
            "-c",
            "Print:CFBundleVersion",
            "/System/Library/Frameworks/WebKit.framework/Versions/A/Resources/Info.plist",
        ])
        .output()
        .expect("read system WebKit framework version");
    assert!(output.status.success(), "read WebKit version: {output:?}");
    String::from_utf8(output.stdout)
        .expect("WebKit version is UTF-8")
        .trim()
        .to_owned()
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
fn run_signed_identity_report(app: &Path) -> std::collections::BTreeMap<String, String> {
    let output = Command::new(signed_host_executable(app))
        .arg("--keld-profile-identity-fixture-v1")
        .output()
        .expect("run signed current-process identity probe");
    parse_signed_identity_report(output)
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
fn run_signed_identity_report_as_user(
    username: &str,
    user_temp: &Path,
    app: &Path,
) -> std::collections::BTreeMap<String, String> {
    let executable = signed_host_executable(app);
    let environment = [("TMPDIR", user_temp.to_string_lossy().into_owned())];
    let output = run_as_local_user_command(
        username,
        executable.as_os_str(),
        &["--keld-profile-identity-fixture-v1"],
        &environment,
    )
    .output()
    .expect("run signed identity probe as second standard user");
    parse_signed_identity_report(output)
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
fn parse_signed_identity_report(output: Output) -> std::collections::BTreeMap<String, String> {
    assert!(
        output.status.success(),
        "signed identity probe failed: {output:?}"
    );
    let report = String::from_utf8(output.stdout).expect("signed identity report is UTF-8");
    let line = report
        .lines()
        .find(|line| line.starts_with("KELD_KEL135_SIGNED_IDENTITY "))
        .expect("validated signed identity report marker");
    let fields: std::collections::BTreeMap<String, String> = line
        .split_whitespace()
        .skip(1)
        .filter_map(|field| {
            field
                .split_once('=')
                .map(|(key, value)| (key.to_owned(), value.to_owned()))
        })
        .collect();
    assert_eq!(
        fields
            .get("signature_validated_before_identity_read")
            .map(String::as_str),
        Some("true")
    );
    fields
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
fn make_signed_fixture_readable_by_standard_users(app: &Path) {
    for directory in [
        app,
        &app.join("Contents"),
        &app.join("Contents/MacOS"),
        &app.join("Contents/_CodeSignature"),
    ] {
        fs::set_permissions(directory, fs::Permissions::from_mode(0o755))
            .expect("make signed fixture directories traversable by the test account");
    }
    fs::set_permissions(
        &app.join("Contents/Info.plist"),
        fs::Permissions::from_mode(0o644),
    )
    .expect("make signed app metadata readable by the test account");
    fs::set_permissions(
        &signed_host_executable(app),
        fs::Permissions::from_mode(0o755),
    )
    .expect("make signed host executable readable by the test account");
    let verification = Command::new("/usr/bin/codesign")
        .args(["--verify", "--deep", "--strict"])
        .arg(app)
        .output()
        .expect("re-verify cross-user fixture after setting traversal permissions");
    assert!(
        verification.status.success(),
        "cross-user fixture signature did not survive permission setup: {verification:?}"
    );
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
fn run_signed_purge_report_as_user(
    username: &str,
    user_temp: &Path,
    profile_root: &Path,
    app: &Path,
) -> Output {
    let executable = signed_host_executable(app);
    let environment = [
        ("TMPDIR", user_temp.to_string_lossy().into_owned()),
        (
            "KELD_PROFILE_TEST_ROOT",
            profile_root.to_string_lossy().into_owned(),
        ),
    ];
    run_as_local_user_command(
        username,
        executable.as_os_str(),
        &["--keld-profile-purge-fixture-v1"],
        &environment,
    )
    .output()
    .expect("run exact-identity purge as second standard user")
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
fn run_as_local_user(
    username: &str,
    executable: &str,
    arguments: &[&str],
    environment: &[(&str, String)],
) -> Output {
    run_as_local_user_command(
        username,
        std::ffi::OsStr::new(executable),
        arguments,
        environment,
    )
    .output()
    .expect("run command as the second ordinary macOS user")
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
fn run_as_local_user_command(
    username: &str,
    executable: &std::ffi::OsStr,
    arguments: &[&str],
    environment: &[(&str, String)],
) -> Command {
    assert!(
        !username.is_empty()
            && username
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_'),
        "local test username has a rejected form"
    );
    let mut command = Command::new("/usr/bin/sudo");
    command.args(["-n", "-H", "-u", username, "--", "/usr/bin/env"]);
    for (name, value) in environment {
        command.arg(format!("{name}={value}"));
    }
    command.arg(executable).args(arguments);
    command
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
fn account_numeric_value(username: &str) -> u32 {
    let output = Command::new("/usr/bin/id")
        .args(["-u", username])
        .output()
        .expect("read macOS account UID");
    assert!(output.status.success(), "account UID lookup command failed");
    std::str::from_utf8(&output.stdout)
        .expect("account UID is UTF-8")
        .trim()
        .parse()
        .expect("account UID is numeric")
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
fn current_account_numeric_id() -> u32 {
    let output = Command::new("/usr/bin/id")
        .arg("-u")
        .output()
        .expect("read current macOS account UID");
    assert!(output.status.success(), "current UID lookup command failed");
    std::str::from_utf8(&output.stdout)
        .expect("current UID is UTF-8")
        .trim()
        .parse()
        .expect("current UID is numeric")
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
fn account_groups(username: &str) -> String {
    let output = Command::new("/usr/bin/id")
        .args(["-Gn", username])
        .output()
        .expect("read macOS account groups");
    assert!(
        output.status.success(),
        "account group lookup command failed"
    );
    std::str::from_utf8(&output.stdout)
        .expect("account groups are UTF-8")
        .trim()
        .to_owned()
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
fn account_home(username: &str) -> PathBuf {
    assert!(
        !username.is_empty()
            && username
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_'),
        "local test username has a rejected form"
    );
    let record = Command::new("/usr/bin/dscl")
        .args([
            ".",
            "-read",
            &format!("/Users/{username}"),
            "NFSHomeDirectory",
        ])
        .output()
        .expect("read macOS account home");
    assert!(
        record.status.success(),
        "account home lookup failed: {record:?}"
    );
    let text = String::from_utf8(record.stdout).expect("account home is UTF-8");
    let path = text
        .trim()
        .strip_prefix("NFSHomeDirectory: ")
        .expect("account record includes its home path");
    PathBuf::from(path)
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
fn path_text(path: &Path) -> &str {
    path.to_str().expect("macOS test fixture path is UTF-8")
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
fn copy_profile_fixture_tree(source: &Path, destination: &Path) {
    for entry in fs::read_dir(source).expect("list signed fixture source") {
        let entry = entry.expect("signed fixture directory entry");
        let source = entry.path();
        let target = destination.join(entry.file_name());
        let metadata = fs::symlink_metadata(&source).expect("inspect signed fixture source");
        if metadata.file_type().is_symlink() {
            panic!("test fixture contains a symlink: {source:?}");
        }
        if metadata.is_dir() {
            fs::create_dir(&target).expect("create signed fixture directory");
            fs::set_permissions(&target, fs::Permissions::from_mode(0o700))
                .expect("protect signed fixture directory");
            copy_profile_fixture_tree(&source, &target);
        } else if metadata.is_file() {
            fs::copy(&source, &target).expect("copy signed fixture file");
            fs::set_permissions(&target, metadata.permissions())
                .expect("preserve staged fixture file permissions");
        } else {
            panic!("unsupported signed fixture node: {source:?}");
        }
    }
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
fn sw_vers_value(key: &str) -> String {
    let output = Command::new("/usr/bin/sw_vers")
        .arg(key)
        .output()
        .expect("read current macOS version");
    assert!(output.status.success(), "sw_vers {key} failed: {output:?}");
    String::from_utf8(output.stdout)
        .expect("sw_vers output is UTF-8")
        .trim()
        .to_owned()
}

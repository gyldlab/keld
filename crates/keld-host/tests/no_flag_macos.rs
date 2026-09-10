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
use std::sync::OnceLock;
use std::sync::mpsc::{self, Receiver};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// Dark background for fixture renderers, so a test run does not flash
/// white windows across the operator's desktop. Cosmetic only: no test
/// asserts on it, and the beacon/marker contracts are unchanged.
const DARK_BG: &str = "<style>html,body{background:#111;color:#eee}</style>";
const TITLE: &str = "KEL96 T1b Fixture";
const MARKER: &str = "KEL96_T1B_EXACT_RENDERER_7e2d9b";
const FORWARDED_LOG: &str = "KEL96_T2_FORWARDED_LOG";
/// What `lsof` reports for a Unix socket that is neither bound nor connected.
///
/// This is a placeholder, not an identity: every unbound socket on the machine
/// reports it, so unlike a `sun_path` or a peer address it can never establish
/// that two descriptors are the same kernel object (measured on macOS `lsof`
/// 4.91).
const ANONYMOUS_UNIX_SOCKET: &str = "->(none)";
const EVENT_DEADLINE: Duration = Duration::from_secs(15);
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

    let mut child = Command::new(std::env::current_exe().expect("current test executable"))
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
        self_opened_unix_sockets(child_pid),
        vec![owned_identity],
        "census must charge the child only the listener it bound itself: {observed:?}"
    );

    // Closing the far end is the child's release signal; the harness keeps its
    // own copy and the listener until scope end, so the leaked identity stays in
    // the harness table for the whole census above.
    drop(far_end);
    let status = child.wait().expect("reap census fixture");
    assert!(status.success(), "census fixture failed: {status:?}");
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
/// here, so this test pins the observed `lsof` output rather than agreeing with
/// whatever constant the census uses.
#[test]
fn unix_descriptor_census_charges_anonymous_sockets_it_cannot_attribute() {
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
    let mut charged = self_opened_unix_sockets(child_pid);
    charged.sort();
    let mut expected = vec!["->(none)".to_owned(), owned_identity];
    expected.sort();
    assert_eq!(
        charged, expected,
        "census must charge the child the anonymous socket it opened itself: {observed:?}"
    );

    drop(child.stdin.take().expect("census fixture release lease"));
    let status = child.wait().expect("reap census fixture");
    assert!(status.success(), "census fixture failed: {status:?}");
    await_process_gone(child_pid);
    drop(harness_anonymous);
}

/// KEL-222: owning no Unix descriptor is an empty census, not a census failure.
///
/// Zero is the *passing* value for the CLI, so the census must distinguish "this
/// process owns none" from "the census could not run". This pins that: the child
/// closes every Unix descriptor this harness could leak into it, discovered from
/// the harness's own census rather than hard-coded, so it provably owns none, and
/// reports readiness on its own pipe so the census never races the `exec`.
#[test]
fn unix_descriptor_census_of_a_process_without_unix_descriptors_is_empty() {
    let mut closes = String::new();
    for (descriptor, _) in unix_descriptors(std::process::id()) {
        // 0, 1 and 2 are replaced by the spawn's own stdio redirection.
        if !matches!(descriptor.as_str(), "0" | "1" | "2") {
            closes.push_str("exec ");
            closes.push_str(&descriptor);
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
    let mut ready = String::new();
    BufReader::new(child.stdout.take().expect("census fixture readiness pipe"))
        .read_line(&mut ready)
        .expect("await census fixture readiness");
    assert_eq!(ready.trim_end(), "READY");

    let observed = unix_socket_identities(child_pid);
    assert!(
        observed.is_empty(),
        "descriptor-free fixture still owns Unix descriptors: {observed:?}"
    );
    let opened = self_opened_unix_sockets(child_pid);
    assert!(
        opened.is_empty(),
        "descriptor-free fixture was charged Unix descriptors: {opened:?}"
    );

    drop(child.stdin.take().expect("census fixture release lease"));
    let status = child.wait().expect("reap descriptor-free census fixture");
    assert!(status.success(), "census fixture failed: {status:?}");
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
    let child = Command::new(stage.host())
        .env("KELD_T1B_CONTROL", &control_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("launch navigation-timeout host");
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
    let child = Command::new(stage.host())
        .env("KELD_T3_CRASH_BEFORE_HELLO", "1")
        .env("KELD_T3_PRE_READY_MARKER", &attempt_marker)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("launch pre-Ready crash host");
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
    cycle
        .current
        .as_mut()
        .expect("third generation")
        .writer
        .write_all(b"CRASH\n")
        .expect("crash threshold generation");
    let output = cycle.wait_host();
    assert!(!output.status.success(), "crash loop became success");
    let stderr = String::from_utf8(output.stderr).expect("crash-loop stderr UTF-8");
    assert!(stderr.contains("KELD-CORE-033"), "{stderr}");
    assert!(stderr.contains("KELD-RUNTIME-002"), "{stderr}");
    assert!(
        matches!(cycle.listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock),
        "crash-loop threshold provisioned a fourth generation"
    );
    cycle.assert_current_group_gone();
}

struct RecoveryCycle {
    host: Option<Child>,
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
        let child = Command::new(stage.host())
            .env("KELD_T1B_CONTROL", &control_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("launch T3 no-flag host");
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
        wait_child_output(self.host.take().expect("live T3 host"), EVENT_DEADLINE)
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
            !self_opened_unix_sockets(cycle.host_pid).is_empty(),
            "host owns no Unix app-link descriptor it opened itself"
        );
        let cli_sockets = self_opened_unix_sockets(cli_pid);
        assert!(
            cli_sockets.is_empty(),
            "CLI {cli_pid} owns Unix descriptors it opened itself: {cli_sockets:?}"
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
            .expect("reuse canonical KEL-72 TypeScript link owner");
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
        self.launch_cycle_inner(cycle, false).0
    }

    fn launch_leased_cycle(&self, cycle: &str) -> (LiveCycle, ChildStdin) {
        let (cycle, lease) = self.launch_cycle_inner(cycle, true);
        (cycle, lease.expect("leased cycle writer"))
    }

    fn launch_cycle_inner(
        &self,
        cycle: &str,
        with_dev_lease: bool,
    ) -> (LiveCycle, Option<ChildStdin>) {
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
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if with_dev_lease {
            command
                .env("KELD_DEV_LEASE", "stdin-v1")
                .env("KELD_T2_EXIT_ON_LINK_EOF", "1")
                .stdin(Stdio::piped());
        }
        let presentation = self.observe_initial_window();
        let mut child = command.spawn().expect("launch staged no-flag host");
        let lease_writer = child.stdin.take();
        let host_pid = child.id();
        let control = accept_before(&listener, Instant::now() + EVENT_DEADLINE);
        control
            .set_read_timeout(Some(EVENT_DEADLINE))
            .expect("control read deadline");
        let control_reader = BufReader::new(control.try_clone().expect("control reader clone"));
        let mut cycle = LiveCycle {
            host: Some(child),
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
        (cycle, lease_writer)
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
            !self_opened_unix_sockets(self.host_pid).is_empty(),
            "host owns no authenticated Unix app-link descriptor"
        );
        assert!(
            !self_opened_unix_sockets(self.bun_pid).is_empty(),
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
        let mut child = self.host.take().expect("live host");
        let deadline = Instant::now() + EVENT_DEADLINE;
        loop {
            if child.try_wait().expect("inspect no-flag host").is_some() {
                return child
                    .wait_with_output()
                    .expect("collect no-flag host output");
            }
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
    assert!(!timed_out, "child exceeded exit deadline: {output:?}");
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

/// Every Unix-domain descriptor open on `pid`, as `(descriptor, identity)`.
///
/// The identity is the `lsof` name field: the bound `sun_path` for a listener
/// and for each peer it accepted, or `->0x<kernel address>` for a connected or
/// paired endpoint. `exec` preserves it byte for byte, so one kernel socket
/// reports the same identity in a parent and in every child that inherited the
/// descriptor. Identities repeat, so callers keep the multiset.
///
/// `lsof` exits 1 only when it cannot locate the pid at all. A live process that
/// owns no Unix descriptor exits 0 with no rows, so an empty census is a result
/// and not an error (measured on macOS `lsof` 4.91). Censusing a pid that has
/// already exited does fail here, which is the intent: treating that as an empty
/// census would let an ownership assertion pass vacuously against a dead process.
fn unix_descriptors(pid: u32) -> Vec<(String, String)> {
    let output = Command::new("/usr/sbin/lsof")
        .args(["-n", "-P", "-a", "-p", &pid.to_string(), "-U", "-Ffn"])
        .output()
        .expect("enumerate Unix descriptors");
    assert!(
        output.status.success(),
        "lsof Unix descriptors for {pid}: {output:?}"
    );
    let rendered = String::from_utf8(output.stdout).expect("lsof output UTF-8");
    let mut descriptor = None;
    let mut descriptors = Vec::new();
    for line in rendered.lines() {
        if let Some(fd) = line.strip_prefix('f') {
            descriptor = Some(fd.to_owned());
        } else if let Some(name) = line.strip_prefix('n') {
            descriptors.push((
                descriptor
                    .take()
                    .expect("lsof names a descriptor it listed"),
                name.to_owned(),
            ));
        }
    }
    descriptors
}

/// One identity per Unix-domain descriptor open on `pid`.
fn unix_socket_identities(pid: u32) -> Vec<String> {
    unix_descriptors(pid)
        .into_iter()
        .map(|(_, identity)| identity)
        .collect()
}

/// The Unix-domain descriptors `pid` opened itself, as identities.
///
/// A raw `lsof -U` count is not an oracle for app-link ownership. Every product
/// process censused here descends from this harness, and a Unix socket the
/// launching shell left without `FD_CLOEXEC` reaches all of them through `exec`,
/// so the raw count charges the CLI for descriptors it never opened (KEL-222).
/// The harness is their only ancestor, so its own live descriptor table bounds
/// everything they can have inherited: whatever survives subtracting it was
/// opened by `pid`.
///
/// Subtracting is only safe for a *named* identity: a `sun_path` names the socket
/// bound to it and every peer accepted on it, and a peer address is unique while
/// both ends are open, so removing one discounts the inherited descriptor rather
/// than a coincidental twin. An unbound socket has no identity at all — `lsof`
/// reports every one of them as [`ANONYMOUS_UNIX_SOCKET`] — so it is never
/// subtracted and is always charged to `pid`. That keeps an anonymous socket the
/// process opened itself visible; the cost is charging one it merely inherited,
/// which fails loudly instead of passing silently.
///
/// One residual is inherent to naming a socket by its path: a descriptor stays
/// bound to its `sun_path` after that path is unlinked, so a harness holding a
/// stale-bound descriptor could absorb a censused process that rebinds the same
/// path. This harness binds only fixture paths under its own temporary directory
/// and never a product session path, so it cannot construct that collision.
///
/// Removing one harness identity per match keeps this a multiset operation,
/// because a listener and each peer it accepted report the same `sun_path`, and
/// a duplicate beyond the inherited one must still be charged to `pid`. The
/// harness is censused first: a descriptor it opens afterwards cannot have been
/// inherited by an already-spawned child, so measuring it later could only
/// excuse a real leak. `cargo nextest` gives each test its own process, so the
/// table read here is this test's own.
fn self_opened_unix_sockets(pid: u32) -> Vec<String> {
    let mut inherited = unix_socket_identities(std::process::id());
    let mut opened = Vec::new();
    for identity in unix_socket_identities(pid) {
        if identity == ANONYMOUS_UNIX_SOCKET {
            opened.push(identity);
            continue;
        }
        match inherited.iter().position(|held| *held == identity) {
            Some(index) => {
                inherited.swap_remove(index);
            }
            None => opened.push(identity),
        }
    }
    opened
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

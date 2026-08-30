//! KEL-96/T1b real-macOS no-flag host/window/session acceptance.
#![cfg(target_os = "macos")]
#![allow(clippy::expect_used, clippy::panic)] // extra test crate: assertions are the oracle
#![allow(clippy::zombie_processes)] // cleanup owns host plus the enrolled Bun process group

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::os::unix::fs::{PermissionsExt, symlink};
use std::os::unix::net::{UnixListener, UnixStream};
use std::os::unix::process::{CommandExt as _, ExitStatusExt};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Output, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const TITLE: &str = "KEL96 T1b Fixture";
const MARKER: &str = "KEL96_T1B_EXACT_RENDERER_7e2d9b";
const FORWARDED_LOG: &str = "KEL96_T2_FORWARDED_LOG";
const EVENT_DEADLINE: Duration = Duration::from_secs(15);
const PROCESS_DEADLINE: Duration = Duration::from_secs(5);

#[test]
fn keld_dev_helper_process() {
    let Some(project) = std::env::var_os("KELD_T2_HELPER_PROJECT") else {
        return;
    };
    keld_cli::dev::run_dev(Path::new(&project)).expect("shipping keld dev helper");
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
            "<!doctype html><title>{TITLE}</title><img src=\"http://127.0.0.1:{}/never\">\n",
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
                "<!doctype html><title>{TITLE}</title><p id=marker>{MARKER}</p><img src=\"http://127.0.0.1:{}/{MARKER}\">\n",
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
        let child = Command::new(stage.host())
            .env("KELD_T1B_CONTROL", &control_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("launch T3 no-flag host");
        let host_pid = child.id();
        let mut current = RecoveryGeneration::accept(&listener, "initial");
        current.expect_ready_and_echoes();
        beacon.assert_exact();
        let window = await_native_windows(host_pid, TITLE, 1);
        assert_eq!(window.len(), 1, "initial T3 native window: {window:?}");
        let first_group = current.bun_pid;
        Self {
            host: Some(child),
            host_pid,
            listener,
            window,
            current: Some(current),
            process_groups: vec![first_group],
        }
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
            native_windows(self.host_pid, TITLE),
            self.window,
            "Bun recovery replaced or closed the host-owned native window"
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
            let _ = Command::new("/bin/kill")
                .args(["-KILL", &format!("-{group}")])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
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
            let _ = Command::new("/bin/kill")
                .args(["-KILL", &format!("-{}", cycle.bun_pid)])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
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

impl ShippingDevCycle {
    fn launch(fixture: &ProductFixture, helper: &Path, name: &str) -> Self {
        let beacon = Beacon::bind(MARKER);
        fs::write(
            fixture.project.join("index.html"),
            format!(
                "<!doctype html><title>{TITLE}</title><p id=marker>{MARKER}</p><img src=\"http://127.0.0.1:{}/{MARKER}\">\n",
                beacon.port()
            ),
        )
        .expect("T2 renderer with exact beacon");
        let control_path = fixture.root.path().join(format!("{name}.sock"));
        let listener = UnixListener::bind(&control_path).expect("bind T2 fixture control");
        listener
            .set_nonblocking(true)
            .expect("nonblocking T2 fixture control");
        let mut cli = Command::new(helper)
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
        if !owns_expected_tree {
            let _ = cli.kill();
            let _ = cli.wait();
            kill_pid(bun_pid);
            kill_pid(descendant_pid);
            let _ = Command::new("/bin/kill")
                .args(["-KILL", &format!("-{bun_pid}")])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
        assert!(
            owns_expected_tree,
            "shipping keld dev did not delegate CLI {cli_pid} -> host {host_pid} -> guardian {guardian_pid} -> Bun {bun_pid}"
        );
        assert_eq!(process_group(bun_pid), bun_pid);
        assert_eq!(process_group(descendant_pid), bun_pid);
        assert_eq!(process_group(cli_pid), cli_pid);
        assert_eq!(process_group(host_pid), host_pid);
        assert_eq!(process_group(guardian_pid), host_pid);
        assert_eq!(read_control_line(&mut control_reader), "READY");
        assert_eq!(read_control_line(&mut control_reader), "ECHO1");
        assert_eq!(read_control_line(&mut control_reader), "ECHO2");
        beacon.assert_exact();
        assert_eq!(await_native_windows(host_pid, TITLE, 1).len(), 1);
        assert!(native_windows(cli_pid, TITLE).is_empty());
        assert!(host_unix_sockets(host_pid) > 0);
        assert_eq!(host_unix_sockets(cli_pid), 0);
        if name == "t2-cli" {
            assert_lease_descriptor_ownership(cli_pid, host_pid, guardian_pid, bun_pid);
        }
        Self {
            cli: Some(cli),
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
        }
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
        let window = native_windows(self.host_pid, TITLE);
        self.control_writer
            .write_all(b"CRASH\n")
            .expect("crash shipping generation");
        let mut successor = RecoveryGeneration::accept(&self.listener, "shipping replacement");
        successor.expect_ready_and_echoes();
        assert_eq!(successor.guardian_pid, old_guardian);
        assert_ne!(successor.bun_pid, old_bun);
        assert_eq!(native_windows(self.host_pid, TITLE), window);
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
        let signal = Command::new("/bin/kill")
            .args([&format!("-{signal_name}"), &format!("-{}", self.cli_pid)])
            .status()
            .expect("signal the CLI process group");
        assert!(signal.success(), "group {signal_name} failed: {signal}");
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
            let _ = Command::new("/bin/kill")
                .args(["-KILL", &format!("-{}", self.bun_pid)])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
    }
}

struct ProductFixture {
    root: tempfile::TempDir,
    project: PathBuf,
    link_source: String,
    harness: &'static str,
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
        }
    }

    fn stage(&self) -> keld_cli::boot::DevBootStage {
        let mut entry = self.link_source.clone();
        entry.push_str(self.harness);
        fs::write(self.project.join("src/main.ts"), entry).expect("fixture entry");
        if !self.project.join("index.html").exists() {
            fs::write(
                self.project.join("index.html"),
                format!("<!doctype html><title>{TITLE}</title><p id=marker>{MARKER}</p>\n"),
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
                "<!doctype html><title>{TITLE}</title><p id=marker>{MARKER}</p><img src=\"http://127.0.0.1:{}/{MARKER}\">\n",
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
    group_gone: bool,
}

impl LiveCycle {
    fn assert_live_product(&self) {
        assert_ne!(self.host_pid, self.guardian_pid);
        assert_ne!(self.guardian_pid, self.bun_pid);
        assert_eq!(parent_process(self.guardian_pid), self.host_pid);
        assert_eq!(parent_process(self.bun_pid), self.guardian_pid);
        assert_eq!(process_group(self.bun_pid), self.bun_pid);
        assert_eq!(process_group(self.descendant_pid), self.bun_pid);
        let windows = await_native_windows(self.host_pid, TITLE, 1);
        assert_eq!(
            windows.len(),
            1,
            "exact host-owned native window: {windows:?}"
        );
        assert!(
            host_unix_sockets(self.host_pid) > 0,
            "host owns no authenticated Unix app-link descriptor"
        );
        assert!(
            host_unix_sockets(self.bun_pid) > 0,
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
            let _ = Command::new("/bin/kill")
                .args(["-KILL", &format!("-{}", self.bun_pid)])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
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

fn host_unix_sockets(pid: u32) -> usize {
    let output = Command::new("/usr/sbin/lsof")
        .args(["-n", "-P", "-a", "-p", &pid.to_string(), "-U", "-Fn"])
        .output()
        .expect("enumerate Unix descriptors");
    assert!(
        output.status.success(),
        "lsof Unix descriptors for {pid}: {output:?}"
    );
    String::from_utf8(output.stdout)
        .expect("lsof output UTF-8")
        .lines()
        .filter(|line| line.starts_with('n'))
        .count()
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

fn native_windows(pid: u32, title: &str) -> Vec<u32> {
    query_native_windows(pid, title, None)
}

fn await_native_windows(pid: u32, title: &str, expected: usize) -> Vec<u32> {
    query_native_windows(pid, title, Some(expected))
}

fn query_native_windows(pid: u32, title: &str, expected: Option<usize>) -> Vec<u32> {
    const SCRIPT: &str = r"
import CoreGraphics
import Darwin
import Foundation
let wantedPID = Int(CommandLine.arguments[1])!
let wantedTitle = CommandLine.arguments[2]
let expected = Int(CommandLine.arguments[3])!
let deadline = Date().addingTimeInterval(Double(CommandLine.arguments[4])!)
while true {
  let rows = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as! [[String: Any]]
  var found: [UInt32] = []
  for row in rows {
    let owner = (row[kCGWindowOwnerPID as String] as? NSNumber)?.intValue
    let name = row[kCGWindowName as String] as? String
    let layer = (row[kCGWindowLayer as String] as? NSNumber)?.intValue
    if owner == wantedPID && name == wantedTitle && layer == 0 {
      found.append((row[kCGWindowNumber as String] as! NSNumber).uint32Value)
    }
  }
  if expected < 0 || found.count == expected {
    for window in found { print(window) }
    exit(0)
  }
  if Date() >= deadline { exit(3) }
  sched_yield()
}
";
    let expected_arg = expected.map_or_else(|| String::from("-1"), |value| value.to_string());
    let timeout_arg = EVENT_DEADLINE.as_secs().to_string();
    let output = Command::new("/usr/bin/xcrun")
        .args([
            "swift",
            "-e",
            SCRIPT,
            &pid.to_string(),
            title,
            &expected_arg,
            &timeout_arg,
        ])
        .output()
        .expect("run native CoreGraphics census");
    assert!(output.status.success(), "CoreGraphics census: {output:?}");
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

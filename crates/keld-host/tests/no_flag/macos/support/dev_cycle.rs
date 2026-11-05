use crate::support::DARK_BG;
use crate::support::EVENT_DEADLINE;
use crate::support::FORWARDED_LOG;
use crate::support::MARKER;
use crate::support::PROCESS_DEADLINE;
use crate::support::TITLE;
use crate::support::control::LiveChildOutput;
use crate::support::control::accept_before;
use crate::support::control::parse_pid;
use crate::support::control::read_control_line;
use crate::support::control::wait_child_output;
use crate::support::lease_descriptors::assert_lease_descriptor_ownership;
use crate::support::native_window::NativeWindowExpectation;
use crate::support::native_window::NativeWindowScope;
use crate::support::native_window::native_windows;
use crate::support::native_window::query_native_windows;
use crate::support::process::ProcessExitObserver;
use crate::support::process::await_process_gone;
use crate::support::process::await_process_gone_before;
use crate::support::process::parent_process;
use crate::support::process::process_exists;
use crate::support::process::process_group;
use crate::support::process::process_ids;
use crate::support::process::signal_process_group;
use crate::support::product::ProductFixture;
use crate::support::recovery_cycle::RecoveryGeneration;
use crate::support::renderer::Beacon;
use crate::support::signal_wait::signal_group_and_await_exit;
use crate::support::unix_descriptors::unix_sockets_not_inherited_from_harness;
use std::fs;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixListener;
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt as _;
use std::os::unix::process::ExitStatusExt as _;
use std::path::Path;
use std::path::PathBuf;
use std::process::Child;
use std::process::Command;
use std::process::Stdio;
use std::time::Instant;

pub(crate) fn prepare_keld_dev_helper(fixture: &ProductFixture) -> PathBuf {
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

const MAX_OBSERVED_PROCESSES: usize = 128;

pub(crate) struct ShippingDevCycle {
    pub(crate) cli: Option<Child>,
    pub(crate) cli_pid: u32,
    pub(crate) host_pid: u32,
    pub(crate) guardian_pid: u32,
    pub(crate) bun_pid: u32,
    pub(crate) descendant_pid: u32,
    pub(crate) session_dir: PathBuf,
    pub(crate) listener: UnixListener,
    pub(crate) control_reader: BufReader<UnixStream>,
    pub(crate) control_writer: UnixStream,
    pub(crate) group_gone: bool,
}

/// Owns a shipping launch until its authenticated process tree is complete.
/// A post-launch assertion may panic before [`ShippingDevCycle`] exists; this
/// guard keeps that failure path from orphaning the CLI lease, host/guardian,
/// or supervised Bun group.
pub(crate) struct ShippingLaunchCleanup {
    pub(crate) cli: Option<Child>,
    pub(crate) host_group: Option<u32>,
    pub(crate) bun_group: Option<u32>,
    owned_stage: Option<(PathBuf, Vec<u32>)>,
    observed_groups: Vec<u32>,
    observed_processes: Vec<(u32, ProcessExitObserver)>,
    live_output: Option<LiveChildOutput>,
}

impl ShippingLaunchCleanup {
    pub(crate) fn new(cli: Child) -> Self {
        Self {
            cli: Some(cli),
            host_group: None,
            bun_group: None,
            owned_stage: None,
            observed_groups: Vec::new(),
            observed_processes: Vec::new(),
            live_output: None,
        }
    }

    pub(crate) fn record_authenticated_groups(&mut self, host_group: u32, bun_group: u32) {
        let test_group = process_group(std::process::id());
        self.host_group = (host_group != 0 && host_group != test_group).then_some(host_group);
        self.bun_group = (bun_group != 0 && bun_group != test_group).then_some(bun_group);
    }

    /// Observe a bounded current family snapshot before operator input and
    /// refresh it afterward. A stable snapshot is not a no-future-fork proof.
    pub(crate) fn observe_family_before(&mut self, deadline: Instant) -> Vec<u32> {
        self.observe_family_before_observing(deadline, |_| {})
    }

    /// The fault observer runs only after actual Bun-group custody is enrolled.
    /// It lets real-process tests fail at that cut before any complete census.
    pub(crate) fn observe_family_before_observing(
        &mut self,
        deadline: Instant,
        mut on_group: impl FnMut(u32),
    ) -> Vec<u32> {
        let host = self.prepare_family_observation();
        let mut previous = Vec::new();
        loop {
            assert!(
                Instant::now() < deadline,
                "Host {host} family snapshot deadline; stage retained as incomplete"
            );
            assert!(
                self.cli
                    .as_mut()
                    .expect("retained Host child")
                    .try_wait()
                    .expect("Host state")
                    .is_none(),
                "Host {host} exited before family custody completed; stage retained as incomplete"
            );
            let guardians = process_ids("ppid", host);
            if guardians.is_empty() {
                std::thread::yield_now();
                continue;
            }
            assert_eq!(
                guardians.len(),
                1,
                "Host {host} has ambiguous children; stage retained as incomplete"
            );
            let guardian = guardians[0];
            if !self
                .observed_processes
                .iter()
                .any(|(pid, _)| *pid == guardian)
            {
                assert!(
                    self.observed_processes.len() < MAX_OBSERVED_PROCESSES,
                    "family observer capacity; unrecorded PID {guardian}; stage retained as incomplete"
                );
                self.observed_processes
                    .push((guardian, ProcessExitObserver::arm(guardian)));
            }
            let buns = process_ids("ppid", guardian);
            if buns.is_empty() {
                std::thread::yield_now();
                continue;
            }
            assert_eq!(
                buns.len(),
                1,
                "Guardian {guardian} has ambiguous children; stage retained as incomplete"
            );
            let bun = buns[0];
            let group = process_group(bun);
            // A physical child relation does not authorize a foreign shared group.
            // Only its own observed leader group or the already-owned Host group
            // is enrollable; escaped/ambiguous groups retain incomplete proof.
            assert!(
                group == bun || group == host,
                "Bun {bun} has unowned observed group {group}; not enrolled; stage retained as incomplete"
            );
            assert_eq!(parent_process(guardian), host);
            assert_eq!(parent_process(bun), guardian);
            // Enroll the actual observed group before callbacks, assertions or
            // kernel observer registration can fail. Never substitute bun as PGID.
            self.record_authenticated_groups(host, group);
            if let Some(group) = self.bun_group
                && !self.observed_groups.contains(&group)
            {
                self.observed_groups.push(group);
            }
            on_group(group);
            assert!(
                self.bun_group == Some(group),
                "Bun {bun} is in an unowned group {group}; stage retained as incomplete"
            );
            if !self.observed_processes.iter().any(|(pid, _)| *pid == bun) {
                assert!(
                    self.observed_processes.len() < MAX_OBSERVED_PROCESSES,
                    "family observer capacity; unrecorded PID {bun}; stage retained as incomplete"
                );
                self.observed_processes
                    .push((bun, ProcessExitObserver::arm(bun)));
            }
            // Before exec/setpgid the child may still share the owned Host group.
            if group == host {
                std::thread::yield_now();
                continue;
            }
            let mut snapshot = vec![(host, std::process::id(), process_group(host))];
            let mut pending = vec![host];
            self.observe_descendants_before(host, deadline, &mut snapshot, &mut pending);
            snapshot.sort_unstable();
            if snapshot == previous && pending.contains(&guardian) && pending.contains(&bun) {
                let pids = self
                    .observed_processes
                    .iter()
                    .map(|(pid, _)| *pid)
                    .collect::<Vec<_>>();
                self.record_stage_processes(&pids);
                return pids;
            }
            previous = snapshot;
            std::thread::yield_now();
        }
    }

    /// Refresh the same retained stage record and enroll the original own
    /// Child's kernel exit witness before the first process-family observation.
    fn prepare_family_observation(&mut self) -> u32 {
        self.owned_stage
            .as_mut()
            .expect("retained exact stage")
            .1
            .clear();
        let host = self.cli.as_ref().expect("retained Host child").id();
        // Keep every observer in this owner through its Drop, including failures.
        if !self.observed_processes.iter().any(|(pid, _)| *pid == host) {
            let observer =
                ProcessExitObserver::arm_own_child(self.cli.as_ref().expect("retained Host child"));
            self.observed_processes.push((host, observer));
        }
        host
    }

    /// Extend the same snapshot while enrolling every actual kernel witness
    /// in this owner before any later assertion or callback can fail.
    fn observe_descendants_before(
        &mut self,
        host: u32,
        deadline: Instant,
        snapshot: &mut Vec<(u32, u32, u32)>,
        pending: &mut Vec<u32>,
    ) {
        let mut cursor = 0;
        while cursor < pending.len() {
            assert!(
                Instant::now() < deadline,
                "family walk deadline; stage retained as incomplete"
            );
            let parent = pending[cursor];
            cursor += 1;
            for child in process_ids("ppid", parent) {
                if pending.contains(&child) {
                    continue;
                }
                assert!(
                    pending.len() < MAX_OBSERVED_PROCESSES,
                    "family census capacity; unrecorded PID {child}; stage retained as incomplete"
                );
                if !self.observed_processes.iter().any(|(pid, _)| *pid == child) {
                    assert!(
                        self.observed_processes.len() < MAX_OBSERVED_PROCESSES,
                        "family observer capacity; unrecorded PID {child}; stage retained as incomplete"
                    );
                    self.observed_processes
                        .push((child, ProcessExitObserver::arm(child)));
                }
                let child_group = process_group(child);
                assert!(
                    child_group == host || self.observed_groups.contains(&child_group),
                    "PID {child} escaped observed groups into {child_group}; stage retained as incomplete"
                );
                assert_eq!(
                    parent_process(child),
                    parent,
                    "family relation changed; stage retained as incomplete"
                );
                snapshot.push((child, parent, child_group));
                pending.push(child);
            }
        }
    }

    /// Transfer kernel exit witnesses only after the actual own Child was reaped.
    pub(crate) fn take_observed_exits(&mut self) -> Vec<(u32, ProcessExitObserver)> {
        assert!(
            self.cli.is_none(),
            "retain exit witnesses until child wait completes"
        );
        std::mem::take(&mut self.observed_processes)
    }

    pub(crate) fn observed_group_ids(&self) -> Vec<u32> {
        let mut groups = self.observed_groups.clone();
        for group in [self.host_group, self.bun_group].into_iter().flatten() {
            if !groups.contains(&group) {
                groups.push(group);
            }
        }
        groups
    }

    /// Drain captured pipes immediately while this owner retains the sole
    /// Child/group/stage custody. The caller supplies its original total bound.
    pub(crate) fn capture_output_before(&mut self, deadline: Instant) {
        assert!(self.live_output.is_none(), "one live pipe owner per Child");
        self.live_output = Some(LiveChildOutput::start(
            self.cli
                .as_mut()
                .expect("retained Child before live capture"),
            deadline,
        ));
    }

    pub(crate) fn release(mut self) -> Child {
        assert!(
            self.live_output.is_none(),
            "live capture requires the owning wait"
        );
        self.host_group = None;
        self.bun_group = None;
        self.owned_stage = None;
        self.observed_groups.clear();
        self.observed_processes.clear();
        self.cli.take().expect("shipping CLI cleanup owner")
    }

    /// Wait under the existing bounded child/pipe owner while retaining this
    /// guard's group and stage custody through caller assertions and unwind.
    pub(crate) fn wait(&mut self, timeout: std::time::Duration) -> std::process::Output {
        let output = wait_child_output(self.cli.take().expect("retained shipping child"), timeout);
        if let Some(capture) = self.live_output.take() {
            let (stdout, stderr) = capture.finish();
            assert!(
                output.stdout.is_empty() && output.stderr.is_empty(),
                "exactly one pipe owner"
            );
            std::process::Output {
                status: output.status,
                stdout,
                stderr,
            }
        } else {
            output
        }
    }

    /// Retain only this fixture's exact producer-owned stage; an incomplete
    /// process census deliberately cannot authorize fallback deletion.
    pub(crate) fn retain_owned_stage(&mut self, stage: &Path) {
        assert!(self.owned_stage.is_none(), "one stage owner per launch");
        assert!(
            stage.is_absolute()
                && fs::symlink_metadata(stage)
                    .expect("owned stage metadata")
                    .file_type()
                    .is_dir()
        );
        self.owned_stage = Some((stage.to_path_buf(), Vec::new()));
    }

    pub(crate) fn record_stage_processes(&mut self, pids: &[u32]) {
        assert!(
            !pids.is_empty()
                && pids
                    .iter()
                    .all(|pid| *pid > 1 && *pid != std::process::id())
        );
        assert!(self.host_group.is_some_and(|group| pids.contains(&group)));
        assert!(self.bun_group.is_some_and(|group| pids.contains(&group)));
        self.owned_stage.as_mut().expect("retained exact stage").1 = pids.to_vec();
    }

    /// Caller must first assert actual product exit, family/window/session
    /// cleanup and stage absence. This never performs cleanup itself.
    pub(crate) fn disarm_after_verified_cleanup(&mut self) {
        assert!(self.cli.is_none(), "child status must already be reaped");
        assert!(
            self.live_output.is_none(),
            "captured output must already be joined"
        );
        self.host_group = None;
        self.bun_group = None;
        self.owned_stage = None;
        self.observed_groups.clear();
        self.observed_processes.clear();
    }
}

impl Drop for ShippingLaunchCleanup {
    fn drop(&mut self) {
        // Reader custody ends synchronously even when Child/family cleanup follows
        // an assertion failure. This never treats cancelled output as evidence.
        drop(self.live_output.take());
        if let Some(cli) = self.cli.as_mut()
            && cli.try_wait().ok().flatten().is_none()
        {
            let _ = cli.kill();
            let _ = cli.wait();
        }
        let observed_groups = self.observed_group_ids();
        for group in &observed_groups {
            let _ = signal_process_group("-TERM", *group);
        }
        if let Some((stage, pids)) = &self.owned_stage {
            // This is emergency harness cleanup after a failed test, never
            // evidence that the product cleaned its stage. Missing proof or
            // timeout retains the exact stage for the root operator.
            let proven = !pids.is_empty()
                && std::panic::catch_unwind(|| {
                    let deadline = Instant::now() + PROCESS_DEADLINE;
                    for pid in pids {
                        await_process_gone_before(*pid, deadline);
                    }
                    for group in &observed_groups {
                        assert!(
                            process_ids("pgid", *group).is_empty(),
                            "owned group {group} still has a process"
                        );
                    }
                })
                .is_ok();
            if proven {
                match fs::remove_dir_all(stage) {
                    Ok(()) => eprintln!("KELD_HARNESS_FALLBACK_STAGE_REMOVED {}", stage.display()),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => eprintln!(
                        "KELD_HARNESS_STAGE_RETAINED {} removal_failed={error}",
                        stage.display()
                    ),
                }
            } else {
                eprintln!(
                    "KELD_HARNESS_STAGE_RETAINED {} process_family_quiescence_unproved",
                    stage.display()
                );
            }
        }
    }
}

impl ShippingDevCycle {
    pub(crate) fn launch(fixture: &ProductFixture, helper: &Path, name: &str) -> Self {
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
        assert_eq!(native_windows(cli_pid, TITLE), [] as [u32; 0]);
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

    pub(crate) fn evidence(&self) -> String {
        format!(
            "{}/{}/{}/{}/{}",
            self.cli_pid, self.host_pid, self.guardian_pid, self.bun_pid, self.descendant_pid
        )
    }

    fn trace_phase(&self, action: &str, signal: Option<&str>) {
        eprintln!(
            "KELD_DEV_CYCLE_ACTION action={action} signal={signal:?} roles={}",
            self.evidence()
        );
    }

    pub(crate) fn crash_and_recover(&mut self) {
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

    pub(crate) fn kill_cli_and_expect_lease_shutdown(&mut self) {
        self.trace_phase("kill-cli", Some("KILL"));
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

    pub(crate) fn kill_cli_and_expect_recovered_lease_shutdown(&mut self) {
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

    pub(crate) fn signal_cli_group_and_expect_lease_shutdown(
        &mut self,
        signal_name: &str,
        number: i32,
    ) {
        self.trace_phase("signal-cli-group", Some(signal_name));
        let status = signal_group_and_await_exit(
            self.cli.as_mut().expect("live shipping CLI"),
            self.cli_pid,
            signal_name,
            Instant::now() + EVENT_DEADLINE,
        );
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

    pub(crate) fn self_terminate_and_expect_verbatim_error(&mut self) {
        self.trace_phase("app-self-terminate", None);
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

    pub(crate) fn quit_and_expect_success(&mut self) {
        self.trace_phase("orderly-quit", None);
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

    pub(crate) fn assert_group_gone(&mut self) {
        eprintln!(
            "KELD_DEV_AWAIT_GONE role=host pid={} roles={}",
            self.host_pid,
            self.evidence()
        );
        await_process_gone(self.host_pid);
        eprintln!(
            "KELD_DEV_AWAIT_GONE role=guardian pid={} roles={}",
            self.guardian_pid,
            self.evidence()
        );
        await_process_gone(self.guardian_pid);
        eprintln!(
            "KELD_DEV_AWAIT_GONE role=bun pid={} roles={}",
            self.bun_pid,
            self.evidence()
        );
        await_process_gone(self.bun_pid);
        eprintln!(
            "KELD_DEV_AWAIT_GONE role=descendant pid={} roles={}",
            self.descendant_pid,
            self.evidence()
        );
        await_process_gone(self.descendant_pid);
        assert_eq!(native_windows(self.host_pid, TITLE), [] as [u32; 0]);
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

use crate::support::DARK_BG;
use crate::support::EVENT_DEADLINE;
use crate::support::FORWARDED_LOG;
use crate::support::MARKER;
use crate::support::TITLE;
use crate::support::control::accept_before;
use crate::support::control::parse_pid;
use crate::support::control::read_control_line;
use crate::support::control::wait_child_output;
use crate::support::lease_descriptors::assert_lease_descriptor_ownership;
use crate::support::native_window::NativeWindowExpectation;
use crate::support::native_window::NativeWindowScope;
use crate::support::native_window::native_windows;
use crate::support::native_window::query_native_windows;
use crate::support::process::await_process_gone;
use crate::support::process::parent_process;
use crate::support::process::process_exists;
use crate::support::process::process_group;
use crate::support::process::signal_process_group;
use crate::support::product::ProductFixture;
use crate::support::recovery_cycle::RecoveryGeneration;
use crate::support::renderer::Beacon;
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
}

impl ShippingLaunchCleanup {
    pub(crate) fn new(cli: Child) -> Self {
        Self {
            cli: Some(cli),
            host_group: None,
            bun_group: None,
        }
    }

    pub(crate) fn record_authenticated_groups(&mut self, host_group: u32, bun_group: u32) {
        let test_group = process_group(std::process::id());
        self.host_group = (host_group != 0 && host_group != test_group).then_some(host_group);
        self.bun_group = (bun_group != 0 && bun_group != test_group).then_some(bun_group);
    }

    pub(crate) fn release(mut self) -> Child {
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

    pub(crate) fn evidence(&self) -> String {
        format!(
            "{}/{}/{}/{}/{}",
            self.cli_pid, self.host_pid, self.guardian_pid, self.bun_pid, self.descendant_pid
        )
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

    pub(crate) fn self_terminate_and_expect_verbatim_error(&mut self) {
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

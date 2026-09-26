use crate::support::DARK_BG;
use crate::support::EVENT_DEADLINE;
use crate::support::MARKER;
use crate::support::TITLE;
use crate::support::control::accept_before;
use crate::support::control::parse_pid;
use crate::support::native_window::NativeWindowObserver;
use crate::support::native_window::compile_native_window_census;
use crate::support::process::parent_process;
use crate::support::process::process_group;
use crate::support::process::signal_process_group;
use crate::support::renderer::Beacon;
use crate::support::unix_descriptors::unix_sockets_not_inherited_from_harness;
use std::fs;
use std::io::BufRead;
use std::io::BufReader;
use std::os::unix::net::UnixListener;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::path::PathBuf;
use std::process::Child;
use std::process::ChildStdin;
use std::process::Command;
use std::process::Output;
use std::process::Stdio;
use std::sync::OnceLock;
use std::thread;
use std::time::Instant;

pub(crate) fn dev_stage_count(project: &Path) -> usize {
    fs::read_dir(project.join(".keld/dev"))
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_dir())
        .count()
}

pub(crate) struct ProductFixture {
    pub(crate) root: tempfile::TempDir,
    pub(crate) project: PathBuf,
    pub(crate) link_source: String,
    pub(crate) harness: &'static str,
    pub(crate) native_census: OnceLock<PathBuf>,
}

impl ProductFixture {
    pub(crate) fn new(name: &str) -> Self {
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
            harness: include_str!("../../../fixtures/t1b_harness.ts"),
            native_census: OnceLock::new(),
        }
    }

    pub(crate) fn observe_initial_window(&self) -> NativeWindowObserver {
        let executable = self
            .native_census
            .get_or_init(|| compile_native_window_census(self.root.path()));
        NativeWindowObserver::arm(executable)
    }

    pub(crate) fn stage(&self) -> keld_cli::boot::DevBootStage {
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

    pub(crate) fn launch_cycle(&self, cycle: &str) -> LiveCycle {
        self.launch_cycle_inner(cycle)
    }

    pub(crate) fn launch_leased_cycle(&self, cycle: &str) -> (LiveCycle, ChildStdin) {
        let mut cycle = self.launch_cycle_inner(cycle);
        let lease = cycle.dev_lease_writer.take().expect("leased cycle writer");
        (cycle, lease)
    }

    pub(crate) fn launch_cycle_inner(&self, cycle: &str) -> LiveCycle {
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

pub(crate) struct LiveCycle {
    pub(crate) host: Option<Child>,
    pub(crate) dev_lease_writer: Option<ChildStdin>,
    pub(crate) host_pid: u32,
    pub(crate) guardian_pid: u32,
    pub(crate) bun_pid: u32,
    pub(crate) descendant_pid: u32,
    pub(crate) session_dir: PathBuf,
    pub(crate) control_reader: BufReader<UnixStream>,
    pub(crate) control_writer: UnixStream,
    pub(crate) beacon: Option<Beacon>,
    pub(crate) presentation: Option<NativeWindowObserver>,
    pub(crate) group_gone: bool,
}

impl LiveCycle {
    pub(crate) fn assert_live_product(&mut self) {
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

    pub(crate) fn next_line(&mut self) -> String {
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

    pub(crate) fn expect_line(&mut self, expected: &str) {
        assert_eq!(self.next_line(), expected);
    }

    pub(crate) fn wait_host(&mut self) -> Output {
        drop(self.dev_lease_writer.take());
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

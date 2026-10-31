//! Existing canonical lifecycle-fixture composition for the two GH1037 arms.

use crate::support::EVENT_DEADLINE;
use crate::support::control::{accept_before, parse_pid, read_control_line, wait_child_output};
use crate::support::dev_cycle::ShippingLaunchCleanup;
use crate::support::process::{await_process_gone, parent_process, process_group};
use crate::support::product::ProductFixture;
use std::io::{BufReader, Write as _};
use std::os::unix::net::{UnixListener, UnixStream};
use std::os::unix::process::CommandExt as _;
use std::path::Path;
use std::process::{ChildStdin, Command, Stdio};
use std::time::Instant;

pub(super) struct BoundRole {
    cleanup: ShippingLaunchCleanup,
    lease: ChildStdin,
    reader: BufReader<UnixStream>,
    writer: UnixStream,
    pub(super) host_pid: u32,
    pub(super) guardian_pid: u32,
    pub(super) bun_pid: u32,
}

/// Both sibling arms use this one command shape. Fresh stage/control paths
/// differ, but no environment or lifecycle policy is added only to a control.
pub(super) fn host_command(stage: &keld_cli::boot::DevBootStage, control_path: &Path) -> Command {
    let mut command = Command::new(stage.host());
    command
        .process_group(0)
        .current_dir(stage.root())
        .env("KELD_DEV_LEASE", "stdin-v1")
        .env("KELD_T1B_CONTROL", control_path)
        .env("KELD_T4_SKIP_DESCENDANT", "1")
        .env("KELD_T2_EXIT_ON_LINK_EOF", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}

impl BoundRole {
    pub(super) fn launch(fixture: &ProductFixture, stage: &keld_cli::boot::DevBootStage) -> Self {
        let control_path = fixture.root.path().join("bound.sock");
        let listener = UnixListener::bind(&control_path).expect("owned canonical fixture control");
        listener
            .set_nonblocking(true)
            .expect("bounded fixture accept");
        let child = host_command(stage, &control_path)
            .spawn()
            .expect("shipping no-flag Host");
        let mut cleanup = ShippingLaunchCleanup::new(child);
        let host = cleanup.cli.as_mut().expect("Host cleanup owner");
        let host_pid = host.id();
        let lease = host.stdin.take().expect("retain dev lease through exit");
        let control = accept_before(&listener, Instant::now() + EVENT_DEADLINE);
        control
            .set_read_timeout(Some(EVENT_DEADLINE))
            .expect("bounded fixture read");
        let mut reader = BufReader::new(control.try_clone().expect("fixture reader"));
        let hello = read_control_line(&mut reader);
        let fields: Vec<_> = hello.split_whitespace().collect();
        assert_eq!(fields.first().copied(), Some("HELLO"), "{hello}");
        let bun_pid = parse_pid(fields.get(1).copied(), &hello);
        let guardian_pid = parent_process(bun_pid);
        cleanup.record_authenticated_groups(process_group(host_pid), process_group(bun_pid));
        for expected in ["DESCENDANT 0", "READY", "ECHO1", "ECHO2"] {
            assert_eq!(read_control_line(&mut reader), expected);
        }
        eprintln!(
            "GH1037_BOUND host={host_pid} guardian={guardian_pid} bun={bun_pid} HELLO READY ECHO1 ECHO2"
        );
        Self {
            cleanup,
            lease,
            reader,
            writer: control,
            host_pid,
            guardian_pid,
            bun_pid,
        }
    }

    pub(super) fn quit(mut self, stage: &keld_cli::boot::DevBootStage) -> String {
        self.writer
            .write_all(b"QUIT\n")
            .expect("authenticated fixture Quit");
        assert_eq!(read_control_line(&mut self.reader), "QUIT_REPLY");
        assert_eq!(read_control_line(&mut self.reader), "LINK_EOF");
        drop(self.writer);
        let output = wait_child_output(self.cleanup.release(), EVENT_DEADLINE);
        drop(self.lease);
        assert!(
            output.status.success(),
            "Host orderly exit failed: {output:?}"
        );
        await_process_gone(self.bun_pid);
        await_process_gone(self.guardian_pid);
        assert!(!stage.root().exists(), "healthy Host left private stage");
        String::from_utf8(output.stderr).expect("complete successful Host capture")
    }
}

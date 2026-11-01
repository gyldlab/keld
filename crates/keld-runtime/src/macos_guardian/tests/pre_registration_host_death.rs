// Halt the registration writer after the child exists, but before its group
// record can reach the host. EOF must still revoke resources and reap the
// isolated group: registration is not a precondition for Guardian cleanup.
use std::io::{self, Read as _, Write as _};
use std::os::unix::process::ExitStatusExt as _;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

use nix::sys::signal::kill;
use nix::unistd::Pid;

use super::super::process_group::{
    GroupIdentity, terminate_attested_group, validate_guardian_child_group,
};
use super::super::{REGISTRATION_LEN, REGISTRATION_MAGIC, run_with_registration};
use super::{assert_gone, liveness_pipe, long_running_command};

mod cleanup_authority;
mod deadline_tests;

struct PausedGroupRegistration {
    entered: mpsc::Sender<u32>,
    release: mpsc::Receiver<()>,
}

impl io::Write for PausedGroupRegistration {
    fn write(&mut self, record: &[u8]) -> io::Result<usize> {
        if record.len() != REGISTRATION_LEN || record[..4] != REGISTRATION_MAGIC {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "unexpected group registration record",
            ));
        }
        let pid = u32::from_be_bytes(record[4..].try_into().map_err(io::Error::other)?);
        self.entered.send(pid).map_err(io::Error::other)?;
        self.release.recv().map_err(io::Error::other)?;
        Ok(record.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn host_eof_before_group_registration_reaps_spawned_child() {
    let (reader, writer) = liveness_pipe();
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::channel();
    let revoked = Arc::new(AtomicBool::new(false));
    let revoke_for_guardian = Arc::clone(&revoked);
    let worker = thread::spawn(move || {
        let outcome = run_with_registration(
            long_running_command(),
            reader,
            PausedGroupRegistration {
                entered: entered_tx,
                release: release_rx,
            },
            |_| Ok(()),
            move || {
                revoke_for_guardian.store(true, Ordering::SeqCst);
                Ok(())
            },
        );
        done_tx.send(outcome).expect("send Guardian result");
    });

    let pid = entered_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("child spawned and Guardian blocked before registration write");
    let raw_pid = i32::try_from(pid).expect("group id fits signed PID");
    assert_eq!(
        nix::unistd::getpgid(Some(Pid::from_raw(raw_pid)))
            .expect("spawned group leader exists")
            .as_raw(),
        raw_pid,
        "child must be an isolated process-group leader before registration"
    );
    assert!(
        matches!(done_rx.try_recv(), Err(TryRecvError::Empty)),
        "Guardian must remain blocked while the host is still live"
    );

    // Simulates host SIGKILL's sole liveness effect: all host-owned writer
    // descriptors close before the Guardian reports the new group to its host.
    drop(writer);
    release_tx.send(()).expect("release held registration");
    let report = done_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("Guardian must observe pre-registration host EOF")
        .expect("Guardian must clean up a group despite pre-registration EOF");
    worker.join().expect("Guardian worker joins");
    assert_eq!(report.leader_pid(), pid);
    assert_eq!(report.leader_status().signal(), Some(9));
    assert!(
        revoked.load(Ordering::SeqCst),
        "registered resources revoked"
    );
    assert_gone(Some(pid));
}

// Unlike the in-process pipe-close test, this proof actually SIGKILLs the
// process holding the only liveness writer. A separate guardian process is
// frozen in its registration writer, before any group record reaches a host.
const RACE_ROLE_ENV: &str = "KELD_TEST_GUARDIAN_PRE_REG_ROLE";
const RACE_SOCKET_ENV: &str = "KELD_TEST_GUARDIAN_PRE_REG_SOCKET";
const RACE_ROLE_TEST: &str =
    "macos_guardian::tests::pre_registration_host_death::pre_registration_sigkill_role";

// The host role is deliberately SIGKILLed before it can wait on its spawned
// Guardian. The real Guardian is a separate process and remains responsible
// for reaping its own isolated Bun group after host death.
#[allow(clippy::zombie_processes)]
#[test]
fn pre_registration_sigkill_role() {
    let Some(role) = std::env::var_os(RACE_ROLE_ENV) else {
        return;
    };
    let socket = std::path::PathBuf::from(
        std::env::var_os(RACE_SOCKET_ENV).expect("pre-registration control socket"),
    );
    match role.to_str().expect("UTF-8 pre-registration role") {
        "host" => {
            let mut command = pre_registration_role_command("guardian", &socket);
            command.stdin(Stdio::piped());
            let mut guardian = command.spawn().expect("spawn independent Guardian");
            let _only_host_writer = guardian.stdin.take().expect("host owns liveness writer");
            thread::park(); // Controller kills this process; no ordinary Drop or EOF injection.
        }
        "guardian" => {
            // The child owns a live descendant before it can register its group.
            // A group cleanup that kills only the leader must fail this proof.
            let marker = socket.with_extension("descendant.pid");
            let mut command = Command::new("/bin/sh");
            command
                .args([
                    "-c",
                    r#"/bin/sleep 60 & echo $! > "$1"; wait"#,
                    "keld-prereg",
                ])
                .arg(marker)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            let report = run_with_registration(
                command,
                io::stdin().lock(),
                SocketPausedGroupRegistration {
                    socket: socket.clone(),
                },
                |_| Ok(()),
                || Ok(()),
            )
            .expect("Guardian reaps group after real host SIGKILL before registration");
            assert_eq!(report.leader_status().signal(), Some(9));
            let mut stream = std::os::unix::net::UnixStream::connect(socket)
                .expect("report independent Guardian completion");
            writeln!(stream, "REAPED {}", report.leader_pid())
                .expect("publish Guardian cleanup after group wait");
        }
        _ => panic!("unknown pre-registration process role"),
    }
}

fn pre_registration_role_command(role: &str, socket: &std::path::Path) -> Command {
    let mut command = Command::new(std::env::current_exe().expect("current test executable"));
    command
        .args(["--exact", RACE_ROLE_TEST, "--nocapture"])
        .env(RACE_ROLE_ENV, role)
        .env(RACE_SOCKET_ENV, socket)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit());
    command
}

struct SocketPausedGroupRegistration {
    socket: std::path::PathBuf,
}

impl io::Write for SocketPausedGroupRegistration {
    fn write(&mut self, record: &[u8]) -> io::Result<usize> {
        if record.len() != REGISTRATION_LEN || record[..4] != REGISTRATION_MAGIC {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "unexpected pre-registration process record",
            ));
        }
        let pid = u32::from_be_bytes(record[4..].try_into().map_err(io::Error::other)?);
        let mut stream = std::os::unix::net::UnixStream::connect(&self.socket)?;
        writeln!(stream, "PAUSED {pid} {}", std::process::id())?;
        stream.set_read_timeout(Some(Duration::from_secs(10)))?;
        let mut resume = [0_u8; 1];
        stream.read_exact(&mut resume)?;
        if resume != *b"R" {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "wrong release"));
        }
        Ok(record.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct PreRegistrationProcessCleanup {
    host: Option<std::process::Child>,
    group_birth: Option<GroupIdentity>,
}

impl Drop for PreRegistrationProcessCleanup {
    fn drop(&mut self) {
        // An owned Child is the only host-kill authority. Closing its writer
        // causes Guardian EOF, including when the PAUSED receipt never arrived.
        // No socket/marker PID may authorize a later Guardian/descendant kill.
        if let Some(host) = self.host.as_mut() {
            if matches!(host.try_wait(), Ok(None)) {
                let _ = host.kill();
            }
            let _ = host.wait();
        }
        // Capture the original group birth while Guardian and leader are live.
        // If no proof arrived before a panic, fail closed rather than adopting
        // the current owner of an old PID. Host EOF remains the first rescue.
        if let Some(group) = self.group_birth.take() {
            let _ = terminate_attested_group(group);
        }
    }
}

fn require_pre_registration_event_deadline(deadline: Instant) {
    assert!(
        Instant::now() < deadline,
        "control event exceeded original deadline before complete newline"
    );
}

fn receive_pre_registration_event(
    listener: &std::os::unix::net::UnixListener,
    deadline: Instant,
) -> (std::os::unix::net::UnixStream, String) {
    receive_pre_registration_event_with_accept_hook(listener, deadline, || {})
}

// Test-owned receipt pins the accepted-stream deadline cut independently of
// scheduler delays. It does not observe or change product Guardian behavior.
fn receive_pre_registration_event_with_accept_hook(
    listener: &std::os::unix::net::UnixListener,
    deadline: Instant,
    on_accept: impl FnOnce(),
) -> (std::os::unix::net::UnixStream, String) {
    receive_pre_registration_event_with_read_hooks(listener, deadline, on_accept, |_| {}, |_| {})
}

// Test-only callbacks expose the real byte boundary: a pre-read callback runs
// after the caller's deadline check, and a post-read callback runs only after
// a native one-byte read succeeds. Normal callers keep both callbacks empty.
fn receive_pre_registration_event_with_read_hooks(
    listener: &std::os::unix::net::UnixListener,
    deadline: Instant,
    on_accept: impl FnOnce(),
    mut on_checked_read: impl FnMut(usize),
    mut on_native_byte: impl FnMut(u8),
) -> (std::os::unix::net::UnixStream, String) {
    loop {
        require_pre_registration_event_deadline(deadline);
        match listener.accept() {
            Ok((mut stream, _)) => {
                on_accept();
                // The same caller-owned absolute deadline covers accept and
                // every byte, including the final newline. macOS can reject
                // SO_RCVTIMEO on these Unix sockets.
                stream
                    .set_nonblocking(true)
                    .expect("bound event reader via nonblocking stream");
                let mut event = Vec::new();
                loop {
                    require_pre_registration_event_deadline(deadline);
                    on_checked_read(event.len());
                    let mut byte = [0_u8; 1];
                    match stream.read(&mut byte) {
                        Ok(1) => {
                            on_native_byte(byte[0]);
                            require_pre_registration_event_deadline(deadline);
                            if byte[0] == b'\n' {
                                break;
                            }
                            event.push(byte[0]);
                            assert!(event.len() <= 128, "control event exceeded 128 bytes");
                        }
                        Ok(0) => panic!("controller event closed before its newline"),
                        Err(error)
                            if matches!(
                                error.kind(),
                                io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                            ) =>
                        {
                            thread::yield_now();
                        }
                        Err(error) => panic!("read control event: {error}"),
                        Ok(_) => unreachable!("one-byte event read"),
                    }
                }
                let event = String::from_utf8(event).expect("control event is UTF-8");
                return (stream, event);
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                thread::yield_now();
            }
            Err(error) => panic!("accept process event: {error}"),
        }
    }
}

#[test]
fn pre_registration_real_host_sigkill_reaps_spawned_group() {
    let temp = tempfile::tempdir().expect("private socket directory");
    let socket = temp.path().join("prereg.sock");
    let listener = std::os::unix::net::UnixListener::bind(&socket)
        .expect("bind pre-registration test control");
    listener
        .set_nonblocking(true)
        .expect("bound controller event poll");
    let host = pre_registration_role_command("host", &socket)
        .spawn()
        .expect("spawn isolated host process");
    let mut cleanup = PreRegistrationProcessCleanup {
        host: Some(host),
        group_birth: None,
    };

    let (mut pending_registration, paused) =
        receive_pre_registration_event(&listener, Instant::now() + Duration::from_secs(10));
    let mut fields = paused.split_whitespace();
    assert_eq!(fields.next(), Some("PAUSED"), "unexpected event: {paused}");
    let group_pid: u32 = fields
        .next()
        .expect("group id")
        .parse()
        .expect("valid group id");
    let guardian_pid: u32 = fields
        .next()
        .expect("guardian id")
        .parse()
        .expect("valid guardian id");
    assert!(fields.next().is_none(), "unexpected event suffix: {paused}");
    // The PAUSED writer is still blocked on this live connection. Before
    // SIGKILLing the host or admitting a cleanup signal, pin the actual native
    // group leader's birth and its direct-Guardian parent relationship.
    cleanup.group_birth = Some(
        validate_guardian_child_group(group_pid, guardian_pid)
            .expect("paused native group must be the original Guardian's direct child"),
    );
    let marker = socket.with_extension("descendant.pid");
    let descendant_deadline = Instant::now() + Duration::from_secs(10);
    let descendant_pid = loop {
        if let Ok(contents) = std::fs::read_to_string(&marker)
            && let Ok(pid) = contents.trim().parse::<u32>()
        {
            break pid;
        }
        assert!(
            Instant::now() < descendant_deadline,
            "descendant must start before host dies"
        );
        thread::sleep(Duration::from_millis(2));
    };
    let group = Pid::from_raw(i32::try_from(group_pid).expect("group id fits PID"));
    let descendant = Pid::from_raw(i32::try_from(descendant_pid).expect("descendant id fits PID"));
    assert_eq!(
        nix::unistd::getpgid(Some(descendant)).expect("live descendant has a group"),
        group,
        "descendant must be enrolled in the isolated group before host death"
    );
    assert_eq!(
        nix::unistd::getpgid(Some(group)).expect("live isolated group leader"),
        group,
        "registration must still be blocked after group creation"
    );
    assert!(
        kill(
            Pid::from_raw(i32::try_from(guardian_pid).expect("guardian id fits PID")),
            None
        )
        .is_ok(),
        "independent Guardian must be alive before host death"
    );

    // Retain the original Child RAII even if kill/wait/assert panics.
    let host = cleanup.host.as_mut().expect("controller owns host child");
    host.kill().expect("SIGKILL only the real host");
    assert_eq!(
        host.wait().expect("reap SIGKILLed host").signal(),
        Some(9),
        "real host death must be caused by SIGKILL"
    );
    pending_registration
        .write_all(b"R")
        .expect("resume Guardian after host is confirmed dead");
    let (_notification, finished) =
        receive_pre_registration_event(&listener, Instant::now() + Duration::from_secs(10));
    assert_eq!(finished, format!("REAPED {group_pid}"));
    assert_gone(Some(group_pid));
    let disappearance_deadline = Instant::now() + Duration::from_secs(5);
    while kill(descendant, None).is_ok() {
        assert!(
            Instant::now() < disappearance_deadline,
            "pre-registration descendant survived group cleanup"
        );
        thread::sleep(Duration::from_millis(2));
    }
    cleanup.group_birth = None;
}

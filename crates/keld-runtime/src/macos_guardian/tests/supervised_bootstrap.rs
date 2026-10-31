use std::io::{self, Write as _};
use std::os::unix::process::ExitStatusExt as _;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use nix::sys::signal::{Signal, kill};
use nix::unistd::Pid;

use super::super::{
    REGISTRATION_ENV, RuntimeError, SUPERVISED_QUIT_ACK, SupervisedGuardianPreparer,
    lifecycle_error, run, run_supervised_with_registration, run_with_registration,
};
use super::test_support::{assert_gone, liveness_pipe, long_running_command};

fn await_stopped_child(pid: u32, deadline: Instant) {
    loop {
        let output = Command::new("/bin/ps")
            .args(["-o", "state=", "-p", &pid.to_string()])
            .output()
            .expect("inspect stopped child state");
        if String::from_utf8(output.stdout)
            .expect("process state is UTF-8")
            .trim()
            .starts_with('T')
        {
            break;
        }
        assert!(Instant::now() < deadline, "child never stopped");
        std::thread::yield_now();
    }
}

struct AckSender(mpsc::Sender<[u8; SUPERVISED_QUIT_ACK.len()]>);

impl io::Write for AckSender {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let ack: [u8; SUPERVISED_QUIT_ACK.len()] = bytes.try_into().map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidData, "unexpected acknowledgment size")
        })?;
        self.0.send(ack).map_err(io::Error::other)?;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn missing_private_registration_bootstrap_creates_no_child() {
    let temp = tempfile::tempdir().expect("temporary marker directory");
    let marker = temp.path().join("spawned");
    let mut command = Command::new("/bin/sh");
    command
        .args(["-c", "printf spawned > \"$1\"", "keld-guardian-test"])
        .arg(&marker);
    let (reader, _writer) = liveness_pipe();
    let revoked = Arc::new(AtomicBool::new(false));
    let revoked_in_callback = Arc::clone(&revoked);
    let error = run(command, reader, move || {
        revoked_in_callback.store(true, Ordering::SeqCst);
        Ok(())
    })
    .expect_err("missing authenticated registration must fail before child spawn");
    assert!(error.to_string().contains(REGISTRATION_ENV), "{error}");
    assert!(!marker.exists(), "invalid bootstrap must create no child");
    assert!(revoked.load(Ordering::SeqCst));
}

#[test]
fn empty_program_is_rejected_before_spawn() {
    let (reader, _writer) = liveness_pipe();
    let error = run_with_registration(Command::new(""), reader, io::sink(), |_| Ok(()), || Ok(()))
        .expect_err("empty guardian command must fail");
    let rendered = error.to_string();
    assert!(rendered.contains("KELD-RUNTIME-003"), "{rendered}");
    assert!(rendered.contains("child program is empty"), "{rendered}");
}

#[test]
fn spawn_failure_revokes_prepared_resources() {
    let (reader, _writer) = liveness_pipe();
    let revoked = Arc::new(AtomicBool::new(false));
    let revoked_in_callback = Arc::clone(&revoked);
    let error = run_with_registration(
        Command::new("/definitely/missing/keld-bun"),
        reader,
        io::sink(),
        |_| Ok(()),
        move || {
            revoked_in_callback.store(true, Ordering::SeqCst);
            Ok(())
        },
    )
    .expect_err("spawn failure must fail closed");
    assert!(matches!(error, RuntimeError::Spawn(_)), "{error}");
    assert!(
        revoked.load(Ordering::SeqCst),
        "spawn failure must revoke every prepared resource"
    );
}

#[test]
fn private_registration_authority_is_not_inherited_by_the_child() {
    let (reader, writer) = liveness_pipe();
    let mut command = Command::new("/bin/sh");
    command
            .args([
                "-c",
                "if [ -z \"${KELD_INTERNAL_MACOS_GUARDIAN_REGISTRATION+x}\" ]; then exec /usr/bin/tail -f /dev/null; else exit 77; fi",
            ])
            .env(REGISTRATION_ENV, "must-not-reach-child");
    let report = run_with_registration(
        command,
        reader,
        io::sink(),
        move |_| {
            drop(writer);
            Ok(())
        },
        || Ok(()),
    )
    .expect("guardian cleanup with stripped private authority");
    assert_eq!(
        report.leader_status().signal(),
        Some(9),
        "a child that inherited the private registration value exits 77"
    );
}

#[test]
fn supervised_guardian_uses_one_supervisor_child_and_clean_host_shutdown() {
    let (reader, writer) = liveness_pipe();
    let registered = Arc::new(Mutex::new(None));
    let registered_in_callback = Arc::clone(&registered);
    let revoked = Arc::new(AtomicBool::new(false));
    let revoked_in_callback = Arc::clone(&revoked);
    let report = run_supervised_with_registration(
        reader,
        io::sink(),
        io::sink(),
        || Ok(long_running_command()),
        move |pid| {
            *registered_in_callback.lock().expect("pid lock") = Some(pid);
            drop(writer);
            Ok(())
        },
        move || {
            revoked_in_callback.store(true, Ordering::SeqCst);
            Ok(())
        },
    )
    .expect("host-driven supervised guardian cleanup");
    assert_eq!(report.0, registered.lock().expect("pid lock").expect("pid"));
    assert_eq!(report.1.self_termination_count, 0);
    assert!(revoked.load(Ordering::SeqCst));
    assert_gone(*registered.lock().expect("pid lock"));
}

#[test]
fn supervised_status_zero_is_retained_as_typed_self_termination() {
    let (reader, writer) = liveness_pipe();
    let revoked = Arc::new(AtomicBool::new(false));
    let revoked_in_callback = Arc::clone(&revoked);
    let error = run_supervised_with_registration(
        reader,
        io::sink(),
        io::sink(),
        || {
            let mut command = Command::new("/bin/sh");
            command
                .args(["-c", "exit 0"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            Ok(command)
        },
        move |_| {
            drop(writer);
            Ok(())
        },
        move || {
            revoked_in_callback.store(true, Ordering::SeqCst);
            Ok(())
        },
    )
    .expect_err("status-zero self-termination must not become host success");
    assert!(error.to_string().contains("KELD-RUNTIME-012"), "{error}");
    assert!(revoked.load(Ordering::SeqCst));
}

#[test]
fn supervised_command_preparation_failure_revokes_before_child() {
    let (reader, _writer) = liveness_pipe();
    let revoked = Arc::new(AtomicBool::new(false));
    let revoked_in_callback = Arc::clone(&revoked);
    let error = run_supervised_with_registration(
        reader,
        io::sink(),
        io::sink(),
        || {
            Err(lifecycle_error(
                "test supervised command preparation",
                io::Error::other("intentional preparation failure"),
            ))
        },
        |_| panic!("preparation failure must not spawn a child"),
        move || {
            revoked_in_callback.store(true, Ordering::SeqCst);
            Ok(())
        },
    )
    .expect_err("command preparation failure must fail closed");
    assert!(
        error
            .to_string()
            .contains("intentional preparation failure")
    );
    assert!(revoked.load(Ordering::SeqCst));
}

#[test]
fn dropping_unhanded_supervised_preparer_revokes_once() {
    let revoked = Arc::new(AtomicBool::new(false));
    let revoked_in_callback = Arc::clone(&revoked);
    let (pid_tx, _pid_rx) = mpsc::sync_channel(1);
    let preparer = SupervisedGuardianPreparer {
        command_factory: Some(|| Ok::<Command, RuntimeError>(Command::new("/usr/bin/true"))),
        registration: Some(io::sink()),
        child_spawned: Some(|_: u32| Ok::<(), io::Error>(())),
        revoke_registered_resources: Some(move || {
            assert!(
                !revoked_in_callback.swap(true, Ordering::SeqCst),
                "unhanded resources revoked more than once"
            );
            Ok(())
        }),
        pid_tx,
    };

    drop(preparer);
    assert!(revoked.load(Ordering::SeqCst));
}

#[test]
fn supervised_self_termination_returns_while_host_writer_is_still_live() {
    let (reader, writer) = liveness_pipe();
    let (pid_tx, pid_rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::channel();
    let deadline = Instant::now() + Duration::from_secs(5);
    let worker = std::thread::spawn(move || {
        let result = run_supervised_with_registration(
            reader,
            io::sink(),
            io::sink(),
            || {
                let mut command = Command::new("/bin/sh");
                command.args(["-c", "kill -STOP $$; exit 0"]);
                Ok(command)
            },
            move |pid| pid_tx.send(pid).map_err(io::Error::other),
            || Ok(()),
        );
        done_tx.send(result).expect("report guardian result");
    });
    // Enrollment must precede the exit under test. Confirm the shell has
    // actually stopped so an early SIGCONT cannot race its SIGSTOP.
    let pid = pid_rx
        .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .expect("self-terminating child registered");
    await_stopped_child(pid, deadline);
    kill(
        Pid::from_raw(i32::try_from(pid).expect("pid fits i32")),
        Signal::SIGCONT,
    )
    .expect("let registered child exit zero");
    let result = done_rx.recv_timeout(deadline.saturating_duration_since(Instant::now()));
    // The result must arrive with the writer live; close it before asserting
    // so a failed observation still lets the guardian clean up its child.
    drop(writer);
    worker.join().expect("guardian worker joins");
    let error = result
        .expect("self-termination must wake the guardian while host is live")
        .expect_err("unrequested status-zero exit is fatal");
    assert!(error.to_string().contains("KELD-RUNTIME-012"), "{error}");
}

#[test]
fn supervised_accepted_quit_does_not_record_status_zero_as_unrequested() {
    let (reader, mut writer) = liveness_pipe();
    let (pid_tx, pid_rx) = mpsc::channel();
    let (ack_tx, ack_rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        let result = run_supervised_with_registration(
            reader,
            io::sink(),
            AckSender(ack_tx),
            || {
                let mut command = Command::new("/bin/sh");
                command.args(["-c", "kill -STOP $$; exit 0"]);
                Ok(command)
            },
            move |pid| pid_tx.send(pid).map_err(io::Error::other),
            || Ok(()),
        );
        done_tx.send(result).expect("report guardian result");
    });
    let pid = pid_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("stopped child registered");

    let stopped_deadline = Instant::now() + Duration::from_secs(5);
    await_stopped_child(pid, stopped_deadline);

    writer.write_all(b"Q").expect("accept Quit before reply");
    assert_eq!(
        ack_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("guardian applied accepted-Quit attribution"),
        SUPERVISED_QUIT_ACK
    );
    kill(
        Pid::from_raw(i32::try_from(pid).expect("pid fits i32")),
        Signal::SIGCONT,
    )
    .expect("let accepted-Quit child exit zero");
    let deadline = Instant::now() + Duration::from_secs(5);
    while kill(
        Pid::from_raw(i32::try_from(pid).expect("pid fits i32")),
        None,
    )
    .is_ok()
    {
        assert!(Instant::now() < deadline, "accepted-Quit child stayed live");
        std::thread::yield_now();
    }
    drop(writer);

    let report = done_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("guardian finishes after host EOF")
        .expect("accepted Quit is a clean supervised shutdown");
    assert_eq!(report.1.self_termination_count, 0);
    worker.join().expect("guardian worker joins");
}

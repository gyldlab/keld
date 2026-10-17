//! Crash-closed handoff of an isolated child group before it can exec Bun.
//!
//! The Guardian's ordinary `Command::spawn` waits for the child to exec. A
//! minimal, async-signal-safe pre-exec gate reports the kernel child PID to a
//! separate Guardian thread. That thread first registers the new process group
//! with the host, then permits exec. A `SIGKILL`ed Guardian closes the only
//! permit writer: the child exits without running Bun, even if the host never
//! receives a registration record.

use std::io;
#[cfg(test)]
use std::io::{Read as _, Write as _};
use std::os::fd::{AsRawFd as _, OwnedFd};
use std::os::unix::process::CommandExt as _;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Instant;

use nix::libc;
use nix::unistd::pipe;

use super::{GUARDIAN_SHUTDOWN_DEADLINE, RuntimeError, lifecycle_error, validate_group_leader};

const RELEASE: u8 = b'R';
const DENY: u8 = b'D';

pub(super) struct SpawnAdmission {
    thread: JoinHandle<Result<u32, RuntimeError>>,
    discovered_group: Arc<Mutex<Option<u32>>>,
}

impl SpawnAdmission {
    // This is the sole new macOS fork/exec safety seam. The pre-exec closure
    // uses only async-signal-safe syscalls; it MUST NOT allocate or lock.
    #[allow(unsafe_code)]
    pub(super) fn install(
        command: &mut Command,
        register: impl FnOnce(u32) -> Result<(), RuntimeError> + Send + 'static,
    ) -> Result<Self, RuntimeError> {
        let (pid_reader, pid_writer) = pipe().map_err(|source| gate_error("PID pipe", source))?;
        let (permit_reader, permit_writer) =
            pipe().map_err(|source| gate_error("permit pipe", source))?;
        for fd in [&pid_reader, &pid_writer, &permit_reader, &permit_writer] {
            rustix::io::fcntl_setfd(fd, rustix::io::FdFlags::CLOEXEC).map_err(|source| {
                lifecycle_error("macOS guardian spawn gate descriptor", source.into())
            })?;
        }
        // The reporter and the pre-exec closure share ownership of the *same*
        // parent-only descriptors, not duplicated OS descriptors. The closure
        // retains these exact fd identities until fork, even if the reporter
        // times out first. In the child it closes the actual inherited permit
        // writer before polling, so Guardian SIGKILL immediately produces EOF.
        let pid_reader = Arc::new(pid_reader);
        let permit_writer = Arc::new(permit_writer);
        let discovered_group = Arc::new(Mutex::new(None));
        let group_for_thread = Arc::clone(&discovered_group);
        let reporter_pid_reader = Arc::clone(&pid_reader);
        let reporter_permit_writer = Arc::clone(&permit_writer);
        let thread = thread::Builder::new()
            .name("keld-guardian-pre-exec-registration".to_owned())
            .spawn(move || {
                let pid = match read_child_pid(&reporter_pid_reader) {
                    Ok(pid) => pid,
                    Err(error) => {
                        let _ = rustix::io::write(&*reporter_permit_writer, &[DENY]);
                        return Err(error);
                    }
                };
                *group_for_thread
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(pid);
                let registered = (|| {
                    validate_group_leader(pid)?;
                    #[cfg(test)]
                    pause_before_registration_for_crash_test(pid)?;
                    register(pid)
                })();
                if let Err(error) = registered {
                    // The command's pre-exec closure retains this writer until
                    // spawn returns. An explicit non-permit byte wakes the child
                    // immediately on a rejected registration; waiting for EOF
                    // would otherwise deadlock until the five-second poll timeout.
                    let _ = rustix::io::write(&*reporter_permit_writer, &[DENY]);
                    return Err(error);
                }
                // Only a host-owned, validated REGISTERED acknowledgment releases
                // this child into the Bun executable. Otherwise drop closes the
                // permit pipe, and the pre-exec child exits with no Bun started.
                let written =
                    rustix::io::write(&*reporter_permit_writer, &[RELEASE]).map_err(|source| {
                        lifecycle_error("macOS guardian child exec permit", source.into())
                    })?;
                if written != 1 {
                    return Err(lifecycle_error(
                        "macOS guardian child exec permit",
                        io::Error::new(io::ErrorKind::WriteZero, "incomplete child exec permit"),
                    ));
                }
                Ok(pid)
            })
            .map_err(|source| lifecycle_error("macOS guardian registration worker", source))?;

        // SAFETY: after fork the closure uses only async-signal-safe libc calls
        // (close/getpid/write/poll/read/_exit), fixed stack storage and no locks
        // or allocations. The shared OwnedFd instances pin the parent-only fd
        // numbers until fork. Close those actual ends (not duplicates): otherwise
        // SIGKILL of the Guardian would not produce permit EOF in this child.
        unsafe {
            command.pre_exec(move || {
                libc::close(pid_reader.as_raw_fd());
                libc::close(permit_writer.as_raw_fd());
                let pid = libc::getpid().to_be_bytes();
                let wrote = libc::write(pid_writer.as_raw_fd(), pid.as_ptr().cast(), pid.len());
                libc::close(pid_writer.as_raw_fd());
                if wrote != 4 {
                    libc::_exit(125);
                }
                let mut readiness = libc::pollfd {
                    fd: permit_reader.as_raw_fd(),
                    events: libc::POLLIN,
                    revents: 0,
                };
                let ready = libc::poll(&raw mut readiness, 1, 5_000);
                if ready != 1 {
                    libc::_exit(125);
                }
                let mut permit = 0_u8;
                let received = libc::read(permit_reader.as_raw_fd(), (&raw mut permit).cast(), 1);
                libc::close(permit_reader.as_raw_fd());
                if received != 1 || permit != RELEASE {
                    libc::_exit(125);
                }
                Ok(())
            });
        }
        Ok(Self {
            thread,
            discovered_group,
        })
    }

    /// The live group is recorded before attempting the host exchange. This
    /// remains available for fail-closed cleanup even when the ACK is rejected.
    pub(super) fn finish(self) -> (Option<u32>, Result<u32, RuntimeError>) {
        let result = self
            .thread
            .join()
            .map_err(|_| {
                lifecycle_error(
                    "macOS guardian child exec registration",
                    io::Error::other("registration worker panicked before exec permission"),
                )
            })
            .and_then(|result| result);
        let pid = *self
            .discovered_group
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        (pid, result)
    }
}

fn gate_error(phase: &'static str, source: nix::errno::Errno) -> RuntimeError {
    lifecycle_error(phase, io::Error::from_raw_os_error(source as i32))
}

#[allow(unsafe_code)] // read-only poll on the retained, owned pre-exec report pipe
fn read_child_pid(reader: &OwnedFd) -> Result<u32, RuntimeError> {
    let deadline = Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE;
    let fd = reader.as_raw_fd();
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let timeout = i32::try_from(remaining.as_millis()).unwrap_or(i32::MAX);
        if timeout == 0 {
            return Err(lifecycle_error(
                "macOS guardian pre-exec PID receipt",
                io::Error::new(
                    io::ErrorKind::TimedOut,
                    "child did not report its PID before exec",
                ),
            ));
        }
        let mut pollfd = libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: poll only reads and writes our initialized stack pollfd;
        // the OwnedFd keeps its target alive until this function returns.
        let ready = unsafe { libc::poll(&raw mut pollfd, 1, timeout) };
        if ready == 0 {
            continue;
        }
        if ready < 0 {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(lifecycle_error("macOS guardian pre-exec PID poll", error));
        }
        let mut pid = [0_u8; 4];
        // One atomic four-byte pipe write by the child is the entire record.
        // No other thread reads this end; a partial or empty record fails closed.
        let received = loop {
            match rustix::io::read(reader, &mut pid) {
                Ok(count) => break count,
                Err(rustix::io::Errno::INTR) => {}
                Err(source) => {
                    return Err(lifecycle_error(
                        "macOS guardian pre-exec PID read",
                        source.into(),
                    ));
                }
            }
        };
        if received != pid.len() {
            return Err(lifecycle_error(
                "macOS guardian pre-exec PID read",
                io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "incomplete pre-exec PID record",
                ),
            ));
        }
        let pid = i32::from_be_bytes(pid);
        return u32::try_from(pid).map_err(|source| {
            lifecycle_error(
                "macOS guardian pre-exec PID",
                io::Error::new(io::ErrorKind::InvalidData, source),
            )
        });
    }
}

// The gate runs only in Rust's test build and pauses the *real* pre-exec
// reporter. The Guardian process is SIGKILLed by a separate macOS test
// process while Bun is demonstrably still blocked before its first exec.
#[cfg(test)]
fn pause_before_registration_for_crash_test(pid: u32) -> Result<(), RuntimeError> {
    use std::os::unix::net::UnixStream;

    const ENV: &str = "KELD_TEST_GUARDIAN_PREEXEC_GATE";
    let Some(socket) = std::env::var_os(ENV) else {
        return Ok(());
    };
    let mut stream = UnixStream::connect(socket)
        .map_err(|source| lifecycle_error("macOS test pre-exec crash gate", source))?;
    writeln!(stream, "SPAWNED {pid} {}", std::process::id())
        .map_err(|source| lifecycle_error("macOS test pre-exec crash receipt", source))?;
    stream
        .set_read_timeout(Some(GUARDIAN_SHUTDOWN_DEADLINE))
        .map_err(|source| lifecycle_error("macOS test pre-exec crash deadline", source))?;
    let mut release = [0_u8; 1];
    stream
        .read_exact(&mut release)
        .map_err(|source| lifecycle_error("macOS test pre-exec crash release", source))?;
    if release != *b"R" {
        return Err(lifecycle_error(
            "macOS test pre-exec crash release",
            io::Error::new(io::ErrorKind::InvalidData, "wrong test gate release"),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejected_registration_denies_child_exec_before_poll_timeout() {
        let temp = tempfile::tempdir().expect("private rejected-admission fixture");
        let marker = temp.path().join("must-not-execute");
        let mut command = Command::new("/usr/bin/touch");
        command.arg(&marker).process_group(0);
        let admission = SpawnAdmission::install(&mut command, |_| {
            Err(lifecycle_error(
                "test guardian admission refusal",
                io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "intentional denied registration",
                ),
            ))
        })
        .expect("install real pre-exec refusal gate");
        let started = Instant::now();
        let spawn = command.spawn();
        drop(command);
        let (pid, registration) = admission.finish();
        assert!(
            pid.is_some(),
            "actual child reported its PID before refusal"
        );
        let error = registration.expect_err("host refused group registration");
        assert!(
            error
                .to_string()
                .contains("intentional denied registration"),
            "{error}"
        );
        if let Ok(mut child) = spawn {
            let status = child.wait().expect("reap refused real child");
            assert!(
                !status.success(),
                "denied registration cannot execute /usr/bin/touch"
            );
        }
        assert!(
            !marker.exists(),
            "denied registration executed untrusted child code"
        );
        assert!(
            started.elapsed() < std::time::Duration::from_secs(3),
            "rejected child waited for five-second pre-exec timeout rather than prompt denial"
        );
    }
}

use crate::support::PROCESS_DEADLINE;
use crate::support::control::wait_child_output;
use crate::support::dev_cycle::ShippingLaunchCleanup;
use std::fs;
use std::io::BufRead;
use std::io::BufReader;
use std::os::unix::process::CommandExt as _;
use std::os::unix::process::ExitStatusExt as _;
use std::path::PathBuf;
use std::process::Command;
use std::process::Stdio;
use std::thread;
use std::time::Instant;

unsafe extern "C" {
    #[link_name = "kill"]
    fn kill_process(pid: i32, signal: i32) -> i32;
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

pub(crate) fn await_process_state(pid: u32, wanted: char) {
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

pub(crate) fn kill_pid(pid: u32) {
    let status = Command::new("/bin/kill")
        .args(["-KILL", &pid.to_string()])
        .status()
        .expect("kill one process");
    assert!(status.success(), "kill {pid}: {status:?}");
}

pub(crate) fn validate_process_group(group: u32) -> std::io::Result<i32> {
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

pub(crate) fn signal_process_group(signal: &str, group: u32) -> std::io::Result<()> {
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

pub(crate) fn parent_process(pid: u32) -> u32 {
    process_number(pid, "ppid")
}

pub(crate) fn process_group(pid: u32) -> u32 {
    process_number(pid, "pgid")
}

pub(crate) fn process_number(pid: u32, field: &str) -> u32 {
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

pub(crate) fn process_exists(pid: u32) -> bool {
    Command::new("/bin/kill")
        .args(["-0", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

pub(crate) fn await_process_gone(pid: u32) {
    let deadline = Instant::now() + PROCESS_DEADLINE;
    while process_exists(pid) {
        assert!(
            Instant::now() < deadline,
            "process {pid} survived cleanup; observed_state={}",
            process_state(pid)
        );
        thread::yield_now();
    }
}

/// The `ps` state letters for `pid`, or why they could not be read.
pub(crate) fn process_state(pid: u32) -> String {
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

pub(crate) fn session_dirs_for(pid: u32) -> Vec<PathBuf> {
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

// SDK sys/event.h ABI and constants, local to this independent macOS test census.
#[repr(C)]
#[derive(Default)]
struct ProcessKevent {
    ident: u64,
    filter: i16,
    flags: u16,
    fflags: u32,
    data: i64,
    udata: u64,
    ext: [u64; 2],
}

#[repr(C)]
struct ProcessKeventTimeout {
    seconds: std::os::raw::c_long,
    nanoseconds: std::os::raw::c_long,
}

unsafe extern "C" {
    fn kqueue() -> i32;
    fn kevent64(
        queue: i32,
        changes: *const ProcessKevent,
        change_count: i32,
        events: *mut ProcessKevent,
        event_count: i32,
        flags: u32,
        timeout: *const ProcessKeventTimeout,
    ) -> i32;
    #[link_name = "fcntl"]
    fn process_observer_fcntl(fd: i32, command: i32, ...) -> i32;
}

/// Observes one fixture-admitted PID's kernel exit status without reaping it.
/// The guardian remains the sole Bun parent/reaper; `OwnedFd` closes this census.
pub(crate) struct ProcessExitObserver {
    pid: u32,
    queue: std::os::fd::OwnedFd,
    observed_status: Option<std::process::ExitStatus>,
}

impl ProcessExitObserver {
    const FILTER: i16 = -5; // EVFILT_PROC
    const EXIT_FLAGS: u32 = 0x8000_0000 | 0x0400_0000; // NOTE_EXIT | NOTE_EXITSTATUS
    const ERROR_FLAG: u16 = 0x4000; // EV_ERROR

    pub(crate) fn arm(pid: u32) -> Self {
        Self::try_arm(pid).expect("fixture-admitted native process exit observer")
    }

    /// Try to register one real kernel process before its exit. A vanished or
    /// inaccessible nonchild is an unproved watch, never proof of liveness.
    pub(crate) fn try_arm(pid: u32) -> std::io::Result<Self> {
        use std::os::fd::{AsRawFd as _, FromRawFd as _};
        if pid <= 1 || pid == std::process::id() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "invalid native process exit witness target",
            ));
        }
        if std::mem::size_of::<ProcessKevent>() != 48
            || std::mem::align_of::<ProcessKevent>() != 8
            || std::mem::size_of::<ProcessKeventTimeout>() != 16
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "native process observer ABI is not supported",
            ));
        }
        // SAFETY: kqueue takes no arguments and returns a new owned descriptor.
        let fd = unsafe { kqueue() };
        if fd < 0 {
            return Err(std::io::Error::last_os_error());
        }
        // SAFETY: successful kqueue returned a valid descriptor owned only here.
        let queue = unsafe { std::os::fd::OwnedFd::from_raw_fd(fd) };
        // SAFETY: variadic fcntl receives this valid owned fd, F_SETFD=2 and the
        // promoted integer FD_CLOEXEC=1. No other owner's descriptor is changed.
        let cloexec = unsafe { process_observer_fcntl(queue.as_raw_fd(), 2, 1_i32) };
        if cloexec != 0 {
            return Err(std::io::Error::last_os_error());
        }
        let change = ProcessKevent {
            ident: u64::from(pid),
            filter: Self::FILTER,
            flags: 0x0001 | 0x0004 | 0x0040, // EV_ADD | EV_ENABLE | EV_RECEIPT
            fflags: Self::EXIT_FLAGS,
            ..ProcessKevent::default()
        };
        let mut receipt = ProcessKevent::default();
        // SAFETY: SDK-matched repr(C) values live for this call. Counts are one;
        // queue is owned/live. KEVENT_FLAG_IMMEDIATE=1 obtains a registration
        // receipt without waiting. No ptrace, signal, waitpid or target mutation.
        let registered = unsafe {
            kevent64(
                queue.as_raw_fd(),
                &raw const change,
                1,
                &raw mut receipt,
                1,
                1,
                std::ptr::null(),
            )
        };
        if registered < 0 {
            return Err(std::io::Error::last_os_error());
        }
        if registered != 1
            || receipt.ident != u64::from(pid)
            || receipt.filter != Self::FILTER
            || receipt.flags & Self::ERROR_FLAG == 0
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "native exit watcher registration receipt is invalid",
            ));
        }
        if receipt.data != 0 {
            let native_errno = i32::try_from(receipt.data).map_err(|_| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "native exit registration error is outside its SDK representation",
                )
            })?;
            return Err(std::io::Error::from_raw_os_error(native_errno));
        }
        Ok(Self {
            pid,
            queue,
            observed_status: None,
        })
    }

    /// Read an already armed process-exit event without delaying Host shutdown.
    /// No event never proves the original process is still live.
    pub(crate) fn try_poll_exit(&mut self) -> std::io::Result<Option<std::process::ExitStatus>> {
        use std::os::fd::AsRawFd as _;
        if let Some(status) = self.observed_status {
            return Ok(Some(status));
        }
        let no_wait = ProcessKeventTimeout {
            seconds: 0,
            nanoseconds: 0,
        };
        let mut event = ProcessKevent::default();
        // SAFETY: the registered, CLOEXEC queue remains owned by this observer.
        // An empty changelist and a zero timespec make this one native event
        // read nonblocking. The SDK-matched one-event buffer is valid here;
        // this neither signals nor reaps the observed process.
        let received = unsafe {
            kevent64(
                self.queue.as_raw_fd(),
                std::ptr::null(),
                0,
                &raw mut event,
                1,
                0,
                &raw const no_wait,
            )
        };
        if received == 0 {
            return Ok(None);
        }
        if received < 0 {
            let error = std::io::Error::last_os_error();
            return if error.kind() == std::io::ErrorKind::Interrupted {
                Ok(None)
            } else {
                Err(error)
            };
        }
        if received != 1
            || event.ident != u64::from(self.pid)
            || event.filter != Self::FILTER
            || event.flags & Self::ERROR_FLAG != 0
            || event.fflags & Self::EXIT_FLAGS != Self::EXIT_FLAGS
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "native exit event failed original registration and status checks",
            ));
        }
        let raw = i32::try_from(event.data).map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "native process exit status did not fit its SDK representation",
            )
        })?;
        let status = std::process::ExitStatus::from_raw(raw);
        self.observed_status = Some(status);
        Ok(Some(status))
    }

    pub(crate) fn wait(self) -> std::process::ExitStatus {
        use std::os::fd::AsRawFd as _;
        if let Some(status) = self.observed_status {
            return status;
        }
        let deadline = Instant::now() + PROCESS_DEADLINE;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(
                !remaining.is_zero(),
                "no kernel exit status for PID {}",
                self.pid
            );
            let timeout = ProcessKeventTimeout {
                seconds: remaining.as_secs().try_into().expect("bounded seconds"),
                nanoseconds: remaining.subsec_nanos().into(),
            };
            let mut event = ProcessKevent::default();
            // SAFETY: the owned queue, writable one-event repr(C) buffer and
            // SDK-matched timespec stay live. Null changelist/count zero makes
            // this a read-only wait for an already registered PID event. This
            // consumes a census event, never the guardian's child wait status.
            let observed = unsafe {
                kevent64(
                    self.queue.as_raw_fd(),
                    std::ptr::null(),
                    0,
                    &raw mut event,
                    1,
                    0,
                    &raw const timeout,
                )
            };
            if observed < 0 {
                let error = std::io::Error::last_os_error();
                if error.kind() == std::io::ErrorKind::Interrupted {
                    continue;
                }
                panic!("kernel exit census for {} failed: {error}", self.pid);
            }
            assert_eq!(observed, 1, "kernel exit census timeout for {}", self.pid);
            assert_eq!(event.ident, u64::from(self.pid));
            assert_eq!(event.filter, Self::FILTER);
            assert_eq!(
                event.flags & Self::ERROR_FLAG,
                0,
                "exit event errno {}",
                event.data
            );
            assert_eq!(event.fflags & Self::EXIT_FLAGS, Self::EXIT_FLAGS);
            let raw = i32::try_from(event.data).expect("kernel wait status fits i32");
            let status = std::process::ExitStatus::from_raw(raw);
            eprintln!(
                "KEL143_RETIRED_OS_STATUS pid={} raw={raw} exit_code={:?} signal={:?} observer_is_parent=false",
                self.pid,
                status.code(),
                status.signal()
            );
            return status;
        }
    }
}

use crate::support::PROCESS_DEADLINE;
use crate::support::control::wait_child_output;
use crate::support::dev_cycle::ShippingLaunchCleanup;
use nix::libc;
use std::fs;
use std::io::BufReader;
use std::io::{BufRead, Read, Write};
use std::os::unix::process::CommandExt as _;
use std::os::unix::process::ExitStatusExt as _;
use std::path::PathBuf;
use std::process::Child;
use std::process::Command;
use std::process::Stdio;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

unsafe extern "C" {
    #[link_name = "kill"]
    fn kill_process(pid: i32, signal: i32) -> i32;
}

/// Test-only authority retained from an original *live* kernel process-group
/// leader, not from a PID reported in an event or copied from another group.
/// This is an observation/recheck, not a kernel-pinned group handle: a leader
/// that has disappeared makes fallback group signaling fail closed.
#[derive(Clone, Copy, Debug)]
pub(crate) struct NativeGroupBirth {
    group: u32,
    real_uid: libc::uid_t,
    start_sec: u64,
    start_usec: u64,
}

impl NativeGroupBirth {
    pub(crate) const fn group(self) -> u32 {
        self.group
    }
}

#[allow(unsafe_code)] // test-only fixed-size read-only Darwin process observation
fn native_group_birth(group: u32) -> Option<(NativeGroupBirth, u32)> {
    let raw = i32::try_from(group).ok()?;
    if raw <= 1 {
        return None;
    }
    // SAFETY: Darwin proc_bsdinfo has only value fields, the kernel retains no
    // Rust pointer, and the output buffer has precisely the requested size.
    let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
    let size = i32::try_from(std::mem::size_of::<libc::proc_bsdinfo>()).ok()?;
    // SAFETY: read-only process query fills the exact-size writable buffer.
    let count =
        unsafe { libc::proc_pidinfo(raw, libc::PROC_PIDTBSDINFO, 0, (&raw mut info).cast(), size) };
    if count != size || info.pbi_pid != group || info.pbi_pgid != group {
        return None;
    }
    Some((
        NativeGroupBirth {
            group,
            real_uid: info.pbi_ruid,
            start_sec: info.pbi_start_tvsec,
            start_usec: info.pbi_start_tvusec,
        },
        info.pbi_ppid,
    ))
}

/// Verify expected native parentage at capture time, independently of test
/// fixture messages, and exclude the harness' own group from this authority.
pub(crate) fn retain_parented_group_birth(group: u32, parent: u32) -> NativeGroupBirth {
    let (birth, actual_parent) = native_group_birth(group)
        .expect("original native group leader must be live when recording cleanup authority");
    assert_eq!(
        actual_parent, parent,
        "group leader must have the owned parent"
    );
    assert_ne!(group, process_group(std::process::id()));
    birth
}

/// Numeric PGID is never signal authority without the *earlier* original
/// birth; a group leader missing after cleanup is not grounds to signal by
/// reusable group number. The check/kill race is not kernel pinned.
pub(crate) fn signal_attested_group(
    signal: &str,
    original: NativeGroupBirth,
) -> std::io::Result<()> {
    let same_birth = native_group_birth(original.group).is_some_and(|(observed, _)| {
        observed.group == original.group
            && observed.real_uid == original.real_uid
            && observed.start_sec == original.start_sec
            && observed.start_usec == original.start_usec
    });
    if !same_birth {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "original native group leader disappeared or changed birth; refuse group signal",
        ));
    }
    signal_process_group(signal, original.group)
}

// This sentinel is a real, freshly spawned, isolated OS process group, not an
// arbitrary host PID. Its retained Child is the final independent rescue on
// regression failure, even when ShippingLaunchCleanup drops in a panic.
struct OwnedOutsider(Child);

impl Drop for OwnedOutsider {
    fn drop(&mut self) {
        if matches!(self.0.try_wait(), Ok(None)) {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}

fn native_outsider() -> OwnedOutsider {
    let outsider = OwnedOutsider(
        Command::new("/bin/cat")
            .process_group(0)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("owned isolated outsider"),
    );
    let outsider_pid = outsider.0.id();
    assert_eq!(process_group(outsider_pid), outsider_pid);
    assert_ne!(process_group(std::process::id()), outsider_pid);
    outsider
}

fn assert_outsider_echo(outsider: &mut OwnedOutsider) {
    outsider
        .0
        .stdin
        .as_mut()
        .expect("outsider stdin retained")
        .write_all(b"still-here\n")
        .expect("outsider survived cleanup");
    let mut stdout = outsider.0.stdout.take().expect("outsider stdout retained");
    let (tx, rx) = mpsc::channel();
    let reader = thread::spawn(move || {
        let mut bytes = [0_u8; 11];
        let outcome = stdout.read_exact(&mut bytes).map(|()| bytes);
        let _ = tx.send(outcome);
    });
    let received = rx.recv_timeout(Duration::from_secs(2));
    if received.is_err() {
        let _ = outsider.0.kill();
    }
    reader
        .join()
        .expect("outsider read terminates after native rescue");
    assert!(
        matches!(received, Ok(Ok(bytes)) if bytes == *b"still-here\n"),
        "unverified ShippingLaunchCleanup marker signaled the independent outsider"
    );
}

#[test]
fn shipping_cleanup_unverified_group_marker_cannot_signal_outsider() {
    let mut outsider = native_outsider();
    let cli = Command::new("/usr/bin/true")
        .spawn()
        .expect("harmless completed CLI owner");
    let mut cleanup = ShippingLaunchCleanup::new(cli);
    cleanup.host_group = Some(outsider.0.id()); // Unverified marker: NO authority.
    drop(cleanup);
    assert_outsider_echo(&mut outsider);
}

#[test]
fn shipping_cleanup_replaced_group_marker_cannot_borrow_original_birth() {
    let mut original = native_outsider();
    let mut outsider = native_outsider();
    let cli = Command::new("/usr/bin/true")
        .spawn()
        .expect("harmless completed CLI owner");
    let mut cleanup = ShippingLaunchCleanup::new(cli);
    cleanup.record_host_group(original.0.id(), std::process::id());
    cleanup.host_group = Some(outsider.0.id()); // Replaced after valid capture.
    drop(cleanup);
    assert_outsider_echo(&mut outsider);
    assert_outsider_echo(&mut original);
}

#[test]
fn shipping_cleanup_wrong_original_birth_cannot_signal_outsider() {
    let mut outsider = native_outsider();
    let original = retain_parented_group_birth(outsider.0.id(), std::process::id());
    let wrong = NativeGroupBirth {
        start_usec: original.start_usec.wrapping_add(1),
        ..original
    };
    let refused = signal_attested_group("-TERM", wrong)
        .expect_err("altered original kernel birth cannot authorize group signaling");
    assert_eq!(refused.kind(), std::io::ErrorKind::PermissionDenied);
    assert_outsider_echo(&mut outsider);
}

#[test]
fn shipping_cleanup_unparented_birth_cannot_gain_group_authority() {
    let mut outsider = native_outsider();
    let refused = std::panic::catch_unwind(|| {
        retain_parented_group_birth(outsider.0.id(), u32::MAX);
    });
    assert!(
        refused.is_err(),
        "foreign parent cannot enroll fixture authority"
    );
    assert_outsider_echo(&mut outsider);
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
    cleanup.record_host_group(host_pid, std::process::id());
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
    cleanup.record_bun_group(bun_pid, std::process::id());
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
}

impl ProcessExitObserver {
    const FILTER: i16 = -5; // EVFILT_PROC
    const EXIT_FLAGS: u32 = 0x8000_0000 | 0x0400_0000; // NOTE_EXIT | NOTE_EXITSTATUS
    const ERROR_FLAG: u16 = 0x4000; // EV_ERROR

    pub(crate) fn arm(pid: u32) -> Self {
        use std::os::fd::{AsRawFd as _, FromRawFd as _};
        assert!(pid > 1 && pid != std::process::id(), "fixture-admitted PID");
        assert_eq!(std::mem::size_of::<ProcessKevent>(), 48, "SDK kevent64 ABI");
        assert_eq!(
            std::mem::align_of::<ProcessKevent>(),
            8,
            "SDK kevent64 alignment"
        );
        assert_eq!(
            std::mem::size_of::<ProcessKeventTimeout>(),
            16,
            "SDK timespec ABI"
        );
        // SAFETY: kqueue takes no arguments and returns a new owned descriptor.
        let fd = unsafe { kqueue() };
        assert!(fd >= 0, "kqueue: {}", std::io::Error::last_os_error());
        // SAFETY: successful kqueue returned a valid descriptor owned only here.
        let queue = unsafe { std::os::fd::OwnedFd::from_raw_fd(fd) };
        // SAFETY: variadic fcntl receives this valid owned fd, F_SETFD=2 and the
        // promoted integer FD_CLOEXEC=1. No other owner's descriptor is changed.
        let cloexec = unsafe { process_observer_fcntl(queue.as_raw_fd(), 2, 1_i32) };
        assert_eq!(
            cloexec,
            0,
            "observer CLOEXEC: {}",
            std::io::Error::last_os_error()
        );
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
        assert_eq!(
            registered,
            1,
            "exit watcher receipt: {}",
            std::io::Error::last_os_error()
        );
        assert_eq!(receipt.ident, u64::from(pid));
        assert_eq!(receipt.filter, Self::FILTER);
        assert_ne!(receipt.flags & Self::ERROR_FLAG, 0, "EV_RECEIPT missing");
        assert_eq!(receipt.data, 0, "exit watcher registration errno for {pid}");
        Self { pid, queue }
    }

    pub(crate) fn wait(self) -> std::process::ExitStatus {
        use std::os::fd::AsRawFd as _;
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

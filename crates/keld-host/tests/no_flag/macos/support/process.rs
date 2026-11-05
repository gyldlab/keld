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

#[test]
fn shipping_launch_timeout_retains_group_custody_until_actual_descendant_cleanup() {
    // A real process topology, not an app/native readiness oracle. The Host
    // stand-in owns a separate Bun group whose shell owns one real descendant.
    const HOST: &str = r"
import subprocess, sys
bun = subprocess.Popen(['/bin/sh', '-c', 'sleep 60 & echo $!; wait'],
                       start_new_session=True, stdout=subprocess.PIPE, text=True)
descendant = int(bun.stdout.readline())
print(bun.pid, descendant, flush=True)
sys.stdin.buffer.read(1)
";
    let root = tempfile::tempdir().expect("owned cleanup fault fixture");
    let stage = root.path().join("exact-owned-stage");
    fs::create_dir(&stage).expect("owned disposable stage");
    let outside = root.path().join("outside-sentinel");
    fs::write(&outside, b"outside survives").expect("independent scope sentinel");
    let child = Command::new("/usr/bin/python3")
        .args(["-u", "-c", HOST])
        .process_group(0)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("actual hanging process-family fixture");
    let host = child.id();
    let mut cleanup = ShippingLaunchCleanup::new(child);
    cleanup.host_group = Some(host);
    cleanup.retain_owned_stage(&stage);
    let retained = cleanup.cli.as_mut().expect("retained Host child");
    let _lease = retained
        .stdin
        .take()
        .expect("keep stand-in live until timeout");
    let mut line = String::new();
    BufReader::new(
        retained
            .stdout
            .take()
            .expect("actual process census handoff"),
    )
    .read_line(&mut line)
    .expect("stand-in child PIDs");
    let pids: Vec<u32> = line
        .split_whitespace()
        .map(|value| value.parse().expect("PID"))
        .collect();
    assert_eq!(pids.len(), 2);
    let (bun, descendant) = (pids[0], pids[1]);
    assert_eq!(parent_process(bun), host);
    assert_eq!(parent_process(descendant), bun);
    assert_eq!(process_group(bun), bun);
    assert_eq!(process_group(descendant), bun);
    cleanup.record_authenticated_groups(process_group(host), bun);
    cleanup.record_stage_processes(&[host, bun, descendant]);
    eprintln!("KELD_HARNESS_FAULT_FAMILY host={host} bun={bun} descendant={descendant}");
    let bun_exit = ProcessExitObserver::arm(bun);
    let descendant_exit = ProcessExitObserver::arm(descendant);

    let failure = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        cleanup.wait(std::time::Duration::ZERO)
    }));
    let panic = failure.expect_err("the real blocked Host cannot finish at zero deadline");
    assert!(
        panic
            .downcast_ref::<String>()
            .is_some_and(|message| message.contains("child exceeded exit/output deadline"))
    );
    assert!(
        process_exists(bun) && process_exists(descendant),
        "direct-child reap alone must leave actual group members to the owner"
    );
    assert!(
        stage.exists(),
        "timeout must not erase a stage while its process group is live"
    );
    drop(cleanup); // fallback is harness cleanup; never a product-success oracle
    assert!(bun_exit.wait().signal().is_some());
    assert!(descendant_exit.wait().signal().is_some());
    for pid in [host, bun, descendant] {
        await_process_gone(pid);
    }
    assert!(
        !stage.exists(),
        "fallback deletes only after real group quiescence"
    );
    assert_eq!(
        fs::read(&outside).expect("outside sentinel"),
        b"outside survives"
    );
}

#[test]
fn shipping_launch_fallback_retains_stage_without_complete_family_proof() {
    let root = tempfile::tempdir().expect("owned incomplete-census fixture");
    let stage = root.path().join("exact-owned-stage");
    fs::create_dir(&stage).expect("owned disposable stage");
    let child = Command::new("/bin/sleep")
        .arg("60")
        .process_group(0)
        .spawn()
        .expect("live child");
    let pid = child.id();
    let mut cleanup = ShippingLaunchCleanup::new(child);
    cleanup.host_group = Some(pid);
    cleanup.retain_owned_stage(&stage);
    drop(cleanup);
    await_process_gone(pid);
    assert!(
        stage.is_dir(),
        "absence of a full family census cannot authorize fallback deletion"
    );
    // TempDir is this synthetic fixture's final independent owner. A real
    // retained producer stage is reported for the root operator to inspect.
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

/// Actual OS PIDs with one direct parent or process-group relation.
/// Returns the process table's values, never an application's PID report.
pub(crate) fn process_ids(field: &str, wanted: u32) -> Vec<u32> {
    assert!(
        matches!(field, "ppid" | "pgid"),
        "bounded process relation census"
    );
    let output = Command::new("/bin/ps")
        .args(["-axo", &format!("pid=,{field}=")])
        .output()
        .expect("inspect actual process relations");
    assert!(output.status.success(), "ps relation census: {output:?}");
    let source = String::from_utf8(output.stdout).expect("process relation UTF-8");
    source
        .lines()
        .filter_map(|line| {
            let fields: Vec<_> = line.split_whitespace().collect();
            assert_eq!(fields.len(), 2, "two numeric OS relation columns");
            let pid: u32 = fields[0].parse().expect("OS PID");
            let relation: u32 = fields[1].parse().expect("OS process relation");
            (relation == wanted).then_some(pid)
        })
        .collect()
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
    await_process_gone_before(pid, Instant::now() + PROCESS_DEADLINE);
}

/// Reuse the same disappearance oracle under a caller-owned total deadline.
pub(crate) fn await_process_gone_before(pid: u32, deadline: Instant) {
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

/// Observes one PID's kernel exit status without reaping it; `OwnedFd` closes
/// this census. For a fixture-admitted process ([`Self::arm`]) the guardian
/// remains the sole parent/reaper. For a test-owned child
/// ([`Self::arm_own_child`]) the test is the parent and reaps it after the
/// census. The evidence line records which relationship applies.
pub(crate) struct ProcessExitObserver {
    pid: u32,
    queue: std::os::fd::OwnedFd,
    observer_is_parent: bool,
}

impl ProcessExitObserver {
    const FILTER: i16 = -5; // EVFILT_PROC
    const EXIT_FLAGS: u32 = 0x8000_0000 | 0x0400_0000; // NOTE_EXIT | NOTE_EXITSTATUS
    const ERROR_FLAG: u16 = 0x4000; // EV_ERROR

    pub(crate) fn arm(pid: u32) -> Self {
        Self::arm_with_parent(pid, false)
    }

    /// Arm the census for a child this test spawned and will reap itself.
    pub(crate) fn arm_own_child(child: &std::process::Child) -> Self {
        Self::arm_with_parent(child.id(), true)
    }

    fn arm_with_parent(pid: u32, observer_is_parent: bool) -> Self {
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
        Self {
            pid,
            queue,
            observer_is_parent,
        }
    }

    pub(crate) fn wait(self) -> std::process::ExitStatus {
        self.wait_before(Instant::now() + PROCESS_DEADLINE)
    }

    /// Wait for the kernel exit status until the caller's original deadline.
    pub(crate) fn wait_before(self, deadline: Instant) -> std::process::ExitStatus {
        use std::os::fd::AsRawFd as _;
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
            // The kevent wait itself reached the deadline: same outcome.
            assert!(observed != 0, "no kernel exit status for PID {}", self.pid);
            assert_eq!(
                observed, 1,
                "kernel exit census event count for {}",
                self.pid
            );
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
                "KEL143_RETIRED_OS_STATUS pid={} raw={raw} exit_code={:?} signal={:?} observer_is_parent={}",
                self.pid,
                status.code(),
                status.signal(),
                self.observer_is_parent
            );
            return status;
        }
    }
}

#[test]
fn shipping_family_custody_survives_failure_before_operator_wait() {
    // Real Host -> Guardian -> separate Bun group. No app PID/Ready endpoint.
    const HOST: &str = r#"import subprocess, sys
guardian = subprocess.Popen([sys.executable, '-u', '-c', "import subprocess, sys\nbun = subprocess.Popen(['/bin/sleep', '60'], start_new_session=True)\nsys.stdin.buffer.read(1)\n"], stdin=subprocess.PIPE)
sys.stdin.buffer.read(1)
"#;
    let root = tempfile::tempdir().expect("owned pre-operator fault fixture");
    let stage = root.path().join("exact-owned-stage");
    fs::create_dir(&stage).expect("owned stage");
    // This final rescue guard cannot satisfy assertions below; it runs only
    // after they pass or unwind, so a mutation does not leak the real group.
    let rescue_child = Command::new("/bin/sleep")
        .arg("60")
        .process_group(0)
        .spawn()
        .expect("independent fault-test rescue owner");
    let mut rescue = ShippingLaunchCleanup::new(rescue_child);
    let child = Command::new("/usr/bin/python3")
        .args(["-u", "-c", HOST])
        .process_group(0)
        .stdin(Stdio::piped())
        .spawn()
        .expect("actual process family");
    let host = child.id();
    let mut cleanup = ShippingLaunchCleanup::new(child);
    cleanup.host_group = Some(host);
    cleanup.retain_owned_stage(&stage);
    let lease = cleanup
        .cli
        .as_mut()
        .expect("Host child")
        .stdin
        .take()
        .expect("live lease");
    let mut actual_bun = None;
    let mut actual_exit = None;
    let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        cleanup.observe_family_before_observing(
            Instant::now() + crate::support::EVENT_DEADLINE,
            |group| {
                if group == host {
                    return;
                }
                // Independent OS relation/exit witness for this first-party fixture.
                assert_eq!(parent_process(parent_process(group)), host);
                assert_eq!(process_group(group), group);
                rescue.bun_group = Some(group);
                actual_bun = Some(group);
                actual_exit = Some(ProcessExitObserver::arm(group));
                panic!("injected failure before operator wait");
            },
        );
    }));
    let panic = failed.expect_err("real group observation must reach the fault cut");
    assert_eq!(
        panic.downcast_ref::<&str>(),
        Some(&"injected failure before operator wait")
    );
    let bun = actual_bun.expect("actual descendant group selected before fault");
    assert!(
        process_exists(bun),
        "fault itself must not fake process retirement"
    );
    assert!(
        stage.is_dir(),
        "no complete snapshot exists at this fault cut"
    );
    drop(cleanup); // This is the implementation under test, not the rescue guard.
    assert!(
        actual_exit
            .expect("independent kernel exit witness")
            .wait()
            .signal()
            .is_some()
    );
    await_process_gone(host);
    await_process_gone(bun);
    assert!(
        process_ids("pgid", bun).is_empty(),
        "observed separate group stranded"
    );
    assert!(
        stage.is_dir(),
        "incomplete family proof must retain the exact stage"
    );
    drop(lease);
    rescue.bun_group = None; // Product/fallback assertions completed before rescue.
}

#[test]
fn shipping_family_census_deadline_keeps_incomplete_stage() {
    let root = tempfile::tempdir().expect("owned deadline fixture");
    let stage = root.path().join("exact-owned-stage");
    fs::create_dir(&stage).expect("owned stage");
    let child = Command::new("/bin/sleep")
        .arg("60")
        .process_group(0)
        .spawn()
        .expect("live owned child without Guardian");
    let pid = child.id();
    let mut cleanup = ShippingLaunchCleanup::new(child);
    cleanup.host_group = Some(pid);
    cleanup.retain_owned_stage(&stage);
    let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        cleanup.observe_family_before(Instant::now());
    }));
    assert!(
        failed.is_err(),
        "expired family deadline must refuse operator phase"
    );
    drop(cleanup);
    await_process_gone(pid);
    assert!(
        stage.is_dir(),
        "deadline cannot certify complete family cleanup"
    );
}

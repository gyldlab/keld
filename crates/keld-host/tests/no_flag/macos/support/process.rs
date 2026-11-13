use crate::support::PROCESS_DEADLINE;
use crate::support::control::wait_child_output;
use crate::support::dev_cycle::ShippingLaunchCleanup;
use std::collections::HashSet;
use std::fs;
use std::io::BufRead;
use std::io::BufReader;
use std::os::unix::fs::MetadataExt as _;
use std::os::unix::process::CommandExt as _;
use std::os::unix::process::ExitStatusExt as _;
use std::path::Path;
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

/// A `kb-<pid hex>-<label>-<random>` directory planted in the bootstrap's first
/// session root. `tempfile` creates it exclusively under a random suffix, so a
/// directory left by an interrupted run cannot make the plant fail, and it is
/// removed on drop.
struct PlantedSessionDir(tempfile::TempDir);

impl PlantedSessionDir {
    fn create(pid: u32, label: &str) -> Self {
        Self::create_in(pid, label, &std::env::temp_dir())
    }

    fn create_in(pid: u32, label: &str, root: &Path) -> Self {
        let dir = tempfile::Builder::new()
            .prefix(&format!("kb-{pid:x}-{label}-"))
            .tempdir_in(root)
            .expect("plant session directory");
        Self(dir)
    }

    fn path(&self) -> &Path {
        self.0.path()
    }
}

#[test]
fn session_census_ignores_a_stale_directory_from_a_reused_host_pid() {
    // macOS reuses PIDs, and a crashed or killed earlier process can leave its
    // `kb-<pid>-*` directory behind. This test runner's own PID stands in for
    // a host that inherited such a PID: the stale directory exists before the
    // "launch", so it is not this host's resource.
    let pid = std::process::id();
    let stale = PlantedSessionDir::create(pid, "stale-before-launch");
    let census = SessionDirCensus::before_launch();
    assert_eq!(
        census.dirs_for(pid),
        [] as [PathBuf; 0],
        "a directory that predates the launch was attributed to the host: {}",
        stale.path().display()
    );
    // Negative control: a directory with the host's prefix that appears after
    // the census is still attributed, so a host that leaves its own session
    // behind still fails the cleanup assertions.
    let owned = PlantedSessionDir::create(pid, "left-by-this-host");
    assert_eq!(census.dirs_for(pid), [owned.path().to_path_buf()]);
}

#[test]
fn planting_a_session_directory_survives_a_leftover_from_an_interrupted_run() {
    // Each invocation owns its own root, so another test process cannot remove
    // this synthetic stale directory while the assertion is running.
    const SYNTHETIC_PID: u32 = 0xdead_beef;
    let fixture_root = tempfile::tempdir().expect("isolated session fixture root");
    let leftover = fixture_root
        .path()
        .join(format!("kb-{SYNTHETIC_PID:x}-stale-before-launch"));
    assert_eq!(leftover.parent(), Some(fixture_root.path()));
    assert_eq!(
        leftover.file_name(),
        Some(std::ffi::OsStr::new("kb-deadbeef-stale-before-launch"))
    );
    // An interrupted run left the fixed name that `PlantedSessionDir` used to
    // create with `fs::create_dir`, which then failed before the census ran.
    fs::create_dir(&leftover).expect("leave a stale directory");
    let planted =
        PlantedSessionDir::create_in(SYNTHETIC_PID, "stale-before-launch", fixture_root.path());
    let planted_path = planted.path().to_path_buf();
    assert_eq!(
        planted_path.parent(),
        Some(fixture_root.path()),
        "plant must share isolated fixture root"
    );
    assert_ne!(
        planted_path, leftover,
        "plant reused a stale directory name"
    );
    assert!(planted_path.is_dir(), "plant did not create its directory");
    drop(planted);
    assert!(!planted_path.exists(), "plant leaked its directory on drop");
    assert!(
        leftover.is_dir(),
        "plant removed the pre-existing directory"
    );
    // fixture_root owns the stale directory and removes it on drop.
}

#[test]
fn session_census_reports_a_directory_once_when_scan_roots_overlap() {
    // `TMPDIR=/tmp` makes `temp_dir()` and the fixed `/tmp` root the same
    // directory, and on macOS `/tmp` is a symlink to `/private/tmp`.
    const SYNTHETIC_PID: u32 = 0xdead_bef0;
    let root = tempfile::tempdir().expect("scan root");
    let alias_parent = tempfile::tempdir().expect("alias parent");
    let alias = alias_parent.path().join("alias-of-root");
    std::os::unix::fs::symlink(root.path(), &alias).expect("alias the scan root");
    let census = SessionDirCensus::before_launch_in(vec![
        root.path().to_path_buf(),
        root.path().to_path_buf(),
        alias,
    ]);
    let owned = root
        .path()
        .join(format!("kb-{SYNTHETIC_PID:x}-left-by-this-host"));
    fs::create_dir(&owned).expect("create the owned directory");
    assert_eq!(census.dirs_for(SYNTHETIC_PID), [owned]);
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

/// The `kb-*` bootstrap session directories that existed before a host launch.
///
/// The bootstrap names each session directory `kb-<pid hex>-<nonce>` in one of
/// three temp roots. macOS reuses PIDs, and an earlier process that was killed
/// before cleanup leaves its directory behind, so a PID prefix alone does not
/// identify the current host's directories. A directory that already existed
/// before the launch cannot be the launched host's resource; every directory
/// with the host's prefix that appears after the census still is, including
/// one that this host leaves behind.
pub(crate) struct SessionDirCensus {
    roots: Vec<PathBuf>,
    existing: HashSet<(PathBuf, u64, u64)>,
}

impl SessionDirCensus {
    /// Records the existing session directories. Call before spawning the
    /// process that launches the host.
    pub(crate) fn before_launch() -> Self {
        Self::before_launch_in(vec![
            std::env::temp_dir(),
            PathBuf::from("/tmp"),
            PathBuf::from("/var/tmp"),
        ])
    }

    /// Records the session directories under `roots`.
    fn before_launch_in(roots: Vec<PathBuf>) -> Self {
        let existing = session_dir_entries(&roots, |_| true).collect();
        Self { roots, existing }
    }

    /// Session directories for `pid` that did not exist at the census, each
    /// reported once even when the scan roots overlap.
    pub(crate) fn dirs_for(&self, pid: u32) -> Vec<PathBuf> {
        let prefix = format!("kb-{pid:x}-");
        session_dir_entries(&self.roots, |name| name.starts_with(&prefix))
            .filter(|entry| !self.existing.contains(entry))
            .map(|(path, _, _)| path)
            .collect()
    }
}

/// `(path, device, inode)` for each `kb-*` entry under `roots` whose name
/// passes `keep`. `TMPDIR=/tmp` or a symlinked root (macOS `/tmp` is
/// `/private/tmp`) makes roots overlap, so each `(device, inode)` is yielded
/// once, under the first path that reached it.
fn session_dir_entries<'a>(
    roots: &'a [PathBuf],
    keep: impl Fn(&str) -> bool + 'a,
) -> impl Iterator<Item = (PathBuf, u64, u64)> + 'a {
    let mut seen = HashSet::new();
    roots
        .iter()
        .flat_map(|base| {
            fs::read_dir(base)
                .into_iter()
                .flatten()
                .filter_map(Result::ok)
        })
        .filter(move |entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            name.starts_with("kb-") && keep(&name)
        })
        .filter_map(|entry| {
            let metadata = fs::symlink_metadata(entry.path()).ok()?;
            Some((entry.path(), metadata.dev(), metadata.ino()))
        })
        .filter(move |&(_, device, inode)| seen.insert((device, inode)))
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

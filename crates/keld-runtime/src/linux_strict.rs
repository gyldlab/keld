//! Linux strict-profile process construction (KEL-78/T4).
//!
//! Unprivileged Bubblewrap owns user/mount/PID/network namespace creation,
//! the empty-root mount table, PID-namespace reaping, capability removal,
//! `no_new_privs`, descriptor closure, and parent-death coupling. Keld owns
//! the fixed syscall policy supplied through Bubblewrap's seccomp FD.
//! Neither component may be replaced with a best-effort fallback.

#![allow(unsafe_code)] // isolated async-signal-safe pre-exec FD ABI
#![deny(unsafe_op_in_unsafe_fn)]

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{OsStr, OsString};
use std::fs::{self, File, OpenOptions};
use std::io::{Read as _, Seek as _, SeekFrom, Write as _};
use std::os::fd::AsRawFd as _;
use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};
use std::os::unix::process::CommandExt as _;
use std::path::{Component, Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::Duration;

use nix::fcntl::OFlag;
use nix::sys::memfd::{MFdFlags, memfd_create};
use nix::unistd::pipe2;
use seccompiler::{
    BpfProgram, SeccompAction, SeccompCmpArgLen, SeccompCmpOp, SeccompCondition, SeccompFilter,
    SeccompRule, TargetArch,
};

const SANDBOX_PROGRAM: &str = "/runtime/program";
const SANDBOX_LAUNCHER: &str = "/runtime/launcher";
const SANDBOX_ROLE_ROOT: &str = "/app";
const LANDLOCK_CANARY_ROOT: &str = "/landlock-probe";
const LAUNCHER_READY_FD: std::os::fd::RawFd = 5;
const LAUNCHER_READY_FD_PATH: &str = "/proc/self/fd/5";
const LAUNCHER_READY_MAX: usize = 64;
const BUBBLEWRAP_MODE_FORBIDDEN: u32 = 0o6022;
const BUBBLEWRAP_READY_TIMEOUT: Duration = Duration::from_secs(10);
const LAUNCHER_READY_FULL: &[u8] = b"KLS1 landlock=fully-enforced\n";
const LAUNCHER_READY_PARTIAL: &[u8] = b"KLS1 landlock=partially-enforced\n";
const LAUNCHER_READY_UNAVAILABLE: &[u8] = b"KLS1 landlock=not-implemented\n";

/// Failure to construct or launch the Linux strict boundary.
#[derive(Debug)]
pub struct LinuxStrictError {
    phase: &'static str,
    detail: String,
}

impl LinuxStrictError {
    fn new(phase: &'static str, detail: impl Into<String>) -> Self {
        Self {
            phase,
            detail: detail.into(),
        }
    }
}

impl std::fmt::Display for LinuxStrictError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "KELD-RUNTIME-016: Linux strict-profile admission failed during {}: {}. \
             Do not start an uncontained replacement. Use a supported x86_64 host with \
             unprivileged user namespaces when this host cannot provide them; otherwise \
             repair the reviewed launcher, runtime mounts, seccomp policy, enabled \
             Landlock layer, readiness channel, or target. A kernel without Landlock is \
             recorded; legacy must be selected explicitly and is never an automatic fallback.",
            self.phase, self.detail
        )
    }
}

impl std::error::Error for LinuxStrictError {}

/// Landlock layer observed before the strict target was allowed to execute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinuxLandlockStatus {
    /// Every requested Landlock restriction was enforced by the kernel.
    FullyEnforced,
    /// The kernel enforced the compatible subset of the requested restrictions.
    PartiallyEnforced,
    /// The kernel does not implement Landlock; namespace, mount, capability,
    /// descriptor, and seccomp containment remain mandatory.
    NotImplemented,
}

/// Validated Linux strict-profile construction inputs.
#[derive(Debug, Clone)]
pub struct LinuxStrictProfile {
    bubblewrap: PathBuf,
    launcher: PathBuf,
    role_root: PathBuf,
    runtime_mounts: Vec<LinuxReadonlyMount>,
}

impl LinuxStrictProfile {
    /// Validates Bubblewrap, the in-sandbox launcher, and the owner-private role root.
    ///
    /// Bubblewrap must be root-owned with no privilege bit/capability. The
    /// Keld launcher must be root- or current-user-owned and immutable to other
    /// principals. Both are regular non-symlink executables. The role root is
    /// a real current-user-owned directory with exact mode `0o700`.
    ///
    /// # Errors
    ///
    /// Returns [`LinuxStrictError`] when either boundary input is missing,
    /// mutable by another principal, privileged, symlinked, or the wrong type.
    pub fn new(
        bubblewrap: &Path,
        launcher: &Path,
        role_root: &Path,
    ) -> Result<Self, LinuxStrictError> {
        let bubblewrap = validate_bubblewrap(bubblewrap)?;
        let launcher = validate_launcher(launcher)?;
        let role_root = validate_role_root(role_root)?;
        Ok(Self {
            bubblewrap,
            launcher,
            role_root,
            runtime_mounts: Vec::new(),
        })
    }

    /// Adds one reviewed host runtime file as a read-only sandbox mount.
    ///
    /// The destination must be normalized and absolute. It cannot overlap the
    /// writable role root, target slot, fresh proc/dev/tmp mounts, or another
    /// runtime destination. This is runtime/code availability, not an app
    /// filesystem grant; the exact list belongs in the profile digest.
    ///
    /// # Errors
    ///
    /// Returns [`LinuxStrictError`] for a missing/non-file source, unsafe
    /// destination, or duplicate destination.
    pub fn readonly_runtime(
        mut self,
        source: &Path,
        destination: &Path,
    ) -> Result<Self, LinuxStrictError> {
        let mount = LinuxReadonlyMount::new(source, destination)?;
        if self
            .runtime_mounts
            .iter()
            .any(|existing| existing.destination == mount.destination)
        {
            return Err(LinuxStrictError::new(
                "runtime mount",
                format!("duplicate destination {}", mount.destination.display()),
            ));
        }
        self.runtime_mounts.push(mount);
        Ok(self)
    }

    /// Builds one fail-closed strict command with an exact environment.
    ///
    /// The target is mounted read-only as `/runtime/program`; the only
    /// writable host path is the validated role root at `/app`. The child sees
    /// fresh `/proc`, minimal `/dev`, and tmpfs `/tmp`, but no host `/etc`,
    /// home, update staging, sibling role, or arbitrary filesystem tree.
    ///
    /// # Errors
    ///
    /// Returns [`LinuxStrictError`] for invalid target/environment inputs or
    /// failure to compile and serialize the Keld seccomp program.
    pub fn command(
        &self,
        program: &Path,
        args: &[OsString],
        environment: &[(OsString, OsString)],
    ) -> Result<LinuxStrictCommand, LinuxStrictError> {
        let program = validate_program(program)?;
        validate_environment(environment)?;
        let seccomp = seccomp_files()?;
        let [first, second] = seccomp.as_slice() else {
            return Err(LinuxStrictError::new(
                "seccomp construction",
                "expected exactly two policy programs",
            ));
        };
        let (readiness_reader, readiness_writer) = pipe2(OFlag::O_CLOEXEC | OFlag::O_NONBLOCK)
            .map_err(|source| {
                LinuxStrictError::new("launcher readiness pipe", source.to_string())
            })?;
        let readiness = File::from(readiness_reader);
        let readiness_writer = File::from(readiness_writer);
        let source_fds = [
            first.as_raw_fd(),
            second.as_raw_fd(),
            readiness_writer.as_raw_fd(),
        ];
        let mut command = Command::new(&self.bubblewrap);
        append_bubblewrap_mounts(
            &mut command,
            &program,
            &self.launcher,
            &self.role_root,
            &self.runtime_mounts,
        );
        append_strict_launch(&mut command, &self.runtime_mounts, environment, args);
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        // SAFETY: after fork and before exec this callback invokes only the
        // async-signal-safe dup3 and close_range syscalls. It allocates
        // nothing, touches no shared Rust state, and returns the OS error
        // through Command's existing exec-error channel. The captured array
        // is Copy data naming the two live memfds and readiness writer retained
        // below.
        unsafe {
            command.pre_exec(move || isolate_pre_exec_fds(source_fds));
        }
        Ok(LinuxStrictCommand {
            command,
            seccomp,
            readiness,
            readiness_writer,
        })
    }
}

fn append_bubblewrap_mounts(
    command: &mut Command,
    program: &Path,
    launcher: &Path,
    role_root: &Path,
    runtime_mounts: &[LinuxReadonlyMount],
) {
    command.args([
        "--unshare-user",
        "--unshare-pid",
        "--unshare-net",
        "--unshare-ipc",
        "--unshare-uts",
        "--unshare-cgroup-try",
        "--disable-userns",
        "--assert-userns-disabled",
        "--die-with-parent",
        "--new-session",
        "--clearenv",
        "--hostname",
        "keld",
        "--proc",
        "/proc",
        "--dev",
        "/dev",
        "--tmpfs",
        "/tmp",
        "--tmpfs",
        LANDLOCK_CANARY_ROOT,
        "--dir",
        "/runtime",
        "--ro-bind",
    ]);
    command.arg(program).arg(SANDBOX_PROGRAM);
    command.arg("--ro-bind").arg(launcher).arg(SANDBOX_LAUNCHER);
    let mut directories = BTreeSet::new();
    for mount in runtime_mounts {
        directories.extend(
            mount
                .destination
                .ancestors()
                .skip(1)
                .filter(|path| *path != Path::new("/"))
                .map(Path::to_path_buf),
        );
    }
    for directory in directories {
        command.arg("--dir").arg(directory);
    }
    for mount in runtime_mounts {
        command
            .arg("--ro-bind")
            .arg(&mount.source)
            .arg(&mount.destination);
    }
    command.arg("--bind").arg(role_root).arg(SANDBOX_ROLE_ROOT);
}

fn append_strict_launch(
    command: &mut Command,
    runtime_mounts: &[LinuxReadonlyMount],
    environment: &[(OsString, OsString)],
    args: &[OsString],
) {
    command.args(["--chdir", SANDBOX_ROLE_ROOT]);
    for fd in [3, 4] {
        command.arg("--add-seccomp-fd").arg(fd.to_string());
    }
    for (key, value) in environment {
        command.arg("--setenv").arg(key).arg(value);
    }
    command
        .arg("--")
        .arg(SANDBOX_LAUNCHER)
        .arg("--keld-linux-strict-launcher-v1")
        .arg("--ro")
        .arg(SANDBOX_PROGRAM)
        .arg("--ro")
        .arg(SANDBOX_LAUNCHER)
        .arg("--ro")
        .arg("/proc");
    for mount in runtime_mounts {
        command.arg("--ro").arg(&mount.destination);
    }
    for path in [SANDBOX_ROLE_ROOT, "/tmp", "/dev"] {
        command.arg("--rw").arg(path);
    }
    command.arg("--").arg(SANDBOX_PROGRAM).args(args);
}

#[derive(Debug, Clone)]
struct LinuxReadonlyMount {
    source: PathBuf,
    destination: PathBuf,
}

impl LinuxReadonlyMount {
    fn new(source: &Path, destination: &Path) -> Result<Self, LinuxStrictError> {
        let source = source
            .canonicalize()
            .map_err(|error| LinuxStrictError::new("runtime mount source", error.to_string()))?;
        let metadata = fs::metadata(&source)
            .map_err(|error| LinuxStrictError::new("runtime mount source", error.to_string()))?;
        if !metadata.is_file() {
            return Err(LinuxStrictError::new(
                "runtime mount source",
                "source must be a regular file; directory-wide code mounts are forbidden",
            ));
        }
        validate_runtime_destination(destination)?;
        Ok(Self {
            source,
            destination: destination.to_path_buf(),
        })
    }
}

fn validate_runtime_destination(destination: &Path) -> Result<(), LinuxStrictError> {
    let mut components = destination.components();
    if !matches!(components.next(), Some(Component::RootDir))
        || components.any(|component| !matches!(component, Component::Normal(_)))
        || destination == Path::new("/")
    {
        return Err(LinuxStrictError::new(
            "runtime mount destination",
            "destination must be a normalized absolute path below root",
        ));
    }
    for reserved in [
        SANDBOX_ROLE_ROOT,
        "/proc",
        "/dev",
        "/tmp",
        "/runtime",
        LANDLOCK_CANARY_ROOT,
    ] {
        if destination.starts_with(reserved) {
            return Err(LinuxStrictError::new(
                "runtime mount destination",
                format!("destination overlaps reserved path {reserved}"),
            ));
        }
    }
    Ok(())
}

fn isolate_pre_exec_fds(source_fds: [std::os::fd::RawFd; 3]) -> std::io::Result<()> {
    use nix::libc;

    let mut relocated = [-1_i32; 3];
    for (index, source) in source_fds.into_iter().enumerate() {
        // SAFETY: fcntl duplicates one child-local live descriptor at or
        // above 5. Relocating every source first prevents a fixed-target dup3
        // from overwriting another source when it occupied FD 3, 4, or 5.
        let duplicate = unsafe { libc::fcntl(source, libc::F_DUPFD, 6) };
        if duplicate == -1 {
            return Err(std::io::Error::last_os_error());
        }
        relocated[index] = duplicate;
    }
    for (source, target) in relocated.into_iter().zip([3, 4, LAUNCHER_READY_FD]) {
        // SAFETY: both numbers name child-local descriptors after fork;
        // `target` is replaced atomically and flags=0 leaves it inheritable
        // for the immediate Bubblewrap exec only.
        if unsafe { libc::dup3(source, target, 0) } == -1 {
            return Err(std::io::Error::last_os_error());
        }
    }
    // SAFETY: close_range with CLOEXEC mutates only the child process's FD
    // table. 0..=5 are stdio, the seccomp pair, and the launcher-only
    // readiness writer; every higher
    // descriptor is closed automatically at the immediate Bubblewrap exec.
    if unsafe {
        libc::syscall(
            libc::SYS_close_range,
            6_u32,
            u32::MAX,
            libc::CLOSE_RANGE_CLOEXEC,
        )
    } == -1
    {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

/// Prepared strict command retaining its inherited seccomp program until spawn.
#[derive(Debug)]
pub struct LinuxStrictCommand {
    command: Command,
    seccomp: Vec<File>,
    readiness: File,
    readiness_writer: File,
}

/// A running strict child paired with its observed Landlock layer.
#[derive(Debug)]
pub struct LinuxStrictChild {
    child: Child,
    landlock: LinuxLandlockStatus,
}

impl LinuxStrictChild {
    /// Returns the Landlock result published before target execution.
    #[must_use]
    pub const fn landlock_status(&self) -> LinuxLandlockStatus {
        self.landlock
    }

    /// Consumes the admission record and returns the running process handle.
    #[must_use]
    pub fn into_child(self) -> Child {
        self.child
    }
}

impl LinuxStrictCommand {
    /// Spawns Bubblewrap. Namespace or setup failure exits without executing
    /// the target; no legacy or unverified fallback exists.
    ///
    /// # Errors
    ///
    /// Returns [`LinuxStrictError`] if Bubblewrap cannot spawn, containment
    /// setup fails, or the post-Landlock readiness record is invalid.
    pub fn spawn(self) -> Result<LinuxStrictChild, LinuxStrictError> {
        let Self {
            mut command,
            seccomp,
            mut readiness,
            readiness_writer,
        } = self;
        let mut child = command
            .spawn()
            .map_err(|source| LinuxStrictError::new("Bubblewrap spawn", source.to_string()))?;
        drop(seccomp);
        drop(readiness_writer);
        let landlock = wait_for_launcher_ready(&mut child, &mut readiness)?;
        Ok(LinuxStrictChild { child, landlock })
    }
}

fn wait_for_launcher_ready(
    child: &mut Child,
    readiness: &mut File,
) -> Result<LinuxLandlockStatus, LinuxStrictError> {
    let deadline = std::time::Instant::now() + BUBBLEWRAP_READY_TIMEOUT;
    let mut record = Vec::new();
    loop {
        let mut chunk = [0_u8; LAUNCHER_READY_MAX];
        match readiness.read(&mut chunk) {
            Ok(0) => {
                return parse_launcher_ready(&record)
                    .map_err(|detail| failed_readiness(child, detail));
            }
            Ok(read) => {
                record.extend_from_slice(&chunk[..read]);
                if record.len() > LAUNCHER_READY_MAX {
                    return Err(failed_readiness(
                        child,
                        format!("record exceeds {LAUNCHER_READY_MAX} bytes"),
                    ));
                }
            }
            Err(source) if source.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(source) if source.kind() == std::io::ErrorKind::Interrupted => {}
            Err(source) => {
                return Err(failed_readiness(child, source.to_string()));
            }
        }
        let status = match child.try_wait() {
            Ok(status) => status,
            Err(source) => {
                terminate_failed_start(child);
                return Err(LinuxStrictError::new(
                    "launcher readiness",
                    source.to_string(),
                ));
            }
        };
        if let Some(status) = status {
            let stderr = failed_child_stderr(child);
            return Err(LinuxStrictError::new(
                "launcher readiness",
                format!(
                    "Bubblewrap exited with {status} before post-Landlock readiness; stderr: {stderr}"
                ),
            ));
        }
        if std::time::Instant::now() >= deadline {
            return Err(failed_readiness(
                child,
                "post-Landlock readiness did not arrive before the deadline",
            ));
        }
        thread::park_timeout(Duration::from_millis(1));
    }
}

fn failed_readiness(child: &mut Child, detail: impl std::fmt::Display) -> LinuxStrictError {
    terminate_failed_start(child);
    let stderr = failed_child_stderr(child);
    LinuxStrictError::new("launcher readiness", format!("{detail}; stderr: {stderr}"))
}

fn parse_launcher_ready(record: &[u8]) -> Result<LinuxLandlockStatus, String> {
    match record {
        LAUNCHER_READY_FULL => Ok(LinuxLandlockStatus::FullyEnforced),
        LAUNCHER_READY_PARTIAL => Ok(LinuxLandlockStatus::PartiallyEnforced),
        LAUNCHER_READY_UNAVAILABLE => Ok(LinuxLandlockStatus::NotImplemented),
        _ => Err(format!("invalid readiness record length {}", record.len())),
    }
}

fn terminate_failed_start(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

fn failed_child_stderr(child: &mut Child) -> String {
    let Some(mut stderr) = child.stderr.take() else {
        return String::from("unavailable");
    };
    let Ok(flags) = rustix::fs::fcntl_getfl(&stderr) else {
        return String::from("unreadable");
    };
    if rustix::fs::fcntl_setfl(&stderr, flags | rustix::fs::OFlags::NONBLOCK).is_err() {
        return String::from("unreadable");
    }
    let mut bytes = Vec::new();
    let mut chunk = [0_u8; 1024];
    while bytes.len() <= 4096 {
        match stderr.read(&mut chunk) {
            Ok(0) => break,
            Ok(read) => bytes.extend_from_slice(&chunk[..read]),
            Err(source) if source.kind() == std::io::ErrorKind::WouldBlock => break,
            Err(source) if source.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => return String::from("unreadable"),
        }
    }
    let truncated = bytes.len() > 4096;
    bytes.truncate(4096);
    let mut text = String::from_utf8_lossy(&bytes).trim().to_owned();
    if truncated {
        text.push_str(" [truncated]");
    }
    if text.is_empty() {
        String::from("empty")
    } else {
        text
    }
}

/// Runs the fixed in-sandbox Landlock launcher role.
///
/// This is called only by the `keld-linux-strict-launcher` binary after
/// Bubblewrap finished namespace/mount/seccomp setup. It applies the stacked
/// Landlock layer, reports it through a launcher-only pipe, closes that
/// capability, then replaces itself with the exact target.
///
/// # Errors
///
/// Returns [`LinuxStrictError`] for malformed private arguments, any Landlock
/// compatibility/enforcement failure, readiness I/O, or target exec failure.
pub fn run_linux_strict_launcher() -> Result<(), LinuxStrictError> {
    let launch = parse_launcher_args(std::env::args_os())?;
    let mut ready = OpenOptions::new()
        .write(true)
        .open(LAUNCHER_READY_FD_PATH)
        .map_err(|source| LinuxStrictError::new("launcher readiness", source.to_string()))?;
    rustix::io::fcntl_setfd(&ready, rustix::io::FdFlags::CLOEXEC)
        .map_err(|source| LinuxStrictError::new("launcher readiness", source.to_string()))?;
    nix::unistd::close(LAUNCHER_READY_FD)
        .map_err(|source| LinuxStrictError::new("launcher readiness", source.to_string()))?;
    let landlock_status = apply_landlock(&launch.readonly, &launch.readwrite)?;
    ready
        .write_all(landlock_status)
        .map_err(|source| LinuxStrictError::new("launcher readiness", source.to_string()))?;
    drop(ready);
    let source = Command::new(&launch.program).args(&launch.args).exec();
    Err(LinuxStrictError::new("target exec", source.to_string()))
}

struct LauncherArgs {
    readonly: Vec<PathBuf>,
    readwrite: Vec<PathBuf>,
    program: OsString,
    args: Vec<OsString>,
}

fn parse_launcher_args(
    args: impl IntoIterator<Item = OsString>,
) -> Result<LauncherArgs, LinuxStrictError> {
    let mut args = args.into_iter();
    let _program_name = args.next();
    if args.next().as_deref() != Some(OsStr::new("--keld-linux-strict-launcher-v1")) {
        return Err(LinuxStrictError::new(
            "launcher arguments",
            "missing private discriminator",
        ));
    }
    let mut readonly = Vec::new();
    let mut readwrite = Vec::new();
    loop {
        let option = args.next().ok_or_else(|| {
            LinuxStrictError::new("launcher arguments", "missing target separator")
        })?;
        match option.to_str() {
            Some("--ro") => readonly.push(PathBuf::from(args.next().ok_or_else(|| {
                LinuxStrictError::new("launcher arguments", "missing read-only path")
            })?)),
            Some("--rw") => readwrite.push(PathBuf::from(args.next().ok_or_else(|| {
                LinuxStrictError::new("launcher arguments", "missing read-write path")
            })?)),
            Some("--") => break,
            _ => {
                return Err(LinuxStrictError::new(
                    "launcher arguments",
                    "unknown private launcher option",
                ));
            }
        }
    }
    let program = args
        .next()
        .ok_or_else(|| LinuxStrictError::new("launcher arguments", "missing target program"))?;
    Ok(LauncherArgs {
        readonly,
        readwrite,
        program,
        args: args.collect(),
    })
}

fn apply_landlock(
    readonly: &[PathBuf],
    readwrite: &[PathBuf],
) -> Result<&'static [u8], LinuxStrictError> {
    use landlock::{
        ABI, Access as _, AccessFs, AccessNet, CompatLevel, Compatible as _, LandlockStatus,
        Ruleset, RulesetAttr as _, RulesetCreatedAttr as _, RulesetStatus, path_beneath_rules,
    };

    let abi = ABI::V9;
    let ruleset = Ruleset::default()
        .set_compatibility(CompatLevel::BestEffort)
        .handle_access(AccessFs::from_all(abi))
        .and_then(|ruleset| ruleset.handle_access(AccessNet::from_all(ABI::V4)))
        .and_then(Ruleset::create)
        .map_err(|source| LinuxStrictError::new("Landlock ruleset", source.to_string()))?;
    let ruleset = ruleset
        .add_rules(path_beneath_rules(readonly, AccessFs::from_read(abi)))
        .and_then(|ruleset| {
            ruleset.add_rules(path_beneath_rules(readwrite, AccessFs::from_all(abi)))
        })
        .map_err(|source| LinuxStrictError::new("Landlock paths", source.to_string()))?;
    let status = ruleset
        .restrict_self()
        .map_err(|source| LinuxStrictError::new("Landlock enforcement", source.to_string()))?;
    match (&status.ruleset, &status.landlock) {
        (RulesetStatus::FullyEnforced, LandlockStatus::Available { .. }) => Ok(LAUNCHER_READY_FULL),
        (RulesetStatus::PartiallyEnforced, LandlockStatus::Available { .. }) => {
            Ok(LAUNCHER_READY_PARTIAL)
        }
        (RulesetStatus::NotEnforced, LandlockStatus::NotImplemented) => {
            Ok(LAUNCHER_READY_UNAVAILABLE)
        }
        _ => Err(LinuxStrictError::new(
            "Landlock enforcement",
            format!("ruleset={:?} kernel={:?}", status.ruleset, status.landlock),
        )),
    }
}

fn validate_launcher(path: &Path) -> Result<PathBuf, LinuxStrictError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|source| LinuxStrictError::new("strict launcher metadata", source.to_string()))?;
    let mode = metadata.permissions().mode() & 0o7777;
    let current_uid = current_effective_uid()?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || (metadata.uid() != 0 && metadata.uid() != current_uid)
        || mode & BUBBLEWRAP_MODE_FORBIDDEN != 0
        || mode & 0o111 == 0
    {
        return Err(LinuxStrictError::new(
            "strict launcher metadata",
            format!(
                "launcher must be trusted, executable and unprivileged; uid={} mode=0o{mode:o}",
                metadata.uid()
            ),
        ));
    }
    reject_file_capabilities(path, "strict launcher file capabilities")?;
    let path = path
        .canonicalize()
        .map_err(|source| LinuxStrictError::new("strict launcher path", source.to_string()))?;
    validate_trusted_ancestors(&path, current_uid, "strict launcher ancestor")?;
    Ok(path)
}

fn validate_bubblewrap(path: &Path) -> Result<PathBuf, LinuxStrictError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|source| LinuxStrictError::new("Bubblewrap metadata", source.to_string()))?;
    let mode = metadata.permissions().mode() & 0o7777;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(LinuxStrictError::new(
            "Bubblewrap metadata",
            "Bubblewrap must be a regular non-symlink file",
        ));
    }
    if metadata.uid() != 0 || mode & BUBBLEWRAP_MODE_FORBIDDEN != 0 || mode & 0o111 == 0 {
        return Err(LinuxStrictError::new(
            "Bubblewrap metadata",
            format!(
                "Bubblewrap must be root-owned, executable, non-setuid/setgid, and not group/world writable; uid={} mode=0o{mode:o}",
                metadata.uid()
            ),
        ));
    }
    reject_file_capabilities(path, "Bubblewrap file capabilities")?;
    let path = path
        .canonicalize()
        .map_err(|source| LinuxStrictError::new("Bubblewrap path", source.to_string()))?;
    validate_trusted_ancestors(&path, current_effective_uid()?, "Bubblewrap ancestor")?;
    Ok(path)
}

fn validate_trusted_ancestors(
    path: &Path,
    current_uid: u32,
    phase: &'static str,
) -> Result<(), LinuxStrictError> {
    for ancestor in path.ancestors().skip(1) {
        let metadata = fs::symlink_metadata(ancestor)
            .map_err(|source| LinuxStrictError::new(phase, source.to_string()))?;
        let mode = metadata.permissions().mode() & 0o7777;
        let trusted_owner = metadata.uid() == 0 || metadata.uid() == current_uid;
        let writable_by_other_principal = mode & 0o022 != 0;
        let sticky_entry_protection = mode & 0o1000 != 0;
        if metadata.file_type().is_symlink()
            || !metadata.is_dir()
            || !trusted_owner
            || (writable_by_other_principal && !sticky_entry_protection)
        {
            return Err(LinuxStrictError::new(
                phase,
                format!(
                    "{} must be a root/current-user-owned directory that other principals cannot replace entries in; uid={} expected_uid={current_uid} mode=0o{mode:o}",
                    ancestor.display(),
                    metadata.uid()
                ),
            ));
        }
    }
    Ok(())
}

fn reject_file_capabilities(path: &Path, phase: &'static str) -> Result<(), LinuxStrictError> {
    let mut capability = [0_u8; 64];
    match rustix::fs::getxattr(path, "security.capability", &mut capability) {
        Ok(0) | Err(rustix::io::Errno::NODATA) => Ok(()),
        Ok(_) => Err(LinuxStrictError::new(
            phase,
            "executable has a security.capability xattr and would add a privileged TCB",
        )),
        Err(source) => Err(LinuxStrictError::new(phase, source.to_string())),
    }
}

fn validate_role_root(path: &Path) -> Result<PathBuf, LinuxStrictError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|source| LinuxStrictError::new("role root metadata", source.to_string()))?;
    let mode = metadata.permissions().mode() & 0o7777;
    let current_uid = current_effective_uid()?;
    if metadata.file_type().is_symlink()
        || !metadata.is_dir()
        || metadata.uid() != current_uid
        || mode != 0o700
    {
        return Err(LinuxStrictError::new(
            "role root metadata",
            format!(
                "root must be a current-user-owned non-symlink directory with mode 0o700; uid={} expected_uid={current_uid} mode=0o{mode:o}",
                metadata.uid()
            ),
        ));
    }
    path.canonicalize()
        .map_err(|source| LinuxStrictError::new("role root path", source.to_string()))
}

fn current_effective_uid() -> Result<u32, LinuxStrictError> {
    let status = fs::read_to_string("/proc/self/status")
        .map_err(|source| LinuxStrictError::new("current UID", source.to_string()))?;
    let uid = status
        .lines()
        .find_map(|line| line.strip_prefix("Uid:\t"))
        .and_then(|values| values.split_whitespace().nth(1))
        .ok_or_else(|| LinuxStrictError::new("current UID", "missing effective Uid field"))?;
    uid.parse::<u32>()
        .map_err(|source| LinuxStrictError::new("current UID", source.to_string()))
}

fn validate_program(path: &Path) -> Result<PathBuf, LinuxStrictError> {
    let path = path
        .canonicalize()
        .map_err(|source| LinuxStrictError::new("target program", source.to_string()))?;
    let metadata = fs::metadata(&path)
        .map_err(|source| LinuxStrictError::new("target program", source.to_string()))?;
    if !metadata.is_file() || metadata.permissions().mode() & 0o111 == 0 {
        return Err(LinuxStrictError::new(
            "target program",
            "target must be a regular executable file",
        ));
    }
    Ok(path)
}

fn validate_environment(environment: &[(OsString, OsString)]) -> Result<(), LinuxStrictError> {
    let mut keys = BTreeSet::new();
    for (key, value) in environment {
        let key_bytes = std::os::unix::ffi::OsStrExt::as_bytes(key.as_os_str());
        let value_bytes = std::os::unix::ffi::OsStrExt::as_bytes(value.as_os_str());
        if key_bytes.is_empty()
            || key_bytes.contains(&0)
            || key_bytes.contains(&b'=')
            || value_bytes.contains(&0)
            || !keys.insert(key.clone())
        {
            return Err(LinuxStrictError::new(
                "target environment",
                "keys must be unique/non-empty/without '=' and keys/values must not contain NUL",
            ));
        }
    }
    Ok(())
}

fn seccomp_files() -> Result<Vec<File>, LinuxStrictError> {
    seccomp_programs()?.into_iter().map(seccomp_file).collect()
}

fn seccomp_file(program: BpfProgram) -> Result<File, LinuxStrictError> {
    let fd = memfd_create(OsStr::new("keld-seccomp"), MFdFlags::empty())
        .map_err(|source| LinuxStrictError::new("seccomp memfd", source.to_string()))?;
    let mut file = File::from(fd);
    for instruction in program {
        file.write_all(&instruction.code.to_ne_bytes())
            .and_then(|()| file.write_all(&[instruction.jt, instruction.jf]))
            .and_then(|()| file.write_all(&instruction.k.to_ne_bytes()))
            .map_err(|source| LinuxStrictError::new("seccomp serialization", source.to_string()))?;
    }
    file.seek(SeekFrom::Start(0))
        .map_err(|source| LinuxStrictError::new("seccomp rewind", source.to_string()))?;
    Ok(file)
}

fn seccomp_programs() -> Result<[BpfProgram; 2], LinuxStrictError> {
    #[cfg(not(target_arch = "x86_64"))]
    return Err(LinuxStrictError::new(
        "seccomp architecture",
        "KEL-78/T4 currently has proof only for x86_64",
    ));
    #[cfg(target_arch = "x86_64")]
    {
        use nix::libc;

        let mut rules = BTreeMap::new();
        let deny = |rules: &mut BTreeMap<i64, Vec<SeccompRule>>, syscall: i64| {
            rules.insert(syscall, Vec::new());
        };
        for syscall in [
            libc::SYS_setns,
            libc::SYS_unshare,
            libc::SYS_mount,
            libc::SYS_umount2,
            libc::SYS_pivot_root,
            libc::SYS_ptrace,
            libc::SYS_bpf,
            libc::SYS_perf_event_open,
            libc::SYS_kexec_load,
            libc::SYS_reboot,
            libc::SYS_swapon,
            libc::SYS_swapoff,
            libc::SYS_init_module,
            libc::SYS_finit_module,
            libc::SYS_delete_module,
            libc::SYS_open_by_handle_at,
            libc::SYS_name_to_handle_at,
            libc::SYS_recvmsg,
            libc::SYS_recvmmsg,
            libc::SYS_sendmsg,
            libc::SYS_sendmmsg,
        ] {
            deny(&mut rules, syscall);
        }
        let socket_rule = SeccompRule::new(vec![
            SeccompCondition::new(
                0,
                SeccompCmpArgLen::Dword,
                SeccompCmpOp::Ne,
                libc::AF_UNIX.cast_unsigned().into(),
            )
            .map_err(|source| LinuxStrictError::new("socket seccomp rule", source.to_string()))?,
        ])
        .map_err(|source| LinuxStrictError::new("socket seccomp rule", source.to_string()))?;
        rules.insert(libc::SYS_socket, vec![socket_rule]);
        let clone_newuser = u64::try_from(libc::CLONE_NEWUSER)
            .map_err(|source| LinuxStrictError::new("clone seccomp rule", source.to_string()))?;
        let clone_rule = SeccompRule::new(vec![
            SeccompCondition::new(
                0,
                SeccompCmpArgLen::Qword,
                SeccompCmpOp::MaskedEq(clone_newuser),
                clone_newuser,
            )
            .map_err(|source| LinuxStrictError::new("clone seccomp rule", source.to_string()))?,
        ])
        .map_err(|source| LinuxStrictError::new("clone seccomp rule", source.to_string()))?;
        rules.insert(libc::SYS_clone, vec![clone_rule]);
        let denied = SeccompFilter::new(
            rules,
            SeccompAction::Allow,
            SeccompAction::Errno(libc::EPERM.cast_unsigned()),
            TargetArch::x86_64,
        )
        .map_err(|source| LinuxStrictError::new("seccomp compilation", source.to_string()))?;
        let denied = denied
            .try_into()
            .map_err(|source: seccompiler::BackendError| {
                LinuxStrictError::new("seccomp compilation", source.to_string())
            })?;
        // glibc and Rust probe clone3 first for ordinary thread creation and
        // fall back to clone only on ENOSYS. Returning EPERM would deny every
        // thread, so clone3 has its own still-denied compatibility action.
        let clone3 = SeccompFilter::new(
            BTreeMap::from([(libc::SYS_clone3, Vec::new())]),
            SeccompAction::Allow,
            SeccompAction::Errno(libc::ENOSYS.cast_unsigned()),
            TargetArch::x86_64,
        )
        .map_err(|source| LinuxStrictError::new("clone3 seccomp compilation", source.to_string()))?
        .try_into()
        .map_err(|source: seccompiler::BackendError| {
            LinuxStrictError::new("clone3 seccomp compilation", source.to_string())
        })?;
        Ok([denied, clone3])
    }
}

#[cfg(test)]
mod readiness_tests {
    use super::{
        LAUNCHER_READY_FULL, LAUNCHER_READY_PARTIAL, LAUNCHER_READY_UNAVAILABLE,
        LinuxLandlockStatus, parse_launcher_ready,
    };

    #[test]
    fn readiness_records_map_to_the_exact_landlock_state() {
        assert_eq!(
            parse_launcher_ready(LAUNCHER_READY_FULL).expect("full readiness"),
            LinuxLandlockStatus::FullyEnforced
        );
        assert_eq!(
            parse_launcher_ready(LAUNCHER_READY_PARTIAL).expect("partial readiness"),
            LinuxLandlockStatus::PartiallyEnforced
        );
        assert_eq!(
            parse_launcher_ready(LAUNCHER_READY_UNAVAILABLE).expect("unavailable readiness"),
            LinuxLandlockStatus::NotImplemented
        );
    }

    #[test]
    fn malformed_or_trailing_readiness_data_is_rejected() {
        for record in [
            b"".as_slice(),
            b"KLS1 landlock=unknown\n".as_slice(),
            b"KLS1 landlock=fully-enforced\ntrailing".as_slice(),
        ] {
            assert!(parse_launcher_ready(record).is_err(), "record={record:?}");
        }
    }
}

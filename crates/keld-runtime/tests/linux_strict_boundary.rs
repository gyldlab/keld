//! Real-Linux KEL-78/T4 strict-profile acceptance.

#![cfg(all(target_os = "linux", target_arch = "x86_64"))]
#![allow(unsafe_code)] // hostile test invokes denied Linux syscalls directly
#![allow(clippy::expect_used, clippy::panic)] // OS/process observations are assertion oracles
#![allow(clippy::zombie_processes)] // host-death fixture deliberately leaves its enrolled descendant live for the external reaper
#![deny(unsafe_op_in_unsafe_fn)]

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{CString, OsString, c_void};
use std::fs;
use std::io::Read as _;
use std::net::{TcpListener, TcpStream};
use std::os::fd::AsRawFd as _;
use std::os::linux::net::SocketAddrExt as _;
use std::os::unix::ffi::OsStrExt as _;
use std::os::unix::fs::{PermissionsExt as _, symlink};
use std::os::unix::net::{SocketAddr, UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use keld_runtime::linux_strict::{LinuxLandlockStatus, LinuxStrictProfile};
use sha2::{Digest as _, Sha256};

const CLONE_NEWUSER: i32 = 0x1000_0000;
const SYS_SETNS: std::os::raw::c_long = 308;
const SYS_CLONE3: std::os::raw::c_long = 435;
const SYS_RECVMSG: std::os::raw::c_long = 47;

unsafe extern "C" {
    fn dlclose(handle: *mut c_void) -> std::os::raw::c_int;
    fn dlopen(filename: *const std::os::raw::c_char, flags: std::os::raw::c_int) -> *mut c_void;
    fn fcntl(fd: std::os::raw::c_int, command: std::os::raw::c_int, ...) -> std::os::raw::c_int;
    fn unshare(flags: std::os::raw::c_int) -> std::os::raw::c_int;
    fn syscall(number: std::os::raw::c_long, ...) -> std::os::raw::c_long;
}

#[test]
fn linux_strict_probe_helper() {
    let Some(mode) = std::env::var_os("KELD_LINUX_STRICT_PROBE") else {
        return;
    };
    if mode == "descendant" {
        assert_strict_process_facts();
        assert_namespace_escape_denied();
        assert_scm_rights_denied();
        assert_host_paths_absent();
        assert_landlock_canary_denied();
        println!("KEL78_LINUX_STRICT_DESCENDANT_PASS");
        return;
    }
    if mode == "host-death-descendant" {
        assert_strict_process_facts();
        assert_namespace_escape_denied();
        loop {
            std::thread::park();
        }
    }
    if mode == "nested-marker" {
        fs::write("/app/nested-target-ran", b"unexpected\n").expect("write nested target marker");
        return;
    }
    if mode == "userns-unavailable" {
        run_nested_userns_negative();
        return;
    }
    if mode == "host-death-primary" {
        run_host_death_primary();
        return;
    }
    assert_eq!(mode, "primary");
    assert_strict_process_facts();
    assert_namespace_escape_denied();
    assert_scm_rights_denied();
    assert_host_paths_absent();
    assert_landlock_canary_denied();
    assert_runtime_fds_are_closed();

    fs::write("/app/role-private-ok", b"strict-write\n").expect("role-private write");
    let connect = TcpStream::connect(("127.0.0.1", 9)).expect_err("direct connect must fail");
    assert_eq!(connect.raw_os_error(), Some(1), "direct connect: {connect}");
    let bind = TcpListener::bind(("127.0.0.1", 0)).expect_err("direct bind must fail");
    assert_eq!(bind.raw_os_error(), Some(1), "direct bind: {bind}");
    let abstract_name =
        std::env::var("KELD_LINUX_HOST_ABSTRACT").expect("host abstract socket name");
    let address =
        SocketAddr::from_abstract_name(abstract_name.as_bytes()).expect("abstract socket address");
    UnixStream::connect_addr(&address)
        .expect_err("host abstract Unix socket must be isolated by the network namespace");

    let output = Command::new("/runtime/program")
        .args([
            "--exact",
            "linux_strict_probe_helper",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("KELD_LINUX_STRICT_PROBE", "descendant")
        .output()
        .expect("spawn equally-contained descendant");
    assert!(
        output.status.success(),
        "descendant failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("KEL78_LINUX_STRICT_DESCENDANT_PASS"));
    println!("KEL78_LINUX_STRICT_PASS");
}

fn run_nested_userns_negative() {
    let output = Command::new("/usr/bin/bwrap")
        .args([
            "--unshare-user",
            "--bind",
            "/app",
            "/app",
            "--ro-bind",
            "/runtime/program",
            "/runtime/program",
            "--",
            "/runtime/program",
            "--exact",
            "linux_strict_probe_helper",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("KELD_LINUX_STRICT_PROBE", "nested-marker")
        .output()
        .expect("invoke nested Bubblewrap");
    assert!(!output.status.success(), "nested userns became available");
    assert!(!Path::new("/app/nested-target-ran").exists());
    println!("KEL78_LINUX_USERNS_FAIL_CLOSED_PASS");
}

fn run_host_death_primary() {
    assert_strict_process_facts();
    assert_namespace_escape_denied();
    let _descendant = Command::new("/runtime/program")
        .args([
            "--exact",
            "linux_strict_probe_helper",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("KELD_LINUX_STRICT_PROBE", "host-death-descendant")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn host-death descendant");
    loop {
        std::thread::park();
    }
}

#[test]
fn linux_strict_host_helper() {
    let Some(role_root) = std::env::var_os("KELD_LINUX_STRICT_HOST_ROLE") else {
        return;
    };
    let profile = strict_profile(Path::new(&role_root));
    let environment = forwarded_strict_environment("host-death-primary");
    let mut child = profile
        .command(
            &std::env::current_exe().expect("host helper executable"),
            &[
                OsString::from("--exact"),
                OsString::from("linux_strict_probe_helper"),
                OsString::from("--nocapture"),
                OsString::from("--test-threads=1"),
            ],
            &environment,
        )
        .expect("host strict command")
        .spawn()
        .expect("host strict spawn")
        .into_child();
    let status = child.wait().expect("wait strict process tree");
    assert!(status.success(), "strict process tree exited {status}");
}

#[test]
fn strict_profile_constructs_only_from_an_unprivileged_launcher_and_private_root() {
    let role = tempfile::tempdir().expect("role-private root");
    fs::set_permissions(role.path(), fs::Permissions::from_mode(0o700))
        .expect("owner-private role mode");
    let profile = strict_profile(role.path());
    let _command = profile
        .command(
            &std::env::current_exe().expect("test executable"),
            &[OsString::from("--help")],
            &[],
        )
        .expect("strict command");
}

#[test]
fn strict_profile_rejects_untrusted_inputs_and_setup_failure_runs_no_target() {
    let root = tempfile::tempdir().expect("negative root");
    let role = root.path().join("role");
    fs::create_dir(&role).expect("negative role");
    fs::set_permissions(&role, fs::Permissions::from_mode(0o700)).expect("negative role mode");
    let link = root.path().join("bwrap-link");
    symlink("/usr/bin/bwrap", &link).expect("bwrap symlink");
    let error = LinuxStrictProfile::new(&link, strict_launcher(), &role)
        .expect_err("symlink launcher must fail");
    assert!(error.to_string().contains("non-symlink"), "{error}");
    let mutable_launcher = root.path().join("mutable-launcher");
    fs::copy(strict_launcher(), &mutable_launcher).expect("copy mutable launcher");
    fs::set_permissions(&mutable_launcher, fs::Permissions::from_mode(0o720))
        .expect("mutable launcher mode");
    let error = LinuxStrictProfile::new(Path::new("/usr/bin/bwrap"), &mutable_launcher, &role)
        .expect_err("group-writable strict launcher must fail");
    assert!(
        error.to_string().contains("strict launcher metadata"),
        "{error}"
    );

    fs::set_permissions(&role, fs::Permissions::from_mode(0o755)).expect("widen role mode");
    let error = LinuxStrictProfile::new(Path::new("/usr/bin/bwrap"), strict_launcher(), &role)
        .expect_err("non-private role must fail");
    assert!(error.to_string().contains("mode 0o700"), "{error}");
    fs::set_permissions(&role, fs::Permissions::from_mode(0o700)).expect("restore role mode");

    let profile = LinuxStrictProfile::new(Path::new("/usr/bin/bwrap"), strict_launcher(), &role)
        .expect("negative strict profile");
    let reserved = profile
        .clone()
        .readonly_runtime(
            Path::new("/usr/lib64/ld-linux-x86-64.so.2"),
            Path::new("/app/lib"),
        )
        .expect_err("runtime mount cannot overlap role authority");
    assert!(
        reserved.to_string().contains("reserved path /app"),
        "{reserved}"
    );
    let relative = profile
        .clone()
        .readonly_runtime(
            Path::new("/usr/lib64/ld-linux-x86-64.so.2"),
            Path::new("relative/lib"),
        )
        .expect_err("runtime mount destination must be absolute");
    assert!(
        relative.to_string().contains("normalized absolute"),
        "{relative}"
    );
    let duplicate_mount = profile
        .clone()
        .readonly_runtime(
            Path::new("/usr/lib64/ld-linux-x86-64.so.2"),
            Path::new("/runtime-lib"),
        )
        .expect("first runtime mount")
        .readonly_runtime(
            Path::new("/usr/lib64/ld-linux-x86-64.so.2"),
            Path::new("/runtime-lib"),
        )
        .expect_err("duplicate runtime destination must fail");
    assert!(
        duplicate_mount.to_string().contains("duplicate"),
        "{duplicate_mount}"
    );
    let duplicate = profile
        .command(
            &std::env::current_exe().expect("negative program"),
            &[OsString::from("--help")],
            &[
                (OsString::from("DUP"), OsString::from("one")),
                (OsString::from("DUP"), OsString::from("two")),
            ],
        )
        .expect_err("duplicate exact-environment key must fail");
    assert!(duplicate.to_string().contains("unique"), "{duplicate}");

    let command = profile
        .command(
            &std::env::current_exe().expect("negative program"),
            &[OsString::from("--help")],
            &[],
        )
        .expect("construct before setup failure");
    fs::remove_dir(&role).expect("remove role root before Bubblewrap setup");
    let error = command
        .spawn()
        .expect_err("missing mount source must fail before returning a target");
    let error = error.to_string();
    assert!(error.contains("launcher readiness"), "{error}");
    assert!(error.contains("stderr: "), "{error}");
    assert!(!error.contains("stderr: empty"), "{error}");
    assert!(!error.contains("stderr: unavailable"), "{error}");
    assert!(!error.contains("stderr: unreadable"), "{error}");

    let malformed = Command::new(strict_launcher())
        .output()
        .expect("invoke malformed private launcher");
    assert!(!malformed.status.success());
    let stderr = String::from_utf8(malformed.stderr).expect("launcher stderr UTF-8");
    assert!(stderr.contains("KELD-RUNTIME-016"), "{stderr}");
    assert!(stderr.contains("launcher arguments"), "{stderr}");
}

#[test]
fn unavailable_nested_user_namespace_fails_closed_before_target() {
    let role = owner_private_tempdir();
    let profile = strict_profile(role.path())
        .readonly_runtime(Path::new("/usr/bin/bwrap"), Path::new("/usr/bin/bwrap"))
        .expect("nested userns negative-control launcher");
    let environment = vec![(
        OsString::from("KELD_LINUX_STRICT_PROBE"),
        OsString::from("userns-unavailable"),
    )];
    let output = profile
        .command(
            &std::env::current_exe().expect("userns negative-control executable"),
            &[
                OsString::from("--exact"),
                OsString::from("linux_strict_probe_helper"),
                OsString::from("--nocapture"),
                OsString::from("--test-threads=1"),
            ],
            &environment,
        )
        .expect("userns negative-control command")
        .spawn()
        .expect("outer strict spawn")
        .into_child()
        .wait_with_output()
        .expect("userns negative-control output");
    assert!(
        output.status.success(),
        "outer strict probe failed: {output:?}"
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("KEL78_LINUX_USERNS_FAIL_CLOSED_PASS")
    );
    assert!(!role.path().join("nested-target-ran").exists());
}

#[test]
fn pinned_bun_starts_and_writes_only_inside_the_strict_role_root() {
    let role = owner_private_tempdir();
    let bun = find_bun();
    let version = Command::new(&bun)
        .arg("--version")
        .output()
        .expect("Bun version");
    assert!(version.status.success());
    assert_eq!(String::from_utf8_lossy(&version.stdout).trim(), "1.4.0");
    fs::write(
        role.path().join("main.ts"),
        "await Bun.write('/app/bun-strict-ok', 'bun-strict-ok\\n');\nconsole.log('KEL78_BUN_STRICT_PASS');\n",
    )
    .expect("strict Bun fixture");
    let output = strict_profile_for_program(role.path(), &bun)
        .command(
            &bun,
            &[OsString::from("run"), OsString::from("/app/main.ts")],
            &[
                (OsString::from("HOME"), OsString::from("/app")),
                (OsString::from("TMPDIR"), OsString::from("/tmp")),
            ],
        )
        .expect("strict Bun command")
        .spawn()
        .expect("strict Bun spawn")
        .into_child()
        .wait_with_output()
        .expect("strict Bun output");
    assert!(
        output.status.success(),
        "strict Bun failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("KEL78_BUN_STRICT_PASS"));
    assert_eq!(
        fs::read(role.path().join("bun-strict-ok")).expect("strict Bun role write"),
        b"bun-strict-ok\n"
    );
    eprintln!(
        "KELD_LINUX_T4_BUN version=1.4.0 artifact_sha256={} status=passed",
        file_sha256(&bun)
    );
}

#[test]
fn strict_profile_denies_host_fs_network_namespace_escape_and_fd_inheritance() {
    let role = owner_private_tempdir();
    let host_only = tempfile::tempdir().expect("host-only root");
    let host_marker = host_only.path().join("host-secret");
    fs::write(&host_marker, b"must not enter sandbox\n").expect("host marker");
    let blocked_library = host_only.path().join("blocked.so");
    fs::copy("/usr/lib/x86_64-linux-gnu/libz.so.1", &blocked_library)
        .expect("real blocked shared library");
    let abstract_name = format!("keld-kel78-{}", std::process::id());
    let abstract_address =
        SocketAddr::from_abstract_name(abstract_name.as_bytes()).expect("host abstract address");
    let abstract_listener = UnixListener::bind_addr(&abstract_address).expect("host abstract bind");
    let inherited = fs::File::open(&host_marker).expect("inheritable host descriptor");
    // SAFETY: the test owns this live descriptor and deliberately clears only
    // FD_CLOEXEC to prove Bubblewrap closes ambient descriptors before target exec.
    assert_ne!(unsafe { fcntl(inherited.as_raw_fd(), 2, 0) }, -1);
    let profile = strict_profile(role.path());
    let environment = vec![
        (
            OsString::from("KELD_LINUX_STRICT_PROBE"),
            OsString::from("primary"),
        ),
        (
            OsString::from("KELD_LINUX_HOST_MARKER"),
            host_marker.as_os_str().to_owned(),
        ),
        (
            OsString::from("KELD_LINUX_BLOCKED_DLOPEN"),
            blocked_library.into_os_string(),
        ),
        (
            OsString::from("KELD_LINUX_EXPECT_UID"),
            OsString::from(current_effective_uid()),
        ),
        (
            OsString::from("KELD_LINUX_HOST_ABSTRACT"),
            OsString::from(&abstract_name),
        ),
        namespace_environment("USER"),
        namespace_environment("MNT"),
        namespace_environment("PID"),
        namespace_environment("NET"),
    ];
    let admitted = profile
        .command(
            &std::env::current_exe().expect("test executable"),
            &[
                OsString::from("--exact"),
                OsString::from("linux_strict_probe_helper"),
                OsString::from("--nocapture"),
                OsString::from("--test-threads=1"),
            ],
            &environment,
        )
        .expect("strict command")
        .spawn()
        .expect("strict spawn");
    assert!(
        matches!(
            admitted.landlock_status(),
            LinuxLandlockStatus::FullyEnforced | LinuxLandlockStatus::PartiallyEnforced
        ),
        "hostile Landlock row requires a kernel Landlock layer: {:?}",
        admitted.landlock_status()
    );
    let mut child = admitted.into_child();
    let status = wait_child(&mut child, Instant::now() + Duration::from_secs(20));
    let mut stdout = String::new();
    let mut stderr = String::new();
    child
        .stdout
        .take()
        .expect("strict stdout")
        .read_to_string(&mut stdout)
        .expect("read strict stdout");
    child
        .stderr
        .take()
        .expect("strict stderr")
        .read_to_string(&mut stderr)
        .expect("read strict stderr");
    assert!(status.success(), "strict target failed: {stderr}");
    assert!(stdout.contains("KEL78_LINUX_STRICT_PASS"), "{stdout}");
    assert_eq!(
        fs::read(role.path().join("role-private-ok")).expect("role output"),
        b"strict-write\n"
    );
    abstract_listener
        .set_nonblocking(true)
        .expect("nonblocking abstract listener");
    assert!(
        matches!(
            abstract_listener.accept(),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock
        ),
        "strict target reached the host abstract socket"
    );
    drop(inherited);
    print_strict_evidence();
}

#[test]
fn host_only_death_reaps_the_strict_tree_and_relaunches() {
    let role = owner_private_tempdir();
    let host_only = tempfile::tempdir().expect("host-only root");
    fs::write(host_only.path().join("host-secret"), b"secret\n").expect("host marker");
    fs::copy(
        "/usr/lib/x86_64-linux-gnu/libz.so.1",
        host_only.path().join("blocked.so"),
    )
    .expect("host blocked library");
    let mut host = Command::new(std::env::current_exe().expect("host helper executable"))
        .args([
            "--exact",
            "linux_strict_host_helper",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("KELD_LINUX_STRICT_HOST_ROLE", role.path())
        .env(
            "KELD_LINUX_HOST_MARKER",
            host_only.path().join("host-secret"),
        )
        .env(
            "KELD_LINUX_BLOCKED_DLOPEN",
            host_only.path().join("blocked.so"),
        )
        .env("KELD_LINUX_EXPECT_UID", current_effective_uid())
        .env("KELD_LINUX_HOST_NS_USER", host_namespace("user"))
        .env("KELD_LINUX_HOST_NS_MNT", host_namespace("mnt"))
        .env("KELD_LINUX_HOST_NS_PID", host_namespace("pid"))
        .env("KELD_LINUX_HOST_NS_NET", host_namespace("net"))
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("launch strict host helper");
    let tree = wait_for_strict_tree(&mut host, Instant::now() + Duration::from_secs(20));
    assert!(tree.len() >= 4, "incomplete strict process tree: {tree:?}");

    host.kill().expect("SIGKILL only the host helper");
    let status = host.wait().expect("wait killed host helper");
    assert!(!status.success());
    for process in &tree {
        wait_process_identity_gone(process, Instant::now() + Duration::from_secs(10));
    }

    let output = run_strict_probe(&role, &host_only);
    assert!(output.status.success(), "relaunch failed: {output:?}");
    assert!(String::from_utf8_lossy(&output.stdout).contains("KEL78_LINUX_STRICT_PASS"));
    eprintln!(
        "KELD_LINUX_T4_HOST_DEATH reaped={} relaunch=passed",
        tree.iter()
            .map(|process| process.pid.to_string())
            .collect::<Vec<_>>()
            .join(",")
    );
}

fn owner_private_tempdir() -> tempfile::TempDir {
    let role = tempfile::tempdir().expect("role-private root");
    fs::set_permissions(role.path(), fs::Permissions::from_mode(0o700))
        .expect("owner-private role mode");
    role
}

fn strict_profile(role_root: &Path) -> LinuxStrictProfile {
    strict_profile_for_program(
        role_root,
        &std::env::current_exe().expect("test executable"),
    )
}

fn strict_profile_for_program(role_root: &Path, program: &Path) -> LinuxStrictProfile {
    let mut profile =
        LinuxStrictProfile::new(Path::new("/usr/bin/bwrap"), strict_launcher(), role_root)
            .expect("strict profile base");
    let mut mounts = runtime_dependencies(program);
    mounts.extend(runtime_dependencies(strict_launcher()));
    mounts.sort();
    mounts.dedup();
    for (source, destination) in mounts {
        profile = profile
            .readonly_runtime(&source, &destination)
            .expect("strict runtime file");
    }
    profile
}

fn find_bun() -> std::path::PathBuf {
    std::env::split_paths(&std::env::var_os("PATH").expect("PATH"))
        .map(|directory| directory.join("bun"))
        .find(|candidate| candidate.is_file())
        .expect("pinned Bun 1.4.0 on PATH")
}

fn runtime_dependencies(program: &Path) -> Vec<(std::path::PathBuf, std::path::PathBuf)> {
    let output = Command::new("ldd")
        .arg(program)
        .output()
        .expect("inspect runtime dependencies");
    assert!(output.status.success(), "ldd failed: {output:?}");
    String::from_utf8(output.stdout)
        .expect("ldd UTF-8")
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            let path = line
                .split_once("=>")
                .map_or(line, |(_, target)| target.trim())
                .split_whitespace()
                .next()?;
            path.starts_with('/').then(|| {
                let destination = std::path::PathBuf::from(path);
                let source = destination.canonicalize().expect("runtime dependency path");
                (source, destination)
            })
        })
        .collect()
}

fn strict_launcher() -> &'static Path {
    static FIXTURE: OnceLock<(tempfile::TempDir, PathBuf)> = OnceLock::new();
    let (_, launcher) = FIXTURE.get_or_init(|| {
        let root = owner_private_tempdir();
        let launcher = root.path().join("keld-linux-strict-launcher");
        fs::copy(
            Path::new(env!("CARGO_BIN_EXE_keld-linux-strict-launcher")),
            &launcher,
        )
        .expect("stage strict launcher fixture");
        fs::set_permissions(&launcher, fs::Permissions::from_mode(0o500))
            .expect("owner-executable strict launcher fixture");
        (root, launcher)
    });
    launcher
}

fn namespace_environment(kind: &str) -> (OsString, OsString) {
    let name = kind.to_ascii_lowercase();
    let observed = fs::read_link(format!("/proc/self/ns/{name}"))
        .expect("host namespace")
        .into_os_string();
    (
        OsString::from(format!("KELD_LINUX_HOST_NS_{kind}")),
        observed,
    )
}

fn assert_strict_process_facts() {
    let status = fs::read_to_string("/proc/self/status").expect("strict process status");
    for field in ["CapInh", "CapPrm", "CapEff", "CapBnd", "CapAmb"] {
        assert!(
            status.contains(&format!("{field}:\t0000000000000000")),
            "{status}"
        );
    }
    assert!(status.contains("NoNewPrivs:\t1"), "{status}");
    assert!(status.contains("Seccomp:\t2"), "{status}");
    let uid = std::env::var("KELD_LINUX_EXPECT_UID").expect("expected UID");
    assert!(
        status.contains(&format!("Uid:\t{uid}\t{uid}\t{uid}\t{uid}")),
        "{status}"
    );
    for (kind, name) in [
        ("USER", "user"),
        ("MNT", "mnt"),
        ("PID", "pid"),
        ("NET", "net"),
    ] {
        let current = fs::read_link(format!("/proc/self/ns/{name}")).expect("strict namespace");
        let host = std::env::var_os(format!("KELD_LINUX_HOST_NS_{kind}"))
            .expect("host namespace identity");
        assert_ne!(
            current.as_os_str(),
            host,
            "{kind} namespace was not isolated"
        );
    }
    let uid_map = fs::read_to_string("/proc/self/uid_map").expect("strict uid map");
    let fields = uid_map.split_whitespace().collect::<Vec<_>>();
    assert_eq!(fields.len(), 3, "{uid_map}");
    assert_eq!(fields[0], uid, "{uid_map}");
    assert_eq!(fields[2], "1", "{uid_map}");
}

fn assert_namespace_escape_denied() {
    // SAFETY: these hostile calls use no pointers or live descriptors. Their
    // only acceptable result is the seccomp-owned EPERM checked immediately.
    assert_eq!(unsafe { unshare(CLONE_NEWUSER) }, -1);
    assert_eq!(std::io::Error::last_os_error().raw_os_error(), Some(1));
    // SAFETY: null clone_args with size zero and invalid setns fd are inert
    // negative controls; seccomp must reject both before argument validation.
    assert_eq!(
        unsafe { syscall(SYS_CLONE3, std::ptr::null::<u8>(), 0_usize) },
        -1
    );
    assert_eq!(std::io::Error::last_os_error().raw_os_error(), Some(38));
    // SAFETY: `-1` is intentionally invalid and no pointer is supplied.
    assert_eq!(unsafe { syscall(SYS_SETNS, -1_i32, CLONE_NEWUSER) }, -1);
    assert_eq!(std::io::Error::last_os_error().raw_os_error(), Some(1));
}

fn assert_scm_rights_denied() {
    // SAFETY: invalid fd and null msghdr are inert; an EPERM result proves the
    // seccomp layer rejected ancillary-message receive before argument use.
    assert_eq!(
        unsafe { syscall(SYS_RECVMSG, -1_i32, std::ptr::null::<u8>(), 0_i32) },
        -1
    );
    assert_eq!(std::io::Error::last_os_error().raw_os_error(), Some(1));
}

fn assert_host_paths_absent() {
    for path in ["/etc/passwd", "/home", "/sys", "/dev/video0"] {
        assert_eq!(
            fs::metadata(path)
                .expect_err("host path/device must be absent")
                .kind(),
            std::io::ErrorKind::NotFound,
            "{path}"
        );
    }
    let marker = std::env::var_os("KELD_LINUX_HOST_MARKER")
        .unwrap_or_else(|| OsString::from("/tmp/host-marker-not-mounted"));
    assert_eq!(
        fs::read(Path::new(&marker))
            .expect_err("host marker must be absent")
            .kind(),
        std::io::ErrorKind::NotFound
    );
    if let Some(blocked) = std::env::var_os("KELD_LINUX_BLOCKED_DLOPEN") {
        let blocked = CString::new(blocked.as_bytes()).expect("blocked dlopen path");
        // SAFETY: the path is a live NUL-terminated buffer. A non-null result
        // is closed immediately before failing the hostile oracle.
        let handle = unsafe { dlopen(blocked.as_ptr(), 2) };
        if !handle.is_null() {
            // SAFETY: `handle` is the live object returned by dlopen above.
            let _ = unsafe { dlclose(handle) };
            panic!("host shared library loaded inside strict profile");
        }
    }
}

fn assert_landlock_canary_denied() {
    let error = fs::write("/landlock-probe/escape", b"must fail\n")
        .expect_err("mounted Landlock canary must remain denied");
    assert_eq!(
        error.kind(),
        std::io::ErrorKind::PermissionDenied,
        "{error}"
    );
}

fn assert_runtime_fds_are_closed() {
    let mut unexpected = Vec::new();
    for entry in fs::read_dir("/proc/self/fd").expect("strict fd census") {
        let entry = entry.expect("strict fd entry");
        let fd = entry
            .file_name()
            .to_string_lossy()
            .parse::<u32>()
            .expect("numeric fd");
        if fd <= 2 {
            continue;
        }
        let target = fs::read_link(entry.path()).expect("strict fd target");
        if !target.to_string_lossy().ends_with("/fd") {
            unexpected.push((fd, target));
        }
    }
    assert!(
        unexpected.is_empty(),
        "unexpected inherited FDs: {unexpected:?}"
    );
}

fn wait_child(child: &mut std::process::Child, deadline: Instant) -> std::process::ExitStatus {
    loop {
        if let Some(status) = child.try_wait().expect("observe strict child") {
            return status;
        }
        assert!(Instant::now() < deadline, "strict child timed out");
        std::thread::park_timeout(Duration::from_millis(10));
    }
}

fn current_effective_uid() -> String {
    let status = fs::read_to_string("/proc/self/status").expect("host status");
    status
        .lines()
        .find_map(|line| line.strip_prefix("Uid:\t"))
        .and_then(|values| values.split_whitespace().nth(1))
        .expect("host effective UID")
        .to_owned()
}

fn print_strict_evidence() {
    let artifact = std::env::current_exe().expect("evidence artifact");
    let artifact_sha = file_sha256(&artifact);
    let launcher_sha = file_sha256(strict_launcher());
    let bwrap_sha = file_sha256(Path::new("/usr/bin/bwrap"));
    let bwrap = Command::new("/usr/bin/bwrap")
        .arg("--version")
        .output()
        .expect("Bubblewrap version");
    assert!(bwrap.status.success());
    let bwrap_version = String::from_utf8(bwrap.stdout)
        .expect("Bubblewrap version UTF-8")
        .trim()
        .replace(' ', "_");
    let kernel = fs::read_to_string("/proc/sys/kernel/osrelease")
        .expect("kernel release")
        .trim()
        .to_owned();
    let profile_material = format!(
        "keld-linux-strict/v1|bwrap={bwrap_sha}|launcher={launcher_sha}|seccompiler=0.5.0|landlock=0.4.7|max-abi=9|net-abi=4|namespaces=user,mount,pid,net|seccomp=two-filter|mount=empty-root|fd=stdio+3+4-preexec"
    );
    let profile_sha = format!("{:x}", Sha256::digest(profile_material.as_bytes()));
    eprintln!(
        "KELD_LINUX_T4_STRICT kernel={kernel} bwrap={bwrap_version} bwrap_sha256={bwrap_sha} launcher_sha256={launcher_sha} artifact_sha256={artifact_sha} profile_sha256={profile_sha} landlock_net_abi=4 layers=mount,landlock,seccomp cleanup=pid-namespace"
    );
}

fn file_sha256(path: &Path) -> String {
    let bytes = fs::read(path).expect("evidence artifact read");
    format!("{:x}", Sha256::digest(bytes))
}

fn host_namespace(name: &str) -> OsString {
    fs::read_link(format!("/proc/self/ns/{name}"))
        .expect("host namespace")
        .into_os_string()
}

fn forwarded_strict_environment(mode: &str) -> Vec<(OsString, OsString)> {
    let mut environment = vec![(
        OsString::from("KELD_LINUX_STRICT_PROBE"),
        OsString::from(mode),
    )];
    for key in [
        "KELD_LINUX_HOST_MARKER",
        "KELD_LINUX_BLOCKED_DLOPEN",
        "KELD_LINUX_EXPECT_UID",
        "KELD_LINUX_HOST_NS_USER",
        "KELD_LINUX_HOST_NS_MNT",
        "KELD_LINUX_HOST_NS_PID",
        "KELD_LINUX_HOST_NS_NET",
    ] {
        environment.push((
            OsString::from(key),
            std::env::var_os(key).expect("forwarded strict environment"),
        ));
    }
    environment
}

fn run_strict_probe(
    role: &tempfile::TempDir,
    host_only: &tempfile::TempDir,
) -> std::process::Output {
    let abstract_name = format!("keld-relaunch-{}", std::process::id());
    let abstract_address = SocketAddr::from_abstract_name(abstract_name.as_bytes())
        .expect("relaunch abstract address");
    let _abstract_listener =
        UnixListener::bind_addr(&abstract_address).expect("relaunch abstract listener");
    let profile = strict_profile(role.path());
    let environment = vec![
        (
            OsString::from("KELD_LINUX_STRICT_PROBE"),
            OsString::from("primary"),
        ),
        (
            OsString::from("KELD_LINUX_HOST_MARKER"),
            host_only.path().join("host-secret").into_os_string(),
        ),
        (
            OsString::from("KELD_LINUX_BLOCKED_DLOPEN"),
            host_only.path().join("blocked.so").into_os_string(),
        ),
        (
            OsString::from("KELD_LINUX_EXPECT_UID"),
            OsString::from(current_effective_uid()),
        ),
        (
            OsString::from("KELD_LINUX_HOST_ABSTRACT"),
            OsString::from(abstract_name),
        ),
        namespace_environment("USER"),
        namespace_environment("MNT"),
        namespace_environment("PID"),
        namespace_environment("NET"),
    ];
    profile
        .command(
            &std::env::current_exe().expect("relaunch test executable"),
            &[
                OsString::from("--exact"),
                OsString::from("linux_strict_probe_helper"),
                OsString::from("--nocapture"),
                OsString::from("--test-threads=1"),
            ],
            &environment,
        )
        .expect("relaunch strict command")
        .spawn()
        .expect("relaunch strict spawn")
        .into_child()
        .wait_with_output()
        .expect("relaunch strict output")
}

#[derive(Debug, Clone, Copy)]
struct ProcessIdentity {
    pid: u32,
    start_time: u64,
}

fn wait_for_strict_tree(host: &mut std::process::Child, deadline: Instant) -> Vec<ProcessIdentity> {
    let host_pid = host.id();
    loop {
        let tree = descendant_identities(host_pid);
        if tree.len() >= 4 {
            return tree;
        }
        if let Some(status) = host.try_wait().expect("observe strict host helper") {
            let mut stderr = String::new();
            host.stderr
                .take()
                .expect("failed host stderr")
                .read_to_string(&mut stderr)
                .expect("read failed host stderr");
            panic!("host helper exited before strict tree: {status}: {stderr}");
        }
        assert!(
            Instant::now() < deadline,
            "strict tree incomplete: {tree:?}"
        );
        std::thread::park_timeout(Duration::from_millis(10));
    }
}

fn descendant_identities(root: u32) -> Vec<ProcessIdentity> {
    let mut parents = BTreeSet::from([root]);
    let mut found = BTreeMap::new();
    loop {
        let before = found.len();
        for entry in fs::read_dir("/proc").expect("process census") {
            let Ok(entry) = entry else { continue };
            let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else {
                continue;
            };
            let Some((parent, start_time)) = process_stat(pid) else {
                continue;
            };
            if parents.contains(&parent) && pid != root {
                parents.insert(pid);
                found.insert(pid, ProcessIdentity { pid, start_time });
            }
        }
        if found.len() == before {
            return found.into_values().collect();
        }
    }
}

fn process_stat(pid: u32) -> Option<(u32, u64)> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let (_, fields) = stat.rsplit_once(") ")?;
    let fields = fields.split_whitespace().collect::<Vec<_>>();
    let parent = fields.get(1)?.parse().ok()?;
    let start_time = fields.get(19)?.parse().ok()?;
    Some((parent, start_time))
}

fn wait_process_identity_gone(process: &ProcessIdentity, deadline: Instant) {
    loop {
        if process_stat(process.pid).is_none_or(|(_, start)| start != process.start_time) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "strict process survived host death: {process:?}"
        );
        std::thread::park_timeout(Duration::from_millis(10));
    }
}

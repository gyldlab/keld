//! Real Windows process-tree proof for the KEL-78/T3 host-death Job.

#![cfg(windows)]
#![allow(unsafe_code)] // isolated test-only process-handle observation with local ABI proofs
#![allow(clippy::expect_used, clippy::panic)] // process fixture invariants must abort loudly
#![deny(unsafe_op_in_unsafe_fn)]

use std::env;
use std::io::{self, BufRead as _, BufReader, Write as _};
use std::os::windows::io::{FromRawHandle as _, OwnedHandle};
use std::process::{Child, Command, Stdio};

use keld_runtime::windows_job::install_host_death_job;
use windows_sys::Win32::Foundation::WAIT_OBJECT_0;
use windows_sys::Win32::System::Threading::{
    OpenProcess, PROCESS_SYNCHRONIZE, PROCESS_TERMINATE, TerminateProcess, WaitForSingleObject,
};

const HELPER_ENV: &str = "KELD_WINDOWS_JOB_HELPER";
const HELPER_TEST: &str = "windows_job_process_helper";
const PROCESS_WAIT_MS: u32 = 10_000;

#[test]
fn abnormal_host_death_reaps_direct_child_and_descendant_then_relaunches() {
    let mut host = spawn_helper("host");
    let stdout = host.stdout.take().expect("host stdout pipe");
    let mut lines = BufReader::new(stdout).lines();

    let observation = next_prefixed_line(&mut lines, "JOB ");
    assert!(
        observation.contains("limits=0x00002000"),
        "Job must report only KILL_ON_JOB_CLOSE: {observation}"
    );
    assert!(
        observation.contains("assigned=true") && observation.contains("inheritable=false"),
        "Job assignment and non-inheritance must be observed: {observation}"
    );

    let direct_pid = parse_pid(&next_prefixed_line(&mut lines, "DIRECT "), "DIRECT");
    let descendant_pid = parse_pid(&next_prefixed_line(&mut lines, "DESCENDANT "), "DESCENDANT");
    let direct = open_process_for_wait(direct_pid);
    let descendant = open_process_for_wait(descendant_pid);

    // Child::kill terminates this one host process. It does not request tree
    // termination, so the two retained process handles independently observe
    // whether the Job kernel contract reaped both enrolled descendants.
    host.kill()
        .expect("terminate only the host fixture process");
    let _ = host.wait().expect("wait for terminated host fixture");
    assert_process_exited(&direct, "direct child");
    assert_process_exited(&descendant, "descendant");

    let relaunch = spawn_helper("relaunch")
        .wait_with_output()
        .expect("run post-cleanup launch");
    assert!(
        relaunch.status.success(),
        "post-cleanup launch failed: status={} stderr={}",
        relaunch.status,
        String::from_utf8_lossy(&relaunch.stderr)
    );
    assert!(
        String::from_utf8_lossy(&relaunch.stdout).contains("RELAUNCH_OK"),
        "post-cleanup launch did not report success: {}",
        String::from_utf8_lossy(&relaunch.stdout)
    );
}

#[test]
#[ignore = "private subprocess entry point"]
fn windows_job_process_helper() {
    match env::var(HELPER_ENV).as_deref() {
        Ok("host") => run_host_helper(),
        Ok("direct") => run_direct_helper(),
        Ok("descendant") => run_descendant_helper(),
        Ok("relaunch") => run_relaunch_helper(),
        other => panic!("unexpected {HELPER_ENV} value: {other:?}"),
    }
}

fn run_host_helper() {
    let observation = install_host_death_job().expect("install host-death Job");
    println!(
        "JOB limits=0x{:08x} nested={} assigned={} inheritable={}",
        observation.limit_flags,
        observation.nested_under_existing_job,
        observation.current_process_assigned,
        observation.handle_inheritable
    );
    io::stdout().flush().expect("flush Job observation");

    let mut direct = spawn_helper("direct");
    let direct_stdout = direct.stdout.take().expect("direct stdout pipe");
    for line in BufReader::new(direct_stdout).lines() {
        println!("{}", line.expect("read direct process observation"));
        io::stdout().flush().expect("flush process observation");
    }
    let status = direct.wait().expect("wait for direct child");
    assert!(status.success(), "direct child failed: {status}");
}

fn run_direct_helper() {
    println!("DIRECT {}", std::process::id());
    io::stdout().flush().expect("flush direct PID");

    let mut descendant = spawn_helper("descendant");
    let descendant_stdout = descendant.stdout.take().expect("descendant stdout pipe");
    let mut lines = BufReader::new(descendant_stdout).lines();
    println!("{}", next_prefixed_line(&mut lines, "DESCENDANT "));
    io::stdout().flush().expect("flush descendant PID");
    let status = descendant.wait().expect("wait for descendant");
    assert!(status.success(), "descendant failed: {status}");
}

fn run_descendant_helper() {
    println!("DESCENDANT {}", std::process::id());
    io::stdout().flush().expect("flush descendant PID");
    std::thread::park();
}

fn run_relaunch_helper() {
    let observation = install_host_death_job().expect("install Job after prior host death");
    assert!(observation.current_process_assigned);

    let status = Command::new("cmd.exe")
        .args(["/d", "/c", "exit 0"])
        .status()
        .expect("spawn child after prior Job cleanup");
    assert!(status.success(), "post-cleanup child failed: {status}");
    println!("RELAUNCH_OK");
}

fn spawn_helper(role: &str) -> Child {
    let mut command = Command::new(env::current_exe().expect("current test executable"));
    command
        .args(["--exact", HELPER_TEST, "--ignored", "--nocapture"])
        .env(HELPER_ENV, role)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command.spawn().unwrap_or_else(|error| {
        panic!("spawn {role} process fixture: {error}");
    })
}

fn next_prefixed_line(
    lines: &mut impl Iterator<Item = io::Result<String>>,
    prefix: &str,
) -> String {
    lines
        .find_map(|line| match line {
            Ok(line) if line.starts_with(prefix) => Some(Ok(line)),
            Ok(_) => None,
            Err(error) => Some(Err(error)),
        })
        .unwrap_or_else(|| panic!("line starting with {prefix:?} missing"))
        .unwrap_or_else(|error| panic!("read line starting with {prefix:?}: {error}"))
}

fn parse_pid(line: &str, label: &str) -> u32 {
    let prefix = format!("{label} ");
    line.strip_prefix(&prefix)
        .unwrap_or_else(|| panic!("expected {label} PID line, got {line:?}"))
        .parse()
        .unwrap_or_else(|error| panic!("parse {label} PID from {line:?}: {error}"))
}

struct ObservedProcess {
    handle: OwnedHandle,
    pid: u32,
}

impl Drop for ObservedProcess {
    fn drop(&mut self) {
        use std::os::windows::io::AsRawHandle as _;

        let raw = self.handle.as_raw_handle().cast();
        // SAFETY: `raw` is live for both calls. A zero-time wait only checks
        // state. If a failed assertion or negative control left the fixture
        // alive, PROCESS_TERMINATE was requested specifically for cleanup.
        if unsafe { WaitForSingleObject(raw, 0) } != WAIT_OBJECT_0 {
            // SAFETY: same live process handle, with PROCESS_TERMINATE access;
            // this test-owned fixture has no state outside the test.
            let _ = unsafe { TerminateProcess(raw, 1) };
        }
    }
}

fn open_process_for_wait(pid: u32) -> ObservedProcess {
    // SAFETY: OpenProcess receives a numeric PID observed from the live test
    // child and requests observation plus test-fixture cleanup rights. A
    // non-null result is one fresh owning handle, converted exactly once.
    let raw = unsafe { OpenProcess(PROCESS_SYNCHRONIZE | PROCESS_TERMINATE, 0, pid) };
    assert!(
        !raw.is_null(),
        "open process {pid}: {}",
        io::Error::last_os_error()
    );
    ObservedProcess {
        // SAFETY: `raw` is the fresh non-null owning handle returned above.
        handle: unsafe { OwnedHandle::from_raw_handle(raw.cast()) },
        pid,
    }
}

fn assert_process_exited(process: &ObservedProcess, description: &str) {
    use std::os::windows::io::AsRawHandle as _;

    // SAFETY: the borrowed raw process handle remains live for this call. A
    // signaled process handle is the kernel's termination oracle; the timeout
    // only bounds a broken fixture and is not synchronization by sleeping.
    let result =
        unsafe { WaitForSingleObject(process.handle.as_raw_handle().cast(), PROCESS_WAIT_MS) };
    assert_eq!(
        result, WAIT_OBJECT_0,
        "{description} PID {} survived host death",
        process.pid
    );
}

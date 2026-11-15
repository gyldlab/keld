//! Exact-self native child custody and independently calibrated object census.
use super::super::super::ReapedRouterProbe;
use nix::errno::Errno;
use nix::sys::signal::{Signal, killpg};
use nix::unistd::{Pid, getpgid, getpgrp, getpid};
use serde_json::{Value, json};
use std::fs::{self, File};
use std::io::{self, Read as _, Seek as _, SeekFrom};
use std::os::unix::process::{CommandExt as _, ExitStatusExt as _};
use std::path::Path;
use std::process::{Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

pub(super) use super::sensor::{calibrate_object_sensor, object_handle_count};

#[path = "native_process_observer.rs"]
mod process;
use process::{
    OBSERVER_SOCKET_ENV, OBSERVER_TOKEN_ENV, ProcessObserver, protocol_error, require_record,
};

const CHILD_ENV: &str = "KELD_CORE_AC7_NATIVE_CHILD";
const ROOT_ENV: &str = "KELD_CORE_AC7_NATIVE_TMP_ROOT";
const CHILD_BOUND: Duration = Duration::from_secs(45);

fn await_helper_absent(helper: u32, deadline: Instant) -> io::Result<()> {
    loop {
        // This is a read-only oracle, never authority to signal a cached PID.
        let mut stdout = tempfile::tempfile()?;
        let mut stderr = tempfile::tempfile()?;
        let mut census = NativeChild::own(
            Command::new("/bin/ps")
                .args(["-p", &helper.to_string(), "-o", "pid=,ppid=,pgid=,state="])
                .stdin(Stdio::null())
                .stdout(Stdio::from(stdout.try_clone()?))
                .stderr(Stdio::from(stderr.try_clone()?))
                .process_group(0)
                .spawn()?,
        )?;
        let status = loop {
            if let Some(status) = census.try_wait()? {
                break status;
            }
            if Instant::now() >= deadline {
                let _ = census.stop_unreaped_group(None);
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "helper census bound",
                ));
            }
            std::thread::yield_now();
        };
        let output = captured(&mut stdout);
        let detail = captured(&mut stderr);
        if status.code() == Some(1) && output.is_empty() && detail.is_empty() {
            return Ok(());
        }
        if !status.success() || !detail.is_empty() {
            return Err(protocol_error("independent helper census failed"));
        }
        if Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                format!("held helper still present before fallback: {output}"),
            ));
        }
        std::thread::yield_now();
    }
}

#[test]
#[ignore = "explicit real macOS observer/tooling group-custody control"]
fn observer_family_reap_removes_held_helper_before_fallback() {
    let mut observer = ProcessObserver::start("helper-custody").expect("observer-only group");
    let (helper, mut control, held) = observer.hold_helper().expect("genuine held-helper ACK");
    let token = observer.token().to_owned();
    let mut absent = Err(protocol_error("helper absence not observed"));
    let status = observer.stop_family(|signal| {
        absent = await_helper_absent(helper, Instant::now() + super::OBSERVATION_BOUND);
        println!(
            "KELD_AC7_HELPER_BEFORE_FALLBACK held={held} group_signal={signal:?} absent={absent:?}"
        );
        if absent.is_err() {
            // A retained private endpoint releases only this admitted helper
            // after the failed oracle, without any post-reap numeric signal.
            let deadline = Instant::now() + super::OBSERVATION_BOUND;
            let release = control
                .send(&json!({"phase": "release", "token": token}), deadline)
                .and_then(|()| control.read(deadline))
                .and_then(|record| require_record(&record, "helper-released", &token))
                .and_then(|()| await_helper_absent(helper, deadline));
            println!("KELD_AC7_HELPER_FALLBACK release={release:?}");
        }
    });
    let report = observer.finish();
    println!("KELD_AC7_HELPER_CUSTODY observer_status={status:?} report={report}");
    assert!(
        absent.is_ok(),
        "group-only cleanup did not remove held helper before fallback"
    );
    assert!(
        status
            .as_ref()
            .is_ok_and(|status| status.signal() == Some(9))
    );
    assert_eq!(report["observer_root_absent"], true);
}

struct NativeChild {
    process: ReapedRouterProbe,
    group: Pid,
    armed: bool,
}

impl NativeChild {
    fn own(process: std::process::Child) -> io::Result<Self> {
        let group = Pid::from_raw(
            i32::try_from(process.id()).map_err(|error| protocol_error(error.to_string()))?,
        );
        let mut owned = Self {
            process: ReapedRouterProbe(process),
            group,
            armed: false,
        };
        if group == getpgrp() || getpgid(Some(group)).map_err(io::Error::other)? != group {
            return Err(protocol_error("new owned group identity mismatch"));
        }
        owned.armed = true;
        Ok(owned)
    }

    fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        let status = self.process.0.try_wait()?;
        if status.is_some() {
            self.armed = false;
        }
        Ok(status)
    }

    fn reap(&mut self) -> io::Result<ExitStatus> {
        // Disarm before every reaping wait, including its error path.
        self.armed = false;
        self.process.0.wait()
    }

    fn stop_unreaped_group(
        &mut self,
        observer: Option<&mut ProcessObserver>,
    ) -> io::Result<ExitStatus> {
        let group = self.group;
        self.stop_unreaped_group_observed(|signal| {
            if let Some(observer) = observer {
                observer.before_reap(group, signal);
            }
        })
    }

    fn stop_unreaped_group_observed(
        &mut self,
        before_reap: impl FnOnce(Result<(), Errno>),
    ) -> io::Result<ExitStatus> {
        // Only the owned, unreaped leader reserves this numerical identity.
        // Reaping permanently removes group-signalling authority.
        if self.armed {
            let signal = killpg(self.group, Signal::SIGKILL);
            // Callers collect proof errors here; no assertion may skip reap.
            before_reap(signal);
        }
        self.reap()
    }
}

impl Drop for NativeChild {
    fn drop(&mut self) {
        if self.armed {
            let _ = self.stop_unreaped_group(None);
        }
    }
}

pub(super) fn is_native_child(case: &str) -> bool {
    if std::env::var(CHILD_ENV).ok().as_deref() != Some(case) {
        return false;
    }
    assert_eq!(
        getpgrp(),
        getpid(),
        "native child must own its isolated group"
    );
    let expected =
        fs::canonicalize(std::env::var_os(ROOT_ENV).expect("exact test child's parent-owned root"))
            .expect("canonical parent-owned root");
    let actual = fs::canonicalize(std::env::temp_dir()).expect("canonical child temp_dir");
    assert_eq!(
        actual, expected,
        "child did not use its parent's controlled temp root"
    );
    println!(
        "KELD_AC7_CHILD_STARTED case={case} pid={} group={} temp_root={}",
        getpid(),
        getpgrp(),
        actual.display()
    );
    true
}

pub(super) fn record_owned_caller(case: &str, child: &std::process::Child) {
    process::record_owned_caller(case, child);
}

pub(super) fn record_owned_caller_exit(pid: u32, status: ExitStatus) {
    let case = std::env::var(CHILD_ENV).expect("exact native child case");
    process::record_owned_caller_exit(&case, pid, status);
}

fn captured(file: &mut File) -> String {
    file.seek(SeekFrom::Start(0))
        .expect("rewind native child log");
    let mut value = String::new();
    file.read_to_string(&mut value)
        .expect("read native child log");
    value
}

fn spawn_native_child(
    case: &str,
    selector: &str,
    root: &Path,
    stdout: &File,
    stderr: &File,
    observer: Option<&ProcessObserver>,
) -> NativeChild {
    let mut command = Command::new(std::env::current_exe().expect("exact native test binary"));
    command
        .args([
            "--exact",
            selector,
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD_ENV, case)
        .env(ROOT_ENV, root)
        .env("TMPDIR", root)
        .stdin(Stdio::null())
        .stdout(Stdio::from(
            stdout.try_clone().expect("native stdout clone"),
        ))
        .stderr(Stdio::from(
            stderr.try_clone().expect("native stderr clone"),
        ))
        .process_group(0);
    if let Some(observer) = observer {
        command
            .env(OBSERVER_SOCKET_ENV, observer.socket())
            .env(OBSERVER_TOKEN_ENV, observer.token());
    }
    let child = command.spawn().expect("spawn exact native proof child");
    let child = NativeChild::own(child).expect("verify exact spawned native group");
    if let Some(observer) = observer {
        assert_ne!(
            child.group,
            observer.group(),
            "observer must not enter the target group"
        );
    }
    child
}

fn await_native_child(
    child: &mut NativeChild,
    mut observer: Option<&mut ProcessObserver>,
) -> (ExitStatus, bool) {
    let started = Instant::now();
    let deadline = started + CHILD_BOUND;
    if let Some(observer) = observer.as_deref_mut() {
        observer.set_origin(started);
    }
    loop {
        if Instant::now() >= deadline {
            // Preserve successful completion priority at the original deadline.
            if let Some(status) = child.try_wait().expect("observe native child exit") {
                return (status, false);
            }
            return (
                child
                    .stop_unreaped_group(observer.as_deref_mut())
                    .expect("reap watchdog-stopped native child"),
                true,
            );
        }
        if let Some(observer) = observer.as_deref_mut()
            && !observer.has_failed()
        {
            let observation_deadline = deadline.min(Instant::now() + super::OBSERVATION_BOUND);
            if let Err(error) = observer.poll(child, observation_deadline) {
                observer.fail(&error);
                let expired = Instant::now() >= deadline;
                return (
                    child
                        .stop_unreaped_group(Some(observer))
                        .expect("reap after failed observer prerequisite"),
                    expired,
                );
            }
        }
        if let Some(status) = child.try_wait().expect("observe native child exit") {
            return (status, false);
        }
        if Instant::now() >= deadline {
            return (
                child
                    .stop_unreaped_group(observer.as_deref_mut())
                    .expect("reap watchdog-stopped native child"),
                true,
            );
        }
        // Completion polling only; this deadline is a failure kill switch.
        std::thread::park_timeout(Duration::from_millis(10));
    }
}

struct NativeReport {
    status: ExitStatus,
    timed_out: bool,
    group_diagnostic: Result<(), Errno>,
    output: String,
    detail: String,
    observation: Option<Value>,
}

fn finish_case(
    case: &str,
    selector: &str,
    root: tempfile::TempDir,
    canonical_root: &Path,
    report: &NativeReport,
) {
    let normal_root_empty = fs::read_dir(canonical_root)
        .and_then(|mut entries| entries.next().transpose().map(|entry| entry.is_none()));
    // Preserve this normal-cleanup oracle before recursive parent fallback.
    let close = root.close();
    let absent = matches!(
        fs::symlink_metadata(canonical_root),
        Err(error) if error.kind() == io::ErrorKind::NotFound
    );
    let normal = !report.timed_out
        && report
            .observation
            .as_ref()
            .is_none_or(|record| record["ok"] == true)
        && report.group_diagnostic == Err(Errno::ESRCH)
        && report.status.success()
        && report
            .output
            .contains(&format!("KELD_AC7_CHILD_STARTED case={case} "))
        && report
            .output
            .contains(&format!("KELD_AC7_CASE_DONE case={case} "))
        && report
            .output
            .contains("test result: ok. 1 passed; 0 failed;");
    println!(
        "KELD_AC7_CHILD_CAPTURE selector={selector} status={:?} watchdog={} group_diagnostic={:?} normal_root_empty={normal_root_empty:?} parent_close={close:?} root_absent={absent}\nprocess_observation={:?}\nstdout={}\nstderr={}",
        report.status,
        report.timed_out,
        report.group_diagnostic,
        report.observation,
        report.output,
        report.detail
    );
    assert!(
        normal && matches!(normal_root_empty, Ok(true)) && close.is_ok() && absent,
        "native child failed: selector={selector} status={:?} watchdog={} group_diagnostic={:?} normal_root_empty={normal_root_empty:?} parent_close={close:?} root_absent={absent}\nstdout={}\nstderr={}",
        report.status,
        report.timed_out,
        report.group_diagnostic,
        report.output,
        report.detail
    );
}

pub(super) fn isolated_case(case: &str, selector: &str) {
    // Anonymous parent logs stay outside the root whose emptiness is observed.
    let mut stdout = tempfile::tempfile().expect("owned native stdout");
    let mut stderr = tempfile::tempfile().expect("owned native stderr");
    let root = tempfile::Builder::new()
        .prefix("k140c-")
        .tempdir_in("/tmp")
        .expect("parent-owned short native child root");
    let canonical_root = fs::canonicalize(root.path()).expect("canonical parent temp root");
    let mut observer = ProcessObserver::start(case).expect("portable process observer startup");
    let mut child = spawn_native_child(
        case,
        selector,
        &canonical_root,
        &stdout,
        &stderr,
        Some(&observer),
    );
    let (status, timed_out) = await_native_child(&mut child, Some(&mut observer));
    observer.record_leader_reaped(&child, status);
    // The leader is reaped and signalling is disarmed. A residual/error group
    // is diagnostic FAILED, never a reason to signal a cached numerical PGID.
    // Prearmed statuses qualify only their exact known processes; a complete
    // historical/atomic group census and EPERM cause remain separate unknowns.
    let group_diagnostic = killpg(child.group, None::<Signal>);
    let detail = captured(&mut stderr);
    observer.capture_owned_caller_exit(&detail, status.success() && !timed_out);
    let report = NativeReport {
        status,
        timed_out,
        group_diagnostic,
        output: captured(&mut stdout),
        detail,
        observation: Some(observer.finish()),
    };
    drop(child);
    finish_case(case, selector, root, &canonical_root, &report);
}

/// Reuses exact-self group/watchdog custody; the retained real guardian owns Bun.
/// No legacy direct-Bun `ProcessObserver` enrollment is inferred for this case.
pub(super) fn isolated_guardian_case(case: &str, selector: &str) {
    let mut stdout = tempfile::tempfile().expect("owned guardian-native stdout");
    let mut stderr = tempfile::tempfile().expect("owned guardian-native stderr");
    let root = tempfile::Builder::new()
        .prefix("k143c-")
        .tempdir_in("/tmp")
        .expect("parent-owned guardian-native child root");
    let canonical_root = fs::canonicalize(root.path()).expect("canonical parent temp root");
    let mut child = spawn_native_child(case, selector, &canonical_root, &stdout, &stderr, None);
    let (status, timed_out) = await_native_child(&mut child, None);
    let report = NativeReport {
        status,
        timed_out,
        group_diagnostic: killpg(child.group, None::<Signal>),
        output: captured(&mut stdout),
        detail: captured(&mut stderr),
        observation: None,
    };
    drop(child);
    finish_case(case, selector, root, &canonical_root, &report);
}

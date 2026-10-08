//! Real Windows proof of the `PerUserDirect` candidate launch (KEL-53 §5, §6 S6b): the
//! same-token suspended creation, the candidate's attempt-Job membership before its first
//! instruction, the launch handle's liveness check and the process-object cells of the
//! claimant-binding row at the owner.
//!
//! Oracles are independent Win32 observations made here: the child's token facts read
//! through its own process handle, a handle-value probe of the child's handle table,
//! Job accounting, the child's exit code and the claimant refusal class.

#![cfg(windows)]
#![allow(unsafe_code)] // isolated test-only token, handle-table and session observation with local ABI proofs
#![allow(clippy::expect_used, clippy::panic)] // process fixture invariants must abort loudly
#![deny(unsafe_op_in_unsafe_fn)]

#[path = "support/windows_standard_handles.rs"]
mod windows_standard_handles;

use std::ffi::OsString;
use std::io::{BufRead as _, BufReader, Write as _};
use std::net::{TcpListener, TcpStream};
use std::os::windows::io::{AsRawHandle as _, FromRawHandle as _, OwnedHandle};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use keld_runtime::windows_job::{
    WindowsClaimantRefusal, WindowsJobMembership, WindowsLaunchedProcess, WindowsProcessJob,
    WindowsProcessPeer,
};
use keld_runtime::windows_lpac::{WindowsLpacTokenObservation, WindowsSuspendedChild};
use windows_standard_handles::{
    CANDIDATE_STDOUT_MARKER, KILL_SWITCH, NO_STANDARD_HANDLE_REPORT, drain,
    exits_within_kill_switch, standard_handle_report,
};
use windows_sys::Win32::Foundation::{
    CompareObjectHandles, DUPLICATE_SAME_ACCESS, DuplicateHandle, HANDLE, HANDLE_FLAG_INHERIT,
    SetHandleInformation,
};
use windows_sys::Win32::Security::{
    EqualSid, GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TOKEN_STATISTICS, TOKEN_USER,
    TokenElevation, TokenIsAppContainer, TokenStatistics, TokenUser,
};
use windows_sys::Win32::System::RemoteDesktop::ProcessIdToSessionId;
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

/// The exit code the candidate's command sets; any other code means it never ran it.
const COMMAND_EXIT: u32 = 7;
/// `TerminateJobObject`'s exit code in `WindowsProcessJob::terminate_and_wait`.
const JOB_TERMINATION_EXIT: u32 = 1;
const WAIT_MS: u32 = 10_000;
/// Selects a private subprocess entry point of this binary.
const HELPER_ENV: &str = "KELD_WINDOWS_CANDIDATE_LAUNCH_HELPER";
/// The test's report listener port, handed to the launcher and on to the candidate.
const REPORT_PORT_ENV: &str = "KELD_WINDOWS_CANDIDATE_LAUNCH_REPORT_PORT";
/// The test's go-ahead byte that ends the candidate's hold.
const GO: u8 = b'G';

fn system32() -> PathBuf {
    PathBuf::from(std::env::var_os("SystemRoot").expect("SystemRoot is set")).join("System32")
}

/// The command interpreter's minimal complete environment block.
fn environment() -> Vec<(OsString, OsString)> {
    ["SystemRoot", "WINDIR"]
        .into_iter()
        .filter_map(|key| std::env::var_os(key).map(|value| (OsString::from(key), value)))
        .collect()
}

fn fixture_args(fixture: &str) -> [OsString; 4] {
    [
        OsString::from("--exact"),
        OsString::from(fixture),
        OsString::from("--ignored"),
        OsString::from("--nocapture"),
    ]
}

/// Creates a same-token suspended `cmd.exe /d /c exit 7`.
fn suspended_exit_command() -> WindowsSuspendedChild {
    let system32 = system32();
    WindowsSuspendedChild::spawn_same_token(
        &system32.join("cmd.exe"),
        &[
            OsString::from("/d"),
            OsString::from("/c"),
            OsString::from(format!("exit {COMMAND_EXIT}")),
        ],
        &environment(),
        &system32,
    )
    .expect("create the same-token candidate suspended")
}

/// A recorded same-token launch assigned to its own attempt Job, not yet resumed.
fn launched_member() -> (
    WindowsProcessJob,
    WindowsLaunchedProcess,
    WindowsJobMembership,
) {
    let mut job = WindowsProcessJob::create().expect("create the attempt Job");
    let launched =
        WindowsLaunchedProcess::record(suspended_exit_command()).expect("record the launch");
    let membership = job
        .assign_child(launched.child())
        .expect("assign the suspended candidate to the attempt Job");
    (job, launched, membership)
}

#[test]
fn the_candidate_runs_under_the_callers_own_token() {
    let child = suspended_exit_command();
    let child_process: HANDLE = child.process_handle().as_raw_handle().cast();
    // SAFETY: GetCurrentProcess returns a pseudo-handle that needs no close.
    let own_process = unsafe { GetCurrentProcess() };
    let child_token = TokenFacts::of(child_process);
    let own_token = TokenFacts::of(own_process);
    assert_eq!(
        child_token.authentication_id, own_token.authentication_id,
        "the candidate runs in the caller's own logon session"
    );
    assert!(
        child_token.same_user(&own_token),
        "the candidate's TokenUser is the caller's"
    );
    assert_eq!(child_token.elevated, own_token.elevated);
    assert!(!child_token.app_container, "no AppContainer token");
    assert_eq!(
        child.observe_token().expect("observe the candidate token"),
        WindowsLpacTokenObservation {
            is_app_container: false,
            all_application_packages_opt_out_configured: false,
            capability_count: 0,
        },
        "the same-token launch supplies no LPAC creation attribute"
    );
}

#[test]
fn the_candidate_inherits_no_handle_of_the_caller() {
    let marker = std::fs::OpenOptions::new()
        .read(true)
        .open(system32().join("cmd.exe"))
        .expect("open an inheritable marker");
    let marker_value: HANDLE = marker.as_raw_handle().cast();
    // SAFETY: the marker handle is live and owned here; only its inherit bit changes.
    assert_ne!(
        unsafe { SetHandleInformation(marker_value, HANDLE_FLAG_INHERIT, HANDLE_FLAG_INHERIT) },
        0,
        "make the marker inheritable: {}",
        std::io::Error::last_os_error()
    );
    let child = suspended_exit_command();
    // An inherited handle keeps its value in the child, so the child's table holds the
    // marker object at the marker's value exactly when it was inherited.
    assert!(
        !child_holds_object_at(&child, marker_value),
        "the caller's inheritable marker crossed into the candidate"
    );
}

#[test]
fn the_candidate_is_an_attempt_job_member_before_its_first_instruction() {
    // Never resumed: the Job admits it and then ends it.
    let mut job = WindowsProcessJob::create().expect("create the attempt Job");
    let mut launched =
        WindowsLaunchedProcess::record(suspended_exit_command()).expect("record the launch");
    assert_eq!(job.active_processes().expect("Job accounting"), 0);
    // This member is never resumed, so its membership proof goes unused.
    let _unused_proof = job
        .assign_child(launched.child())
        .expect("assign the suspended candidate to the attempt Job");
    assert_eq!(
        job.active_processes().expect("Job accounting"),
        1,
        "the suspended candidate is the Job's one member"
    );
    assert!(
        job.contains_child(launched.child())
            .expect("query membership"),
        "membership reads back while the primary thread was never resumed"
    );
    job.terminate_and_wait(launched.child(), Duration::from_secs(10))
        .expect("the Job ends its suspended member and reaches zero");
    assert_eq!(
        launched.wait(WAIT_MS).expect("reap the candidate"),
        JOB_TERMINATION_EXIT,
        "the Job ends its never-resumed member, which never ran its command"
    );

    // Assigned with read-back, then resumed: the resume reports a previous suspend
    // count of 1, so the primary thread ran nothing before its Job membership.
    let (job, mut launched, membership) = launched_member();
    launched
        .resume(&membership)
        .expect("resume the member once, from a previous suspend count of 1");
    assert_eq!(
        launched.wait(WAIT_MS).expect("the candidate exits"),
        COMMAND_EXIT
    );
    job.terminate_and_wait(launched.child(), Duration::from_secs(10))
        .expect("an exited member leaves the Job at zero");
    assert!(
        launched.resume(&membership).is_err(),
        "the one resume is spent: the suspended-child type owns it"
    );
}

#[test]
fn a_launch_resumes_only_with_the_membership_proof_of_its_own_process() {
    let (_job, mut launched, membership) = launched_member();
    let (_other_job, _other, other_membership) = launched_member();
    let refusal = launched
        .resume(&other_membership)
        .expect_err("another process's membership proof refuses");
    assert_eq!(
        refusal.to_string(),
        "KELD-RUNTIME-020: the same-token suspended candidate launch failed during child \
         resume: the attempt-Job membership proof names another process. Start no candidate \
         in its place: end the attempt Job and roll the attempt back; never resume a child \
         whose creation, Job membership or launch record was not proved."
    );
    assert!(
        !launched.has_exited().expect("query the launch"),
        "the refused launch stays suspended"
    );
    // The resume was not spent: its own proof still resumes it.
    launched
        .resume(&membership)
        .expect("its own membership proof resumes it");
    assert_eq!(
        launched.wait(WAIT_MS).expect("the launch exits"),
        COMMAND_EXIT
    );
}

#[test]
fn a_bare_same_token_resume_refuses_and_leaves_the_child_to_its_launch_record() {
    let mut child = suspended_exit_command();
    let refusal = child
        .resume()
        .expect_err("a same-token child never runs outside its launch record");
    assert_eq!(
        refusal.to_string(),
        "KELD-RUNTIME-020: the same-token suspended candidate launch failed during child \
         resume: a same-token candidate resumes only through its launch record and \
         attempt-Job membership proof. Start no candidate in its place: end the attempt Job \
         and roll the attempt back; never resume a child whose creation, Job membership or \
         launch record was not proved."
    );
    // The refusal ran and spent nothing: the record and its Job membership resume it.
    let mut job = WindowsProcessJob::create().expect("create the attempt Job");
    let mut launched = WindowsLaunchedProcess::record(child)
        .expect("a refused bare resume leaves the child unresumed");
    assert!(
        !launched.has_exited().expect("query the launch"),
        "the child is still suspended"
    );
    let membership = job
        .assign_child(launched.child())
        .expect("assign the suspended candidate");
    launched
        .resume(&membership)
        .expect("its launch record resumes it");
    assert_eq!(
        launched.wait(WAIT_MS).expect("the candidate exits"),
        COMMAND_EXIT
    );
}

#[test]
fn the_launch_liveness_check_reads_the_retained_handle_without_consuming_it() {
    let (_job, mut launched, membership) = launched_member();
    for _ in 0..2 {
        assert!(
            !launched.has_exited().expect("query the live launch"),
            "a suspended launch is live"
        );
    }
    launched.resume(&membership).expect("resume the launch");
    assert_eq!(
        launched.wait(WAIT_MS).expect("the launch exits"),
        COMMAND_EXIT
    );
    for _ in 0..2 {
        assert!(
            launched.has_exited().expect("query the exited launch"),
            "an exited launch is signaled, every time it is asked"
        );
    }
    assert_eq!(
        launched
            .wait(0)
            .expect("the retained handle still reports the exit"),
        COMMAND_EXIT,
        "the liveness check consumed nothing"
    );
}

#[test]
fn the_owner_binds_only_its_launched_candidate_and_only_while_it_lives() {
    let (_job, mut launched, membership) = launched_member();
    let claimant = claimant_by_pid(launched.child().id());
    launched
        .bind_claimant(&claimant)
        .expect("the exact same-token launch binds");

    // A second instance of the candidate image, started by the same launch path.
    let copy = WindowsLaunchedProcess::record(suspended_exit_command()).expect("record a copy");
    let refusal = launched.bind_claimant(&claimant_by_pid(copy.child().id()));
    assert!(
        matches!(refusal, Err(WindowsClaimantRefusal::NotLaunchedProcess)),
        "{refusal:?}"
    );

    launched.resume(&membership).expect("resume the launch");
    assert_eq!(
        launched.wait(WAIT_MS).expect("the launch exits"),
        COMMAND_EXIT
    );
    let refusal = launched.bind_claimant(&claimant);
    assert!(
        matches!(refusal, Err(WindowsClaimantRefusal::LaunchExited)),
        "{refusal:?}"
    );
    assert!(launched.has_exited().expect("query the launch"));
}

#[test]
fn a_candidate_launch_refusal_is_typed_runtime_020_before_any_process_exists() {
    let system32 = system32();
    let relative = WindowsSuspendedChild::spawn_same_token(
        Path::new("cmd.exe"),
        &[],
        &environment(),
        &system32,
    )
    .expect_err("a relative program path refuses");
    assert_eq!(
        relative.to_string(),
        "KELD-RUNTIME-020: the same-token suspended candidate launch failed during \
         application path: path is not absolute. Start no candidate in its place: end the \
         attempt Job and roll the attempt back; never resume a child whose creation, Job \
         membership or launch record was not proved."
    );
    let missing = WindowsSuspendedChild::spawn_same_token(
        &system32.join("keld-absent-candidate.exe"),
        &[],
        &environment(),
        &system32,
    )
    .expect_err("an absent image refuses");
    let text = missing.to_string();
    assert!(
        text.starts_with(
            "KELD-RUNTIME-020: the same-token suspended candidate launch failed during \
             CreateProcessW same-token launch: "
        ),
        "{text}"
    );
    let relative_directory = WindowsSuspendedChild::spawn_same_token(
        &system32.join("cmd.exe"),
        &[],
        &environment(),
        Path::new("tree"),
    )
    .expect_err("a relative working directory refuses");
    assert!(
        relative_directory
            .to_string()
            .contains("during current directory: path is not absolute"),
        "{relative_directory}"
    );
}

/// KEL-270 F51: the launcher's three standard handles are pipes, as a host's are under
/// `keld dev` or a test runner, and it starts the candidate through the same-token
/// launch. The candidate holds none of them: its own `GetStdHandle` reports NULL three
/// times and its standard-output write fails; no candidate byte reaches the launcher's
/// pipes; and those pipes reach EOF once the launcher exits while the released
/// candidate still runs. Without `STARTF_USESTDHANDLES` process creation duplicates a
/// parent's non-console standard handles into the child even with inheritance off, and
/// the released candidate then holds the launcher's log pipe until its own exit.
#[test]
fn the_candidate_holds_no_standard_handle_of_the_launcher() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind the report listener");
    let port = listener.local_addr().expect("listener address").port();
    let mut launcher = Command::new(std::env::current_exe().expect("current test executable"))
        .args(fixture_args("launcher_fixture"))
        .env(HELPER_ENV, "launcher")
        .env(REPORT_PORT_ENV, port.to_string())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn the launcher fixture");
    // Held to the end, so the launcher's standard input is a live pipe throughout.
    let stdin_writer = launcher.stdin.take().expect("launcher stdin");
    let stdout = drain(launcher.stdout.take().expect("launcher stdout"));
    let stderr = drain(launcher.stderr.take().expect("launcher stderr"));

    let mut report = Report::accept(&listener);
    let candidate = claimant_by_pid(
        report
            .line("CANDIDATE ")
            .parse()
            .expect("the candidate's process ID"),
    );
    let handles = report.line("STDHANDLES ");

    assert!(
        exits_within_kill_switch(&launcher),
        "the launcher did not exit after releasing the candidate"
    );
    let status = launcher.wait().expect("reap the launcher");
    assert!(status.success(), "launcher failed: {status}");
    assert!(
        !candidate.has_exited().expect("query the candidate"),
        "the released candidate outlives the launcher"
    );

    // Every contract fact is collected before the first failure, so one run of the
    // unfixed code shows the candidate's handles and what they did to the pipes.
    let mut defects = Vec::new();
    if handles != NO_STANDARD_HANDLE_REPORT {
        defects.push(format!(
            "the candidate holds standard handles: {handles} (expected {NO_STANDARD_HANDLE_REPORT})"
        ));
    }
    let marker = String::from_utf8_lossy(CANDIDATE_STDOUT_MARKER);
    for (name, pipe) in [("output", stdout), ("error", stderr)] {
        match pipe.recv_timeout(KILL_SWITCH) {
            Ok(bytes) => {
                let text = String::from_utf8_lossy(&bytes);
                if name == "output" && !text.contains("LAUNCHER_RELEASED ") {
                    defects.push(format!(
                        "the launcher's release record is missing: {text:?}"
                    ));
                }
                if text.contains(&*marker) {
                    defects.push(format!(
                        "candidate bytes reached the launcher's standard {name}: {text:?}"
                    ));
                }
            }
            Err(_) => defects.push(format!(
                "the launcher's standard {name} reached no EOF within {KILL_SWITCH:?} of the \
                 launcher's exit: the released candidate holds the launcher's pipe"
            )),
        }
    }
    assert!(defects.is_empty(), "{}", defects.join("\n"));

    report.go();
    assert!(
        candidate
            .wait_until_exited(KILL_SWITCH)
            .expect("wait for the candidate"),
        "the candidate did not exit after the go-ahead"
    );
    drop(stdin_writer);
}

/// The launcher stand-in: creates the candidate through the same-token launch as the
/// `PerUserDirect` host does, assigned to its attempt Job before its first instruction,
/// then releases the Job and exits. It exits through `process::exit`: the retained
/// launch record would otherwise terminate the released candidate.
#[test]
#[ignore = "private subprocess entry point"]
fn launcher_fixture() {
    assert_eq!(
        std::env::var(HELPER_ENV).as_deref(),
        Ok("launcher"),
        "unexpected private launcher fixture entry"
    );
    let port = std::env::var(REPORT_PORT_ENV).expect("report port");
    let exe = std::env::current_exe().expect("current test executable");
    let mut environment: Vec<(OsString, OsString)> = ["SystemRoot", "WINDIR", "PATH"]
        .into_iter()
        .filter_map(|key| std::env::var_os(key).map(|value| (OsString::from(key), value)))
        .collect();
    environment.push((OsString::from(HELPER_ENV), OsString::from("candidate")));
    environment.push((OsString::from(REPORT_PORT_ENV), OsString::from(port)));
    let child = WindowsSuspendedChild::spawn_same_token(
        &exe,
        &fixture_args("candidate_fixture"),
        &environment,
        exe.parent().expect("test executable directory"),
    )
    .expect("create the candidate suspended");
    let mut job = WindowsProcessJob::create().expect("create the attempt Job");
    let mut launched = WindowsLaunchedProcess::record(child).expect("record the launch");
    let membership = job
        .assign_child(launched.child())
        .expect("assign the candidate before its first instruction");
    launched.resume(&membership).expect("resume the candidate");
    let _released = job.release_family().expect("release the attempt Job");
    println!("LAUNCHER_RELEASED {}", launched.child().id());
    std::io::stdout()
        .flush()
        .expect("flush the launcher record");
    std::process::exit(0);
}

/// The candidate stand-in: reports its process ID and its three standard handles over
/// the test's listener, never through a standard handle, then holds whatever it was
/// given until the test's go-ahead or the report's close.
#[test]
#[ignore = "private subprocess entry point"]
fn candidate_fixture() {
    assert_eq!(
        std::env::var(HELPER_ENV).as_deref(),
        Ok("candidate"),
        "unexpected private candidate fixture entry"
    );
    let port: u16 = std::env::var(REPORT_PORT_ENV)
        .expect("report port")
        .parse()
        .expect("numeric report port");
    let mut report = TcpStream::connect(("127.0.0.1", port)).expect("connect the report");
    writeln!(report, "CANDIDATE {}", std::process::id()).expect("report the process ID");
    writeln!(report, "STDHANDLES {}", standard_handle_report())
        .expect("report the standard handles");
    report.flush().expect("flush the report");
    let mut go = [0_u8; 1];
    let _ = std::io::Read::read(&mut report, &mut go);
    let _ = report.shutdown(std::net::Shutdown::Both);
    std::process::exit(0);
}

/// The test side of the candidate's report: one line-oriented connection. Dropping it
/// closes the candidate's hold.
struct Report {
    reader: BufReader<TcpStream>,
    stream: TcpStream,
}

impl Report {
    /// Accepts the candidate's connection within the kill switch.
    fn accept(listener: &TcpListener) -> Self {
        let (sender, receiver) = std::sync::mpsc::channel();
        let listener = listener.try_clone().expect("clone the report listener");
        std::thread::spawn(move || {
            let _ = sender.send(listener.accept());
        });
        let (stream, _) = receiver
            .recv_timeout(KILL_SWITCH)
            .expect("the candidate must connect its report within the kill switch")
            .expect("accept the candidate report");
        stream
            .set_read_timeout(Some(KILL_SWITCH))
            .expect("bound report reads");
        let reader = BufReader::new(stream.try_clone().expect("clone the report stream"));
        Self { reader, stream }
    }

    /// The rest of the next report line that starts with `prefix`.
    fn line(&mut self, prefix: &str) -> String {
        loop {
            let mut line = String::new();
            let read = self
                .reader
                .read_line(&mut line)
                .expect("read the candidate report");
            assert_ne!(
                read, 0,
                "the report closed before a line starting with {prefix:?}"
            );
            if let Some(rest) = line.trim_end().strip_prefix(prefix) {
                return rest.to_owned();
            }
        }
    }

    /// Ends the candidate's hold.
    fn go(&mut self) {
        self.stream.write_all(&[GO]).expect("send the go-ahead");
    }
}

/// Opens a claimant exactly as the owner does: from the process ID and session that
/// the connected pipe reports.
fn claimant_by_pid(process_id: u32) -> WindowsProcessPeer {
    let mut session_id = 0_u32;
    // SAFETY: the PID names a live test-owned process; the output is writable.
    assert_ne!(
        unsafe { ProcessIdToSessionId(process_id, &raw mut session_id) },
        0,
        "query the claimant session: {}",
        std::io::Error::last_os_error()
    );
    WindowsProcessPeer::open(process_id, session_id).expect("open the claimant by PID")
}

/// Whether `child`'s handle table holds, at `value`, the object that `value` names in
/// this process.
fn child_holds_object_at(child: &WindowsSuspendedChild, value: HANDLE) -> bool {
    let mut duplicate = std::ptr::null_mut();
    // SAFETY: the child's process handle and the current-process pseudo-handle are live;
    // `value` is only resolved in the child's table; `duplicate` is writable storage.
    if unsafe {
        DuplicateHandle(
            child.process_handle().as_raw_handle().cast(),
            value,
            GetCurrentProcess(),
            &raw mut duplicate,
            0,
            0,
            DUPLICATE_SAME_ACCESS,
        )
    } == 0
        || duplicate.is_null()
    {
        return false;
    }
    // SAFETY: a successful DuplicateHandle returned one fresh owning handle.
    let duplicate = unsafe { OwnedHandle::from_raw_handle(duplicate.cast()) };
    // SAFETY: both compared handles are live for this call.
    (unsafe { CompareObjectHandles(value, duplicate.as_raw_handle().cast()) }) != 0
}

/// Token facts read through a process handle with `TOKEN_QUERY`.
struct TokenFacts {
    authentication_id: (u32, i32),
    elevated: bool,
    app_container: bool,
    user: Vec<usize>,
}

impl TokenFacts {
    fn of(process: HANDLE) -> Self {
        let mut raw_token = std::ptr::null_mut();
        // SAFETY: the process handle is live; the output receives one token handle.
        assert_ne!(
            unsafe { OpenProcessToken(process, TOKEN_QUERY, &raw mut raw_token) },
            0,
            "open the process token: {}",
            std::io::Error::last_os_error()
        );
        // SAFETY: a successful OpenProcessToken returned one fresh owning handle.
        let token = unsafe { OwnedHandle::from_raw_handle(raw_token.cast()) };
        let statistics = token_information(&token, TokenStatistics);
        // SAFETY: the buffer is aligned and filled for the TOKEN_STATISTICS class.
        let luid = unsafe { (*statistics.as_ptr().cast::<TOKEN_STATISTICS>()).AuthenticationId };
        let elevation = token_information(&token, TokenElevation);
        // SAFETY: the buffer is aligned and filled for the TOKEN_ELEVATION class.
        let elevated = unsafe { (*elevation.as_ptr().cast::<TOKEN_ELEVATION>()).TokenIsElevated };
        let app_container = token_information(&token, TokenIsAppContainer);
        // SAFETY: the buffer is aligned and filled with the class's DWORD.
        let app_container = unsafe { *app_container.as_ptr().cast::<u32>() };
        Self {
            authentication_id: (luid.LowPart, luid.HighPart),
            elevated: elevated != 0,
            app_container: app_container != 0,
            user: token_information(&token, TokenUser),
        }
    }

    fn same_user(&self, other: &Self) -> bool {
        // SAFETY: both buffers hold a TOKEN_USER whose SID points into the same live
        // buffer; EqualSid only reads the two SIDs.
        unsafe {
            EqualSid(
                (*self.user.as_ptr().cast::<TOKEN_USER>()).User.Sid,
                (*other.user.as_ptr().cast::<TOKEN_USER>()).User.Sid,
            ) != 0
        }
    }
}

fn token_information(token: &OwnedHandle, class: i32) -> Vec<usize> {
    let mut bytes = 0_u32;
    // SAFETY: the sizing call writes only the returned length.
    let _ = unsafe {
        GetTokenInformation(
            token.as_raw_handle().cast(),
            class,
            std::ptr::null_mut(),
            0,
            &raw mut bytes,
        )
    };
    assert_ne!(bytes, 0, "size token class {class}");
    let mut storage = vec![
        0_usize;
        usize::try_from(bytes)
            .expect("fits")
            .div_ceil(size_of::<usize>())
    ];
    // SAFETY: the aligned storage has at least `bytes` writable bytes.
    assert_ne!(
        unsafe {
            GetTokenInformation(
                token.as_raw_handle().cast(),
                class,
                storage.as_mut_ptr().cast(),
                bytes,
                &raw mut bytes,
            )
        },
        0,
        "read token class {class}: {}",
        std::io::Error::last_os_error()
    );
    storage
}

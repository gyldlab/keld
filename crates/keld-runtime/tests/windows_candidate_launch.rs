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

use std::ffi::OsString;
use std::os::windows::io::{AsRawHandle as _, FromRawHandle as _, OwnedHandle};
use std::path::{Path, PathBuf};
use std::time::Duration;

use keld_runtime::windows_job::{
    WindowsClaimantRefusal, WindowsJobMembership, WindowsLaunchedProcess, WindowsProcessJob,
    WindowsProcessPeer,
};
use keld_runtime::windows_lpac::{WindowsLpacTokenObservation, WindowsSuspendedChild};
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

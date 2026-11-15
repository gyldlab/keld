//! Windows updater helper launch contract: DLL search, `ShellExecute`, process identity and argument ownership.

use super::{
    ERROR_CANCELLED, GetProcessId, HANDLE, OpenProcess, OwnedHandle,
    PROCESS_QUERY_LIMITED_INFORMATION, ShellVerb, WAIT_OBJECT_0, WaitForSingleObject,
    WindowsDllSearchError, WindowsElevatedUpdaterHelper, WindowsUpdaterHelperArgument,
    WindowsUpdaterHelperLaunchError, WindowsUpdaterHelperPath, launch_updater_helper,
    nul_terminated_wide, shell_execute_outcome,
};
use std::io;
use std::os::windows::io::{AsHandle as _, AsRawHandle as _, FromRawHandle as _};

#[test]
fn dll_search_refusal_names_its_code_cause_and_exit_guidance() {
    let error = WindowsDllSearchError {
        source: io::Error::from_raw_os_error(87),
    };
    let rendered = error.to_string();
    assert!(
        rendered.starts_with("KELD-RUNTIME-018: restricting this process's DLL search"),
        "{rendered}"
    );
    assert!(rendered.contains("(os error 87)"), "{rendered}");
    assert!(
        rendered.contains("Exit before loading any library"),
        "{rendered}"
    );
    assert_eq!(
        std::error::Error::source(&error)
            .and_then(|source| source.downcast_ref::<io::Error>())
            .and_then(io::Error::raw_os_error),
        Some(87)
    );
}

#[test]
fn shell_execute_outcome_separates_decline_failure_and_a_missing_process_handle() {
    // Only compared, never used as a handle.
    let returned: HANDLE = std::ptr::without_provenance_mut(0x1234);
    let cancelled = || io::Error::from_raw_os_error(ERROR_CANCELLED.cast_signed());
    assert!(matches!(
        shell_execute_outcome(false, cancelled(), 5, std::ptr::null_mut()),
        Err(WindowsUpdaterHelperLaunchError::Declined)
    ));
    match shell_execute_outcome(
        false,
        io::Error::from_raw_os_error(2),
        2,
        std::ptr::null_mut(),
    ) {
        Err(WindowsUpdaterHelperLaunchError::ShellExecute { source, inst_app }) => {
            assert_eq!((source.raw_os_error(), inst_app), (Some(2), 2));
        }
        other => panic!("expected a ShellExecute failure, got {other:?}"),
    }
    // A failed call is a failure even if a handle value is present.
    assert!(matches!(
        shell_execute_outcome(false, io::Error::from_raw_os_error(5), 5, returned),
        Err(WindowsUpdaterHelperLaunchError::ShellExecute { .. })
    ));
    assert!(matches!(
        shell_execute_outcome(true, cancelled(), 42, std::ptr::null_mut()),
        Err(WindowsUpdaterHelperLaunchError::NoProcessHandle)
    ));
    assert_eq!(
        shell_execute_outcome(true, cancelled(), 42, returned).expect("a success with a handle"),
        returned
    );
}

#[test]
fn launched_process_is_retained_only_with_the_id_its_handle_reports() {
    // SAFETY: OpenProcess dereferences no caller memory.
    let raw = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, std::process::id()) };
    assert!(!raw.is_null(), "open this test process");
    // SAFETY: OpenProcess succeeded, so this is the handle's sole owner.
    let process = unsafe { OwnedHandle::from_raw_handle(raw.cast()) };
    let helper =
        WindowsElevatedUpdaterHelper::identify(process).expect("a process handle has an ID");
    assert_eq!(helper.id(), std::process::id());

    let not_a_process = OwnedHandle::from(std::fs::File::open("NUL").expect("open NUL"));
    match WindowsElevatedUpdaterHelper::identify(not_a_process) {
        Err(WindowsUpdaterHelperLaunchError::ProcessIdentity { source }) => {
            assert!(source.raw_os_error().is_some(), "{source}");
        }
        other => panic!("a file handle has no process ID, got {other:?}"),
    }
}

/// Runs the production `ShellExecuteExW` block with the `open` verb, which
/// needs no consent prompt. The image is a `.cmd` script that the shell
/// starts through its association (`cmd.exe`): it records the arguments and
/// working directory it received, then exits 7. The closed path type admits
/// only `.exe`, so this test builds the path value directly.
#[test]
fn shell_launch_owns_the_process_handle_and_passes_the_argument_and_directory() {
    let fixture = tempfile::tempdir().expect("launch fixture");
    let directory = fixture.path().join("helper dir");
    std::fs::create_dir(&directory).expect("helper directory");
    let script = directory.join("probe.cmd");
    std::fs::write(
        &script,
        "@echo off\r\n>\"%~dp0observed.txt\" (\r\n echo ARG1=%~1\r\n echo ALL=%*\r\n \
         echo CWD=%CD%\r\n)\r\nexit /b 7\r\n",
    )
    .expect("probe script");
    let path = WindowsUpdaterHelperPath {
        path: script.clone(),
        file: nul_terminated_wide(script.as_os_str()).expect("script path"),
        directory: nul_terminated_wide(directory.as_os_str()).expect("script directory"),
    };
    let rendezvous = format!(r"\\.\pipe\keld-attempt-{}", "0123456789abcdef".repeat(4));
    let argument = WindowsUpdaterHelperArgument::activation(&rendezvous).expect("rendezvous");

    let launched =
        launch_updater_helper(&path, &argument, ShellVerb::Open).expect("open-verb launch");
    let handle: HANDLE = launched.as_handle().as_raw_handle().cast();
    // SAFETY: `launched` retains `handle` for each call below.
    assert_eq!(unsafe { GetProcessId(handle) }, launched.id());
    // SAFETY: as above; the wait is bounded.
    assert_eq!(
        unsafe { WaitForSingleObject(handle, 60_000) },
        WAIT_OBJECT_0
    );
    let mut exit_code = u32::MAX;
    // SAFETY: as above; `exit_code` is writable.
    assert_ne!(
        unsafe {
            windows_sys::Win32::System::Threading::GetExitCodeProcess(handle, &raw mut exit_code)
        },
        0
    );
    assert_eq!(
        exit_code, 7,
        "the retained handle is the launched script's process"
    );

    let observed =
        std::fs::read_to_string(directory.join("observed.txt")).expect("the script's record");
    let lines: Vec<&str> = observed.lines().collect();
    assert_eq!(
        lines.first().copied(),
        Some(format!("ARG1={rendezvous}").as_str())
    );
    assert_eq!(
        lines.get(1).copied(),
        Some(format!("ALL={rendezvous}").as_str()),
        "the argument arrives as exactly one token"
    );
    let cwd = lines
        .get(2)
        .and_then(|line| line.strip_prefix("CWD="))
        .expect("the working-directory record");
    assert_eq!(
        std::fs::canonicalize(cwd).expect("canonical working directory"),
        std::fs::canonicalize(&directory).expect("canonical helper directory"),
        "lpDirectory is the launched process's working directory"
    );
}

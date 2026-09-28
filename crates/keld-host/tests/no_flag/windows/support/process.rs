//! Native process wait, signal, identity and test-owned termination observers.

use std::os::windows::io::{AsRawHandle as _, FromRawHandle as _, OwnedHandle};
use std::process::{Child, Command, ExitStatus};
use std::thread;
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::WAIT_OBJECT_0;
use windows_sys::Win32::System::Threading::{
    OpenProcess, PROCESS_DUP_HANDLE, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
    PROCESS_TERMINATE, TerminateProcess, WaitForSingleObject,
};

pub(crate) fn open_process_for_census(pid: u32) -> OwnedHandle {
    // SAFETY: numeric PID belongs to a live test-owned process; the returned
    // handle requests query/duplicate rights only and is converted once.
    let raw = unsafe {
        OpenProcess(
            PROCESS_DUP_HANDLE | PROCESS_QUERY_LIMITED_INFORMATION,
            0,
            pid,
        )
    };
    assert!(
        !raw.is_null(),
        "open PID {pid} for handle census: {}",
        std::io::Error::last_os_error()
    );
    // SAFETY: raw is the fresh non-null owning process handle returned above.
    unsafe { OwnedHandle::from_raw_handle(raw.cast()) }
}

pub(crate) fn open_process_for_wait(pid: u32, terminate: bool) -> OwnedHandle {
    let access = PROCESS_SYNCHRONIZE
        | PROCESS_QUERY_LIMITED_INFORMATION
        | if terminate { PROCESS_TERMINATE } else { 0 };
    // SAFETY: PID belongs to a live test-owned process. Access permits wait and
    // exit-status observation plus optional host-only termination. The returned
    // handle is converted once.
    let raw = unsafe { OpenProcess(access, 0, pid) };
    assert!(
        !raw.is_null(),
        "open PID {pid} for exact wait: {}",
        std::io::Error::last_os_error()
    );
    // SAFETY: raw is the fresh non-null owning process handle returned above.
    unsafe { OwnedHandle::from_raw_handle(raw.cast()) }
}

pub(crate) fn terminate_test_process(process: &OwnedHandle) {
    // SAFETY: this is the live test-owned host handle opened with terminate access.
    assert_ne!(
        unsafe { TerminateProcess(process.as_raw_handle().cast(), 1) },
        0,
        "terminate only the no-flag host: {}",
        std::io::Error::last_os_error()
    );
}

pub(crate) fn assert_process_signaled(process: &OwnedHandle, label: &str) {
    let result = wait_for_process_signal(process, 10_000);
    assert_eq!(result, WAIT_OBJECT_0, "{label} survived host death");
}

pub(crate) fn wait_for_process_signal(process: &OwnedHandle, timeout_ms: u32) -> u32 {
    // SAFETY: process is a live retained handle; signaling is the kernel's
    // exact process-termination oracle. Zero observes current state; a positive
    // timeout only bounds failure. This neither closes nor transfers the handle.
    unsafe { WaitForSingleObject(process.as_raw_handle().cast(), timeout_ms) }
}

pub(crate) fn wait_child(child: &mut Child, deadline: Instant) -> ExitStatus {
    loop {
        if let Some(status) = child.try_wait().expect("observe host exit") {
            return status;
        }
        assert!(Instant::now() < deadline, "host exit timed out");
        thread::park_timeout(Duration::from_millis(10));
    }
}

pub(crate) fn process_exists(pid: u32) -> bool {
    Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            &format!("if (Get-Process -Id {pid} -ErrorAction SilentlyContinue) {{ exit 0 }} else {{ exit 1 }}"),
        ])
        .status()
        .expect("query process")
        .success()
}

pub(crate) fn wait_process_gone(pid: u32, deadline: Instant) {
    while process_exists(pid) {
        assert!(Instant::now() < deadline, "process {pid} remained live");
        thread::park_timeout(Duration::from_millis(20));
    }
}

pub(crate) fn wait_for_child_process(parent_pid: u32, name: &str, deadline: Instant) -> u32 {
    loop {
        let script = format!(
            "$p=Get-CimInstance Win32_Process -Filter \"ParentProcessId={parent_pid}\" | Where-Object {{$_.Name -eq '{name}' -and $_.CommandLine -notmatch 'keld-windows-dev-stage-cleanup'}} | Select-Object -First 1 -ExpandProperty ProcessId; if ($p) {{$p}}"
        );
        let output = Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .output()
            .expect("query delegated child process");
        if output.status.success()
            && let Ok(pid) = String::from_utf8_lossy(&output.stdout)
                .trim()
                .parse::<u32>()
        {
            return pid;
        }
        if Instant::now() >= deadline {
            let dump = Command::new("powershell.exe")
                .args([
                    "-NoProfile",
                    "-NonInteractive",
                    "-Command",
                    &format!(
                        "Get-CimInstance Win32_Process -Filter \"ParentProcessId={parent_pid}\" | ForEach-Object {{ $_.Name + ' pid=' + $_.ProcessId + ' ' + $_.CommandLine }}"
                    ),
                ])
                .output()
                .expect("dump delegated children");
            panic!(
                "delegated child `{name}` did not appear under {parent_pid}; parent_alive={}; children:\n{}",
                process_exists(parent_pid),
                String::from_utf8_lossy(&dump.stdout)
            );
        }
        thread::park_timeout(Duration::from_millis(20));
    }
}

pub(crate) fn wait_for_cleanup_sentinel(parent_pid: u32, deadline: Instant) -> u32 {
    loop {
        let script = format!(
            "$p=Get-CimInstance Win32_Process -Filter \"ParentProcessId={parent_pid}\" | Where-Object {{$_.CommandLine -match 'keld-windows-dev-stage-cleanup-v1'}} | Select-Object -First 1 -ExpandProperty ProcessId; if ($p) {{$p}}"
        );
        let output = Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .output()
            .expect("query cleanup sentinel process");
        if output.status.success()
            && let Ok(pid) = String::from_utf8_lossy(&output.stdout)
                .trim()
                .parse::<u32>()
        {
            return pid;
        }
        assert!(
            Instant::now() < deadline,
            "cleanup sentinel did not appear as a CLI-owned host sibling"
        );
        thread::park_timeout(Duration::from_millis(20));
    }
}

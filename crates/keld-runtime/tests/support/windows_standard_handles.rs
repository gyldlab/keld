//! Observer and launcher-side waits for the Windows standard-handle regressions
//! (KEL-270 F51): a child of a launcher whose standard handles are pipes must hold
//! none of them. Shared by the same-token candidate launch and the LPAC launch
//! tests; it holds no test function.
//!
//! The observer reports what the child's own `GetStdHandle` returns and whether a
//! `WriteFile` to its standard output succeeds; it derives nothing from the launch
//! code. The waits are bounded OS waits, never sleeps.

use std::io::Read;
use std::os::windows::io::AsRawHandle as _;
use std::process::Child;
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use windows_sys::Win32::Foundation::{INVALID_HANDLE_VALUE, WAIT_OBJECT_0};
use windows_sys::Win32::Storage::FileSystem::{GetFileType, WriteFile};
use windows_sys::Win32::System::Console::{
    GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
};
use windows_sys::Win32::System::Threading::WaitForSingleObject;

/// The kill switch of every bounded wait here: a launcher's exit, a pipe's EOF, a
/// report's arrival. It is never a synchronization delay.
pub const KILL_SWITCH: Duration = Duration::from_secs(10);

/// The bytes the child writes to its standard output. They never reach the
/// launcher's pipe: with no standard handle the write fails.
pub const CANDIDATE_STDOUT_MARKER: &[u8] = b"CANDIDATE_WROTE_TO_STDOUT\n";

/// The report of a process whose three standard handles are null: `GetStdHandle`
/// returns NULL for each, and `WriteFile` to standard output fails with
/// `ERROR_INVALID_HANDLE` (6).
pub const NO_STANDARD_HANDLE_REPORT: &str = "in=NULL out=NULL err=NULL stdout_write=error:6";

/// Reports this process's three standard handles as `GetStdHandle` returns them,
/// with `GetFileType` of each live one (3 is a pipe), and the result of a
/// `WriteFile` of [`CANDIDATE_STDOUT_MARKER`] to standard output.
pub fn standard_handle_report() -> String {
    let describe = |id: u32| -> String {
        // SAFETY: GetStdHandle retains no pointer; it reads this process's own
        // standard-handle table.
        let handle = unsafe { GetStdHandle(id) };
        if handle.is_null() {
            return "NULL".to_owned();
        }
        if handle == INVALID_HANDLE_VALUE {
            return "INVALID_HANDLE_VALUE".to_owned();
        }
        // SAFETY: a non-null, non-invalid value of this process's standard-handle
        // table; GetFileType only classifies it and fails on a bad value.
        let file_type = unsafe { GetFileType(handle) };
        format!("0x{:x}:type={file_type}", handle.addr())
    };
    let input = describe(STD_INPUT_HANDLE);
    let output = describe(STD_OUTPUT_HANDLE);
    let error = describe(STD_ERROR_HANDLE);

    // SAFETY: as above.
    let stdout = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) };
    let length = u32::try_from(CANDIDATE_STDOUT_MARKER.len()).expect("marker length fits a DWORD");
    let mut written = 0_u32;
    // SAFETY: the marker is live for this synchronous call, `written` is writable
    // and no overlapped structure is passed. A null or closed handle fails the
    // call instead of writing anywhere.
    let succeeded = unsafe {
        WriteFile(
            stdout,
            CANDIDATE_STDOUT_MARKER.as_ptr(),
            length,
            &raw mut written,
            std::ptr::null_mut(),
        )
    };
    let write = if succeeded == 0 {
        format!(
            "error:{}",
            std::io::Error::last_os_error()
                .raw_os_error()
                .unwrap_or_default()
        )
    } else {
        format!("ok:{written}")
    };
    format!("in={input} out={output} err={error} stdout_write={write}")
}

/// Reads `source` to EOF on its own thread. The receiver yields the bytes once EOF
/// arrives; a bounded `recv_timeout` on it observes whether EOF arrives at all.
pub fn drain<R: Read + Send + 'static>(mut source: R) -> Receiver<Vec<u8>> {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = source.read_to_end(&mut bytes);
        let _ = sender.send(bytes);
    });
    receiver
}

/// Whether `child` exits within [`KILL_SWITCH`]: an OS wait on its process handle
/// that consumes nothing, so the caller still reaps it.
pub fn exits_within_kill_switch(child: &Child) -> bool {
    let millis = u32::try_from(KILL_SWITCH.as_millis()).expect("the kill switch fits a DWORD");
    // SAFETY: the Child owns a live process handle for this synchronous wait.
    unsafe { WaitForSingleObject(child.as_raw_handle().cast(), millis) == WAIT_OBJECT_0 }
}

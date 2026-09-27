//! One Windows test oracle for the current process's live executive handles.

#![allow(unsafe_code)] // Test-only GetCurrentProcess/GetProcessHandleCount with live local output.
#![deny(unsafe_op_in_unsafe_fn)]

use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessHandleCount};

pub(crate) fn owner_handle_count() -> usize {
    let mut count = 0_u32;
    // SAFETY: GetCurrentProcess returns the current live process's pseudo handle;
    // it is borrowed, requires no closure, and has query access. `count` remains
    // initialized writable storage through this synchronous call. No pointer escapes.
    let success = unsafe { GetProcessHandleCount(GetCurrentProcess(), &raw mut count) };
    assert_ne!(
        success,
        0,
        "current-process handle census failed: {}",
        std::io::Error::last_os_error()
    );
    count as usize // Windows pointer widths are at least 32 bits.
}

//! Native observation only, never authority to act on a recycled HWND.

use std::io;

use windows_sys::Win32::Foundation::{HWND, LPARAM, RECT, TRUE};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumChildWindows, GetClassNameW, GetParent, GetWindowRect, GetWindowThreadProcessId, IsWindow,
};

const MAX_OBSERVED_CHILDREN: usize = 4096;

/// A live fixture's native window tree. These numeric handles do not pin identity.
/// A later live/recycled handle makes a teardown check fail, never authorizes a kill.
#[derive(Debug)]
pub(crate) struct NativeWindowSnapshot {
    pub(crate) hwnd: usize,
    /// Raw observer-context screen coordinates; not physical-pixel or client bounds.
    pub(crate) outer_rect: [i32; 4],
    pub(crate) children: Vec<usize>,
    /// Read-only metadata from the same enumeration; never window authority.
    #[cfg(feature = "profile-test-hooks")]
    pub(crate) child_details: Vec<String>,
}

impl NativeWindowSnapshot {
    /// Caller must keep its original Host alive and unclosed during this observation.
    /// Reject null/foreign parents before enumeration: a null parent enumerates desktop windows.
    pub(crate) fn capture(hwnd: u64, expected_pid: u32) -> io::Result<Self> {
        let hwnd = usize::try_from(hwnd)
            .map_err(|_| invalid_input("HWND does not fit the native pointer width"))?;
        if hwnd == 0 || expected_pid == 0 {
            return Err(invalid_input(
                "a nonzero fixture HWND and process are required",
            ));
        }
        check_owner(hwnd, expected_pid)?;
        let mut rect = RECT::default();
        // SAFETY: HWND is a numeric query target, never dereferenced by Rust; the
        // initialized RECT outlives this synchronous call. No window is modified.
        // https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-getwindowrect
        if unsafe { GetWindowRect(hwnd as HWND, &raw mut rect) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let mut census = ChildCensus::default();
        // SAFETY: the callback borrows only this stack-owned census for this call.
        // The parent is non-null. The documented return value is unused, not a BOOL
        // success oracle. Overflow is recorded by the callback and refused below.
        // https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-enumchildwindows
        unsafe { EnumChildWindows(hwnd as HWND, Some(visit_child), &raw mut census as LPARAM) };
        if census.overflow {
            return Err(io::Error::other(
                "native child-window census exceeded 4096 entries",
            ));
        }
        // Detect loss/replacement across the read. This does not claim an atomic
        // snapshot or prevent same-process HWND reuse; the fixture controls closure.
        check_owner(hwnd, expected_pid)?;
        Ok(Self {
            hwnd,
            outer_rect: [rect.left, rect.top, rect.right, rect.bottom],
            children: census.handles,
            #[cfg(feature = "profile-test-hooks")]
            child_details: census.details,
        })
    }

    /// Current liveness of the parent and exactly the previously observed descendants.
    /// This read is conservative on recycling: any reused/live number remains a failure.
    pub(crate) fn live_handles(&self) -> Vec<usize> {
        std::iter::once(self.hwnd)
            .chain(self.children.iter().copied())
            .filter(|hwnd| {
                // SAFETY: this is an observation, not validation before a window action.
                // HWNDs can be recycled; no authority or retained identity is inferred.
                // https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-iswindow
                unsafe { IsWindow(*hwnd as HWND) != 0 }
            })
            .collect()
    }
}

#[derive(Default)]
struct ChildCensus {
    handles: Vec<usize>,
    details: Vec<String>,
    overflow: bool,
}

unsafe extern "system" fn visit_child(hwnd: HWND, state: LPARAM) -> i32 {
    // SAFETY: capture passes one exclusive ChildCensus pointer, valid through this
    // synchronous enumeration. The callback is not retained by EnumChildWindows.
    let census = unsafe { &mut *(state as *mut ChildCensus) };
    if census.handles.len() == MAX_OBSERVED_CHILDREN {
        census.overflow = true;
        return 0;
    }
    census.handles.push(hwnd as usize);
    let mut pid = 0;
    let mut class = [0_u16; 256];
    // SAFETY: read-only queries of the numeric child passed by EnumChildWindows.
    // Each local output buffer outlives its call. A concurrent disappearance is
    // recorded as zero/empty metadata; these reads grant no authority to act.
    // https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-getclassnamew
    // https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-getparent
    let (thread, copied, parent, live) = unsafe {
        (
            GetWindowThreadProcessId(hwnd, &raw mut pid),
            GetClassNameW(hwnd, class.as_mut_ptr(), 256),
            GetParent(hwnd) as usize,
            IsWindow(hwnd) != 0,
        )
    };
    let copied = usize::try_from(copied).unwrap_or(0);
    census.details.push(format!(
        "hwnd={} class={:?} pid={pid} thread={thread} parent={parent} live={live}",
        hwnd as usize,
        String::from_utf16_lossy(&class[..copied])
    ));
    TRUE
}

fn check_owner(hwnd: usize, expected_pid: u32) -> io::Result<()> {
    let mut actual_pid = 0;
    // SAFETY: a query of the numeric HWND with an initialized local output slot.
    // The retained fixture process, not this PID read, owns the process lifetime.
    // https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-getwindowthreadprocessid
    let thread = unsafe { GetWindowThreadProcessId(hwnd as HWND, &raw mut actual_pid) };
    if thread == 0 {
        return Err(io::Error::last_os_error());
    }
    if actual_pid != expected_pid {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "observed HWND is not owned by the expected fixture process",
        ));
    }
    Ok(())
}

fn invalid_input(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

#[cfg(test)]
mod tests;

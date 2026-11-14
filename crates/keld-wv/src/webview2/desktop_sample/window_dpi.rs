//! Read DPI from existing per-monitor-V2 windows on the Host UI thread. No
//! window is created, retained, transferred, or selected by a private class name.

use super::unavailable;
use crate::WvError;
use windows_sys::Win32::Foundation::{HWND, LPARAM, POINT, RECT};
use windows_sys::Win32::Graphics::Gdi::{
    HMONITOR, MONITOR_DEFAULTTONEAREST, MONITOR_DEFAULTTONULL, MonitorFromPoint, MonitorFromWindow,
};
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::HiDpi::{
    AreDpiAwarenessContextsEqual, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForWindow,
    GetWindowDpiAwarenessContext,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumThreadWindows, GetWindowRect, GetWindowThreadProcessId,
};

struct Reading {
    thread: u32,
    primary: HMONITOR,
    dpi: Option<u32>,
}

unsafe extern "system" fn observe(hwnd: HWND, data: LPARAM) -> i32 {
    // SAFETY: EnumThreadWindows on our UI thread supplies this current HWND
    // synchronously and passes our exclusive stack borrow for this call only.
    let reading = unsafe { &mut *(data as *mut Reading) };
    let mut pid = 0;
    // SAFETY: read owner and window awareness from the enumerated HWND; pid is writable.
    let (tid, pmv2) = unsafe {
        (
            GetWindowThreadProcessId(hwnd, &raw mut pid),
            AreDpiAwarenessContextsEqual(
                GetWindowDpiAwarenessContext(hwnd),
                DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
            ),
        )
    };
    if tid != reading.thread || pid != std::process::id() || pmv2 == 0 {
        return 1;
    }
    let mut rect = RECT::default();
    // SAFETY: only the verified UI-thread window; Win32 writes the complete live RECT.
    if unsafe { GetWindowRect(hwnd, &raw mut rect) } == 0 {
        return 0;
    }
    let origin = POINT {
        x: rect.left,
        y: rect.top,
    };
    // SAFETY: read location only. A zero-sized event target must have its
    // actual origin on the primary; "nearest" alone cannot qualify it.
    let (at_origin, attached) = unsafe {
        (
            MonitorFromPoint(origin, MONITOR_DEFAULTTONULL),
            MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST),
        )
    };
    if at_origin != reading.primary || attached != reading.primary {
        return 1;
    }
    // SAFETY: GetDpiForWindow on a live per-monitor-aware window returns the
    // DPI of its monitor. Zero or disagreement invalidates this whole capture.
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    if dpi == 0 || reading.dpi.is_some_and(|prior| prior != dpi) {
        return 0;
    }
    reading.dpi = Some(dpi);
    1
}

pub(super) fn read(primary: HMONITOR) -> Result<u32, WvError> {
    // SAFETY: synchronous read of the UI thread executing this capture.
    let thread = unsafe { GetCurrentThreadId() };
    let mut reading = Reading {
        thread,
        primary,
        dpi: None,
    };
    // SAFETY: callback runs synchronously for nonchild windows on this exact
    // thread; no HWND or pointer to reading escapes this call.
    if unsafe { EnumThreadWindows(thread, Some(observe), (&raw mut reading) as LPARAM) } == 0 {
        return Err(unavailable(
            "could not verify a consistent Host UI-thread monitor DPI",
        ));
    }
    reading
        .dpi
        .ok_or_else(|| unavailable("no owned primary-monitor PMv2 window for DPI capture"))
}

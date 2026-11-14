use crate::args::Mode;
use crate::error::{ProbeError, ProbeResult};
use serde_json::{Value, json};
use windows_sys::Win32::Foundation::{ERROR_ACCESS_DENIED, GetLastError};
use windows_sys::Win32::UI::HiDpi::{
    AreDpiAwarenessContextsEqual, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
    DPI_AWARENESS_CONTEXT_UNAWARE, GetAwarenessFromDpiAwarenessContext,
    GetThreadDpiAwarenessContext, SetProcessDpiAwarenessContext,
};

pub(crate) fn configure(mode: Mode) -> ProbeResult<Value> {
    let requested = match mode {
        Mode::Pmv2 => DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        Mode::Unaware => DPI_AWARENESS_CONTEXT_UNAWARE,
    };
    // SAFETY: changes only this new probe process; no HWND is created anywhere
    // in this fixture. The input is a documented DPI context pseudo-handle.
    let set_ok = unsafe { SetProcessDpiAwarenessContext(requested) } != 0;
    // SAFETY: GetLastError takes no pointers and immediately follows the call.
    let set_error = if set_ok {
        None
    } else {
        Some(unsafe { GetLastError() })
    };
    // SAFETY: the returned context is borrowed and is not closed or dereferenced.
    let observed = unsafe { GetThreadDpiAwarenessContext() };
    // SAFETY: both parameters are documented DPI context pseudo-handles.
    let exact = unsafe { AreDpiAwarenessContextsEqual(observed, requested) } != 0;
    if !set_ok && !(set_error == Some(ERROR_ACCESS_DENIED) && exact) {
        return Err(ProbeError::native(
            "SetProcessDpiAwarenessContext",
            i64::from(set_error.unwrap_or(0)),
        ));
    }
    if !exact {
        return Err(ProbeError::invalid(
            "AreDpiAwarenessContextsEqual",
            "Requested exact process context was not established; PMv1 is not an accepted substitute",
        ));
    }
    // SAFETY: observed was returned by GetThreadDpiAwarenessContext.
    let awareness = unsafe { GetAwarenessFromDpiAwarenessContext(observed) };
    Ok(json!({
        "requested": mode.name(), "exactRequestedContext": exact,
        "awareness": awareness, "setterSucceeded": set_ok, "setterLastError": set_error,
        "coordinates": if mode == Mode::Pmv2 { "physical-pixels" } else { "dpi-virtualized-caller-units" },
    }))
}

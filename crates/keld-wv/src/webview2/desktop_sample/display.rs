use super::unavailable;
use crate::WvError;
use crate::engine::AppDesktopRect;
use windows_sys::Win32::Foundation::{POINT, RECT};
use windows_sys::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MONITOR_DEFAULTTONULL, MONITORINFO, MONITORINFOEXW, MonitorFromPoint,
};
use windows_sys::Win32::UI::HiDpi::{
    AreDpiAwarenessContextsEqual, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
    GetThreadDpiAwarenessContext,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, MONITORINFOF_PRIMARY, SM_CMONITORS,
};

/// Native physical edges; right and bottom are exclusive.
#[derive(Eq, PartialEq)]
pub(super) struct Geometry {
    pub(super) source_name: Vec<u16>,
    pub(super) bounds: [i32; 4],
    pub(super) work_area: [i32; 4],
    pub(super) dpi: u32,
}

fn edges(r: RECT) -> [i32; 4] {
    [r.left, r.top, r.right, r.bottom]
}

pub(super) fn read() -> Result<Geometry, WvError> {
    // SAFETY: read the calling UI thread's borrowed context; no DPI setter or HWND is used.
    if unsafe {
        AreDpiAwarenessContextsEqual(
            GetThreadDpiAwarenessContext(),
            DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        )
    } == 0
    {
        return Err(unavailable(
            "UI thread must already be per-monitor V2 DPI aware",
        ));
    }
    // SAFETY: this index has no pointer arguments and returns a count.
    if unsafe { GetSystemMetrics(SM_CMONITORS) } != 1 {
        return Err(unavailable("requires exactly one desktop monitor"));
    }
    // SAFETY: the physical primary origin identifies its monitor; no closest fallback.
    let monitor = unsafe { MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTONULL) };
    if monitor.is_null() {
        return Err(unavailable("no monitor at the primary origin"));
    }
    let mut info = MONITORINFOEXW {
        monitorInfo: MONITORINFO {
            cbSize: u32::try_from(size_of::<MONITORINFOEXW>())
                .map_err(|_| unavailable("MONITORINFOEXW size"))?,
            ..Default::default()
        },
        ..Default::default()
    };
    // SAFETY: borrowed monitor, fully initialized EX structure, pointer derived
    // from the complete allocation (including Windows' output device-name buffer).
    if unsafe { GetMonitorInfoW(monitor, (&raw mut info).cast()) } == 0 {
        return Err(unavailable("GetMonitorInfoW failed"));
    }
    let bounds = edges(info.monitorInfo.rcMonitor);
    let work_area = edges(info.monitorInfo.rcWork);
    if info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY == 0
        || bounds[0] != 0
        || bounds[1] != 0
        || bounds[2] <= 0
        || bounds[3] <= 0
        || work_area[0] < 0
        || work_area[1] < 0
        || work_area[0] >= work_area[2]
        || work_area[1] >= work_area[3]
        || work_area[2] > bounds[2]
        || work_area[3] > bounds[3]
    {
        return Err(unavailable("invalid primary physical bounds or work area"));
    }
    let end = info
        .szDevice
        .iter()
        .position(|&v| v == 0)
        .filter(|&v| v != 0)
        .ok_or_else(|| unavailable("invalid monitor name"))?;
    let dpi = super::window_dpi::read(monitor)?;
    Ok(Geometry {
        source_name: info.szDevice[..end].to_vec(),
        bounds,
        work_area,
        dpi,
    })
}

pub(super) fn logical(rect: [i32; 4], scale: f64) -> Result<AppDesktopRect, WvError> {
    if !scale.is_finite() || scale <= 0.0 {
        return Err(unavailable("invalid effective scale"));
    }
    // The enclosing rectangle uses floors for the left/top, ceilings for right/bottom.
    let edge = |i: i32, upper: bool| -> Result<u32, WvError> {
        let q = f64::from(i) / scale;
        let n = if upper { q.ceil() } else { q.floor() };
        if !(0.0..=f64::from(u32::MAX)).contains(&n) {
            return Err(unavailable("logical edge out of range"));
        }
        // Range and integrality are established above; the first-primary subset has no negative edges.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let integral = n as u32;
        Ok(integral)
    };
    let x = edge(rect[0], false)?;
    let y = edge(rect[1], false)?;
    let width = edge(rect[2], true)?
        .checked_sub(x)
        .filter(|w| *w > 0)
        .ok_or_else(|| unavailable("invalid logical width"))?;
    let height = edge(rect[3], true)?
        .checked_sub(y)
        .filter(|h| *h > 0)
        .ok_or_else(|| unavailable("invalid logical height"))?;
    Ok(AppDesktopRect {
        x: i32::try_from(x).map_err(|_| unavailable("logical x out of range"))?,
        y: i32::try_from(y).map_err(|_| unavailable("logical y out of range"))?,
        width,
        height,
    })
}

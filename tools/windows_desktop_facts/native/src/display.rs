use crate::args::Mode;
use crate::error::{ProbeError, ProbeResult, utf16, wide};
use crate::identity;
use serde_json::{Value, json};
use windows_sys::Win32::Foundation::{GetLastError, LPARAM, RECT, SetLastError};
use windows_sys::Win32::Graphics::Gdi::{
    DEVMODEW, DM_PELSHEIGHT, DM_PELSWIDTH, ENUM_CURRENT_SETTINGS, EnumDisplayMonitors,
    EnumDisplaySettingsW, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO, MONITORINFOEXW,
};
use windows_sys::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    MONITORINFOF_PRIMARY, SPI_GETWORKAREA, SystemParametersInfoW,
};

struct Enumeration {
    handles: Vec<HMONITOR>,
    allocation_failed: bool,
}

unsafe extern "system" fn collect_monitor(
    monitor: HMONITOR,
    _dc: HDC,
    _rect: *mut RECT,
    data: LPARAM,
) -> i32 {
    // SAFETY: enumerate() passes its unique live Enumeration pointer; Windows
    // invokes this callback synchronously and the pointer does not escape.
    let state = unsafe { &mut *(data as *mut Enumeration) };
    if state.handles.try_reserve(1).is_err() {
        state.allocation_failed = true;
        return 0;
    }
    state.handles.push(monitor);
    1
}

fn enumerate() -> ProbeResult<Vec<HMONITOR>> {
    let mut state = Enumeration {
        handles: Vec::new(),
        allocation_failed: false,
    };
    // SAFETY: null DC/clip enumerate the desktop; callback state remains alive.
    let ok = unsafe {
        EnumDisplayMonitors(
            std::ptr::null_mut(),
            std::ptr::null(),
            Some(collect_monitor),
            (&mut state as *mut Enumeration) as LPARAM,
        )
    };
    if state.allocation_failed {
        return Err(ProbeError::invalid(
            "EnumDisplayMonitors",
            "Unable to allocate monitor list",
        ));
    }
    if ok == 0 {
        // SAFETY: no arguments, directly after the failed native enumeration.
        return Err(ProbeError::native(
            "EnumDisplayMonitors",
            i64::from(unsafe { GetLastError() }),
        ));
    }
    if state.handles.is_empty() {
        return Err(ProbeError::invalid(
            "EnumDisplayMonitors",
            "No monitors returned",
        ));
    }
    Ok(state.handles)
}

fn monitor_info(handle: HMONITOR) -> ProbeResult<MONITORINFOEXW> {
    let mut info = MONITORINFOEXW {
        monitorInfo: MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFOEXW>() as u32,
            ..Default::default()
        },
        ..Default::default()
    };
    // SAFETY: the only supplied handle is borrowed from enumeration, or null
    // for the explicit invalid-handle control. info has the full cbSize.
    // Derive the pointer from the whole packet so the API may write its full size.
    let ok = unsafe {
        SetLastError(0);
        GetMonitorInfoW(handle, (&raw mut info).cast())
    };
    if ok == 0 {
        // SAFETY: no arguments, immediately follows the failing API.
        return Err(ProbeError::native(
            "GetMonitorInfoW",
            i64::from(unsafe { GetLastError() }),
        ));
    }
    Ok(info)
}

pub(crate) fn rectangle(rect: RECT) -> ProbeResult<Value> {
    let width = i64::from(rect.right) - i64::from(rect.left);
    let height = i64::from(rect.bottom) - i64::from(rect.top);
    if width <= 0 || height <= 0 {
        return Err(ProbeError::invalid(
            "native RECT",
            "Rectangle is empty or reversed",
        ));
    }
    Ok(json!({"x": rect.left, "y": rect.top, "width": width, "height": height}))
}

pub(crate) fn validate_dpi(status: i32, x: u32, y: u32) -> ProbeResult<f64> {
    if status != 0 {
        return Err(ProbeError::native("GetDpiForMonitor", i64::from(status)));
    }
    if x == 0 || y == 0 || x != y {
        return Err(ProbeError::invalid(
            "GetDpiForMonitor",
            "Effective DPI must be positive and equal in both axes",
        ));
    }
    Ok(f64::from(x) / 96.0)
}

fn dpi(handle: HMONITOR, invalid_type: bool) -> ProbeResult<Value> {
    let (mut x, mut y) = (0, 0);
    let kind = if invalid_type {
        i32::MAX
    } else {
        MDT_EFFECTIVE_DPI
    };
    // SAFETY: handle is borrowed from enumeration; outputs are writable u32s.
    // The control changes only the enum argument and must receive API failure.
    let status = unsafe { GetDpiForMonitor(handle, kind, &mut x, &mut y) };
    let scale = validate_dpi(status, x, y)?;
    Ok(
        json!({"source": "GetDpiForMonitor(MDT_EFFECTIVE_DPI)", "x": x, "y": y, "deviceScaleFactor": scale}),
    )
}

fn physical_mode(name: &str) -> ProbeResult<Value> {
    let name = wide(name);
    let mut mode = DEVMODEW {
        dmSize: std::mem::size_of::<DEVMODEW>() as u16,
        ..Default::default()
    };
    // SAFETY: name is NUL terminated; mode is correctly sized and writable.
    let ok = unsafe {
        SetLastError(0);
        EnumDisplaySettingsW(name.as_ptr(), ENUM_CURRENT_SETTINGS, &mut mode)
    };
    if ok == 0 {
        // SAFETY: no arguments, immediately follows the failing API.
        return Err(ProbeError::native(
            "EnumDisplaySettingsW",
            i64::from(unsafe { GetLastError() }),
        ));
    }
    if mode.dmFields & (DM_PELSWIDTH | DM_PELSHEIGHT) != (DM_PELSWIDTH | DM_PELSHEIGHT)
        || mode.dmPelsWidth == 0
        || mode.dmPelsHeight == 0
    {
        return Err(ProbeError::invalid(
            "EnumDisplaySettingsW",
            "Physical mode dimensions are unavailable or zero",
        ));
    }
    Ok(
        json!({"source": "EnumDisplaySettingsW", "width": mode.dmPelsWidth,
        "height": mode.dmPelsHeight, "frequency": mode.dmDisplayFrequency}),
    )
}

fn describe(handle: HMONITOR, mode: Mode, paths: &[identity::NativePath]) -> ProbeResult<Value> {
    let info = monitor_info(handle)?;
    let name = utf16(&info.szDevice, "GetMonitorInfoW")?;
    let bounds = rectangle(info.monitorInfo.rcMonitor)?;
    let work = rectangle(info.monitorInfo.rcWork)?;
    let b = info.monitorInfo.rcMonitor;
    let w = info.monitorInfo.rcWork;
    if w.left < b.left || w.top < b.top || w.right > b.right || w.bottom > b.bottom {
        return Err(ProbeError::invalid(
            "GetMonitorInfoW",
            "Work area lies outside monitor bounds",
        ));
    }
    let physical = physical_mode(&name)?;
    if mode == Mode::Pmv2
        && (bounds["width"] != physical["width"] || bounds["height"] != physical["height"])
    {
        return Err(ProbeError::invalid(
            "physical bounds cross-check",
            "Exact PMv2 monitor bounds disagree with EnumDisplaySettingsW physical dimensions",
        ));
    }
    Ok(json!({
        "sourceName": name, "observedHandleHex": format!("0x{:x}", handle as usize),
        "primary": info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0,
        "bounds": bounds, "workArea": work, "dpi": dpi(handle, false)?,
        "physicalMode": physical, "nativePaths": identity::for_source(paths, &name)?,
    }))
}

fn primary_work_area() -> ProbeResult<Value> {
    let mut rect = RECT::default();
    // SAFETY: SPI_GETWORKAREA is read-only and rect is writable for a RECT.
    let ok = unsafe {
        SetLastError(0);
        SystemParametersInfoW(SPI_GETWORKAREA, 0, (&mut rect as *mut RECT).cast(), 0)
    };
    if ok == 0 {
        // SAFETY: no arguments, immediately follows the failing API.
        return Err(ProbeError::native(
            "SystemParametersInfoW(SPI_GETWORKAREA)",
            i64::from(unsafe { GetLastError() }),
        ));
    }
    rectangle(rect)
}

pub(crate) fn sample(mode: Mode) -> ProbeResult<Value> {
    let paths = identity::sample()?;
    let mut displays = enumerate()?
        .into_iter()
        .map(|handle| describe(handle, mode, &paths))
        .collect::<ProbeResult<Vec<_>>>()?;
    displays.sort_by(|a, b| a["sourceName"].as_str().cmp(&b["sourceName"].as_str()));
    let primary: Vec<_> = displays
        .iter()
        .filter(|value| value["primary"] == true)
        .collect();
    if primary.len() != 1 {
        return Err(ProbeError::invalid(
            "GetMonitorInfoW",
            "Expected exactly one primary monitor",
        ));
    }
    let primary_work = primary_work_area()?;
    if mode == Mode::Pmv2 && primary[0]["workArea"] != primary_work {
        return Err(ProbeError::invalid(
            "primary work area cross-check",
            "Monitor info disagrees with SPI_GETWORKAREA under exact PMv2",
        ));
    }
    Ok(json!({
        "coordinateSpace": if mode == Mode::Pmv2 { "physical-pixels" } else { "dpi-virtualized-caller-units" },
        "primarySourceName": primary[0]["sourceName"],
        "primaryWorkAreaViaSpi": primary_work, "displays": displays,
    }))
}

pub(crate) fn invalid_monitor_control() -> ProbeResult<()> {
    monitor_info(std::ptr::null_mut()).map(|_| ())
}

pub(crate) fn failed_dpi_control() -> ProbeResult<()> {
    let handles = enumerate()?;
    dpi(handles[0], true).map(|_| ())
}

//! Read-only desktop capture on the existing Windows UI thread, before the first window.
//! Only one primary monitor with one active physical target is admitted. A failed
//! native query or unsupported display configuration never produces public facts.
//!
//! Native call inventory (UI thread; read-only):
//! - `display`: `GetThreadDpiAwarenessContext` / `AreDpiAwarenessContextsEqual`,
//!   `GetSystemMetrics(SM_CMONITORS)`, `MonitorFromPoint`, [GetMonitorInfoW],
//!   read-only monitor geometry.
//! - `window_dpi`: [EnumThreadWindows] on `GetCurrentThreadId`; `GetWindowThreadProcessId`,
//!   `GetWindowDpiAwarenessContext` / `AreDpiAwarenessContextsEqual`, `GetWindowRect`,
//!   `MonitorFromPoint` / `MonitorFromWindow`, [GetDpiForWindow] on own `PMv2` primary-monitor windows.
//! - `target`: `GetDisplayConfigBufferSizes`, [QueryDisplayConfig] (active paths),
//!   [DisplayConfigGetDeviceInfo] (source/target names only).
//! - `appearance`: [TextScaleFactor], [HighContrast] and [RegGetValueW] for the
//!   user's `AppsUseLightTheme` DWORD only; balanced RoInitialize/RoUninitialize on
//!   the same UI thread. No setting is changed or handle exposed.
//!
//! [EnumThreadWindows]: https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-enumthreadwindows
//! [GetDpiForWindow]: https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-getdpiforwindow
//! [GetMonitorInfoW]: https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-getmonitorinfow
//! [QueryDisplayConfig]: https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-querydisplayconfig
//! [DisplayConfigGetDeviceInfo]: https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-displayconfiggetdeviceinfo
//! [TextScaleFactor]: https://learn.microsoft.com/uwp/api/windows.ui.viewmanagement.uisettings.textscalefactor
//! [HighContrast]: https://learn.microsoft.com/uwp/api/windows.ui.viewmanagement.accessibilitysettings.highcontrast
//! [RegGetValueW]: https://learn.microsoft.com/windows/win32/api/winreg/nf-winreg-reggetvaluew

#![deny(unsafe_op_in_unsafe_fn)]

mod appearance;
mod display;
mod target;
mod window_dpi;

use crate::WvError;
use crate::engine::{AppDesktopDisplay, AppDesktopFacts};
use target::Target;

fn unavailable(reason: &str) -> WvError {
    WvError::Window(format!("Windows desktop unavailable: {reason}"))
}

fn os_error(operation: &str, status: impl std::fmt::Display) -> WvError {
    unavailable(&format!("{operation}: {status}"))
}

/// Keep the display identity association for the Host session, across Bun generations.
/// This owns no HWND, monitor handle, COM object or copy of the desktop mirror.
#[derive(Default)]
pub(super) struct DesktopSampler {
    known: Vec<Target>,
}

impl DesktopSampler {
    pub(super) fn capture(&mut self) -> Result<AppDesktopFacts, WvError> {
        self.capture_with(display::read, target::read, appearance::read)
    }

    /// The production capture and source-bound controls share this exact order.
    /// Only acquisition is supplied; qualification and ID/history stay here.
    fn capture_with(
        &mut self,
        mut read_geometry: impl FnMut() -> Result<display::Geometry, WvError>,
        mut read_target: impl FnMut(&[u16]) -> Result<Target, WvError>,
        read_appearance: impl FnOnce() -> Result<(f64, bool), WvError>,
    ) -> Result<AppDesktopFacts, WvError> {
        let geometry = read_geometry()?;
        let target = read_target(&geometry.source_name)?;
        let (text_scale, dark) = read_appearance()?;
        let scale_factor = f64::from(geometry.dpi) / 96.0 * text_scale;
        let bounds = display::logical(geometry.bounds, scale_factor)?;
        let work_area = display::logical(geometry.work_area, scale_factor)?;
        // A changed output (even behind the same GDI name) is not the target we sampled.
        if read_geometry()? != geometry || read_target(&geometry.source_name)? != target {
            return Err(unavailable("display changed during capture"));
        }
        let id = self.display_id(target)?;
        Ok(AppDesktopFacts {
            primary_id: id,
            displays: vec![AppDesktopDisplay {
                id,
                bounds,
                work_area,
                scale_factor,
            }],
            should_use_dark_colors: dark,
        })
    }

    fn display_id(&mut self, target: Target) -> Result<u32, WvError> {
        let index = if let Some(i) = self.known.iter().position(|old| *old == target) {
            i
        } else {
            let i = self.known.len();
            if i >= 32 {
                return Err(unavailable("Host-session display identity limit reached"));
            }
            self.known.push(target);
            i
        };
        u32::try_from(index + 1).map_err(|_| unavailable("display ID overflow"))
    }
}

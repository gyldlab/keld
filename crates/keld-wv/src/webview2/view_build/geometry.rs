//! Checked geometry at the same native window custodian.
#![forbid(unsafe_code)]

use super::super::{View, managed_windows::DeferredCreate};
use crate::WvError;
use crate::engine::WindowSnapshot;
use crate::native_window::checked_logical_bounds;
use tao::dpi::{LogicalPosition, PhysicalSize};
use tao::window::Window;

/// Preserve the requested outer size by measuring Tao's actual frame delta on
/// the hidden HWND. No copied caption/border metrics or second DPI policy.
pub(super) fn correct_outer_frame(window: &Window, create: &DeferredCreate) -> Result<(), WvError> {
    let scale = window.scale_factor();
    let inner = window.inner_size();
    let outer = window.outer_size();
    let desired = outer_client_size(create.width, create.height, scale, outer, inner)?;
    window.set_inner_size(desired);
    if create.x.is_some() != create.y.is_some() {
        let physical = window
            .outer_position()
            .map_err(|e| WvError::Window(e.to_string()))?;
        let (current, _) = checked_logical_bounds(physical, window.outer_size(), scale)?;
        window.set_outer_position(LogicalPosition::new(
            create.x.unwrap_or(current.x),
            create.y.unwrap_or(current.y),
        ));
    }
    Ok(())
}

fn outer_client_size(
    width: u32,
    height: u32,
    scale: f64,
    outer: PhysicalSize<u32>,
    inner: PhysicalSize<u32>,
) -> Result<PhysicalSize<u32>, WvError> {
    let invalid = || {
        WvError::Window(String::from(
            "requested outer frame cannot be represented by the native window",
        ))
    };
    if !scale.is_normal() || scale <= 0.0 {
        return Err(invalid());
    }
    let desired = [f64::from(width) * scale, f64::from(height) * scale];
    if desired
        .iter()
        .any(|n| !n.is_finite() || n.round() <= 0.0 || n.round() > f64::from(i32::MAX))
    {
        return Err(invalid());
    }
    let desired: PhysicalSize<u32> = PhysicalSize::new(desired[0], desired[1]).cast();
    let frame_width = outer.width.checked_sub(inner.width).ok_or_else(invalid)?;
    let frame_height = outer.height.checked_sub(inner.height).ok_or_else(invalid)?;
    let width = desired
        .width
        .checked_sub(frame_width)
        .filter(|n| *n > 0)
        .ok_or_else(invalid)?;
    let height = desired
        .height
        .checked_sub(frame_height)
        .filter(|n| *n > 0)
        .ok_or_else(invalid)?;
    Ok(PhysicalSize::new(width, height))
}

pub(in crate::webview2) fn snapshot(view: &View) -> Result<WindowSnapshot, WvError> {
    let position = view
        .window
        .outer_position()
        .map_err(|e| WvError::Window(e.to_string()))?;
    let (position, size) = checked_logical_bounds(
        position,
        view.window.outer_size(),
        view.window.scale_factor(),
    )?;
    Ok(WindowSnapshot {
        x: position.x,
        y: position.y,
        width: size.width,
        height: size.height,
        focused: view.is_foreground(),
        maximized: view.window.is_maximized(),
        full_screen: view.window.fullscreen().is_some(),
    })
}

#[cfg(test)]
mod tests {
    use super::{PhysicalSize, outer_client_size};

    #[test]
    fn outer_frame_conversion_preserves_scaled_frame_delta() {
        assert_eq!(
            outer_client_size(
                800,
                600,
                1.5,
                PhysicalSize::new(1248, 966),
                PhysicalSize::new(1200, 900)
            )
            .expect("representable frame"),
            PhysicalSize::new(1152, 834),
        );
        assert_eq!(
            outer_client_size(
                800,
                600,
                1.0,
                PhysicalSize::new(816, 639),
                PhysicalSize::new(800, 600)
            )
            .expect("representable frame"),
            PhysicalSize::new(784, 561),
        );
    }

    #[test]
    fn outer_frame_conversion_refuses_invalid_native_and_requested_geometry() {
        let native = (PhysicalSize::new(816, 639), PhysicalSize::new(800, 600));
        for scale in [0.0, -1.0, f64::INFINITY, f64::NAN, f64::MIN_POSITIVE / 2.0] {
            assert!(outer_client_size(800, 600, scale, native.0, native.1).is_err());
        }
        for size in [(0, 600), (800, 0), (1, 1), (u32::MAX, 600), (800, u32::MAX)] {
            assert!(outer_client_size(size.0, size.1, 1.0, native.0, native.1).is_err());
        }
        assert!(outer_client_size(800, 600, 1.0, native.1, native.0).is_err());
    }
}

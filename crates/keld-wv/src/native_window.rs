//! Shared checked native geometry and backend view-id arithmetic.
#![forbid(unsafe_code)]

use tao::dpi::{LogicalPosition, PhysicalPosition, PhysicalSize};

use crate::WebviewId;
use crate::error::WvError;

pub(crate) fn next_view_id(next: &mut u32) -> Result<WebviewId, WvError> {
    let id = *next;
    let successor = id.checked_add(1).filter(|_| id != 0).ok_or_else(|| {
        WvError::EventLoop(String::from(
            "native webview id counter is exhausted; end this host session",
        ))
    })?;
    *next = successor;
    Ok(WebviewId(id))
}

pub(crate) fn checked_logical_bounds(
    position: PhysicalPosition<i32>,
    size: PhysicalSize<u32>,
    scale: f64,
) -> Result<(LogicalPosition<i32>, tao::dpi::LogicalSize<u32>), WvError> {
    // dpi 0.1.2 to_logical asserts a valid scale, and integer Pixel casts round
    // then saturate. Reject invalid/unrepresentable data BEFORE either operation.
    if !scale.is_normal() || scale <= 0.0 {
        return Err(WvError::Window(String::from(
            "native window scale factor is invalid",
        )));
    }
    let position = position.to_logical::<f64>(scale);
    let size = size.to_logical::<f64>(scale);
    if ![position.x, position.y, size.width, size.height]
        .into_iter()
        .all(f64::is_finite)
        || [position.x, position.y]
            .into_iter()
            .any(|value| value.round() < f64::from(i32::MIN) || value.round() > f64::from(i32::MAX))
        || [size.width, size.height]
            .into_iter()
            .any(|value| value.round() < 0.0 || value.round() > f64::from(u32::MAX))
    {
        return Err(WvError::Window(String::from(
            "native logical window bounds cannot be represented",
        )));
    }
    Ok((position.cast(), size.cast()))
}

#[cfg(test)]
mod tests {
    use super::{checked_logical_bounds, next_view_id};
    use tao::dpi::{PhysicalPosition, PhysicalSize};

    #[test]
    fn native_bounds_use_logical_outer_geometry_and_checked_dpi_rounding() {
        let (position, size) = checked_logical_bounds(
            PhysicalPosition::new(-75, 183),
            PhysicalSize::new(1600, 1221),
            2.0,
        )
        .expect("representable native bounds");
        assert_eq!(
            (position.x, position.y, size.width, size.height),
            (-38, 92, 800, 611)
        );
        for scale in [
            0.0,
            -1.0,
            f64::NAN,
            f64::INFINITY,
            f64::from_bits(1),
            f64::MIN_POSITIVE,
        ] {
            assert!(
                checked_logical_bounds(
                    PhysicalPosition::new(i32::MIN, 1),
                    PhysicalSize::new(2, u32::MAX),
                    scale
                )
                .is_err()
            );
        }
        let (position, size) = checked_logical_bounds(
            PhysicalPosition::new(i32::MIN, i32::MAX),
            PhysicalSize::new(0, u32::MAX),
            1.0,
        )
        .expect("exact integer bounds");
        assert_eq!(
            (position.x, position.y, size.width, size.height),
            (i32::MIN, i32::MAX, 0, u32::MAX)
        );
    }

    #[test]
    fn native_view_ids_are_checked_and_never_wrap_or_mint_zero() {
        let mut next = 7;
        assert_eq!(next_view_id(&mut next).expect("first view").0, 7);
        assert_eq!(next_view_id(&mut next).expect("next view").0, 8);
        for mut exhausted in [0, u32::MAX] {
            let original = exhausted;
            assert!(next_view_id(&mut exhausted).is_err());
            assert_eq!(exhausted, original);
        }
    }
}

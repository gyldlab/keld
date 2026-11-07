//! Source-bound component regression for Windows display::logical.
//! The function between the two markers must remain byte-identical to the
//! verified OneFive/Ramani production display.rs logical fragment.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AppDesktopRect {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

#[derive(Debug, PartialEq, Eq)]
struct WvError(&'static str);

fn unavailable(message: &'static str) -> WvError {
    WvError(message)
}

#[rustfmt::skip]
mod production {
    use super::{AppDesktopRect, WvError, unavailable};

    // BEGIN VERBATIM WINDOWS LOGICAL
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
// END VERBATIM WINDOWS LOGICAL
}

#[cfg(test)]
mod tests {
    use super::{AppDesktopRect, WvError, production};

    fn rect(x: i32, y: i32, width: u32, height: u32) -> AppDesktopRect {
        AppDesktopRect {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn observed_primary_bounds_and_work_area_use_effective_dpi() {
        // Independent physical Windows/Electron observation (not derived by the helper).
        assert_eq!(
            production::logical([0, 0, 1920, 1080], 1.25),
            Ok(rect(0, 0, 1536, 864))
        );
        assert_eq!(
            production::logical([0, 0, 1920, 1020], 1.25),
            Ok(rect(0, 0, 1536, 816))
        );
    }

    #[test]
    fn fractional_edges_enclose_the_complete_physical_rectangle() {
        // Deliberately non-divisible edges distinguish ceil(right/bottom) from truncation.
        assert_eq!(
            production::logical([1, 2, 1919, 1019], 1.25),
            Ok(rect(0, 1, 1536, 815))
        );
        assert_eq!(
            production::logical([0, 0, 1, 1], 1.25),
            Ok(rect(0, 0, 1, 1))
        );
        assert_eq!(
            production::logical([0, 0, 1919, 1019], 1.5),
            Ok(rect(0, 0, 1280, 680))
        );
    }

    #[test]
    fn zero_and_reversed_dimensions_refuse_instead_of_publishing_a_display() {
        assert_eq!(
            production::logical([5, 0, 5, 10], 1.0),
            Err(WvError("invalid logical width"))
        );
        assert_eq!(
            production::logical([5, 0, 4, 10], 1.0),
            Err(WvError("invalid logical width"))
        );
        assert_eq!(
            production::logical([0, 5, 10, 5], 1.0),
            Err(WvError("invalid logical height"))
        );
    }

    #[test]
    fn nonfinite_nonpositive_and_out_of_range_scale_refuse() {
        for scale in [0.0, -1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(
                production::logical([0, 0, 1920, 1080], scale),
                Err(WvError("invalid effective scale"))
            );
        }
        assert_eq!(
            production::logical([i32::MAX - 100, 0, i32::MAX, 8], 0.5),
            Err(WvError("logical x out of range"))
        );
    }
}

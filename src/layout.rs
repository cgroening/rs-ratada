//! Layout helpers.

use ratatui::layout::Rect;

/// Returns a `width` x `height` rect centered within `area`, clamped to it.
pub fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    }
}

/// Grows `wanted` to at least `min`, then caps it at `max`.
///
/// Not `wanted.clamp(min, max)`: a terminal smaller than a widget's preferred
/// minimum makes `max < min`, and [`Ord::clamp`] panics on that. Here `max`
/// wins instead, because the available space is the hard limit and the minimum
/// is only a preference.
pub fn fit(wanted: u16, min: u16, max: u16) -> u16 {
    wanted.max(min).min(max)
}

/// Returns a rect centered in `area` sized to the `numerator/denominator`
/// fraction of it, but never smaller than `min_width` x `min_height` (and never
/// larger than `area`). The shared sizing for centered popups.
///
/// A `denominator` of zero is treated as "the whole area" rather than dividing
/// by zero; it can only come from a caller mistake, and a popup sized to the
/// full area is the harmless reading.
pub fn centered_fraction(
    area: Rect,
    numerator: u16,
    denominator: u16,
    min_width: u16,
    min_height: u16,
) -> Rect {
    let width = fit(
        scale(area.width, numerator, denominator),
        min_width,
        area.width,
    );
    let height = fit(
        scale(area.height, numerator, denominator),
        min_height,
        area.height,
    );
    centered_rect(width, height, area)
}

/// Scales `extent` by `numerator/denominator`, in `u32` so the product cannot
/// overflow `u16` for a very wide area (`u16::MAX * 9` does not fit).
fn scale(extent: u16, numerator: u16, denominator: u16) -> u16 {
    if denominator == 0 {
        return extent;
    }
    let scaled =
        u32::from(extent) * u32::from(numerator) / u32::from(denominator);
    u16::try_from(scaled).unwrap_or(u16::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centers_within_the_area() {
        let area = Rect::new(0, 0, 100, 40);
        let rect = centered_rect(40, 10, area);
        assert_eq!(rect, Rect::new(30, 15, 40, 10));
    }

    #[test]
    fn clamps_to_the_area_when_larger() {
        let area = Rect::new(4, 6, 20, 8);
        let rect = centered_rect(200, 200, area);
        // Clamped to the area's size and pinned to its origin (no overflow).
        assert_eq!(rect, Rect::new(4, 6, 20, 8));
    }

    #[test]
    fn fit_prefers_the_minimum_but_the_maximum_wins() {
        assert_eq!(fit(10, 5, 20), 10); // the wanted size fits
        assert_eq!(fit(2, 5, 20), 5); // grown to the minimum
        assert_eq!(fit(30, 5, 20), 20); // capped at the maximum
        // The case `clamp` would panic on: no room for the minimum.
        assert_eq!(fit(10, 28, 20), 20);
    }

    #[test]
    fn centered_fraction_survives_an_area_below_its_minimum() {
        let area = Rect::new(0, 0, 10, 3);
        let rect = centered_fraction(area, 9, 10, 28, 5);
        assert_eq!(rect, area);
    }

    /// `area.width * numerator` overflowed `u16` for a very wide area, which
    /// panics in debug and silently wraps in release. The fraction must be
    /// computed in a wider type instead.
    #[test]
    fn centered_fraction_survives_an_area_too_wide_for_u16_math() {
        let area = Rect::new(0, 0, u16::MAX, u16::MAX);
        let rect = centered_fraction(area, 9, 10, 28, 5);
        let expected = u16::try_from(u32::from(u16::MAX) * 9 / 10)
            .expect("nine tenths of u16::MAX still fits u16");
        assert_eq!(rect.width, expected);
        assert_eq!(rect.height, expected);
    }

    /// A zero denominator is a caller mistake, but it must not divide by zero.
    #[test]
    fn centered_fraction_treats_a_zero_denominator_as_the_full_area() {
        let area = Rect::new(0, 0, 80, 24);
        let rect = centered_fraction(area, 9, 0, 10, 4);
        assert_eq!(rect, area);
    }
}

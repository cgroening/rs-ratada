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
pub fn centered_fraction(
    area: Rect,
    numerator: u16,
    denominator: u16,
    min_width: u16,
    min_height: u16,
) -> Rect {
    let width =
        fit(area.width * numerator / denominator, min_width, area.width);
    let height = fit(
        area.height * numerator / denominator,
        min_height,
        area.height,
    );
    centered_rect(width, height, area)
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
}

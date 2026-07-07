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
        (area.width * numerator / denominator).clamp(min_width, area.width);
    let height =
        (area.height * numerator / denominator).clamp(min_height, area.height);
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
}

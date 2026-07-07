//! Reusable list navigation and scroll-offset helpers.

/// A scroll window over a list: `viewport` units (rows or columns) are visible
/// starting at `offset`, out of `total` units.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScrollView {
    /// The total number of units (rows/columns).
    pub total: usize,
    /// The index of the first visible unit.
    pub offset: usize,
    /// The number of units visible at once.
    pub viewport: usize,
}

/// Moves `cursor` by `delta` within a list of `len`, wrapping at both ends.
///
/// One step past the last entry lands on the first and vice versa. An empty
/// list yields `0`.
pub fn cycle(cursor: usize, len: usize, delta: isize) -> usize {
    if len == 0 {
        return 0;
    }
    (cursor as isize + delta).rem_euclid(len as isize) as usize
}

/// Moves `cursor` by `delta`, clamping at the ends (used for page jumps and
/// Home/End, which are not cyclic).
pub fn step_clamped(cursor: usize, len: usize, delta: isize) -> usize {
    if len == 0 {
        return 0;
    }
    let last = (len - 1) as isize;
    (cursor as isize + delta).clamp(0, last) as usize
}

/// Returns a scroll offset that keeps `selected` inside `view`'s window,
/// scrolling only when the cursor would leave the currently visible rows.
pub fn keep_visible(view: ScrollView, selected: usize) -> usize {
    if view.viewport == 0 || view.total == 0 {
        return 0;
    }
    let max_offset = view.total.saturating_sub(view.viewport);
    let mut offset = view.offset.min(max_offset);
    if selected < offset {
        offset = selected;
    } else if selected >= offset + view.viewport {
        offset = selected + 1 - view.viewport;
    }
    offset.min(max_offset)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycle_wraps_at_both_ends() {
        assert_eq!(cycle(0, 3, -1), 2);
        assert_eq!(cycle(2, 3, 1), 0);
        assert_eq!(cycle(0, 0, 1), 0);
    }

    #[test]
    fn step_clamped_stops_at_the_edges() {
        assert_eq!(step_clamped(0, 3, -1), 0);
        assert_eq!(step_clamped(2, 3, 5), 2);
    }

    #[test]
    fn keep_visible_follows_the_cursor() {
        let view = |offset| ScrollView {
            total: 10,
            offset,
            viewport: 3,
        };
        assert_eq!(keep_visible(view(0), 5), 3);
        assert_eq!(keep_visible(view(5), 2), 2);
        assert_eq!(keep_visible(view(0), 0), 0);
    }

    #[test]
    fn keep_visible_handles_empty_and_zero_viewport() {
        let empty = ScrollView {
            total: 0,
            offset: 0,
            viewport: 3,
        };
        assert_eq!(keep_visible(empty, 0), 0);
        let no_viewport = ScrollView {
            total: 10,
            offset: 4,
            viewport: 0,
        };
        assert_eq!(keep_visible(no_viewport, 5), 0);
    }
}

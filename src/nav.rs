//! Reusable list navigation and scroll-offset helpers.

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

/// Returns a scroll offset that keeps `selected` inside a `viewport`-row window
/// over `total` rows, scrolling only when the cursor would leave the window.
pub fn keep_visible(
    offset: usize,
    selected: usize,
    viewport: usize,
    total: usize,
) -> usize {
    if viewport == 0 || total == 0 {
        return 0;
    }
    let max_offset = total.saturating_sub(viewport);
    let mut offset = offset.min(max_offset);
    if selected < offset {
        offset = selected;
    } else if selected >= offset + viewport {
        offset = selected + 1 - viewport;
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
        assert_eq!(keep_visible(0, 5, 3, 10), 3);
        assert_eq!(keep_visible(5, 2, 3, 10), 2);
        assert_eq!(keep_visible(0, 0, 3, 10), 0);
    }
}

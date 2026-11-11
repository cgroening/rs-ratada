//! Reusable list navigation and scroll-offset helpers.

use std::ops::Range;

use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyModifiers;

/// The footer hint of [`scroll_key`]: scroll the list a line without the
/// cursor.
pub const SCROLL_HINT: (&str, &str) = ("Alt+J/Alt+K", "scroll");

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

/// The offset after scrolling `view` by `delta` rows (down for a positive
/// `delta`), clamped so the window stays within the list; `0` when the list
/// fits.
#[must_use]
pub fn scroll_by(view: ScrollView, delta: isize) -> usize {
    let max_offset = view.total.saturating_sub(view.viewport);
    let offset = view.offset.min(max_offset) as isize + delta;
    offset.clamp(0, max_offset as isize) as usize
}

/// The rows on screen when `viewport` rows are visible from `offset` in a
/// list of `total`.
#[must_use]
pub fn visible_rows(
    offset: usize,
    viewport: usize,
    total: usize,
) -> Range<usize> {
    let start = offset.min(total);
    start..(start + viewport).min(total)
}

/// Scrolls `view` by `delta` rows without moving the cursor while its row
/// stays on screen; when it would leave, the cursor moves along to the
/// nearest visible row (for a list where every row can hold the cursor).
/// Returns the new offset and cursor.
#[must_use]
pub fn scroll_with_cursor(
    view: ScrollView,
    delta: isize,
    selected: usize,
) -> (usize, usize) {
    let offset = scroll_by(view, delta);
    let visible = visible_rows(offset, view.viewport, view.total);
    if visible.is_empty() {
        return (offset, selected);
    }
    (offset, selected.clamp(visible.start, visible.end - 1))
}

/// The rows `key` scrolls a list by without moving the cursor: `Alt+J` one
/// down, `Alt+K` one up (also typed as `J`/`K` with Shift); `None` for any
/// other key.
#[must_use]
pub fn scroll_key(key: &KeyEvent) -> Option<isize> {
    let extra = key.modifiers - KeyModifiers::SHIFT;
    if extra != KeyModifiers::ALT {
        return None;
    }
    match key.code {
        KeyCode::Char('j' | 'J') => Some(1),
        KeyCode::Char('k' | 'K') => Some(-1),
        _ => None,
    }
}

/// How far `view` is scrolled, as a percentage of its scrollable range. Content
/// that fits entirely is fully scrolled, hence `100`.
pub fn scroll_percent(view: ScrollView) -> usize {
    let max_offset = view.total.saturating_sub(view.viewport);
    if max_offset == 0 {
        return 100;
    }
    view.offset.min(max_offset) * 100 / max_offset
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

    #[test]
    fn scroll_percent_spans_the_scrollable_range() {
        let view = |total, offset, viewport| ScrollView {
            total,
            offset,
            viewport,
        };
        assert_eq!(scroll_percent(view(100, 0, 10)), 0);
        assert_eq!(scroll_percent(view(100, 45, 10)), 50);
        assert_eq!(scroll_percent(view(100, 90, 10)), 100);
    }

    #[test]
    fn scroll_percent_is_full_when_everything_fits() {
        assert_eq!(
            scroll_percent(ScrollView {
                total: 8,
                offset: 5,
                viewport: 20,
            }),
            100,
        );
    }

    fn view(offset: usize, viewport: usize, total: usize) -> ScrollView {
        ScrollView {
            total,
            offset,
            viewport,
        }
    }

    #[test]
    fn scroll_by_stays_within_the_list() {
        assert_eq!(scroll_by(view(0, 5, 20), 1), 1);
        assert_eq!(scroll_by(view(0, 5, 20), -1), 0);
        assert_eq!(scroll_by(view(15, 5, 20), 1), 15);
        assert_eq!(scroll_by(view(3, 10, 8), 1), 0, "a list that fits");
    }

    #[test]
    fn the_cursor_stays_while_visible_and_moves_along_at_the_edge() {
        assert_eq!(scroll_with_cursor(view(0, 5, 20), 1, 3), (1, 3));
        assert_eq!(scroll_with_cursor(view(0, 5, 20), 1, 0), (1, 1));
        assert_eq!(scroll_with_cursor(view(5, 5, 20), -1, 9), (4, 8));
        assert_eq!(scroll_with_cursor(view(0, 5, 0), 1, 0), (0, 0));
    }

    #[test]
    fn only_alt_j_and_alt_k_scroll() {
        let key = |code, modifiers| KeyEvent::new(code, modifiers);
        let alt = KeyModifiers::ALT;
        assert_eq!(scroll_key(&key(KeyCode::Char('j'), alt)), Some(1));
        assert_eq!(scroll_key(&key(KeyCode::Char('k'), alt)), Some(-1));
        let alt_shift = alt | KeyModifiers::SHIFT;
        assert_eq!(scroll_key(&key(KeyCode::Char('J'), alt_shift)), Some(1));
        assert_eq!(
            scroll_key(&key(KeyCode::Char('j'), KeyModifiers::NONE)),
            None
        );
        let ctrl_alt = alt | KeyModifiers::CONTROL;
        assert_eq!(scroll_key(&key(KeyCode::Char('j'), ctrl_alt)), None);
        assert_eq!(scroll_key(&key(KeyCode::Down, alt)), None);
    }
}

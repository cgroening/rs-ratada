//! The floating rendering: a borderless menu drawn over the view.
//!
//! The menu sits under the field (or above it when the room below runs out),
//! its labels starting in the column where the value starts, so the typed text
//! and the suggestions line up. Nothing around it moves: it overdraws the rows
//! below instead of pushing them down. The look follows blink.cmp's compact
//! menu - a flat fill with one blank column on either side, the labels bold
//! with the matched part plain, and a solid-cell scrollbar in the right-hand
//! column once the matches outgrow [`MAX_VISIBLE`].

use std::ops::Range;

use ratatui::{
    Frame,
    layout::{Position, Rect},
    style::{Color as RatColor, Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, Paragraph},
};
use unicode_width::UnicodeWidthStr;

use super::{Autocomplete, MAX_VISIBLE, Match};
use crate::nav::ScrollView;
use crate::scroll::{self, CellColors};
use crate::style;
use crate::text;
use crate::theme::Palette;

/// Narrowest menu, so a short match still reads as a menu.
const MIN_WIDTH: u16 = 12;
/// Blank columns on each side of the labels; the right one holds the bar.
const PADDING: u16 = 1;
/// Narrowest menu worth drawing: one char of text inside the padding.
const MIN_DRAWABLE_WIDTH: u16 = 2 * PADDING + 1;

/// The colors of the floating menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PopupStyle {
    /// The menu fill.
    pub background: RatColor,
    /// The labels (drawn bold).
    pub foreground: RatColor,
    /// The fill of the highlighted row.
    pub selection_background: RatColor,
    /// The part of a label that matches the query (drawn plain, so it stands
    /// out from the bold rest).
    pub match_foreground: RatColor,
    /// The scrollbar thumb.
    pub scrollbar_thumb: RatColor,
    /// The scrollbar track.
    pub scrollbar_track: RatColor,
}

impl PopupStyle {
    /// Derives the menu colors from a toolkit palette: the panel fill, the
    /// selection tint and the dimmed foreground for the thumb.
    pub fn from_palette(palette: &Palette) -> Self {
        Self {
            background: style::to_ratatui(palette.panel),
            foreground: style::to_ratatui(palette.foreground),
            selection_background: style::to_ratatui(palette.selection),
            match_foreground: style::to_ratatui(palette.foreground),
            scrollbar_thumb: style::to_ratatui(palette.foreground_dim),
            scrollbar_track: style::to_ratatui(palette.border),
        }
    }
}

/// The label and match styles of one menu row.
struct RowStyles {
    label: Style,
    hit: Style,
}

impl Autocomplete {
    /// Draws the open suggestions as a floating menu; a no-op when closed.
    ///
    /// `anchor` is the screen cell where the field's value starts: the menu
    /// opens on the row below with its labels in that column (its padding one
    /// column further left), and flips above the field when there is more room
    /// there than below. The menu never leaves
    /// `bounds` (the host's panel or modal), so it shrinks - scrolling the rest
    /// - or is skipped when even one row does not fit.
    pub fn render_popup(
        &self,
        frame: &mut Frame,
        anchor: Position,
        bounds: Rect,
        style: &PopupStyle,
    ) {
        if !self.is_open() {
            return;
        }
        let rows = self.matches.len().min(MAX_VISIBLE) as u16;
        let width = self.label_width().saturating_add(2 * PADDING);
        let Some(area) = popup_area(anchor, bounds, width, rows) else {
            return;
        };
        let viewport = usize::from(area.height);
        let offset = self.window_offset(viewport);
        let text_width = usize::from(area.width - 2 * PADDING);
        let lines: Vec<Line<'static>> = self
            .matches
            .iter()
            .enumerate()
            .skip(offset)
            .take(viewport)
            .map(|(row, hit)| {
                let selected = Some(row) == self.selected;
                self.popup_row(hit, text_width, selected, style)
            })
            .collect();
        frame.render_widget(Clear, area);
        frame.render_widget(
            Paragraph::new(lines).style(Style::default().bg(style.background)),
            area,
        );
        let bar = Rect {
            x: area.right() - PADDING,
            width: PADDING,
            ..area
        };
        let view = ScrollView {
            total: self.matches.len(),
            offset,
            viewport,
        };
        let colors = CellColors {
            thumb: style.scrollbar_thumb,
            track: style.scrollbar_track,
        };
        scroll::render_cell_scrollbar(frame, bar, view, &colors);
    }

    /// The display width of the widest matching label, capped to the screen.
    fn label_width(&self) -> u16 {
        let widest = self
            .matches
            .iter()
            .map(|hit| self.candidates[hit.index].width())
            .max()
            .unwrap_or(0);
        u16::try_from(widest).unwrap_or(u16::MAX)
    }

    /// One menu row, padded to the full menu width so its fill is solid.
    fn popup_row(
        &self,
        hit: &Match,
        text_width: usize,
        selected: bool,
        style: &PopupStyle,
    ) -> Line<'static> {
        let fill = if selected {
            style.selection_background
        } else {
            style.background
        };
        let base = Style::default().bg(fill);
        let styles = RowStyles {
            label: base.fg(style.foreground).add_modifier(Modifier::BOLD),
            hit: base.fg(style.match_foreground),
        };
        let pad = " ".repeat(usize::from(PADDING));
        let mut spans = vec![Span::styled(pad.clone(), base)];
        let label = &self.candidates[hit.index];
        spans.extend(label_spans(label, &hit.range, text_width, &styles));
        spans.push(Span::styled(pad, base));
        Line::from(spans)
    }
}

/// Places a menu of `width` x `rows` cells for a field whose value starts at
/// `anchor`, inside `bounds`.
///
/// The menu goes on the row below `anchor` while it fits there, flips above
/// when the room above is larger, and otherwise shrinks to the room below. It
/// starts [`PADDING`] columns left of `anchor`, so its labels line up with the
/// value; it is at least [`MIN_WIDTH`] wide, never wider than `bounds`, and
/// shifted left when it would cross the right edge. `None` when `anchor` lies
/// outside `bounds`, `rows` is zero, or no usable rectangle remains.
fn popup_area(
    anchor: Position,
    bounds: Rect,
    width: u16,
    rows: u16,
) -> Option<Rect> {
    let inside = anchor.y >= bounds.y && anchor.y < bounds.bottom();
    if rows == 0 || !inside {
        return None;
    }
    let width = width.max(MIN_WIDTH).min(bounds.width);
    if width < MIN_DRAWABLE_WIDTH {
        return None;
    }
    let below = bounds.bottom() - anchor.y - 1;
    let above = anchor.y - bounds.y;
    let (y, height) = if rows <= below || below >= above {
        (anchor.y + 1, rows.min(below))
    } else {
        let height = rows.min(above);
        (anchor.y - height, height)
    };
    if height == 0 {
        return None;
    }
    let x = anchor
        .x
        .saturating_sub(PADDING)
        .max(bounds.x)
        .min(bounds.right() - width);
    Some(Rect {
        x,
        y,
        width,
        height,
    })
}

/// The spans of `label` clipped to `width` columns (with `…` when it does not
/// fit) and padded to exactly `width`: the matched `range` in the hit style,
/// the rest in the label style. A match cut off by the clip is shown as far as
/// it is visible.
fn label_spans(
    label: &str,
    range: &Range<usize>,
    width: usize,
    styles: &RowStyles,
) -> Vec<Span<'static>> {
    let shown = text::truncate(label, width);
    // Chars of `label` still on screen; a clip replaced the rest with `…`.
    let kept = if label.width() <= width {
        label.chars().count()
    } else {
        shown.chars().count().saturating_sub(1)
    };
    let hit_end = range.end.min(kept);
    let hit_start = range.start.min(hit_end);
    let start = byte_offset(&shown, hit_start);
    let end = byte_offset(&shown, hit_end);
    let filler = " ".repeat(width.saturating_sub(shown.width()));
    [
        (&shown[..start], styles.label),
        (&shown[start..end], styles.hit),
        (&shown[end..], styles.label),
        (filler.as_str(), styles.label),
    ]
    .into_iter()
    .filter(|(part, _)| !part.is_empty())
    .map(|(part, part_style)| Span::styled(part.to_string(), part_style))
    .collect()
}

/// The byte offset of char `index` in `text` (its length past the end).
fn byte_offset(text: &str, index: usize) -> usize {
    text.char_indices()
        .nth(index)
        .map_or(text.len(), |(offset, _)| offset)
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::{Terminal, backend::TestBackend, buffer::Buffer};

    use super::*;

    const BG: RatColor = RatColor::Rgb(62, 62, 62);
    const SELECTION: RatColor = RatColor::Rgb(21, 38, 64);
    const THUMB: RatColor = RatColor::Rgb(121, 121, 121);
    const TRACK: RatColor = RatColor::Rgb(60, 60, 60);

    fn nox() -> PopupStyle {
        PopupStyle {
            background: BG,
            foreground: RatColor::Rgb(212, 212, 212),
            selection_background: SELECTION,
            match_foreground: RatColor::Rgb(200, 200, 200),
            scrollbar_thumb: THUMB,
            scrollbar_track: TRACK,
        }
    }

    fn at(x: u16, y: u16) -> Position {
        Position { x, y }
    }

    fn rect(x: u16, y: u16, width: u16, height: u16) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    /// Renders `ac`'s popup for a value at `anchor` into a 40x20 screen
    /// pre-filled with `.`, so untouched cells are easy to tell apart.
    fn render(ac: &Autocomplete, anchor: Position, bounds: Rect) -> Buffer {
        let mut terminal = Terminal::new(TestBackend::new(40, 20))
            .expect("a TestBackend terminal cannot fail to build");
        terminal
            .draw(|frame| {
                let dots = vec![Line::from(".".repeat(40)); 20];
                frame.render_widget(Paragraph::new(dots), frame.area());
                ac.render_popup(frame, anchor, bounds, &nox());
            })
            .expect("drawing into a TestBackend cannot fail");
        terminal.backend().buffer().clone()
    }

    fn row_text(buffer: &Buffer, y: u16, x: u16, width: u16) -> String {
        (x..x + width)
            .map(|col| buffer[(col, y)].symbol())
            .collect()
    }

    fn opened(candidates: &[&str], query: &str) -> Autocomplete {
        let owned = candidates.iter().map(|c| (*c).to_string()).collect();
        let mut ac = Autocomplete::new(owned);
        ac.refresh(query);
        ac
    }

    fn screen() -> Rect {
        rect(0, 0, 40, 20)
    }

    #[test]
    fn area_opens_below_the_anchor() {
        let area = popup_area(at(5, 2), screen(), 14, 3);
        assert_eq!(area, Some(rect(4, 3, 14, 3)));
    }

    #[test]
    fn area_flips_above_when_the_room_below_runs_out() {
        let area = popup_area(at(5, 18), screen(), 14, 3);
        assert_eq!(area, Some(rect(4, 15, 14, 3)));
    }

    #[test]
    fn area_shrinks_below_when_there_is_less_room_above() {
        let area = popup_area(at(5, 8), screen(), 14, 12);
        assert_eq!(area, Some(rect(4, 9, 14, 11)));
    }

    #[test]
    fn area_shifts_left_at_the_right_edge() {
        let area = popup_area(at(35, 2), screen(), 14, 3);
        assert_eq!(area, Some(rect(26, 3, 14, 3)));
    }

    #[test]
    fn area_has_a_minimum_and_a_maximum_width() {
        assert_eq!(
            popup_area(at(0, 0), screen(), 5, 1).map(|a| a.width),
            Some(12)
        );
        let narrow = rect(0, 0, 8, 20);
        assert_eq!(
            popup_area(at(0, 0), narrow, 30, 1).map(|a| a.width),
            Some(8)
        );
    }

    #[test]
    fn area_stays_inside_offset_bounds() {
        let bounds = rect(10, 5, 20, 6);
        let area = popup_area(at(12, 6), bounds, 14, 8);
        assert_eq!(area, Some(rect(11, 7, 14, 4)));
        // Even at the bounds' left edge the padding never leaves them.
        let area = popup_area(at(10, 6), bounds, 14, 8);
        assert_eq!(area.map(|a| a.x), Some(10));
    }

    #[test]
    fn area_is_none_without_usable_room() {
        assert_eq!(popup_area(at(5, 2), screen(), 14, 0), None);
        assert_eq!(popup_area(at(5, 25), screen(), 14, 3), None);
        assert_eq!(popup_area(at(0, 0), rect(0, 0, 2, 20), 14, 3), None);
        assert_eq!(popup_area(at(0, 0), rect(0, 0, 40, 1), 14, 3), None);
        assert_eq!(popup_area(at(0, 0), Rect::default(), 14, 3), None);
    }

    #[test]
    fn popup_overdraws_without_moving_the_rows_around_it() {
        let ac = opened(&["Rewe big shop", "Rewe small"], "rewe");
        let buffer = render(&ac, at(4, 2), screen());
        // Menu: 13 label columns + 2 padding at x 3..18, rows 3 and 4; the
        // labels start in the anchor's column.
        assert_eq!(row_text(&buffer, 3, 3, 15), " Rewe big shop ");
        assert_eq!(row_text(&buffer, 4, 3, 15), " Rewe small    ");
        assert_eq!(buffer[(4, 3)].symbol(), "R");
        assert_eq!(row_text(&buffer, 2, 0, 40), ".".repeat(40));
        assert_eq!(row_text(&buffer, 5, 0, 40), ".".repeat(40));
        assert_eq!(row_text(&buffer, 3, 0, 3), "...");
        assert_eq!(buffer[(3, 3)].bg, BG);
    }

    #[test]
    fn closed_suggestions_draw_nothing() {
        let ac = Autocomplete::new(vec!["Edeka".into()]);
        let buffer = render(&ac, at(4, 2), screen());
        assert_eq!(row_text(&buffer, 3, 0, 40), ".".repeat(40));
    }

    #[test]
    fn labels_are_bold_and_the_match_is_plain() {
        let ac = opened(&["Rewe small"], "small");
        let buffer = render(&ac, at(0, 0), screen());
        // " Rewe small": "R" at x 1 is label, "s" at x 6 is the match.
        assert!(buffer[(1, 1)].modifier.contains(Modifier::BOLD));
        assert!(!buffer[(6, 1)].modifier.contains(Modifier::BOLD));
        assert_eq!(buffer[(6, 1)].fg, nox().match_foreground);
    }

    #[test]
    fn the_selected_row_is_filled_across_the_menu() {
        let mut ac = opened(&["Rewe big shop", "Rewe small"], "rewe");
        ac.on_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE)); // row 1
        let buffer = render(&ac, at(0, 0), screen());
        assert_eq!(buffer[(0, 2)].bg, SELECTION);
        assert_eq!(buffer[(14, 2)].bg, SELECTION);
        assert_eq!(buffer[(0, 1)].bg, BG);
    }

    #[test]
    fn scrollbar_appears_only_beyond_the_visible_rows() {
        let few = opened(&["Store 1", "Store 2"], "store");
        let buffer = render(&few, at(0, 0), screen());
        // 12 wide (minimum): the right padding column keeps the fill.
        assert_eq!(buffer[(11, 1)].bg, BG);

        let names: Vec<String> =
            (0..12).map(|n| format!("Store {n:02}")).collect();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let many = opened(&refs, "store");
        let buffer = render(&many, at(0, 0), screen());
        let column: Vec<RatColor> =
            (1..=8).map(|y| buffer[(11, y)].bg).collect();
        assert!(column.contains(&THUMB));
        assert!(column.contains(&TRACK));
        assert_eq!(row_text(&buffer, 9, 0, 12), ".".repeat(12));
    }

    #[test]
    fn long_labels_are_clipped_with_an_ellipsis() {
        let ac = opened(&["A very long supplier name"], "supplier");
        let buffer = render(&ac, at(0, 0), rect(0, 0, 14, 20));
        assert_eq!(row_text(&buffer, 1, 0, 14), " A very long… ");
    }

    #[test]
    fn a_clipped_match_is_highlighted_as_far_as_it_shows() {
        let styles = RowStyles {
            label: Style::default().add_modifier(Modifier::BOLD),
            hit: Style::default(),
        };
        let spans = label_spans("abcdefgh", &(4..8), 6, &styles);
        let parts: Vec<&str> =
            spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(parts, vec!["abcd", "e", "…"]);
    }

    #[test]
    fn wide_glyphs_keep_the_row_width() {
        let styles = RowStyles {
            label: Style::default(),
            hit: Style::default(),
        };
        let spans = label_spans("日本語テキスト", &(0..2), 7, &styles);
        let width: usize = spans.iter().map(|s| s.content.width()).sum();
        assert_eq!(width, 7);
    }
}

//! A multi-line text area: wrapped editing with a caret, selection and
//! clipboard. Reuses [`TextCursor`] and the shared edit core
//! [`input::apply_edit_key`] from the `input` module, so it carries the same
//! shortcuts as a single-line field - including `Ctrl+U`/`Ctrl+K`, which act on
//! the **display** line, not the logical one.
//!
//! Wrapping is word-aware: a soft break falls on the last space that fits, and
//! a word longer than the width is hard-split. [`wrap_offsets`],
//! [`cursor_to_display`] and [`display_to_cursor`] expose that mapping, for a
//! host that measures and renders a wrapped box itself.
//!
//! The caller handles its own control keys (e.g. `Esc`, `Ctrl+G` for an
//! external editor) before delegating editing keys here.

mod wrap;

pub use wrap::{cursor_to_display, display_to_cursor, wrap_offsets};
use wrap::{locate, wrap_ranges};

use std::cell::Cell;

use crossterm::event::KeyEvent;
use ratatui::{Frame, layout::Rect, text::Line, widgets::Paragraph};

use super::{
    chrome,
    input::{self, TextCursor},
    nav, scroll, style,
};
use crate::theme::Skin;

/// A wrapped, editable multi-line text buffer.
#[derive(Default)]
pub struct TextArea {
    text: String,
    cursor: TextCursor,
    width: Cell<usize>,
    height: Cell<usize>,
    scroll: Cell<usize>,
    max_len: Option<usize>,
    decor: Option<chrome::BoxDecor>,
}

impl TextArea {
    /// Creates a multi-line editor pre-filled with `initial`.
    ///
    /// # Examples
    ///
    /// ```
    /// use ratada::textarea::TextArea;
    ///
    /// let area = TextArea::new("line 1\nline 2").max_len(200);
    /// assert_eq!(area.text(), "line 1\nline 2");
    /// ```
    pub fn new(initial: &str) -> Self {
        Self {
            text: initial.to_string(),
            cursor: TextCursor::at_end(initial),
            width: Cell::new(1),
            height: Cell::new(1),
            scroll: Cell::new(0),
            ..Self::default()
        }
    }

    /// Limits the buffer to `max` characters (enforced on typing and paste) and
    /// feeds the badge in the boxed variant.
    #[must_use]
    pub fn max_len(mut self, max: usize) -> Self {
        self.max_len = Some(max);
        self
    }

    /// Draws the area inside a rounded box with the given caption/badge (see
    /// [`chrome::BoxDecor`]); omit it for a plain area.
    #[must_use]
    pub fn boxed(mut self, decor: chrome::BoxDecor) -> Self {
        self.decor = Some(decor);
        self
    }

    /// The current buffer contents.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Replaces the whole buffer (e.g. after an external editor) and parks the
    /// caret at the end.
    pub fn set_text(&mut self, text: String) {
        self.cursor = TextCursor::at_end(&text);
        self.text = text;
    }

    /// Applies one editing key; returns whether it was consumed.
    ///
    /// The whole behaviour lives in [`input::apply_edit_key`], driven with the
    /// current wrap width, so a text area and a single-line field never drift
    /// apart.
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        input::apply_edit_key(
            &mut self.text,
            &mut self.cursor,
            key,
            input::EditMode::Multiline {
                width: self.width.get().max(1),
                height: self.height.get().max(1),
            },
            self.max_len,
        )
    }

    /// Inserts pasted `text` at the caret, replacing any active selection.
    ///
    /// Newlines are kept (the box wraps and grows) while other control
    /// characters are dropped, honoring the length limit. This routes a
    /// bracketed paste; `Ctrl+V` goes through [`Self::handle_key`].
    pub fn paste(&mut self, text: &str) {
        input::paste_text(
            &mut self.text,
            &mut self.cursor,
            input::EditMode::Multiline {
                width: self.width.get().max(1),
                height: self.height.get().max(1),
            },
            self.max_len,
            text,
        );
    }

    /// Renders the buffer into `area`, scrolling so the caret stays visible and
    /// filling the field with the input background (active tint when `focused`).
    /// A block caret is shown only when `focused`. Wrapped in a box when
    /// decorated via [`Self::boxed`].
    pub fn render(
        &self,
        frame: &mut Frame,
        area: Rect,
        skin: &Skin,
        focused: bool,
    ) {
        let inner = match &self.decor {
            Some(decor) => {
                chrome::framed_decor(frame, area, skin, decor, &self.badge())
            }
            None => area,
        };
        let palette = &skin.palette;
        let base_bg = if focused {
            palette.input_bg_active
        } else {
            palette.input_bg
        };

        let width = inner.width.max(1) as usize;
        let height = inner.height.max(1) as usize;
        self.width.set(width);
        self.height.set(height);
        let chars: Vec<char> = self.text.chars().collect();
        let rows = wrap_ranges(&chars, width);
        let (caret_row, caret_col) = locate(&rows, self.cursor.pos);

        let scroll = nav::keep_visible(
            nav::ScrollView {
                total: rows.len(),
                offset: self.scroll.get(),
                viewport: height,
            },
            caret_row,
        );
        self.scroll.set(scroll);

        let selection = self.cursor.selection();
        let lines: Vec<Line> = rows
            .iter()
            .enumerate()
            .skip(scroll)
            .take(height)
            .map(|(row_index, &(start, end))| {
                let visible: String = chars[start..end].iter().collect();
                let paint = input::LinePaint {
                    caret: input::LineCaret {
                        cursor: (focused && row_index == caret_row)
                            .then_some(caret_col),
                        selection: selection
                            .and_then(|(from, to)| {
                                input::intersect(from, to, start, end)
                            })
                            .map(|(from, to)| (from - start, to - start)),
                    },
                    ..input::LinePaint::default()
                };
                Line::from(input::line_spans(&visible, paint, palette))
            })
            .collect();

        // The paragraph's base style fills the whole field (including blank
        // rows) with the input background; spans override per cell.
        frame.render_widget(
            Paragraph::new(lines).style(style::bg(base_bg)),
            inner,
        );
        // A scrollbar on the right whenever the wrapped text overflows.
        scroll::render_scrollbar(
            frame,
            inner,
            skin,
            nav::ScrollView {
                total: rows.len(),
                offset: scroll,
                viewport: height,
            },
        );
    }

    /// The automatic badge text: character count, or `n/max` with a limit.
    fn badge(&self) -> String {
        let count = self.text.chars().count();
        match self.max_len {
            Some(max) => format!("{count}/{max}"),
            None => count.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyModifiers};
    use unicode_width::UnicodeWidthStr;

    use super::*;

    /// Texts covering every branch of the wrap: a plain run, runs of spaces, an
    /// explicit break, a trailing break, an over-long word and wide chars.
    const WRAP_SAMPLES: [&str; 8] = [
        "",
        "hello world",
        "aaaa  bbbb   cccc",
        "one\ntwo three",
        "trailing\n",
        "supercalifragilistic word",
        "日本語 テスト",
        "a b c d e f g h i j k",
    ];

    #[test]
    fn narrower_boxes_never_wrap_to_more_columns_of_text() {
        // A box only ever needs *more* rows as it gets narrower. Callers lean on
        // this to probe a layout once: a box that overflows at its full width
        // still overflows a column in, so reserving that column for a scroll
        // indicator can never turn an overflowing box into a fitting one.
        for text in WRAP_SAMPLES {
            for width in 1..40usize {
                let wide = wrap_offsets(text, width + 1).len();
                let narrow = wrap_offsets(text, width).len();
                assert!(
                    narrow >= wide,
                    "{text:?}: width {width} wrapped to {narrow} rows, \
                     but width {} wrapped to {wide}",
                    width + 1,
                );
            }
        }
    }

    #[test]
    fn a_soft_break_leaves_its_row_a_column_short() {
        // A soft break falls on a space *inside* the width and consumes it, so
        // the row it ends is always at least one column short. Only a hard split
        // or an explicit newline can leave a row exactly full - which is why a
        // caller cannot read "this row is full" as "this row is the last one".
        let chars: Vec<Vec<char>> =
            WRAP_SAMPLES.iter().map(|t| t.chars().collect()).collect();
        for (text, chars) in WRAP_SAMPLES.iter().zip(&chars) {
            for width in 1..40usize {
                let rows = wrap_offsets(text, width);
                for (index, (row, start)) in rows.iter().enumerate() {
                    let end = start + row.chars().count();
                    // The break is soft only when the consumed char is a space;
                    // a newline is the caller's own break, not the wrap's.
                    let soft = rows
                        .get(index + 1)
                        .is_some_and(|(_, next)| *next == end + 1)
                        && chars.get(end) == Some(&' ');
                    if soft {
                        assert!(
                            row.width() < width,
                            "{text:?} at {width}: soft-broken row {index} \
                             ({row:?}) fills the box",
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn altgr_char_is_typed_not_swallowed() {
        // German keyboards emit `\` as AltGr (Control + Alt); it must insert.
        let mut area = TextArea::new("C:");
        let altgr = KeyEvent::new(
            KeyCode::Char('\\'),
            KeyModifiers::CONTROL | KeyModifiers::ALT,
        );
        assert!(area.handle_key(altgr));
        assert_eq!(area.text(), "C:\\");

        // A genuine Ctrl chord still does not type a character.
        let mut area = TextArea::new("C:");
        let ctrl_s = KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL);
        assert!(!area.handle_key(ctrl_s));
        assert_eq!(area.text(), "C:");
    }

    #[test]
    fn enter_inserts_newline() {
        let mut area = TextArea::new("ab");
        area.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(area.text(), "ab\n");
    }

    #[test]
    fn max_len_blocks_typing_past_the_limit() {
        let mut area = TextArea::new("ab").max_len(3);
        area.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE));
        assert_eq!(area.text(), "abc");
        area.handle_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE));
        assert_eq!(area.text(), "abc");
    }

    #[test]
    fn backspace_delegates_to_the_shared_edit_core() {
        let mut area = TextArea::new("ab");
        area.handle_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        assert_eq!(area.text(), "a");
    }

    #[test]
    fn shift_left_selects_and_typing_replaces_via_the_core() {
        let mut area = TextArea::new("abc");
        area.handle_key(KeyEvent::new(KeyCode::Left, KeyModifiers::SHIFT));
        assert_eq!(area.cursor.selection(), Some((2, 3)));
        area.handle_key(KeyEvent::new(KeyCode::Char('X'), KeyModifiers::NONE));
        assert_eq!(area.text(), "abX");
    }

    #[test]
    fn page_down_moves_by_the_stored_viewport_height() {
        // Five rows via newlines (starts 0,3,6,9,12); a two-row viewport.
        let mut area = TextArea::new("l0\nl1\nl2\nl3\nl4");
        area.width.set(10);
        area.height.set(2);
        area.cursor.move_to(0);
        area.handle_key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE));
        assert_eq!(area.cursor.pos, 6); // two rows down: "l2"
    }

    #[test]
    fn page_down_falls_back_to_one_row_before_a_render() {
        // Without a render the height defaults to one row, so a page is one row.
        let mut area = TextArea::new("l0\nl1\nl2");
        area.width.set(10);
        area.cursor.move_to(0);
        area.handle_key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE));
        assert_eq!(area.cursor.pos, 3); // one row down: "l1"
    }

    #[test]
    fn ctrl_a_selects_all_then_backspace_clears() {
        let mut area = TextArea::new("line 1\nline 2");
        area.handle_key(KeyEvent::new(
            KeyCode::Char('a'),
            KeyModifiers::CONTROL,
        ));
        assert_eq!(area.cursor.selection(), Some((0, 13)));
        area.handle_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        assert_eq!(area.text(), "");
    }
}

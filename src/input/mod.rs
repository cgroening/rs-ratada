//! Shared text editing: one caret with an optional selection anchor over a
//! `String`. This is the single source of editing behaviour for every text
//! field, so shortcuts stay consistent.
//!
//! [`apply_edit_key`] is that core, and [`EditMode`] picks the geometry it
//! works in: [`InputField`] drives it with [`EditMode::SingleLine`], and
//! `textarea::TextArea` with [`EditMode::Multiline`], where the line-oriented
//! keys follow the wrap. Both therefore share one set of shortcuts.
//!
//! The editor handles only editing keys. A field's control keys (`Esc`, a
//! confirming `Enter`, other chords) belong to the caller and must be handled
//! before delegating here.
//!
//! A host that lays out its own text can still borrow the caret behaviour
//! instead of copying it: [`line_spans`] paints one already-windowed line,
//! [`scrolled_line_spans`] scrolls a single line, and [`query_spans_at`] draws
//! a movable caret over a filter line.

mod clip;
mod edit;
mod keys;
mod mutate;
mod paint;

pub use clip::{handle_clipboard, paste_text};
pub use edit::apply_edit_key;
pub use keys::{is_bare_character, is_command};
pub use mutate::{insert_str, replace_selection, selected_text};
#[cfg(test)]
use paint::SCROLL_MARKER;
pub use paint::{
    LinePaint, ScrollPaint, intersect, line_spans, query_spans, query_spans_at,
    scrolled_line_spans,
};
use paint::{LineView, caret_spans};

use crossterm::event::KeyEvent;
use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
    widgets::Paragraph,
};

use super::{chrome, style};
use crate::theme::{Palette, Skin};

/// A text caret with an optional selection anchor, both as char indices.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TextCursor {
    /// The caret position, as a char index.
    pub pos: usize,
    /// The selection anchor (a char index), or `None` when nothing is selected.
    pub anchor: Option<usize>,
}

impl TextCursor {
    /// A caret at `pos` with no selection.
    #[must_use]
    pub fn at(pos: usize) -> Self {
        Self { pos, anchor: None }
    }

    /// A caret placed at the end of `text`.
    #[must_use]
    pub fn at_end(text: &str) -> Self {
        Self::at(text.chars().count())
    }

    /// Moves the caret to `pos`, dropping any selection (a plain cursor move).
    pub fn move_to(&mut self, pos: usize) {
        self.pos = pos;
        self.anchor = None;
    }

    /// Moves the caret to `pos`, seeding the anchor at the old caret when no
    /// selection is active (a Shift-extended move).
    pub fn extend_to(&mut self, pos: usize) {
        self.anchor.get_or_insert(self.pos);
        self.pos = pos;
    }

    /// Selects the whole value of `len` characters.
    pub fn select_all(&mut self, len: usize) {
        self.anchor = Some(0);
        self.pos = len;
    }

    /// The current selection as an ordered `(start, end)` range, if any.
    #[must_use]
    pub fn selection(&self) -> Option<(usize, usize)> {
        let anchor = self.anchor?;
        let (start, end) = if anchor <= self.pos {
            (anchor, self.pos)
        } else {
            (self.pos, anchor)
        };
        (start != end).then_some((start, end))
    }

    /// Whether a non-empty selection is active.
    #[must_use]
    pub fn has_selection(&self) -> bool {
        self.selection().is_some()
    }
}

/// Where to paint the caret and selection within one already-windowed line, in
/// local (per-line) character columns.
///
/// A caller that lays out its own text - a wrapped multiline box, a scrolled
/// field - maps the global caret into line-local columns and hands the result
/// to [`line_spans`].
#[derive(Debug, Clone, Copy, Default)]
pub struct LineCaret {
    /// The caret column on this line, or `None` when the caret is elsewhere.
    pub cursor: Option<usize>,
    /// The selected column range on this line as a half-open `(start, end)`.
    pub selection: Option<(usize, usize)>,
}

/// Whether an input is a single line, a word-wrapped box, or one logical line
/// that merely *looks* wrapped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditMode {
    /// One line: `Home`/`End` jump to the value's start/end, `Enter` is free
    /// for the caller and a paste drops its line breaks.
    SingleLine,
    /// A box wrapped at `width` columns and `height` display rows tall:
    /// `Home`/`End` act on the display line, `Up`/`Down` walk them,
    /// `PageUp`/`PageDown` move by `height` rows, `Enter` inserts a newline and
    /// a paste keeps them.
    Multiline {
        /// The wrap width in columns.
        width: usize,
        /// The viewport height in display rows, driving `PageUp`/`PageDown`.
        height: usize,
    },
    /// One logical line, soft-wrapped at `width` columns for display only.
    ///
    /// Navigation behaves as in [`EditMode::Multiline`] - `Home`/`End` act on
    /// the display line and `Up`/`Down` walk the wrapped rows - but the buffer
    /// never gains a `\n`: `Enter` is left to the caller and a paste drops its
    /// line breaks, exactly as in [`EditMode::SingleLine`].
    ///
    /// This is the mode for a field whose value is a single line by definition
    /// while being too long to show on one row - an expression, a query, a
    /// path. Using [`EditMode::Multiline`] there would let a paste smuggle a
    /// newline into a value that must not contain one.
    Wrapped {
        /// The wrap width in columns.
        width: usize,
        /// The viewport height in display rows, driving `PageUp`/`PageDown`.
        height: usize,
    },
}

impl EditMode {
    /// The column count the text wraps at, or `None` when it does not wrap.
    fn wrap_width(self) -> Option<usize> {
        match self {
            EditMode::SingleLine => None,
            EditMode::Multiline { width, .. }
            | EditMode::Wrapped { width, .. } => Some(width),
        }
    }

    /// Whether the text is laid out over several display rows, which is what
    /// gives `Up`/`Down` and the page keys something to move through.
    fn is_wrapping(self) -> bool {
        self.wrap_width().is_some()
    }

    /// Whether the buffer may contain a `\n`. Only a true multiline box may;
    /// this is what decides whether `Enter` inserts one and whether a paste
    /// keeps the ones it carries.
    fn keeps_newlines(self) -> bool {
        matches!(self, EditMode::Multiline { .. })
    }
}

/// A single-line input field bundling text with its caret, an optional length
/// limit and an optional boxed decoration.
#[derive(Debug, Clone, Default)]
pub struct InputField {
    /// The current text.
    pub text: String,
    /// The caret and selection over `text`.
    pub cursor: TextCursor,
    max_len: Option<usize>,
    decor: Option<chrome::BoxDecor>,
}

impl InputField {
    /// Creates a field pre-filled with `initial`, caret at the end.
    ///
    /// # Examples
    ///
    /// ```
    /// use ratada::chrome::BoxDecor;
    /// use ratada::input::InputField;
    ///
    /// let field = InputField::new("hello")
    ///     .max_len(20)
    ///     .boxed(BoxDecor::new().caption("Name"));
    /// assert_eq!(field.value(), "hello");
    /// ```
    pub fn new(initial: &str) -> Self {
        Self {
            text: initial.to_string(),
            cursor: TextCursor::at_end(initial),
            ..Self::default()
        }
    }

    /// Limits the field to `max` characters (enforced on typing and paste) and
    /// feeds the `n/max` badge in the boxed variant.
    #[must_use]
    pub fn max_len(mut self, max: usize) -> Self {
        self.max_len = Some(max);
        self
    }

    /// Draws the field inside a rounded box with the given caption/badge (see
    /// [`chrome::BoxDecor`]); omit it for a plain field.
    #[must_use]
    pub fn boxed(mut self, decor: chrome::BoxDecor) -> Self {
        self.decor = Some(decor);
        self
    }

    /// Handles one editing key; returns whether it was consumed.
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        apply_edit_key(
            &mut self.text,
            &mut self.cursor,
            key,
            EditMode::SingleLine,
            self.max_len,
        )
    }

    /// Inserts pasted `text` at the caret, replacing any active selection.
    ///
    /// Control characters (including newlines) are stripped so the field stays
    /// on one line, honoring the length limit. This routes a bracketed paste;
    /// `Ctrl+V` goes through [`Self::handle_key`].
    pub fn paste(&mut self, text: &str) {
        paste_text(
            &mut self.text,
            &mut self.cursor,
            EditMode::SingleLine,
            self.max_len,
            text,
        );
    }

    /// The current text.
    pub fn value(&self) -> &str {
        &self.text
    }

    /// Renders the field as a single horizontally scrolling, background-filled
    /// line for embedding into a larger layout. `focused` picks the active
    /// background and shows the block caret.
    pub fn render_line(
        &self,
        palette: &Palette,
        width: usize,
        focused: bool,
    ) -> Line<'static> {
        render_line(&self.text, &self.cursor, palette, width, focused)
    }

    /// Like [`Self::render_line`], but without the field background: just the
    /// text and its block caret, on whatever surface the line sits on. For
    /// filter and search lines that read as plain text rather than a field.
    ///
    /// The caret sits at the field's real position; the text scrolls to keep it
    /// inside `width` display columns, marking a scrolled-off head with `…`.
    ///
    /// Reach for [`scrolled_line_spans`] to paint a line this field does not
    /// own - it marks *both* clipped ends and takes the caret as an argument.
    pub fn caret_spans(
        &self,
        palette: &Palette,
        width: usize,
    ) -> Vec<Span<'static>> {
        caret_spans(&self.text, &self.cursor, palette, LineView::flat(width)).0
    }

    /// Renders the field into `area`: a filled background (active tint when
    /// `focused`) plus the text line, wrapped in a box when decorated via
    /// [`Self::boxed`].
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
        let line =
            self.render_line(&skin.palette, inner.width as usize, focused);
        frame.render_widget(Paragraph::new(line), inner);
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

/// Renders `text` as a single line that scrolls horizontally to keep the caret
/// visible, filled with the input background (active tint when `focused`), with
/// a block caret (only when `focused`) and selection highlight. The line is
/// padded to `width` so the field reads as a solid background strip.
pub(crate) fn render_line(
    text: &str,
    cursor: &TextCursor,
    palette: &Palette,
    width: usize,
    focused: bool,
) -> Line<'static> {
    let width = width.max(1);
    let base_bg = if focused {
        palette.input_bg_active
    } else {
        palette.input_bg
    };
    let base = style::bg(base_bg);
    let view = LineView::field(width, base, focused);
    let (mut spans, used) = caret_spans(text, cursor, palette, view);
    if used < width {
        spans.push(Span::styled(" ".repeat(width - used), base));
    }
    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyModifiers};
    use ratatui::style::{Modifier, Style};
    use unicode_width::UnicodeWidthStr;

    use super::*;
    use crate::theme::{ColorOverrides, ThemeRegistry};

    fn press(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::CONTROL)
    }

    /// `AltGr`, as crossterm reports it on a German keyboard.
    fn altgr(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::CONTROL | KeyModifiers::ALT)
    }

    fn shift(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::SHIFT)
    }

    /// Applies one key in the given mode and returns the new `(text, cursor)`.
    fn edit(
        text: &str,
        cursor: TextCursor,
        key: KeyEvent,
        mode: EditMode,
    ) -> (String, TextCursor) {
        let mut text = text.to_string();
        let mut cursor = cursor;
        apply_edit_key(&mut text, &mut cursor, key, mode, None);
        (text, cursor)
    }

    #[test]
    fn ctrl_u_and_ctrl_k_delete_before_and_after_the_caret() {
        let single = EditMode::SingleLine;
        let (text, cursor) = edit(
            "abcdef",
            TextCursor::at(3),
            ctrl(KeyCode::Char('u')),
            single,
        );
        assert_eq!((text.as_str(), cursor.pos), ("def", 0));
        let (text, cursor) = edit(
            "abcdef",
            TextCursor::at(3),
            ctrl(KeyCode::Char('k')),
            single,
        );
        assert_eq!((text.as_str(), cursor.pos), ("abc", 3));
        // Ctrl+U at the start is a no-op.
        let (text, cursor) =
            edit("abc", TextCursor::at(0), ctrl(KeyCode::Char('u')), single);
        assert_eq!((text.as_str(), cursor.pos), ("abc", 0));
    }

    #[test]
    fn ctrl_u_and_ctrl_k_act_on_the_display_line_when_multiline() {
        // "alpha beta gamma" word-wraps to "alpha beta" / "gamma" at width 11.
        let mode = EditMode::Multiline {
            width: 11,
            height: 4,
        };
        let caret = TextCursor::at(13); // inside "gamma"
        let (text, cursor) =
            edit("alpha beta gamma", caret, ctrl(KeyCode::Char('k')), mode);
        assert_eq!((text.as_str(), cursor.pos), ("alpha beta ga", 13));
        let (text, cursor) =
            edit("alpha beta gamma", caret, ctrl(KeyCode::Char('u')), mode);
        assert_eq!((text.as_str(), cursor.pos), ("alpha beta mma", 11));
    }

    #[test]
    fn multiline_home_end_and_vertical_move_on_display_lines() {
        let mode = EditMode::Multiline {
            width: 11,
            height: 4,
        };
        let text = "alpha beta gamma";
        let caret = TextCursor::at(13); // line 1, column 2
        assert_eq!(edit(text, caret, press(KeyCode::Home), mode).1.pos, 11);
        assert_eq!(edit(text, caret, press(KeyCode::End), mode).1.pos, 16);
        // Up keeps the column, landing on the first display line.
        assert_eq!(edit(text, caret, press(KeyCode::Up), mode).1.pos, 2);
        // Shift+Up extends instead of moving.
        let (_, cursor) = edit(text, caret, shift(KeyCode::Up), mode);
        assert_eq!(cursor.selection(), Some((2, 13)));
    }

    #[test]
    fn page_keys_move_by_the_viewport_height_of_display_rows() {
        // Six one-char-wide rows via explicit newlines; height 2 is the page.
        let mode = EditMode::Multiline {
            width: 10,
            height: 2,
        };
        let text = "l0\nl1\nl2\nl3\nl4\nl5"; // rows start at 0,3,6,9,12,15
        // PageDown moves two rows down, keeping the column.
        assert_eq!(
            edit(text, TextCursor::at(1), press(KeyCode::PageDown), mode)
                .1
                .pos,
            7
        );
        // PageUp moves two rows up, keeping the column.
        assert_eq!(
            edit(text, TextCursor::at(16), press(KeyCode::PageUp), mode)
                .1
                .pos,
            10
        );
    }

    #[test]
    fn page_keys_clamp_at_the_first_and_last_row() {
        let mode = EditMode::Multiline {
            width: 10,
            height: 4,
        };
        let text = "l0\nl1\nl2"; // rows start at 0,3,6
        // PageUp near the top lands on row 0, not a no-op.
        assert_eq!(
            edit(text, TextCursor::at(6), press(KeyCode::PageUp), mode)
                .1
                .pos,
            0
        );
        // PageDown past the end lands on the last row.
        assert_eq!(
            edit(text, TextCursor::at(0), press(KeyCode::PageDown), mode)
                .1
                .pos,
            6
        );
    }

    #[test]
    fn shift_page_down_extends_the_selection() {
        let mode = EditMode::Multiline {
            width: 10,
            height: 2,
        };
        let text = "l0\nl1\nl2\nl3";
        let (_, cursor) =
            edit(text, TextCursor::at(0), shift(KeyCode::PageDown), mode);
        assert_eq!(cursor.selection(), Some((0, 6)));
    }

    #[test]
    fn enter_inserts_a_newline_only_in_multiline() {
        let (text, _) = edit(
            "ab",
            TextCursor::at(2),
            press(KeyCode::Enter),
            EditMode::Multiline {
                width: 8,
                height: 4,
            },
        );
        assert_eq!(text.as_str(), "ab\n");
        // In a single line the caller owns `Enter`, so it is not consumed.
        let mut text = String::from("ab");
        let mut cursor = TextCursor::at(2);
        let consumed = apply_edit_key(
            &mut text,
            &mut cursor,
            press(KeyCode::Enter),
            EditMode::SingleLine,
            None,
        );
        assert!(!consumed);
        assert_eq!(text.as_str(), "ab");
    }

    #[test]
    fn replace_selection_swaps_the_range_and_clears_the_anchor() {
        let mut text = String::from("abcd");
        let mut cursor = TextCursor {
            pos: 3,
            anchor: Some(1),
        };
        replace_selection(&mut text, &mut cursor, "X");
        assert_eq!((text.as_str(), cursor.pos), ("aXd", 2));
        assert_eq!(cursor.selection(), None);
    }

    #[test]
    fn selected_text_reads_the_ordered_range() {
        let cursor = TextCursor {
            pos: 13,
            anchor: Some(6),
        };
        assert_eq!(
            selected_text("alpha beta gamma", &cursor).as_deref(),
            Some("beta ga"),
        );
        assert_eq!(selected_text("abc", &TextCursor::at(1)), None);
    }

    #[test]
    fn intersect_clips_to_the_window() {
        assert_eq!(intersect(0, 2, 5, 9), None);
        assert_eq!(intersect(10, 14, 5, 9), None);
        assert_eq!(intersect(3, 7, 5, 9), Some((5, 7)));
        assert_eq!(intersect(7, 12, 5, 9), Some((7, 9)));
    }

    #[test]
    fn line_spans_paint_the_caret_and_selection_distinctly() {
        let palette = palette();
        let paint = LinePaint {
            caret: LineCaret {
                cursor: Some(2),
                selection: Some((1, 3)),
            },
            ..LinePaint::default()
        };
        let spans = line_spans("abcd", paint, &palette);
        // The caret cell (col 2) carries the cursor bg; the other selected cell
        // (col 1) carries the selection bg - two distinct runs.
        let cursor_bg = Some(style::to_ratatui(palette.cursor));
        let selection_bg = Some(style::to_ratatui(palette.selection));
        let caret_run = spans.iter().find(|span| span.style.bg == cursor_bg);
        let selected = spans.iter().find(|span| span.style.bg == selection_bg);
        assert_eq!(caret_run.map(|span| span.content.as_ref()), Some("c"));
        assert_eq!(selected.map(|span| span.content.as_ref()), Some("b"));
    }

    #[test]
    fn line_spans_patch_a_per_character_overlay_under_the_caret() {
        let palette = palette();
        let bold = Style::default().add_modifier(Modifier::BOLD);
        let overlay = [bold, bold, Style::default(), Style::default()];
        let paint = LinePaint {
            caret: LineCaret {
                cursor: Some(0),
                selection: None,
            },
            content: Some(&overlay),
            ..LinePaint::default()
        };
        let spans = line_spans("abcd", paint, &palette);
        // The caret cell keeps the overlay's bold *and* takes the cursor bg.
        let caret = &spans[0];
        assert_eq!(caret.content.as_ref(), "a");
        assert!(caret.style.add_modifier.contains(Modifier::BOLD));
        assert_eq!(caret.style.bg, Some(style::to_ratatui(palette.cursor)));
        // The un-overlaid tail stays unstyled and coalesces into one run.
        assert_eq!(spans.last().map(|s| s.content.as_ref()), Some("cd"));
    }

    /// Every text/width/caret/marker combination worth worrying about.
    fn line_views(width: usize) -> [LineView<'static>; 4] {
        let base = Style::default();
        let of = |caret, head_marker, tail_marker| LineView {
            width,
            base,
            content: None,
            caret,
            head_marker,
            tail_marker,
        };
        [
            of(true, false, false),
            of(true, true, false),
            of(true, true, true),
            of(false, true, true),
        ]
    }

    #[test]
    fn a_rendered_line_never_exceeds_its_width() {
        // A marker costs a column, so a narrow window must drop it rather than
        // overflow: at width 2 there is no room for a head *and* a tail `…`.
        let palette = palette();
        let texts = ["", "a", "abcdefghij", "世界a", "hello world foo"];
        for text in texts {
            for width in 1..=14 {
                for pos in 0..=text.chars().count() {
                    for view in line_views(width) {
                        let cursor = TextCursor::at(pos);
                        let (spans, used) =
                            caret_spans(text, &cursor, &palette, view);
                        let cols = columns(&spans);
                        assert!(
                            cols <= width,
                            "{text:?} w={width} pos={pos} drew {cols}",
                        );
                        assert_eq!(cols, used, "{text:?} w={width} pos={pos}");
                    }
                }
            }
        }
    }

    #[test]
    fn a_focused_caret_is_always_inside_the_window() {
        let palette = palette();
        let cursor_bg = Some(style::to_ratatui(palette.cursor));
        for text in ["abcdefghij", "a世b界c", "hello world"] {
            for width in 2..=14 {
                for pos in 0..=text.chars().count() {
                    let view =
                        LineView::embedded(width, Style::default(), None);
                    let cursor = TextCursor::at(pos);
                    let (spans, _) = caret_spans(text, &cursor, &palette, view);
                    assert!(
                        spans.iter().any(|span| span.style.bg == cursor_bg),
                        "caret lost: {text:?} w={width} pos={pos}",
                    );
                }
            }
        }
    }

    #[test]
    fn scrolled_line_spans_mark_both_clipped_ends() {
        let palette = palette();
        // A caret in the middle of a long value clips head *and* tail.
        let paint = ScrollPaint {
            cursor: TextCursor::at(5),
            width: 5,
            base: Style::default(),
            content: None,
        };
        let spans = scrolled_line_spans("abcdefghij", paint, &palette);
        let text: String =
            spans.iter().map(|span| span.content.as_ref()).collect();
        assert!(text.starts_with(SCROLL_MARKER), "{text:?}");
        assert!(text.ends_with(SCROLL_MARKER), "{text:?}");
        assert_eq!(columns(&spans), 5);
    }

    #[test]
    fn scrolled_line_spans_paint_only_the_visible_selection_slice() {
        let palette = palette();
        let paint = ScrollPaint {
            cursor: TextCursor {
                pos: 9,
                anchor: Some(0),
            },
            width: 5,
            base: Style::default(),
            content: None,
        };
        let spans = scrolled_line_spans("abcdefghij", paint, &palette);
        let selection_bg = Some(style::to_ratatui(palette.selection));
        let selected: String = spans
            .iter()
            .filter(|span| span.style.bg == selection_bg)
            .map(|span| span.content.as_ref())
            .collect();
        // Only a slice is visible, never the whole 10-char selection.
        assert!(!selected.is_empty());
        assert!(selected.chars().count() < 10, "{selected:?}");
        assert!(columns(&spans) <= 5);
    }

    #[test]
    fn query_spans_at_honours_a_mid_string_caret_and_selection() {
        let palette = palette();
        let cursor = TextCursor {
            pos: 1,
            anchor: Some(3),
        };
        let spans = query_spans_at("abcd", cursor, &palette, 20);
        let cursor_bg = Some(style::to_ratatui(palette.cursor));
        let caret = spans.iter().find(|span| span.style.bg == cursor_bg);
        assert_eq!(caret.map(|span| span.content.as_ref()), Some("b"));
        let selection_bg = Some(style::to_ratatui(palette.selection));
        let selected: String = spans
            .iter()
            .filter(|span| span.style.bg == selection_bg)
            .map(|span| span.content.as_ref())
            .collect();
        assert_eq!(selected, "c");
    }

    #[test]
    fn is_command_requires_control_without_alt() {
        assert!(is_command(ctrl(KeyCode::Char('s'))));
        assert!(!is_command(altgr(KeyCode::Char('\\'))));
        assert!(!is_command(press(KeyCode::Char('a'))));
    }

    /// A plain letter, whatever its case, but nothing carrying `Control` or
    /// `Alt`. The `Alt`-only case is the one that matters: every other
    /// rejection here is also caught by the `Control` half, so dropping the
    /// `Alt` half of the check would go unnoticed without it.
    #[test]
    fn is_bare_character_accepts_only_an_unmodified_letter() {
        assert!(is_bare_character(press(KeyCode::Char('y'))));
        assert!(is_bare_character(KeyEvent::new(
            KeyCode::Char('Y'),
            KeyModifiers::SHIFT,
        )));

        assert!(!is_bare_character(ctrl(KeyCode::Char('y'))));
        assert!(!is_bare_character(altgr(KeyCode::Char('@'))));
        assert!(!is_bare_character(KeyEvent::new(
            KeyCode::Char('y'),
            KeyModifiers::ALT,
        )));
        // Not a character at all.
        assert!(!is_bare_character(press(KeyCode::Enter)));
    }

    #[test]
    fn altgr_char_is_typed_not_swallowed() {
        // German keyboards emit `\` as AltGr (Control + Alt); it must insert.
        let mut text = String::from("C:");
        let mut cursor = TextCursor::at_end(&text);
        assert!(apply_edit_key(
            &mut text,
            &mut cursor,
            altgr(KeyCode::Char('\\')),
            EditMode::SingleLine,
            None,
        ));
        assert_eq!((text.as_str(), cursor.pos), ("C:\\", 3));

        // A genuine Ctrl chord still does not type a character.
        let mut text = String::from("C:");
        let mut cursor = TextCursor::at_end(&text);
        apply_edit_key(
            &mut text,
            &mut cursor,
            ctrl(KeyCode::Char('s')),
            EditMode::SingleLine,
            None,
        );
        assert_eq!(text.as_str(), "C:");
    }

    fn palette() -> Palette {
        Palette::resolve(
            ThemeRegistry::builtin().resolve("default"),
            &ColorOverrides::default(),
        )
    }

    /// The display columns the spans occupy.
    fn columns(spans: &[Span<'_>]) -> usize {
        spans.iter().map(|span| span.content.width()).sum()
    }

    /// Whether `span` carries the block-caret background.
    fn is_caret(span: &Span<'_>, palette: &Palette) -> bool {
        span.style.bg == Some(style::to_ratatui(palette.cursor))
    }

    #[test]
    fn typing_inserts_at_the_caret() {
        let mut field = InputField::default();
        field.handle_key(press(KeyCode::Char('h')));
        field.handle_key(press(KeyCode::Char('i')));
        assert_eq!(field.value(), "hi");
        assert_eq!(field.cursor.pos, 2);
    }

    #[test]
    fn backspace_removes_the_previous_char() {
        let mut field = InputField::new("ab");
        field.handle_key(press(KeyCode::Backspace));
        assert_eq!(field.value(), "a");
    }

    #[test]
    fn shift_left_selects_and_typing_replaces() {
        let mut field = InputField::new("abc");
        field.handle_key(KeyEvent::new(KeyCode::Left, KeyModifiers::SHIFT));
        assert_eq!(field.cursor.selection(), Some((2, 3)));
        field.handle_key(press(KeyCode::Char('X')));
        assert_eq!(field.value(), "abX");
    }

    #[test]
    fn max_len_blocks_typing_past_the_limit() {
        let mut field = InputField::new("ab").max_len(3);
        field.handle_key(press(KeyCode::Char('c')));
        assert_eq!(field.value(), "abc");
        // The fourth character is rejected.
        field.handle_key(press(KeyCode::Char('d')));
        assert_eq!(field.value(), "abc");
    }

    #[test]
    fn render_line_windows_by_display_width_for_wide_chars() {
        let palette = palette();
        // Four width-2 glyphs (8 columns) into a 5-column field.
        let text = "世界世界";
        let cursor = TextCursor::at_end(text);
        let line = render_line(text, &cursor, &palette, 5, true);
        // The rendered line is exactly `width` columns: wide glyphs never
        // overflow the field and the remainder is padded.
        assert_eq!(columns(&line.spans), 5);
    }

    #[test]
    fn render_line_pads_to_width_and_hides_the_caret_when_unfocused() {
        let palette = palette();
        let line =
            render_line("ab", &TextCursor::at_end("ab"), &palette, 8, false);
        assert_eq!(columns(&line.spans), 8);
        assert!(!line.spans.iter().any(|span| is_caret(span, &palette)));
    }

    #[test]
    fn query_spans_put_the_caret_past_the_last_char() {
        let palette = palette();
        let spans = query_spans("src", &palette, 20);
        // Three chars plus the caret; a flat line pads nothing.
        assert_eq!(columns(&spans), 4);
        let caret = spans.last().expect("caret span");
        assert_eq!(caret.content, " ");
        assert!(is_caret(caret, &palette));
    }

    #[test]
    fn caret_spans_mark_the_char_under_the_caret() {
        let mut field = InputField::new("abc");
        field.cursor.pos = 1;
        let palette = palette();
        let spans = field.caret_spans(&palette, 20);
        assert_eq!(spans.len(), 3, "no trailing caret space mid-text");
        assert_eq!(spans[1].content, "b");
        assert!(is_caret(&spans[1], &palette));
        assert!(!is_caret(&spans[0], &palette));
    }

    #[test]
    fn a_flat_line_scrolls_and_marks_the_scrolled_off_head() {
        let palette = palette();
        // Ten columns of text into six: the tail plus the caret stay visible.
        let spans = query_spans("abcdefghij", &palette, 6);
        assert_eq!(columns(&spans), 6);
        assert_eq!(spans[0].content, SCROLL_MARKER);
        // The marker costs a column, so only four chars fit before the caret.
        let text: String = spans[1..spans.len() - 1]
            .iter()
            .map(|s| &*s.content)
            .collect();
        assert_eq!(text, "ghij");
        assert!(is_caret(spans.last().expect("caret span"), &palette));
    }

    #[test]
    fn a_flat_line_that_fits_carries_no_marker() {
        let palette = palette();
        let spans = query_spans("abc", &palette, 10);
        assert_ne!(spans[0].content, SCROLL_MARKER);
    }

    #[test]
    fn a_flat_line_never_splits_a_wide_glyph_at_the_edge() {
        let palette = palette();
        // Wide glyphs are two columns each; an odd width leaves one unused.
        let spans = query_spans("世界世界", &palette, 5);
        assert!(columns(&spans) <= 5);
        assert!(spans.iter().all(|span| span.content != "\u{fffd}"));
    }

    #[test]
    fn a_flat_line_survives_an_empty_text_and_a_zero_width() {
        let palette = palette();
        let spans = query_spans("", &palette, 0);
        assert_eq!(spans.len(), 1);
        assert!(is_caret(&spans[0], &palette));
    }

    #[test]
    fn paste_into_a_single_line_field_drops_control_and_newlines() {
        let mut text = String::new();
        let mut cursor = TextCursor::default();
        let mode = EditMode::SingleLine;
        paste_text(&mut text, &mut cursor, mode, None, "a\nb\tc");
        assert_eq!(text, "abc");
        assert_eq!(cursor.pos, 3);
    }

    #[test]
    fn paste_into_a_multiline_field_keeps_newlines() {
        let mut text = String::new();
        let mut cursor = TextCursor::default();
        let mode = EditMode::Multiline {
            width: 10,
            height: 4,
        };
        paste_text(&mut text, &mut cursor, mode, None, "a\nb\tc");
        assert_eq!(text, "a\nbc");
    }

    #[test]
    fn paste_caps_the_result_at_max_len() {
        let mut text = String::new();
        let mut cursor = TextCursor::default();
        let mode = EditMode::SingleLine;
        paste_text(&mut text, &mut cursor, mode, Some(3), "abcdef");
        assert_eq!(text, "abc");
    }

    #[test]
    fn paste_replaces_the_active_selection() {
        let mut text = String::from("hello");
        let mut cursor = TextCursor {
            pos: 5,
            anchor: Some(0),
        };
        let mode = EditMode::SingleLine;
        paste_text(&mut text, &mut cursor, mode, None, "hi");
        assert_eq!((text.as_str(), cursor.pos, cursor.anchor), ("hi", 2, None));
    }

    /// `Wrapped` is the mode for a value that is one logical line but too long
    /// to show on one row. Everything below is the contract that separates it
    /// from the two modes it sits between.
    mod wrapped {
        use super::*;

        /// Wraps at 6 columns, 3 rows tall.
        const MODE: EditMode = EditMode::Wrapped {
            width: 6,
            height: 3,
        };

        fn press(
            text: &mut String,
            cursor: &mut TextCursor,
            code: KeyCode,
        ) -> bool {
            apply_edit_key(text, cursor, KeyEvent::from(code), MODE, None)
        }

        /// The whole point of the mode: `Enter` belongs to the caller, which
        /// uses it to submit. Consuming it here would insert a newline into a
        /// value that must never hold one.
        #[test]
        fn enter_is_left_to_the_caller() {
            let mut text = "ab".to_string();
            let mut cursor = TextCursor::at(2);

            assert!(!press(&mut text, &mut cursor, KeyCode::Enter));
            assert_eq!(text, "ab", "the buffer must be untouched");
        }

        /// A multi-line paste collapses, as it does in a single-line field.
        #[test]
        fn a_paste_drops_its_line_breaks() {
            let mut text = String::new();
            let mut cursor = TextCursor::at(0);

            paste_text(&mut text, &mut cursor, MODE, None, "one\ntwo");
            assert!(!text.contains('\n'), "got: {text:?}");
            assert_eq!(text, "onetwo");
        }

        /// Navigation follows the display, not the logical line: that is what
        /// it borrows from `Multiline`.
        #[test]
        fn home_and_end_act_on_the_display_line() {
            // Wrapped at 6 columns: "aaa" / "bbb".
            let text = "aaa bbb".to_string();
            let mut cursor = TextCursor::at(5);
            let mut buffer = text.clone();

            press(&mut buffer, &mut cursor, KeyCode::Home);
            assert_eq!(cursor.pos, 4, "the second display line starts at 4");

            press(&mut buffer, &mut cursor, KeyCode::End);
            assert_eq!(cursor.pos, 7, "and ends at the value's end");
        }

        #[test]
        fn up_and_down_walk_the_wrapped_rows() {
            let mut text = "aaa bbb".to_string();
            let mut cursor = TextCursor::at(5);

            assert!(press(&mut text, &mut cursor, KeyCode::Up));
            assert!(
                cursor.pos < 4,
                "moved onto the first row, got {}",
                cursor.pos
            );

            assert!(press(&mut text, &mut cursor, KeyCode::Down));
            assert!(cursor.pos >= 4, "back onto the second row");
        }

        /// Typing, deleting and selecting are unchanged - the mode only alters
        /// the geometry and the newline rule.
        #[test]
        fn ordinary_editing_still_works() {
            let mut text = String::new();
            let mut cursor = TextCursor::at(0);

            for ch in "abc".chars() {
                press(&mut text, &mut cursor, KeyCode::Char(ch));
            }
            assert_eq!(text, "abc");

            press(&mut text, &mut cursor, KeyCode::Backspace);
            assert_eq!(text, "ab");
        }
    }
}

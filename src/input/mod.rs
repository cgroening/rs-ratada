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

    /// The char range a paste or typed text would replace: the selection, or
    /// an empty range at the caret. Lets a caller look at the characters
    /// around the insertion point before pasting.
    ///
    /// # Examples
    ///
    /// ```
    /// use ratada::input::InputField;
    ///
    /// let field = InputField::new("huk");
    /// assert_eq!(field.insertion_range(), (3, 3));
    /// ```
    #[must_use]
    pub fn insertion_range(&self) -> (usize, usize) {
        let len = self.text.chars().count();
        let pos = self.cursor.pos.min(len);
        self.cursor
            .selection()
            .map_or((pos, pos), |(start, end)| (start.min(len), end.min(len)))
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
mod tests;

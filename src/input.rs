//! Shared single-line text editing: one caret with an optional selection
//! anchor over a `String`. This is the single source of editing behaviour for
//! every text field, so shortcuts stay consistent.
//!
//! The editor handles only editing keys. A field's control keys (`Esc`, a
//! confirming `Enter`, other chords) belong to the caller and must be handled
//! before delegating here.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use unicode_width::UnicodeWidthChar;

use super::{chrome, clipboard, style};
use crate::theme::{Palette, Skin};

/// A text caret with an optional selection anchor, both as char indices.
#[derive(Debug, Clone, Default)]
pub struct TextCursor {
    /// The caret position, as a char index.
    pub pos: usize,
    /// The selection anchor (a char index), or `None` when nothing is selected.
    pub anchor: Option<usize>,
}

impl TextCursor {
    /// A caret placed at the end of `text`.
    pub fn at_end(text: &str) -> Self {
        Self {
            pos: text.chars().count(),
            anchor: None,
        }
    }

    /// The current selection as an ordered `(start, end)` range, if any.
    pub fn selection(&self) -> Option<(usize, usize)> {
        let anchor = self.anchor?;
        let (start, end) = if anchor <= self.pos {
            (anchor, self.pos)
        } else {
            (self.pos, anchor)
        };
        (start != end).then_some((start, end))
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
        apply_edit_key(&mut self.text, &mut self.cursor, key, self.max_len)
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

/// Applies one editing key to `text`/`cursor`, respecting an optional
/// `max_len`. Returns whether it was consumed.
pub(crate) fn apply_edit_key(
    text: &mut String,
    cursor: &mut TextCursor,
    key: KeyEvent,
    max_len: Option<usize>,
) -> bool {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);
    let mut chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    cursor.pos = cursor.pos.min(len);

    let consumed = match key.code {
        KeyCode::Left => {
            move_caret(cursor, cursor.pos.saturating_sub(1), shift);
            true
        }
        KeyCode::Right => {
            move_caret(cursor, (cursor.pos + 1).min(len), shift);
            true
        }
        KeyCode::Home => {
            move_caret(cursor, 0, shift);
            true
        }
        KeyCode::End => {
            move_caret(cursor, len, shift);
            true
        }
        KeyCode::Char('a') if ctrl => {
            cursor.anchor = Some(0);
            cursor.pos = len;
            true
        }
        KeyCode::Char('u') if ctrl => {
            delete_range(&mut chars, cursor, 0, cursor.pos);
            true
        }
        KeyCode::Char('k') if ctrl => {
            delete_range(&mut chars, cursor, cursor.pos, len);
            true
        }
        KeyCode::Char('c') if ctrl => {
            copy_selection(&chars, cursor);
            true
        }
        KeyCode::Char('x') if ctrl => {
            copy_selection(&chars, cursor);
            delete_selection(&mut chars, cursor);
            true
        }
        KeyCode::Char('v') if ctrl => {
            paste(&mut chars, cursor, max_len);
            true
        }
        KeyCode::Backspace => {
            backspace(&mut chars, cursor);
            true
        }
        KeyCode::Delete => {
            delete_forward(&mut chars, cursor);
            true
        }
        KeyCode::Char(ch) if !ctrl => {
            insert_char(&mut chars, cursor, ch, max_len);
            true
        }
        _ => false,
    };

    if consumed {
        *text = chars.into_iter().collect();
    }
    consumed
}

/// The marker standing in for the text scrolled off the left edge.
const SCROLL_MARKER: &str = "\u{2026}";

/// How one caret-carrying line is drawn: the visible `width` in display
/// columns, the style every unhighlighted char takes, whether the block caret
/// shows, and whether a scrolled-off head is marked with [`SCROLL_MARKER`].
#[derive(Debug, Clone, Copy)]
struct LineView {
    width: usize,
    base: Style,
    caret: bool,
    marker: bool,
}

impl LineView {
    /// A field line: filled with `base`, caret only while focused. It signals
    /// its overflow through the filled strip, so it carries no marker.
    fn field(width: usize, base: Style, focused: bool) -> Self {
        Self {
            width,
            base,
            caret: focused,
            marker: false,
        }
    }

    /// A flat line on whatever surface it sits on: no fill, always a caret, and
    /// a marker once the head scrolls out of view.
    fn flat(width: usize) -> Self {
        Self {
            width,
            base: Style::default(),
            caret: true,
            marker: true,
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

/// A query line: `text` followed by the block caret, for the filter and search
/// lines that keep no caret position of their own. Scrolls horizontally so the
/// caret stays visible, marking a scrolled-off head with `…`.
///
/// Prepend the line's own label; the returned spans cover only the text and its
/// caret, and paint no background of their own.
///
/// # Examples
///
/// ```
/// use ratada::input::query_spans;
/// use ratada::theme::{ColorOverrides, Palette, ThemeRegistry};
///
/// let base = ThemeRegistry::builtin().resolve("default");
/// let palette = Palette::resolve(base, &ColorOverrides::default());
/// // The trailing span is the caret sitting past the last char.
/// let spans = query_spans("src", &palette, 20);
/// assert_eq!(spans.len(), 4);
/// ```
pub fn query_spans(
    text: &str,
    palette: &Palette,
    width: usize,
) -> Vec<Span<'static>> {
    let cursor = TextCursor::at_end(text);
    caret_spans(text, &cursor, palette, LineView::flat(width)).0
}

/// The visible slice of `text`, scrolled so the caret stays inside `view.width`
/// display columns. Returns the spans and the columns they occupy. The single
/// source of caret rendering for both the field and the flat line.
fn caret_spans(
    text: &str,
    cursor: &TextCursor,
    palette: &Palette,
    view: LineView,
) -> (Vec<Span<'static>>, usize) {
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let pos = cursor.pos.min(len);
    let width = view.width.max(1);
    let selection = cursor.selection();
    let cursor_style = style::cursor(palette).fg(Color::Black);
    let selection_style = style::bg(palette.selection);

    // Display columns before each char index, so scrolling and the visible
    // window are measured in columns (wide glyphs count as two), not chars.
    let widths: Vec<usize> =
        chars.iter().map(|ch| ch.width().unwrap_or(0)).collect();
    let mut column_at = vec![0usize; len + 1];
    for index in 0..len {
        column_at[index + 1] = column_at[index] + widths[index];
    }
    let caret_column = column_at[pos];

    // The marker eats a column, which can push the window further right, so the
    // start is resolved once to learn whether the head scrolls off at all.
    let mut start = scroll_start(&column_at, caret_column, width);
    let marked = view.marker && start > 0 && width > 1;
    if marked {
        start = scroll_start(&column_at, caret_column, width - 1);
    }

    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut used = 0usize;
    if marked {
        spans.push(Span::styled(SCROLL_MARKER, style::secondary(palette)));
        used += 1;
    }
    let mut index = start;
    while index < len {
        let char_width = widths[index];
        if used + char_width > width {
            break;
        }
        let mut style = view.base;
        if let Some((from, to)) = selection
            && index >= from
            && index < to
        {
            style = selection_style;
        }
        if view.caret && index == pos {
            style = cursor_style;
        }
        spans.push(Span::styled(chars[index].to_string(), style));
        used += char_width;
        index += 1;
    }
    if view.caret && pos >= len && used < width {
        spans.push(Span::styled(" ".to_string(), cursor_style));
        used += 1;
    }
    (spans, used)
}

/// The first char index to draw so that `caret_column` stays inside a window of
/// `width` display columns.
fn scroll_start(
    column_at: &[usize],
    caret_column: usize,
    width: usize,
) -> usize {
    let last = column_at.len() - 1;
    let window = caret_column.saturating_sub(width.max(1) - 1);
    (0..=last)
        .find(|&index| column_at[index] >= window)
        .unwrap_or(last)
}

/// Moves the caret to `to`, extending the selection when `extend` is set (an
/// anchor is dropped there on the first extend) or clearing it otherwise.
pub(crate) fn move_caret(cursor: &mut TextCursor, to: usize, extend: bool) {
    if extend {
        cursor.anchor.get_or_insert(cursor.pos);
    } else {
        cursor.anchor = None;
    }
    cursor.pos = to;
}

/// Inserts `ch` at the caret, first replacing any selection and respecting
/// `max_len`.
pub(crate) fn insert_char(
    chars: &mut Vec<char>,
    cursor: &mut TextCursor,
    ch: char,
    max_len: Option<usize>,
) {
    delete_selection(chars, cursor);
    if max_len.is_some_and(|max| chars.len() >= max) {
        return;
    }
    chars.insert(cursor.pos, ch);
    cursor.pos += 1;
    cursor.anchor = None;
}

/// Deletes the selection, or the char before the caret when there is none.
pub(crate) fn backspace(chars: &mut Vec<char>, cursor: &mut TextCursor) {
    if delete_selection(chars, cursor) {
        return;
    }
    if cursor.pos > 0 {
        chars.remove(cursor.pos - 1);
        cursor.pos -= 1;
    }
}

/// Deletes the selection, or the char at the caret when there is none.
pub(crate) fn delete_forward(chars: &mut Vec<char>, cursor: &mut TextCursor) {
    if delete_selection(chars, cursor) {
        return;
    }
    if cursor.pos < chars.len() {
        chars.remove(cursor.pos);
    }
}

/// Deletes the current selection if any, returning whether one was removed.
pub(crate) fn delete_selection(
    chars: &mut Vec<char>,
    cursor: &mut TextCursor,
) -> bool {
    let Some((start, end)) = cursor.selection() else {
        return false;
    };
    chars.drain(start..end);
    cursor.pos = start;
    cursor.anchor = None;
    true
}

fn delete_range(
    chars: &mut Vec<char>,
    cursor: &mut TextCursor,
    start: usize,
    end: usize,
) {
    let start = start.min(chars.len());
    let end = end.min(chars.len());
    if start >= end {
        return;
    }
    chars.drain(start..end);
    cursor.pos = start;
    cursor.anchor = None;
}

/// Copies the current selection (if any) to the clipboard.
pub(crate) fn copy_selection(chars: &[char], cursor: &TextCursor) {
    if let Some((start, end)) = cursor.selection() {
        let text: String = chars[start..end].iter().collect();
        clipboard::copy(&text);
    }
}

/// Pastes clipboard text at the caret, replacing any selection and stripping
/// all control characters (single-line fields).
pub(crate) fn paste(
    chars: &mut Vec<char>,
    cursor: &mut TextCursor,
    max_len: Option<usize>,
) {
    paste_filtered(chars, cursor, max_len, |ch| !ch.is_control());
}

/// Like [`paste`], but keeps newlines so multi-line fields preserve the pasted
/// line breaks.
pub(crate) fn paste_multiline(
    chars: &mut Vec<char>,
    cursor: &mut TextCursor,
    max_len: Option<usize>,
) {
    paste_filtered(chars, cursor, max_len, |ch| ch == '\n' || !ch.is_control());
}

/// Shared paste: replaces any selection, then inserts the clipboard's
/// `keep`-passing chars at the caret, respecting `max_len`.
fn paste_filtered(
    chars: &mut Vec<char>,
    cursor: &mut TextCursor,
    max_len: Option<usize>,
    keep: impl Fn(char) -> bool,
) {
    let Some(text) = clipboard::paste() else {
        return;
    };
    delete_selection(chars, cursor);
    for ch in text.chars().filter(|&ch| keep(ch)) {
        if max_len.is_some_and(|max| chars.len() >= max) {
            break;
        }
        chars.insert(cursor.pos, ch);
        cursor.pos += 1;
    }
    cursor.anchor = None;
}

#[cfg(test)]
mod tests {
    use crossterm::event::KeyCode;
    use unicode_width::UnicodeWidthStr;

    use super::*;
    use crate::theme::{ColorOverrides, ThemeRegistry};

    fn press(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
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
}

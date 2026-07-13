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

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use unicode_width::UnicodeWidthChar;

use super::{chrome, clipboard, style, textarea};
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

/// Whether an input is a single line or a word-wrapped box of the given width.
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
}

/// The overlap of the half-open range `[s, e)` with the window `[lo, hi)`, or
/// `None` when they don't meet.
///
/// Maps a global selection into the visible window of a scrolled single line or
/// one wrapped display line.
///
/// # Examples
///
/// ```
/// use ratada::input::intersect;
///
/// assert_eq!(intersect(2, 8, 4, 6), Some((4, 6)));
/// assert_eq!(intersect(0, 3, 5, 9), None);
/// ```
#[must_use]
pub fn intersect(
    s: usize,
    e: usize,
    lo: usize,
    hi: usize,
) -> Option<(usize, usize)> {
    let start = s.max(lo);
    let end = e.min(hi);
    (start < end).then_some((start, end))
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

/// Whether `key` is a Ctrl **command** chord rather than a typed character.
///
/// A command requires Control *without* Alt: on many keyboards (e.g. German)
/// `AltGr` is reported as `Control + Alt` and produces real characters (`\`,
/// `@`, `[`, `]`, `|`, ...), so those must type, not be swallowed as a chord.
///
/// # Examples
///
/// ```
/// use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
/// use ratada::input::is_command;
///
/// let ctrl_s = KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL);
/// assert!(is_command(ctrl_s));
///
/// let alt_gr = KeyEvent::new(
///     KeyCode::Char('\\'),
///     KeyModifiers::CONTROL | KeyModifiers::ALT,
/// );
/// assert!(!is_command(alt_gr));
/// ```
#[must_use]
pub fn is_command(key: KeyEvent) -> bool {
    key.modifiers.contains(KeyModifiers::CONTROL)
        && !key.modifiers.contains(KeyModifiers::ALT)
}

/// Applies one editing key to `text`/`cursor`, returning whether it was
/// consumed (so the caller stops handling it). Steering keys the caller owns -
/// `Esc`, a confirming `Enter` in [`EditMode::SingleLine`], other chords - must
/// be handled before delegating here.
///
/// A `Shift`-modified motion key (arrows, `Home`/`End`) extends the selection;
/// the same key without `Shift` moves and clears it. `Ctrl+A` selects the whole
/// value, `Ctrl+U`/`Ctrl+K` delete from the line start to the caret / from the
/// caret to the line end, and `Ctrl+C`/`X`/`V` drive the clipboard. Typing,
/// `Backspace` and `Delete` replace an active selection.
///
/// In [`EditMode::Multiline`] the line-oriented keys act on the **display**
/// line (the wrap at `width`), `Up`/`Down` walk display lines,
/// `PageUp`/`PageDown` move by the viewport `height` in display rows (clamped at
/// the edges), `Enter` inserts a newline and a paste keeps its line breaks.
/// `max_len` caps the character count on typing and paste.
///
/// # Examples
///
/// ```
/// use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
/// use ratada::input::{EditMode, TextCursor, apply_edit_key};
///
/// let mut text = String::from("ab");
/// let mut cursor = TextCursor::at_end(&text);
/// let key = KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE);
/// assert!(apply_edit_key(
///     &mut text,
///     &mut cursor,
///     key,
///     EditMode::SingleLine,
///     None,
/// ));
/// assert_eq!(text, "a");
/// ```
pub fn apply_edit_key(
    text: &mut String,
    cursor: &mut TextCursor,
    key: KeyEvent,
    mode: EditMode,
    max_len: Option<usize>,
) -> bool {
    let ctrl = is_command(key);
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);
    let multiline = matches!(mode, EditMode::Multiline { .. });
    let mut chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    cursor.pos = cursor.pos.min(len);

    if let Some(target) = motion_target(text, cursor.pos, key, mode) {
        move_caret(cursor, target, shift);
        return true;
    }

    let consumed = match key.code {
        KeyCode::Char('a') if ctrl => {
            cursor.select_all(len);
            true
        }
        KeyCode::Char('u') if ctrl => {
            let start = line_start(text, cursor.pos, mode);
            delete_range(&mut chars, cursor, start, cursor.pos);
            true
        }
        KeyCode::Char('k') if ctrl => {
            let end = line_end(text, cursor.pos, mode);
            delete_range(&mut chars, cursor, cursor.pos, end);
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
            if multiline {
                paste_multiline(&mut chars, cursor, max_len);
            } else {
                paste(&mut chars, cursor, max_len);
            }
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
        KeyCode::Enter if multiline => {
            insert_char(&mut chars, cursor, '\n', max_len);
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

/// Handles the clipboard chords against `(text, cursor)`, returning whether the
/// key was one of them: `Ctrl+C` copies the selection, `Ctrl+X` cuts it and
/// `Ctrl+V` replaces the selection (or inserts at the caret) with the clipboard.
///
/// [`apply_edit_key`] already covers these; this is for hosts that dispatch the
/// clipboard separately from the rest of their editing keys. A paste strips
/// control characters, keeping newlines only in [`EditMode::Multiline`].
pub fn handle_clipboard(
    text: &mut String,
    cursor: &mut TextCursor,
    key: KeyEvent,
    mode: EditMode,
) -> bool {
    if !is_command(key) {
        return false;
    }
    if !matches!(key.code, KeyCode::Char('c' | 'x' | 'v')) {
        return false;
    }
    apply_edit_key(text, cursor, key, mode, None)
}

/// Replaces the active selection - or, with none, inserts at the caret - with
/// `s`, leaving the caret after the inserted text and no selection.
///
/// The seam every host mutation goes through, so no stale anchor outlives an
/// edit.
///
/// # Examples
///
/// ```
/// use ratada::input::{TextCursor, replace_selection};
///
/// let mut text = String::from("hello");
/// let mut cursor = TextCursor { pos: 5, anchor: Some(1) };
/// replace_selection(&mut text, &mut cursor, "i");
/// assert_eq!((text.as_str(), cursor.pos), ("hi", 2));
/// ```
pub fn replace_selection(text: &mut String, cursor: &mut TextCursor, s: &str) {
    let mut chars: Vec<char> = text.chars().collect();
    delete_selection(&mut chars, cursor);
    cursor.anchor = None;
    let mut at = cursor.pos.min(chars.len());
    for ch in s.chars() {
        chars.insert(at, ch);
        at += 1;
    }
    cursor.pos = at;
    *text = chars.into_iter().collect();
}

/// Inserts `to_insert` at the caret, replacing an active selection first.
///
/// An alias of [`replace_selection`] that reads right at an insertion site.
pub fn insert_str(text: &mut String, cursor: &mut TextCursor, to_insert: &str) {
    replace_selection(text, cursor, to_insert);
}

/// The selected substring, or `None` when nothing is selected.
#[must_use]
pub fn selected_text(text: &str, cursor: &TextCursor) -> Option<String> {
    let (start, end) = cursor.selection()?;
    let chars: Vec<char> = text.chars().collect();
    let end = end.min(chars.len());
    let start = start.min(end);
    Some(chars[start..end].iter().collect())
}

/// The motion target for a navigation key, or `None` when `key` is not one. A
/// vertical move that cannot go further returns the unchanged `pos`, so a plain
/// `Up`/`Down` still clears the selection.
fn motion_target(
    text: &str,
    pos: usize,
    key: KeyEvent,
    mode: EditMode,
) -> Option<usize> {
    let multiline = matches!(mode, EditMode::Multiline { .. });
    let target = match key.code {
        KeyCode::Left => pos.saturating_sub(1),
        KeyCode::Right => (pos + 1).min(text.chars().count()),
        KeyCode::Home => line_start(text, pos, mode),
        KeyCode::End => line_end(text, pos, mode),
        KeyCode::Up if multiline => display_line_target(text, pos, mode, -1),
        KeyCode::Down if multiline => display_line_target(text, pos, mode, 1),
        KeyCode::PageUp if multiline => {
            display_line_target(text, pos, mode, -page(mode))
        }
        KeyCode::PageDown if multiline => {
            display_line_target(text, pos, mode, page(mode))
        }
        _ => return None,
    };
    Some(target)
}

/// The page step for `PageUp`/`PageDown`: the viewport height in display rows
/// (at least one). A single line has no page, so it reports one row.
fn page(mode: EditMode) -> isize {
    match mode {
        EditMode::Multiline { height, .. } => height.max(1) as isize,
        EditMode::SingleLine => 1,
    }
}

/// The caret index for `Home`: the value's start, or the display line's start.
fn line_start(text: &str, pos: usize, mode: EditMode) -> usize {
    let EditMode::Multiline { width, .. } = mode else {
        return 0;
    };
    let lines = textarea::wrap_offsets(text, width);
    let (line, _) =
        textarea::cursor_to_display(&lines, text.chars().count(), pos);
    textarea::display_to_cursor(&lines, line, 0)
}

/// The caret index for `End`: the value's end, or the display line's end.
fn line_end(text: &str, pos: usize, mode: EditMode) -> usize {
    let EditMode::Multiline { width, .. } = mode else {
        return text.chars().count();
    };
    let lines = textarea::wrap_offsets(text, width);
    let (line, _) =
        textarea::cursor_to_display(&lines, text.chars().count(), pos);
    let col = lines[line].0.chars().count();
    textarea::display_to_cursor(&lines, line, col)
}

/// The caret index `delta` display lines away, keeping the column where
/// possible. The target line is clamped to the first/last row, so a `PageUp`
/// near the top still lands on row 0; a single step at an edge stays put (it is
/// already on the clamped row).
fn display_line_target(
    text: &str,
    pos: usize,
    mode: EditMode,
    delta: isize,
) -> usize {
    let EditMode::Multiline { width, .. } = mode else {
        return pos;
    };
    let lines = textarea::wrap_offsets(text, width);
    let (line, col) =
        textarea::cursor_to_display(&lines, text.chars().count(), pos);
    let target =
        (line as isize + delta).clamp(0, lines.len() as isize - 1) as usize;
    textarea::display_to_cursor(&lines, target, col)
}

/// The marker standing in for the text scrolled off an edge.
const SCROLL_MARKER: &str = "\u{2026}";

/// How one caret-carrying line is drawn: the visible `width` in display
/// columns, the style every unhighlighted char takes, whether the block caret
/// shows, and whether a clipped head/tail is marked with [`SCROLL_MARKER`].
#[derive(Debug, Clone, Copy)]
struct LineView<'a> {
    width: usize,
    base: Style,
    content: Option<&'a [Style]>,
    caret: bool,
    head_marker: bool,
    tail_marker: bool,
}

impl<'a> LineView<'a> {
    /// A field line: filled with `base`, caret only while focused. It signals
    /// its overflow through the filled strip, so it carries no markers.
    fn field(width: usize, base: Style, focused: bool) -> Self {
        Self {
            width,
            base,
            content: None,
            caret: focused,
            head_marker: false,
            tail_marker: false,
        }
    }

    /// A flat line on whatever surface it sits on: no fill, always a caret, and
    /// a marker once the head scrolls out of view.
    fn flat(width: usize) -> Self {
        Self {
            width,
            base: Style::default(),
            content: None,
            caret: true,
            head_marker: true,
            tail_marker: false,
        }
    }

    /// An embedded line that marks **both** clipped ends, painted on `base`
    /// with an optional per-character overlay.
    fn embedded(
        width: usize,
        base: Style,
        content: Option<&'a [Style]>,
    ) -> Self {
        Self {
            width,
            base,
            content,
            caret: true,
            head_marker: true,
            tail_marker: true,
        }
    }
}

/// The paint of one already-windowed line: where the caret and selection sit,
/// the style every unhighlighted cell takes, and an optional per-character
/// style overlay aligned to `visible`.
#[derive(Debug, Clone, Copy, Default)]
pub struct LinePaint<'a> {
    /// The caret column and selection range, in line-local columns.
    pub caret: LineCaret,
    /// The style every unhighlighted cell takes.
    pub base: Style,
    /// A per-character style overlay, patched onto `base` before the caret and
    /// selection backgrounds compose on top. Aligned to `visible`'s characters.
    pub content: Option<&'a [Style]>,
}

/// Paints one **already-windowed** line: the selection background, the block
/// caret and, where given, a per-character style overlay.
///
/// The caret cell wins over the selection so the two stay distinct, and a caret
/// sitting past the last character becomes a trailing block. Runs of equal style
/// are coalesced into one [`Span`].
///
/// This is the single source of caret/selection painting. A caller that lays out
/// its own text - a wrapped multiline box - maps the global caret into
/// line-local columns and calls this directly; a single scrolled line goes
/// through [`scrolled_line_spans`] instead.
///
/// # Examples
///
/// ```
/// use ratada::input::{LineCaret, LinePaint, line_spans};
/// use ratada::theme::{ColorOverrides, Palette, ThemeRegistry};
///
/// let base = ThemeRegistry::builtin().resolve("default");
/// let palette = Palette::resolve(base, &ColorOverrides::default());
/// let paint = LinePaint {
///     caret: LineCaret { cursor: Some(2), selection: None },
///     ..LinePaint::default()
/// };
/// // "ab" | the caret cell "c" | "d"
/// assert_eq!(line_spans("abcd", paint, &palette).len(), 3);
/// ```
#[must_use]
pub fn line_spans(
    visible: &str,
    paint: LinePaint<'_>,
    palette: &Palette,
) -> Vec<Span<'static>> {
    let chars: Vec<char> = visible.chars().collect();
    let cursor_style = style::cursor(palette).fg(Color::Black);
    let selection_style = style::bg(palette.selection);

    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut run = String::new();
    let mut run_style = paint.base;
    for (index, ch) in chars.iter().enumerate() {
        let cell_base = match paint.content.and_then(|over| over.get(index)) {
            Some(over) => paint.base.patch(*over),
            None => paint.base,
        };
        let style = cell_style(
            index,
            paint.caret,
            cell_base,
            cursor_style,
            selection_style,
        );
        if !run.is_empty() && style != run_style {
            spans.push(Span::styled(std::mem::take(&mut run), run_style));
        }
        if run.is_empty() {
            run_style = style;
        }
        run.push(*ch);
    }
    if !run.is_empty() {
        spans.push(Span::styled(run, run_style));
    }
    if paint.caret.cursor == Some(chars.len()) {
        spans.push(Span::styled(" ".to_string(), cursor_style));
    }
    spans
}

/// The style of one rendered cell: the caret cell, a selected cell, or `base`.
fn cell_style(
    index: usize,
    caret: LineCaret,
    base: Style,
    cursor_style: Style,
    selection_style: Style,
) -> Style {
    if caret.cursor == Some(index) {
        return base.patch(cursor_style);
    }
    if let Some((start, end)) = caret.selection
        && index >= start
        && index < end
    {
        return base.patch(selection_style);
    }
    base
}

/// How a scrolled single line is painted: the caret, the visible `width` in
/// display columns, the base style and an optional per-character overlay.
#[derive(Debug, Clone, Copy)]
pub struct ScrollPaint<'a> {
    /// The caret and selection over the whole value.
    pub cursor: TextCursor,
    /// The visible width in display columns.
    pub width: usize,
    /// The style every unhighlighted cell takes.
    pub base: Style,
    /// A per-character style overlay aligned to the whole value; the visible
    /// window slices it to match.
    pub content: Option<&'a [Style]>,
}

/// Paints `value` as a single line of at most `paint.width` display columns,
/// scrolling to keep the caret visible and marking **both** clipped ends with a
/// `…`.
///
/// Use this for a value embedded in a larger layout (a form field's cell, a
/// list row). A boxed input field pads to its width instead and goes through
/// [`InputField::caret_spans`]; a bare query line uses [`query_spans`].
#[must_use]
pub fn scrolled_line_spans(
    value: &str,
    paint: ScrollPaint<'_>,
    palette: &Palette,
) -> Vec<Span<'static>> {
    let view = LineView::embedded(paint.width, paint.base, paint.content);
    caret_spans(value, &paint.cursor, palette, view).0
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
/// // Equally styled characters coalesce into one span; the trailing span is
/// // the caret sitting past the last char.
/// let spans = query_spans("src", &palette, 20);
/// assert_eq!(spans.len(), 2);
/// ```
pub fn query_spans(
    text: &str,
    palette: &Palette,
    width: usize,
) -> Vec<Span<'static>> {
    query_spans_at(text, TextCursor::at_end(text), palette, width)
}

/// Like [`query_spans`], but the caret sits wherever `cursor` says and its
/// selection is painted.
///
/// Use this for a filter or search line that keeps its own movable caret; pass
/// [`TextCursor::at_end`] to get [`query_spans`]' append-only behaviour.
#[must_use]
pub fn query_spans_at(
    text: &str,
    cursor: TextCursor,
    palette: &Palette,
    width: usize,
) -> Vec<Span<'static>> {
    caret_spans(text, &cursor, palette, LineView::flat(width)).0
}

/// The visible slice of `text`, scrolled so the caret stays inside `view.width`
/// display columns. Returns the spans and the columns they occupy.
///
/// Windowing lives here; the per-cell painting is [`line_spans`]'.
fn caret_spans(
    text: &str,
    cursor: &TextCursor,
    palette: &Palette,
    view: LineView<'_>,
) -> (Vec<Span<'static>>, usize) {
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let pos = cursor.pos.min(len);
    let width = view.width.max(1);

    // Display columns before each char index, so scrolling and the visible
    // window are measured in columns (wide glyphs count as two), not chars.
    let widths: Vec<usize> =
        chars.iter().map(|ch| ch.width().unwrap_or(0)).collect();
    let mut column_at = vec![0usize; len + 1];
    for index in 0..len {
        column_at[index + 1] = column_at[index] + widths[index];
    }

    let (start, end, head, tail) = window(&column_at, pos, width, view);

    let marker_style = style::secondary(palette);
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut used = 0usize;
    if head {
        spans.push(Span::styled(SCROLL_MARKER, marker_style));
        used += 1;
    }

    let visible: String = chars[start..end].iter().collect();
    let caret_col = (view.caret && pos >= start && pos <= end)
        .then(|| pos.saturating_sub(start));
    let selection = cursor
        .selection()
        .and_then(|(from, to)| intersect(from, to, start, end))
        .map(|(from, to)| (from - start, to - start));
    let paint = LinePaint {
        caret: LineCaret {
            cursor: caret_col,
            selection,
        },
        base: view.base,
        content: view.content.map(|over| &over[start..end.min(over.len())]),
    };
    spans.extend(line_spans(&visible, paint, palette));
    used += column_at[end] - column_at[start];
    if caret_col == Some(end - start) {
        used += 1; // the trailing block caret
    }

    if tail {
        spans.push(Span::styled(SCROLL_MARKER, marker_style));
        used += 1;
    }
    (spans, used)
}

/// The visible char range `[start, end)` plus whether a head/tail `…` marker is
/// needed, for a caret at `pos` in a window of `width` display columns.
///
/// Each marker eats a column, which can push the window further along, so the
/// three are resolved together by iterating to a fixed point. A marker is only
/// taken while [`afford`] leaves the text a column, so the whole line - markers,
/// characters and the block caret - never exceeds `width`.
fn window(
    column_at: &[usize],
    pos: usize,
    width: usize,
    view: LineView<'_>,
) -> (usize, usize, bool, bool) {
    let len = column_at.len() - 1;
    let caret_column = column_at[pos];
    let (mut head, mut tail) = (false, false);
    loop {
        // `afford` keeps at least one column, so `avail` is never zero.
        let avail = width - usize::from(head) - usize::from(tail);
        let start = scroll_start(column_at, caret_column, avail);
        // Fill the window with whole characters. `scroll_start` guarantees the
        // caret's column lies within `avail - 1` of `start`, so a caret sitting
        // past the last visible character always has room for its own cell.
        let mut end = start;
        while end < len && column_at[end + 1] - column_at[start] <= avail {
            end += 1;
        }
        let next = afford(
            width,
            view.head_marker && start > 0,
            view.tail_marker && end < len,
        );
        if next == (head, tail) {
            return (start, end, head, tail);
        }
        (head, tail) = next;
    }
}

/// Drops the markers a `width`-column line cannot pay for, tail first: the text
/// and its caret always keep at least one column.
fn afford(width: usize, head: bool, tail: bool) -> (bool, bool) {
    let (mut head, mut tail) = (head, tail);
    while usize::from(head) + usize::from(tail) + 1 > width {
        if tail {
            tail = false;
        } else if head {
            head = false;
        } else {
            break;
        }
    }
    (head, tail)
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
    use ratatui::style::Modifier;
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
}

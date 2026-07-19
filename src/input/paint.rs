//! Painting a text line: the caret and selection spans, the per-character
//! style overlay, and the horizontal scroll window with its ellipsis marks.

use ratatui::{
    style::{Color, Style},
    text::Span,
};
use unicode_width::UnicodeWidthChar;

use super::{LineCaret, TextCursor};
use crate::{style, theme::Palette};

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

/// The marker standing in for the text scrolled off an edge.
pub(super) const SCROLL_MARKER: &str = "\u{2026}";

/// How one caret-carrying line is drawn: the visible `width` in display
/// columns, the style every unhighlighted char takes, whether the block caret
/// shows, and whether a clipped head/tail is marked with [`SCROLL_MARKER`].
#[derive(Debug, Clone, Copy)]
pub(super) struct LineView<'a> {
    pub(super) width: usize,
    pub(super) base: Style,
    pub(super) content: Option<&'a [Style]>,
    pub(super) caret: bool,
    pub(super) head_marker: bool,
    pub(super) tail_marker: bool,
}

impl<'a> LineView<'a> {
    /// A field line: filled with `base`, caret only while focused. It signals
    /// its overflow through the filled strip, so it carries no markers.
    pub(super) fn field(width: usize, base: Style, focused: bool) -> Self {
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
    pub(super) fn flat(width: usize) -> Self {
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
    pub(super) fn embedded(
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
pub(super) fn cell_style(
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
/// [`super::InputField::caret_spans`]; a bare query line uses [`query_spans`].
#[must_use]
pub fn scrolled_line_spans(
    value: &str,
    paint: ScrollPaint<'_>,
    palette: &Palette,
) -> Vec<Span<'static>> {
    let view = LineView::embedded(paint.width, paint.base, paint.content);
    caret_spans(value, &paint.cursor, palette, view).0
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
pub(super) fn caret_spans(
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
pub(super) fn window(
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
pub(super) fn afford(width: usize, head: bool, tail: bool) -> (bool, bool) {
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
pub(super) fn scroll_start(
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

//! The building blocks of a form row, shared by [`Form`](super::Form) and
//! every app that draws a form of its own.
//!
//! A row is the change marker column, the label (padded to the label column),
//! [`LABEL_GAP`] blank columns and the value. The focused row is tinted from
//! edge to edge with its label in the accent and bold; a field with a problem
//! has a red label and a red line under it. A field that opens a picker on
//! `Enter` carries the pointer glyph at its right edge and, while empty, the
//! dimmed [`PICKER_PLACEHOLDER`].
//!
//! The helpers take their styles from a [`FieldStyles`], so an app that reads
//! its colours through its own theme accessors keeps them;
//! [`FieldStyles::from_palette`] derives the toolkit look.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};
use unicode_width::UnicodeWidthStr;

use crate::{
    style,
    text::{pad_end, truncate},
    theme::Palette,
};

/// Blank columns between the label column and the values.
pub const LABEL_GAP: usize = 2;

/// Columns of the change marker before a label (`*` or a blank), so a label
/// never moves when its field changes.
pub const MARK_WIDTH: usize = 1;

/// What an empty field that opens a picker on `Enter` shows (dimmed).
pub const PICKER_PLACEHOLDER: &str = "enter to choose";

/// What an empty value shows (dimmed) where a field has no picker.
pub const EMPTY_VALUE: &str = "-";

/// The change marker of a field whose value differs from its baseline.
const CHANGE_MARK: &str = "*";

/// The styles of the parts of a form row.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FieldStyles {
    /// A label that is neither focused nor faulty (dimmed).
    pub label: Style,
    /// The label of the focused field (accent and bold).
    pub focused_label: Style,
    /// The change marker `*`.
    pub marker: Style,
    /// A problem: the label of a faulty field and the line under it.
    pub error: Style,
    /// The tint under the focused row.
    pub focus_tint: Style,
    /// Side information, the placeholder, an empty value and the pointer.
    pub muted: Style,
}

impl FieldStyles {
    /// The toolkit look, derived from `palette`.
    pub fn from_palette(palette: &Palette) -> Self {
        Self {
            label: style::secondary(palette),
            focused_label: style::accent(palette).add_modifier(Modifier::BOLD),
            marker: style::warning(palette),
            error: style::error(palette),
            focus_tint: style::bg(palette.selection),
            muted: style::secondary(palette),
        }
    }
}

/// The state a label is drawn in.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LabelMarks {
    /// The value differs from its baseline: the `*` marker shows.
    pub changed: bool,
    /// The field has the focus: the label is accent and bold.
    pub focused: bool,
    /// The field has a problem: the label is red (wins over the focus).
    pub problem: bool,
}

/// The width of the label column: the widest of `labels`.
pub fn label_column_width<'a>(
    labels: impl IntoIterator<Item = &'a str>,
) -> usize {
    labels
        .into_iter()
        .map(UnicodeWidthStr::width)
        .max()
        .unwrap_or(0)
}

/// The column at which the values start, for a label column `label_width`
/// wide: the marker, the labels and the gap.
pub fn value_column(label_width: usize) -> usize {
    MARK_WIDTH + label_width + LABEL_GAP
}

/// A field's label cell: the change marker column, then `label` padded to
/// `width` columns (0 leaves it as long as it is) in the style `marks` ask
/// for. The gap to the value is not included; see [`label_cell`].
pub fn label_spans(
    label: &str,
    width: usize,
    marks: LabelMarks,
    styles: &FieldStyles,
) -> Vec<Span<'static>> {
    let marker = if marks.changed {
        Span::styled(CHANGE_MARK, styles.marker)
    } else {
        Span::raw(" ".repeat(MARK_WIDTH))
    };
    let label_style = if marks.problem {
        styles.error.add_modifier(Modifier::BOLD)
    } else if marks.focused {
        styles.focused_label
    } else {
        styles.label
    };
    vec![marker, Span::styled(pad_end(label, width), label_style)]
}

/// [`label_spans`] followed by the [`LABEL_GAP`]: everything left of the
/// value, so the value starts at [`value_column`]`(width)`.
pub fn label_cell(
    label: &str,
    width: usize,
    marks: LabelMarks,
    styles: &FieldStyles,
) -> Vec<Span<'static>> {
    let mut spans = label_spans(label, width, marks, styles);
    spans.push(Span::raw(" ".repeat(LABEL_GAP)));
    spans
}

/// The red line under a field with a problem: `message`, `indent` columns in
/// (where the field's value starts), padded to `width`.
pub fn problem_line(
    message: &str,
    indent: usize,
    width: usize,
    styles: &FieldStyles,
) -> Line<'static> {
    let text = truncate(message, width.saturating_sub(indent));
    let used = indent + text.width();
    Line::from(vec![
        Span::raw(" ".repeat(indent)),
        Span::styled(text, styles.error),
        Span::raw(" ".repeat(width.saturating_sub(used))),
    ])
}

/// `spans` as the focused row: the `tint` under every span and padded to
/// `width`; spans with a background of their own (a value box, a selection,
/// the caret) keep it.
pub fn tint_focused(
    spans: Vec<Span<'static>>,
    width: usize,
    tint: Style,
) -> Vec<Span<'static>> {
    let used: usize = spans.iter().map(|span| span.content.width()).sum();
    let mut tinted: Vec<Span<'static>> = spans
        .into_iter()
        .map(|span| {
            let style = tint.patch(span.style);
            span.style(style)
        })
        .collect();
    if used < width {
        tinted.push(Span::styled(" ".repeat(width - used), tint));
    }
    tinted
}

/// `lines` of a field tinted as the focused row (see [`tint_focused`]), each
/// padded to `width`.
pub fn tint_focused_lines(
    lines: Vec<Line<'static>>,
    width: usize,
    tint: Style,
) -> Vec<Line<'static>> {
    lines
        .into_iter()
        .map(|line| Line {
            spans: tint_focused(line.spans, width, tint),
            ..line
        })
        .collect()
}

/// The value of a field that opens a picker on `Enter`, `width` columns wide:
/// the value (or, while it is empty, the dimmed [`PICKER_PLACEHOLDER`]) and
/// the `pointer` glyph flush right.
pub fn picker_value_spans(
    value: &str,
    width: usize,
    pointer: &str,
    styles: &FieldStyles,
) -> Vec<Span<'static>> {
    let (text, text_style) = if value.is_empty() {
        (PICKER_PLACEHOLDER, styles.muted)
    } else {
        (value, Style::default())
    };
    let room = width.saturating_sub(pointer.width() + 1);
    let text = truncate(text, room);
    let fill = width.saturating_sub(text.width() + pointer.width());
    vec![
        Span::styled(text, text_style),
        Span::raw(" ".repeat(fill)),
        Span::styled(pointer.to_string(), styles.muted),
    ]
}

/// `value` with side information (a length counter, a calculated result, the
/// reason a field is read-only) right-aligned in the same `width` columns.
/// The value is cut with `…` where both do not fit; the info keeps its room.
pub fn with_side_info(
    value: Vec<Span<'static>>,
    info: &str,
    width: usize,
    styles: &FieldStyles,
) -> Vec<Span<'static>> {
    let info = truncate(info, width);
    let room = width.saturating_sub(info.width() + 1);
    let mut used = 0;
    let mut spans = Vec::with_capacity(value.len() + 2);
    for span in value {
        let left = room.saturating_sub(used);
        if left == 0 {
            break;
        }
        let content = truncate(&span.content, left);
        used += content.width();
        spans.push(Span::styled(content, span.style));
    }
    spans.push(Span::raw(
        " ".repeat(width.saturating_sub(used + info.width())),
    ));
    spans.push(Span::styled(info, styles.muted));
    spans
}

/// Whether `key` is `Ctrl+O`: put the focused field back to its baseline (the
/// value the form opened with, or last saved). A chord, not a letter, so it
/// works the same while a field is being typed. `AltGr` (reported as
/// `Ctrl+Alt`) types characters and does not count.
pub fn is_reset_key(key: KeyEvent) -> bool {
    key.code == KeyCode::Char('o')
        && key.modifiers.contains(KeyModifiers::CONTROL)
        && !key.modifiers.contains(KeyModifiers::ALT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{ColorOverrides, ThemeRegistry};

    fn styles() -> FieldStyles {
        let base = ThemeRegistry::builtin().resolve("default");
        FieldStyles::from_palette(&Palette::resolve(
            base,
            &ColorOverrides::default(),
        ))
    }

    fn text(spans: &[Span<'_>]) -> String {
        spans.iter().map(|span| span.content.as_ref()).collect()
    }

    #[test]
    fn the_value_starts_two_columns_after_the_longest_label() {
        let width = label_column_width(["Due", "Description"]);
        let cell = label_cell("Due", width, LabelMarks::default(), &styles());
        assert_eq!(text(&cell).width(), value_column(width));
        assert_eq!(value_column(width), 1 + 11 + 2);
    }

    #[test]
    fn a_change_marks_its_own_column_without_moving_the_label() {
        let plain = label_spans("Due", 3, LabelMarks::default(), &styles());
        let changed = LabelMarks {
            changed: true,
            ..LabelMarks::default()
        };
        let marked = label_spans("Due", 3, changed, &styles());
        assert_eq!(text(&plain), " Due");
        assert_eq!(text(&marked), "*Due");
    }

    #[test]
    fn a_problem_turns_the_label_red_even_when_focused() {
        let styles = styles();
        let marks = LabelMarks {
            focused: true,
            problem: true,
            ..LabelMarks::default()
        };
        let spans = label_spans("Due", 3, marks, &styles);
        assert_eq!(spans[1].style.fg, styles.error.fg);
    }

    #[test]
    fn an_empty_picker_field_shows_the_placeholder_and_the_pointer() {
        let spans = picker_value_spans("", 20, ">", &styles());
        let row = text(&spans);
        assert!(row.starts_with(PICKER_PLACEHOLDER), "{row:?}");
        assert!(row.ends_with('>'));
        assert_eq!(row.width(), 20);
    }

    #[test]
    fn side_info_stays_right_and_cuts_the_value() {
        let value = vec![Span::raw("a rather long value")];
        let spans = with_side_info(value, "12/40", 16, &styles());
        let row = text(&spans);
        assert!(row.ends_with("12/40"), "{row:?}");
        assert_eq!(row.width(), 16);
        assert!(row.contains('…'));
    }

    #[test]
    fn the_problem_line_starts_under_the_value() {
        let line = problem_line("Too late.", 6, 30, &styles());
        let row: String = line
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect();
        assert_eq!(row.find('T'), Some(6));
        assert_eq!(row.width(), 30);
    }

    #[test]
    fn the_focus_tint_fills_the_row() {
        let tint = styles().focus_tint;
        let spans = tint_focused(vec![Span::raw("ab")], 6, tint);
        assert_eq!(text(&spans).width(), 6);
        assert!(spans.iter().all(|span| span.style.bg == tint.bg));
    }

    #[test]
    fn only_a_plain_ctrl_o_resets() {
        let ctrl = KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL);
        let altgr = KeyEvent::new(
            KeyCode::Char('o'),
            KeyModifiers::CONTROL | KeyModifiers::ALT,
        );
        assert!(is_reset_key(ctrl));
        assert!(!is_reset_key(altgr));
        assert!(!is_reset_key(KeyEvent::new(
            KeyCode::Char('o'),
            KeyModifiers::NONE
        )));
    }
}

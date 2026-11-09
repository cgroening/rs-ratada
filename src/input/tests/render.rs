//! Tests of the drawn line: the window, the caret and selection spans,
//! scrolling marks and wide glyphs.

use super::*;

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
    let selection_bg = Some(style::to_ratatui(palette.text_selection));
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
                let view = LineView::embedded(width, Style::default(), None);
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
    let text: String = spans.iter().map(|span| span.content.as_ref()).collect();
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
    let selection_bg = Some(style::to_ratatui(palette.text_selection));
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
    let selection_bg = Some(style::to_ratatui(palette.text_selection));
    let selected: String = spans
        .iter()
        .filter(|span| span.style.bg == selection_bg)
        .map(|span| span.content.as_ref())
        .collect();
    assert_eq!(selected, "c");
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
    let line = render_line("ab", &TextCursor::at_end("ab"), &palette, 8, false);
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

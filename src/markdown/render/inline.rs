//! Converting inline Markdown events into styled cells.
//!
//! The inline half of the renderer: everything that happens *within* a
//! paragraph - emphasis, code spans, links, `==highlights==`.

use pulldown_cmark::{Event, Parser, Tag, TagEnd};
use ratatui::style::Style;

use super::{Cell, options};
use crate::markdown::StyleSheet;

/// The inline cells of `src` when it is exactly one paragraph, else `None`.
pub(super) fn single_paragraph_cells(
    src: &str,
    sheet: &StyleSheet,
) -> Option<Vec<Cell>> {
    let events: Vec<Event> =
        Parser::new_ext(src, options(sheet.smart_punctuation)).collect();
    let mut iter = events.iter();
    match iter.next() {
        Some(Event::Start(Tag::Paragraph)) => {}
        _ => return None,
    }
    let mut inline: Vec<&Event> = Vec::new();
    for event in iter {
        match event {
            Event::End(TagEnd::Paragraph) => break,
            Event::Start(_) if !is_inline_start(event) => return None,
            _ => inline.push(event),
        }
    }
    Some(inline_to_cells(&inline, sheet, sheet.base))
}

/// Whether a `Start` event opens an inline (span-level) element rather than a
/// block.
pub(super) fn is_inline_start(event: &Event) -> bool {
    matches!(
        event,
        Event::Start(
            Tag::Emphasis
                | Tag::Strong
                | Tag::Strikethrough
                | Tag::Link { .. }
                | Tag::Image { .. }
        )
    )
}

/// Renders a raw string as a single styled run (no Markdown interpretation).
pub(super) fn literal_cells(src: &str, base: Style) -> Vec<Cell> {
    src.chars()
        .filter(|&ch| ch != '\n' && ch != '\r')
        .map(|ch| (ch, base))
        .collect()
}

/// Converts a flat run of inline events into styled cells, threading the active
/// emphasis/strong/strike/link/code styles and splitting out `==highlight==`.
pub(super) fn inline_to_cells(
    events: &[&Event],
    sheet: &StyleSheet,
    base: Style,
) -> Vec<Cell> {
    let mut cells = Vec::new();
    let mut style = base;
    let mut stack: Vec<Style> = Vec::new();
    for event in events {
        match event {
            Event::Start(Tag::Emphasis) => {
                stack.push(style);
                style = style.patch(sheet.emphasis);
            }
            Event::Start(Tag::Strong) => {
                stack.push(style);
                style = style.patch(sheet.strong);
            }
            Event::Start(Tag::Strikethrough) => {
                stack.push(style);
                style = style.patch(sheet.strikethrough);
            }
            // A link and an image's alt text both style like a link.
            Event::Start(Tag::Link { .. } | Tag::Image { .. }) => {
                stack.push(style);
                style = style.patch(sheet.link);
            }
            Event::End(
                TagEnd::Emphasis
                | TagEnd::Strong
                | TagEnd::Strikethrough
                | TagEnd::Link
                | TagEnd::Image,
            ) => {
                if let Some(prev) = stack.pop() {
                    style = prev;
                }
            }
            Event::Text(text) => push_text(&mut cells, text, style, sheet),
            // Raw HTML (`<br>`, `<span>`, …) is shown literally rather than
            // dropped, in the configured HTML style.
            Event::Html(html) | Event::InlineHtml(html) => {
                push_text(&mut cells, html, style.patch(sheet.html), sheet);
            }
            Event::Code(code) => {
                let code_style = style.patch(sheet.inline_code);
                for ch in code.chars() {
                    cells.push((ch, code_style));
                }
            }
            Event::SoftBreak => cells.push((
                if sheet.preserve_line_breaks {
                    '\n'
                } else {
                    ' '
                },
                style,
            )),
            Event::HardBreak => cells.push(('\n', style)),
            _ => {}
        }
    }
    cells
}

/// Pushes text, styling `==highlighted==` runs and dropping their markers.
fn push_text(
    cells: &mut Vec<Cell>,
    text: &str,
    style: Style,
    sheet: &StyleSheet,
) {
    for (segment, highlighted) in split_highlight(text) {
        let seg_style = if highlighted {
            style.patch(sheet.highlight)
        } else {
            style
        };
        for ch in segment.chars() {
            cells.push((ch, seg_style));
        }
    }
}

/// Splits text into `(segment, is_highlighted)` runs, treating `==x==` (with a
/// non-empty, marker-free inner) as a highlight and removing the `==` markers.
///
/// Mirrors the `==[^=]+==` rule used in the user's editor: the inner run may not
/// contain `=`, so `====` and empty `====` never match.
fn split_highlight(text: &str) -> Vec<(String, bool)> {
    let chars: Vec<char> = text.chars().collect();
    let mut out: Vec<(String, bool)> = Vec::new();
    let mut plain = String::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '='
            && i + 1 < chars.len()
            && chars[i + 1] == '='
            && let Some(end) = find_highlight_end(&chars, i + 2)
        {
            if !plain.is_empty() {
                out.push((std::mem::take(&mut plain), false));
            }
            let inner: String = chars[i + 2..end].iter().collect();
            out.push((inner, true));
            i = end + 2;
            continue;
        }
        plain.push(chars[i]);
        i += 1;
    }
    if !plain.is_empty() {
        out.push((plain, false));
    }
    out
}

/// Finds the index of the first `=` of a closing `==` for a highlight opened at
/// `start`, requiring a non-empty inner run that contains no `=`.
pub(super) fn find_highlight_end(
    chars: &[char],
    start: usize,
) -> Option<usize> {
    let mut j = start;
    while j + 1 < chars.len() {
        if chars[j] == '=' && chars[j + 1] == '=' {
            return (j > start).then_some(j);
        }
        if chars[j] == '=' {
            return None;
        }
        j += 1;
    }
    None
}

/// Builds styled cells from a plain string.
pub(super) fn styled_cells(text: &str, style: Style) -> Vec<Cell> {
    text.chars().map(|ch| (ch, style)).collect()
}

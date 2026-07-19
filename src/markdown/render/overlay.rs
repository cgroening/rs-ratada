//! A per-character style overlay over the raw Markdown source.
//!
//! A separate algorithm from the renderer: it keeps the source text intact,
//! markers included, and only decides how each byte range is painted - what an
//! editable field showing live Markdown needs.

use pulldown_cmark::{Event, Parser, Tag};
use ratatui::style::Style;

use super::{inline::find_highlight_end, options};
use crate::markdown::StyleSheet;

/// Builds a per-character style overlay for `src` (one [`Style`] per char, to be
/// patched onto the caller's base). Markers are kept, so the styles align with
/// the raw characters and coexist with a text cursor. Uses pulldown's offset
/// iterator (whose event ranges include the markers) plus the `==highlight==`
/// scan.
pub(crate) fn style_overlay(src: &str, sheet: &StyleSheet) -> Vec<Style> {
    let offsets: Vec<usize> =
        src.char_indices().map(|(byte, _)| byte).collect();
    let char_count = offsets.len();
    // Carry the configured body-text colour so edit mode tints normal text too
    // (a no-op when `text` is unset).
    let mut base = Style::default();
    if let Some(fg) = sheet.base.fg {
        base = base.fg(fg);
    }
    let mut styles = vec![base; char_count];
    if char_count == 0 {
        return styles;
    }
    for (event, range) in
        Parser::new_ext(src, options(false)).into_offset_iter()
    {
        let style = match &event {
            Event::Start(Tag::Heading { level, .. }) => {
                Some(sheet.heading_char_style(*level))
            }
            Event::Start(Tag::Strong) => Some(sheet.strong),
            Event::Start(Tag::Emphasis) => Some(sheet.emphasis),
            Event::Start(Tag::Strikethrough) => Some(sheet.strikethrough),
            Event::Start(Tag::Link { .. } | Tag::Image { .. }) => {
                Some(sheet.link)
            }
            // A callout colours its whole block; a plain quote uses the quote
            // colours.
            Event::Start(Tag::BlockQuote(Some(kind))) => {
                Some(sheet.callout.get(*kind).char_style())
            }
            Event::Start(Tag::BlockQuote(None)) => {
                Some(sheet.quote_char_style())
            }
            Event::Start(Tag::CodeBlock(_)) => {
                Some(sheet.code_block.char_style())
            }
            Event::Code(_) => Some(sheet.inline_code),
            _ => None,
        };
        if let Some(style) = style {
            patch_byte_range(&mut styles, &offsets, range, style);
        }
    }
    // `==highlight==` is not CommonMark; scan it directly (markers kept).
    for (start, end) in highlight_char_ranges(src) {
        for cell in &mut styles[start..end.min(char_count)] {
            *cell = cell.patch(sheet.highlight);
        }
    }
    styles
}

/// Patches `style` over the character range that the byte `range` covers.
fn patch_byte_range(
    styles: &mut [Style],
    offsets: &[usize],
    range: std::ops::Range<usize>,
    style: Style,
) {
    let len = styles.len();
    let start = offsets.partition_point(|&byte| byte < range.start);
    let end = offsets.partition_point(|&byte| byte < range.end).min(len);
    for cell in &mut styles[start..end] {
        *cell = cell.patch(style);
    }
}

/// The character ranges of `==highlighted==` runs (markers included), mirroring
/// the editor's `==[^=]+==` rule.
fn highlight_char_ranges(src: &str) -> Vec<(usize, usize)> {
    let chars: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '='
            && i + 1 < chars.len()
            && chars[i + 1] == '='
            && let Some(end) = find_highlight_end(&chars, i + 2)
        {
            out.push((i, end + 2));
            i = end + 2;
            continue;
        }
        i += 1;
    }
    out
}

//! The rendering engine: a CommonMark event stream turned into styled
//! `ratatui` lines and spans.
//!
//! This module is deliberately free of any mdtask types - it speaks only
//! `ratatui` styles plus the local [`StyleSheet`]. The block walk buffers the
//! inline events of each leaf block, converts them to styled character cells
//! (so emphasis, inline code and `==highlight==` compose), word-wraps them to
//! the available width and decorates each line for its container (heading
//! style, list marker, quote bar, code band).

use pulldown_cmark::Alignment;
use pulldown_cmark::BlockQuoteKind;
use pulldown_cmark::CodeBlockKind;
use pulldown_cmark::Event;
use pulldown_cmark::HeadingLevel;
use pulldown_cmark::Options;
use pulldown_cmark::Parser;
use pulldown_cmark::Tag;
use pulldown_cmark::TagEnd;
use ratatui::style::Color;
use ratatui::style::Modifier;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::text::Span;
use unicode_width::UnicodeWidthChar;

use super::Link;
use super::StyleSheet;
use super::callout_label;

/// A single character paired with the style it renders in.
type Cell = (char, Style);

/// The enabled CommonMark extensions: strikethrough (`~~`), GFM task lists
/// (`- [ ]`), GFM callouts (`> [!NOTE]`) and tables; smart punctuation
/// (`--`→`—`, …) is added only when `smart` (display rendering opts in).
fn options(smart: bool) -> Options {
    let mut options = Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_GFM
        | Options::ENABLE_TABLES;
    if smart {
        options |= Options::ENABLE_SMART_PUNCTUATION;
    }
    options
}

/// Display width of a single character (wide glyphs count as two cells).
fn ch_width(ch: char) -> usize {
    ch.width().unwrap_or(0)
}

/// Display width of a cell slice.
fn cells_width(cells: &[Cell]) -> usize {
    cells.iter().map(|&(ch, _)| ch_width(ch)).sum()
}

/// Renders a Markdown source string into wrapped, decorated lines for a
/// multi-line display box of the given `width`.
pub(super) fn render_block(
    src: &str,
    width: usize,
    sheet: &StyleSheet,
) -> Vec<Line<'static>> {
    let events: Vec<Event> =
        Parser::new_ext(src, options(sheet.smart_punctuation)).collect();
    let mut state = BlockState::new(width, sheet);
    let mut i = 0;
    while i < events.len() {
        state.handle(&events[i]);
        i += 1;
    }
    state.into_lines()
}

/// Renders a Markdown source string as inline-only spans for a single-line
/// context (titles, single-line fields), clipped to `width` with an ellipsis.
///
/// Only a value that parses to a single paragraph is treated as inline
/// Markdown; anything whose top-level structure is a block (a heading, quote,
/// list, ...) is shown verbatim, so block syntax such as a leading `# ` stays
/// literal in single-line contexts.
pub(super) fn render_inline(
    src: &str,
    width: usize,
    sheet: &StyleSheet,
) -> Vec<Span<'static>> {
    let cells = single_paragraph_cells(src, sheet)
        .unwrap_or_else(|| literal_cells(src, sheet.base));
    clip_cells(&cells, width, sheet.ellipsis)
}

/// The collected hyperlinks of a Markdown source, in document order.
pub(super) fn links(src: &str) -> Vec<Link> {
    let mut out = Vec::new();
    let mut current: Option<(String, String)> = None;
    for event in Parser::new_ext(src, options(false)) {
        match event {
            // Both links and images carry an openable destination URL.
            Event::Start(
                Tag::Link { dest_url, .. } | Tag::Image { dest_url, .. },
            ) => {
                current = Some((dest_url.into_string(), String::new()));
            }
            Event::Text(text) | Event::Code(text) => {
                if let Some((_, label)) = current.as_mut() {
                    label.push_str(&text);
                }
            }
            Event::End(TagEnd::Link | TagEnd::Image) => {
                if let Some((url, text)) = current.take() {
                    out.push(Link { text, url });
                }
            }
            _ => {}
        }
    }
    out
}

/// Clips a span list to `max` display columns, appending the `ellipsis`-styled
/// `…` when it overflows.
pub(super) fn clip_spans(
    spans: Vec<Span<'static>>,
    max: usize,
    ellipsis: Style,
) -> Vec<Span<'static>> {
    let total: usize = spans.iter().map(|s| span_width(s)).sum();
    if total <= max {
        return spans;
    }
    let budget = max.saturating_sub(1);
    let mut out: Vec<Span<'static>> = Vec::new();
    let mut used = 0usize;
    for span in spans {
        if used >= budget {
            break;
        }
        let mut text = String::new();
        for ch in span.content.chars() {
            let cw = ch_width(ch);
            if used + cw > budget {
                break;
            }
            text.push(ch);
            used += cw;
        }
        if !text.is_empty() {
            out.push(Span::styled(text, span.style));
        }
    }
    out.push(Span::styled("\u{2026}".to_string(), ellipsis));
    out
}

/// Display width of a span's content.
fn span_width(span: &Span) -> usize {
    span.content.chars().map(ch_width).sum()
}

// --- edit-mode style overlay --------------------------------------------

/// Builds a per-character style overlay for `src` (one [`Style`] per char, to be
/// patched onto the caller's base). Markers are kept, so the styles align with
/// the raw characters and coexist with a text cursor. Uses pulldown's offset
/// iterator (whose event ranges include the markers) plus the `==highlight==`
/// scan.
pub(super) fn style_overlay(src: &str, sheet: &StyleSheet) -> Vec<Style> {
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

// --- inline conversion --------------------------------------------------

/// The inline cells of `src` when it is exactly one paragraph, else `None`.
fn single_paragraph_cells(src: &str, sheet: &StyleSheet) -> Option<Vec<Cell>> {
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
fn is_inline_start(event: &Event) -> bool {
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
fn literal_cells(src: &str, base: Style) -> Vec<Cell> {
    src.chars()
        .filter(|&ch| ch != '\n' && ch != '\r')
        .map(|ch| (ch, base))
        .collect()
}

/// Converts a flat run of inline events into styled cells, threading the active
/// emphasis/strong/strike/link/code styles and splitting out `==highlight==`.
fn inline_to_cells(
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
            Event::SoftBreak => cells.push((' ', style)),
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
fn find_highlight_end(chars: &[char], start: usize) -> Option<usize> {
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

// --- block walk ---------------------------------------------------------

/// One open list level: ordered lists carry the next item number.
struct ListLevel {
    ordered: Option<u64>,
}

/// A GFM table being accumulated: column alignments, the collected rows (each a
/// list of styled cells), and how many leading rows are the header.
struct TableState {
    aligns: Vec<Alignment>,
    rows: Vec<Vec<Vec<Cell>>>,
    head_rows: usize,
    in_head: bool,
}

/// Mutable state threaded through the block walk.
struct BlockState<'s> {
    sheet: &'s StyleSheet,
    width: usize,
    out: Vec<Line<'static>>,
    /// Buffered inline events of the current leaf block.
    inline: Vec<Event<'static>>,
    heading: Option<HeadingLevel>,
    /// Open blockquote stack; each entry is its callout kind (`None` = plain
    /// quote). The length is the quote nesting depth.
    quotes: Vec<Option<BlockQuoteKind>>,
    lists: Vec<ListLevel>,
    /// Marker cells for the next item's first line (bullet/number/checkbox).
    pending_marker: Option<Vec<Cell>>,
    /// Raw text of the code block currently being collected.
    code: Option<String>,
    /// The fenced code block's language label (empty/none = no label).
    code_lang: Option<String>,
    /// The GFM table currently being accumulated, if any.
    table: Option<TableState>,
    /// Whether a top-level block has already been emitted (drives separators).
    produced: bool,
}

impl<'s> BlockState<'s> {
    fn new(width: usize, sheet: &'s StyleSheet) -> Self {
        BlockState {
            sheet,
            width: width.max(1),
            out: Vec::new(),
            inline: Vec::new(),
            heading: None,
            quotes: Vec::new(),
            lists: Vec::new(),
            pending_marker: None,
            code: None,
            code_lang: None,
            table: None,
            produced: false,
        }
    }

    fn into_lines(self) -> Vec<Line<'static>> {
        self.out
    }

    fn at_top_level(&self) -> bool {
        self.quotes.is_empty() && self.lists.is_empty()
    }

    /// The innermost open callout kind, if the current quote is a callout.
    fn callout_kind(&self) -> Option<BlockQuoteKind> {
        self.quotes.last().copied().flatten()
    }

    /// Inserts a blank separator before a new top-level block.
    fn separate(&mut self) {
        if self.at_top_level() && self.produced {
            self.out.push(Line::from(String::new()));
        }
    }

    fn handle(&mut self, event: &Event) {
        match event {
            Event::Start(Tag::Paragraph) => self.separate(),
            Event::End(TagEnd::Paragraph) => self.flush_inline(),
            Event::Start(Tag::Heading { level, .. }) => {
                self.separate();
                self.heading = Some(*level);
            }
            Event::End(TagEnd::Heading(_)) => {
                self.flush_inline();
                self.heading = None;
            }
            Event::Start(Tag::BlockQuote(kind)) => {
                self.separate();
                self.quotes.push(*kind);
                // A callout opens with a coloured title line (NOTE/TIP/…).
                if let Some(kind) = kind {
                    self.emit_callout_title(*kind);
                }
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                self.quotes.pop();
                if self.at_top_level() {
                    self.produced = true;
                }
            }
            Event::Start(Tag::List(start)) => {
                self.separate();
                self.lists.push(ListLevel { ordered: *start });
            }
            Event::End(TagEnd::List(_)) => {
                self.lists.pop();
                if self.at_top_level() {
                    self.produced = true;
                }
            }
            Event::Start(Tag::Item) => self.begin_item(),
            Event::End(TagEnd::Item) => self.flush_inline(),
            Event::TaskListMarker(checked) => self.set_task_marker(*checked),
            Event::Start(Tag::CodeBlock(kind)) => {
                self.separate();
                self.code = Some(String::new());
                self.code_lang = match kind {
                    CodeBlockKind::Fenced(lang) if !lang.is_empty() => {
                        Some(lang.to_string())
                    }
                    _ => None,
                };
            }
            Event::End(TagEnd::CodeBlock) => self.flush_code(),
            // GFM table: accumulate cells, then lay it out on close.
            Event::Start(Tag::Table(aligns)) => {
                self.separate();
                self.table = Some(TableState {
                    aligns: aligns.clone(),
                    rows: Vec::new(),
                    head_rows: 0,
                    in_head: false,
                });
            }
            Event::End(TagEnd::Table) => self.flush_table(),
            Event::Start(Tag::TableHead) => self.table_begin_row(true),
            Event::End(TagEnd::TableHead) => {}
            Event::Start(Tag::TableRow) => self.table_begin_row(false),
            Event::End(TagEnd::TableRow) => {}
            Event::Start(Tag::TableCell) => self.inline.clear(),
            Event::End(TagEnd::TableCell) => self.table_push_cell(),
            // HTML blocks are paragraph-like; render their text literally.
            Event::Start(Tag::HtmlBlock) => self.separate(),
            Event::End(TagEnd::HtmlBlock) => self.flush_inline(),
            Event::Rule => self.emit_rule(),
            Event::Text(_)
            | Event::Code(_)
            | Event::Html(_)
            | Event::InlineHtml(_)
            | Event::SoftBreak
            | Event::HardBreak
            | Event::Start(
                Tag::Emphasis
                | Tag::Strong
                | Tag::Strikethrough
                | Tag::Link { .. }
                | Tag::Image { .. },
            )
            | Event::End(
                TagEnd::Emphasis
                | TagEnd::Strong
                | TagEnd::Strikethrough
                | TagEnd::Link
                | TagEnd::Image,
            ) => self.inline_event(event),
            _ => {}
        }
    }

    /// Buffers an inline event, or appends to the open code block's raw text.
    fn inline_event(&mut self, event: &Event) {
        if let Some(code) = self.code.as_mut() {
            if let Event::Text(text) = event {
                code.push_str(text);
            }
            return;
        }
        self.inline.push(event.clone().into_static());
    }

    /// Computes the marker cells for a new list item (bullet/number).
    fn begin_item(&mut self) {
        let depth = self.lists.len().saturating_sub(1);
        let marker = match self.lists.last_mut() {
            Some(level) => match level.ordered.as_mut() {
                Some(number) => {
                    let text = format!("{number}. ");
                    *number += 1;
                    styled_cells(&text, self.bullet_style())
                }
                None => {
                    let glyph = self.bullet_glyph(depth);
                    styled_cells(&format!("{glyph} "), self.bullet_style())
                }
            },
            None => Vec::new(),
        };
        self.pending_marker = Some(marker);
    }

    /// Replaces the pending bullet with a checkbox glyph for a task item.
    fn set_task_marker(&mut self, checked: bool) {
        let (glyph, style) = if checked {
            (
                &self.sheet.checkbox.checked,
                self.sheet.checkbox.checked_style(),
            )
        } else {
            (
                &self.sheet.checkbox.unchecked,
                self.sheet.checkbox.unchecked_style(),
            )
        };
        self.pending_marker = Some(styled_cells(&format!("{glyph} "), style));
    }

    fn bullet_glyph(&self, depth: usize) -> String {
        let glyphs = &self.sheet.bullet.glyphs;
        if glyphs.is_empty() {
            "\u{2022}".to_string() // •
        } else {
            glyphs[depth % glyphs.len()].clone()
        }
    }

    fn bullet_style(&self) -> Style {
        match self.sheet.bullet.fg {
            Some(color) => Style::default().fg(color),
            None => Style::default(),
        }
    }

    /// Flushes the buffered inline events as one wrapped, decorated block.
    fn flush_inline(&mut self) {
        if self.inline.is_empty() && self.pending_marker.is_none() {
            return;
        }
        let events: Vec<&Event> = self.inline.iter().collect();
        let base = self.text_base();
        let content = inline_to_cells(&events, self.sheet, base);
        self.inline.clear();

        let (first_prefix, cont_prefix) = self.prefixes();
        let fill_bg = self.fill_bg();
        self.emit_wrapped(content, first_prefix, cont_prefix, fill_bg);
        if self.at_top_level() {
            self.produced = true;
        }
    }

    /// The base text style for the current container (heading / callout / quote
    /// / body).
    fn text_base(&self) -> Style {
        let sheet = self.sheet;
        if let Some(level) = self.heading {
            return sheet.heading_style(level);
        }
        if let Some(kind) = self.callout_kind() {
            let mut style = sheet.base;
            if let Some(fg) = sheet.callout.get(kind).fg {
                style = style.fg(fg);
            }
            return style;
        }
        if !self.quotes.is_empty() {
            let mut style = sheet.base;
            if let Some(fg) = sheet.quote.fg {
                style = style.fg(fg);
            }
            return style;
        }
        sheet.base
    }

    /// The full-width band colour for the current container, if any.
    fn fill_bg(&self) -> Option<Color> {
        let sheet = self.sheet;
        if let Some(level) = self.heading {
            return sheet.heading_bg(level);
        }
        if let Some(kind) = self.callout_kind() {
            return sheet.callout.get(kind).bg;
        }
        if !self.quotes.is_empty() {
            return sheet.quote.bg;
        }
        None
    }

    /// The first-line and continuation-line prefixes (quote bars, list indent,
    /// item marker) padded to a common width for alignment.
    fn prefixes(&mut self) -> (Vec<Cell>, Vec<Cell>) {
        let mut first: Vec<Cell> = Vec::new();
        for _ in 0..self.quotes.len() {
            first.extend(self.quote_bar());
        }
        let indent = self.lists.len().saturating_sub(1) * 2;
        let indent_style = self.text_base();
        for _ in 0..indent {
            first.push((' ', indent_style));
        }
        let mut cont = first.clone();
        if let Some(marker) = self.pending_marker.take() {
            let marker_w = cells_width(&marker);
            first.extend(marker);
            for _ in 0..marker_w {
                cont.push((' ', indent_style));
            }
        }
        (first, cont)
    }

    /// One quote-bar prefix cell pair (`▎ `): in a callout it uses the callout
    /// colour, else the configured quote bar colour.
    fn quote_bar(&self) -> Vec<Cell> {
        let sheet = self.sheet;
        let mut style = Style::default();
        let fg = match self.callout_kind() {
            Some(kind) => sheet.callout.get(kind).fg,
            None => sheet.quote.bar_fg,
        };
        if let Some(fg) = fg {
            style = style.fg(fg);
        }
        let mut cells = styled_cells(&sheet.quote.bar, style);
        cells.push((' ', style));
        cells
    }

    /// Emits a callout's coloured title line (e.g. `NOTE`) above its body.
    fn emit_callout_title(&mut self, kind: BlockQuoteKind) {
        let sheet = self.sheet;
        let callout = sheet.callout.get(kind);
        let bg = callout.bg;
        let mut style = Style::default().add_modifier(Modifier::BOLD);
        if let Some(fg) = callout.fg {
            style = style.fg(fg);
        }
        let content = styled_cells(callout_label(kind), style);
        let mut prefix: Vec<Cell> = Vec::new();
        for _ in 0..self.quotes.len() {
            prefix.extend(self.quote_bar());
        }
        self.push_decorated(&prefix, &content, bg);
    }

    /// Starts a new table row (header or body).
    fn table_begin_row(&mut self, head: bool) {
        if let Some(table) = self.table.as_mut() {
            table.in_head = head;
            table.rows.push(Vec::new());
            if head {
                table.head_rows += 1;
            }
        }
    }

    /// Converts the buffered inline events into the current row's next cell.
    fn table_push_cell(&mut self) {
        let cell = {
            let events: Vec<&Event> = self.inline.iter().collect();
            inline_to_cells(&events, self.sheet, self.sheet.base)
        };
        self.inline.clear();
        if let Some(table) = self.table.as_mut()
            && let Some(row) = table.rows.last_mut()
        {
            row.push(cell);
        }
    }

    /// Lays out the accumulated table: aligned columns joined by `│`, with a
    /// `├─┼─┤` rule after the header, capped to the box width.
    fn flush_table(&mut self) {
        let Some(table) = self.table.take() else {
            return;
        };
        let cols = table
            .aligns
            .len()
            .max(table.rows.iter().map(Vec::len).max().unwrap_or(0));
        if cols == 0 {
            return;
        }
        let mut widths = vec![1usize; cols];
        for row in &table.rows {
            for (i, cell) in row.iter().enumerate() {
                widths[i] = widths[i].max(cells_width(cell).max(1));
            }
        }
        // Cap to the box: cols+1 bars + 2 padding spaces per column.
        let overhead = cols + 1 + 2 * cols;
        let avail = self.width.saturating_sub(overhead).max(cols);
        if widths.iter().sum::<usize>() > avail {
            let cap = (avail / cols).max(1);
            for width in &mut widths {
                *width = (*width).min(cap);
            }
        }
        let border = match self.sheet.table_border {
            Some(color) => Style::default().fg(color),
            None => Style::default(),
        };
        for (index, row) in table.rows.iter().enumerate() {
            self.out
                .push(table_row_line(row, &widths, &table.aligns, border));
            if table.head_rows > 0 && index + 1 == table.head_rows {
                self.out.push(table_separator(&widths, border));
            }
        }
        if self.at_top_level() {
            self.produced = true;
        }
    }

    /// Wraps `content` to the available width and pushes the decorated lines.
    fn emit_wrapped(
        &mut self,
        content: Vec<Cell>,
        first_prefix: Vec<Cell>,
        cont_prefix: Vec<Cell>,
        fill_bg: Option<Color>,
    ) {
        let prefix_w = cells_width(&first_prefix);
        let avail = self.width.saturating_sub(prefix_w).max(1);
        let wrapped = wrap_cells(&content, avail);
        let lines = if wrapped.is_empty() {
            vec![Vec::new()]
        } else {
            wrapped
        };
        for (index, line) in lines.into_iter().enumerate() {
            let prefix = if index == 0 {
                &first_prefix
            } else {
                &cont_prefix
            };
            self.push_decorated(prefix, &line, fill_bg);
        }
    }

    /// Assembles prefix + content (+ optional full-width band) into one line.
    fn push_decorated(
        &mut self,
        prefix: &[Cell],
        content: &[Cell],
        fill_bg: Option<Color>,
    ) {
        let mut cells: Vec<Cell> = prefix.to_vec();
        cells.extend_from_slice(content);
        if let Some(bg) = fill_bg {
            for cell in &mut cells {
                cell.1 = cell.1.bg(bg);
            }
            let pad = Style::default().bg(bg);
            let mut filled = cells_width(&cells);
            while filled < self.width {
                cells.push((' ', pad));
                filled += 1;
            }
        }
        self.out.push(Line::from(cells_to_spans(&cells)));
    }

    /// Flushes the collected code block as a banded set of hard-wrapped lines,
    /// prefixed by the fenced language label when present.
    fn flush_code(&mut self) {
        let Some(code) = self.code.take() else {
            return;
        };
        let style = self.sheet.code_block.text_style();
        let bg = self.sheet.code_block.bg;
        if let Some(lang) = self.code_lang.take() {
            let mut title = Style::default();
            if let Some(fg) = self.sheet.code_block.title_fg {
                title = title.fg(fg);
            }
            let cells = styled_cells(&lang, title);
            self.push_decorated(&[], &cells, bg);
        }
        let body = code.strip_suffix('\n').unwrap_or(&code);
        for raw in body.split('\n') {
            let cells: Vec<Cell> = raw.chars().map(|ch| (ch, style)).collect();
            for chunk in hard_wrap(&cells, self.width) {
                self.push_decorated(&[], &chunk, bg);
            }
        }
        if self.at_top_level() {
            self.produced = true;
        }
    }

    /// Emits a full-width horizontal rule.
    fn emit_rule(&mut self) {
        self.separate();
        let mut style = Style::default();
        if let Some(fg) = self.sheet.rule.fg {
            style = style.fg(fg);
        }
        let glyph = self.sheet.rule.glyph.chars().next().unwrap_or('\u{2500}');
        let line: Vec<Cell> = (0..self.width).map(|_| (glyph, style)).collect();
        self.out.push(Line::from(cells_to_spans(&line)));
        if self.at_top_level() {
            self.produced = true;
        }
    }
}

// --- cell helpers -------------------------------------------------------

/// Builds styled cells from a plain string.
fn styled_cells(text: &str, style: Style) -> Vec<Cell> {
    text.chars().map(|ch| (ch, style)).collect()
}

/// One rendered table row: each cell aligned and padded to its column width,
/// wrapped in `│` borders.
fn table_row_line(
    row: &[Vec<Cell>],
    widths: &[usize],
    aligns: &[Alignment],
    border: Style,
) -> Line<'static> {
    let mut spans = vec![Span::styled("\u{2502}".to_string(), border)];
    for (i, &width) in widths.iter().enumerate() {
        let empty = Vec::new();
        let cell = row.get(i).unwrap_or(&empty);
        let align = aligns.get(i).copied().unwrap_or(Alignment::None);
        spans.push(Span::raw(" ".to_string()));
        spans.extend(cells_to_spans(&align_cell(cell, width, align)));
        spans.push(Span::raw(" ".to_string()));
        spans.push(Span::styled("\u{2502}".to_string(), border));
    }
    Line::from(spans)
}

/// The header separator line (`├──┼──┤`) sized to the column widths.
fn table_separator(widths: &[usize], border: Style) -> Line<'static> {
    let mut text = String::from("\u{251c}");
    for (i, &width) in widths.iter().enumerate() {
        if i > 0 {
            text.push('\u{253c}');
        }
        text.push_str(&"\u{2500}".repeat(width + 2));
    }
    text.push('\u{2524}');
    Line::from(Span::styled(text, border))
}

/// Clips a cell to `width` display columns and pads it per the column alignment.
fn align_cell(cell: &[Cell], width: usize, align: Alignment) -> Vec<Cell> {
    let mut clipped: Vec<Cell> = Vec::new();
    let mut used = 0usize;
    for &c in cell {
        let cw = ch_width(c.0);
        if used + cw > width {
            break;
        }
        clipped.push(c);
        used += cw;
    }
    let pad = width.saturating_sub(used);
    let (left, right) = match align {
        Alignment::Right => (pad, 0),
        Alignment::Center => (pad / 2, pad - pad / 2),
        _ => (0, pad),
    };
    let mut out = Vec::with_capacity(width);
    out.extend(std::iter::repeat_n((' ', Style::default()), left));
    out.extend(clipped);
    out.extend(std::iter::repeat_n((' ', Style::default()), right));
    out
}

/// Greedy word-wrap over styled cells: breaks at the last space, hard-splits an
/// over-long word, and treats an embedded `\n` (a hard break) as a forced
/// break. Each output line is a cell run with its styles preserved.
fn wrap_cells(cells: &[Cell], width: usize) -> Vec<Vec<Cell>> {
    let width = width.max(1);
    let mut lines: Vec<Vec<Cell>> = Vec::new();
    let mut cur: Vec<Cell> = Vec::new();
    let mut cur_w = 0usize;
    let mut last_space: Option<usize> = None;
    for &(ch, style) in cells {
        if ch == '\n' {
            lines.push(std::mem::take(&mut cur));
            cur_w = 0;
            last_space = None;
            continue;
        }
        let cw = ch_width(ch);
        if cur_w + cw > width && !cur.is_empty() {
            match last_space {
                Some(sp) => {
                    let tail = cur.split_off(sp);
                    lines.push(std::mem::take(&mut cur));
                    cur = tail.into_iter().skip(1).collect();
                    cur_w = cells_width(&cur);
                }
                None => {
                    lines.push(std::mem::take(&mut cur));
                    cur_w = 0;
                }
            }
            last_space = None;
        }
        if ch == ' ' {
            last_space = Some(cur.len());
        }
        cur.push((ch, style));
        cur_w += cw;
    }
    lines.push(cur);
    lines
}

/// Hard-wraps cells purely by display width (for code blocks, which must keep
/// their literal spacing rather than wrap at word boundaries).
fn hard_wrap(cells: &[Cell], width: usize) -> Vec<Vec<Cell>> {
    let width = width.max(1);
    if cells.is_empty() {
        return vec![Vec::new()];
    }
    let mut lines: Vec<Vec<Cell>> = Vec::new();
    let mut cur: Vec<Cell> = Vec::new();
    let mut cur_w = 0usize;
    for &cell in cells {
        let cw = ch_width(cell.0);
        if cur_w + cw > width && !cur.is_empty() {
            lines.push(std::mem::take(&mut cur));
            cur_w = 0;
        }
        cur.push(cell);
        cur_w += cw;
    }
    lines.push(cur);
    lines
}

/// Coalesces a cell run into spans, merging adjacent cells of equal style.
fn cells_to_spans(cells: &[Cell]) -> Vec<Span<'static>> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut buf = String::new();
    let mut current: Option<Style> = None;
    for &(ch, style) in cells {
        if current != Some(style) {
            if let Some(prev) = current
                && !buf.is_empty()
            {
                spans.push(Span::styled(std::mem::take(&mut buf), prev));
            }
            current = Some(style);
        }
        buf.push(ch);
    }
    if let (Some(style), false) = (current, buf.is_empty()) {
        spans.push(Span::styled(buf, style));
    }
    spans
}

/// Clips a cell run to `width` columns (single-line), appending the
/// `ellipsis`-styled `…` on overflow.
fn clip_cells(
    cells: &[Cell],
    width: usize,
    ellipsis: Style,
) -> Vec<Span<'static>> {
    let total = cells_width(cells);
    if total <= width {
        return cells_to_spans(cells);
    }
    let budget = width.saturating_sub(1);
    let mut kept: Vec<Cell> = Vec::new();
    let mut used = 0usize;
    for &cell in cells {
        let cw = ch_width(cell.0);
        if used + cw > budget {
            break;
        }
        kept.push(cell);
        used += cw;
    }
    let mut spans = cells_to_spans(&kept);
    spans.push(Span::styled("\u{2026}".to_string(), ellipsis));
    spans
}

//! The rendering engine: a CommonMark event stream turned into styled
//! `ratatui` lines and spans.
//!
//! This module is deliberately free of any mdtask types - it speaks only
//! `ratatui` styles plus the local [`StyleSheet`]. The block walk buffers the
//! inline events of each leaf block, converts them to styled character cells
//! (so emphasis, inline code and `==highlight==` compose), word-wraps them to
//! the available width and decorates each line for its container (heading
//! style, list marker, quote bar, code band).

mod block_list;
mod block_table;
mod inline;
mod overlay;
mod wrap;

pub(super) use overlay::style_overlay;

use block_list::ListLevel;
use block_table::TableState;
use inline::{
    inline_to_cells, literal_cells, single_paragraph_cells, styled_cells,
};
use wrap::{cells_to_spans, clip_cells, hard_wrap, wrap_cells};

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
/// (`--`→`-`, …) is added only when `smart` (display rendering opts in).
pub(super) fn options(smart: bool) -> Options {
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
pub(super) fn ch_width(ch: char) -> usize {
    ch.width().unwrap_or(0)
}

/// Display width of a cell slice.
pub(super) fn cells_width(cells: &[Cell]) -> usize {
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

// --- inline conversion --------------------------------------------------

// --- block walk ---------------------------------------------------------

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

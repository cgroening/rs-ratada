//! A small, self-contained Markdown renderer for `ratatui`.
//!
//! Turns CommonMark (plus strikethrough, task lists, GFM tables/callouts and a
//! `==highlight==` extension) into styled [`Line`]s and [`Span`]s. The engine
//! depends only on `ratatui`, `pulldown-cmark` and `unicode-width` and carries
//! no colour policy: the host supplies a [`StyleSheet`]. [`StyleSheet::default`]
//! and [`StyleSheet::from_skin`] provide a ready look; [`MarkdownView`] and
//! [`viewer`] add scrolling and link navigation on top.
//!
//! Rendering is display-only: it hides Markdown markers and reflows text, so it
//! must not be used where a text cursor indexes the raw string (editing surfaces
//! keep showing the raw source, for which [`style_overlay`] exists).
//!
//! - [`render_block`] lays out a multi-line value (headings, lists, code,
//!   quotes, ...) wrapped to a width.
//! - [`render_inline`] renders a single-line value as inline-only markup,
//!   clipped to a width; block syntax stays literal.
//! - [`measure_block`] reports the line count [`render_block`] would produce.
//! - [`links`] extracts the hyperlinks of a source (for an "open link" key).
//! - [`clip_spans`] truncates a span list to a column budget.

// This is a self-contained CommonMark renderer lifted largely verbatim; a few
// pedantic style lints are relaxed module-wide (one justified block, not
// scattered allows) so the engine stays close to its origin instead of being
// reshaped per line. doc_markdown fires on proper nouns (CommonMark, GFM); the
// others are stylistic choices in the pulldown-cmark event walk.
#![allow(
    clippy::doc_markdown,
    clippy::match_same_arms,
    clippy::enum_glob_use,
    clippy::single_match_else,
    clippy::needless_pass_by_value
)]

mod render;
mod theme;
mod view;

use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

pub use view::{MarkdownView, viewer};

/// Style of one heading level: foreground, bold, and an optional full-width
/// background band (`None` = no band).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadingStyle {
    /// Text color (`None` inherits the base).
    pub fg: Option<Color>,
    /// Optional full-width background band (`None` = no band).
    pub bg: Option<Color>,
    /// Whether the heading text is bold.
    pub bold: bool,
}

/// Style of fenced/indented code blocks: a full-width band.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeBlockStyle {
    /// Text color of the code (`None` inherits the base).
    pub fg: Option<Color>,
    /// Background band color (`None` = no band).
    pub bg: Option<Color>,
    /// Colour for a fenced block's language title (reserved for a title bar).
    pub title_fg: Option<Color>,
}

impl CodeBlockStyle {
    /// The text style for code-block content.
    fn text_style(&self) -> Style {
        let mut style = Style::default();
        if let Some(fg) = self.fg {
            style = style.fg(fg);
        }
        style
    }

    /// The per-character style (fg + bg) for the edit-mode style overlay, where
    /// the band can't fill the full width (no reflow), so the colours sit behind
    /// the literal characters.
    fn char_style(&self) -> Style {
        let mut style = self.text_style();
        if let Some(bg) = self.bg {
            style = style.bg(bg);
        }
        style
    }
}

/// Style of blockquotes: text colours plus a left bar glyph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuoteStyle {
    /// Text color (`None` inherits the base).
    pub fg: Option<Color>,
    /// Background band color (`None` = none).
    pub bg: Option<Color>,
    /// Color of the left bar (`None` inherits the text color).
    pub bar_fg: Option<Color>,
    /// The left-bar glyph (e.g. `▎`).
    pub bar: String,
}

/// Style of list bullets: a colour and the glyphs cycled by nesting depth.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BulletStyle {
    /// Bullet color (`None` inherits the base).
    pub fg: Option<Color>,
    /// Bullet glyphs, cycled by nesting depth.
    pub glyphs: Vec<String>,
}

/// Style of task-list checkboxes: glyphs and colours per checked state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckboxStyle {
    /// Glyph for a checked box.
    pub checked: String,
    /// Glyph for an unchecked box.
    pub unchecked: String,
    /// Color for a checked box (`None` inherits the base).
    pub checked_fg: Option<Color>,
    /// Color for an unchecked box (`None` inherits the base).
    pub unchecked_fg: Option<Color>,
}

impl CheckboxStyle {
    fn checked_style(&self) -> Style {
        color_style(self.checked_fg)
    }

    fn unchecked_style(&self) -> Style {
        color_style(self.unchecked_fg)
    }
}

/// Style of horizontal rules: a colour and the glyph repeated full-width.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleStyle {
    /// Rule color (`None` inherits the base).
    pub fg: Option<Color>,
    /// The glyph repeated across the full width.
    pub glyph: String,
}

/// Foreground/background of one GFM callout kind (`> [!NOTE]` …).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalloutStyle {
    /// Text color (`None` inherits the base).
    pub fg: Option<Color>,
    /// Background color (`None` = none).
    pub bg: Option<Color>,
}

impl CalloutStyle {
    /// The per-character style (fg + bg) for the callout body and overlay.
    fn char_style(&self) -> Style {
        let mut style = Style::default();
        if let Some(fg) = self.fg {
            style = style.fg(fg);
        }
        if let Some(bg) = self.bg {
            style = style.bg(bg);
        }
        style
    }
}

/// Styles for the five GFM callout kinds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalloutTheme {
    /// Style for `> [!NOTE]`.
    pub note: CalloutStyle,
    /// Style for `> [!TIP]`.
    pub tip: CalloutStyle,
    /// Style for `> [!IMPORTANT]`.
    pub important: CalloutStyle,
    /// Style for `> [!WARNING]`.
    pub warning: CalloutStyle,
    /// Style for `> [!CAUTION]`.
    pub caution: CalloutStyle,
}

impl CalloutTheme {
    /// The style for a parsed callout kind.
    fn get(&self, kind: pulldown_cmark::BlockQuoteKind) -> &CalloutStyle {
        use pulldown_cmark::BlockQuoteKind::*;
        match kind {
            Note => &self.note,
            Tip => &self.tip,
            Important => &self.important,
            Warning => &self.warning,
            Caution => &self.caution,
        }
    }
}

/// The upper-case title shown above a callout block (Unicode-only, no icon).
pub(super) fn callout_label(
    kind: pulldown_cmark::BlockQuoteKind,
) -> &'static str {
    use pulldown_cmark::BlockQuoteKind::*;
    match kind {
        Note => "NOTE",
        Tip => "TIP",
        Important => "IMPORTANT",
        Warning => "WARNING",
        Caution => "CAUTION",
    }
}

/// The complete set of styles a host supplies to render Markdown. Every element
/// is described here, so the renderer itself stays free of any colour policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyleSheet {
    /// Style of ordinary body text.
    pub base: Style,
    /// Styles for heading levels H1..=H6 (index 0 = H1).
    pub headings: [HeadingStyle; 6],
    /// Patch applied inside `**strong**`.
    pub strong: Style,
    /// Patch applied inside `*emphasis*`.
    pub emphasis: Style,
    /// Patch applied inside `~~strikethrough~~`.
    pub strikethrough: Style,
    /// Patch applied inside `` `inline code` ``.
    pub inline_code: Style,
    /// Code-block band.
    pub code_block: CodeBlockStyle,
    /// Blockquote styling.
    pub quote: QuoteStyle,
    /// Patch applied inside `==highlight==`.
    pub highlight: Style,
    /// Patch applied to link text.
    pub link: Style,
    /// Horizontal-rule styling.
    pub rule: RuleStyle,
    /// List-bullet styling.
    pub bullet: BulletStyle,
    /// Task-checkbox styling.
    pub checkbox: CheckboxStyle,
    /// GFM callout (`> [!NOTE]` …) styling.
    pub callout: CalloutTheme,
    /// Border/separator colour for GFM tables.
    pub table_border: Option<Color>,
    /// Replace `--`/`...`/quotes with their typographic forms (display only).
    pub smart_punctuation: bool,
    /// Render a single source newline (CommonMark soft break) as a real line
    /// break instead of collapsing it to a space (display only).
    pub preserve_line_breaks: bool,
    /// Style for literal raw HTML (`<br>`, …).
    pub html: Style,
    /// Style for the overflow `…` ellipsis on clipped values.
    pub ellipsis: Style,
}

impl StyleSheet {
    /// The base text style for a heading of the given `level` (fg + bold).
    fn heading_style(&self, level: pulldown_cmark::HeadingLevel) -> Style {
        let head = &self.headings[heading_index(level)];
        let mut style = self.base;
        if let Some(fg) = head.fg {
            style = style.fg(fg);
        }
        if head.bold {
            style = style.add_modifier(Modifier::BOLD);
        }
        style
    }

    /// The optional background band for a heading of the given `level`.
    fn heading_bg(&self, level: pulldown_cmark::HeadingLevel) -> Option<Color> {
        self.headings[heading_index(level)].bg
    }

    /// The per-character heading style (fg + bold + optional bg) for the
    /// edit-mode overlay.
    fn heading_char_style(&self, level: pulldown_cmark::HeadingLevel) -> Style {
        let mut style = self.heading_style(level);
        if let Some(bg) = self.heading_bg(level) {
            style = style.bg(bg);
        }
        style
    }

    /// The per-character blockquote style (fg + bg) for the edit-mode overlay.
    fn quote_char_style(&self) -> Style {
        let mut style = Style::default();
        if let Some(fg) = self.quote.fg {
            style = style.fg(fg);
        }
        if let Some(bg) = self.quote.bg {
            style = style.bg(bg);
        }
        style
    }
}

/// A hyperlink found in a source, with its visible text and destination URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    /// The link's visible text.
    pub text: String,
    /// The link's destination URL.
    pub url: String,
}

/// Renders `src` as wrapped, decorated Markdown lines for a box of `width`
/// columns.
#[must_use]
pub fn render_block(
    src: &str,
    width: usize,
    sheet: &StyleSheet,
) -> Vec<Line<'static>> {
    render::render_block(src, width, sheet)
}

/// Renders `src` as inline-only Markdown spans clipped to `width` columns.
/// Block syntax (a leading `# `, `> `, ...) is shown literally.
#[must_use]
pub fn render_inline(
    src: &str,
    width: usize,
    sheet: &StyleSheet,
) -> Vec<Span<'static>> {
    render::render_inline(src, width, sheet)
}

/// The number of lines [`render_block`] produces for `src` at `width`.
#[must_use]
pub fn measure_block(src: &str, width: usize, sheet: &StyleSheet) -> usize {
    render::render_block(src, width, sheet).len()
}

/// The hyperlinks in `src`, in document order.
#[must_use]
pub fn links(src: &str) -> Vec<Link> {
    render::links(src)
}

/// A per-character style overlay for `src`: one [`Style`] per character, to be
/// patched onto the caller's base style. Markdown markers are kept (so the
/// styles align 1:1 with the raw characters and survive a text cursor), making
/// this the edit-mode counterpart of [`render_block`]/[`render_inline`].
/// Unstyled characters get [`Style::default`].
#[must_use]
pub fn style_overlay(src: &str, sheet: &StyleSheet) -> Vec<Style> {
    render::style_overlay(src, sheet)
}

/// Clips a span list to `max` display columns, appending the `ellipsis`-styled
/// `…` on overflow.
#[must_use]
pub fn clip_spans(
    spans: Vec<Span<'static>>,
    max: usize,
    ellipsis: Style,
) -> Vec<Span<'static>> {
    render::clip_spans(spans, max, ellipsis)
}

/// Maps a heading level to its zero-based index into [`StyleSheet::headings`].
fn heading_index(level: pulldown_cmark::HeadingLevel) -> usize {
    use pulldown_cmark::HeadingLevel::*;
    match level {
        H1 => 0,
        H2 => 1,
        H3 => 2,
        H4 => 3,
        H5 => 4,
        H6 => 5,
    }
}

/// A style with just a foreground colour, or the default when `None`.
fn color_style(color: Option<Color>) -> Style {
    match color {
        Some(fg) => Style::default().fg(fg),
        None => Style::default(),
    }
}

#[cfg(test)]
mod tests;

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
    pub fg: Option<Color>,
    pub bg: Option<Color>,
    pub bold: bool,
}

/// Style of fenced/indented code blocks: a full-width band.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeBlockStyle {
    pub fg: Option<Color>,
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
    pub fg: Option<Color>,
    pub bg: Option<Color>,
    pub bar_fg: Option<Color>,
    /// The left-bar glyph (e.g. `▎`).
    pub bar: String,
}

/// Style of list bullets: a colour and the glyphs cycled by nesting depth.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BulletStyle {
    pub fg: Option<Color>,
    pub glyphs: Vec<String>,
}

/// Style of task-list checkboxes: glyphs and colours per checked state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckboxStyle {
    pub checked: String,
    pub unchecked: String,
    pub checked_fg: Option<Color>,
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
    pub fg: Option<Color>,
    pub glyph: String,
}

/// Foreground/background of one GFM callout kind (`> [!NOTE]` …).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalloutStyle {
    pub fg: Option<Color>,
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
    pub note: CalloutStyle,
    pub tip: CalloutStyle,
    pub important: CalloutStyle,
    pub warning: CalloutStyle,
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
    pub text: String,
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
mod tests {
    use super::*;

    /// A plain, easily-asserted stylesheet: distinguishable colours, simple
    /// glyphs, no background bands (so line content is the bare text).
    fn sheet() -> StyleSheet {
        let heading = |fg: Color| HeadingStyle {
            fg: Some(fg),
            bg: None,
            bold: true,
        };
        StyleSheet {
            base: Style::default(),
            headings: [
                heading(Color::Rgb(1, 1, 1)),
                heading(Color::Rgb(2, 2, 2)),
                heading(Color::Rgb(3, 3, 3)),
                heading(Color::Rgb(4, 4, 4)),
                heading(Color::Rgb(5, 5, 5)),
                heading(Color::Rgb(6, 6, 6)),
            ],
            strong: Style::default().add_modifier(Modifier::BOLD),
            emphasis: Style::default().add_modifier(Modifier::ITALIC),
            strikethrough: Style::default().add_modifier(Modifier::CROSSED_OUT),
            inline_code: Style::default().fg(Color::Rgb(200, 0, 200)),
            code_block: CodeBlockStyle {
                fg: Some(Color::Rgb(0, 200, 200)),
                bg: Some(Color::Rgb(20, 20, 20)),
                title_fg: Some(Color::Rgb(240, 240, 130)),
            },
            quote: QuoteStyle {
                fg: Some(Color::Rgb(180, 180, 180)),
                bg: None,
                bar_fg: Some(Color::Rgb(100, 100, 100)),
                bar: "\u{258e}".to_string(),
            },
            highlight: Style::default()
                .fg(Color::Rgb(0, 0, 0))
                .bg(Color::Rgb(240, 240, 0)),
            link: Style::default()
                .fg(Color::Rgb(0, 120, 255))
                .add_modifier(Modifier::UNDERLINED),
            rule: RuleStyle {
                fg: Some(Color::Rgb(210, 140, 60)),
                glyph: "\u{2500}".to_string(),
            },
            bullet: BulletStyle {
                fg: Some(Color::Rgb(240, 240, 130)),
                glyphs: vec!["\u{25cf}".to_string(), "\u{25cb}".to_string()],
            },
            checkbox: CheckboxStyle {
                checked: "\u{2611}".to_string(),
                unchecked: "\u{2610}".to_string(),
                checked_fg: Some(Color::Rgb(150, 240, 150)),
                unchecked_fg: Some(Color::Rgb(240, 240, 130)),
            },
            callout: CalloutTheme {
                note: CalloutStyle {
                    fg: Some(Color::Rgb(0, 120, 255)),
                    bg: Some(Color::Rgb(10, 20, 40)),
                },
                tip: CalloutStyle {
                    fg: Some(Color::Rgb(0, 200, 0)),
                    bg: Some(Color::Rgb(0, 30, 10)),
                },
                important: CalloutStyle {
                    fg: Some(Color::Rgb(200, 120, 255)),
                    bg: Some(Color::Rgb(30, 10, 50)),
                },
                warning: CalloutStyle {
                    fg: Some(Color::Rgb(255, 170, 80)),
                    bg: Some(Color::Rgb(40, 25, 10)),
                },
                caution: CalloutStyle {
                    fg: Some(Color::Rgb(255, 100, 120)),
                    bg: Some(Color::Rgb(40, 15, 20)),
                },
            },
            table_border: Some(Color::Rgb(210, 140, 60)),
            smart_punctuation: false,
            html: Style::default().add_modifier(Modifier::DIM),
            ellipsis: Style::default().add_modifier(Modifier::DIM),
        }
    }

    /// Joins a rendered block's line texts for content assertions.
    fn texts(lines: &[Line<'static>]) -> Vec<String> {
        lines
            .iter()
            .map(|line| line.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect()
    }

    #[test]
    fn heading_drops_markers_and_styles_text() {
        let lines = render_block("# Title", 40, &sheet());
        assert_eq!(texts(&lines), vec!["Title".to_string()]);
        let style = lines[0].spans[0].style;
        assert_eq!(style.fg, Some(Color::Rgb(1, 1, 1)));
        assert!(style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn emphasis_and_strong_compose_modifiers() {
        let lines = render_block("a *b* **c**", 40, &sheet());
        let spans = &lines[0].spans;
        let joined: String = spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(joined, "a b c");
        let italic = spans
            .iter()
            .find(|s| s.content.as_ref() == "b")
            .expect("emphasis span");
        assert!(italic.style.add_modifier.contains(Modifier::ITALIC));
        let bold = spans
            .iter()
            .find(|s| s.content.as_ref() == "c")
            .expect("strong span");
        assert!(bold.style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn highlight_marker_is_dropped_and_styled() {
        let lines = render_block("x ==hot== y", 40, &sheet());
        let spans = &lines[0].spans;
        let joined: String = spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(joined, "x hot y");
        let hot = spans
            .iter()
            .find(|s| s.content.as_ref() == "hot")
            .expect("highlight span");
        assert_eq!(hot.style.bg, Some(Color::Rgb(240, 240, 0)));
    }

    #[test]
    fn highlight_does_not_fire_inside_inline_code() {
        let lines = render_block("`==x==`", 40, &sheet());
        let joined: String =
            lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(joined, "==x==");
    }

    #[test]
    fn highlight_requires_nonempty_marker_free_inner() {
        // Empty `====` and a `=` inside never match.
        let lines = render_block("==== a=b", 40, &sheet());
        let joined: String =
            lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(joined, "==== a=b");
    }

    #[test]
    fn inline_code_uses_code_style() {
        let lines = render_block("run `cargo`", 40, &sheet());
        let code = lines[0]
            .spans
            .iter()
            .find(|s| s.content.as_ref() == "cargo")
            .expect("code span");
        assert_eq!(code.style.fg, Some(Color::Rgb(200, 0, 200)));
    }

    #[test]
    fn bullets_render_with_glyph_by_depth() {
        let src = "- one\n- two";
        let lines = render_block(src, 40, &sheet());
        assert_eq!(
            texts(&lines),
            vec!["\u{25cf} one".to_string(), "\u{25cf} two".to_string()]
        );
    }

    #[test]
    fn ordered_list_numbers_items() {
        let lines = render_block("1. a\n2. b", 40, &sheet());
        assert_eq!(texts(&lines), vec!["1. a".to_string(), "2. b".to_string()]);
    }

    #[test]
    fn task_list_uses_checkbox_glyphs() {
        let lines = render_block("- [ ] todo\n- [x] done", 40, &sheet());
        assert_eq!(
            texts(&lines),
            vec!["\u{2610} todo".to_string(), "\u{2611} done".to_string()]
        );
    }

    #[test]
    fn strikethrough_sets_crossed_out() {
        let lines = render_block("~~gone~~", 40, &sheet());
        let span = &lines[0].spans[0];
        assert_eq!(span.content.as_ref(), "gone");
        assert!(span.style.add_modifier.contains(Modifier::CROSSED_OUT));
    }

    #[test]
    fn rule_fills_width_with_glyph() {
        let lines = render_block("---", 5, &sheet());
        assert_eq!(
            texts(&lines),
            vec!["\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}".to_string()]
        );
    }

    #[test]
    fn code_block_band_fills_width_and_keeps_text() {
        let src = "```\nlet x;\n```";
        let lines = render_block(src, 10, &sheet());
        assert_eq!(lines.len(), 1);
        let joined: String =
            lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        // Full-width band: text padded with spaces to 10 columns.
        assert_eq!(joined, "let x;    ");
        assert_eq!(lines[0].spans[0].style.bg, Some(Color::Rgb(20, 20, 20)));
    }

    #[test]
    fn quote_prefixes_a_bar() {
        let lines = render_block("> hi", 40, &sheet());
        let joined: String =
            lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(joined, "\u{258e} hi");
    }

    #[test]
    fn block_wraps_at_width() {
        let lines = render_block("alpha beta gamma", 7, &sheet());
        assert_eq!(
            texts(&lines),
            vec!["alpha".to_string(), "beta".to_string(), "gamma".to_string()]
        );
    }

    #[test]
    fn paragraphs_are_separated_by_a_blank_line() {
        let lines = render_block("a\n\nb", 40, &sheet());
        assert_eq!(
            texts(&lines),
            vec!["a".to_string(), String::new(), "b".to_string()]
        );
    }

    #[test]
    fn render_inline_strips_inline_markers() {
        let spans = render_inline("a **b** `c`", 40, &sheet());
        let joined: String = spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(joined, "a b c");
    }

    #[test]
    fn render_inline_keeps_block_syntax_literal() {
        let spans = render_inline("# not a heading", 40, &sheet());
        let joined: String = spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(joined, "# not a heading");
    }

    #[test]
    fn render_inline_clips_with_ellipsis() {
        let spans = render_inline("abcdefhij", 5, &sheet());
        let joined: String = spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(joined, "abcd\u{2026}");
    }

    #[test]
    fn measure_block_matches_line_count() {
        let src = "# h\n\nbody text here";
        assert_eq!(
            measure_block(src, 40, &sheet()),
            render_block(src, 40, &sheet()).len()
        );
    }

    #[test]
    fn links_are_extracted_in_order() {
        let src = "see [a](http://a) and [b](http://b)";
        let found = links(src);
        assert_eq!(
            found,
            vec![
                Link {
                    text: "a".to_string(),
                    url: "http://a".to_string()
                },
                Link {
                    text: "b".to_string(),
                    url: "http://b".to_string()
                },
            ]
        );
    }

    #[test]
    fn style_overlay_keeps_markers_and_styles_emphasis() {
        // `*x*` - all three chars (markers included) carry the italic modifier.
        let styles = style_overlay("*x*", &sheet());
        assert_eq!(styles.len(), 3);
        for style in &styles {
            assert!(style.add_modifier.contains(Modifier::ITALIC));
        }
    }

    #[test]
    fn style_overlay_styles_heading_including_hash() {
        let styles = style_overlay("# Hi", &sheet());
        assert_eq!(styles.len(), 4);
        // The `#` marker is styled with the H1 colour, like the rest.
        assert_eq!(styles[0].fg, Some(Color::Rgb(1, 1, 1)));
        assert!(styles[0].add_modifier.contains(Modifier::BOLD));
        assert_eq!(styles[3].fg, Some(Color::Rgb(1, 1, 1)));
    }

    #[test]
    fn style_overlay_highlights_with_markers() {
        let styles = style_overlay("==hot==", &sheet());
        assert_eq!(styles.len(), 7);
        for style in &styles {
            assert_eq!(style.bg, Some(Color::Rgb(240, 240, 0)));
        }
    }

    #[test]
    fn style_overlay_styles_inline_code_with_backticks() {
        let styles = style_overlay("`c`", &sheet());
        assert_eq!(styles.len(), 3);
        for style in &styles {
            assert_eq!(style.fg, Some(Color::Rgb(200, 0, 200)));
        }
    }

    #[test]
    fn style_overlay_leaves_plain_text_unstyled() {
        let styles = style_overlay("plain", &sheet());
        assert!(styles.iter().all(|style| *style == Style::default()));
    }

    #[test]
    fn style_overlay_handles_multibyte_chars() {
        // Umlaut before emphasis (`ä *x*`, 5 chars): the multi-byte char means
        // byte and char indices differ, so the byte->char mapping must align.
        let styles = style_overlay("\u{e4} *x*", &sheet());
        assert_eq!(styles.len(), 5);
        assert_eq!(styles[0], Style::default());
        assert_eq!(styles[1], Style::default());
        for style in &styles[2..5] {
            assert!(style.add_modifier.contains(Modifier::ITALIC));
        }
    }

    #[test]
    fn callout_renders_title_and_colours() {
        let lines = render_block("> [!WARNING]\n> careful", 40, &sheet());
        let warning_fg = Color::Rgb(255, 170, 80);
        // The title line carries the kind label in the warning colour.
        let title: String =
            lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(title.contains("WARNING"));
        assert!(
            lines[0]
                .spans
                .iter()
                .any(|s| s.style.fg == Some(warning_fg))
        );
        // The body line is coloured + backgrounded with the callout colours.
        let body = lines
            .iter()
            .find(|l| {
                l.spans
                    .iter()
                    .any(|s| s.content.as_ref().contains("careful"))
            })
            .expect("callout body line");
        assert!(
            body.spans
                .iter()
                .any(|s| s.style.bg == Some(Color::Rgb(40, 25, 10)))
        ); // warning bg
    }

    #[test]
    fn image_alt_text_styles_like_a_link_and_is_collectable() {
        let lines = render_block("![pic](http://img)", 40, &sheet());
        let alt = lines[0]
            .spans
            .iter()
            .find(|s| s.content.as_ref() == "pic")
            .expect("alt text span");
        assert!(alt.style.add_modifier.contains(Modifier::UNDERLINED));
        let found = links("![pic](http://img)");
        assert_eq!(found[0].url, "http://img");
    }

    #[test]
    fn code_block_shows_language_label() {
        let src = "```rust\nlet x;\n```";
        let lines = render_block(src, 20, &sheet());
        let label: String =
            lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(label.trim_end().starts_with("rust"));
        assert_eq!(lines[0].spans[0].style.fg, Some(Color::Rgb(240, 240, 130)));
        // The code line follows.
        let joined: String =
            lines[1].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(joined.starts_with("let x;"));
    }

    #[test]
    fn raw_html_is_rendered_literally() {
        let lines = render_block("see <br> end", 40, &sheet());
        let joined: String =
            lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(joined, "see <br> end");
    }

    #[test]
    fn smart_punctuation_replaces_when_enabled() {
        let mut sheet = sheet();
        sheet.smart_punctuation = true;
        let lines = render_block("a -- b ...", 40, &sheet);
        let joined: String =
            lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(!joined.contains("--"));
        assert!(joined.contains('\u{2026}')); // …
    }

    #[test]
    fn smart_punctuation_off_keeps_raw() {
        let lines = render_block("a -- b ...", 40, &sheet());
        let joined: String =
            lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(joined, "a -- b ...");
    }

    #[test]
    fn table_renders_columns_and_header_rule() {
        let src = "| a | b |\n|---|---|\n| 1 | 2 |";
        let lines = render_block(src, 40, &sheet());
        let all: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.as_ref())
            .collect();
        assert!(all.contains('\u{2502}')); // │ column border
        assert!(all.contains('a') && all.contains('1'));
        // The header separator line is present.
        assert!(lines.iter().any(|l| {
            l.spans.iter().any(|s| s.content.contains('\u{251c}'))
        }));
    }

    #[test]
    fn body_text_uses_the_configured_base_colour() {
        let mut sheet = sheet();
        sheet.base = Style::default().fg(Color::Rgb(10, 20, 30));
        let lines = render_block("plain text", 40, &sheet);
        assert_eq!(lines[0].spans[0].style.fg, Some(Color::Rgb(10, 20, 30)));
    }

    #[test]
    fn raw_html_uses_the_configured_html_style() {
        let mut sheet = sheet();
        sheet.html = Style::default().fg(Color::Rgb(90, 90, 90));
        let lines = render_block("a <br> b", 40, &sheet);
        let html = lines[0]
            .spans
            .iter()
            .find(|s| s.content.as_ref() == "<br>")
            .expect("html span");
        assert_eq!(html.style.fg, Some(Color::Rgb(90, 90, 90)));
    }

    #[test]
    fn ellipsis_uses_the_configured_style() {
        let mut sheet = sheet();
        sheet.ellipsis = Style::default().fg(Color::Rgb(200, 50, 50));
        let spans = render_inline("abcdefghij", 4, &sheet);
        let ell = spans.last().expect("ellipsis span");
        assert_eq!(ell.content.as_ref(), "\u{2026}");
        assert_eq!(ell.style.fg, Some(Color::Rgb(200, 50, 50)));
    }

    #[test]
    fn clip_spans_truncates_to_budget() {
        let spans = vec![Span::raw("hello world")];
        let clipped = clip_spans(spans, 4, Style::default());
        let joined: String =
            clipped.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(joined, "hel\u{2026}");
    }
}

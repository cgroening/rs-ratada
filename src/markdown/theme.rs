//! The standard Markdown [`StyleSheet`] and its `Skin`-aware variant.
//!
//! [`StyleSheet::default`] is the built-in look (a fixed, dark palette carried
//! over verbatim as the toolkit standard); [`StyleSheet::from_skin`] keeps those
//! colours but swaps in ASCII glyph fallbacks when the skin asks for them.
//!
//! The palette below is declared in [`crate::theme::Color`] and mapped to
//! ratatui through [`crate::style::to_ratatui`], like every other colour in the
//! kit - `style` stays the single seam. `StyleSheet` itself keeps ratatui types
//! in its fields, since it is handed straight to the renderer.
//!
//! These colours are the built-in Markdown theme and are deliberately *not*
//! taken from the active [`crate::theme::Palette`]: a Markdown document has its own semantic slots
//! (six heading levels, five callout kinds) that a UI palette does not carry.
//! A host that wants them themed builds its own [`StyleSheet`].

use ratatui::style::{Modifier, Style};

use super::{
    BulletStyle, CalloutStyle, CalloutTheme, CheckboxStyle, CodeBlockStyle,
    HeadingStyle, QuoteStyle, RuleStyle, StyleSheet,
};
use crate::{
    style::to_ratatui,
    theme::{Color, GlyphVariant, Skin},
};

// Glyphs: Unicode standard and ASCII fallbacks (nesting-cycled bullets).
const UNICODE_BULLETS: [&str; 4] =
    ["\u{25cf}", "\u{25cb}", "\u{25c6}", "\u{25c7}"]; // ● ○ ◆ ◇
const ASCII_BULLETS: [&str; 4] = ["*", "-", "+", "."];
const QUOTE_BAR: &str = "\u{258e}"; // ▎
const RULE_GLYPH: &str = "\u{2500}"; // ─
const CHECKBOX_CHECKED: &str = "\u{2611}"; // ☑
const CHECKBOX_UNCHECKED: &str = "\u{2610}"; // ☐

fn heading(fg: Color) -> HeadingStyle {
    HeadingStyle {
        fg: Some(to_ratatui(fg)),
        bg: None,
        bold: true,
    }
}

fn callout(fg: Color, bg: Color) -> CalloutStyle {
    CalloutStyle {
        fg: Some(to_ratatui(fg)),
        bg: Some(to_ratatui(bg)),
    }
}

fn glyphs(list: &[&str]) -> Vec<String> {
    list.iter().map(|glyph| (*glyph).to_string()).collect()
}

impl Default for StyleSheet {
    /// The built-in Markdown look: coloured bold headings, a code-block band, a
    /// quoted left bar, cycled bullet glyphs, task checkboxes and GFM callouts.
    fn default() -> Self {
        StyleSheet {
            base: Style::default(),
            headings: [
                heading(Color::hex("#6abeff")),
                heading(Color::hex("#ebd58c")),
                heading(Color::hex("#b9d5a2")),
                heading(Color::hex("#d3accc")),
                heading(Color::hex("#88c0d0")),
                heading(Color::hex("#d08770")),
            ],
            strong: Style::default()
                .fg(to_ratatui(Color::hex("#01e030")))
                .add_modifier(Modifier::BOLD),
            emphasis: Style::default()
                .fg(to_ratatui(Color::hex("#b27cde")))
                .add_modifier(Modifier::ITALIC),
            strikethrough: Style::default().add_modifier(Modifier::CROSSED_OUT),
            inline_code: Style::default()
                .fg(to_ratatui(Color::hex("#fb95ff")))
                .bg(to_ratatui(Color::hex("#342434"))),
            code_block: CodeBlockStyle {
                fg: Some(to_ratatui(Color::hex("#b3ffed"))),
                bg: Some(to_ratatui(Color::hex("#1f1f1f"))),
                title_fg: Some(to_ratatui(Color::hex("#f0f08b"))),
            },
            quote: QuoteStyle {
                fg: Some(to_ratatui(Color::hex("#b9d6da"))),
                bg: Some(to_ratatui(Color::hex("#151c22"))),
                bar_fg: Some(to_ratatui(Color::hex("#b9d6da"))),
                bar: QUOTE_BAR.to_string(),
            },
            highlight: Style::default()
                .fg(to_ratatui(Color::hex("#202020")))
                .bg(to_ratatui(Color::hex("#f0f08b"))),
            link: Style::default()
                .fg(to_ratatui(Color::hex("#7ecbff")))
                .add_modifier(Modifier::UNDERLINED),
            rule: RuleStyle {
                fg: Some(to_ratatui(Color::hex("#d38f44"))),
                glyph: RULE_GLYPH.to_string(),
            },
            bullet: BulletStyle {
                fg: Some(to_ratatui(Color::hex("#f0f08b"))),
                glyphs: glyphs(&UNICODE_BULLETS),
            },
            checkbox: CheckboxStyle {
                checked: CHECKBOX_CHECKED.to_string(),
                unchecked: CHECKBOX_UNCHECKED.to_string(),
                checked_fg: Some(to_ratatui(Color::hex("#9ef09b"))),
                unchecked_fg: Some(to_ratatui(Color::hex("#f0f08b"))),
            },
            callout: CalloutTheme {
                note: callout(Color::hex("#7fc6ff"), Color::hex("#0b2131")),
                tip: callout(Color::hex("#74ea76"), Color::hex("#041a10")),
                important: callout(
                    Color::hex("#d4b1ff"),
                    Color::hex("#1d1231"),
                ),
                warning: callout(Color::hex("#ffb287"), Color::hex("#261b0d")),
                caution: callout(Color::hex("#ff98a8"), Color::hex("#241114")),
            },
            table_border: Some(to_ratatui(Color::hex("#d38f44"))),
            smart_punctuation: false,
            preserve_line_breaks: false,
            html: Style::default().add_modifier(Modifier::DIM),
            ellipsis: Style::default().add_modifier(Modifier::DIM),
        }
    }
}

impl StyleSheet {
    /// The standard Markdown look adapted to `skin`'s glyph variant: the same
    /// colours as [`StyleSheet::default`], but with ASCII glyph fallbacks (`*`
    /// bullets, `[x]`/`[ ]` checkboxes, `|` quote bar, `-` rule) when the skin
    /// uses [`GlyphVariant::Ascii`].
    #[must_use]
    pub fn from_skin(skin: &Skin) -> StyleSheet {
        let mut sheet = StyleSheet::default();
        if skin.glyphs.variant == GlyphVariant::Ascii {
            sheet.quote.bar = "|".to_string();
            sheet.rule.glyph = "-".to_string();
            sheet.bullet.glyphs = glyphs(&ASCII_BULLETS);
            sheet.checkbox.checked = "[x]".to_string();
            sheet.checkbox.unchecked = "[ ]".to_string();
        }
        sheet
    }
}

#[cfg(test)]
mod tests {
    use ratatui::style::Color as RatColor;

    use super::*;
    use crate::theme::{ColorOverrides, Glyphs, Palette, ThemeRegistry};

    fn skin(variant: GlyphVariant) -> Skin {
        let base = ThemeRegistry::builtin().resolve("default");
        Skin::new(
            Palette::resolve(base, &ColorOverrides::default()),
            Glyphs::new(variant),
        )
    }

    #[test]
    fn default_carries_the_standard_colors_and_glyphs() {
        let sheet = StyleSheet::default();
        assert_eq!(
            sheet.headings[0].fg,
            Some(to_ratatui(Color::hex("#6abeff")))
        );
        assert!(sheet.headings[0].bold);
        assert_eq!(sheet.bullet.glyphs, glyphs(&UNICODE_BULLETS));
        assert_eq!(sheet.checkbox.checked, CHECKBOX_CHECKED);
        assert_eq!(
            sheet.callout.warning.fg,
            Some(to_ratatui(Color::hex("#ffb287")))
        );
    }

    #[test]
    fn from_skin_ascii_uses_fallback_glyphs_but_keeps_colors() {
        let sheet = StyleSheet::from_skin(&skin(GlyphVariant::Ascii));
        assert_eq!(sheet.bullet.glyphs, glyphs(&ASCII_BULLETS));
        assert_eq!(sheet.checkbox.unchecked, "[ ]");
        assert_eq!(sheet.rule.glyph, "-");
        // Colours stay the standard ones.
        assert_eq!(
            sheet.headings[0].fg,
            Some(to_ratatui(Color::hex("#6abeff")))
        );
        assert_eq!(sheet.bullet.fg, Some(to_ratatui(Color::hex("#f0f08b"))));
    }

    /// Pins the rendered colours to literal RGB rather than to the constants,
    /// which the other tests compare against and which would therefore follow
    /// any change silently. This is what makes moving the palette onto
    /// `theme::Color` provably colour-neutral.
    #[test]
    fn the_standard_sheet_renders_these_exact_colors() {
        let sheet = StyleSheet::default();
        assert_eq!(sheet.headings[0].fg, Some(RatColor::Rgb(0x6a, 0xbe, 0xff)));
        assert_eq!(sheet.headings[5].fg, Some(RatColor::Rgb(0xd0, 0x87, 0x70)));
        assert_eq!(sheet.strong.fg, Some(RatColor::Rgb(0x01, 0xe0, 0x30)));
        assert_eq!(sheet.emphasis.fg, Some(RatColor::Rgb(0xb2, 0x7c, 0xde)));
        assert_eq!(sheet.inline_code.fg, Some(RatColor::Rgb(0xfb, 0x95, 0xff)));
        assert_eq!(sheet.inline_code.bg, Some(RatColor::Rgb(0x34, 0x24, 0x34)));
        assert_eq!(sheet.link.fg, Some(RatColor::Rgb(0x7e, 0xcb, 0xff)));
        assert_eq!(sheet.code_block.bg, Some(RatColor::Rgb(0x1f, 0x1f, 0x1f)));
        assert_eq!(sheet.quote.fg, Some(RatColor::Rgb(0xb9, 0xd6, 0xda)));
        assert_eq!(sheet.highlight.bg, Some(RatColor::Rgb(0xf0, 0xf0, 0x8b)));
        assert_eq!(sheet.rule.fg, Some(RatColor::Rgb(0xd3, 0x8f, 0x44)));
        assert_eq!(sheet.bullet.fg, Some(RatColor::Rgb(0xf0, 0xf0, 0x8b)));
        assert_eq!(
            sheet.checkbox.checked_fg,
            Some(RatColor::Rgb(0x9e, 0xf0, 0x9b))
        );
        assert_eq!(sheet.table_border, Some(RatColor::Rgb(0xd3, 0x8f, 0x44)));
        assert_eq!(
            sheet.callout.note.fg,
            Some(RatColor::Rgb(0x7f, 0xc6, 0xff))
        );
        assert_eq!(
            sheet.callout.note.bg,
            Some(RatColor::Rgb(0x0b, 0x21, 0x31))
        );
        assert_eq!(
            sheet.callout.caution.bg,
            Some(RatColor::Rgb(0x24, 0x11, 0x14))
        );
    }

    #[test]
    fn from_skin_unicode_matches_the_default() {
        assert_eq!(
            StyleSheet::from_skin(&skin(GlyphVariant::Unicode)),
            StyleSheet::default(),
        );
    }
}

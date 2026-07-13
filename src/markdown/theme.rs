//! The standard Markdown [`StyleSheet`] and its `Skin`-aware variant.
//!
//! [`StyleSheet::default`] is the built-in look (a fixed, dark palette carried
//! over verbatim as the toolkit standard); [`StyleSheet::from_skin`] keeps those
//! colours but swaps in ASCII glyph fallbacks when the skin asks for them.

use ratatui::style::{Color, Modifier, Style};

use super::{
    BulletStyle, CalloutStyle, CalloutTheme, CheckboxStyle, CodeBlockStyle,
    HeadingStyle, QuoteStyle, RuleStyle, StyleSheet,
};
use crate::theme::{GlyphVariant, Skin};

// Heading foregrounds H1..H6.
const H1: Color = Color::Rgb(0x6a, 0xbe, 0xff);
const H2: Color = Color::Rgb(0xeb, 0xd5, 0x8c);
const H3: Color = Color::Rgb(0xb9, 0xd5, 0xa2);
const H4: Color = Color::Rgb(0xd3, 0xac, 0xcc);
const H5: Color = Color::Rgb(0x88, 0xc0, 0xd0);
const H6: Color = Color::Rgb(0xd0, 0x87, 0x70);

// Inline runs.
const STRONG: Color = Color::Rgb(0x01, 0xe0, 0x30);
const EMPHASIS: Color = Color::Rgb(0xb2, 0x7c, 0xde);
const INLINE_CODE_FG: Color = Color::Rgb(0xfb, 0x95, 0xff);
const INLINE_CODE_BG: Color = Color::Rgb(0x34, 0x24, 0x34);
const HIGHLIGHT_FG: Color = Color::Rgb(0x20, 0x20, 0x20);
const HIGHLIGHT_BG: Color = Color::Rgb(0xf0, 0xf0, 0x8b);
const LINK: Color = Color::Rgb(0x7e, 0xcb, 0xff);

// Blocks.
const CODE_FG: Color = Color::Rgb(0xb3, 0xff, 0xed);
const CODE_BG: Color = Color::Rgb(0x1f, 0x1f, 0x1f);
const CODE_TITLE: Color = Color::Rgb(0xf0, 0xf0, 0x8b);
const QUOTE: Color = Color::Rgb(0xb9, 0xd6, 0xda);
const QUOTE_BG: Color = Color::Rgb(0x15, 0x1c, 0x22);
const RULE: Color = Color::Rgb(0xd3, 0x8f, 0x44);
const BULLET: Color = Color::Rgb(0xf0, 0xf0, 0x8b);
const CHECKED: Color = Color::Rgb(0x9e, 0xf0, 0x9b);
const UNCHECKED: Color = Color::Rgb(0xf0, 0xf0, 0x8b);
const TABLE_BORDER: Color = Color::Rgb(0xd3, 0x8f, 0x44);

// Callout foreground/background pairs.
const NOTE_FG: Color = Color::Rgb(0x7f, 0xc6, 0xff);
const NOTE_BG: Color = Color::Rgb(0x0b, 0x21, 0x31);
const TIP_FG: Color = Color::Rgb(0x74, 0xea, 0x76);
const TIP_BG: Color = Color::Rgb(0x04, 0x1a, 0x10);
const IMPORTANT_FG: Color = Color::Rgb(0xd4, 0xb1, 0xff);
const IMPORTANT_BG: Color = Color::Rgb(0x1d, 0x12, 0x31);
const WARNING_FG: Color = Color::Rgb(0xff, 0xb2, 0x87);
const WARNING_BG: Color = Color::Rgb(0x26, 0x1b, 0x0d);
const CAUTION_FG: Color = Color::Rgb(0xff, 0x98, 0xa8);
const CAUTION_BG: Color = Color::Rgb(0x24, 0x11, 0x14);

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
        fg: Some(fg),
        bg: None,
        bold: true,
    }
}

fn callout(fg: Color, bg: Color) -> CalloutStyle {
    CalloutStyle {
        fg: Some(fg),
        bg: Some(bg),
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
                heading(H1),
                heading(H2),
                heading(H3),
                heading(H4),
                heading(H5),
                heading(H6),
            ],
            strong: Style::default().fg(STRONG).add_modifier(Modifier::BOLD),
            emphasis: Style::default()
                .fg(EMPHASIS)
                .add_modifier(Modifier::ITALIC),
            strikethrough: Style::default().add_modifier(Modifier::CROSSED_OUT),
            inline_code: Style::default().fg(INLINE_CODE_FG).bg(INLINE_CODE_BG),
            code_block: CodeBlockStyle {
                fg: Some(CODE_FG),
                bg: Some(CODE_BG),
                title_fg: Some(CODE_TITLE),
            },
            quote: QuoteStyle {
                fg: Some(QUOTE),
                bg: Some(QUOTE_BG),
                bar_fg: Some(QUOTE),
                bar: QUOTE_BAR.to_string(),
            },
            highlight: Style::default().fg(HIGHLIGHT_FG).bg(HIGHLIGHT_BG),
            link: Style::default().fg(LINK).add_modifier(Modifier::UNDERLINED),
            rule: RuleStyle {
                fg: Some(RULE),
                glyph: RULE_GLYPH.to_string(),
            },
            bullet: BulletStyle {
                fg: Some(BULLET),
                glyphs: glyphs(&UNICODE_BULLETS),
            },
            checkbox: CheckboxStyle {
                checked: CHECKBOX_CHECKED.to_string(),
                unchecked: CHECKBOX_UNCHECKED.to_string(),
                checked_fg: Some(CHECKED),
                unchecked_fg: Some(UNCHECKED),
            },
            callout: CalloutTheme {
                note: callout(NOTE_FG, NOTE_BG),
                tip: callout(TIP_FG, TIP_BG),
                important: callout(IMPORTANT_FG, IMPORTANT_BG),
                warning: callout(WARNING_FG, WARNING_BG),
                caution: callout(CAUTION_FG, CAUTION_BG),
            },
            table_border: Some(TABLE_BORDER),
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
        assert_eq!(sheet.headings[0].fg, Some(H1));
        assert!(sheet.headings[0].bold);
        assert_eq!(sheet.bullet.glyphs, glyphs(&UNICODE_BULLETS));
        assert_eq!(sheet.checkbox.checked, CHECKBOX_CHECKED);
        assert_eq!(sheet.callout.warning.fg, Some(WARNING_FG));
    }

    #[test]
    fn from_skin_ascii_uses_fallback_glyphs_but_keeps_colors() {
        let sheet = StyleSheet::from_skin(&skin(GlyphVariant::Ascii));
        assert_eq!(sheet.bullet.glyphs, glyphs(&ASCII_BULLETS));
        assert_eq!(sheet.checkbox.unchecked, "[ ]");
        assert_eq!(sheet.rule.glyph, "-");
        // Colours stay the standard ones.
        assert_eq!(sheet.headings[0].fg, Some(H1));
        assert_eq!(sheet.bullet.fg, Some(BULLET));
    }

    #[test]
    fn from_skin_unicode_matches_the_default() {
        assert_eq!(
            StyleSheet::from_skin(&skin(GlyphVariant::Unicode)),
            StyleSheet::default(),
        );
    }
}

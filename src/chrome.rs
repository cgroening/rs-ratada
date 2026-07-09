//! Shared chrome: the borderless view panels and the modal frame.
//!
//! Centralises framing so views and widgets never build blocks inline. A
//! [`panel`] is a borderless block whose content is inset one cell; a
//! [`modal_block`] is a rounded, bordered box with a filled background and an
//! inset title. [`render_badge`] is the one seam every `position/total` (or
//! percent) indicator goes through, painting it into a frame's bottom border.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Padding},
};

use super::style;
use crate::theme::Skin;

/// How far a modal's fill is lifted above the full-screen `background`, so the
/// box reads as an elevated surface against the dimmed backdrop behind it.
const MODAL_BG_LIFT: f32 = 0.06;

/// The container block for a view section: a borderless block that insets its
/// content by one cell all around, so a filled column (see [`menu_panel`]) keeps
/// its content off the panel edges.
pub fn panel() -> Block<'static> {
    Block::default().padding(Padding::uniform(1))
}

/// Like [`panel`], but filled with the `panel` color so a list/menu column reads
/// as its own panel against the body.
pub fn menu_panel(skin: &Skin) -> Block<'static> {
    panel().style(style::bg(skin.palette.panel))
}

/// The shared modal frame: a rounded, bordered box with a filled background and
/// an inset title. Used by every blocking modal widget. The fill is lifted
/// above `background` (see [`MODAL_BG_LIFT`]) so the box stands out from the
/// dimmed view behind it.
pub fn modal_block(skin: &Skin, title: &str) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(style::border(&skin.palette))
        .style(style::bg(skin.palette.background.lighten(MODAL_BG_LIFT)))
        .title(border_title(
            skin,
            title,
            style::accent(&skin.palette).add_modifier(Modifier::BOLD),
        ))
}

/// The single source of truth for a titled rounded frame: builds the inset
/// title line that reads as part of the top border (`╭─ Title ───`).
///
/// The leading `─ ` always keeps the border color so the connecting stroke
/// blends into the frame; only the trimmed title itself takes `label`. Hand the
/// result to `Block::title(...)` so every box titles the same way instead of a
/// flush `╭ Title` — the blessed alternative to a bare `.title("Title")`.
///
/// # Examples
///
/// ```
/// use ratada::chrome::border_title;
/// use ratada::style;
/// use ratada::theme::{
///     ColorOverrides, GlyphVariant, Glyphs, Palette, Skin, ThemeRegistry,
/// };
///
/// let base = ThemeRegistry::builtin().resolve("default");
/// let palette = Palette::resolve(base, &ColorOverrides::default());
/// let skin = Skin::new(palette, Glyphs::new(GlyphVariant::Unicode));
/// let line = border_title(&skin, "Info", style::accent(&skin.palette));
/// assert!(line.to_string().starts_with("\u{2500} Info"));
/// ```
pub fn border_title(skin: &Skin, title: &str, label: Style) -> Line<'static> {
    Line::from(vec![
        Span::styled("\u{2500} ", style::border(&skin.palette)),
        Span::styled(format!("{} ", title.trim()), label),
    ])
}

/// The bottom-right badge shown inside a boxed widget's border.
#[derive(Debug, Clone, Default)]
pub enum Badge {
    /// The widget's own automatic count (characters, entries, rows).
    #[default]
    Auto,
    /// A caller-supplied label, overriding the automatic one.
    Text(String),
    /// No badge at all.
    Hidden,
}

/// Optional decoration for a boxed widget: a caption in the top border and a
/// badge in the bottom-right border. Shared by every boxable widget through
/// [`framed_decor`] so the look stays consistent (SSOT).
#[derive(Debug, Clone, Default)]
pub struct BoxDecor {
    /// The caption inset in the top border, if any.
    pub caption: Option<String>,
    /// The bottom-right badge (automatic count by default).
    pub badge: Badge,
}

impl BoxDecor {
    /// An empty decoration: no caption, automatic badge.
    ///
    /// # Examples
    ///
    /// ```
    /// use ratada::chrome::BoxDecor;
    ///
    /// // Caption in the top border; a fixed bottom-right badge.
    /// let decor = BoxDecor::new().caption("Notes").badge("draft");
    /// ```
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the caption shown in the top border.
    #[must_use]
    pub fn caption(mut self, caption: impl Into<String>) -> Self {
        self.caption = Some(caption.into());
        self
    }

    /// Overrides the bottom-right badge with a fixed label.
    #[must_use]
    pub fn badge(mut self, text: impl Into<String>) -> Self {
        self.badge = Badge::Text(text.into());
        self
    }

    /// Hides the bottom-right badge.
    #[must_use]
    pub fn no_badge(mut self) -> Self {
        self.badge = Badge::Hidden;
        self
    }

    /// The badge label to show given the widget's automatic value, or `None`
    /// when hidden (or when `Auto` and the automatic value is empty).
    fn badge_text<'a>(&'a self, auto: &'a str) -> Option<&'a str> {
        match &self.badge {
            Badge::Auto => (!auto.is_empty()).then_some(auto),
            Badge::Text(text) => Some(text.as_str()),
            Badge::Hidden => None,
        }
    }
}

/// Renders a rounded accent box with `decor`'s caption inset in the top border
/// and its badge in the bottom-right border, then returns the inner content
/// area. The single framing seam for boxable widgets; `auto_badge` is the
/// widget's own count, used only when the badge is [`Badge::Auto`].
pub fn framed_decor(
    frame: &mut Frame,
    area: Rect,
    skin: &Skin,
    decor: &BoxDecor,
    auto_badge: &str,
) -> Rect {
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(style::border(&skin.palette))
        .padding(Padding::horizontal(1));
    if let Some(caption) = &decor.caption {
        // Box captions are bold to set them apart from plain modal titles.
        let label = style::accent(&skin.palette).add_modifier(Modifier::BOLD);
        block = block.title(border_title(skin, caption, label));
    }
    let badge = decor.badge_text(auto_badge).unwrap_or_default();
    let inner = block.inner(area);
    frame.render_widget(block, area);
    render_badge(frame, area, skin, badge);
    inner
}

/// The `position/total` badge label (1-based), e.g. `"3/12"`. An empty label
/// means "nothing to count", which [`Badge::Auto`] and [`render_badge`] both
/// treat as "no badge".
///
/// # Examples
///
/// ```
/// use ratada::chrome::position_badge;
///
/// assert_eq!(position_badge(2, 5), "3/5");
/// assert_eq!(position_badge(0, 0), "");
/// ```
pub fn position_badge(selected: usize, total: usize) -> String {
    if total == 0 {
        return String::new();
    }
    format!("{}/{}", selected + 1, total)
}

/// Paints `text` as a badge into the bottom-right of `area`'s border row, so it
/// reads as part of the frame (`─ 3/12 ─╯`). The single seam every badge goes
/// through, whether the frame comes from [`framed_decor`] or from a popup's
/// [`modal_block`].
///
/// A no-op for an empty label, an empty area, or an area too narrow to hold the
/// badge between its two border corners. Call it *after* the frame's block has
/// been rendered; the badge only touches the border row, never the content.
pub fn render_badge(frame: &mut Frame, area: Rect, skin: &Skin, text: &str) {
    if text.trim().is_empty() || area.height == 0 {
        return;
    }
    let line = badge_line(skin, text);
    let width = line.width() as u16;
    // The two border corners stay untouched.
    if width + 2 > area.width {
        return;
    }
    let x = area.right() - 1 - width;
    let y = area.bottom() - 1;
    // Written straight into the buffer (rather than via a `Paragraph`) so the
    // frame's own background shows through the badge cells.
    frame.buffer_mut().set_line(x, y, &line, width);
}

/// The bottom-right badge line reading as part of the bottom border
/// (`─ 12/80 ─╯`): the connecting dashes in the border color (one on each side
/// so it joins the corner), the count in muted chrome text.
fn badge_line(skin: &Skin, text: &str) -> Line<'static> {
    let border = style::border(&skin.palette);
    Line::from(vec![
        Span::styled("\u{2500} ", border),
        Span::styled(format!("{} ", text.trim()), style::muted(&skin.palette)),
        Span::styled("\u{2500}", border),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn position_badge_is_one_based() {
        assert_eq!(position_badge(0, 12), "1/12");
        assert_eq!(position_badge(11, 12), "12/12");
    }

    #[test]
    fn position_badge_is_empty_without_items() {
        assert!(position_badge(0, 0).is_empty());
    }
}

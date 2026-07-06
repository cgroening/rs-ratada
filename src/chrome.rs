//! Shared chrome: the framing that differs between the display modes.
//!
//! Centralises the one decision "framed or not" so views and widgets never
//! branch on [`Mode`](crate::theme::Mode) inline. In `Boxed` mode a [`panel`]
//! is a rounded, accent-bordered box with an inset title; in `Minimal` mode it
//! is a no-op frame, so content fills the area exactly as before.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Padding},
};

use super::style;
use crate::theme::Skin;

/// The container block for a view section. `Boxed` returns a rounded accent box
/// with `title`; `Minimal` returns an empty block whose inner area equals the
/// outer one, so existing layouts are unchanged. `Panels` returns a borderless
/// block that just insets its content by one cell all around, so a filled
/// column (see [`menu_panel`]) keeps its content off the panel edges.
pub fn panel(skin: &Skin, title: &str) -> Block<'static> {
    if skin.is_panels() {
        return Block::default().padding(Padding::uniform(1));
    }
    if !skin.is_boxed() {
        return Block::default();
    }
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(style::border(&skin.palette))
        .padding(Padding::horizontal(1))
        .title(title_span(skin, title))
}

/// Like [`panel`], but in `Panels` mode the box is filled with the `surface`
/// color so a list/menu column reads as its own panel against the body.
pub fn menu_panel(skin: &Skin, title: &str) -> Block<'static> {
    let block = panel(skin, title);
    if skin.is_panels() {
        block.style(style::bg(skin.palette.panel))
    } else {
        block
    }
}

/// The shared modal frame: a rounded accent border with a filled background and
/// an inset title. Unlike [`panel`], a modal is always framed; in `Boxed` mode
/// the body is padded and the title bold. Used by every blocking modal widget.
pub fn modal_block(skin: &Skin, title: &str) -> Block<'static> {
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(style::border(&skin.palette))
        .style(style::bg(skin.palette.background))
        .title(modal_title(skin, title));
    if skin.is_boxed() {
        block = block.padding(Padding::horizontal(1));
    }
    block
}

/// An inset title that reads as part of the top border (`╭─ Title ───`): the
/// connecting dash keeps the border color so the frame stays uniform, and the
/// label is accented (bold in `Boxed` mode).
fn modal_title(skin: &Skin, title: &str) -> Line<'static> {
    let mut label = style::accent(&skin.palette);
    if skin.is_boxed() {
        label = label.add_modifier(Modifier::BOLD);
    }
    title_line(skin, title, label)
}

/// An inset title that reads as part of the top border (`╭─ Title ───`): the
/// connecting dash keeps the border color, the label is accented and bold.
fn title_span(skin: &Skin, title: &str) -> Line<'static> {
    let label = style::accent(&skin.palette).add_modifier(Modifier::BOLD);
    title_line(skin, title, label)
}

/// Builds the inset title line: the leading `─ ` in the border color, then the
/// trimmed title in `label` style.
fn title_line(skin: &Skin, title: &str, label: Style) -> Line<'static> {
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
    pub caption: Option<String>,
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
        block = block.title(title_span(skin, caption));
    }
    if let Some(badge) = decor.badge_text(auto_badge) {
        block =
            block.title_bottom(Line::from(badge_span(badge)).right_aligned());
    }
    let inner = block.inner(area);
    frame.render_widget(block, area);
    inner
}

/// The bottom-right badge span: a dim, padded label reading as part of the
/// bottom border (`─ 12/80 ─╯`).
fn badge_span(text: &str) -> Span<'static> {
    Span::styled(format!("\u{2500} {} ", text.trim()), style::dim())
}

//! Shared chrome: the borderless view panels and the modal frame.
//!
//! Centralises framing so views and widgets never build blocks inline. A
//! [`panel`] is a borderless block whose content is inset one cell; a
//! [`modal_block`] is a rounded, bordered box with a filled background and an
//! inset title.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Padding},
};

use super::style;
use crate::theme::Skin;

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
/// an inset title. Used by every blocking modal widget.
pub fn modal_block(skin: &Skin, title: &str) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(style::border(&skin.palette))
        .style(style::bg(skin.palette.background))
        .title(modal_title(skin, title))
}

/// An inset title that reads as part of the top border (`╭─ Title ───`): the
/// connecting dash keeps the border color so the frame stays uniform, and the
/// label is accented.
fn modal_title(skin: &Skin, title: &str) -> Line<'static> {
    title_line(skin, title, style::accent(&skin.palette))
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

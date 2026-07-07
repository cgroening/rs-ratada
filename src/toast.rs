//! Transient toast notifications: timed, stacked messages in semantic colors.
//!
//! Push messages onto a [`Toasts`] stack; call [`Toasts::prune`] on each tick
//! to drop expired ones, and [`Toasts::render`] to draw the live stack.

use std::time::{Duration, Instant};

use ratatui::{
    Frame,
    layout::Rect,
    style::Modifier,
    text::Span,
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap},
};

use super::style;
use crate::theme::{Color, Skin};

/// How long a toast stays visible by default.
const DEFAULT_TTL: Duration = Duration::from_secs(4);
/// Width of a toast box.
const TOAST_WIDTH: u16 = 32;
/// Height of a toast box (border + one wrapped line).
const TOAST_HEIGHT: u16 = 3;

/// The severity of a toast, mapped to a semantic palette color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    /// Neutral information.
    Info,
    /// A successful action.
    Success,
    /// A non-fatal warning.
    Warning,
    /// A failure.
    Error,
}

impl ToastKind {
    fn color(self, skin: &Skin) -> Color {
        match self {
            ToastKind::Info => skin.palette.info,
            ToastKind::Success => skin.palette.success,
            ToastKind::Warning => skin.palette.warning,
            ToastKind::Error => skin.palette.error,
        }
    }

    fn label(self) -> &'static str {
        match self {
            ToastKind::Info => "info",
            ToastKind::Success => "success",
            ToastKind::Warning => "warning",
            ToastKind::Error => "error",
        }
    }
}

struct Toast {
    kind: ToastKind,
    message: String,
    expires: Instant,
}

/// A stack of timed toast notifications.
#[derive(Default)]
pub struct Toasts {
    items: Vec<Toast>,
}

impl Toasts {
    /// An empty stack.
    pub fn new() -> Self {
        Self::default()
    }

    /// Pushes a toast that disappears after the default time-to-live.
    pub fn push(&mut self, kind: ToastKind, message: impl Into<String>) {
        self.push_with_ttl(kind, message, DEFAULT_TTL);
    }

    /// Pushes a toast with an explicit time-to-live.
    pub fn push_with_ttl(
        &mut self,
        kind: ToastKind,
        message: impl Into<String>,
        ttl: Duration,
    ) {
        self.items.push(Toast {
            kind,
            message: message.into(),
            expires: Instant::now() + ttl,
        });
    }

    /// Drops toasts whose time-to-live has elapsed. Call on each tick.
    pub fn prune(&mut self) {
        let now = Instant::now();
        self.items.retain(|toast| toast.expires > now);
    }

    /// Whether no toasts are currently held.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Renders the live toasts stacked from the top-right of `area`.
    pub fn render(&self, frame: &mut Frame, area: Rect, skin: &Skin) {
        let now = Instant::now();
        let width = TOAST_WIDTH.min(area.width);
        let mut y = area.y;
        for toast in self.items.iter().filter(|t| t.expires > now) {
            if y + TOAST_HEIGHT > area.y + area.height {
                break;
            }
            let rect = Rect {
                x: area.x + area.width.saturating_sub(width),
                y,
                width,
                height: TOAST_HEIGHT,
            };
            let color = toast.kind.color(skin);
            let block = Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(style::fg(color))
                .style(style::bg(skin.palette.background))
                .title(Span::styled(
                    format!("\u{2500} {} ", toast.kind.label()),
                    style::fg(color).add_modifier(Modifier::BOLD),
                ));
            frame.render_widget(Clear, rect);
            frame.render_widget(
                Paragraph::new(toast.message.clone())
                    .block(block)
                    .wrap(Wrap { trim: true }),
                rect,
            );
            y += TOAST_HEIGHT;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prune_drops_expired_but_keeps_fresh() {
        let mut toasts = Toasts::new();
        toasts.push_with_ttl(ToastKind::Error, "boom", Duration::ZERO);
        toasts.push(ToastKind::Info, "hello");
        toasts.prune();
        assert!(!toasts.is_empty()); // the fresh one remains
    }

    #[test]
    fn prune_empties_when_all_expired() {
        let mut toasts = Toasts::new();
        toasts.push_with_ttl(ToastKind::Warning, "x", Duration::ZERO);
        toasts.prune();
        assert!(toasts.is_empty());
    }
}

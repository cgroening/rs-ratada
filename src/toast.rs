//! Transient toast notifications: timed, stacked messages in semantic colors.
//!
//! Push messages onto a [`Toasts`] stack; call [`Toasts::prune`] on each tick
//! to drop expired ones, and [`Toasts::render`] to draw the live stack.
//!
//! A toast box has a fixed width and grows in height to fit its word-wrapped
//! message, up to six lines; a longer message ends with an ellipsis, so one
//! toast can never fill the stack.

use std::time::{Duration, Instant};

use ratatui::{
    Frame,
    layout::Rect,
    style::Modifier,
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
};

use super::{chrome, style};
use crate::text;
use crate::theme::{Color, Skin};

/// How long a toast stays visible by default.
const DEFAULT_TTL: Duration = Duration::from_secs(4);
/// Width of a toast box.
const TOAST_WIDTH: u16 = 32;
/// Rows the box border occupies (top plus bottom).
const BORDER_ROWS: u16 = 2;
/// Most message lines one toast shows; the rest is cut with an ellipsis.
const MAX_TOAST_LINES: usize = 6;

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

    /// Renders the live toasts stacked from the top-right of `area`, each box
    /// as tall as its wrapped message needs.
    pub fn render(&self, frame: &mut Frame, area: Rect, skin: &Skin) {
        let now = Instant::now();
        let width = TOAST_WIDTH.min(area.width);
        let bottom = area.y + area.height;
        let mut y = area.y;
        for toast in self.items.iter().filter(|t| t.expires > now) {
            let lines = body_lines(&toast.message, width);
            let wanted = u16::try_from(lines.len()).unwrap_or(u16::MAX);
            // A box that no longer fits whole is clipped to the rows that are
            // left rather than dropped; below one message row nothing shows.
            let height = wanted
                .saturating_add(BORDER_ROWS)
                .min(bottom.saturating_sub(y));
            if height <= BORDER_ROWS {
                break;
            }
            let rect = Rect {
                x: area.x + area.width.saturating_sub(width),
                y,
                width,
                height,
            };
            let color = toast.kind.color(skin);
            let label = style::fg(color).add_modifier(Modifier::BOLD);
            let block = Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(style::fg(color))
                .style(style::bg(skin.palette.background))
                .title(chrome::border_title(skin, toast.kind.label(), label));
            frame.render_widget(Clear, rect);
            // The lines are wrapped already, so no `Wrap` here: measuring and
            // rendering must not use two different wrap implementations.
            let body = Paragraph::new(lines.join("\n")).block(block);
            frame.render_widget(body, rect);
            y += height;
        }
    }
}

/// The message word-wrapped into the box's inner width, capped at
/// [`MAX_TOAST_LINES`] lines; the last kept line then ends with an ellipsis.
fn body_lines(message: &str, width: u16) -> Vec<String> {
    let inner = usize::from(width.saturating_sub(BORDER_ROWS));
    let mut lines = text::wrap(message, inner);
    if lines.len() <= MAX_TOAST_LINES {
        return lines;
    }
    lines.truncate(MAX_TOAST_LINES);
    if let Some(last) = lines.last_mut() {
        // Appending the ellipsis marks the cut even when the last kept line
        // ended short; `truncate` re-clips it when that pushes it over `inner`.
        *last = text::truncate(&format!("{last}\u{2026}"), inner);
    }
    lines
}

#[cfg(test)]
mod tests {
    use unicode_width::UnicodeWidthStr;

    use super::*;

    /// The inner width of a default-width box.
    const INNER: usize = (TOAST_WIDTH - BORDER_ROWS) as usize;

    #[test]
    fn a_short_message_stays_on_one_line() {
        assert_eq!(body_lines("saved", TOAST_WIDTH), vec!["saved"]);
    }

    #[test]
    fn a_long_message_wraps_into_the_inner_width() {
        let message = "switch to custom order (t) to reorder";
        let lines = body_lines(message, TOAST_WIDTH);
        assert_eq!(lines.len(), 2);
        assert!(lines.iter().all(|line| line.width() <= INNER));
        assert_eq!(lines.join(" "), message, "no word is lost");
    }

    #[test]
    fn a_very_long_message_is_capped_with_an_ellipsis() {
        let message = "word ".repeat(100);
        let lines = body_lines(&message, TOAST_WIDTH);
        assert_eq!(lines.len(), MAX_TOAST_LINES);
        let last = lines.last().expect("the capped body has lines");
        assert!(last.ends_with('\u{2026}'), "last line: {last}");
        assert!(last.width() <= INNER);
    }

    #[test]
    fn a_wide_glyph_counts_as_two_columns() {
        // 20 double-width glyphs are 40 columns, so they cannot share a line.
        let lines = body_lines(&"世".repeat(20), TOAST_WIDTH);
        assert_eq!(lines.len(), 2);
        assert!(lines.iter().all(|line| line.width() <= INNER));
    }

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

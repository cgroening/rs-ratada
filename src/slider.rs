//! A numeric slider/stepper modal: pick a value in a bounded range.

use std::io;

use crossterm::event::KeyCode;
use ratatui::{
    Frame,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use super::{
    layout::centered_rect,
    modal::ModalSignal,
    overlay::{self, PopupFlow, popup},
    shortcut_hints, style,
    terminal::Tui,
};
use crate::theme::Skin;

const BOX_WIDTH: u16 = 40;

/// The bounds and step of a [`slider`].
#[derive(Debug, Clone, Copy)]
pub struct SliderConfig {
    pub min: i64,
    pub max: i64,
    pub step: i64,
    pub initial: i64,
}

/// Lets the user pick an integer in `cfg.min..=cfg.max`. `Left`/`Right` (or
/// `h`/`l`) step by `cfg.step`, `Home`/`End` jump to the bounds, `Enter`
/// confirms and `Esc` cancels.
pub fn slider(
    tui: &mut Tui,
    skin: &Skin,
    title: &str,
    cfg: SliderConfig,
    render_bg: impl Fn(&mut Frame),
) -> io::Result<ModalSignal<i64>> {
    let mut value = cfg.initial.clamp(cfg.min, cfg.max);
    popup(
        tui,
        &mut value,
        |area, _| {
            let rows = body_lines(skin, &cfg, cfg.initial).len() as u16 + 2;
            centered_rect(BOX_WIDTH, rows, area)
        },
        |frame, _| render_bg(frame),
        |frame, rect, value: &i64| {
            let inner = overlay::framed(frame, rect, skin, title);
            frame.render_widget(
                Paragraph::new(body_lines(skin, &cfg, *value)),
                inner,
            );
        },
        |value, key| match key.code {
            KeyCode::Left | KeyCode::Char('h') => {
                *value = step_value(*value, -cfg.step, cfg.min, cfg.max);
                PopupFlow::Continue
            }
            KeyCode::Right | KeyCode::Char('l') => {
                *value = step_value(*value, cfg.step, cfg.min, cfg.max);
                PopupFlow::Continue
            }
            KeyCode::Home => {
                *value = cfg.min;
                PopupFlow::Continue
            }
            KeyCode::End => {
                *value = cfg.max;
                PopupFlow::Continue
            }
            KeyCode::Enter => PopupFlow::Done(*value),
            KeyCode::Esc => PopupFlow::Cancelled,
            _ => PopupFlow::Continue,
        },
    )
}

/// Moves `value` by `delta`, clamped to `min..=max`.
pub(crate) fn step_value(value: i64, delta: i64, min: i64, max: i64) -> i64 {
    value.saturating_add(delta).clamp(min, max)
}

fn body_lines(
    skin: &Skin,
    cfg: &SliderConfig,
    value: i64,
) -> Vec<Line<'static>> {
    let palette = &skin.palette;
    // The bar sits inside the box border (1 cell each side) and one padding
    // cell each side, so it is 4 columns narrower than the box.
    let bar_width = (BOX_WIDTH as usize).saturating_sub(4).max(1);
    let ratio = if cfg.max > cfg.min {
        (value - cfg.min) as f64 / (cfg.max - cfg.min) as f64
    } else {
        0.0
    };
    let filled = (ratio * bar_width as f64).round() as usize;
    let accent = style::to_ratatui(palette.accent);
    let track = style::to_ratatui(palette.selection);
    let bar: Vec<Span> = (0..bar_width)
        .map(|index| {
            let background = if index < filled { accent } else { track };
            Span::styled(" ", Style::default().bg(background))
        })
        .collect();

    let value_line = Line::from(Span::styled(
        format!(" {value}  ({}..{})", cfg.min, cfg.max),
        style::fg(palette.accent).add_modifier(Modifier::BOLD),
    ));
    let hint = shortcut_hints::lines(
        &[("\u{2190}/\u{2192}", "adjust"), ("enter", "ok")],
        palette.accent_dim,
        bar_width,
    )
    .into_iter()
    .next()
    .unwrap_or_default();
    vec![
        value_line,
        Line::from(""),
        Line::from(bar),
        Line::from(""),
        hint,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step_value_clamps_to_bounds() {
        assert_eq!(step_value(50, 5, 0, 100), 55);
        assert_eq!(step_value(98, 5, 0, 100), 100);
        assert_eq!(step_value(2, -5, 0, 100), 0);
    }
}

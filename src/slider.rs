//! A numeric slider/stepper modal: pick a value in a bounded range.

use std::io;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use super::{
    input,
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
    /// The lowest selectable value.
    pub min: i64,
    /// The highest selectable value.
    pub max: i64,
    /// The increment per arrow press.
    pub step: i64,
    /// The value the slider opens on.
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
        |value, key| handle_key(value, key, &cfg),
    )
}

/// Applies one key to `value`, or reports that the slider is done.
///
/// A named function rather than a closure inside [`popup`], so the guard below
/// is reachable from a test: everything in `popup` needs a live terminal.
fn handle_key(
    value: &mut i64,
    key: KeyEvent,
    cfg: &SliderConfig,
) -> PopupFlow<i64> {
    // The value steps on bare keys only: in raw mode crossterm reports Ctrl+H
    // as `Char('h') + CONTROL`, so without this guard a chord would silently
    // adjust the value.
    if input::is_command(key) {
        return PopupFlow::Continue;
    }
    match key.code {
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
    }
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
    let mut lines = vec![value_line, Line::from(""), Line::from(bar)];
    // The spacer belongs to the hint: both go when the hints are hidden, so the
    // box (sized from these lines) loses exactly the two rows.
    lines.extend(
        shortcut_hints::lines(
            &[("\u{2190}/\u{2192}", "adjust"), ("enter", "ok")],
            palette.accent_dim,
            bar_width,
        )
        .into_iter()
        .flat_map(|hint| [Line::from(""), hint]),
    );
    lines
}

#[cfg(test)]
mod tests {
    use crossterm::event::KeyModifiers;

    use super::*;

    #[test]
    fn step_value_clamps_to_bounds() {
        assert_eq!(step_value(50, 5, 0, 100), 55);
        assert_eq!(step_value(98, 5, 0, 100), 100);
        assert_eq!(step_value(2, -5, 0, 100), 0);
    }

    fn cfg() -> SliderConfig {
        SliderConfig {
            min: 0,
            max: 100,
            step: 5,
            initial: 50,
        }
    }

    /// `Ctrl+H`/`Ctrl+L` arrive as plain characters in raw mode, so without the
    /// guard a chord would step the value.
    #[test]
    fn ctrl_chords_do_not_step_the_value() {
        for code in [
            KeyCode::Char('h'),
            KeyCode::Char('l'),
            KeyCode::Left,
            KeyCode::Right,
            KeyCode::Home,
            KeyCode::End,
        ] {
            let mut value = 50;
            let key = KeyEvent::new(code, KeyModifiers::CONTROL);
            assert!(matches!(
                handle_key(&mut value, key, &cfg()),
                PopupFlow::Continue
            ));
            assert_eq!(value, 50, "Ctrl+{code:?} moved the value");
        }
    }

    #[test]
    fn bare_keys_still_step_and_confirm() {
        let mut value = 50;
        let press = |code| KeyEvent::new(code, KeyModifiers::NONE);
        handle_key(&mut value, press(KeyCode::Right), &cfg());
        assert_eq!(value, 55);
        handle_key(&mut value, press(KeyCode::Char('h')), &cfg());
        assert_eq!(value, 50);
        handle_key(&mut value, press(KeyCode::End), &cfg());
        assert_eq!(value, 100);
        assert!(matches!(
            handle_key(&mut value, press(KeyCode::Enter), &cfg()),
            PopupFlow::Done(100)
        ));
        assert!(matches!(
            handle_key(&mut value, press(KeyCode::Esc), &cfg()),
            PopupFlow::Cancelled
        ));
    }
}

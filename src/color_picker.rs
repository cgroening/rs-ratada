//! An RGB color picker modal with a live preview swatch and hex readout.

use std::io;

use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::{
    Frame,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use super::{
    footer,
    layout::centered_rect,
    modal::ModalSignal,
    overlay::{self, PopupFlow, popup},
    style,
    terminal::Tui,
};
use crate::theme::{Color, Skin};

const BOX_WIDTH: u16 = 40;
const CHANNELS: [&str; 3] = ["R", "G", "B"];
/// Default adjustment step; holding `Shift` switches to the fine step.
const COARSE_STEP: i32 = 10;
const FINE_STEP: i32 = 1;

/// The picker state: the three channel values and the focused channel.
struct Rgb {
    channels: [u8; 3],
    focus: usize,
}

/// Lets the user compose an RGB color. `Up`/`Down` (or `Tab`) pick a channel,
/// `Left`/`Right` adjust it by ten (hold `Shift` for fine steps of one).
/// `Enter` returns the color, `Esc` cancels. `initial` seeds the channels
/// (falling back to the accent color).
pub fn color_picker(
    tui: &mut Tui,
    skin: &Skin,
    title: &str,
    initial: Option<Color>,
    render_bg: impl Fn(&mut Frame),
) -> io::Result<ModalSignal<Color>> {
    let channels = initial
        .and_then(Color::rgb)
        .or_else(|| skin.palette.accent.rgb())
        .map_or([128, 128, 128], |(r, g, b)| [r, g, b]);
    let mut state = Rgb { channels, focus: 0 };
    popup(
        tui,
        &mut state,
        |area, state: &Rgb| {
            let rows =
                body_lines(skin, state.channels, state.focus).len() as u16 + 2;
            centered_rect(overlay::box_width(BOX_WIDTH, skin), rows, area)
        },
        |frame, _| render_bg(frame),
        |frame, rect, state: &Rgb| {
            let inner = overlay::framed(frame, rect, skin, title);
            let lines = body_lines(skin, state.channels, state.focus);
            frame.render_widget(Paragraph::new(lines), inner);
        },
        |state, key| {
            // Shift switches from the coarse default step to fine.
            let step = if key.modifiers.contains(KeyModifiers::SHIFT) {
                FINE_STEP
            } else {
                COARSE_STEP
            };
            match key.code {
                KeyCode::Up | KeyCode::Char('k') | KeyCode::BackTab => {
                    state.focus = (state.focus + 2) % 3;
                    PopupFlow::Continue
                }
                KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab => {
                    state.focus = (state.focus + 1) % 3;
                    PopupFlow::Continue
                }
                KeyCode::Left | KeyCode::Char('h' | 'H') => {
                    state.channels[state.focus] =
                        adjust(state.channels[state.focus], -step);
                    PopupFlow::Continue
                }
                KeyCode::Right | KeyCode::Char('l' | 'L') => {
                    state.channels[state.focus] =
                        adjust(state.channels[state.focus], step);
                    PopupFlow::Continue
                }
                KeyCode::Enter => PopupFlow::Done(Color::Rgb(
                    state.channels[0],
                    state.channels[1],
                    state.channels[2],
                )),
                KeyCode::Esc => PopupFlow::Cancelled,
                _ => PopupFlow::Continue,
            }
        },
    )
}

/// Moves a channel by `delta`, saturating at `0..=255`.
pub(crate) fn adjust(channel: u8, delta: i32) -> u8 {
    (i32::from(channel) + delta).clamp(0, 255) as u8
}

fn body_lines(skin: &Skin, rgb: [u8; 3], channel: usize) -> Vec<Line<'static>> {
    let palette = &skin.palette;
    let color = Color::Rgb(rgb[0], rgb[1], rgb[2]);
    let bar_width = (BOX_WIDTH as usize).saturating_sub(12).max(1);
    let accent = style::to_ratatui(palette.accent);
    let track = style::to_ratatui(palette.selection_bg);

    let mut lines: Vec<Line> = CHANNELS
        .iter()
        .enumerate()
        .map(|(index, name)| {
            let value = rgb[index];
            let filled = usize::from(value) * bar_width / 255;
            let label_style = if index == channel {
                style::fg(palette.accent).add_modifier(Modifier::BOLD)
            } else {
                style::dim()
            };
            let mut spans =
                vec![Span::styled(format!(" {name} {value:>3} "), label_style)];
            spans.extend((0..bar_width).map(|x| {
                let background = if x < filled { accent } else { track };
                Span::styled(" ", Style::default().bg(background))
            }));
            let line = Line::from(spans);
            if index == channel {
                line.style(style::bg(palette.selection_bg))
            } else {
                line
            }
        })
        .collect();

    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("        ", style::bg(color)),
        Span::raw("  "),
        Span::styled(
            color.to_hex(),
            style::fg(palette.accent).add_modifier(Modifier::BOLD),
        ),
    ]));
    lines.push(Line::from(""));
    lines.extend(footer::lines(
        &[
            ("\u{2190}/\u{2192}", "\u{b1}10"),
            ("shift", "fine"),
            ("\u{2191}/\u{2193}", "channel"),
            ("enter", "ok"),
        ],
        palette.accent_dim,
        BOX_WIDTH as usize - 2,
    ));
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adjust_saturates_at_bounds() {
        assert_eq!(adjust(250, 16), 255);
        assert_eq!(adjust(5, -16), 0);
        assert_eq!(adjust(100, 20), 120);
    }
}

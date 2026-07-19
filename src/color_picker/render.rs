//! Rendering the colour picker: the model tab bar, the channel sliders, the
//! hex field, the contrast preview and the preset row.

use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use super::{BOX_WIDTH, Focus, Model, State};
use crate::{
    shortcut_hints, style,
    theme::{Palette, Skin},
};

/// Marker drawn on a slider at the current value, and on the focused preset.
const SLIDER_MARK: &str = "\u{2502}";

const PRESET_MARK: &str = "\u{25cf}";

pub(super) fn body_lines(skin: &Skin, state: &State) -> Vec<Line<'static>> {
    let palette = &skin.palette;
    let width = BOX_WIDTH as usize - 2;
    let mut lines = vec![tab_line(state, palette), Line::from("")];
    for index in 0..3 {
        lines.push(channel_line(state, palette, index, width));
    }
    lines.push(Line::from(""));
    lines.push(hex_line(state, palette, width));
    lines.push(Line::from(""));
    lines.extend(preview_lines(state, palette));
    lines.push(Line::from(""));
    lines.push(preset_line(state, palette));
    lines.push(Line::from(""));
    lines.extend(hint_lines(state, palette, width));
    lines
}

/// The `RGB · HSL · OKLCH` model tab line, active model accented.
fn tab_line(state: &State, palette: &Palette) -> Line<'static> {
    let mut spans = vec![Span::raw(" ")];
    for (index, model) in Model::ALL.iter().enumerate() {
        if index > 0 {
            spans.push(Span::styled(" \u{b7} ", style::dim()));
        }
        let text = model.label().to_string();
        if *model == state.model {
            spans.push(Span::styled(
                text,
                style::fg(palette.accent).add_modifier(Modifier::BOLD),
            ));
        } else {
            spans.push(Span::styled(text, style::secondary(palette)));
        }
    }
    Line::from(spans)
}

/// A channel row: label, value and a gradient bar with a value marker.
fn channel_line(
    state: &State,
    palette: &Palette,
    index: usize,
    width: usize,
) -> Line<'static> {
    let channel = state.model.channels()[index];
    let value = state.channels[index];
    let focused = state.focus == Focus::Channel(index);
    let label = format!(" {} {:>3} ", channel.label, value.round() as i32);
    let bar_width = width.saturating_sub(label.len() + 1).max(1);
    let fraction =
        ((value - channel.min) / (channel.max - channel.min)).clamp(0.0, 1.0);
    let denominator = bar_width.saturating_sub(1).max(1) as f32;
    let mark_at = (fraction * denominator).round() as usize;

    let mut spans = vec![Span::styled(label, label_style(palette, focused))];
    for x in 0..bar_width {
        let cell_fraction = x as f32 / denominator;
        let mut channels = state.channels;
        channels[index] =
            channel.min + (channel.max - channel.min) * cell_fraction;
        let color = state.model.to_color(channels);
        if x == mark_at {
            let fg = style::to_ratatui(color.readable_on(color));
            spans.push(Span::styled(
                SLIDER_MARK.to_string(),
                style::bg(color).fg(fg),
            ));
        } else {
            spans.push(Span::styled(" ".to_string(), style::bg(color)));
        }
    }
    Line::from(spans)
}

/// The editable hex field row.
fn hex_line(state: &State, palette: &Palette, width: usize) -> Line<'static> {
    let focused = state.focus == Focus::Hex;
    let label = " hex ";
    let field_width = width.saturating_sub(label.len());
    let mut spans = vec![Span::styled(label, label_style(palette, focused))];
    spans.extend(state.hex.render_line(palette, field_width, focused).spans);
    Line::from(spans)
}

/// The preview block: swatch + hex, rgb/hsl readouts, and a light/dark contrast
/// sample with the luminance value.
fn preview_lines(state: &State, palette: &Palette) -> Vec<Line<'static>> {
    let color = state.current_color();
    let (red, green, blue) = color.rgb().unwrap_or((0, 0, 0));
    let (hue, sat, light) = color.to_hsl().unwrap_or((0.0, 0.0, 0.0));

    let swatch = Line::from(vec![
        Span::styled("          ", style::bg(color)),
        Span::styled(
            format!("  {}", color.to_hex()),
            style::fg(palette.accent).add_modifier(Modifier::BOLD),
        ),
    ]);
    let readout = Line::from(Span::styled(
        format!(
            "  rgb {red} {green} {blue}    hsl {} {} {}",
            hue.round() as i32,
            (sat * 100.0).round() as i32,
            (light * 100.0).round() as i32,
        ),
        style::secondary(palette),
    ));
    let contrast = Line::from(vec![
        Span::styled(
            " Ab ",
            style::fg(color).bg(style::to_ratatui(style::LIGHT_BG)),
        ),
        Span::raw(" "),
        Span::styled(
            " Ab ",
            style::fg(color).bg(style::to_ratatui(style::DARK_BG)),
        ),
        Span::styled(
            format!("   lum {:.2}", color.luminance()),
            style::secondary(palette),
        ),
    ]);
    vec![swatch, readout, contrast]
}

/// The preset swatch row; the focused preset carries a marker.
fn preset_line(state: &State, palette: &Palette) -> Line<'static> {
    let focused = state.focus == Focus::Presets;
    let mut spans = vec![Span::styled(" set ", label_style(palette, focused))];
    for (index, &color) in state.presets.iter().enumerate() {
        let fill = style::bg(color);
        if focused && index == state.preset {
            let fg = style::to_ratatui(color.readable_on(color));
            spans.push(Span::styled(" ".to_string(), fill));
            spans.push(Span::styled(PRESET_MARK.to_string(), fill.fg(fg)));
            spans.push(Span::styled(" ".to_string(), fill));
        } else {
            spans.push(Span::styled("   ".to_string(), fill));
        }
        spans.push(Span::raw(" "));
    }
    Line::from(spans)
}

/// The focus-dependent shortcut hint line(s).
fn hint_lines(
    state: &State,
    palette: &Palette,
    width: usize,
) -> Vec<Line<'static>> {
    let hints: &[(&str, &str)] = match state.focus {
        Focus::Hex => &[
            ("type", "hex"),
            ("m", "model"),
            ("\u{2191}/\u{2193}", "focus"),
            ("enter", "ok"),
            ("esc", "back"),
        ],
        Focus::Presets => &[
            ("\u{2190}/\u{2192}", "choose"),
            ("m", "model"),
            ("s", "swatches"),
            ("y", "copy"),
            ("enter", "ok"),
            ("esc", "back"),
        ],
        Focus::Channel(_) => &[
            ("\u{2190}/\u{2192}", "adjust"),
            ("shift", "fine"),
            ("m", "model"),
            ("s", "swatches"),
            ("y", "copy"),
            ("enter", "ok"),
            ("esc", "back"),
        ],
    };
    shortcut_hints::lines(hints, palette.accent_dim, width)
}

/// Accent+bold for a focused label, dim secondary otherwise.
fn label_style(palette: &Palette, focused: bool) -> Style {
    if focused {
        style::fg(palette.accent).add_modifier(Modifier::BOLD)
    } else {
        style::secondary(palette)
    }
}

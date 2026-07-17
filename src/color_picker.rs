//! A color picker modal: switchable RGB/HSL/OKLCH channels with gradient
//! sliders, an editable hex field, palette presets and a live preview.

use std::io;

use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::{
    Frame,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use super::{
    clipboard,
    input::{self, InputField},
    layout::centered_rect,
    modal::ModalSignal,
    nav,
    overlay::{self, PopupFlow, popup_with_paste},
    shortcut_hints, style,
    terminal::Tui,
};
use crate::theme::{Color, Palette, Skin, parse_color};

const BOX_WIDTH: u16 = 54;
/// A light and a dark reference background for the contrast preview.
const LIGHT_BG: Color = Color::hex("#e5e5e5");
const DARK_BG: Color = Color::hex("#151515");
/// Marker drawn on a slider at the current value, and on the focused preset.
const SLIDER_MARK: &str = "\u{2502}";
const PRESET_MARK: &str = "\u{25cf}";

/// A color model whose three channels the user edits.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Model {
    Rgb,
    Hsl,
    Oklch,
}

impl Model {
    const ALL: [Model; 3] = [Model::Rgb, Model::Hsl, Model::Oklch];

    fn label(self) -> &'static str {
        match self {
            Model::Rgb => "RGB",
            Model::Hsl => "HSL",
            Model::Oklch => "OKLCH",
        }
    }

    fn next(self) -> Model {
        match self {
            Model::Rgb => Model::Hsl,
            Model::Hsl => Model::Oklch,
            Model::Oklch => Model::Rgb,
        }
    }

    /// The three channels in display units (RGB 0..255; HSL 0..360/0..100/0..100;
    /// OKLCH L 0..100, C 0..40, H 0..360).
    fn channels(self) -> [Channel; 3] {
        match self {
            Model::Rgb => [
                Channel::new("R", 0.0, 255.0, 16.0),
                Channel::new("G", 0.0, 255.0, 16.0),
                Channel::new("B", 0.0, 255.0, 16.0),
            ],
            Model::Hsl => [
                Channel::new("H", 0.0, 360.0, 10.0),
                Channel::new("S", 0.0, 100.0, 5.0),
                Channel::new("L", 0.0, 100.0, 5.0),
            ],
            Model::Oklch => [
                Channel::new("L", 0.0, 100.0, 2.0),
                Channel::new("C", 0.0, 40.0, 2.0),
                Channel::new("H", 0.0, 360.0, 10.0),
            ],
        }
    }

    /// The channel values (display units) representing `color`.
    fn channels_of(self, color: Color) -> [f32; 3] {
        match self {
            Model::Rgb => {
                let (r, g, b) = color.rgb().unwrap_or((128, 128, 128));
                [f32::from(r), f32::from(g), f32::from(b)]
            }
            Model::Hsl => {
                let (h, s, l) = color.to_hsl().unwrap_or((0.0, 0.0, 0.5));
                [h, s * 100.0, l * 100.0]
            }
            Model::Oklch => {
                let (l, c, h) = color.to_oklch().unwrap_or((0.5, 0.0, 0.0));
                [l * 100.0, c * 100.0, h]
            }
        }
    }

    /// The color for the given channel values (display units).
    fn to_color(self, channels: [f32; 3]) -> Color {
        match self {
            Model::Rgb => Color::Rgb(
                channels[0].round() as u8,
                channels[1].round() as u8,
                channels[2].round() as u8,
            ),
            Model::Hsl => Color::from_hsl(
                channels[0],
                channels[1] / 100.0,
                channels[2] / 100.0,
            ),
            Model::Oklch => Color::from_oklch(
                channels[0] / 100.0,
                channels[1] / 100.0,
                channels[2],
            ),
        }
    }
}

/// One editable channel: its label, range and coarse step (fine step is one).
#[derive(Clone, Copy)]
struct Channel {
    label: &'static str,
    min: f32,
    max: f32,
    coarse: f32,
}

impl Channel {
    const fn new(label: &'static str, min: f32, max: f32, coarse: f32) -> Self {
        Self {
            label,
            min,
            max,
            coarse,
        }
    }

    /// A big jump for `PageUp`/`PageDown` (an eighth of the range).
    fn page(self) -> f32 {
        ((self.max - self.min) / 8.0).max(self.coarse)
    }
}

/// Which control has the keyboard focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Focus {
    Channel(usize),
    Hex,
    Presets,
}

/// The five focus targets in cycle order.
const FOCUS_ORDER: [Focus; 5] = [
    Focus::Channel(0),
    Focus::Channel(1),
    Focus::Channel(2),
    Focus::Hex,
    Focus::Presets,
];

/// The full picker state.
struct State {
    model: Model,
    channels: [f32; 3],
    focus: Focus,
    hex: InputField,
    presets: Vec<Color>,
    preset: usize,
}

impl State {
    fn current_color(&self) -> Color {
        self.model.to_color(self.channels)
    }

    /// Rebuilds the hex field from the current color (after a non-hex edit).
    fn sync_hex(&mut self) {
        self.hex = InputField::new(&self.current_color().to_hex());
    }

    /// Switches the model, re-deriving the channels from the current color.
    fn set_model(&mut self, model: Model) {
        let color = self.current_color();
        self.model = model;
        self.channels = model.channels_of(color);
        self.sync_hex();
    }

    /// Moves the focused channel by `delta`, clamped to its range.
    fn adjust(&mut self, index: usize, delta: f32) {
        let channel = self.model.channels()[index];
        self.channels[index] =
            (self.channels[index] + delta).clamp(channel.min, channel.max);
        self.sync_hex();
    }

    /// Sets the focused channel to `value`, clamped to its range.
    fn set(&mut self, index: usize, value: f32) {
        let channel = self.model.channels()[index];
        self.channels[index] = value.clamp(channel.min, channel.max);
        self.sync_hex();
    }

    /// Adopts `color` (from a preset or a valid hex entry).
    fn adopt(&mut self, color: Color) {
        self.channels = self.model.channels_of(color);
    }

    /// Advances the focus by `delta` (wrapping); syncs the hex field unless the
    /// new focus is the hex field itself.
    fn cycle_focus(&mut self, delta: isize) {
        let position = FOCUS_ORDER
            .iter()
            .position(|focus| *focus == self.focus)
            .unwrap_or(0);
        let count = FOCUS_ORDER.len();
        let next = (position as isize + delta).rem_euclid(count as isize);
        self.focus = FOCUS_ORDER[next as usize];
        if self.focus != Focus::Hex {
            self.sync_hex();
        }
    }
}

/// How the color picker was left.
pub enum ColorExit {
    /// The user confirmed this color (`Enter`).
    Done(Color),
    /// The user stepped back (`Esc`), carrying the current color.
    Back(Color),
    /// The user asked to switch to the swatch view (`s`), carrying the color.
    Swatches(Color),
    /// The user hard-quit (`Ctrl+Q`).
    Quit,
}

/// The picker's popup result: confirmed, stepped back, or switched to swatches,
/// each carrying the current color.
enum Outcome {
    Done(Color),
    Back(Color),
    Swatches(Color),
}

/// Lets the user compose a color. `↑`/`↓` (or `Tab`) move focus across the three
/// channels, the hex field and the presets; `←`/`→` adjust the focused channel
/// (hold `Shift` for fine steps), edit the hex field or pick a preset. `m` cycles
/// the color model (RGB/HSL/OKLCH), `y` copies the hex code, `s` switches to the
/// swatch view. `Enter` confirms the color; `Esc` steps back (see [`ColorExit`]).
/// `initial` seeds the color (falling back to the accent).
pub fn color_picker(
    tui: &mut Tui,
    skin: &Skin,
    title: &str,
    initial: Option<Color>,
    render_bg: impl Fn(&mut Frame),
) -> io::Result<ColorExit> {
    let color = initial
        .filter(|c| c.rgb().is_some())
        .or_else(|| {
            skin.palette
                .accent
                .rgb()
                .map(|(r, g, b)| Color::Rgb(r, g, b))
        })
        .unwrap_or(Color::Rgb(128, 128, 128));
    let mut state = State {
        model: Model::Rgb,
        channels: Model::Rgb.channels_of(color),
        focus: Focus::Channel(0),
        hex: InputField::new(&color.to_hex()),
        presets: preset_colors(&skin.palette),
        preset: 0,
    };
    let signal = popup_with_paste(
        tui,
        &mut state,
        |area, state: &State| {
            let rows = body_lines(skin, state).len() as u16 + 2;
            centered_rect(BOX_WIDTH, rows, area)
        },
        |frame, _| render_bg(frame),
        |frame, rect, state: &State| {
            let inner = overlay::framed(frame, rect, skin, title);
            let lines = body_lines(skin, state);
            frame.render_widget(Paragraph::new(lines), inner);
        },
        handle,
        |state, text| handle_paste(state, &text),
    )?;
    Ok(match signal {
        ModalSignal::Value(Outcome::Done(color)) => ColorExit::Done(color),
        // `Esc` maps to a step-back; `Cancelled` cannot occur (no such path).
        ModalSignal::Value(Outcome::Back(color)) => ColorExit::Back(color),
        ModalSignal::Value(Outcome::Swatches(color)) => {
            ColorExit::Swatches(color)
        }
        ModalSignal::Cancelled | ModalSignal::Quit => ColorExit::Quit,
    })
}

/// The palette colors offered as quick-start presets.
fn preset_colors(palette: &Palette) -> Vec<Color> {
    vec![
        palette.accent,
        palette.accent_vivid,
        palette.success,
        palette.warning,
        palette.error,
        palette.info,
        palette.foreground,
        palette.border,
    ]
}

/// Routes a key press to the focused control.
fn handle(
    state: &mut State,
    key: crossterm::event::KeyEvent,
) -> PopupFlow<Outcome> {
    match key.code {
        KeyCode::Enter => {
            return PopupFlow::Done(Outcome::Done(state.current_color()));
        }
        KeyCode::Esc => {
            return PopupFlow::Done(Outcome::Back(state.current_color()));
        }
        KeyCode::Tab | KeyCode::Down => {
            state.cycle_focus(1);
            return PopupFlow::Continue;
        }
        KeyCode::BackTab | KeyCode::Up => {
            state.cycle_focus(-1);
            return PopupFlow::Continue;
        }
        _ => {}
    }
    match state.focus {
        Focus::Hex => handle_hex(state, key),
        Focus::Channel(index) => handle_channel(state, index, key),
        Focus::Presets => handle_presets(state, key),
    }
}

/// Edits the hex field; a valid entry updates the channels live.
fn handle_hex(
    state: &mut State,
    key: crossterm::event::KeyEvent,
) -> PopupFlow<Outcome> {
    if state.hex.handle_key(key)
        && let Some(color) = parse_color(state.hex.value())
    {
        state.adopt(color);
    }
    PopupFlow::Continue
}

/// Routes a paste to the hex field, but only while it is focused; a valid entry
/// updates the channels live, like [`handle_hex`].
fn handle_paste(state: &mut State, text: &str) -> PopupFlow<Outcome> {
    if state.focus == Focus::Hex {
        state.hex.paste(text);
        if let Some(color) = parse_color(state.hex.value()) {
            state.adopt(color);
        }
    }
    PopupFlow::Continue
}

/// Adjusts the focused channel, or handles the model/copy chords.
fn handle_channel(
    state: &mut State,
    index: usize,
    key: crossterm::event::KeyEvent,
) -> PopupFlow<Outcome> {
    // This control binds no Ctrl chord of its own, so one must not reach the
    // bare keys below (Ctrl+Y would copy, Ctrl+S leave for the swatches).
    // `Shift` stays honoured: it only picks the fine step.
    if input::is_command(key) {
        return PopupFlow::Continue;
    }
    let channel = state.model.channels()[index];
    let step = if key.modifiers.contains(KeyModifiers::SHIFT) {
        1.0
    } else {
        channel.coarse
    };
    match key.code {
        KeyCode::Left | KeyCode::Char('h') => state.adjust(index, -step),
        KeyCode::Right | KeyCode::Char('l') => state.adjust(index, step),
        KeyCode::Home => state.set(index, channel.min),
        KeyCode::End => state.set(index, channel.max),
        KeyCode::PageUp => state.adjust(index, -channel.page()),
        KeyCode::PageDown => state.adjust(index, channel.page()),
        KeyCode::Char('m') => state.set_model(state.model.next()),
        KeyCode::Char('y') => copy_hex(state),
        KeyCode::Char('s') => {
            return PopupFlow::Done(Outcome::Swatches(state.current_color()));
        }
        _ => {}
    }
    PopupFlow::Continue
}

/// Picks a preset (live) or handles the model/copy chords.
fn handle_presets(
    state: &mut State,
    key: crossterm::event::KeyEvent,
) -> PopupFlow<Outcome> {
    // This control binds no Ctrl chord of its own, so one must not reach the
    // bare keys below (Ctrl+Y would copy, Ctrl+S leave for the swatches).
    if input::is_command(key) {
        return PopupFlow::Continue;
    }
    let count = state.presets.len();
    match key.code {
        KeyCode::Left | KeyCode::Char('h') => {
            select_preset(state, nav::cycle(state.preset, count, -1));
        }
        KeyCode::Right | KeyCode::Char('l') => {
            select_preset(state, nav::cycle(state.preset, count, 1));
        }
        KeyCode::Home => select_preset(state, 0),
        KeyCode::End => select_preset(state, count.saturating_sub(1)),
        KeyCode::Char('m') => state.set_model(state.model.next()),
        KeyCode::Char('y') => copy_hex(state),
        KeyCode::Char('s') => {
            return PopupFlow::Done(Outcome::Swatches(state.current_color()));
        }
        _ => {}
    }
    PopupFlow::Continue
}

/// Adopts the preset at `index` as the live color and refreshes the hex field.
fn select_preset(state: &mut State, index: usize) {
    state.preset = index;
    state.adopt(state.presets[index]);
    state.sync_hex();
}

/// Copies the current color's hex code to the clipboard (best effort).
fn copy_hex(state: &State) {
    let _ = clipboard::copy(&state.current_color().to_hex());
}

fn body_lines(skin: &Skin, state: &State) -> Vec<Line<'static>> {
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
        Span::styled(" Ab ", style::fg(color).bg(style::to_ratatui(LIGHT_BG))),
        Span::raw(" "),
        Span::styled(" Ab ", style::fg(color).bg(style::to_ratatui(DARK_BG))),
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

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyEvent, KeyModifiers};

    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::CONTROL)
    }

    fn state_from(color: Color) -> State {
        State {
            model: Model::Rgb,
            channels: Model::Rgb.channels_of(color),
            focus: Focus::Channel(0),
            hex: InputField::new(&color.to_hex()),
            presets: vec![Color::hex("#8bd3cd"), Color::hex("#a3c995")],
            preset: 0,
        }
    }

    #[test]
    fn model_roundtrip_preserves_color() {
        let color = Color::hex("#8bd3cd");
        for model in Model::ALL {
            let (r, g, b) =
                model.to_color(model.channels_of(color)).rgb().unwrap();
            let (rr, gg, bb) = color.rgb().unwrap();
            let name = model.label();
            assert!(r.abs_diff(rr) <= 1, "{name} R: {r} vs {rr}");
            assert!(g.abs_diff(gg) <= 1, "{name} G: {g} vs {gg}");
            assert!(b.abs_diff(bb) <= 1, "{name} B: {b} vs {bb}");
        }
    }

    #[test]
    fn presets_wrap_cyclically_and_home_end_jump() {
        let mut state = state_from(Color::hex("#8bd3cd"));
        state.focus = Focus::Presets;
        // Two presets: Left from the first wraps to the last.
        handle(&mut state, key(KeyCode::Left));
        assert_eq!(state.preset, 1);
        // Right from the last wraps back to the first.
        handle(&mut state, key(KeyCode::Right));
        assert_eq!(state.preset, 0);
        // End jumps to the last, Home back to the first.
        handle(&mut state, key(KeyCode::End));
        assert_eq!(state.preset, 1);
        handle(&mut state, key(KeyCode::Home));
        assert_eq!(state.preset, 0);
    }

    #[test]
    fn enter_confirms_and_esc_steps_back() {
        let mut state = state_from(Color::hex("#8bd3cd"));
        assert!(matches!(
            handle(&mut state, key(KeyCode::Enter)),
            PopupFlow::Done(Outcome::Done(_)),
        ));
        assert!(matches!(
            handle(&mut state, key(KeyCode::Esc)),
            PopupFlow::Done(Outcome::Back(_)),
        ));
    }

    #[test]
    fn s_switches_to_swatches_from_a_channel() {
        let mut state = state_from(Color::hex("#8bd3cd"));
        assert!(matches!(state.focus, Focus::Channel(_)));
        assert!(matches!(
            handle(&mut state, key(KeyCode::Char('s'))),
            PopupFlow::Done(Outcome::Swatches(_)),
        ));
    }

    #[test]
    fn ctrl_chords_do_not_leave_for_swatches_or_adjust() {
        let mut state = state_from(Color::hex("#8bd3cd"));
        let before = state.channels;
        for focus in [Focus::Channel(0), Focus::Presets] {
            state.focus = focus;
            // Ctrl+S must not hand off to the swatches; Ctrl+M must not cycle
            // the model, and Ctrl+H/Ctrl+L must not adjust a channel.
            for code in ['s', 'm', 'h', 'l', 'y'] {
                let flow = handle(&mut state, ctrl(KeyCode::Char(code)));
                assert!(
                    matches!(flow, PopupFlow::Continue),
                    "Ctrl+{code} escaped its guard",
                );
            }
            assert!(matches!(state.model, Model::Rgb), "Ctrl+M cycled it");
        }
        for (after, was) in state.channels.iter().zip(before) {
            assert!((after - was).abs() < 1e-3, "a channel moved");
        }
        assert_eq!(state.preset, 0);
    }

    #[test]
    fn shift_still_picks_the_fine_channel_step() {
        // Shift has no AltGr interaction, so it must keep reaching the arms.
        let mut state = state_from(Color::Rgb(100, 100, 100));
        handle(
            &mut state,
            KeyEvent::new(KeyCode::Right, KeyModifiers::SHIFT),
        );
        assert!((state.channels[0] - 101.0).abs() < 1e-3);
    }

    #[test]
    fn adjust_clamps_to_channel_bounds() {
        let mut state = state_from(Color::Rgb(250, 5, 100));
        state.adjust(0, 50.0);
        state.adjust(1, -50.0);
        assert!((state.channels[0] - 255.0).abs() < 1e-3);
        assert!(state.channels[1].abs() < 1e-3);
    }

    #[test]
    fn switching_model_keeps_the_color() {
        let mut state = state_from(Color::hex("#d57b76"));
        let before = state.current_color().rgb().unwrap();
        state.set_model(Model::Hsl);
        state.set_model(Model::Oklch);
        let after = state.current_color().rgb().unwrap();
        assert!(before.0.abs_diff(after.0) <= 2);
        assert!(before.1.abs_diff(after.1) <= 2);
        assert!(before.2.abs_diff(after.2) <= 2);
    }

    #[test]
    fn focus_cycles_through_all_targets() {
        let mut state = state_from(Color::Rgb(0, 0, 0));
        let seen: Vec<Focus> = (0..FOCUS_ORDER.len())
            .map(|_| {
                let focus = state.focus;
                state.cycle_focus(1);
                focus
            })
            .collect();
        assert_eq!(seen, FOCUS_ORDER);
        assert_eq!(state.focus, Focus::Channel(0));
    }

    #[test]
    fn hex_entry_updates_the_channels() {
        let mut state = state_from(Color::Rgb(0, 0, 0));
        state.focus = Focus::Hex;
        state.hex = InputField::new("#ffffff");
        state.adopt(parse_color(state.hex.value()).unwrap());
        assert_eq!(state.current_color(), Color::Rgb(255, 255, 255));
    }
}

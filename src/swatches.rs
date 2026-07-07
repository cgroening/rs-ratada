//! A multi-mode color picker modal: pick from named swatches, a hue/saturation
//! grid, a grayscale ramp or the theme palette.
//!
//! `m` cycles the modes (carrying the focused color over via perceptual
//! distance). `Space` returns the focused color directly; `Enter` hands it to
//! the full [`color_picker`] for editing. `y` copies its
//! hex. Each mode shows a focus preview (swatch, hex/hsl, nearest name,
//! light/dark contrast).

use std::{cell::Cell, io};

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::Modifier,
    text::{Line, Span},
    widgets::Paragraph,
};

use super::{
    clipboard, color_picker, fuzzy,
    layout::centered_rect,
    list,
    modal::ModalSignal,
    nav,
    overlay::{self, PopupFlow, popup},
    shortcut_hints, style,
    terminal::Tui,
};
use crate::theme::{Color, Palette, Skin};

const LIST_WIDTH: u16 = 30;
/// Visible list rows before a list-style mode scrolls.
const VISIBLE_ROWS: u16 = 12;
/// Width of the color swatch shown at the start of each list row.
const SWATCH_WIDTH: usize = 4;
/// Name column width for aligning the hex readout in list rows.
const NAME_WIDTH: usize = 12;
/// Grid dimensions (hue columns × saturation rows) and grayscale steps.
const GRID_COLS: usize = 18;
const GRID_ROWS: usize = 8;
const GRAY_STEPS: usize = 16;
/// A full turn of hue in degrees, spread across the grid's columns.
const HUE_DEGREES: f32 = 360.0;
/// The maximum 8-bit channel value, for spreading the gray ramp.
const MAX_CHANNEL: f32 = 255.0;
/// The grid's starting lightness plane and the `[`/`]` step.
const GRID_LIGHT_DEFAULT: f32 = 0.5;
const GRID_LIGHT_STEP: f32 = 0.08;
/// Display width of one grid cell. The focus marker is a thin vertical bar split
/// across the two columns, so it centers on the cell's midline even at width two:
/// `▕` (right one-eighth block) sits at the right edge of the left column, `▏`
/// (left one-eighth block) at the left edge of the right column, meeting at the
/// center.
const CELL_WIDTH: usize = 2;
const MARK_LEFT: &str = "\u{2595}";
const MARK_RIGHT: &str = "\u{258f}";
/// Reference backgrounds for the contrast preview.
const LIGHT_BG: Color = Color::hex("#e5e5e5");
const DARK_BG: Color = Color::hex("#151515");

/// A curated set of named colors, independent of the active theme. CSS-derived
/// so the names are familiar.
pub const NAMED_COLORS: &[(&str, Color)] = &[
    ("Black", Color::hex("#000000")),
    ("Gray", Color::hex("#808080")),
    ("Silver", Color::hex("#c0c0c0")),
    ("White", Color::hex("#ffffff")),
    ("Slate", Color::hex("#708090")),
    ("Red", Color::hex("#e6194b")),
    ("Crimson", Color::hex("#dc143c")),
    ("Coral", Color::hex("#ff7f50")),
    ("Orange", Color::hex("#ffa500")),
    ("Gold", Color::hex("#ffd700")),
    ("Yellow", Color::hex("#ffe119")),
    ("Olive", Color::hex("#808000")),
    ("Lime", Color::hex("#bfef45")),
    ("Green", Color::hex("#3cb44b")),
    ("Mint", Color::hex("#aaffc3")),
    ("Teal", Color::hex("#469990")),
    ("Cyan", Color::hex("#22d3d3")),
    ("Sky", Color::hex("#87ceeb")),
    ("Blue", Color::hex("#4363d8")),
    ("Navy", Color::hex("#000075")),
    ("Indigo", Color::hex("#4b0082")),
    ("Violet", Color::hex("#911eb4")),
    ("Magenta", Color::hex("#f032e6")),
    ("Pink", Color::hex("#fabed4")),
    ("Rose", Color::hex("#e6007e")),
    ("Brown", Color::hex("#9a6324")),
    ("Tan", Color::hex("#d2b48c")),
    ("Beige", Color::hex("#fffac8")),
];

/// The view a swatch picker is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Names,
    Grid,
    Grays,
    Palette,
}

impl Mode {
    const ALL: [Mode; 4] =
        [Mode::Names, Mode::Grid, Mode::Grays, Mode::Palette];

    fn label(self) -> &'static str {
        match self {
            Mode::Names => "Names",
            Mode::Grid => "Grid",
            Mode::Grays => "Grays",
            Mode::Palette => "Palette",
        }
    }

    fn next(self) -> Mode {
        match self {
            Mode::Names => Mode::Grid,
            Mode::Grid => Mode::Grays,
            Mode::Grays => Mode::Palette,
            Mode::Palette => Mode::Names,
        }
    }

    /// Whether this mode renders as a named list (vs. a color grid).
    fn is_list(self) -> bool {
        matches!(self, Mode::Names | Mode::Palette)
    }
}

/// A single selectable color, optionally named (named ones render list-style).
#[derive(Clone)]
struct Swatch {
    color: Color,
    name: Option<String>,
}

/// What `Enter`/`Space` produced: edit in the color picker, or take as-is.
enum Choice {
    Pick(Color),
    Edit(Color),
}

/// The picker state (threaded through [`popup`]).
struct State {
    mode: Mode,
    cursor: usize,
    offset: Cell<usize>,
    cells: Vec<Swatch>,
    cols: usize,
    palette: Vec<(&'static str, Color)>,
    grid_light: f32,
    filter: String,
    filtering: bool,
}

impl State {
    /// Rebuilds the current mode's cells (after a mode/filter/lightness change).
    fn rebuild(&mut self) {
        let (cells, cols) =
            mode_cells(self.mode, &self.palette, self.grid_light, &self.filter);
        self.cells = cells;
        self.cols = cols.max(1);
        if self.cursor >= self.cells.len() {
            self.cursor = self.cells.len().saturating_sub(1);
        }
    }

    fn focus_color(&self) -> Color {
        self.cells
            .get(self.cursor)
            .map_or(Color::Default, |cell| cell.color)
    }

    /// Cycles to the next mode, carrying the focused color to its nearest cell.
    fn switch_mode(&mut self) {
        let current = self.focus_color();
        self.mode = self.mode.next();
        self.filter.clear();
        self.filtering = false;
        self.rebuild();
        self.cursor = nearest(&self.cells, current);
    }
}

/// Builds the cells and column count for `mode`.
/// The swatch cells for `mode` and the grid column count used to lay them out
/// (`1` for the single-column list modes).
fn mode_cells(
    mode: Mode,
    palette: &[(&'static str, Color)],
    grid_light: f32,
    filter: &str,
) -> (Vec<Swatch>, usize) {
    match mode {
        Mode::Names => (named_cells(filter), 1),
        Mode::Palette => (palette_cells(palette), 1),
        Mode::Grid => (grid_cells(grid_light), GRID_COLS),
        Mode::Grays => (gray_cells(), GRAY_STEPS),
    }
}

/// The named colors matching `filter` (all of them when it is empty).
fn named_cells(filter: &str) -> Vec<Swatch> {
    NAMED_COLORS
        .iter()
        .filter(|(name, _)| {
            filter.is_empty() || fuzzy::score(name, filter).is_some()
        })
        .map(|(name, color)| Swatch {
            color: *color,
            name: Some((*name).to_string()),
        })
        .collect()
}

/// The current theme palette entries as named swatches.
fn palette_cells(palette: &[(&'static str, Color)]) -> Vec<Swatch> {
    palette
        .iter()
        .map(|(name, color)| Swatch {
            color: *color,
            name: Some((*name).to_string()),
        })
        .collect()
}

/// A hue x saturation grid at the `grid_light` lightness plane.
fn grid_cells(grid_light: f32) -> Vec<Swatch> {
    let mut cells = Vec::with_capacity(GRID_COLS * GRID_ROWS);
    for row in 0..GRID_ROWS {
        let saturation = 1.0 - row as f32 / (GRID_ROWS - 1) as f32;
        for col in 0..GRID_COLS {
            let hue = col as f32 / GRID_COLS as f32 * HUE_DEGREES;
            cells.push(Swatch {
                color: Color::from_hsl(hue, saturation, grid_light),
                name: None,
            });
        }
    }
    cells
}

/// An evenly spaced black-to-white gray ramp.
fn gray_cells() -> Vec<Swatch> {
    (0..GRAY_STEPS)
        .map(|step| {
            let value = (step as f32 / (GRAY_STEPS - 1) as f32 * MAX_CHANNEL)
                .round() as u8;
            Swatch {
                color: Color::Rgb(value, value, value),
                name: None,
            }
        })
        .collect()
}

/// The index of the cell perceptually closest to `color`.
fn nearest(cells: &[Swatch], color: Color) -> usize {
    cells
        .iter()
        .enumerate()
        .min_by(|(_, first), (_, second)| {
            first
                .color
                .distance(color)
                .total_cmp(&second.color.distance(color))
        })
        .map_or(0, |(index, _)| index)
}

/// The semantic theme colors offered by the palette mode.
fn palette_entries(palette: &Palette) -> Vec<(&'static str, Color)> {
    vec![
        ("accent", palette.accent),
        ("accent_vivid", palette.accent_vivid),
        ("success", palette.success),
        ("warning", palette.warning),
        ("error", palette.error),
        ("info", palette.info),
        ("foreground", palette.foreground),
        ("border", palette.border),
        ("surface", palette.surface),
        ("background", palette.background),
    ]
}

/// Which view the [`color_chooser`] starts in (also its current view).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Start {
    Swatches,
    Picker,
}

/// How the swatch view was left.
enum SwatchExit {
    Pick(Color),
    Edit(Color),
    Cancel,
    Quit,
}

/// The swatch view state that survives a trip through the picker, so returning
/// (`Esc`/`s`) restores the same mode and grid lightness.
#[derive(Clone, Copy)]
struct SwatchMemory {
    mode: Mode,
    grid_light: f32,
}

impl Default for SwatchMemory {
    fn default() -> Self {
        Self {
            mode: Mode::Names,
            grid_light: GRID_LIGHT_DEFAULT,
        }
    }
}

/// Opens the color chooser starting in the swatch view. See [`color_chooser`].
pub fn swatch_picker(
    tui: &mut Tui,
    skin: &Skin,
    title: &str,
    initial: Option<Color>,
    render_bg: impl Fn(&mut Frame),
) -> io::Result<ModalSignal<Color>> {
    color_chooser(tui, skin, title, initial, Start::Swatches, render_bg)
}

/// A combined color chooser that alternates between the swatch view and the full
/// [`color_picker`]. Swatch view: `↑`/`↓`/`←`/`→` (or `k`/`j`/`h`/`l`) move, `m`
/// cycles the mode, `[`/`]` shift the grid lightness, `/` filters the named list,
/// `y` copies the hex, `Space` returns the color directly, `Enter` opens it in the
/// picker, `Esc` cancels. Picker view: `Enter` confirms, `s` switches to the
/// swatch view, `Esc` steps back to it (or cancels if the picker was opened
/// first and no swatch view has been shown). The focused color carries across
/// both ways, and the swatch view keeps its mode/lightness across a round trip.
/// `start` picks the initial view; `initial` highlights its nearest color.
pub fn color_chooser(
    tui: &mut Tui,
    skin: &Skin,
    title: &str,
    initial: Option<Color>,
    start: Start,
    render_bg: impl Fn(&mut Frame),
) -> io::Result<ModalSignal<Color>> {
    let mut view = start;
    let mut color = initial;
    let mut memory = SwatchMemory::default();
    // Whether the swatch view has been shown, so the picker's `Esc` (back) knows
    // whether there is a swatch view to return to (else it cancels).
    let mut swatch_seen = matches!(start, Start::Swatches);
    loop {
        match view {
            Start::Swatches => {
                swatch_seen = true;
                let (exit, updated) =
                    run_swatch(tui, skin, title, color, memory, &render_bg)?;
                memory = updated;
                match exit {
                    SwatchExit::Pick(chosen) => {
                        return Ok(ModalSignal::Value(chosen));
                    }
                    SwatchExit::Edit(chosen) => {
                        color = Some(chosen);
                        view = Start::Picker;
                    }
                    SwatchExit::Cancel => return Ok(ModalSignal::Cancelled),
                    SwatchExit::Quit => return Ok(ModalSignal::Quit),
                }
            }
            Start::Picker => {
                let exit = color_picker::color_picker(
                    tui, skin, title, color, &render_bg,
                )?;
                match exit {
                    color_picker::ColorExit::Done(chosen) => {
                        return Ok(ModalSignal::Value(chosen));
                    }
                    color_picker::ColorExit::Swatches(chosen) => {
                        color = Some(chosen);
                        view = Start::Swatches;
                    }
                    color_picker::ColorExit::Back(chosen) => {
                        // Back to swatches if we came from there, else cancel.
                        if swatch_seen {
                            color = Some(chosen);
                            view = Start::Swatches;
                        } else {
                            return Ok(ModalSignal::Cancelled);
                        }
                    }
                    color_picker::ColorExit::Quit => {
                        return Ok(ModalSignal::Quit);
                    }
                }
            }
        }
    }
}

/// Runs one pass of the swatch view, seeded from and reporting back `memory`
/// (mode + grid lightness) so a round-trip through the picker is seamless.
fn run_swatch(
    tui: &mut Tui,
    skin: &Skin,
    title: &str,
    initial: Option<Color>,
    memory: SwatchMemory,
    render_bg: impl Fn(&mut Frame),
) -> io::Result<(SwatchExit, SwatchMemory)> {
    let mut state = State {
        mode: memory.mode,
        cursor: 0,
        offset: Cell::new(0),
        cells: Vec::new(),
        cols: 1,
        palette: palette_entries(&skin.palette),
        grid_light: memory.grid_light,
        filter: String::new(),
        filtering: false,
    };
    state.rebuild();
    if let Some(color) = initial {
        state.cursor = nearest(&state.cells, color);
    }
    let signal = popup(
        tui,
        &mut state,
        |area, state: &State| {
            centered_rect(box_size(state).0, box_size(state).1, area)
        },
        |frame, _| render_bg(frame),
        |frame, rect, state: &State| {
            render_box(frame, rect, skin, title, state);
        },
        handle,
    )?;
    let memory = SwatchMemory {
        mode: state.mode,
        grid_light: state.grid_light,
    };
    let exit = match signal {
        ModalSignal::Value(Choice::Pick(color)) => SwatchExit::Pick(color),
        ModalSignal::Value(Choice::Edit(color)) => SwatchExit::Edit(color),
        ModalSignal::Cancelled => SwatchExit::Cancel,
        ModalSignal::Quit => SwatchExit::Quit,
    };
    Ok((exit, memory))
}

/// The modal's `(width, height)` for the current mode. Layout parts: a mode bar,
/// an optional filter row, the content, a blank, a 4-row preview and a 2-row
/// footer, all inside the border.
fn box_size(state: &State) -> (u16, u16) {
    let extras = 1 + 1 + 4 + 2 + 2; // bar + blank + preview + footer + border
    match state.mode {
        Mode::Names | Mode::Palette => {
            let filter_row = u16::from(state.filtering);
            (LIST_WIDTH, VISIBLE_ROWS + filter_row + extras)
        }
        Mode::Grid => {
            let width = (GRID_COLS * CELL_WIDTH) as u16 + 4;
            (width, GRID_ROWS as u16 + extras)
        }
        Mode::Grays => {
            let width = (GRAY_STEPS * CELL_WIDTH).max(24) as u16 + 4;
            (width, 1 + extras)
        }
    }
}

/// Renders the frame, mode bar, content, preview and footer.
fn render_box(
    frame: &mut Frame,
    rect: Rect,
    skin: &Skin,
    title: &str,
    state: &State,
) {
    let inner = overlay::framed(frame, rect, skin, title);
    let filtering = state.mode == Mode::Names && state.filtering;

    let mut constraints = vec![Constraint::Length(1)];
    if filtering {
        constraints.push(Constraint::Length(1));
    }
    constraints.extend([
        Constraint::Min(0),
        Constraint::Length(1),
        Constraint::Length(4),
        Constraint::Length(2),
    ]);
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner);

    let mut index = 0;
    frame.render_widget(
        Paragraph::new(mode_bar(state, &skin.palette)),
        rows[index],
    );
    index += 1;
    if filtering {
        frame.render_widget(
            Paragraph::new(filter_line(state, &skin.palette)),
            rows[index],
        );
        index += 1;
    }
    let content = rows[index];
    index += 2; // skip the blank spacer
    if state.mode.is_list() {
        list::render(
            frame,
            content,
            skin,
            list::ListView {
                rows: list_rows(state, skin),
                selected: state.cursor,
                offset: &state.offset,
            },
        );
    } else {
        render_grid(frame, content, state, skin);
    }
    frame.render_widget(
        Paragraph::new(preview_lines(state, &skin.palette)),
        rows[index],
    );
    index += 1;
    frame.render_widget(
        Paragraph::new(footer_lines(
            state,
            &skin.palette,
            inner.width as usize,
        )),
        rows[index],
    );
}

/// The `Names · Grid · Grays · Palette` mode bar, active mode accented.
fn mode_bar(state: &State, palette: &Palette) -> Line<'static> {
    let mut spans = vec![Span::raw(" ")];
    for (index, mode) in Mode::ALL.iter().enumerate() {
        if index > 0 {
            spans.push(Span::styled(" \u{b7} ", style::dim()));
        }
        let text = mode.label().to_string();
        if *mode == state.mode {
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

/// The `/query` line shown while filtering the named list.
fn filter_line(state: &State, palette: &Palette) -> Line<'static> {
    Line::from(vec![
        Span::styled("/", style::fg(palette.accent)),
        Span::raw(state.filter.clone()),
        Span::styled(" ", style::bg(palette.cursor)),
    ])
}

/// Swatch + name + hex rows for the list-style modes.
fn list_rows(state: &State, skin: &Skin) -> Vec<Line<'static>> {
    state
        .cells
        .iter()
        .map(|cell| {
            let name = cell.name.clone().unwrap_or_default();
            Line::from(vec![
                Span::styled(" ".repeat(SWATCH_WIDTH), style::bg(cell.color)),
                Span::raw(" "),
                Span::styled(
                    format!("{name:<NAME_WIDTH$}"),
                    style::fg(skin.palette.foreground),
                ),
                Span::styled(
                    cell.color.to_hex(),
                    style::secondary(&skin.palette),
                ),
            ])
        })
        .collect()
}

/// Renders the color grid, marking the focused cell.
fn render_grid(frame: &mut Frame, area: Rect, state: &State, _skin: &Skin) {
    let cols = state.cols.max(1);
    let mut lines = Vec::new();
    let row_count = state.cells.len().div_ceil(cols);
    for row in 0..row_count {
        let mut spans = Vec::new();
        for col in 0..cols {
            let index = row * cols + col;
            let Some(cell) = state.cells.get(index) else {
                break;
            };
            let fill = style::bg(cell.color);
            if index == state.cursor {
                let mark =
                    style::to_ratatui(cell.color.readable_on(cell.color));
                let styled = fill.fg(mark);
                spans.push(Span::styled(MARK_LEFT, styled));
                spans.push(Span::styled(MARK_RIGHT, styled));
            } else {
                spans.push(Span::styled(" ".repeat(CELL_WIDTH), fill));
            }
        }
        lines.push(Line::from(spans));
    }
    frame.render_widget(Paragraph::new(lines), area);
}

/// The focus preview: a swatch, the hex/hsl readout with the nearest name, and a
/// light/dark contrast sample with the luminance.
fn preview_lines(state: &State, palette: &Palette) -> Vec<Line<'static>> {
    let color = state.focus_color();
    let (hue, saturation, lightness) =
        color.to_hsl().unwrap_or((0.0, 0.0, 0.0));
    let (marker, name) = nearest_name(color);
    let swatch = Line::from(Span::styled("            ", style::bg(color)));
    let info = Line::from(vec![
        Span::styled(
            color.to_hex(),
            style::fg(palette.accent).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(
                " \u{b7} hsl {} {} {}   {marker} {name}",
                hue.round() as i32,
                (saturation * 100.0).round() as i32,
                (lightness * 100.0).round() as i32,
            ),
            style::secondary(palette),
        ),
    ]);
    let contrast = Line::from(vec![
        Span::styled(" Ab ", style::fg(color).bg(style::to_ratatui(LIGHT_BG))),
        Span::raw(" "),
        Span::styled(" Ab ", style::fg(color).bg(style::to_ratatui(DARK_BG))),
        Span::styled(
            format!("  lum {:.2}", color.luminance()),
            style::secondary(palette),
        ),
    ]);
    vec![swatch.clone(), swatch, info, contrast]
}

/// The name of the nearest `NAMED_COLORS` entry, with `=` for an exact match or
/// `≈` for an approximation.
fn nearest_name(color: Color) -> (&'static str, &'static str) {
    let (distance, name) = NAMED_COLORS
        .iter()
        .map(|(name, candidate)| (candidate.distance(color), *name))
        .min_by(|first, second| first.0.total_cmp(&second.0))
        .unwrap_or((f32::INFINITY, "?"));
    let marker = if distance < 1e-4 { "=" } else { "\u{2248}" };
    (marker, name)
}

/// The focus-/mode-dependent footer hints.
fn footer_lines(
    state: &State,
    palette: &Palette,
    width: usize,
) -> Vec<Line<'static>> {
    let hints: Vec<(&str, &str)> =
        if state.mode == Mode::Names && state.filtering {
            vec![
                ("type", "filter"),
                ("\u{2191}/\u{2193}", "move"),
                ("enter", "edit"),
                ("esc", "clear"),
            ]
        } else {
            let mut hints = vec![("m", "mode")];
            if state.mode == Mode::Grid {
                hints.push(("[ ]", "light"));
            }
            if state.mode == Mode::Names {
                hints.push(("/", "filter"));
            }
            hints.extend_from_slice(&[
                ("space", "pick"),
                ("y", "copy"),
                ("enter", "edit"),
                ("esc", "cancel"),
            ]);
            hints
        };
    shortcut_hints::lines(&hints, palette.accent_dim, width)
}

/// Routes a key to the active mode.
fn handle(state: &mut State, key: KeyEvent) -> PopupFlow<Choice> {
    if state.mode == Mode::Names && state.filtering {
        return handle_filter(state, key);
    }
    let len = state.cells.len();
    match key.code {
        KeyCode::Enter => return done(state, false),
        KeyCode::Char(' ') => return done(state, true),
        KeyCode::Esc => return PopupFlow::Cancelled,
        KeyCode::Char('m') => state.switch_mode(),
        KeyCode::Char('y') => {
            let _ = clipboard::copy(&state.focus_color().to_hex());
        }
        KeyCode::Up | KeyCode::Char('k') => move_vertical(state, -1),
        KeyCode::Down | KeyCode::Char('j') => move_vertical(state, 1),
        KeyCode::Left | KeyCode::Char('h') => move_horizontal(state, -1),
        KeyCode::Right | KeyCode::Char('l') => move_horizontal(state, 1),
        KeyCode::PageUp if state.mode.is_list() => {
            state.cursor =
                nav::step_clamped(state.cursor, len, -(VISIBLE_ROWS as isize));
        }
        KeyCode::PageDown if state.mode.is_list() => {
            state.cursor =
                nav::step_clamped(state.cursor, len, VISIBLE_ROWS as isize);
        }
        KeyCode::Home => state.cursor = 0,
        KeyCode::End => state.cursor = len.saturating_sub(1),
        KeyCode::Char('[') if state.mode == Mode::Grid => {
            adjust_light(state, -GRID_LIGHT_STEP);
        }
        KeyCode::Char(']') if state.mode == Mode::Grid => {
            adjust_light(state, GRID_LIGHT_STEP);
        }
        KeyCode::Char('/') if state.mode == Mode::Names => {
            state.filtering = true;
        }
        _ => {}
    }
    PopupFlow::Continue
}

/// Edits the named-list filter; any change resets the cursor to the first match.
fn handle_filter(state: &mut State, key: KeyEvent) -> PopupFlow<Choice> {
    match key.code {
        KeyCode::Esc => {
            state.filter.clear();
            state.filtering = false;
            state.rebuild();
            state.cursor = 0;
        }
        KeyCode::Enter => return done(state, false),
        KeyCode::Up => {
            state.cursor = nav::cycle(state.cursor, state.cells.len(), -1);
        }
        KeyCode::Down => {
            state.cursor = nav::cycle(state.cursor, state.cells.len(), 1);
        }
        KeyCode::Backspace => {
            state.filter.pop();
            state.rebuild();
            state.cursor = 0;
        }
        KeyCode::Char(ch) => {
            state.filter.push(ch);
            state.rebuild();
            state.cursor = 0;
        }
        _ => {}
    }
    PopupFlow::Continue
}

/// Finishes with the focused color, either taken directly or sent to the editor.
fn done(state: &State, pick: bool) -> PopupFlow<Choice> {
    match state.cells.get(state.cursor) {
        Some(cell) if pick => PopupFlow::Done(Choice::Pick(cell.color)),
        Some(cell) => PopupFlow::Done(Choice::Edit(cell.color)),
        None => PopupFlow::Continue,
    }
}

/// Moves the cursor a row up/down: list modes wrap, grid modes clamp.
fn move_vertical(state: &mut State, direction: isize) {
    let len = state.cells.len();
    if len == 0 {
        return;
    }
    if state.mode.is_list() {
        state.cursor = nav::cycle(state.cursor, len, direction);
        return;
    }
    let step = direction * state.cols as isize;
    let next = state.cursor as isize + step;
    if next >= 0 && (next as usize) < len {
        state.cursor = next as usize;
    }
}

/// Moves the cursor within its row: the hue grid wraps, others clamp; list modes
/// have a single column and ignore it.
fn move_horizontal(state: &mut State, direction: isize) {
    if state.mode.is_list() {
        return;
    }
    let cols = state.cols.max(1);
    let row = state.cursor / cols;
    let col = state.cursor % cols;
    let next_col = if state.mode == Mode::Grid {
        (col as isize + direction).rem_euclid(cols as isize) as usize
    } else {
        (col as isize + direction).clamp(0, cols as isize - 1) as usize
    };
    let candidate = row * cols + next_col;
    if candidate < state.cells.len() {
        state.cursor = candidate;
    }
}

/// Shifts the grid's lightness plane and rebuilds it (keeping the cursor cell).
fn adjust_light(state: &mut State, delta: f32) {
    state.grid_light = (state.grid_light + delta).clamp(0.05, 0.95);
    state.rebuild();
}

#[cfg(test)]
mod tests {
    use crossterm::event::KeyModifiers;

    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn state() -> State {
        let mut state = State {
            mode: Mode::Names,
            cursor: 0,
            offset: Cell::new(0),
            cells: Vec::new(),
            cols: 1,
            palette: vec![("accent", Color::hex("#8bd3cd"))],
            grid_light: GRID_LIGHT_DEFAULT,
            filter: String::new(),
            filtering: false,
        };
        state.rebuild();
        state
    }

    #[test]
    fn names_mode_matches_named_colors() {
        let state = state();
        assert_eq!(state.cells.len(), NAMED_COLORS.len());
        assert_eq!(state.cells[0].color, NAMED_COLORS[0].1);
    }

    #[test]
    fn m_cycles_all_modes() {
        let mut state = state();
        let seen: Vec<Mode> = (0..4)
            .map(|_| {
                let mode = state.mode;
                handle(&mut state, key(KeyCode::Char('m')));
                mode
            })
            .collect();
        assert_eq!(seen, Mode::ALL);
        assert_eq!(state.mode, Mode::Names);
    }

    #[test]
    fn switching_mode_carries_the_color_close() {
        let mut state = state();
        state.cursor = 6; // Crimson
        let before = state.focus_color();
        handle(&mut state, key(KeyCode::Char('m'))); // -> Grid
        assert_eq!(state.mode, Mode::Grid);
        assert!(state.focus_color().distance(before) < 0.2);
    }

    #[test]
    fn grid_left_wraps_hue_but_up_clamps() {
        let mut state = state();
        state.mode = Mode::Grid;
        state.rebuild();
        state.cursor = 0; // top-left
        move_horizontal(&mut state, -1);
        assert_eq!(state.cursor, GRID_COLS - 1); // wrapped to row end
        state.cursor = 0;
        move_vertical(&mut state, -1);
        assert_eq!(state.cursor, 0); // clamped at the top
    }

    #[test]
    fn list_up_wraps_to_the_end() {
        let mut state = state();
        handle(&mut state, key(KeyCode::Up));
        assert_eq!(state.cursor, NAMED_COLORS.len() - 1);
    }

    #[test]
    fn brackets_change_the_grid_lightness() {
        let mut state = state();
        state.mode = Mode::Grid;
        state.rebuild();
        let before = state.grid_light;
        let color_before = state.focus_color();
        handle(&mut state, key(KeyCode::Char(']')));
        assert!(state.grid_light > before);
        assert_ne!(state.focus_color(), color_before);
    }

    #[test]
    fn filter_narrows_the_named_list() {
        let mut state = state();
        handle(&mut state, key(KeyCode::Char('/')));
        assert!(state.filtering);
        for ch in "crim".chars() {
            handle(&mut state, key(KeyCode::Char(ch)));
        }
        assert!(state.cells.len() < NAMED_COLORS.len());
        assert_eq!(state.cursor, 0);
        assert!(
            state
                .cells
                .iter()
                .any(|cell| cell.name.as_deref() == Some("Crimson"))
        );
    }

    #[test]
    fn space_picks_and_enter_edits() {
        let mut state = state();
        state.cursor = 3;
        match done(&state, true) {
            PopupFlow::Done(Choice::Pick(color)) => {
                assert_eq!(color, NAMED_COLORS[3].1);
            }
            _ => panic!("expected a direct pick"),
        }
        match done(&state, false) {
            PopupFlow::Done(Choice::Edit(color)) => {
                assert_eq!(color, NAMED_COLORS[3].1);
            }
            _ => panic!("expected an edit hand-off"),
        }
    }
}

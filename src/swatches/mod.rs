//! A multi-mode color picker modal: pick from named swatches, a hue/saturation
//! grid, a grayscale ramp or the theme palette.
//!
//! `m` cycles the modes (carrying the focused color over via perceptual
//! distance). `Enter` returns the focused color; `e` hands it to the full
//! [`color_picker`] for fine-tuning. `y` copies its hex. Each mode shows a
//! focus preview (swatch, hex/hsl, nearest name, light/dark contrast).
//!
//! [`color_chooser_with`] adds two opt-ins: removing the color (a `(none)`
//! entry plus `Backspace`/`Delete`) and opening straight in the picker for a
//! color that no list holds.

mod cells;
mod interaction;
mod named;
mod render;

pub use named::NAMED_COLORS;

use cells::{is_listed, mode_cells, nearest, palette_entries};
use interaction::handle;
use render::{box_size, render_box};

use std::{cell::Cell, io};

use ratatui::Frame;

use super::{
    color_picker::{self, Clearing, PickerResult},
    layout::centered_rect,
    modal::ModalSignal,
    overlay::popup,
    terminal::Tui,
};
use crate::theme::{Color, Skin};

/// The grid's starting lightness plane and the `[`/`]` step.
const GRID_LIGHT_DEFAULT: f32 = 0.5;
const GRID_LIGHT_STEP: f32 = 0.08;

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

/// Grid dimensions (hue columns × saturation rows) and grayscale steps.
const GRID_COLS: usize = 18;
const GRID_ROWS: usize = 8;
const GRAY_STEPS: usize = 16;

/// Visible list rows before a list-style mode scrolls.
const VISIBLE_ROWS: u16 = 12;

/// The view a swatch picker is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Names,
    Grid,
    Grays,
    Palette,
}

/// A single selectable color, optionally named (named ones render list-style).
#[derive(Clone)]
struct Swatch {
    color: Color,
    name: Option<String>,
}

/// How the swatch view was answered: take a color as-is (`Enter`), edit one
/// in the color picker (`e`), or remove the color (`Clearing::On` only).
enum Choice {
    Pick(Color),
    Edit(Color),
    Clear,
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
    /// Whether the color can be removed: adds the `(none)` entry and the
    /// `Backspace`/`Delete` keys.
    clearing: Clearing,
}

impl State {
    /// Rebuilds the current mode's cells (after a mode/filter/lightness change).
    fn rebuild(&mut self) {
        let (cells, cols) = mode_cells(
            self.mode,
            &self.palette,
            self.grid_light,
            &self.filter,
            self.clearing,
        );
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

/// Which view the [`color_chooser`] starts in (also its current view).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Start {
    /// Open on the swatch grid.
    Swatches,
    /// Open on the channel picker.
    Picker,
}

/// How the swatch view was left.
enum SwatchExit {
    Pick(Color),
    Edit(Color),
    Clear,
    Cancel,
    Quit,
}

/// Opt-in behaviour of [`color_chooser_with`]; the default is the plain
/// chooser of [`color_chooser`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ChooserOptions {
    /// The color can be removed: a `(none)` entry heads the Names and Palette
    /// lists, and `Backspace`/`Delete` remove it in the list and in the
    /// picker (outside its hex field). Reported as [`ColorChoice::Cleared`].
    pub clearable: bool,
    /// Open straight in the picker when the initial color is set but no named
    /// color and no palette entry equals it exactly, so a custom value is not
    /// replaced by its nearest swatch. `Esc` there cancels; `s` still shows
    /// the list.
    pub picker_unless_listed: bool,
}

/// What [`color_chooser_with`] returns: a color, or "no color" when the user
/// removed it (only with [`ChooserOptions::clearable`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorChoice {
    /// The chosen color.
    Color(Color),
    /// The color was removed.
    Cleared,
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
/// `y` copies the hex, `Enter` returns the color, `e` opens it in the picker,
/// `Esc` cancels. Picker view: `Enter` confirms, `s` switches to the
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
    let session = Session {
        title,
        initial,
        clearing: Clearing::Off,
    };
    Ok(match run_chooser(tui, skin, &session, start, render_bg)? {
        ModalSignal::Value(ColorChoice::Color(color)) => {
            ModalSignal::Value(color)
        }
        // Cannot occur with clearing off; cancelling is the harmless reading.
        ModalSignal::Value(ColorChoice::Cleared) | ModalSignal::Cancelled => {
            ModalSignal::Cancelled
        }
        ModalSignal::Quit => ModalSignal::Quit,
    })
}

/// The swatch chooser with the opt-ins of [`ChooserOptions`]: removing the
/// color, and opening straight in the picker for a color no list holds.
/// Otherwise it behaves like [`swatch_picker`].
pub fn color_chooser_with(
    tui: &mut Tui,
    skin: &Skin,
    title: &str,
    initial: Option<Color>,
    options: ChooserOptions,
    render_bg: impl Fn(&mut Frame),
) -> io::Result<ModalSignal<ColorChoice>> {
    let palette = palette_entries(&skin.palette);
    let custom = initial.is_some_and(|color| {
        color != Color::Default && !is_listed(color, &palette)
    });
    let start = if options.picker_unless_listed && custom {
        Start::Picker
    } else {
        Start::Swatches
    };
    let session = Session {
        title,
        initial,
        clearing: if options.clearable {
            Clearing::On
        } else {
            Clearing::Off
        },
    };
    run_chooser(tui, skin, &session, start, render_bg)
}

/// What one chooser run is about: its title, the color it opened with and
/// whether the color can be removed.
struct Session<'a> {
    title: &'a str,
    initial: Option<Color>,
    clearing: Clearing,
}

/// The chooser loop behind [`color_chooser`] and [`color_chooser_with`],
/// alternating between the swatch view and the picker.
fn run_chooser(
    tui: &mut Tui,
    skin: &Skin,
    session: &Session,
    start: Start,
    render_bg: impl Fn(&mut Frame),
) -> io::Result<ModalSignal<ColorChoice>> {
    let title = session.title;
    let mut view = start;
    let mut color = session.initial;
    let mut memory = SwatchMemory::default();
    // Whether the swatch view has been shown, so the picker's `Esc` (back) knows
    // whether there is a swatch view to return to (else it cancels).
    let mut swatch_seen = matches!(start, Start::Swatches);
    loop {
        match view {
            Start::Swatches => {
                swatch_seen = true;
                let (exit, updated) =
                    run_swatch(tui, skin, session, color, memory, &render_bg)?;
                memory = updated;
                match exit {
                    SwatchExit::Pick(chosen) => {
                        return Ok(ModalSignal::Value(ColorChoice::Color(
                            chosen,
                        )));
                    }
                    SwatchExit::Edit(chosen) => {
                        color = Some(chosen);
                        view = Start::Picker;
                    }
                    SwatchExit::Clear => {
                        return Ok(ModalSignal::Value(ColorChoice::Cleared));
                    }
                    SwatchExit::Cancel => return Ok(ModalSignal::Cancelled),
                    SwatchExit::Quit => return Ok(ModalSignal::Quit),
                }
            }
            Start::Picker => {
                let exit = match color_picker::run_picker(
                    tui,
                    skin,
                    title,
                    color,
                    session.clearing,
                    &render_bg,
                )? {
                    PickerResult::Exit(exit) => exit,
                    PickerResult::Cleared => {
                        return Ok(ModalSignal::Value(ColorChoice::Cleared));
                    }
                };
                match exit {
                    color_picker::ColorExit::Done(chosen) => {
                        return Ok(ModalSignal::Value(ColorChoice::Color(
                            chosen,
                        )));
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
    session: &Session,
    initial: Option<Color>,
    memory: SwatchMemory,
    render_bg: impl Fn(&mut Frame),
) -> io::Result<(SwatchExit, SwatchMemory)> {
    let title = session.title;
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
        clearing: session.clearing,
    };
    state.rebuild();
    // With clearing on, no color starts on `(none)`, the first entry;
    // `nearest` never lands there.
    let starts_on_none = session.clearing == Clearing::On
        && initial.is_none_or(|color| color == Color::Default);
    if let Some(color) = initial.filter(|_| !starts_on_none) {
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
        ModalSignal::Value(Choice::Clear) => SwatchExit::Clear,
        ModalSignal::Cancelled => SwatchExit::Cancel,
        ModalSignal::Quit => SwatchExit::Quit,
    };
    Ok((exit, memory))
}

#[cfg(test)]
mod tests;

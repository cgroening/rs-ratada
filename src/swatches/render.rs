//! Rendering the swatch picker: its box, the mode bar, the filter line, the
//! list and grid bodies, the preview and the footer.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::Modifier,
    text::{Line, Span},
    widgets::Paragraph,
};

use super::{
    GRAY_STEPS, GRID_COLS, GRID_ROWS, Mode, State, VISIBLE_ROWS,
    named::nearest_name,
};
use crate::{
    chrome, input, list, overlay, shortcut_hints, style,
    theme::{Palette, Skin},
};

const LIST_WIDTH: u16 = 30;

/// Rows the hint footer occupies (always shown: popup hints ignore F1).
const FOOTER_ROWS: u16 = 2;

/// Width of the color swatch shown at the start of each list row.
const SWATCH_WIDTH: usize = 4;

/// Name column width for aligning the hex readout in list rows.
const NAME_WIDTH: usize = 12;

/// Display width of one grid cell. The focus marker is a thin vertical bar split
/// across the two columns, so it centers on the cell's midline even at width two:
/// `▕` (right one-eighth block) sits at the right edge of the left column, `▏`
/// (left one-eighth block) at the left edge of the right column, meeting at the
/// center.
const CELL_WIDTH: usize = 2;

const MARK_LEFT: &str = "\u{2595}";

const MARK_RIGHT: &str = "\u{258f}";

/// The modal's `(width, height)` for the current mode. Layout parts: a mode bar,
/// an optional filter row, the content, a blank, a 4-row preview and the
/// two-row footer (always shown), all inside the border.
pub(super) fn box_size(state: &State) -> (u16, u16) {
    let footer = shortcut_hints::footer_height(FOOTER_ROWS);
    let extras = 1 + 1 + 4 + footer + 2; // bar + blank + preview + footer + border
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
pub(super) fn render_box(
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
        Constraint::Length(shortcut_hints::footer_height(FOOTER_ROWS)),
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
        let width = rows[index].width as usize;
        frame.render_widget(
            Paragraph::new(filter_line(state, &skin.palette, width)),
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

    // Both modes walk the same (filtered) cell list, so the badge counts cells
    // rather than rows or grid columns.
    let badge = chrome::position_badge(state.cursor, state.cells.len());
    chrome::render_badge(frame, rect, skin, &badge);
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

/// The `/query` line shown while filtering the named list, clipped to `width`.
fn filter_line(
    state: &State,
    palette: &Palette,
    width: usize,
) -> Line<'static> {
    let mut spans = vec![Span::styled("/", style::fg(palette.accent))];
    spans.extend(input::query_spans(
        &state.filter,
        palette,
        width.saturating_sub(1),
    ));
    Line::from(spans)
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
            format!("  lum {:.2}", color.luminance()),
            style::secondary(palette),
        ),
    ]);
    vec![swatch.clone(), swatch, info, contrast]
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

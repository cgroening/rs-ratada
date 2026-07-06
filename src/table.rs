//! An interactive table: typed sorting, fuzzy filtering, row/cell multi-select,
//! multi-line rows and data-driven styling.
//!
//! [`Table`] is a stateful inline widget (the host calls [`Table::handle_key`]
//! and [`Table::render`] and reads the selection back); [`table_select`] wraps
//! it in a blocking modal. Selection is reported both as original row indices
//! and as host-provided keys.

use std::{
    cell::Cell, cmp::Ordering, collections::HashSet, fmt::Write as _, io,
};

use chrono::NaiveDate;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use super::{
    chrome, fuzzy,
    layout::centered_rect,
    modal::ModalSignal,
    nav,
    overlay::{self, PopupFlow, popup},
    scroll, style,
    terminal::Tui,
    text::truncate,
};
use crate::theme::Skin;

const SEPARATOR: &str = "  ";
/// Left gutter (selection marker) width.
const GUTTER: usize = 2;

/// Distributes `avail` columns across `(min, target)` specs: each column gets
/// at least `min`; the rest is shared round-robin up to `target`.
pub(crate) fn allocate_columns(
    specs: &[(usize, usize)],
    avail: usize,
) -> Vec<usize> {
    let mut widths: Vec<usize> = specs.iter().map(|&(min, _)| min).collect();
    let separators = SEPARATOR.width() * specs.len().saturating_sub(1);
    let used: usize = widths.iter().sum::<usize>() + separators;
    let mut leftover = avail.saturating_sub(used);
    while leftover > 0 {
        let mut progressed = false;
        for (width, &(_, target)) in widths.iter_mut().zip(specs) {
            if leftover == 0 {
                break;
            }
            if *width < target {
                *width += 1;
                leftover -= 1;
                progressed = true;
            }
        }
        if !progressed {
            break;
        }
    }
    widths
}

/// The data kind of a column, used for type-aware sorting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnKind {
    Text,
    Number,
    Date,
}

/// Cell text alignment within its column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    Right,
}

/// Whether selection acts on whole rows or individual cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectMode {
    Row,
    Cell,
}

/// Sort direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDir {
    Asc,
    Desc,
}

/// Which columns the fuzzy filter searches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterScope {
    AllColumns,
    ActiveColumn,
}

/// What the host should do after a key press.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableAction {
    None,
    /// `Enter` was pressed; read the current selection/cursor.
    Activate,
}

/// A column definition: title, data kind, alignment, width hints and optional
/// per-column header/cell styles. `wrap` lets the column's cells span lines.
#[derive(Debug, Clone)]
pub struct Column {
    title: String,
    kind: ColumnKind,
    align: Align,
    min: usize,
    target: usize,
    wrap: bool,
    header_style: Option<Style>,
    cell_style: Option<Style>,
}

impl Column {
    /// A text column with default widths and left alignment.
    pub fn text(title: impl Into<String>) -> Self {
        Self::new(title, ColumnKind::Text)
    }

    /// A right-aligned numeric column (sorted numerically).
    pub fn number(title: impl Into<String>) -> Self {
        Self::new(title, ColumnKind::Number).align(Align::Right)
    }

    /// A date column (`YYYY-MM-DD`, sorted chronologically).
    pub fn date(title: impl Into<String>) -> Self {
        Self::new(title, ColumnKind::Date)
    }

    fn new(title: impl Into<String>, kind: ColumnKind) -> Self {
        Self {
            title: title.into(),
            kind,
            align: Align::Left,
            min: 4,
            target: 20,
            wrap: false,
            header_style: None,
            cell_style: None,
        }
    }

    #[must_use]
    pub fn widths(mut self, min: usize, target: usize) -> Self {
        self.min = min;
        self.target = target;
        self
    }

    #[must_use]
    pub fn align(mut self, align: Align) -> Self {
        self.align = align;
        self
    }

    #[must_use]
    pub fn wrap(mut self, wrap: bool) -> Self {
        self.wrap = wrap;
        self
    }

    #[must_use]
    pub fn header_style(mut self, style: Style) -> Self {
        self.header_style = Some(style);
        self
    }

    #[must_use]
    pub fn cell_style(mut self, style: Style) -> Self {
        self.cell_style = Some(style);
        self
    }
}

/// One data row: cells, an optional per-row style and an optional host key.
#[derive(Debug, Clone)]
pub struct Row {
    cells: Vec<String>,
    style: Option<Style>,
    key: Option<String>,
}

impl Row {
    pub fn new(cells: Vec<String>) -> Self {
        Self {
            cells,
            style: None,
            key: None,
        }
    }

    #[must_use]
    pub fn with_style(mut self, style: Style) -> Self {
        self.style = Some(style);
        self
    }

    #[must_use]
    pub fn with_key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }

    fn cell(&self, col: usize) -> &str {
        self.cells.get(col).map_or("", String::as_str)
    }
}

/// Wraps `text` to `width` display columns, breaking between characters.
pub(crate) fn wrap_cell(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![String::new()];
    }
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut used = 0usize;
    for ch in text.chars() {
        let char_width = ch.width().unwrap_or(0);
        if used + char_width > width && !current.is_empty() {
            lines.push(std::mem::take(&mut current));
            used = 0;
        }
        current.push(ch);
        used += char_width;
    }
    if !current.is_empty() || lines.is_empty() {
        lines.push(current);
    }
    lines
}

/// Compares two cells for ascending order under `kind`.
fn compare_cells(kind: ColumnKind, a: &str, b: &str) -> Ordering {
    match kind {
        ColumnKind::Number => {
            let x = a.trim().parse::<f64>().unwrap_or(f64::NEG_INFINITY);
            let y = b.trim().parse::<f64>().unwrap_or(f64::NEG_INFINITY);
            x.partial_cmp(&y).unwrap_or(Ordering::Equal)
        }
        ColumnKind::Date => {
            let parse =
                |s: &str| NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").ok();
            parse(a).cmp(&parse(b))
        }
        ColumnKind::Text => a.cmp(b),
    }
}

/// Stably sorts `indices` (row indices into `rows`) by `col` in direction
/// `dir`, using the column's [`ColumnKind`] for type-aware comparison.
pub(crate) fn sort_indices(
    rows: &[Row],
    columns: &[Column],
    mut indices: Vec<usize>,
    col: usize,
    dir: SortDir,
) -> Vec<usize> {
    let kind = columns.get(col).map_or(ColumnKind::Text, |c| c.kind);
    indices.sort_by(|&a, &b| {
        let order = compare_cells(kind, rows[a].cell(col), rows[b].cell(col));
        match dir {
            SortDir::Asc => order,
            SortDir::Desc => order.reverse(),
        }
    });
    indices
}

/// Returns row indices matching `query` fuzzily; empty query keeps all rows in
/// order, otherwise results are best-score first.
pub(crate) fn filter_indices(
    rows: &[Row],
    query: &str,
    scope: FilterScope,
    active_col: usize,
) -> Vec<usize> {
    if query.trim().is_empty() {
        return (0..rows.len()).collect();
    }
    let mut scored: Vec<(u32, usize)> = rows
        .iter()
        .enumerate()
        .filter_map(|(index, row)| {
            let haystack = match scope {
                FilterScope::AllColumns => row.cells.join(" "),
                FilterScope::ActiveColumn => row.cell(active_col).to_string(),
            };
            fuzzy::score(&haystack, query).map(|score| (score, index))
        })
        .collect();
    scored.sort_by(|left, right| right.0.cmp(&left.0));
    scored.into_iter().map(|(_, index)| index).collect()
}

/// The top row offset that keeps `cursor` visible given per-row `heights` and
/// the viewport height `area_h`, starting from `prev`.
pub(crate) fn visible_offset(
    heights: &[u16],
    cursor: usize,
    area_h: u16,
    prev: usize,
) -> usize {
    if heights.is_empty() || area_h == 0 {
        return 0;
    }
    let mut off = prev.min(cursor).min(heights.len() - 1);
    while off < cursor {
        let used: u16 = heights[off..=cursor].iter().copied().sum();
        if used <= area_h {
            break;
        }
        off += 1;
    }
    off
}

/// An interactive table widget.
pub struct Table {
    columns: Vec<Column>,
    rows: Vec<Row>,
    view: Vec<usize>,
    cursor: usize,
    active_col: usize,
    anchor: usize,
    mode: SelectMode,
    selected_rows: HashSet<usize>,
    selected_cells: HashSet<(usize, usize)>,
    sort: Option<(usize, SortDir)>,
    filter: String,
    filter_scope: FilterScope,
    filtering: bool,
    offset: Cell<usize>,
    viewport: Cell<usize>,
    header_style: Style,
    show_status: bool,
    decor: Option<chrome::BoxDecor>,
    force_box: bool,
}

impl Table {
    /// Builds a table over `columns` and `rows`. All rows are initially shown.
    ///
    /// # Examples
    ///
    /// ```
    /// use ratada::table::{Column, Row, Table};
    ///
    /// let table = Table::new(
    ///     vec![Column::text("Name").widths(6, 20), Column::number("Age")],
    ///     vec![
    ///         Row::new(vec!["Ada".into(), "36".into()]),
    ///         Row::new(vec!["Linus".into(), "54".into()]),
    ///     ],
    /// );
    /// assert_eq!(table.selected_rows(), Vec::<usize>::new());
    /// ```
    pub fn new(columns: Vec<Column>, rows: Vec<Row>) -> Self {
        let view = (0..rows.len()).collect();
        Self {
            columns,
            rows,
            view,
            cursor: 0,
            active_col: 0,
            anchor: 0,
            mode: SelectMode::Row,
            selected_rows: HashSet::new(),
            selected_cells: HashSet::new(),
            sort: None,
            filter: String::new(),
            filter_scope: FilterScope::AllColumns,
            filtering: false,
            offset: Cell::new(0),
            viewport: Cell::new(1),
            header_style: style::dim().add_modifier(Modifier::BOLD),
            show_status: true,
            decor: None,
            force_box: false,
        }
    }

    /// Draws the table inside a rounded box in `Boxed` mode, plain otherwise;
    /// the caption sits in the top border and the row-count badge bottom-right.
    /// The inner status/filter line is kept.
    #[must_use]
    pub fn boxed(mut self, decor: chrome::BoxDecor) -> Self {
        self.decor = Some(decor);
        self
    }

    /// Like [`Self::boxed`] but always draws the box, regardless of the mode.
    #[must_use]
    pub fn boxed_always(mut self, decor: chrome::BoxDecor) -> Self {
        self.decor = Some(decor);
        self.force_box = true;
        self
    }

    /// Forces the plain (unframed) style, dropping any [`Self::boxed`]
    /// decoration even in `Boxed` mode.
    #[must_use]
    pub fn minimal(mut self) -> Self {
        self.decor = None;
        self.force_box = false;
        self
    }

    #[must_use]
    pub fn with_select_mode(mut self, mode: SelectMode) -> Self {
        self.mode = mode;
        self
    }

    #[must_use]
    pub fn with_filter_scope(mut self, scope: FilterScope) -> Self {
        self.filter_scope = scope;
        self
    }

    #[must_use]
    pub fn with_status(mut self, show: bool) -> Self {
        self.show_status = show;
        self
    }

    #[must_use]
    pub fn with_header_style(mut self, style: Style) -> Self {
        self.header_style = style;
        self
    }

    /// The original index of the row under the cursor, if any.
    pub fn cursor_row(&self) -> Option<usize> {
        self.view.get(self.cursor).copied()
    }

    /// The `(original row, column)` cell under the cursor, if any.
    pub fn cursor_cell(&self) -> Option<(usize, usize)> {
        self.cursor_row().map(|row| (row, self.active_col))
    }

    /// Selected original row indices (sorted). In cell mode, the rows that
    /// contain at least one selected cell.
    pub fn selected_rows(&self) -> Vec<usize> {
        let mut rows: Vec<usize> = match self.mode {
            SelectMode::Row => self.selected_rows.iter().copied().collect(),
            SelectMode::Cell => {
                self.selected_cells.iter().map(|&(row, _)| row).collect()
            }
        };
        rows.sort_unstable();
        rows.dedup();
        rows
    }

    /// Host keys of the selected rows (rows without a key are skipped).
    pub fn selected_keys(&self) -> Vec<String> {
        self.selected_rows()
            .into_iter()
            .filter_map(|row| self.rows[row].key.clone())
            .collect()
    }

    /// Selected `(original row, column)` cells (sorted).
    pub fn selected_cells(&self) -> Vec<(usize, usize)> {
        let mut cells: Vec<(usize, usize)> =
            self.selected_cells.iter().copied().collect();
        cells.sort_unstable();
        cells
    }

    /// Handles a key press; returns whether the host should act on it.
    pub fn handle_key(&mut self, key: KeyEvent) -> TableAction {
        if self.filtering {
            self.handle_filter_key(key);
            return TableAction::None;
        }
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let page = self.viewport.get().max(1) as isize;
        let last = self.view.len().saturating_sub(1);
        match key.code {
            KeyCode::Char('a') if ctrl => self.select_all(),
            KeyCode::Up | KeyCode::Char('k') => self.move_rows(-1, shift),
            KeyCode::Down | KeyCode::Char('j') => self.move_rows(1, shift),
            KeyCode::PageUp => self.move_rows(-page, shift),
            KeyCode::PageDown => self.move_rows(page, shift),
            KeyCode::Home | KeyCode::Char('g') => self.move_to(0, shift),
            KeyCode::End | KeyCode::Char('G') => self.move_to(last, shift),
            KeyCode::Left | KeyCode::Char('h') => self.move_col(-1, shift),
            KeyCode::Right | KeyCode::Char('l') => self.move_col(1, shift),
            KeyCode::Char(' ') => self.toggle_current(),
            KeyCode::Char('s') => self.sort_by_active(),
            KeyCode::Char('f') => self.toggle_filter_scope(),
            KeyCode::Char('m') => self.toggle_mode(),
            KeyCode::Char('/') => self.filtering = true,
            KeyCode::Esc => self.clear_selection(),
            KeyCode::Enter => return TableAction::Activate,
            _ => {}
        }
        TableAction::None
    }

    fn handle_filter_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.filtering = false;
                self.filter.clear();
                self.rebuild_view();
            }
            KeyCode::Enter => self.filtering = false,
            KeyCode::Backspace => {
                self.filter.pop();
                self.rebuild_view();
            }
            KeyCode::Char(ch) => {
                self.filter.push(ch);
                self.rebuild_view();
            }
            _ => {}
        }
    }

    fn move_rows(&mut self, delta: isize, shift: bool) {
        let target = nav::step_clamped(self.cursor, self.view.len(), delta);
        self.move_to(target, shift);
    }

    fn move_to(&mut self, target: usize, shift: bool) {
        if self.view.is_empty() {
            return;
        }
        let prev = self.cursor;
        self.cursor = target.min(self.view.len() - 1);
        self.after_move(prev, shift);
    }

    fn move_col(&mut self, delta: isize, shift: bool) {
        if self.columns.is_empty() {
            return;
        }
        let prev_cell = self.cursor_cell();
        self.active_col =
            nav::step_clamped(self.active_col, self.columns.len(), delta);
        if shift && self.mode == SelectMode::Cell {
            if let Some(cell) = prev_cell {
                self.selected_cells.insert(cell);
            }
            if let Some(cell) = self.cursor_cell() {
                self.selected_cells.insert(cell);
            }
        }
    }

    /// Applies selection side effects after the cursor row moved.
    fn after_move(&mut self, prev: usize, shift: bool) {
        if !shift {
            self.anchor = self.cursor;
            return;
        }
        match self.mode {
            SelectMode::Row => {
                let (lo, hi) = minmax(self.anchor, self.cursor);
                for view_idx in lo..=hi {
                    self.selected_rows.insert(self.view[view_idx]);
                }
            }
            SelectMode::Cell => {
                for view_idx in [prev, self.cursor] {
                    self.selected_cells
                        .insert((self.view[view_idx], self.active_col));
                }
            }
        }
    }

    fn toggle_current(&mut self) {
        match self.mode {
            SelectMode::Row => {
                if let Some(row) = self.cursor_row() {
                    toggle(&mut self.selected_rows, row);
                }
            }
            SelectMode::Cell => {
                if let Some(cell) = self.cursor_cell() {
                    toggle(&mut self.selected_cells, cell);
                }
            }
        }
    }

    fn select_all(&mut self) {
        match self.mode {
            SelectMode::Row => {
                self.selected_rows = self.view.iter().copied().collect();
            }
            SelectMode::Cell => {
                self.selected_cells = self
                    .view
                    .iter()
                    .flat_map(|&row| {
                        (0..self.columns.len()).map(move |col| (row, col))
                    })
                    .collect();
            }
        }
    }

    fn clear_selection(&mut self) {
        self.selected_rows.clear();
        self.selected_cells.clear();
    }

    fn toggle_mode(&mut self) {
        self.mode = match self.mode {
            SelectMode::Row => SelectMode::Cell,
            SelectMode::Cell => SelectMode::Row,
        };
        // Selections do not translate between the modes.
        self.clear_selection();
    }

    fn toggle_filter_scope(&mut self) {
        self.filter_scope = match self.filter_scope {
            FilterScope::AllColumns => FilterScope::ActiveColumn,
            FilterScope::ActiveColumn => FilterScope::AllColumns,
        };
        self.rebuild_view();
    }

    fn sort_by_active(&mut self) {
        let dir = match self.sort {
            Some((col, SortDir::Asc)) if col == self.active_col => {
                SortDir::Desc
            }
            _ => SortDir::Asc,
        };
        self.sort = Some((self.active_col, dir));
        self.rebuild_view();
    }

    /// Recomputes `view` (filter then sort), keeping the cursor on its row.
    fn rebuild_view(&mut self) {
        let keep = self.cursor_row();
        let mut view = filter_indices(
            &self.rows,
            &self.filter,
            self.filter_scope,
            self.active_col,
        );
        if let Some((col, dir)) = self.sort {
            view = sort_indices(&self.rows, &self.columns, view, col, dir);
        }
        self.view = view;
        self.cursor = keep
            .and_then(|row| self.view.iter().position(|&r| r == row))
            .unwrap_or(0)
            .min(self.view.len().saturating_sub(1));
        self.anchor = self.cursor;
    }

    /// Whether the filter input is active (the host modal yields keys to it).
    pub fn is_filtering(&self) -> bool {
        self.filtering
    }

    /// Renders the table: header, multi-line body with selection/cursor styling,
    /// scrollbar and an optional status/filter line.
    pub fn render(&self, frame: &mut Frame, area: Rect, skin: &Skin) {
        let area = match &self.decor {
            Some(decor) if self.force_box || skin.is_boxed() => {
                chrome::framed_decor(
                    frame,
                    area,
                    skin,
                    decor,
                    &self.view.len().to_string(),
                )
            }
            _ => area,
        };
        let bottom = u16::from(self.filtering || self.show_status);
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Min(0),
                Constraint::Length(bottom),
            ])
            .split(area);
        let body = chunks[1];

        let specs: Vec<(usize, usize)> =
            self.columns.iter().map(|c| (c.min, c.target)).collect();
        let avail = (body.width as usize).saturating_sub(1 + GUTTER);
        let widths = allocate_columns(&specs, avail);

        frame.render_widget(
            Paragraph::new(self.header_line(&widths, skin)),
            chunks[0],
        );

        let heights: Vec<u16> = self
            .view
            .iter()
            .map(|&row| self.row_height(row, &widths))
            .collect();
        let offset = visible_offset(
            &heights,
            self.cursor,
            body.height,
            self.offset.get(),
        );
        self.offset.set(offset);

        let mut lines: Vec<Line> = Vec::new();
        let mut used = 0u16;
        let mut count = 0usize;
        for (view_idx, &height) in heights.iter().enumerate().skip(offset) {
            if used + height > body.height {
                break;
            }
            lines.extend(self.row_lines(
                view_idx,
                &widths,
                body.width as usize,
                skin,
            ));
            used += height;
            count += 1;
        }
        self.viewport.set(count.max(1));
        frame.render_widget(Paragraph::new(lines), body);
        scroll::render_scrollbar(
            frame,
            body,
            self.view.len(),
            offset,
            count.max(1),
        );

        if bottom > 0 {
            frame.render_widget(
                Paragraph::new(self.bottom_line(skin)),
                chunks[2],
            );
        }
    }

    fn header_line(&self, widths: &[usize], skin: &Skin) -> Line<'static> {
        let mut spans: Vec<Span> = vec![Span::raw(" ".repeat(GUTTER))];
        for (col, (column, &width)) in
            self.columns.iter().zip(widths).enumerate()
        {
            if col > 0 {
                spans.push(Span::raw(SEPARATOR));
            }
            let mut title = column.title.clone();
            if let Some((sorted, dir)) = self.sort
                && sorted == col
            {
                let arrow = if dir == SortDir::Asc {
                    '\u{25b2}'
                } else {
                    '\u{25bc}'
                };
                title = format!("{title} {arrow}");
            }
            let mut style = column.header_style.unwrap_or(self.header_style);
            if col == self.active_col {
                style =
                    style::fg(skin.palette.accent).add_modifier(Modifier::BOLD);
            }
            spans.push(Span::styled(
                pad_align(&truncate(&title, width), width, column.align),
                style,
            ));
        }
        Line::from(spans)
    }

    fn row_height(&self, row: usize, widths: &[usize]) -> u16 {
        let row = &self.rows[row];
        let height = (0..self.columns.len())
            .map(|col| {
                if self.columns[col].wrap {
                    wrap_cell(row.cell(col), widths[col]).len()
                } else {
                    1
                }
            })
            .max()
            .unwrap_or(1);
        height.max(1) as u16
    }

    fn row_lines(
        &self,
        view_idx: usize,
        widths: &[usize],
        line_width: usize,
        skin: &Skin,
    ) -> Vec<Line<'static>> {
        let orig = self.view[view_idx];
        let row = &self.rows[orig];
        let is_cursor = view_idx == self.cursor;
        let row_bg = self.row_highlight(orig, is_cursor, skin);
        let sublines: Vec<Vec<String>> = self
            .columns
            .iter()
            .enumerate()
            .map(|(col, column)| {
                if column.wrap {
                    wrap_cell(row.cell(col), widths[col])
                } else {
                    vec![truncate(row.cell(col), widths[col])]
                }
            })
            .collect();
        let height = sublines.iter().map(Vec::len).max().unwrap_or(1).max(1);

        // Content width already laid out before the trailing fill: gutter plus
        // every column and the separators between them.
        let separators = SEPARATOR.width() * widths.len().saturating_sub(1);
        let content_width = GUTTER + widths.iter().sum::<usize>() + separators;

        (0..height)
            .map(|line_no| {
                let mut gutter = self.gutter_span(orig, line_no, skin);
                if let Some(bg) = row_bg {
                    gutter.style = gutter.style.patch(bg);
                }
                let mut spans: Vec<Span> = vec![gutter];
                for (col, &width) in widths.iter().enumerate() {
                    if col > 0 {
                        spans.push(Span::styled(
                            SEPARATOR,
                            row_bg.unwrap_or_default(),
                        ));
                    }
                    let text =
                        sublines[col].get(line_no).cloned().unwrap_or_default();
                    let style = self.cell_style(orig, col, is_cursor, skin);
                    spans.push(Span::styled(
                        pad_align(&text, width, self.columns[col].align),
                        style,
                    ));
                }
                // Extend the highlight bar to the right edge so it reads as one
                // continuous row rather than per-cell patches.
                if let Some(bg) = row_bg {
                    let trailing = line_width.saturating_sub(content_width);
                    if trailing > 0 {
                        spans.push(Span::styled(" ".repeat(trailing), bg));
                    }
                }
                Line::from(spans)
            })
            .collect()
    }

    /// The full-width background for a row's highlight bar, or `None` when the
    /// row is neither the cursor nor selected. Mirrors the background that
    /// [`Self::cell_style`] applies per cell so the bar reads as one strip;
    /// only meaningful in [`SelectMode::Row`] (cell mode highlights per cell).
    fn row_highlight(
        &self,
        orig: usize,
        is_cursor: bool,
        skin: &Skin,
    ) -> Option<Style> {
        if self.mode != SelectMode::Row {
            return None;
        }
        if is_cursor {
            return Some(cursor_highlight(skin));
        }
        if self.selected_rows.contains(&orig) {
            return Some(style::bg(skin.palette.selection_bg));
        }
        None
    }

    /// The 2-cell left gutter: a check marker for selected rows (row mode).
    fn gutter_span(
        &self,
        orig: usize,
        line_no: usize,
        skin: &Skin,
    ) -> Span<'static> {
        let selected =
            self.mode == SelectMode::Row && self.selected_rows.contains(&orig);
        let text = if line_no == 0 && selected {
            format!("{} ", skin.glyphs.check)
        } else {
            " ".repeat(GUTTER)
        };
        Span::styled(text, style::fg(skin.palette.accent))
    }

    fn cell_style(
        &self,
        orig: usize,
        col: usize,
        is_cursor: bool,
        skin: &Skin,
    ) -> Style {
        let mut style = Style::default();
        if let Some(cs) = self.columns[col].cell_style {
            style = style.patch(cs);
        }
        if let Some(rs) = self.rows[orig].style {
            style = style.patch(rs);
        }
        let selected = match self.mode {
            SelectMode::Row => self.selected_rows.contains(&orig),
            SelectMode::Cell => self.selected_cells.contains(&(orig, col)),
        };
        if selected {
            style = style.patch(style::bg(skin.palette.selection_bg));
        }
        let cursor_here = is_cursor
            && (self.mode == SelectMode::Row || col == self.active_col);
        if cursor_here {
            style = style.patch(cursor_highlight(skin));
        }
        style
    }

    fn bottom_line(&self, skin: &Skin) -> Line<'static> {
        let palette = &skin.palette;
        if self.filtering {
            return Line::from(vec![
                Span::styled("/", style::fg(palette.accent)),
                Span::raw(self.filter.clone()),
                Span::styled(" ", style::bg(palette.cursor)),
            ]);
        }
        let count = match self.mode {
            SelectMode::Row => self.selected_rows().len(),
            SelectMode::Cell => self.selected_cells.len(),
        };
        let position = if self.view.is_empty() {
            0
        } else {
            self.cursor + 1
        };
        let mut text = format!(
            " row {position}/{} \u{b7} {count} selected",
            self.view.len()
        );
        if let Some((col, dir)) = self.sort {
            let arrow = if dir == SortDir::Asc {
                '\u{25b2}'
            } else {
                '\u{25bc}'
            };
            let title = self.columns.get(col).map_or("", |c| c.title.as_str());
            let _ = write!(text, " \u{b7} sort: {title}{arrow}");
        }
        if !self.filter.is_empty() {
            let _ = write!(text, " \u{b7} filter: {}", self.filter);
        }
        Line::from(Span::styled(text, style::dim()))
    }
}

/// The cursor-row highlight: a bold accent bar in `Boxed`, a tint in `Minimal`.
fn cursor_highlight(skin: &Skin) -> Style {
    if skin.is_boxed() {
        style::bg(skin.palette.accent_dark)
            .fg(style::to_ratatui(skin.palette.accent))
            .add_modifier(Modifier::BOLD)
    } else {
        style::bg(skin.palette.selection_bg)
    }
}

/// Pads `text` to `width` display columns on the side opposite `align`.
fn pad_align(text: &str, width: usize, align: Align) -> String {
    let pad = width.saturating_sub(text.width());
    match align {
        Align::Left => format!("{text}{}", " ".repeat(pad)),
        Align::Right => format!("{}{text}", " ".repeat(pad)),
    }
}

/// Runs [`Table`] in a blocking modal; `Enter` returns the selected original row
/// indices (or the cursor row when nothing is selected), `Esc` cancels.
pub fn table_select(
    tui: &mut Tui,
    skin: &Skin,
    title: &str,
    columns: Vec<Column>,
    rows: Vec<Row>,
    render_bg: impl Fn(&mut Frame),
) -> io::Result<ModalSignal<Vec<usize>>> {
    let mut table = Table::new(columns, rows);
    popup(
        tui,
        &mut table,
        |area, _| {
            centered_rect(
                (area.width * 3 / 4).clamp(40, area.width),
                (area.height * 3 / 4).clamp(8, area.height),
                area,
            )
        },
        |frame, _| render_bg(frame),
        |frame, rect, table: &Table| {
            let inner = overlay::framed(frame, rect, skin, title);
            table.render(frame, inner, skin);
        },
        |table, key| {
            // While the filter input is open every key edits it.
            if table.is_filtering() {
                table.handle_key(key);
                return PopupFlow::Continue;
            }
            match key.code {
                KeyCode::Esc => PopupFlow::Cancelled,
                KeyCode::Enter => {
                    let mut picked = table.selected_rows();
                    if picked.is_empty() {
                        picked.extend(table.cursor_row());
                    }
                    PopupFlow::Done(picked)
                }
                _ => {
                    table.handle_key(key);
                    PopupFlow::Continue
                }
            }
        },
    )
}

fn toggle<T: Eq + std::hash::Hash>(set: &mut HashSet<T>, value: T) {
    if !set.remove(&value) {
        set.insert(value);
    }
}

fn minmax(a: usize, b: usize) -> (usize, usize) {
    if a <= b { (a, b) } else { (b, a) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn columns() -> Vec<Column> {
        vec![Column::text("Title"), Column::number("Amount")]
    }

    fn rows() -> Vec<Row> {
        vec![
            Row::new(vec!["beta".into(), "9".into()]).with_key("b"),
            Row::new(vec!["alpha".into(), "10".into()]).with_key("a"),
            Row::new(vec!["gamma".into(), "100".into()]).with_key("g"),
        ]
    }

    #[test]
    fn shares_leftover_up_to_target() {
        assert_eq!(allocate_columns(&[(3, 10), (3, 10)], 20), vec![9, 9]);
    }

    fn all_indices() -> Vec<usize> {
        (0..rows().len()).collect()
    }

    #[test]
    fn number_column_sorts_numerically() {
        let order =
            sort_indices(&rows(), &columns(), all_indices(), 1, SortDir::Asc);
        // 9, 10, 100 -> rows 0, 1, 2 (not lexicographic 10,100,9).
        assert_eq!(order, vec![0, 1, 2]);
    }

    #[test]
    fn text_column_sorts_lexicographically_and_desc_reverses() {
        let asc =
            sort_indices(&rows(), &columns(), all_indices(), 0, SortDir::Asc);
        assert_eq!(asc, vec![1, 0, 2]); // alpha, beta, gamma
        let desc =
            sort_indices(&rows(), &columns(), all_indices(), 0, SortDir::Desc);
        assert_eq!(desc, vec![2, 0, 1]);
    }

    #[test]
    fn filter_matches_across_columns() {
        let hits = filter_indices(&rows(), "alp", FilterScope::AllColumns, 0);
        assert_eq!(hits, vec![1]);
    }

    #[test]
    fn empty_filter_keeps_all_rows() {
        let hits = filter_indices(&rows(), "", FilterScope::AllColumns, 0);
        assert_eq!(hits, vec![0, 1, 2]);
    }

    #[test]
    fn wrap_cell_breaks_on_width() {
        assert_eq!(wrap_cell("abcdef", 3), vec!["abc", "def"]);
        assert_eq!(wrap_cell("", 3), vec![String::new()]);
    }

    #[test]
    fn visible_offset_keeps_cursor_in_view() {
        let heights = vec![1u16; 10];
        assert_eq!(visible_offset(&heights, 5, 3, 0), 3);
        assert_eq!(visible_offset(&heights, 2, 3, 5), 2);
    }

    #[test]
    fn space_toggles_row_selection() {
        let mut table = Table::new(columns(), rows());
        table.handle_key(key(KeyCode::Char(' ')));
        assert_eq!(table.selected_rows(), vec![0]);
        table.handle_key(key(KeyCode::Char(' ')));
        assert!(table.selected_rows().is_empty());
    }

    #[test]
    fn ctrl_a_selects_all_then_esc_clears() {
        let mut table = Table::new(columns(), rows());
        table.handle_key(KeyEvent::new(
            KeyCode::Char('a'),
            KeyModifiers::CONTROL,
        ));
        assert_eq!(table.selected_rows(), vec![0, 1, 2]);
        table.handle_key(key(KeyCode::Esc));
        assert!(table.selected_rows().is_empty());
    }

    #[test]
    fn shift_down_extends_a_row_range() {
        let mut table = Table::new(columns(), rows());
        table.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::SHIFT));
        // Range from anchor (0) to new cursor (1).
        assert_eq!(table.selected_rows(), vec![0, 1]);
    }

    #[test]
    fn keys_select_skip_rows_without_keys() {
        let mut table = Table::new(columns(), rows());
        table.handle_key(KeyEvent::new(
            KeyCode::Char('a'),
            KeyModifiers::CONTROL,
        ));
        assert_eq!(table.selected_keys(), vec!["b", "a", "g"]);
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }
}

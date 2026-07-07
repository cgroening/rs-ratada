//! An interactive table: typed sorting, fuzzy filtering, row/cell multi-select,
//! multi-line rows and data-driven styling.
//!
//! [`Table`] is a stateful inline widget (the host calls [`Table::handle_key`]
//! and [`Table::render`] and reads the selection back); [`table_select`] wraps
//! it in a blocking modal. Selection is reported both as original row indices
//! and as host-provided keys.
//!
//! The widget is split across submodules: the data/layout helpers in `model`,
//! key dispatch/navigation/selection in `interaction`, and drawing in `render`.

use std::{cell::Cell, collections::HashSet, io};

use crossterm::event::KeyCode;
use ratatui::{
    Frame,
    style::{Modifier, Style},
};

use super::{
    chrome,
    layout::centered_fraction,
    modal::ModalSignal,
    overlay::{self, PopupFlow, popup},
    terminal::Tui,
};
use crate::theme::Skin;

mod interaction;
mod model;
mod render;

const SEPARATOR: &str = "  ";
/// Left gutter (selection marker) width.
const GUTTER: usize = 2;

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
            // Bold on the `panel` header band; overridable per table/column.
            header_style: Style::default().add_modifier(Modifier::BOLD),
            show_status: true,
            decor: None,
        }
    }

    /// Draws the table inside a rounded box with the given caption/badge (see
    /// [`chrome::BoxDecor`]); the caption sits in the top border and the
    /// row-count badge bottom-right. The inner status/filter line is kept. Omit
    /// it for a plain table.
    #[must_use]
    pub fn boxed(mut self, decor: chrome::BoxDecor) -> Self {
        self.decor = Some(decor);
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
        |area, _| centered_fraction(area, 3, 4, 40, 8),
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

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyEvent, KeyModifiers};

    use super::model::{
        allocate_columns, filter_indices, sort_indices, visible_offset,
        wrap_cell,
    };
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

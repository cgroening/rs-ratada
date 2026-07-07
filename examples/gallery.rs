//! The widget gallery used for the README screenshot: a single, static frame
//! that composes a cross-section of the toolkit - header, tab bar, a
//! three-column dashboard (boxed table | boxed tree + list | Markdown view), a
//! gauge, a shortcut-hint footer and a status bar - over one `Skin`.
//!
//! It is deliberately non-interactive: it draws one frame and quits on the next
//! key press, so you can capture it cleanly. Run it with `cargo run --example
//! gallery`, size the terminal to taste (roughly 100x30 reads well), take the
//! screenshot, then press any key (or `Ctrl+Q`) to leave.

use std::cell::Cell;

use crossterm::event::KeyEvent;
use ratada::prelude::*;
use ratada::theme::{
    ColorOverrides, GlyphVariant, Glyphs, Palette, Skin, ThemeRegistry,
};
use ratada::{
    chrome, gauge, header,
    list::{self, ListView},
    markdown::MarkdownView,
    shortcut_hints::{self, HintGroup, HintStyle},
    spinner::Spinner,
    statusbar, style,
    table::{Column, Row, Table},
    tabs,
    tree::{TreeItem, TreeView},
};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::Modifier,
    text::Line,
};

/// The brand shown in the header and tab bar.
const BRAND: &str = "ratada";

/// The tab bar's segments (`key`, `label`); the second tab renders active.
const TABS: &[(&str, &str)] =
    &[("1", "Overview"), ("2", "Gallery"), ("3", "Settings")];

/// Fill ratio of the demo gauge (a fixed value keeps the screenshot stable).
const GAUGE_RATIO: f64 = 0.62;

/// The Markdown document shown in the right-hand viewer.
const README: &str = "\
# ratada

A reusable **ratatui** widget toolkit.

- modals, forms, pickers
- tables, trees, lists
- a themeable `Skin`

> [!NOTE]
> One frame, many widgets.
";

/// The whole gallery state: the animated widgets frozen at a fixed frame plus
/// the list's scroll offset.
struct Gallery {
    spinner: Spinner,
    list_offset: Cell<usize>,
}

impl Gallery {
    fn new() -> Self {
        Self {
            spinner: Spinner::new(),
            list_offset: Cell::new(0),
        }
    }

    /// Splits the frame into header, tab bar, dashboard body, gauge, footer
    /// hints and status bar, then draws each region.
    fn render_dashboard(&self, frame: &mut Frame, area: Rect, skin: &Skin) {
        let tab_height = tabs::height(BRAND, TABS, area.width as usize);
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),          // header
                Constraint::Length(tab_height), // tab bar
                Constraint::Min(0),             // dashboard
                Constraint::Length(1),          // gauge
                Constraint::Length(2),          // shortcut hints
                Constraint::Length(1),          // status bar
            ])
            .split(area);

        header::render(frame, rows[0], skin, BRAND, "widget gallery");
        tabs::render(frame, rows[1], skin, BRAND, TABS, 1);
        self.render_body(frame, rows[2], skin);
        gauge::render(
            frame,
            rows[3],
            &skin.palette,
            GAUGE_RATIO,
            "Indexing 62%",
        );
        Self::render_hints(frame, rows[4], skin);
        self.render_status(frame, rows[5], skin);
    }

    /// The three-column dashboard: a boxed table, a boxed tree above a boxed
    /// list, and a Markdown view.
    fn render_body(&self, frame: &mut Frame, area: Rect, skin: &Skin) {
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .spacing(1)
            .constraints([
                Constraint::Percentage(40),
                Constraint::Percentage(28),
                Constraint::Percentage(32),
            ])
            .split(area);

        Self::render_table(frame, columns[0], skin);
        self.render_middle(frame, columns[1], skin);
        Self::render_markdown(frame, columns[2], skin);
    }

    /// Left column: a boxed, type-aware task table.
    fn render_table(frame: &mut Frame, area: Rect, skin: &Skin) {
        let columns = vec![
            Column::text("Id").widths(4, 6),
            Column::text("Title").widths(10, 24).wrap(true),
            Column::number("Amount").widths(6, 8),
            Column::date("Due").widths(10, 10),
        ];
        let rows = vec![
            Row::new(vec![
                "4acc".into(),
                "Write the quarterly report".into(),
                "120".into(),
                "2026-06-25".into(),
            ]),
            Row::new(vec![
                "2fca".into(),
                "Pay rent".into(),
                "900".into(),
                "2026-07-01".into(),
            ])
            .with_style(style::dim()),
            Row::new(vec![
                "9cf7".into(),
                "Read the docs".into(),
                "30".into(),
                "2026-05-01".into(),
            ]),
        ];
        Table::new(columns, rows)
            .boxed(chrome::BoxDecor::new().caption("Tasks"))
            .render(frame, area, skin);
    }

    /// Middle column: a boxed file tree above a boxed fruit list.
    fn render_middle(&self, frame: &mut Frame, area: Rect, skin: &Skin) {
        let split = Layout::default()
            .direction(Direction::Vertical)
            .spacing(1)
            .constraints([
                Constraint::Percentage(50),
                Constraint::Percentage(50),
            ])
            .split(area);

        TreeView::new(sample_tree())
            .boxed(chrome::BoxDecor::new().caption("Files"))
            .render(frame, split[0], skin);

        list::render_boxed(
            frame,
            split[1],
            skin,
            ListView {
                rows: fruit_rows(),
                selected: 1,
                offset: &self.list_offset,
            },
            &chrome::BoxDecor::new().caption("Fruit"),
            true,
        );
    }

    /// Right column: the scrollable Markdown view.
    fn render_markdown(frame: &mut Frame, area: Rect, skin: &Skin) {
        MarkdownView::new(README)
            .boxed(chrome::BoxDecor::new().caption("README"))
            .render(frame, area, skin);
    }

    /// The footer: two label-aligned shortcut-hint groups with accent keys.
    fn render_hints(frame: &mut Frame, area: Rect, skin: &Skin) {
        let navigation = [("j/k", "move"), ("g/G", "top/bottom")];
        let actions = [("a", "add"), ("d", "delete"), ("?", "help")];
        let groups = [
            HintGroup {
                label: "Navigation",
                hints: &navigation,
            },
            HintGroup {
                label: "Actions",
                hints: &actions,
            },
        ];
        shortcut_hints::render(
            frame,
            area,
            &groups,
            &HintStyle {
                key: style::fg(skin.palette.accent)
                    .add_modifier(Modifier::BOLD),
                ..HintStyle::default()
            },
        );
    }

    /// The bottom status bar, with the spinner glyph on the left.
    fn render_status(&self, frame: &mut Frame, area: Rect, skin: &Skin) {
        let left = format!(
            " {} 3 tasks \u{b7} 1 done \u{b7} 2 open",
            self.spinner.frame(skin.glyphs.variant)
        );
        statusbar::render(
            frame,
            area,
            skin,
            skin.palette.selection,
            &left,
            "saved ",
        );
    }
}

impl Screen for Gallery {
    type Error = std::io::Error;

    fn render(&self, frame: &mut Frame) {
        self.render_dashboard(frame, frame.area(), &default_skin());
    }

    fn handle_key(
        &mut self,
        _key: KeyEvent,
        _tui: &mut Tui,
    ) -> std::io::Result<Flow> {
        // Any key leaves; the frame is static, so there is nothing to drive.
        Ok(Flow::Quit)
    }
}

/// Builds the default built-in `Skin` (default palette, Unicode glyphs).
fn default_skin() -> Skin {
    let base = ThemeRegistry::builtin().resolve("default");
    Skin::new(
        Palette::resolve(base, &ColorOverrides::default()),
        Glyphs::new(GlyphVariant::Unicode),
    )
}

/// A small example file tree for the middle column.
fn sample_tree() -> Vec<TreeItem> {
    vec![
        TreeItem::node(
            "src",
            vec![
                TreeItem::node(
                    "table",
                    vec![
                        TreeItem::leaf("model.rs"),
                        TreeItem::leaf("render.rs"),
                    ],
                ),
                TreeItem::leaf("lib.rs"),
            ],
        ),
        TreeItem::leaf("Cargo.toml"),
    ]
}

/// The rows for the boxed fruit list.
fn fruit_rows() -> Vec<Line<'static>> {
    ["Apple", "Banana", "Cherry", "Date", "Elderberry", "Fig"]
        .iter()
        .map(|item| Line::from(format!(" {item}")))
        .collect()
}

fn main() -> std::io::Result<()> {
    let mut tui = Tui::new()?;
    run(&mut tui, &mut Gallery::new())
}

//! Standalone render smoke tests: every frame-based widget must render without
//! panicking across modes, styles and sizes (including cramped viewports and
//! wide characters). Popups drive a real terminal loop and are covered by unit
//! tests instead.

use std::cell::Cell;

use ratatui::{Frame, Terminal, backend::TestBackend, text::Line};

use ratada::{
    chrome::BoxDecor,
    input::InputField,
    list,
    table::{Column, Row, Table},
    textarea::TextArea,
    theme::{
        ColorOverrides, GlyphVariant, Glyphs, Mode, Palette, Skin,
        ThemeRegistry,
    },
    tree::{TreeItem, TreeView},
};

/// The default-theme skin in `mode`, Unicode glyphs.
fn skin(mode: Mode) -> Skin {
    let base = ThemeRegistry::builtin().resolve("default");
    Skin::new(
        Palette::resolve(base, &ColorOverrides::default()),
        Glyphs::new(GlyphVariant::Unicode),
        mode,
    )
}

/// Renders `render` into a `width`x`height` test terminal; panics propagate.
fn draw(width: u16, height: u16, render: impl FnOnce(&mut Frame)) {
    let mut terminal =
        Terminal::new(TestBackend::new(width, height)).expect("backend");
    terminal.draw(render).expect("draw");
}

/// A roomy viewport and a cramped one, to exercise overflow and clamping.
const SIZES: [(u16, u16); 2] = [(80, 24), (4, 3)];

#[test]
fn widgets_render_across_modes_styles_and_sizes() {
    for mode in Mode::ALL {
        let skin = skin(mode);
        for (width, height) in SIZES {
            let offset = Cell::new(0);
            draw(width, height, |frame| {
                let rows: Vec<Line<'static>> =
                    ["Apple", "世界 wide", "🚀 launch"]
                        .iter()
                        .map(|item| Line::from(*item))
                        .collect();
                list::render(frame, frame.area(), &skin, rows, 1, &offset);
            });

            for boxed in [false, true] {
                draw(width, height, |frame| {
                    let mut table = Table::new(
                        vec![
                            Column::text("Name").widths(4, 12),
                            Column::number("N"),
                        ],
                        vec![
                            Row::new(vec!["世界".into(), "1".into()]),
                            Row::new(vec!["b".into(), "2".into()]),
                        ],
                    );
                    if boxed {
                        table =
                            table.boxed_always(BoxDecor::new().caption("Tbl"));
                    }
                    table.render(frame, frame.area(), &skin);
                });

                draw(width, height, |frame| {
                    let mut tree = TreeView::new(vec![
                        TreeItem::node("src", vec![TreeItem::leaf("main.rs")]),
                        TreeItem::leaf("Cargo.toml"),
                    ]);
                    if boxed {
                        tree =
                            tree.boxed_always(BoxDecor::new().caption("Tree"));
                    }
                    tree.render(frame, frame.area(), &skin);
                });
            }

            for focused in [false, true] {
                for boxed in [false, true] {
                    draw(width, height, |frame| {
                        let mut field =
                            InputField::new("世界🚀 mix").max_len(40);
                        if boxed {
                            field = field
                                .boxed_always(BoxDecor::new().caption("In"));
                        }
                        field.render(frame, frame.area(), &skin, focused);
                    });
                    draw(width, height, |frame| {
                        let mut area =
                            TextArea::new("世界 wide\nsecond line\n🚀");
                        if boxed {
                            area = area
                                .boxed_always(BoxDecor::new().caption("Ta"));
                        }
                        area.render(frame, frame.area(), &skin, focused);
                    });
                }
            }
        }
    }
}

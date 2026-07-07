//! Standalone render smoke tests: every frame-based widget must render without
//! panicking across styles and sizes (including cramped viewports and wide
//! characters). Popups drive a real terminal loop and are covered by unit
//! tests instead.

use std::cell::Cell;

use ratatui::{Frame, Terminal, backend::TestBackend, text::Line};

use ratada::{
    chrome::BoxDecor,
    gauge, header,
    input::InputField,
    list, shortcut_hints,
    shortcut_hints::{HintGroup, HintStyle},
    statusbar,
    table::{Column, Row, Table},
    tabs,
    textarea::TextArea,
    theme::{
        ColorOverrides, GlyphVariant, Glyphs, Palette, Skin, ThemeRegistry,
    },
    theme_preview,
    toast::{ToastKind, Toasts},
    tree::{TreeItem, TreeView},
};

/// The default-theme skin with Unicode glyphs.
fn skin() -> Skin {
    let base = ThemeRegistry::builtin().resolve("default");
    Skin::new(
        Palette::resolve(base, &ColorOverrides::default()),
        Glyphs::new(GlyphVariant::Unicode),
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
fn widgets_render_across_styles_and_sizes() {
    let skin = skin();
    for (width, height) in SIZES {
        let offset = Cell::new(0);
        draw(width, height, |frame| {
            let rows: Vec<Line<'static>> = ["Apple", "世界 wide", "🚀 launch"]
                .iter()
                .map(|item| Line::from(*item))
                .collect();
            list::render(
                frame,
                frame.area(),
                &skin,
                list::ListView {
                    rows,
                    selected: 1,
                    offset: &offset,
                },
            );
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
                    table = table.boxed(BoxDecor::new().caption("Tbl"));
                }
                table.render(frame, frame.area(), &skin);
            });

            draw(width, height, |frame| {
                let mut tree = TreeView::new(vec![
                    TreeItem::node("src", vec![TreeItem::leaf("main.rs")]),
                    TreeItem::leaf("Cargo.toml"),
                ]);
                if boxed {
                    tree = tree.boxed(BoxDecor::new().caption("Tree"));
                }
                tree.render(frame, frame.area(), &skin);
            });
        }

        for focused in [false, true] {
            for boxed in [false, true] {
                draw(width, height, |frame| {
                    let mut field = InputField::new("世界🚀 mix").max_len(40);
                    if boxed {
                        field = field.boxed(BoxDecor::new().caption("In"));
                    }
                    field.render(frame, frame.area(), &skin, focused);
                });
                draw(width, height, |frame| {
                    let mut area = TextArea::new("世界 wide\nsecond line\n🚀");
                    if boxed {
                        area = area.boxed(BoxDecor::new().caption("Ta"));
                    }
                    area.render(frame, frame.area(), &skin, focused);
                });
            }
        }
    }
}

#[test]
fn chrome_widgets_render_across_sizes() {
    let skin = skin();
    for (width, height) in SIZES {
        draw(width, height, |frame| {
            tabs::render(
                frame,
                frame.area(),
                &skin,
                "Brand",
                &[("1", "One"), ("2", "Two 世界"), ("3", "Three")],
                1,
            );
        });

        draw(width, height, |frame| {
            header::render(frame, frame.area(), &skin, "Brand", "status 世界");
        });

        draw(width, height, |frame| {
            statusbar::render(
                frame,
                frame.area(),
                &skin,
                skin.palette.panel,
                "left 世界",
                "right",
            );
        });

        for ratio in [0.0, 0.5, 1.5] {
            draw(width, height, |frame| {
                gauge::render(
                    frame,
                    frame.area(),
                    &skin.palette,
                    ratio,
                    "load 世界",
                );
            });
        }

        draw(width, height, |frame| {
            let groups = [
                HintGroup {
                    label: "File",
                    hints: &[("s", "save"), ("q", "quit 世界")],
                },
                HintGroup {
                    label: "Edit",
                    hints: &[("u", "undo")],
                },
            ];
            shortcut_hints::render(
                frame,
                frame.area(),
                &groups,
                &HintStyle::default(),
            );
        });

        draw(width, height, |frame| {
            let mut toasts = Toasts::new();
            toasts.push(ToastKind::Info, "info 世界");
            toasts.push(ToastKind::Error, "error");
            toasts.render(frame, frame.area(), &skin);
        });

        draw(width, height, |frame| {
            theme_preview::render(frame, frame.area(), &skin);
        });
    }
}

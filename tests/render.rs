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
    list,
    markdown::MarkdownView,
    shortcut_hints,
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

/// Like [`draw`], but returns the terminal's bottom row as a string.
fn draw_bottom_row(
    width: u16,
    height: u16,
    render: impl FnOnce(&mut Frame),
) -> String {
    let mut terminal =
        Terminal::new(TestBackend::new(width, height)).expect("backend");
    terminal.draw(render).expect("draw");
    let buffer = terminal.backend().buffer();
    (0..width)
        .map(|x| buffer[(x, height - 1)].symbol())
        .collect()
}

/// Draws a twelve-row list with the third row selected, boxed or bare.
fn draw_list_bottom_row(width: u16, height: u16, boxed: bool) -> String {
    let skin = skin();
    let offset = Cell::new(0);
    draw_bottom_row(width, height, |frame| {
        let rows: Vec<Line<'static>> =
            (1..=12).map(|n| Line::from(format!("row {n}"))).collect();
        let view = list::ListView {
            rows,
            selected: 2,
            offset: &offset,
        };
        list::render_boxed(
            frame,
            frame.area(),
            &skin,
            view,
            &BoxDecor::new(),
            boxed,
        );
    })
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
fn boxed_list_puts_the_position_badge_in_the_bottom_border() {
    let row = draw_list_bottom_row(20, 6, true);
    let expected =
        format!("\u{2570}{} 3/12 \u{2500}\u{256f}", "\u{2500}".repeat(11));
    assert_eq!(row, expected);
}

#[test]
fn a_bare_list_draws_no_position_badge() {
    let row = draw_list_bottom_row(20, 6, false);
    assert!(!row.contains("3/12"), "unexpected badge in {row:?}");
}

#[test]
fn a_box_too_narrow_for_the_badge_keeps_its_border_intact() {
    // The badge needs its own width plus both corners; below that it is
    // dropped rather than overwriting the corner.
    let row = draw_list_bottom_row(9, 6, true);
    assert!(!row.contains('3'), "badge should be dropped in {row:?}");
    assert!(row.starts_with('\u{2570}') && row.ends_with('\u{256f}'));
}

/// Renders `render` and returns whether the frame has any non-blank cell.
fn draws_anything(
    width: u16,
    height: u16,
    render: impl FnOnce(&mut Frame),
) -> bool {
    let mut terminal =
        Terminal::new(TestBackend::new(width, height)).expect("backend");
    terminal.draw(render).expect("draw");
    let buffer = terminal.backend().buffer();
    buffer.content.iter().any(|cell| cell.symbol() != " ")
}

#[test]
fn hidden_hints_leave_no_row_behind_not_even_the_top_margin() {
    let groups = [HintGroup {
        label: "File",
        hints: &[("s", "save")],
    }];
    let paint = |frame: &mut Frame| {
        shortcut_hints::render(
            frame,
            frame.area(),
            &groups,
            &HintStyle::default(),
        );
    };

    assert!(draws_anything(40, 3, paint), "hints should draw when shown");

    shortcut_hints::set_visible(false);
    let drew = draws_anything(40, 3, paint);
    shortcut_hints::set_visible(true);
    assert!(!drew, "hidden hints must draw nothing at all");

    // The row budget collapses with them, margin included.
    shortcut_hints::set_visible(false);
    let rows = shortcut_hints::height(&groups, 40, 1);
    shortcut_hints::set_visible(true);
    assert_eq!(rows, 0);
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

/// A document exercising every Markdown element the renderer handles.
const MARKDOWN_SAMPLE: &str = "\
# Heading 世界

Body with **bold**, *italic*, `code` and a [link](http://example.com).

- bullet one
- bullet two
  - nested

1. first
2. second

- [x] done
- [ ] open

> a blockquote

> [!NOTE]
> a callout

```rust
let wide = \"世界\";
```

| A | B |
|---|---|
| 1 | 2 |

---
";

#[test]
fn markdown_view_renders_across_sizes() {
    let skin = skin();
    for (width, height) in SIZES {
        for boxed in [false, true] {
            draw(width, height, |frame| {
                let mut view = MarkdownView::new(MARKDOWN_SAMPLE);
                if boxed {
                    view = view.boxed(BoxDecor::new().caption("Doc"));
                }
                view.render(frame, frame.area(), &skin);
            });
        }
    }
}

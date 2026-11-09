use crossterm::event::KeyModifiers;

use super::*;
use crate::theme::Skin;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn ctrl(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::CONTROL)
}

fn sample() -> TreeView {
    TreeView::new(vec![
        TreeItem::node(
            "src",
            vec![TreeItem::leaf("main.rs"), TreeItem::leaf("lib.rs")],
        ),
        TreeItem::leaf("Cargo.toml"),
    ])
}

#[test]
fn starts_collapsed_showing_only_roots() {
    let view = sample();
    assert_eq!(view.flatten().len(), 2);
    assert_eq!(view.selected_label().as_deref(), Some("src"));
}

#[test]
fn expanding_reveals_children() {
    let mut view = sample();
    view.handle_key(key(KeyCode::Right));
    assert_eq!(view.flatten().len(), 4);
}

#[test]
fn collapsing_hides_children_again() {
    let mut view = sample();
    view.handle_key(key(KeyCode::Enter)); // expand src
    assert_eq!(view.flatten().len(), 4);
    view.handle_key(key(KeyCode::Enter)); // collapse src
    assert_eq!(view.flatten().len(), 2);
}

#[test]
fn navigation_stays_in_bounds() {
    let mut view = sample();
    view.handle_key(key(KeyCode::Up)); // wrap to last
    assert_eq!(view.selected_label().as_deref(), Some("Cargo.toml"));
}

#[test]
fn home_and_end_jump_to_the_first_and_last_row() {
    let mut view = sample();
    view.handle_key(key(KeyCode::End));
    assert_eq!(view.selected_label().as_deref(), Some("Cargo.toml"));
    view.handle_key(key(KeyCode::Home));
    assert_eq!(view.selected_label().as_deref(), Some("src"));
    // vim g/G mirror Home/End.
    view.handle_key(key(KeyCode::Char('G')));
    assert_eq!(view.selected_label().as_deref(), Some("Cargo.toml"));
    view.handle_key(key(KeyCode::Char('g')));
    assert_eq!(view.selected_label().as_deref(), Some("src"));
}

#[test]
fn page_down_clamps_at_the_last_row() {
    // The default one-row viewport makes a page one row; PageDown from the
    // top still clamps at the last node rather than wrapping.
    let mut view = sample();
    view.handle_key(key(KeyCode::PageDown));
    assert_eq!(view.selected_label().as_deref(), Some("Cargo.toml"));
    view.handle_key(key(KeyCode::PageDown));
    assert_eq!(view.selected_label().as_deref(), Some("Cargo.toml"));
}

#[test]
fn ctrl_chords_do_not_navigate_or_expand() {
    let mut view = sample();
    // crossterm reports Ctrl+L/Ctrl+H as `Char('l')`/`Char('h')`, which
    // would expand and collapse the node under the cursor.
    view.handle_key(ctrl(KeyCode::Char('l')));
    assert_eq!(view.flatten().len(), 2, "Ctrl+L must not expand");
    view.handle_key(key(KeyCode::Right)); // expand src for real
    view.handle_key(ctrl(KeyCode::Char('h')));
    assert_eq!(view.flatten().len(), 4, "Ctrl+H must not collapse");
    // Ctrl+J/Ctrl+G must not move the cursor either.
    view.handle_key(ctrl(KeyCode::Char('j')));
    view.handle_key(ctrl(KeyCode::Char('G')));
    assert_eq!(view.selected_label().as_deref(), Some("src"));
}

/// A folder holding two identically labelled leaves, so only the ids can
/// tell them apart.
fn sample_with_ids() -> TreeView {
    TreeView::new(vec![
        TreeItem::node(
            "decks",
            vec![
                TreeItem::leaf_with_id("rust", 7),
                TreeItem::leaf_with_id("rust", 9),
            ],
        ),
        TreeItem::leaf_with_id("geography", 3),
    ])
}

#[test]
fn selected_id_survives_expanding_and_collapsing() {
    let mut view = sample_with_ids();
    assert_eq!(view.selected_id(), None); // the folder carries no id
    view.handle_key(key(KeyCode::Right)); // expand decks
    view.handle_key(key(KeyCode::Down));
    assert_eq!(view.selected_id(), Some(7));
    view.handle_key(key(KeyCode::Down));
    assert_eq!(view.selected_id(), Some(9));
    view.handle_key(key(KeyCode::Up));
    view.handle_key(key(KeyCode::Up));
    view.handle_key(key(KeyCode::Left)); // collapse decks
    view.handle_key(key(KeyCode::Down));
    assert_eq!(view.selected_id(), Some(3));
}

#[test]
fn selected_is_leaf_separates_folders_from_leaves() {
    let mut view = sample_with_ids();
    assert!(!view.selected_is_leaf()); // on "decks"
    view.handle_key(key(KeyCode::Down));
    assert!(view.selected_is_leaf()); // on "geography"
    assert!(!TreeView::new(Vec::new()).selected_is_leaf());
}

#[test]
fn a_node_with_an_id_reports_it_and_still_folds() {
    let mut view = TreeView::new(vec![TreeItem::node_with_id(
        "deck",
        4,
        vec![TreeItem::leaf_with_id("section", 5)],
    )]);
    assert_eq!(view.selected_id(), Some(4));
    assert!(!view.selected_is_leaf());
    view.expand_selected();
    view.step(1);
    assert_eq!(view.selected_id(), Some(5));
}

#[test]
fn a_dimmed_row_is_drawn_with_the_dim_attribute() {
    use ratatui::{Terminal, backend::TestBackend, style::Modifier};

    let view = TreeView::new(vec![
        TreeItem::leaf("plain"),
        TreeItem::leaf("faded").dimmed(),
    ]);
    let mut terminal =
        Terminal::new(TestBackend::new(20, 4)).expect("terminal");
    let base = crate::theme::ThemeRegistry::builtin().resolve("default");
    let skin = Skin::new(
        crate::theme::Palette::resolve(
            base,
            &crate::theme::ColorOverrides::default(),
        ),
        crate::theme::Glyphs::new(crate::theme::GlyphVariant::Unicode),
    );
    terminal
        .draw(|frame| view.render(frame, frame.area(), &skin))
        .expect("draw");
    let buffer = terminal.backend().buffer();
    let row_has_dim = |y: u16| {
        (0..buffer.area.width).any(|x| {
            buffer[(x, y)].modifier.contains(Modifier::DIM)
                && buffer[(x, y)].symbol() != " "
        })
    };
    assert!(row_has_dim(1), "the dimmed row carries DIM");
}

#[test]
fn the_fold_state_and_cursor_restore_by_id() {
    let build = || {
        TreeView::new(vec![TreeItem::node_with_id(
            "decks",
            1,
            vec![TreeItem::node_with_id(
                "rust",
                2,
                vec![TreeItem::leaf_with_id("traits", 3)],
            )],
        )])
    };
    let mut view = build();
    assert!(view.select_id(3), "expands the ancestors");
    assert_eq!(view.selected_label().as_deref(), Some("traits"));
    let expanded = view.expanded_ids();
    assert_eq!(expanded, [1, 2]);
    let mut rebuilt = build();
    rebuilt.expand_ids(&expanded);
    assert_eq!(rebuilt.flatten().len(), 3);
    assert!(!rebuilt.select_id(99));
}

/// The default skin with Unicode glyphs.
fn skin() -> Skin {
    let base = crate::theme::ThemeRegistry::builtin().resolve("default");
    Skin::new(
        crate::theme::Palette::resolve(
            base,
            &crate::theme::ColorOverrides::default(),
        ),
        crate::theme::Glyphs::new(crate::theme::GlyphVariant::Unicode),
    )
}

/// The rows `view` draws into a `width` x `height` area, trailing blanks cut.
fn rendered(view: &TreeView, width: u16, height: u16) -> Vec<String> {
    use ratatui::{Terminal, backend::TestBackend};

    let mut terminal =
        Terminal::new(TestBackend::new(width, height)).expect("terminal");
    terminal
        .draw(|frame| view.render(frame, frame.area(), &skin()))
        .expect("draw");
    let buffer = terminal.backend().buffer();
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect()
}

/// A folder with two counted leaves, the folder counted too, opened.
fn counted_tree() -> TreeView {
    let cells =
        |due: &str, total: &str| vec![TreeCell::new(due), TreeCell::new(total)];
    let mut view = TreeView::new(vec![
        TreeItem::node(
            "Languages",
            vec![
                TreeItem::leaf("Rust").with_columns(cells("3", "25")),
                TreeItem::leaf("Go").with_columns(cells("12", "7")),
            ],
        )
        .with_columns(cells("15", "32")),
    ])
    .with_column_headers(vec!["Due".to_string(), "Total".to_string()]);
    view.expand_selected();
    view
}

#[test]
fn cells_line_up_across_depths_under_their_headers() {
    let rows = rendered(&counted_tree(), 40, 6);
    // Every row ends in the same column: the cells are right-aligned.
    let ends: Vec<usize> =
        rows[..4].iter().map(|row| row.chars().count()).collect();
    assert!(ends.iter().all(|end| *end == ends[0]), "{rows:#?}");
    assert!(rows[0].ends_with("Due  Total"), "{rows:#?}");
    assert!(
        rows[2].contains("Rust") && rows[2].ends_with(" 3     25"),
        "{rows:#?}"
    );
    assert!(rows[3].ends_with("12      7"), "{rows:#?}");
}

#[test]
fn the_columns_start_after_the_widest_label() {
    let rows = rendered(&counted_tree(), 60, 6);
    // "    Rust" is not the widest: "▾ Languages" is 11 columns wide, then
    // two columns of gap and the two-wide due column.
    assert_eq!(rows[1].chars().count(), 11 + 2 + 3 + 2 + 5, "{rows:#?}");
}

#[test]
fn a_narrow_row_shortens_the_label_but_keeps_the_cells() {
    let view = TreeView::new(vec![
        TreeItem::leaf("A very long deck name")
            .with_columns(vec![TreeCell::new("42")]),
    ]);
    let rows = rendered(&view, 14, 3);
    assert!(rows[0].contains('\u{2026}'), "{rows:#?}");
    assert!(rows[0].ends_with("42"), "{rows:#?}");
    assert!(rows[0].chars().count() <= 13, "the last column stays free");
}

#[test]
fn a_suffix_gives_way_before_the_label() {
    let view = TreeView::new(vec![
        TreeItem::leaf("Deck").with_suffix("excluded from every session"),
    ]);
    let wide = rendered(&view, 40, 3);
    assert_eq!(wide[0], "  Deck  excluded from every session");
    let narrow = rendered(&view, 14, 3);
    assert!(narrow[0].starts_with("  Deck  "), "{narrow:#?}");
    assert!(narrow[0].ends_with('\u{2026}'), "{narrow:#?}");
}

#[test]
fn a_row_without_cells_uses_the_whole_width() {
    let view = TreeView::new(vec![
        TreeItem::leaf("Go").with_columns(vec![TreeCell::new("1")]),
        TreeItem::leaf("broken.md").with_suffix("invalid front matter"),
    ]);
    let rows = rendered(&view, 40, 4);
    assert_eq!(rows[1], "  broken.md  invalid front matter");
}

#[test]
fn a_tree_without_columns_draws_as_before() {
    let mut view = sample();
    view.expand_selected();
    let rows = rendered(&view, 20, 5);
    assert_eq!(
        rows[..4],
        ["\u{25be} src", "    main.rs", "    lib.rs", "  Cargo.toml"]
    );
}

#[test]
fn a_suffix_on_a_row_with_cells_widens_the_label_column() {
    let view = TreeView::new(vec![
        TreeItem::leaf("Go")
            .with_suffix("2 notes")
            .with_columns(vec![TreeCell::new("1")]),
        TreeItem::leaf("Rust").with_columns(vec![TreeCell::new("3")]),
        TreeItem::leaf("A long broken deck name"),
    ]);
    let rows = rendered(&view, 40, 5);
    assert_eq!(rows[0], "  Go  2 notes  1");
    assert_eq!(rows[1], "  Rust         3");
}

/// The modifiers of every cell of row `y` that `view` draws `width` wide.
fn row_modifiers(
    view: &TreeView,
    width: u16,
    y: u16,
) -> Vec<(String, ratatui::style::Modifier)> {
    use ratatui::{Terminal, backend::TestBackend};

    let mut terminal =
        Terminal::new(TestBackend::new(width, 6)).expect("terminal");
    terminal
        .draw(|frame| view.render(frame, frame.area(), &skin()))
        .expect("draw");
    let buffer = terminal.backend().buffer();
    (0..width)
        .map(|x| {
            let cell = &buffer[(x, y)];
            (cell.symbol().to_string(), cell.modifier)
        })
        .collect()
}

/// A folder whose only child has a long name, folded.
fn folded_long_child() -> TreeView {
    TreeView::new(vec![
        TreeItem::node(
            "Lang",
            vec![
                TreeItem::leaf("A very long section name")
                    .with_columns(vec![TreeCell::new("1")]),
            ],
        )
        .with_columns(vec![TreeCell::new("1")]),
    ])
}

#[test]
fn a_folded_long_name_does_not_widen_the_label_column() {
    let mut view = folded_long_child();
    let folded = rendered(&view, 60, 4);
    assert_eq!(folded[0], "\u{25b8} Lang  1", "{folded:#?}");
    view.expand_selected();
    let open = rendered(&view, 60, 4);
    assert!(
        open[0].chars().count() > folded[0].chars().count(),
        "{open:#?}"
    );
}

#[test]
fn the_label_heading_sits_over_the_labels_and_the_header_is_underlined() {
    use ratatui::style::Modifier;

    let view = TreeView::new(vec![
        TreeItem::leaf("Rust").with_columns(vec![TreeCell::new("3")]),
    ])
    .with_column_headers(vec!["Due".to_string()])
    .with_label_heading("Deck");
    let rows = rendered(&view, 30, 4);
    assert_eq!(rows[0], "  Deck  Due", "{rows:#?}");
    assert_eq!(rows[1], "  Rust    3", "{rows:#?}");
    let header = row_modifiers(&view, 30, 0);
    let table_width = rows[0].chars().count();
    assert!(
        header[..table_width]
            .iter()
            .all(|(_, modifier)| modifier.contains(Modifier::UNDERLINED)),
        "the underline runs across the table: {header:?}"
    );
    assert!(!header[table_width].1.contains(Modifier::UNDERLINED));
}

#[test]
fn a_row_style_runs_to_the_last_cell_and_a_label_style_stays_on_the_label() {
    use ratatui::style::{Modifier, Style};

    let underlined = Style::default().add_modifier(Modifier::UNDERLINED);
    let bold = Style::default().add_modifier(Modifier::BOLD);
    let view = TreeView::new(vec![
        TreeItem::leaf("All")
            .with_row_style(underlined)
            .with_columns(vec![TreeCell::new("12")]),
        TreeItem::leaf("Group")
            .with_label_style(bold)
            .with_columns(vec![TreeCell::new("3")]),
    ]);
    let summary = row_modifiers(&view, 30, 0);
    // "  All    12": every cell up to the last digit is underlined.
    assert!(
        summary[..11]
            .iter()
            .all(|(_, m)| m.contains(Modifier::UNDERLINED))
    );
    let group = row_modifiers(&view, 30, 1);
    let label: Vec<_> = group
        .iter()
        .filter(|(_, m)| m.contains(Modifier::BOLD))
        .collect();
    assert_eq!(label.len(), "Group".len(), "{group:?}");
}

//! A collapsible tree view: a hierarchical, keyboard-navigable list.
//!
//! The caller builds an owned tree of [`TreeItem`]s; [`TreeView`] holds the
//! expand/collapse and cursor state and renders the currently visible nodes.

use std::{cell::Cell, collections::HashSet};

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
};

use super::{chrome, input, list, nav, style};
use crate::theme::Skin;

/// A node in a tree: a label plus zero or more children.
///
/// A leaf or a node may carry a caller-defined `id`, which
/// [`TreeView::selected_id`] hands back for the row under the cursor. Labels
/// are not unique, so an `id` is the only reliable way to map a selection back
/// to the caller's data.
#[derive(Debug, Clone)]
pub struct TreeItem {
    label: String,
    id: Option<usize>,
    is_dimmed: bool,
    children: Vec<TreeItem>,
}

impl TreeItem {
    /// A leaf node with no children and no id.
    pub fn leaf(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            id: None,
            is_dimmed: false,
            children: Vec::new(),
        }
    }

    /// A leaf node carrying `id`, so the caller can map a selection back to
    /// whatever the leaf stands for.
    pub fn leaf_with_id(label: impl Into<String>, id: usize) -> Self {
        Self {
            label: label.into(),
            id: Some(id),
            is_dimmed: false,
            children: Vec::new(),
        }
    }

    /// A node with children.
    pub fn node(label: impl Into<String>, children: Vec<TreeItem>) -> Self {
        Self {
            label: label.into(),
            id: None,
            is_dimmed: false,
            children,
        }
    }

    /// A node with children that also carries `id`, for a row that stands
    /// for something of its own (a deck with sections, a folder that can be
    /// opened as a whole) besides grouping its children.
    pub fn node_with_id(
        label: impl Into<String>,
        id: usize,
        children: Vec<TreeItem>,
    ) -> Self {
        Self {
            label: label.into(),
            id: Some(id),
            is_dimmed: false,
            children,
        }
    }

    /// Draws this row dimmed, for an entry that is shown but inactive (an
    /// excluded or unreadable item).
    #[must_use]
    pub fn dimmed(mut self) -> Self {
        self.is_dimmed = true;
        self
    }

    fn has_children(&self) -> bool {
        !self.children.is_empty()
    }
}

/// One currently visible node, produced by flattening the tree.
struct Flat {
    index: usize,
    depth: usize,
    label: String,
    id: Option<usize>,
    is_dimmed: bool,
    has_children: bool,
    expanded: bool,
}

/// One node of the whole tree, visible or not: its preorder index, id and
/// the preorder indices of its ancestors.
struct NodeRef {
    index: usize,
    id: Option<usize>,
    ancestors: Vec<usize>,
}

/// A tree view over owned [`TreeItem`] roots. Nodes start collapsed.
pub struct TreeView {
    roots: Vec<TreeItem>,
    expanded: HashSet<usize>,
    cursor: usize,
    offset: Cell<usize>,
    viewport: Cell<usize>,
    decor: Option<chrome::BoxDecor>,
}

impl TreeView {
    /// Builds a tree view over `roots`, all collapsed, cursor on the first row.
    pub fn new(roots: Vec<TreeItem>) -> Self {
        Self {
            roots,
            expanded: HashSet::new(),
            cursor: 0,
            offset: Cell::new(0),
            viewport: Cell::new(1),
            decor: None,
        }
    }

    /// Draws the tree inside a rounded box with the given caption/badge (see
    /// [`chrome::BoxDecor`]); the badge defaults to the number of visible rows.
    /// Omit it for a plain tree.
    #[must_use]
    pub fn boxed(mut self, decor: chrome::BoxDecor) -> Self {
        self.decor = Some(decor);
        self
    }

    /// The label of the node under the cursor, if any.
    pub fn selected_label(&self) -> Option<String> {
        self.flatten()
            .get(self.cursor)
            .map(|node| node.label.clone())
    }

    /// The id of the node under the cursor, or `None` when the tree is empty or
    /// the node was built without one (see [`TreeItem::leaf_with_id`] and
    /// [`TreeItem::node_with_id`]).
    pub fn selected_id(&self) -> Option<usize> {
        self.flatten().get(self.cursor).and_then(|node| node.id)
    }

    /// Whether the node under the cursor is a leaf. An empty tree has no
    /// cursor node and so reports `false`.
    pub fn selected_is_leaf(&self) -> bool {
        self.flatten()
            .get(self.cursor)
            .is_some_and(|node| !node.has_children)
    }

    /// Handles a key: move the cursor (`Up`/`Down`/`k`/`j` cyclically,
    /// `PageUp`/`PageDown` by a page, `Home`/`End`/`g`/`G` to the ends),
    /// expand/collapse (`Left`/`Right`/`h`/`l`) or toggle (`Enter`/`Space`) the
    /// current node.
    ///
    /// These are bare keys only: a Ctrl chord is ignored, so a caller stays
    /// free to bind `Ctrl+<key>` itself. A host whose keys are remappable
    /// resolves its own keymap and calls the movement methods ([`step`],
    /// [`page`], [`to_first`], [`to_last`], [`expand_selected`],
    /// [`collapse_selected`], [`toggle_selected`]) instead.
    ///
    /// [`step`]: Self::step
    /// [`page`]: Self::page
    /// [`to_first`]: Self::to_first
    /// [`to_last`]: Self::to_last
    /// [`expand_selected`]: Self::expand_selected
    /// [`collapse_selected`]: Self::collapse_selected
    /// [`toggle_selected`]: Self::toggle_selected
    pub fn handle_key(&mut self, key: KeyEvent) {
        // The tree binds no chord of its own, and crossterm reports Ctrl+L and
        // Ctrl+H as `Char('l')`/`Char('h')` - without this they would expand
        // and collapse the node, and Ctrl+J would move the cursor.
        if input::is_command(key) {
            return;
        }
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.step(-1),
            KeyCode::Down | KeyCode::Char('j') => self.step(1),
            KeyCode::PageUp => self.page(-1),
            KeyCode::PageDown => self.page(1),
            KeyCode::Home | KeyCode::Char('g') => self.to_first(),
            KeyCode::End | KeyCode::Char('G') => self.to_last(),
            KeyCode::Right | KeyCode::Char('l') => self.expand_selected(),
            KeyCode::Left | KeyCode::Char('h') => self.collapse_selected(),
            KeyCode::Enter | KeyCode::Char(' ') => self.toggle_selected(),
            _ => {}
        }
    }

    /// Moves the cursor `delta` rows, wrapping at both ends.
    pub fn step(&mut self, delta: isize) {
        let len = self.flatten().len();
        self.cursor = nav::cycle(self.cursor, len, delta);
    }

    /// Moves the cursor by `pages` viewports, clamped at the ends.
    pub fn page(&mut self, pages: isize) {
        let len = self.flatten().len();
        let page = self.viewport.get().max(1) as isize;
        self.cursor = nav::step_clamped(self.cursor, len, pages * page);
    }

    /// Moves the cursor to the first row.
    pub fn to_first(&mut self) {
        self.cursor = 0;
    }

    /// Moves the cursor to the last row.
    pub fn to_last(&mut self) {
        self.cursor = self.flatten().len().saturating_sub(1);
    }

    /// The ids of the expanded nodes that carry one, so a host can restore
    /// the fold state after rebuilding the tree (see [`expand_ids`]).
    ///
    /// [`expand_ids`]: Self::expand_ids
    pub fn expanded_ids(&self) -> Vec<usize> {
        self.all_nodes()
            .into_iter()
            .filter(|node| self.expanded.contains(&node.index))
            .filter_map(|node| node.id)
            .collect()
    }

    /// Expands every node whose id is in `ids`; unknown ids are ignored.
    pub fn expand_ids(&mut self, ids: &[usize]) {
        for node in self.all_nodes() {
            if node.id.is_some_and(|id| ids.contains(&id)) {
                self.expanded.insert(node.index);
            }
        }
    }

    /// Moves the cursor onto the node with `id`, expanding its ancestors so
    /// it is visible. Returns whether such a node exists.
    pub fn select_id(&mut self, id: usize) -> bool {
        let nodes = self.all_nodes();
        let Some(target) = nodes.iter().find(|node| node.id == Some(id)) else {
            return false;
        };
        self.expanded.extend(target.ancestors.iter().copied());
        let index = target.index;
        if let Some(row) =
            self.flatten().iter().position(|node| node.index == index)
        {
            self.cursor = row;
        }
        true
    }

    /// Opens the node under the cursor; a leaf does nothing.
    pub fn expand_selected(&mut self) {
        let flat = self.flatten();
        self.set_expanded(&flat, true);
    }

    /// Closes the node under the cursor; a leaf does nothing.
    pub fn collapse_selected(&mut self) {
        let flat = self.flatten();
        self.set_expanded(&flat, false);
        self.clamp_cursor();
    }

    /// Opens a closed node under the cursor and closes an open one.
    pub fn toggle_selected(&mut self) {
        let flat = self.flatten();
        self.toggle(&flat);
        self.clamp_cursor();
    }

    /// Opens (`expand`) or closes the node under the cursor. Leaves do nothing.
    fn set_expanded(&mut self, flat: &[Flat], expand: bool) {
        let Some(node) = flat.get(self.cursor) else {
            return;
        };
        if !node.has_children {
            return;
        }
        if expand {
            self.expanded.insert(node.index);
        } else {
            self.expanded.remove(&node.index);
        }
    }

    fn toggle(&mut self, flat: &[Flat]) {
        if let Some(node) = flat.get(self.cursor)
            && node.has_children
        {
            if node.expanded {
                self.expanded.remove(&node.index);
            } else {
                self.expanded.insert(node.index);
            }
        }
    }

    fn clamp_cursor(&mut self) {
        let len = self.flatten().len();
        if self.cursor >= len {
            self.cursor = len.saturating_sub(1);
        }
    }

    /// The currently visible nodes, in display order, with a stable per-node
    /// index (preorder over the whole tree) used as the expansion key.
    fn flatten(&self) -> Vec<Flat> {
        let mut out = Vec::new();
        let mut counter = 0usize;
        for item in &self.roots {
            self.walk(item, 0, true, &mut counter, &mut out);
        }
        out
    }

    /// Every node with its preorder index, id and ancestors, visible or not.
    fn all_nodes(&self) -> Vec<NodeRef> {
        fn visit(
            item: &TreeItem,
            ancestors: &mut Vec<usize>,
            counter: &mut usize,
            out: &mut Vec<NodeRef>,
        ) {
            let index = *counter;
            *counter += 1;
            out.push(NodeRef {
                index,
                id: item.id,
                ancestors: ancestors.clone(),
            });
            ancestors.push(index);
            for child in &item.children {
                visit(child, ancestors, counter, out);
            }
            ancestors.pop();
        }
        let mut out = Vec::new();
        let mut counter = 0usize;
        for item in &self.roots {
            visit(item, &mut Vec::new(), &mut counter, &mut out);
        }
        out
    }

    fn walk(
        &self,
        item: &TreeItem,
        depth: usize,
        visible: bool,
        counter: &mut usize,
        out: &mut Vec<Flat>,
    ) {
        let index = *counter;
        *counter += 1;
        let expanded = self.expanded.contains(&index);
        if visible {
            out.push(Flat {
                index,
                depth,
                label: item.label.clone(),
                id: item.id,
                is_dimmed: item.is_dimmed,
                has_children: item.has_children(),
                expanded,
            });
        }
        for child in &item.children {
            self.walk(child, depth + 1, visible && expanded, counter, out);
        }
    }

    /// Renders the visible nodes with indentation and expand markers, the cursor
    /// row highlighted and a scrollbar on overflow.
    pub fn render(&self, frame: &mut Frame, area: Rect, skin: &Skin) {
        let lines: Vec<Line<'static>> = self
            .flatten()
            .iter()
            .map(|node| {
                let marker = if node.has_children {
                    skin.glyphs.fold(node.expanded)
                } else {
                    " "
                };
                let indent = "  ".repeat(node.depth);
                let text = format!("{indent}{marker} {}", node.label);
                // The dim rides on the span, so the cursor tint still lays
                // over the row as a whole.
                if node.is_dimmed {
                    Line::from(Span::styled(text, style::dim()))
                } else {
                    Line::from(text)
                }
            })
            .collect();
        let view = list::ListView {
            rows: lines,
            selected: self.cursor,
            offset: &self.offset,
        };
        let viewport = match &self.decor {
            Some(decor) => {
                list::render_boxed(frame, area, skin, view, decor, true)
            }
            // Without a box there is no border to hang the badge on, so the
            // bottom row carries it.
            None => list::render_counted(frame, area, skin, view),
        };
        self.viewport.set(viewport);
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::KeyModifiers;

    use super::*;

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
}

//! A collapsible tree view: a hierarchical, keyboard-navigable list.
//!
//! The caller builds an owned tree of [`TreeItem`]s; [`TreeView`] holds the
//! expand/collapse and cursor state and renders the currently visible nodes.

use std::{cell::Cell, collections::HashSet};

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{Frame, layout::Rect, text::Line};

use super::{chrome, list, nav};
use crate::theme::{GlyphVariant, Skin};

/// A node in a tree: a label plus zero or more children.
///
/// A leaf may carry a caller-defined `id`, which [`TreeView::selected_id`]
/// hands back for the node under the cursor. Labels are not unique, so an `id`
/// is the only reliable way to map a selection back to the caller's data.
#[derive(Debug, Clone)]
pub struct TreeItem {
    label: String,
    id: Option<usize>,
    children: Vec<TreeItem>,
}

impl TreeItem {
    /// A leaf node with no children and no id.
    pub fn leaf(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            id: None,
            children: Vec::new(),
        }
    }

    /// A leaf node carrying `id`, so the caller can map a selection back to
    /// whatever the leaf stands for.
    pub fn leaf_with_id(label: impl Into<String>, id: usize) -> Self {
        Self {
            label: label.into(),
            id: Some(id),
            children: Vec::new(),
        }
    }

    /// A node with children.
    pub fn node(label: impl Into<String>, children: Vec<TreeItem>) -> Self {
        Self {
            label: label.into(),
            id: None,
            children,
        }
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
    has_children: bool,
    expanded: bool,
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
    /// the node was built without one (see [`TreeItem::leaf_with_id`]).
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
    pub fn handle_key(&mut self, key: KeyEvent) {
        let flat = self.flatten();
        if flat.is_empty() {
            return;
        }
        let page = self.viewport.get().max(1) as isize;
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.cursor = nav::cycle(self.cursor, flat.len(), -1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.cursor = nav::cycle(self.cursor, flat.len(), 1);
            }
            KeyCode::PageUp => {
                self.cursor = nav::step_clamped(self.cursor, flat.len(), -page);
            }
            KeyCode::PageDown => {
                self.cursor = nav::step_clamped(self.cursor, flat.len(), page);
            }
            KeyCode::Home | KeyCode::Char('g') => self.cursor = 0,
            KeyCode::End | KeyCode::Char('G') => {
                self.cursor = flat.len().saturating_sub(1);
            }
            KeyCode::Right | KeyCode::Char('l') => {
                self.set_expanded(&flat, true);
            }
            KeyCode::Left | KeyCode::Char('h') => {
                self.set_expanded(&flat, false);
            }
            KeyCode::Enter | KeyCode::Char(' ') => self.toggle(&flat),
            _ => {}
        }
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
        let ascii = matches!(skin.glyphs.variant, GlyphVariant::Ascii);
        let lines: Vec<Line<'static>> = self
            .flatten()
            .iter()
            .map(|node| {
                let marker = marker(node.has_children, node.expanded, ascii);
                let indent = "  ".repeat(node.depth);
                Line::from(format!("{indent}{marker} {}", node.label))
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

/// The expand/collapse marker for a node.
fn marker(has_children: bool, expanded: bool, ascii: bool) -> &'static str {
    match (has_children, expanded, ascii) {
        (false, _, _) => " ",
        (true, true, false) => "\u{25be}", // ▾
        (true, false, false) => "\u{25b8}", // ▸
        (true, true, true) => "-",
        (true, false, true) => "+",
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::KeyModifiers;

    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
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
}

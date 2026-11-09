//! A collapsible tree view: a hierarchical, keyboard-navigable list.
//!
//! The caller builds an owned tree of [`TreeItem`]s; [`TreeView`] holds the
//! expand/collapse and cursor state and renders the currently visible nodes.
//! A row may carry a dimmed suffix and right-aligned [`TreeCell`]s, which the
//! view lines up into columns across the whole tree, under an optional
//! header row (see [`TreeView::with_column_headers`]).

mod render;
#[cfg(test)]
mod tests;

use std::{cell::Cell, collections::HashSet};

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::style::Style;

use crate::{chrome, input, nav};

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
    suffix: Option<String>,
    columns: Vec<TreeCell>,
    children: Vec<TreeItem>,
}

/// One right-aligned cell of a tree row (see [`TreeItem::with_columns`]).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TreeCell {
    text: String,
    style: Style,
}

impl TreeCell {
    /// A cell showing `text` in the row's own style.
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            style: Style::default(),
        }
    }

    /// A cell showing `text` in `style`, for a value whose look carries
    /// meaning (a highlighted count, a dimmed zero).
    pub fn styled(text: impl Into<String>, style: Style) -> Self {
        Self {
            text: text.into(),
            style,
        }
    }
}

impl TreeItem {
    /// A leaf node with no children and no id.
    pub fn leaf(label: impl Into<String>) -> Self {
        Self::build(label.into(), None, Vec::new())
    }

    /// A leaf node carrying `id`, so the caller can map a selection back to
    /// whatever the leaf stands for.
    pub fn leaf_with_id(label: impl Into<String>, id: usize) -> Self {
        Self::build(label.into(), Some(id), Vec::new())
    }

    /// A node with children.
    pub fn node(label: impl Into<String>, children: Vec<TreeItem>) -> Self {
        Self::build(label.into(), None, children)
    }

    /// A node with children that also carries `id`, for a row that stands
    /// for something of its own (a deck with sections, a folder that can be
    /// opened as a whole) besides grouping its children.
    pub fn node_with_id(
        label: impl Into<String>,
        id: usize,
        children: Vec<TreeItem>,
    ) -> Self {
        Self::build(label.into(), Some(id), children)
    }

    /// Draws this row dimmed, for an entry that is shown but inactive (an
    /// excluded or unreadable item).
    #[must_use]
    pub fn dimmed(mut self) -> Self {
        self.is_dimmed = true;
        self
    }

    /// Adds `suffix` after the label, dimmed: a note on the entry (that it
    /// is excluded, why it failed) that gives way first when the row is too
    /// narrow, so the label stays readable.
    #[must_use]
    pub fn with_suffix(mut self, suffix: impl Into<String>) -> Self {
        self.suffix = Some(suffix.into());
        self
    }

    /// Adds right-aligned `cells` after the label. The view lines each
    /// column up across all rows, the columns starting after the widest
    /// label and suffix among the rows with cells; a row with fewer cells
    /// leaves the rest blank, and a row without any may use the whole width
    /// for its label and suffix.
    #[must_use]
    pub fn with_columns(mut self, cells: Vec<TreeCell>) -> Self {
        self.columns = cells;
        self
    }

    fn build(
        label: String,
        id: Option<usize>,
        children: Vec<TreeItem>,
    ) -> Self {
        Self {
            label,
            id,
            is_dimmed: false,
            suffix: None,
            columns: Vec::new(),
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
    is_dimmed: bool,
    suffix: Option<String>,
    columns: Vec<TreeCell>,
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
    /// The column headings drawn above the rows; empty draws no header.
    headers: Vec<String>,
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
            headers: Vec::new(),
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

    /// Draws `headers` in a row above the tree, each right-aligned over its
    /// column of [`TreeCell`]s (see [`TreeItem::with_columns`]). The row stays
    /// in place while the tree scrolls.
    #[must_use]
    pub fn with_column_headers(mut self, headers: Vec<String>) -> Self {
        self.headers = headers;
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
                suffix: item.suffix.clone(),
                columns: item.columns.clone(),
                has_children: item.has_children(),
                expanded,
            });
        }
        for child in &item.children {
            self.walk(child, depth + 1, visible && expanded, counter, out);
        }
    }
}

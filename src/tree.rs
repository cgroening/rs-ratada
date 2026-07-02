//! A collapsible tree view: a hierarchical, keyboard-navigable list.
//!
//! The caller builds an owned tree of [`TreeItem`]s; [`TreeView`] holds the
//! expand/collapse and cursor state and renders the currently visible nodes.

use std::{cell::Cell, collections::HashSet};

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{Frame, layout::Rect, text::Line};

use super::{list, nav};
use crate::theme::{GlyphVariant, Skin};

/// A node in a tree: a label plus zero or more children.
#[derive(Debug, Clone)]
pub struct TreeItem {
    label: String,
    children: Vec<TreeItem>,
}

impl TreeItem {
    /// A leaf node with no children.
    pub fn leaf(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            children: Vec::new(),
        }
    }

    /// A node with children.
    pub fn node(label: impl Into<String>, children: Vec<TreeItem>) -> Self {
        Self {
            label: label.into(),
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
    has_children: bool,
    expanded: bool,
}

/// A tree view over owned [`TreeItem`] roots. Nodes start collapsed.
pub struct TreeView {
    roots: Vec<TreeItem>,
    expanded: HashSet<usize>,
    cursor: usize,
    offset: Cell<usize>,
}

impl TreeView {
    /// Builds a tree view over `roots`, all collapsed, cursor on the first row.
    pub fn new(roots: Vec<TreeItem>) -> Self {
        Self {
            roots,
            expanded: HashSet::new(),
            cursor: 0,
            offset: Cell::new(0),
        }
    }

    /// The label of the node under the cursor, if any.
    pub fn selected_label(&self) -> Option<String> {
        self.flatten()
            .get(self.cursor)
            .map(|node| node.label.clone())
    }

    /// Handles a key: navigate, expand/collapse or toggle the current node.
    pub fn handle_key(&mut self, key: KeyEvent) {
        let flat = self.flatten();
        if flat.is_empty() {
            return;
        }
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.cursor = nav::cycle(self.cursor, flat.len(), -1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.cursor = nav::cycle(self.cursor, flat.len(), 1);
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
        list::render(frame, area, skin, lines, self.cursor, &self.offset);
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
}

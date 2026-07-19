//! Fuzzy command palette overlay: pick a command and run it.
//!
//! Like the [`help`](super::help) overlay it groups commands into titled
//! sections and filters them fuzzily, but it returns the chosen command so the
//! caller can execute it. With an empty query the commands stay grouped under
//! their section headers; as soon as the user types, the list flattens and
//! re-sorts by match score so the best hit is first. Commands whose `enabled`
//! flag is `false` render dimmed and cannot be selected.

mod layout;
mod render;

use layout::{layout_rows, selected_index};
use render::render_body;

use std::io;

use crossterm::event::KeyCode;
use ratatui::Frame;

use super::{
    chrome,
    filter_list::FilterList,
    layout::{centered_rect, fit},
    modal::ModalSignal,
    overlay::{self, PopupFlow, popup_with_paste},
    terminal::Tui,
};
use crate::theme::Skin;

/// One selectable command in the palette.
pub struct CommandItem<'a> {
    /// The command name shown to the user, e.g. `"add task"`.
    pub label: &'a str,
    /// The section this command is grouped under, e.g. `"Tasks"`.
    pub category: &'a str,
    /// The keys bound to the command, e.g. `"a"` (may be empty).
    pub key_hint: &'a str,
    /// Whether the command can run now; a disabled command renders dimmed and
    /// is not selectable.
    pub enabled: bool,
}

/// One rendered row: a section header or a command.
enum Row<'a> {
    Header(&'a str),
    Item {
        item: &'a CommandItem<'a>,
        /// The command's original index into the caller's `items`.
        index: usize,
    },
}

/// Shows the command palette until the user runs a command or cancels.
///
/// `Enter` runs the highlighted command and returns its index into `items`;
/// `Esc` cancels. Typing filters the commands fuzzily and re-sorts them by
/// score; with an empty query they stay grouped and `Tab`/`BackTab` jump
/// between sections. An empty `items` cancels immediately.
pub fn command_palette(
    tui: &mut Tui,
    skin: &Skin,
    title: &str,
    items: &[CommandItem<'_>],
    render_bg: impl Fn(&mut Frame),
) -> io::Result<ModalSignal<usize>> {
    if items.is_empty() {
        return Ok(ModalSignal::Cancelled);
    }
    let mut state = FilterList::new();
    popup_with_paste(
        tui,
        &mut state,
        |area, _| {
            centered_rect(
                fit(area.width * 2 / 3, 40, area.width),
                fit(area.height * 2 / 3, 8, area.height),
                area,
            )
        },
        |frame, _| render_bg(frame),
        |frame, rect, state: &FilterList| {
            let inner = overlay::framed(frame, rect, skin, title);
            render_body(frame, inner, skin, items, state);
            // Section headers are rows but not positions: the badge counts the
            // selectable commands.
            let count = layout_rows(items, &state.query).selectable.len();
            let cursor = state.cursor.min(count.saturating_sub(1));
            let badge = chrome::position_badge(cursor, count);
            chrome::render_badge(frame, rect, skin, &badge);
        },
        |state, key| match key.code {
            KeyCode::Esc => PopupFlow::Cancelled,
            KeyCode::Enter => {
                let layout = layout_rows(items, &state.query);
                match selected_index(&layout, state.cursor) {
                    Some(index) => PopupFlow::Done(index),
                    None => PopupFlow::Continue,
                }
            }
            // Everything else is list navigation or filter text, shared with
            // the other filtered overlays.
            _ => {
                let layout = layout_rows(items, &state.query);
                state.handle_key(
                    key,
                    layout.selectable.len(),
                    &layout.section_starts,
                );
                PopupFlow::Continue
            }
        },
        |state, text| {
            state
                .query
                .extend(text.chars().filter(|ch| !ch.is_control()));
            state.cursor = 0;
            PopupFlow::Continue
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items() -> Vec<CommandItem<'static>> {
        vec![
            CommandItem {
                label: "add task",
                category: "Tasks",
                key_hint: "a",
                enabled: true,
            },
            CommandItem {
                label: "delete",
                category: "Tasks",
                key_hint: "d",
                enabled: false,
            },
            CommandItem {
                label: "view summary",
                category: "Views",
                key_hint: "2",
                enabled: true,
            },
        ]
    }

    #[test]
    fn empty_query_groups_and_lists_only_enabled_as_selectable() {
        let items = items();
        let layout = layout_rows(&items, "");
        // Header(Tasks), add, delete, Header(Views), view summary = 5 rows.
        assert_eq!(layout.rows.len(), 5);
        // Only the two enabled items are selectable (delete is disabled).
        assert_eq!(layout.selectable, vec![1, 4]);
        // One section start per section that has a selectable item.
        assert_eq!(layout.section_starts, vec![0, 1]);
    }

    #[test]
    fn disabled_item_is_rendered_but_not_selectable() {
        let items = items();
        let layout = layout_rows(&items, "");
        // The disabled "delete" sits at row 2 but never in `selectable`.
        assert!(matches!(layout.rows[2], Row::Item { index: 1, .. }));
        assert!(!layout.selectable.contains(&2));
    }

    #[test]
    fn query_flattens_ranks_and_drops_headers() {
        let items = items();
        let layout = layout_rows(&items, "task");
        // No headers in the flat list, no sections to jump between.
        assert!(layout.rows.iter().all(|r| matches!(r, Row::Item { .. })));
        assert!(layout.section_starts.is_empty());
        // "add task" is the only enabled match, so it is the sole selection.
        assert_eq!(layout.selectable.len(), 1);
        assert_eq!(selected_index(&layout, 0), Some(0));
    }

    #[test]
    fn non_matching_query_leaves_nothing_selectable() {
        let items = items();
        let layout = layout_rows(&items, "zzzzz");
        assert!(layout.selectable.is_empty());
        assert_eq!(selected_index(&layout, 0), None);
    }
}

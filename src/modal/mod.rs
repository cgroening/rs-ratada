//! Reusable modal widgets. Each is a thin wrapper over [`crate::overlay::popup`]: it
//! sets up its state and closures and returns a [`ModalSignal`]. The dimmed
//! backdrop, box centering and event loop live in [`crate::overlay`], not here.
//!
//! Yes/no questions go through [`confirm`], which lets `Enter` mean yes. A
//! destructive action goes through [`confirm_default`] with
//! [`Question::declining`] instead, so a stray `Enter` cannot confirm the
//! deletion. [`confirm_styled`] asks the same question with a frame title and
//! a body the caller styles line by line, for a dialog that shows what is
//! about to be affected rather than one plain sentence; its body scrolls when
//! it does not fit, and [`confirm_styled_views`] adds a second view of the
//! body that `←`/`→` switch to, plus an optional [`ToggleView`] that its own
//! key shows and hides. A choice between a few answers shown side by side
//! as buttons goes through [`choose`].
//!
//! Every modal takes a [`crate::theme::Skin`], whose palette drives the colors.

mod choice;
mod confirm;
mod message;
mod multi_select_with;
mod multi_state;
mod number;
mod picker;
pub(crate) mod render;
mod save_changes;
mod select_with;
mod styled;
mod text_input;

pub use choice::{Choice, choose};
pub use confirm::{Question, confirm, confirm_default};
pub use message::message;
pub use multi_select_with::{
    MultiChoice, MultiSelectOptions, multi_select_with,
};
pub use number::{number_input, number_input_bounded};
pub use picker::{
    ListAction, multi_select, multi_select_styled, select, select_reorderable,
    select_styled,
};
pub use render::{MAX_HINT_ROWS, hint_block, hint_block_height, hints_width};
pub use save_changes::{SaveChoice, ask_save};
pub use select_with::{SelectItem, SelectOptions, select_with};
pub use styled::{
    StyledQuestion, StyledViews, ToggleView, confirm_styled,
    confirm_styled_views,
};
pub use text_input::{input, input_wide};

/// Outcome of a modal interaction.
pub enum ModalSignal<T> {
    /// The user confirmed with a value.
    Value(T),
    /// The user dismissed the modal (Esc).
    Cancelled,
    /// The global quit chord was pressed inside the modal.
    Quit,
}

#[cfg(test)]
mod tests;

//! `ratada`: a reusable ratatui widget toolkit.
//!
//! The toolkit owns the generic terminal, navigation, rendering and modal
//! building blocks over `ratatui`/`crossterm` (plus `unicode-width`,
//! `nucleo-matcher`, `chrono`, `log`) and never depends on any application
//! types. The [`theme`] layer supplies the framework-agnostic styling
//! vocabulary (a [`theme::Palette`], [`theme::Glyphs`] and [`theme::Mode`],
//! bundled into a [`theme::Skin`]); the host supplies lifecycle hooks (see
//! [`terminal::Tui::with_hooks`]). Theme colors are mapped to ratatui styles in
//! [`style`].
//!
//! # Example
//!
//! Implement [`Screen`] and hand it to [`run`], which owns the draw/input loop
//! inside a raw-mode [`Tui`] guard:
//!
//! ```no_run
//! use ratada::prelude::*;
//! use ratatui::{Frame, text::Line};
//! use crossterm::event::{KeyCode, KeyEvent};
//!
//! struct App {
//!     count: u32,
//! }
//!
//! impl Screen for App {
//!     type Error = std::io::Error;
//!
//!     fn render(&self, frame: &mut Frame) {
//!         frame.render_widget(Line::from(format!("count: {}", self.count)), frame.area());
//!     }
//!
//!     fn handle_key(&mut self, key: KeyEvent, _tui: &mut Tui) -> std::io::Result<Flow> {
//!         match key.code {
//!             KeyCode::Char('q') => Ok(Flow::Quit),
//!             KeyCode::Char(' ') => {
//!                 self.count += 1;
//!                 Ok(Flow::Continue)
//!             }
//!             _ => Ok(Flow::Continue),
//!         }
//!     }
//! }
//!
//! let mut tui = Tui::new()?;
//! run(&mut tui, &mut App { count: 0 })?;
//! # Ok::<(), std::io::Error>(())
//! ```
#![warn(clippy::pedantic)]
// Terminal geometry mixes u16 (ratatui areas) and usize (indices/lengths); the
// conversions are bounded by the screen size, so these pedantic cast lints are
// allowed crate-wide rather than scattered per call.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap
)]
// `#[must_use]` on every constructor/getter and a `# Errors` paragraph on every
// I/O wrapper add noise without catching real bugs; the meaningful public APIs
// already document their errors. Allowed crate-wide rather than per item.
#![allow(clippy::must_use_candidate, clippy::missing_errors_doc)]

pub mod theme;

pub mod autocomplete;
pub mod chrome;
pub mod clipboard;
pub mod color_picker;
pub mod date_picker;
pub mod date_range_picker;
pub mod double_press;
pub mod driver;
pub mod editor;
pub mod finder;
pub mod footer;
pub mod form;
pub mod fuzzy;
pub mod gauge;
pub mod header;
pub mod help;
pub mod input;
pub mod layout;
pub mod list;
pub mod modal;
pub mod month_picker;
pub mod nav;
pub mod overlay;
pub mod pager;
pub mod palette;
pub mod path_picker;
pub mod scroll;
pub mod slider;
pub mod spinner;
pub mod statusbar;
pub mod style;
pub mod table;
pub mod tabs;
pub mod terminal;
pub mod text;
pub mod textarea;
pub mod toast;
pub mod tree;

pub use driver::{Flow, Screen, run};
pub use modal::ModalSignal;
pub use overlay::{PopupFlow, popup};
pub use terminal::{Tui, TuiEvent};

/// The common imports for building a TUI on `ratada`: the terminal guard, the
/// event-loop driver and the shared box decoration. Glob-import it with
/// `use ratada::prelude::*;`.
pub mod prelude {
    pub use crate::chrome::BoxDecor;
    pub use crate::{
        Flow, ModalSignal, PopupFlow, Screen, Tui, TuiEvent, popup, run,
    };
}

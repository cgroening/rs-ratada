//! The opt-in confirmation asked before an app quits.
//!
//! Two quits reach a user: the toolkit's hard `Ctrl+Q`, honoured everywhere
//! including modals, and the host's own quit action (conventionally `q`/`Esc`).
//! [`set_confirm`] decides *whether* either is questioned; a guard registered
//! with [`set_guard`] decides *how* the question is drawn. Nothing is asked by
//! default, which is exactly the behaviour of a toolkit without this module.
//!
//! `driver::run` and `overlay::popup` ask for [`QuitKind::Hard`] themselves, so
//! every screen and every modal inherits it. [`QuitKind::Soft`] is the host's:
//! it calls [`request`] in its own quit action, *before* it records the intent.
//! Only the host knows where its quit came from, and a refusal has to be
//! possible before that intent is stored.

use std::{cell::Cell, cell::RefCell, io, rc::Rc};

use ratatui::Frame;

use super::{modal::ModalSignal, terminal::Tui};

/// The signature of a registered quit guard.
type Guard = dyn Fn(
    &mut Tui,
    QuitKind,
    &dyn Fn(&mut Frame),
) -> io::Result<ModalSignal<bool>>;

/// Which quit a confirmation applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuitKind {
    /// The host's own quit action (conventionally `q`/`Esc`), which the host
    /// raises and questions itself.
    Soft,
    /// The toolkit's `Ctrl+Q`, honoured everywhere including modals.
    Hard,
}

/// When to ask before quitting. Never, by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum QuitConfirm {
    /// Quit straight away, never asking. The default.
    #[default]
    Never,
    /// Ask only before the host's own quit action.
    Soft,
    /// Ask only before the hard `Ctrl+Q`.
    Hard,
    /// Ask before either.
    Both,
}

impl QuitConfirm {
    /// Whether this policy questions `kind`.
    fn asks(self, kind: QuitKind) -> bool {
        match self {
            QuitConfirm::Never => false,
            QuitConfirm::Soft => kind == QuitKind::Soft,
            QuitConfirm::Hard => kind == QuitKind::Hard,
            QuitConfirm::Both => true,
        }
    }
}

thread_local! {
    /// The active policy. Per thread, like the hint visibility: a TUI owns one
    /// terminal and drives it from one thread.
    static CONFIRM: Cell<QuitConfirm> = const { Cell::new(QuitConfirm::Never) };

    /// How a confirmation is drawn, once a host registers it.
    static GUARD: RefCell<Option<Rc<Guard>>> = const { RefCell::new(None) };

    /// Set while the guard runs, so a `Ctrl+Q` inside its dialog does not open
    /// a second one.
    static ASKING: Cell<bool> = const { Cell::new(false) };
}

/// The active quit-confirmation policy.
pub fn confirm_mode() -> QuitConfirm {
    CONFIRM.with(Cell::get)
}

/// Sets when a confirmation is asked before quitting.
///
/// A policy that questions [`QuitKind::Soft`] only takes effect where the host
/// calls [`request`] in its own quit action; the toolkit never sees that key
/// and cannot ask on the host's behalf.
///
/// # Examples
///
/// ```
/// use ratada::quit::{QuitConfirm, confirm_mode, set_confirm};
///
/// set_confirm(QuitConfirm::Soft);
/// assert_eq!(confirm_mode(), QuitConfirm::Soft);
/// ```
pub fn set_confirm(mode: QuitConfirm) {
    CONFIRM.with(|policy| policy.set(mode));
}

/// Registers how a quit confirmation is drawn. Call it before `run`.
///
/// The guard receives the terminal, which quit is pending, and a painter that
/// repaints the surface the dialog should sit on. It hands back the modal's own
/// outcome; [`request`] reads it, so a host cannot get the edge cases wrong.
/// Typically it is a single expression:
///
/// ```no_run
/// # use ratada::theme::Skin;
/// # fn demo(skin: Skin) {
/// use ratada::{modal, quit};
///
/// quit::set_guard(move |tui, _kind, bg| modal::confirm(tui, &skin, "Quit?", bg));
/// # }
/// ```
pub fn set_guard(
    guard: impl Fn(
        &mut Tui,
        QuitKind,
        &dyn Fn(&mut Frame),
    ) -> io::Result<ModalSignal<bool>>
    + 'static,
) {
    GUARD.with(|slot| *slot.borrow_mut() = Some(Rc::new(guard)));
}

/// Whether the app may quit.
///
/// `true` unless the policy questions `kind` and a registered guard's dialog
/// declines. `render_bg` repaints the surface behind that dialog.
///
/// Returns `true` without asking when the policy is silent for `kind`, when no
/// guard is registered, or when a guard is already on screen (a `Ctrl+Q` inside
/// the dialog means the user insists).
pub fn request(
    tui: &mut Tui,
    kind: QuitKind,
    render_bg: &dyn Fn(&mut Frame),
) -> bool {
    if !should_ask(kind) {
        return true;
    }
    let Some(guard) = GUARD.with(|slot| slot.borrow().clone()) else {
        return true;
    };
    let _asking = Asking::enter();
    match guard(tui, kind, render_bg) {
        Ok(ModalSignal::Value(answer)) => answer,
        // `Ctrl+Q` inside the dialog: the user insists.
        Ok(ModalSignal::Quit) => true,
        Ok(ModalSignal::Cancelled) => false,
        // We could not ask, so we must not trap the user in the app.
        Err(error) => {
            log::warn!("quit confirmation failed: {error}; quitting");
            true
        }
    }
}

/// Whether `kind` should be put to the user: the policy questions it, a guard
/// exists to ask with, and no guard is already on screen.
///
/// Split out from [`request`] because `Tui` cannot be built in a test (it takes
/// over the terminal), while this decision is the whole of the logic.
fn should_ask(kind: QuitKind) -> bool {
    if !confirm_mode().asks(kind) {
        return false;
    }
    if ASKING.with(Cell::get) {
        return false;
    }
    if GUARD.with(|slot| slot.borrow().is_none()) {
        log::warn!("quit confirmation requested without a guard; quitting");
        return false;
    }
    true
}

/// Marks the guard as on screen for as long as it lives, so a panic inside the
/// guard cannot leave the flag stuck.
struct Asking;

impl Asking {
    fn enter() -> Self {
        ASKING.with(|flag| flag.set(true));
        Self
    }
}

impl Drop for Asking {
    fn drop(&mut self) {
        ASKING.with(|flag| flag.set(false));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs `body` with `mode` active and a no-op guard registered, restoring
    /// both afterwards. Several tests share a thread, so each one cleans up.
    fn with_policy(mode: QuitConfirm, body: impl FnOnce()) {
        let before = confirm_mode();
        set_confirm(mode);
        body();
        set_confirm(before);
    }

    /// Registers a guard that is never actually called by `should_ask`.
    fn register_guard() {
        set_guard(|_tui, _kind, _bg| Ok(ModalSignal::Value(true)));
    }

    fn clear_guard() {
        GUARD.with(|slot| *slot.borrow_mut() = None);
    }

    #[test]
    fn nothing_is_asked_by_default() {
        assert_eq!(confirm_mode(), QuitConfirm::Never);
        assert!(!should_ask(QuitKind::Soft));
        assert!(!should_ask(QuitKind::Hard));
    }

    #[test]
    fn without_a_guard_nothing_is_asked_even_when_the_policy_wants_to() {
        clear_guard();
        with_policy(QuitConfirm::Both, || {
            assert!(!should_ask(QuitKind::Soft));
            assert!(!should_ask(QuitKind::Hard));
        });
    }

    #[test]
    fn a_policy_asks_only_for_its_own_kind() {
        register_guard();
        with_policy(QuitConfirm::Soft, || {
            assert!(should_ask(QuitKind::Soft));
            assert!(!should_ask(QuitKind::Hard));
        });
        with_policy(QuitConfirm::Hard, || {
            assert!(!should_ask(QuitKind::Soft));
            assert!(should_ask(QuitKind::Hard));
        });
        with_policy(QuitConfirm::Both, || {
            assert!(should_ask(QuitKind::Soft));
            assert!(should_ask(QuitKind::Hard));
        });
        clear_guard();
    }

    #[test]
    fn a_guard_already_on_screen_is_not_asked_again() {
        register_guard();
        with_policy(QuitConfirm::Both, || {
            let _asking = Asking::enter();
            assert!(!should_ask(QuitKind::Hard));
        });
        // The marker restored the flag on drop.
        with_policy(QuitConfirm::Both, || {
            assert!(should_ask(QuitKind::Hard));
        });
        clear_guard();
    }
}

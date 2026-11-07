//! The "Save changes?" question a form asks when it is left with changes.

use std::io;

use ratatui::Frame;

use super::{ModalSignal, SelectItem, SelectOptions, select_with};
use crate::{terminal::Tui, theme::Skin};

/// The answer to "Save changes?".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveChoice {
    /// Save, then leave (`y`).
    Save,
    /// Leave without saving (`n`).
    Discard,
    /// Stay in the form (`b` or `Esc`).
    Back,
}

/// Asks whether to save the changes of a form being left: `y` saves, `n`
/// discards, `b` or `Esc` go back to the form. The global quit chord comes
/// back as [`ModalSignal::Quit`]; a cancel is [`SaveChoice::Back`].
///
/// # Errors
///
/// Returns an I/O error when drawing or reading input fails.
pub fn ask_save(
    tui: &mut Tui,
    skin: &Skin,
    render_bg: impl Fn(&mut Frame),
) -> io::Result<ModalSignal<SaveChoice>> {
    let items = [
        SelectItem::new("Save").hotkey('y'),
        SelectItem::new("Discard").hotkey('n'),
        SelectItem::new("Back to the form").hotkey('b'),
    ];
    let options = SelectOptions::default();
    let signal = select_with(
        tui,
        skin,
        "Save changes?",
        &items,
        0,
        &options,
        render_bg,
    )?;
    Ok(match signal {
        ModalSignal::Quit => ModalSignal::Quit,
        ModalSignal::Value(0) => ModalSignal::Value(SaveChoice::Save),
        ModalSignal::Value(1) => ModalSignal::Value(SaveChoice::Discard),
        ModalSignal::Value(_) | ModalSignal::Cancelled => {
            ModalSignal::Value(SaveChoice::Back)
        }
    })
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::*;
    use crate::terminal::{clear_scripted_input, script_keys};
    use crate::theme::{
        ColorOverrides, GlyphVariant, Glyphs, Palette, ThemeRegistry,
    };

    fn skin() -> Skin {
        let base = ThemeRegistry::builtin().resolve("default");
        Skin::new(
            Palette::resolve(base, &ColorOverrides::default()),
            Glyphs::new(GlyphVariant::Unicode),
        )
    }

    fn answer(code: KeyCode) -> ModalSignal<SaveChoice> {
        script_keys(vec![KeyEvent::new(code, KeyModifiers::NONE)]);
        let mut tui = Tui::for_test(60, 16).expect("a test terminal");
        let signal = ask_save(&mut tui, &skin(), |_| {}).expect("the dialog");
        clear_scripted_input();
        signal
    }

    #[test]
    fn each_key_gives_its_answer() {
        assert!(matches!(
            answer(KeyCode::Char('y')),
            ModalSignal::Value(SaveChoice::Save)
        ));
        assert!(matches!(
            answer(KeyCode::Char('n')),
            ModalSignal::Value(SaveChoice::Discard)
        ));
        assert!(matches!(
            answer(KeyCode::Char('b')),
            ModalSignal::Value(SaveChoice::Back)
        ));
        assert!(matches!(
            answer(KeyCode::Esc),
            ModalSignal::Value(SaveChoice::Back)
        ));
    }
}

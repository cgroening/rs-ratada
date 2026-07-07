# Development

Developer notes for working on `ratada`. For a usage overview see
[`README.md`](README.md); for coding conventions see [`CLAUDE.md`](CLAUDE.md);
for the public API surface see [`API.md`](API.md).

`ratada` is a reusable ratatui widget toolkit. It depends only on external
crates (`ratatui`, `crossterm`, `unicode-width`, `nucleo-matcher`,
`pulldown-cmark`, `chrono`, `log`, `serde`) and never on application types, so
any host can build a TUI on it.

## Project layout

Widget modules sit flat at the crate root; the theming vocabulary is a
submodule. Cross-module references use `super::`.

```text
src/
  lib.rs            crate root: lint config, module decls, prelude, crate example
  theme/            framework-agnostic styling vocabulary (a CLI can share it)
    color.rs        Color + parse_color + OKLCH variants (darken/lighten/...)
    palette.rs      Palette (+ resolve) and ColorOverrides (define_palette! SSOT)
    skin.rs         Skin = Palette + Glyphs
    glyphs.rs       GlyphVariant + Glyphs (Unicode / ASCII)
    theme_set.rs    ThemeColors, ThemeRegistry (built-in themes)

  # driver / infrastructure
  terminal.rs       Tui RAII guard (raw mode + alt screen) + TuiEvent + hooks
  driver.rs         Screen trait + Flow + generic run() loop (idle tick)
  overlay.rs        popup() driver, dimmed scrim, framed() modal helper
  chrome.rs         panels / modal frame + BoxDecor (caption + badge) box seam
  layout.rs         centered_rect / centered_fraction (shared popup sizing)
  nav.rs            cycle / step_clamped / keep_visible + ScrollView
  scroll.rs         overflow-only vertical/horizontal scrollbar (ScrollView)
  style.rs          the single theme::Color -> ratatui adapter
  text.rs           unicode-width truncate

  # input / editing
  input.rs          TextCursor, InputField, shared edit core (apply_edit_key)
  textarea.rs       TextArea (wrapped multi-line editor; reuses input's core)
  autocomplete.rs   inline suggestion dropdown
  editor.rs         launch $EDITOR via Tui::suspend
  clipboard.rs      best-effort copy/paste via platform tools

  # data display
  table/            Table (type-aware sort, fuzzy filter, row/cell select),
                    split into model / interaction / render
  tree.rs           collapsible TreeView
  list.rs           selectable list + scrollbar (ListView; render/render_boxed)
  markdown/         CommonMark renderer (mod/render = engine, theme = StyleSheet
                    default/from_skin, view = MarkdownView + viewer modal)
  sidebar.rs        sectioned menu column (headers + items, optional filter)
  tabs.rs, pager.rs, gauge.rs, spinner.rs, toast.rs

  # pickers / modals
  modal.rs          confirm / select(+styled,reorderable) / multi_select /
                    number_input / message
  form.rs           schema-driven form (text/multiline/bool/choice/date)
  finder.rs         fuzzy picker; fuzzy.rs: nucleo score + highlight
  color_picker.rs, swatches.rs, date_picker.rs, date_range_picker.rs,
  month_picker.rs, path_picker.rs (optional root confinement), slider.rs

  # chrome / misc
  help.rs           sectioned, fuzzy help overlay (Tab jumps sections)
  command_palette.rs  fuzzy command palette overlay
  shortcut_hints.rs   footer key-hint lines (flat or grouped)
  theme_preview.rs    palette/variant preview for a gallery
  header.rs, statusbar.rs, double_press.rs
tests/
  render.rs         headless TestBackend render smoke tests
```

## Conventions (SSOT)

This crate is the single source of truth for the TUI conventions in
CLAUDE.md §7.10. When building a widget, reuse the shared building blocks rather
than reinventing them:

- **Navigation:** `nav::cycle` (wrapping), `nav::keep_visible` (scroll offset).
- **Scrollbar:** `scroll::render_scrollbar` — renders only on overflow and
  skips empty areas; `list::render` already calls it, so list-backed widgets
  (`tree`, `finder`, `help`, `path_picker`) get it for free.
- **Framing:** `chrome::framed_decor` draws the rounded accent box with a
  caption (top border) and badge (bottom-right); every boxable widget goes
  through it and exposes `.boxed(decor)`.
- **Popup sizing:** `layout::centered_fraction` gives every centered popup its
  size (a fraction of the area, floored at a minimum).
- **Colors:** only `style.rs` maps `theme::Color` to ratatui; widgets take a
  `&Skin`/`&Palette`, never a raw literal.
- **Text editing:** `input::TextCursor` + the crate-internal `apply_edit_key`
  are the shared caret/edit core (reused by `textarea`); widths are measured
  with `unicode-width` (wide glyphs count as 2).

## Common commands

```bash
cargo build
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test            # unit tests + doctests + tests/render.rs
```

`ratada` is a library, so there is no binary to run; exercise widgets through
the `clibase` gallery (`cargo run` in `../../templates/clibase`, view `3`) or a
`TestBackend` in `tests/render.rs`.

## Testing

- **Unit tests** live inline (`#[cfg(test)] mod tests`) and cover pure logic
  (navigation, wrap/width, filtering, selection, badge counts).
- **Doctests** on the key public items double as compile-checked examples.
- **`tests/render.rs`** renders the frame-based widgets into a `TestBackend`
  plain/boxed, at a roomy and a cramped size, and with wide characters — a
  panic-free smoke test (this is how the empty-area scrollbar panic was caught).
  Popups (`help`/`finder`/`modal`/`path_picker`) need a live `Tui` loop and are
  covered by their unit tests instead.

## Adding a widget

1. Add `pub mod <name>;` to `lib.rs` (flat at the crate root).
2. Take a `&Skin` (or `&Palette`) for styling; never depend on host types.
3. Reuse `nav`/`scroll`/`chrome`/`style` and the `unicode-width` helpers.
4. If it should support the boxed style, store an `Option<chrome::BoxDecor>` and
   render through `chrome::framed_decor`; add a `.boxed(decor)` builder.
5. Add unit tests for the logic and a case in `tests/render.rs`; a doctest for
   the constructor.
6. Keep `README.md`/`API.md`/`CLAUDE.md` in sync when the public API changes.

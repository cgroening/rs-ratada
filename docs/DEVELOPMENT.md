# Development

Developer notes for working on `ratada`. For a usage overview see [`README.md`](../README.md); for coding conventions see [`CLAUDE.md`](../CLAUDE.md); the authoritative API reference is the rustdoc (`cargo doc --open`, or [docs.rs/ratada](https://docs.rs/ratada)).

`ratada` is a reusable ratatui widget toolkit. It depends only on external crates (`ratatui`, `crossterm`, `unicode-width`, `nucleo-matcher`, `pulldown-cmark`, `chrono`, `log`, `serde`, plus the Windows-only `clipboard-win`) and never on application types, so any host can build a TUI on it.

## Project layout

Widget modules sit flat at the crate root; the theming vocabulary is a submodule. Cross-module references use `super::`.

```text
src/
  lib.rs            crate root: lint config, module decls, prelude, crate example
  theme/            framework-agnostic styling vocabulary (a CLI can share it)
    color/          Color + OKLCH variants, split into mod / space / parse
                    (space = Oklab/Oklch/Hsl math, parse = text -> Color)
    palette.rs      Palette (+ resolve) and ColorOverrides (define_palette! SSOT)
    skin.rs         Skin = Palette + Glyphs
    glyphs.rs       GlyphVariant + Glyphs (Unicode / ASCII)
    theme_set.rs    ThemeColors (+ KEYS), ThemeRegistry (built-in themes)

  # driver / infrastructure
  terminal.rs       Tui RAII guard (raw mode + alt screen) + TuiEvent + hooks
  driver.rs         Screen trait + Flow + generic run() loop (idle tick)
  overlay.rs        popup() driver, dimmed scrim, framed() modal helper
  chrome.rs         panels / modal frame + BoxDecor (caption + badge) box seam
                    and the two badge renderers (border / frameless corner)
  layout.rs         fit / centered_rect / centered_fraction (shared popup sizing)
  keymap/           KeyChord grammar + Keymap<A> over an app's Action enum,
                    split into mod / chord (the notation) / config (overrides)
  nav.rs            cycle / step_clamped / keep_visible + ScrollView
  scroll.rs         overflow-only vertical/horizontal scrollbar (ScrollView)
  style.rs          the single theme::Color -> ratatui adapter
  text.rs           unicode-width truncate

  # input / editing
  input/            TextCursor, InputField and the shared edit core, split into
                    mod / keys (chord classification) / edit (apply_edit_key) /
                    mutate / clip (clipboard) / paint (spans, scroll window)
  textarea/         TextArea (wrapped multi-line editor; reuses input's core),
                    split into mod / wrap (wrapping + caret<->display mapping)
  autocomplete.rs   inline suggestion dropdown
  editor.rs         launch $EDITOR via Tui::suspend
  clipboard.rs      best-effort copy/paste (native Win32 on Windows, CLI tools elsewhere)

  # data display
  table/            Table (type-aware sort, fuzzy filter, row/cell select),
                    split into model / interaction / render
  tree.rs           collapsible TreeView
  list.rs           selectable list + scrollbar (ListView; render/_counted/_boxed)
  markdown/         CommonMark renderer: render/ is the engine (mod = block
                    walk, inline, overlay, wrap, block_list, block_table),
                    theme = StyleSheet default/from_skin, view = MarkdownView
  sidebar/          sectioned menu column, split into mod / interaction /
                    layout / render
  tabs.rs, pager.rs, gauge.rs, spinner.rs, toast.rs

  # pickers / modals
  modal/            one file per widget family: confirm / text_input / number /
                    picker / message, over a shared render (geometry, hint
                    block, picker list)
  form/             schema-driven form (text/multiline/bool/choice/date),
                    split into mod / field / run / render
  finder.rs         fuzzy picker; fuzzy.rs: nucleo score + highlight + rank_by
  filter_list.rs    the query/cursor/scroll core shared by the three filtered
                    overlays (finder, help, command_palette) - crate-internal
  color_picker/     mod / interaction / render
  swatches/         mod / named (the CSS catalogue) / cells / interaction /
                    render
  path_picker/      mod / fs (the confinement checks) / interaction / render
  date_picker.rs, date_range_picker.rs, month_picker.rs, slider.rs

  # chrome / misc
  help.rs           sectioned, fuzzy help overlay (Tab jumps sections)
  command_palette/    fuzzy command palette overlay (mod / layout / render)
  shortcut_hints/     footer key-hint lines (flat or grouped), split into
                      state (the global toggle) and render (the layout)
  quit.rs             opt-in confirmation before quitting (policy + guard)
  opener.rs           open a file in the OS default app (no shell)
  theme_preview.rs    palette/variant preview for a gallery
  header.rs, statusbar.rs, double_press.rs
tests/
  render.rs         headless TestBackend render smoke tests
  logging.rs        verifies a `log` diagnostic fires (via a capture logger)
```

## Conventions (SSOT)

This crate is the single source of truth for the TUI conventions in CLAUDE.md §7.10. When building a widget, reuse the shared building blocks rather than reinventing them:

- **Navigation:** `nav::cycle` (wrapping), `nav::keep_visible` (scroll offset). Never open-code `(i + 1) % len`.
- **Filtered overlays:** `finder`, `help` and `command_palette` share `filter_list::FilterList` - the query buffer, the cursor and the whole navigation/typing dispatch (including the `Ctrl` guard and the `Tab` section jump). A new overlay of that shape delegates to it and keeps only its own keys (`Esc`, a confirming `Enter`).
- **Fuzzy ranking:** `fuzzy::rank_by` is the shared score-filter-sort pass behind every filter picker; a caller supplies only how to derive each item's haystack.
- **Scrollbar:** `scroll::render_scrollbar` – renders only on overflow and skips empty areas; `list::render` already calls it, so list-backed widgets (`tree`, `finder`, `help`, `path_picker`) get it for free.
- **Framing:** `chrome::framed_decor` draws the rounded accent box with a caption (top border) and badge (bottom-right); every boxable widget goes through it and exposes `.boxed(decor)`.
- **Global chords:** the toolkit intercepts exactly two keys – `Ctrl+Q` (`terminal::classify`) and the hints toggle (`driver::run`/`overlay::popup`). `shortcut_hints::global_bindings` names them with their current bindings, for a host to splice into its footer and help; the host's own conventional keys (`q`, `?`) stay with the host, which alone knows when the user rebinds them. A quit confirmation is opt-in via the `quit` module. An app that drives its own event loop calls `shortcut_hints::consume_toggle(key)` at the top of its key dispatch — never match the chord by hand, or the modifier check gets forgotten.
- **Hint footers:** every footer goes through `shortcut_hints::lines`/`footer_height` (a popup) or `group_lines`/`height`/`render` (the host's main-app footer) — never hand-roll a hint line. The global `F1` toggle governs **only the grouped main-app footer**: `group_lines`/`height`/`render` collapse to nothing while the hints are hidden. The flat popup API (`lines`/`footer_height`) is **un-gated** and always renders, because a modal's key prompt (a confirm's `y/n`, a picker's `enter/esc`) is essential and must show regardless of the toggle. A popup that *does* want its footer to follow `F1` opts in by guarding its hint construction with `shortcut_hints::visible()`.
- **Block caret:** filter/search lines go through `input::query_spans` (caret at the end, for widgets keeping a bare `String`), fields with their own cursor through `InputField::caret_spans`. Both scroll horizontally and mark a scrolled-off head with `…`. Never rebuild the caret span inline.
- **Position badge:** the `position/total` (or percent) indicator never overlays content. Where there is a frame, `chrome::render_badge` paints it into the bottom border – used by `framed_decor` and by every popup that frames a scrollable list; whoever owns the frame owns the badge. Where there is none, `chrome::render_corner_badge` puts it right-aligned into a reserved bottom row (`list::render_counted` does that for a list, and yields the row back when the area is too short to spare one). Both take their label from `chrome::position_badge` and their colour from `style::muted`.
- **Popup sizing:** `layout::centered_fraction` gives every centered popup its size (a fraction of the area, grown to a preferred minimum). It and every hand-rolled popup rect derive that size through `layout::fit(wanted, min, max)`, never through `clamp(min, max)`: a terminal below the popup's minimum makes `max < min`, and `Ord::clamp` panics on that. The available space is the hard limit; the minimum is only a preference.
- **Colors:** only `style.rs` maps `theme::Color` to ratatui; widgets take a `&Skin`/`&Palette`, never a raw literal. This holds for the Markdown stylesheet too: `markdown/theme.rs` declares its palette in `theme::Color` and maps it through `style::to_ratatui`, even though `StyleSheet`'s fields are ratatui types. `style::LIGHT_BG`/`DARK_BG` are the shared contrast backdrops for the colour pickers.
- **Focused frames:** a focused field brightens its own fill, so a fixed border loses most of its contrast against it. Draw such a frame from `border_focus` (`style::border_focus`), not from `border`. It is lifted above `border` and follows it: a theme or a host that sets only `border` gets a matching focus color for free, and an explicit `border_focus` always wins. Because `chrome::border_title` reads the stroke colour from `palette.border`, a widget that titles a focused box hands it a `Skin` copy whose `border` *is* the focus color — styling `border_style` alone would leave that one stroke behind.
- **Validating a theme table:** check a `[themes.<name>]` against `ThemeColors::KEYS`, never against `Palette::KEYS`. The palette carries derived colors a theme cannot contribute (`selection`, `cursor`, `input_bg`, …); accepting them there drops the value without a word.
- **Text editing:** `input::TextCursor` + the public `input::apply_edit_key` are the shared caret/edit core. `EditMode` picks the geometry: `InputField` drives it with `SingleLine`, `textarea::TextArea` with `Multiline`, so both carry one set of shortcuts. Widths are measured with `unicode-width` (wide glyphs count as 2).
- **Logging:** diagnostics go through the `log` facade, sparingly. `error!` for an unrecoverable, otherwise-silent failure (a failed terminal restore in `Drop`); `warn!` for a noticeable degradation (a missing clipboard tool, an unreadable directory, a canonicalize fallback that weakens the path confinement, an invalid color override, an unknown theme name); `debug!` for per-attempt breadcrumbs. Never log in the hot path (per-frame render, the sort comparator) or on normal control flow (Esc, empty input). The host installs the logger; the library only emits.

## Common commands

```bash
cargo build
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test            # unit tests + doctests + tests/render.rs
```

`ratada` is a library, so there is no binary to run; exercise widgets through the bundled examples, the `clibase` gallery (`cargo run` in `../../templates/clibase`, view `3`) or a `TestBackend` in `tests/render.rs`.

```bash
cargo run --example counter   # minimal Screen/run app (space +1, q quits)
cargo run --example gallery   # static one-frame widget collage; any key quits
```

See [Screenshot](#screenshot) below for how the `gallery` example is captured.

## Screenshot

The `gallery` example renders a static, one-frame dashboard – header, tab bar, a boxed table, a tree, a list, a Markdown view, a gauge, shortcut hints and a status bar – built to be captured for the README screenshot:

```bash
cargo run --example gallery
```

Size the terminal to taste (roughly 100x30 reads well), take the screenshot, then press any key (or `Ctrl+Q`) to leave. The `clibase` template additionally renders every widget in a live, interactive gallery (run it and open `view 3`).

## Testing

- **Unit tests** live inline (`#[cfg(test)] mod tests`) and cover pure logic (navigation, wrap/width, filtering, selection, badge counts).
- **Doctests** on the key public items double as compile-checked examples.
- **`tests/render.rs`** renders the frame-based widgets into a `TestBackend` plain/boxed, at a roomy and a cramped size, and with wide characters – a panic-free smoke test (this is how the empty-area scrollbar panic was caught). Popups (`help`/`finder`/`modal`/`path_picker`) need a live `Tui` loop and are covered by their unit tests instead.

## Adding a widget

1. Add `pub mod <name>;` to `lib.rs` (flat at the crate root).
2. Take a `&Skin` (or `&Palette`) for styling; never depend on host types.
3. Reuse `nav`/`scroll`/`chrome`/`style` and the `unicode-width` helpers.
4. If it should support the boxed style, store an `Option<chrome::BoxDecor>` and render through `chrome::framed_decor`; add a `.boxed(decor)` builder.
5. Add unit tests for the logic and a case in `tests/render.rs`; a doctest for the constructor.
6. Keep the rustdoc, `README.md` and `CLAUDE.md` in sync when the public API changes.

# Code Walkthrough & Cleanup (checklist to tick off)

> [!NOTE]
> Completed, archived working document – all items are done. It describes the cleanup pass; module/file mentions (e.g. `palette`, `table.rs`) reflect the state **before** the rework. The final state: `palette` → `command_palette`, `table.rs` → `table/` submodule, new `markdown` module; for the current structure see `DEVELOPMENT.md`.

## Context

The repo is stable and clean after several feature rounds (`cargo fmt --check` green, `cargo clippy --all-targets -- -D warnings` green, the full test suite green, `clippy::pedantic` crate-wide, only a few justified `#[allow]`, no open TODOs, every module has a `//!` doc). `ratada` is the **library itself** – the reusable ratatui widget toolkit plus the framework-agnostic `theme` layer; there is no binary, no domain/persistence layer. Consuming apps (e.g. `clibase`) depend on it as a path dependency. This checklist therefore concerns the **entire crate**; since it is a public API, visibility and signature stability are especially important here (`pub` changes are breaking changes).

Ordering principle: first establish a baseline, then layer by layer from the dependency-free foundations (`theme`) outward to the composed widgets (this way understanding builds up bottom-up and each layer is checked after its dependencies), and finally a cross-cutting pass.

## Generic checkpoints (apply to EVERY module)

While going through each file, check the following each time (CLAUDE.md §1, §2, §7):
- **Names:** predicates `is_/has_/can_/should_`; methods = verbs, types = nouns; no `Manager/Helper/Data` catch-all names; no negative booleans; acronyms like normal words (`UserId`, not `UserID`).
- **Functions:** SLAP (one level of abstraction), at most 2 levels of nesting with early return, ≤ 3 parameters (otherwise a struct), command-query separation.
- **Visibility:** as private as possible; `pub` only where truly needed for the public API – internals to `pub(crate)`; keep prelude re-exports lean (`lib.rs`).
- **Errors:** `Result`/`?`, no `unwrap/expect/panic` in the normal flow; every `expect` justified. The `Screen` trait leaves the error type to the host – no `anyhow` in the public API.
- **Magic numbers/strings:** replaced by named constants/`enum`s (glyphs, colors, keyboard shortcuts, layout dimensions).
- **Hygiene:** no dead/commented-out code; comments explain the *why*; doc comments per public item, first line a one-sentence summary, prose instead of `# Arguments`; 80 columns; straight quotation marks; no em dash.
- **Tests:** logic-bearing code has tests; test names describe behavior; doctests in `# Examples` must run.
- **TUI conventions (§7.10):** cyclic navigation via `nav::cycle`/`rem_euclid` (not `saturating_add/sub`); scrollbar on overflow via `scroll::render_scrollbar`; overflow truncation via `text::truncate`; rounded borders; glyphs in both variants; colors held centrally in the `theme` submodule.

---

## Orientation – reading pass (before phase 0, no changes)

Bottom-up, only *reading*, to build up the mental map before cleaning up. Nothing is changed here – just capture the wiring and module structure.

- [x] `lib.rs`: skim the module tree, crate-wide `#![warn/allow]`, public re-exports and `prelude` – what is visible to the outside, which layers are there?
- [x] `theme/mod.rs` → `style.rs`: follow the one seam `theme::Color → ratatui::style::Color`; everything else builds on it.
- [x] Follow the dependencies from the inside out (`theme` → primitives `nav/scroll/text` → `terminal/driver` → `overlay/chrome` → input/display/picker → composed widgets `modal/form/finder/help`). Note anything conspicuous, but don't touch it yet – that happens bottom-up from phase 1.
- [x] Cross-reference with the docs: skim rustdoc (SSOT of the API), `DEVELOPMENT.md`, `README.md` and reconcile them with the actual module tree.

## Phase 0 – Baseline & Scope

- [x] Run `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` – green starting state confirmed.
- [x] Use a clean branch (`clean-up`) (no commit on `main`); back up the working state.
- [x] Decide: pure review (only reading + mini-fixes) vs. real refactors – stake out the scope. For `pub` signature changes, deliberately treat them as a breaking change and document them.

## Phase 1 – theme (`src/theme/`)

The dependency-free foundation (only `serde` for the persistable enums).

- [x] `color.rs` (`Color`, `parse_color`, `hex`, OKLCH variants `darken`/`lighten`/`vivid`/`dim`/`shade`/`mix`/`readable_on`, `distance`, model conversions `to_hsl`/`from_hsl`/`to_oklch`/`from_oklch`): generic checks; parsing error paths robust (no `unwrap`); value ranges/clamping.
- [x] `glyphs.rs` (`Glyphs`, `GlyphVariant`): two icon variants (Unicode + ASCII fallback), no emojis; `serde` derivations deliberate.
- [x] `palette.rs` (`Palette`, `resolve`, `ColorOverrides`, `define_palette!`): SSOT of the accent/dim/tint colors (palette fields declared once in the macro); override merge clear; named constants instead of scattered RGB literals.
- [x] `skin.rs` (`Skin`): bundle of palette/glyphs – lean construction.
- [x] `theme_set.rs` (`ThemeRegistry`, `ThemeColors`, built-in themes `default`/`monochrome`): registry structure, default fallback; no magic strings for theme names.
- [x] `mod.rs`: re-exports minimal and consistent.

## Phase 2 – style (`src/style.rs`)

- [x] `style.rs`: the **only** seam `theme::Color → ratatui::style::Color`. Check that this mapping is not duplicated anywhere else (DRY/SSOT); conversions complete and without panics.

## Phase 3 – Primitives & Utilities (`nav`, `scroll`, `layout`, `text`, `fuzzy`, `double_press`)

Stateless helpers on which the widgets build – free functions (CLAUDE.md §2.6).

- [x] `nav.rs` (`cycle`/`rem_euclid`): cyclic navigation as SSOT; an empty list yields index 0; edge clamping for pages/jumps correct.
- [x] `scroll.rs` (`render_scrollbar`): visible style without arrows (thumb `foreground_dim`, track `border`), takes `skin`; position number `total - viewport + 1`; only on overflow.
- [x] `layout.rs`, `text.rs` (`truncate`): overflow truncation with `…` to the visible width; `unicode-width`-correct (no byte/char confusion with wide glyphs).
- [x] `fuzzy.rs` (backed by `nucleo-matcher`): match/ranking interface clear; inputs limited.
- [x] `double_press.rs`: time-window logic; `Instant` usage; generic checks.

## Phase 4 – terminal & driver (`src/terminal.rs`, `src/driver.rs`)

The app frame: RAII guard and event loop.

- [x] `terminal.rs` (`Tui`, `TuiEvent`, `with_hooks`, `suspend`): raw mode + alternate screen on creation, clean restore in `Drop` (also on error paths/panic); `Resize` triggers a redraw; lifecycle hooks correctly included.
- [x] `driver.rs` (`Screen`, `Flow`, `run`, `TICK`): generic loop; `type Error: From<io::Error>` lets the host choose the error type; `# Errors` doc; `tick` cadence justified (`TICK` constant).

## Phase 5 – chrome & overlay (`src/chrome.rs`, `src/overlay.rs`)

- [x] `overlay.rs` (`popup`, `PopupFlow`, dim backdrop): the one overlay primitive – centered box + `Clear` + key routing as SSOT for every blocking widget. Check that pickers/modals really run through it (no reimplementations).
- [x] `chrome.rs` (`panel`/`menu_panel`/`modal_block`/`BoxDecor`/`framed_decor`): centralizes the border chrome (caption in the top border, badge at the bottom right via `framed_decor`); rounded borders (`BorderType::Rounded`); views/widgets don't build blocks inline.

## Phase 6 – Text input & editing (`input`, `textarea`, `autocomplete`, `clipboard`, `editor`)

- [x] `input.rs` (**shared editing core**: `apply_edit_key`, `TextCursor`, `render_line`): SSOT/DRY of the editing shortcuts (one caret + optional selection anchor). The core handles only editing keys, control keys belong to the caller; horizontal scrolling with `…` clipping; `unicode-width`-correct caret position. `apply_edit_key` is now `pub` (mode-aware via `EditMode`), so that a host with its own layout uses the same caret logic instead of reimplementing it.
- [x] `textarea.rs`: multiline, shares `input::TextCursor` – check that the editing logic is not duplicated; word-wise wrapping; block cursor; `Up/Down` only multiline.
- [x] `autocomplete.rs`: inline dropdown for suggestions; navigation cyclic via `nav`; scrollbar via `scroll`.
- [x] `clipboard.rs`: external tools via `Command` with `.arg()`/`.args()` – **no `sh -c` with composed strings** (§7.9 command injection); error paths controlled.
- [x] `editor.rs`: `$EDITOR` via a temp file, terminal suspended/restored around the process via `Tui::suspend`; command-injection discipline; temp-file handling robust.

## Phase 7 – Display widgets (`table`, `tree`, `list`, `sidebar`, `tabs`, `pager`, `gauge`, `spinner`, `toast`, `header`, `statusbar`, `shortcut_hints`, `theme_preview`)

- [x] `table.rs` (**largest file, ~1170 lines**): check the dense render/navigation functions specifically for SLAP and nesting depth; navigation helper via `nav`; sticky header/column head; no magic strings. Candidate for decomposition into smaller units (see concrete candidates).
- [x] `tree.rs`, `list.rs`: navigation/selection/scroll offset generic; `list.rs` carries the one `#[allow(too_many_arguments)]` – check (see candidates).
- [x] `sidebar.rs`: sectioned menu column (header + items, optional `/`-fuzzy filter, `Overflow::Truncate`/`Scroll` with a horizontal scrollbar); selection skips headers, `selected_id` mapping; highlight = pointer + accent + `selection` tint; uses `nav`/`text`/`scroll`/`chrome::menu_panel`.
- [x] `tabs.rs`: tab bar, active tab in the accent tone; cyclic.
- [x] `pager.rs`: scroll/page navigation; scrollbar on overflow; `PageUp/Down` clamped.
- [x] `gauge.rs`, `spinner.rs`, `toast.rs`: small display widgets; animation via `tick`; named constants for frames/timings. `gauge.rs`: percent label over the filled bar in a contrast color (`readable_on`).
- [x] `theme_preview.rs`: renders the color/variant preview (OKLCH steps) for the gallery – no magic RGB, colors from `palette`.
- [x] `header.rs`, `statusbar.rs`, `shortcut_hints.rs`: `shortcut_hints::lines`/`group_lines` as the shared hint helper (`(key, description)` tokens, key in the accent tone, ` · `-separated, wrapping); `statusbar` as a transient status line; secondary text dim.

## Phase 8 – Picker (`color_picker`, `swatches`, `date_picker`, `date_range_picker`, `month_picker`, `path_picker`, `slider`)

All should be thin wrappers over `overlay::popup` – shared look/shortcuts.

- [x] `date_picker.rs`, `date_range_picker.rs`, `month_picker.rs`: shared calendar modal pattern; `chrono` usage (no `unwrap` outside tests); uniform shortcuts; edge/month-change logic.
- [x] `color_picker.rs`, `slider.rs`: value ranges/clamping; step sizes as named constants. `color_picker.rs`: RGB/HSL/OKLCH models (toggle via `m`), gradient slider with marker, editable hex field, palette presets, light/dark preview; returns `ColorExit` (`Enter`=Done, `Esc`=Back, `s`=Swatches, Ctrl+Q=Quit); model conversions as SSOT in `theme::color` (`to_hsl`/`from_hsl`/`to_oklch`/`from_oklch`).
- [x] `swatches.rs`: multi-mode color picker (`m` cycles Names/Grid/Grays/Palette; color carried over via `Color::distance`); Names/Palette as a list via `list::render`, Grid (hue×saturation, `[`/`]` = brightness) and Grays as a color raster; `/`-filter in Names, focus preview. `color_chooser` connects the swatch and picker views (switching loop): `Enter` swatch→picker, `Esc`/`s` picker→swatch (mode/brightness are preserved), `Space` = direct, `y` = copy; `swatch_picker` is the wrapper starting in the swatch view.
- [x] `path_picker.rs`: **secure path traversal** – check paths from outside with `canonicalize()` + `starts_with()` (§7.9); directory navigation robust; scrollbar on overflow.

## Phase 9 – Composed widgets (`modal`, `form`, `finder`, `help`)

- [x] `modal.rs` (`ModalSignal`, `confirm`/`select`/`multi_select`/`number_input`/`message`): the shared modal set as SSOT – not reimplemented per call site; destructive actions via `confirm`; `ModalSignal::Quit` propagation consistent.
- [x] `form.rs`: all fields visible; `Tab/BackTab` wrapping, `Ctrl+S`/`Esc`; focus tint; dirty marker `*`/reset `r`; external editor `Ctrl+G`; pan mode. Check the dense dispatch function for SLAP.
- [x] `finder.rs`: fuzzy filter via `fuzzy`; scrollable list; selection return.
- [x] `help.rs`: full overlay with all shortcuts, scrollable fuzzy finder; footer points to it with `? help`. When changing shortcuts, keep footer/help/docs in sync.

## Phase 10 – Crate root (`src/lib.rs`, `tests/render.rs`)

Last, because everything comes together here:

- [x] `lib.rs`: module declarations complete/consistent; public re-exports and `prelude` minimal and deliberate (breaking-change surface); confirm the crate-wide `#![warn(clippy::pedantic)]` and the three `#![allow(...)]` blocks (cast lints, `must_use_candidate`/`missing_errors_doc`) with a current justification; module doc with a runnable example doctest up to date.
- [x] `tests/render.rs`: integration render tests cover the central widgets; name gaps if any (assess, not necessarily extend – YAGNI).

## Phase 11 – Cross-cutting & wrap-up

- [x] **`#[allow]` inventory:** deliberately confirm the crate-wide allows in `lib.rs` (cast lints, `must_use_candidate`, `missing_errors_doc`); reduce the local `#[allow(clippy::too_many_arguments)]` in `list.rs:39` (on `render_boxed`) after phase 7 where possible (group parameters into a struct) or deliberately keep it + a current justification.
- [x] **Docs sync:** `README.md` / `DEVELOPMENT.md` / `API.md` and the rustdoc comments against the cleaned-up state; footer/help/shortcut references consistent; `prelude` description correct.
- [x] **Tests:** paths touched by refactors tested; all green (incl. doctests).
- [x] **Final gates:** `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` – all green.
- [x] Propose commit message(s) in Conventional-Commits style (no auto-commit per CLAUDE.md §11).

Concrete candidates:
- [x] **`table.rs` (~1170 lines):** by far the largest file. Check the render/navigation responsibilities for SLAP and, if appropriate, decompose into coherent units (sticky header, column layout, body render, navigation). Pure refactoring, behavior identical – the render tests must pass without regeneration.
- [x] **`list.rs:39` `#[allow(clippy::too_many_arguments)]` (on `render_boxed`):** the fanned-out signature is an indicator of too many parameters (§2.5). Check whether related parameters can be grouped into a struct so that the `#[allow]` becomes unnecessary.
- [x] **Editing-core duplication:** double-check that `textarea.rs` really draws the editing logic from `input.rs` and doesn't reimplement anything in parallel (SSOT/DRY of the text-field shortcuts, §7.10).

## Verification

After each layer and at the end: `cargo fmt --check` + `cargo clippy --all-targets -- -D warnings` + `cargo test` green. Pure refactorings must not change behavior – the render/integration tests (`tests/`) and doctests must pass without regeneration; only for a deliberate behavior/layout change update snapshots/expectations in a targeted way.

## Notes / non-goals

- **`ratada` is the library:** changes to `pub` signatures are breaking changes for consuming apps – make them deliberately and documented; keep the public API small.
- **No binary/no domain:** there is deliberately no `main`, no CLI, no persistence – only the generic TUI building blocks. Don't "retrofit" any of it (YAGNI).
- Known, separate: CI workflow (`cargo audit`, §7.9) – deliberately left outside this cleanup pass (unless already present).
- KISS/YAGNI over "my style": respect local style, only touch what the task requires, separate refactoring from behavior (CLAUDE.md §3).

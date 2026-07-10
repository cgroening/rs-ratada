# Changelog

All notable changes to `ratada` are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project follows
[Semantic Versioning](https://semver.org/spec/v2.0.0.html) – while `0.x`, a minor
bump may contain breaking changes.

## [Unreleased]

### Added

- `tree::TreeItem::leaf_with_id` plus `tree::TreeView::selected_id` and
  `selected_is_leaf` – a leaf may now carry a caller-defined id, and the view
  hands it back for the node under the cursor. Labels are not unique, so an id
  is the only reliable way to map a selection back to the caller's data.
  `TreeItem::leaf`/`node` keep their signatures and simply carry no id.
- `layout::fit` – grows a size to a preferred minimum, then caps it at the
  available space. The single seam for popup sizing.
- `modal::input_wide` – a single-line text prompt in a box spanning ~90% of the
  terminal width, so long values (such as file paths) stay visible instead of
  scrolling in a narrow box.
- `modal::number_input_bounded` – like `number_input`, but the accepted value is
  clamped to `[min, max]`.
- `chrome::border_title` – public helper that builds the inset border-title line
  (`╭─ Title ───`); the single source of truth every framed box titles with.
- `chrome::render_badge` and `chrome::position_badge` – the single seam for the
  bottom-right `position/total` indicator and its 1-based label.
- `chrome::render_corner_badge` and `list::render_counted` – the same indicator
  for a widget with **no** frame to hang it on: right-aligned in a reserved
  bottom row, so it never overlays an entry. An area too short to spare that row
  keeps its content instead.
- `style::muted` and the palette color `foreground_muted` – a text tone between
  `foreground_dim` and `border`, for chrome annotations that must not compete
  with the content.
- `style::border_focus` and the color `border_focus` – the border of a *focused*
  box. A focused field brightens its own fill, and a fixed border loses most of
  its contrast against it; `border_focus` is lifted above `border` so the frame
  stays legible in both states. It exists both as a `ThemeColors` base color (a
  `[themes.<name>]` may ship its own) and as a `Palette` color (a host may
  override it). Left out, it follows `border` – and an override on `border`
  alone drags it along, so the pair can never drift apart.
- `ThemeColors::KEYS` – the color names `ThemeColors::from_lookup` actually
  reads, so a host can validate a `[themes.<name>]` table against them. Checking
  a theme against `Palette::KEYS` instead accepts derived colors (`selection`,
  `cursor`, `input_bg`, …) that a theme cannot contribute, and drops them
  silently.
- `nav::scroll_percent` – how far a `ScrollView` is scrolled, in percent.
- A global `F1` chord toggling every shortcut-hint footer (shown by default).
  `driver::run` and `overlay::popup` consume it, so every screen and every modal
  inherits it and the host wires up nothing. Hiding the hints reclaims their
  rows, the blank spacer above them included, so widget boxes shrink to fit.
  New: `shortcut_hints::{visible, set_visible, toggle, footer_height,
  default_toggle_key, toggle_key, set_toggle_key, global_bindings}`. The chord
  is rebindable, and unbinding it with `set_toggle_key(None)` hands the key back
  to the host. `global_bindings` yields the chords the toolkit itself intercepts
  – the toggle (named after its current binding) and the hard `Ctrl+Q` – for a
  host to splice into its footer and help overlay; with the hints hidden the
  toggle is nowhere else to be seen.
- `shortcut_hints::consume_toggle` is now public, so an app that drives its own
  event loop instead of `driver::run` can inherit the hints toggle with one line
  at the top of its key dispatch, rather than matching the chord by hand.
- `quit` – an opt-in confirmation before quitting. `quit::set_confirm` picks
  whether the hard `Ctrl+Q`, the host's own quit action, both or neither are
  questioned (neither, by default); `quit::set_guard` registers how the dialog
  is drawn. `run` and `popup` ask for the hard chord themselves; a host calls
  `quit::request` in its own quit action, which is the only place that knows
  where that quit came from.
- `input::query_spans` and `InputField::caret_spans` – a text line with a block
  caret and no field background, scrolling horizontally to keep the caret in
  view. The single source every filter/search line now draws its caret with.

### Fixed

- Opening a popup in a terminal narrower or shorter than the popup's preferred
  minimum panicked with `assertion failed: min <= max`. `modal::confirm`,
  `message`, the list pickers, `input_wide`, `command_palette` and
  `layout::centered_fraction` reached `Ord::clamp(min, max)` with `max < min`.
  They now use `layout::fit`, where the available space wins over the preferred
  minimum. A `confirm` dialog in a 20x6 terminal used to crash the host
  application.
- `path_picker` shows the block caret in its filter line again. The field is a
  full `InputField`, but the render path drew only its value, so nothing marked
  where typing would insert – including on an empty filter.
- The hints toggle compared only the key code, so `Shift+F1` toggled the hints
  as well. Modifiers are now matched exactly.
- An unboxed `tree` shows its `position/total` counter again. It had vanished
  with the overlay chip, since a frameless widget had nowhere to put one.

### Changed

- **Breaking:** `ThemeColors` gained a `border_focus` field. Every struct
  literal has to name it; `ThemeColors::derived`, `from_accent` and
  `from_lookup` fill it on their own. Hosts building a theme from a color
  table are unaffected.

- The `position/total` indicator now always sits in a frame's bottom border
  (`─ 3/12 ─╯`) instead of floating over the last list row, and is drawn in the
  new, dimmer `foreground_muted`. Popups over a scrollable list – `path_picker`,
  `finder`, `command_palette`, `help`, `swatches`, `modal::select`,
  `modal::multi_select` and the `table` modal – gained one; `form` gained a
  focused-field counter. A frame too narrow for the badge drops it rather than
  overwriting a corner.
- `pager` and the `markdown` viewer show their scroll percentage in the bottom
  border; the pager's footer no longer repeats it.
- A boxed `table`'s badge now reads `12/80` (cursor position and row count)
  instead of the bare row count. Its status line is unchanged.
- `shortcut_hints::height` returns `0` while the hints are hidden (previously at
  least `1`), and `lines`/`group_lines` return an empty `Vec`, so a host that
  budgets its footer with `height` reclaims the top margin along with the hints.
- Filter and search lines (`finder`, `command_palette`, `help`, `swatches`,
  `sidebar`, `table`, `pager`) scroll horizontally and mark a scrolled-off head
  with `…`, instead of being cut off at the line end. They share one caret
  renderer rather than each rebuilding the caret span inline.

- Modal frames now fill with a slightly lightened background, lifting the box
  above the dimmed backdrop so it reads as an elevated surface.
- Form and toast frames now title via the shared `chrome::border_title`, so
  their leading `─` connector takes the border color (matching modals) instead
  of the title/accent color.
- `modal::number_input` now falls back to the initial value instead of `0` when
  the entered text cannot be parsed as an integer.

### Removed

- The overlay `position/total` chip `list::render` used to draw over its last
  row. Plain `list::render` no longer shows a count; use `list::render_counted`
  (a reserved bottom row), `list::render_boxed` with a `BoxDecor`, or let the
  surrounding popup frame carry the badge.

## [0.2.0] - 2026-07-07

### Added

- `markdown` module: a CommonMark renderer (headings, lists, task lists, code
  blocks, blockquotes, GFM tables and callouts, links, plus a `==highlight==`
  extension) that produces styled `ratatui` lines. Includes a themeable
  `StyleSheet` (`Default` plus `StyleSheet::from_skin`), a scrollable
  `MarkdownView` widget with link navigation, and a blocking `viewer` modal.
  Backed by the new `pulldown-cmark` dependency.
- `text::wrap` – unicode-width-aware word wrapping (hard-splits over-long words).
- `log` diagnostics for degraded conditions (`warn`/`error`): a failed terminal
  restore on exit, a missing clipboard tool, an unreadable directory, a
  `canonicalize` fallback that weakens path confinement, an invalid color
  override, and an unknown theme name.

### Changed

- Complete rustdoc coverage, enforced crate-wide with `#![warn(missing_docs)]`.

## [0.1.0] - 2026-07-02

Crate-wide cleanup and API consolidation. This release contains breaking changes.

### Changed

- Grouped scroll parameters into `nav::ScrollView` (used by
  `scroll::render_scrollbar`/`render_hscrollbar` and `nav::keep_visible`) and
  list parameters into `list::ListView` (`list::render`/`render_boxed`).
- `path_picker::path_picker` now takes a `PathPickerConfig` with an optional
  `root` that confines navigation (checked via `canonicalize` + `starts_with`).
- Renamed the crate-root command-palette module `palette` → `command_palette`
  (`PaletteItem` → `CommandItem`), ending the name clash with `theme::palette`.
- Split the `table` widget into a `table/` submodule (model / interaction /
  render); behavior unchanged.
- `textarea` now reuses the shared single-line edit core from `input` (SSOT).
- Documentation synced with the code and the public API surface tightened.

[Unreleased]: https://github.com/cgroening/rs-ratada/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/cgroening/rs-ratada/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/cgroening/rs-ratada/releases/tag/v0.1.0

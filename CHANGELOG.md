# Changelog

All notable changes to `ratada` are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project follows
[Semantic Versioning](https://semver.org/spec/v2.0.0.html) – while `0.x`, a minor
bump may contain breaking changes.

## [Unreleased]

### Added

- `modal::input_wide` – a single-line text prompt in a box spanning ~90% of the
  terminal width, so long values (such as file paths) stay visible instead of
  scrolling in a narrow box.
- `modal::number_input_bounded` – like `number_input`, but the accepted value is
  clamped to `[min, max]`.
- `chrome::border_title` – public helper that builds the inset border-title line
  (`╭─ Title ───`); the single source of truth every framed box titles with.

### Changed

- Modal frames now fill with a slightly lightened background, lifting the box
  above the dimmed backdrop so it reads as an elevated surface.
- Form and toast frames now title via the shared `chrome::border_title`, so
  their leading `─` connector takes the border color (matching modals) instead
  of the title/accent color.
- `modal::number_input` now falls back to the initial value instead of `0` when
  the entered text cannot be parsed as an integer.

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

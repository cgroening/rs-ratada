# Changelog

All notable changes to `ratada` are documented here. The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html) – while `0.x`, a minor bump may contain breaking changes.

## [Unreleased]

### Removed

- **Breaking: `keymap::KeyChord::code`.** It was added in 0.4.0 on spec and has no caller in any consuming app; `KeyChord::to_key` covers the one real need (synthesizing the press a chord matches). Since 0.4.0 is the release that introduced it, nothing that ever shipped can depend on it.

### Fixed

- Nothing user-visible: this release pins the 0.4.0 key-handling rules with tests. Every Ctrl guard, the `AltGr`-must-type rule of each filter buffer, `is_bare_character`'s `Alt` clause, `is_global_quit`, and `keymap::Action::overlaps` now have a test that fails if the behaviour is reverted. Several were previously provable only by reading the code - and one, `is_bare_character`'s `Alt` clause, could be deleted with every doctest still green.

## [0.4.0] - 2026-07-17

### Added

- **`keymap` – user-remappable key bindings, the shared chord layer.** An app implements `keymap::Action` for its own action enum (`all`/`config_name`/`description`/`default_keys`, plus an optional `overlaps` for scoped actions and a defaulted `from_config_name`) and gets:
  - `KeyChord` – `parse` (`"ctrl+s"`, `"shift+left"`, `"pgup"`, `"G"`), `matches`, `display`, plus `from_key`/`to_key` to convert to and from a live `KeyEvent` and `code`.
  - `Keymap<A>` – `from_overrides` (every action), `for_actions` (a subset, for an app that builds one map per view), `action_for`, `action_for_where` (a filtered lookup for scoped actions), `keys_for`, `hints` (`(keys, description)` pairs for a footer), `conflicts`, and `Default`.
  - `Conflict<A>`, the `KeyBinding` serde enum (`key = "ctrl+s"` and `key = ["ctrl+s", "f2"]` both deserialize) and `warn_unknown`.

  This replaces the ~285 lines of chord machinery each consuming app had copied; the action table stays in the app, the chord grammar lives here once. `KeyChord::matches` compares `ctrl`/`alt` **exactly**, never with `contains`, which is what keeps `AltGr` (reported as `Control + Alt`) from triggering a `ctrl+…` binding. `Action::all` returns an iterator rather than a slice, because an app's actions are typically one column of a catalog table and a `&'static [Self]` would force a second, redundant list. The grammar also gained `backtab` and `insert`, which the hints renderer knew but the chord parser did not.
- `input::is_bare_character` – the counterpart of `input::is_command`: whether a key is a plain typed character (`Char(_)` without Control or Alt). Stricter than `!is_command`, because it also excludes `AltGr`. A widget matching a plain letter (`y` to confirm, `j` to move) gates on this; a widget filling a **text buffer** keeps using `!is_command`, since `AltGr` must type there.

### Changed

- **Breaking (behaviour, not signature): `shift` is significant for non-character keys.** In `keymap::KeyChord`, `left` matches `Left` *without* Shift, and `shift+left` is a separate chord an app can bind. A character key is unaffected: its Shift lives in the case (`G`, not `shift+g`) and is not compared. An app that relied on a bare arrow chord also firing on `Shift`+arrow must bind `shift+…` explicitly - which is the point, since it removes the need to handle `Shift`+arrow beside the keymap.
- **One rendering of a key.** `shortcut_hints`' private `chord_label`/`key_label` now render through `keymap::KeyChord::display`, so a footer hint, a config `[keys]` entry and the chord a handler matches on are the same text, and it round-trips through `KeyChord::parse`. This changes three tokens (`delete` → `del`, `pageup`/`pagedown` → `pgup`/`pgdn`) and, deliberately, **stops lower-casing a character**: an app binding both `g` and `G` used to show one footer token for two actions. `global_bindings()` still returns `String`s.

### Fixed

- **A Ctrl chord no longer triggers a widget's plain-key action.** Every key handler matched on `key.code` while ignoring the modifiers, so - in raw mode, where crossterm reports `Ctrl+J`/`Ctrl+H` as `Char('j')`/`Char('h')` plus CONTROL - a chord fired the bare binding. Most severe: **`Ctrl+Y` silently confirmed a `modal::confirm`**, defeating `Question::declining` on a destructive prompt (`AltGr+Y` did too). Also fixed in `modal`'s list navigation and multi-select, `table` (`Ctrl+S` re-sorted, colliding with `form`'s `Ctrl+S` = save), `tree`, `sidebar`, `pager` (`Ctrl+N` jumped to the next match), `swatches` and `color_picker` (`Ctrl+Y` copied, `Ctrl+S` switched view), the three date pickers, `slider`, `markdown::view` (`Ctrl+O` opened a link) and `form`'s checkbox.
- **A Ctrl chord is no longer typed into a filter or search buffer** as its plain letter, so `Ctrl+U` clears the line instead of inserting a `u` (`command_palette`, `finder`, `help`, `pager`, `sidebar`, `table`, `swatches`). `AltGr` characters (`@`, `\`, `[`, `~`) still reach those buffers, as they always must.
- `terminal`'s global quit and `path_picker`'s `Ctrl+H` now test `input::is_command` rather than a bare `CONTROL` check, so `AltGr` cannot reach them. Neither was reachable in practice - no German `AltGr` glyph collides with `q` or `h` - but the rule no longer depends on that.

## [0.3.1] - 2026-07-16

### Added

- `StyleSheet::preserve_line_breaks` – renders a single source newline (a CommonMark soft break) as a real line break instead of collapsing it to a space (display only). Off by default, so reflowed text is unchanged; a host whose Markdown carries meaningful hard-wrapped lines opts in. Like every `StyleSheet` field it is public, so a struct-literal construction must name it, while `StyleSheet::default` and `from_skin` fill it in.

### Changed

- **The `F1` hints toggle now governs only the host's main-app footer, not popups.** A modal's key prompt (a confirm's `y/n`, an input's `enter/esc`, a picker's hints) is essential, so every popup always shows its footer regardless of the toggle. Concretely, the flat popup API `shortcut_hints::lines`/`footer_height` no longer follows `visible()` (they always render and reserve their rows); only the grouped main-footer API `group_lines`/`height`/`render` still collapses while the hints are hidden. This reverts the part of 0.3.0's "`lines`/`footer_height` return nothing while hidden" for popups. A popup that *does* want its footer to follow `F1` opts in by guarding its hint construction with `shortcut_hints::visible()`. Fixes confirm/input/picker modals rendering with no key hint at all once a host had hidden its footer.
- **Windows clipboard is now native.** `Ctrl+C`/`X`/`V` on Windows talk to the Win32 clipboard directly (new Windows-only `clipboard-win` dependency) instead of spawning a `powershell.exe` per operation, which added ~200-700 ms of lag per keypress. The Win32 API also returns correct Unicode with no OEM-codepage mojibake and no BOM. macOS/Linux keep the `pbcopy`/`pbpaste`, `wl-copy`/`xclip`/`xsel` tools unchanged.

### Fixed

- **A multi-line paste no longer lands with its lines reversed on Windows.** crossterm's Windows event source reads console key records and never emits an `Event::Paste`, so ratada no longer enables bracketed paste there. With it off, a terminal that maps a key to paste (e.g. WezTerm's `Ctrl+V` → `PasteFrom`) delivers plain, in-order text instead of the `\e[200~…\e[201~` sequence crossterm would mangle into reversed lines. On Windows a paste now arrives through the `Ctrl+V` key path (a direct clipboard read); macOS and Linux keep the full `TuiEvent::Paste` / `Screen::handle_paste` seam.

## [0.3.0] - 2026-07-11

### Added

- `opener::open` opens a file in the operating system's default application, joining `clipboard` and `editor` as the OS-integration helpers. It invokes the platform opener (`open`/`start`/`xdg-open`) with an argument list, never a shell, and reports a missing file as `io::ErrorKind::NotFound`.
- **Embeddable editing primitives.** `input` now exposes the pieces a host needs to lay out its own text and still get the toolkit's caret behaviour, instead of copying it: `LineCaret`, `LinePaint` and `line_spans` (paint one already-windowed line), `ScrollPaint` and `scrolled_line_spans` (a scrolled single line marking *both* clipped ends with `…`), `query_spans_at` (like `query_spans` but with a movable caret and a painted selection), `intersect`, `is_command`, `replace_selection`, `insert_str`, `selected_text` and `handle_clipboard`. `LinePaint::content` carries a **per-character style overlay** patched under the caret and selection, so a host can render styled source text - Markdown with its markers kept - inside an editable field.
- `input::EditMode` plus a public, mode-aware `input::apply_edit_key`. It is now the single edit core: `InputField` drives it with `SingleLine` and `TextArea::handle_key` with `Multiline { width }`. `TextArea` thereby gains `Ctrl+U`/`Ctrl+K`, which act on the **display** line.
- `input::TextCursor` gained `at`, `move_to`, `extend_to`, `select_all` and `has_selection`, and now derives `Copy`/`PartialEq`/`Eq`.
- `textarea::wrap_offsets`, `cursor_to_display` and `display_to_cursor` - the wrap and caret mapping are public, so a host can measure and render a wrapped box itself.
- `modal::confirm_default` and `modal::Question`: a yes/no dialog whose `Enter` answer the caller picks. `Question::declining` is the safe default for a destructive prompt, where a stray `Enter` must not confirm the deletion. The footer hint binds `enter` to whichever answer it gives.
- `fuzzy::score_indices` returns the score **and** the matched char positions from one matcher pass; `score`/`match_indices` delegate to it. A search view that ranks and highlights no longer builds the matcher twice per candidate.
- `fuzzy::Fuzzy` - a reusable matcher that keeps its scratch buffers alive and caches the last parsed pattern. The free functions rebuild both on every call, which dominates the work when a whole corpus is scored on each keystroke.
- `scroll::row_indicator` - the right-edge thumb/track cell for one visual row of a box that wraps its own text. A `Scrollbar` owns a whole `Rect`; this rides along inside a `Line` instead of overdrawing the content.
- `chrome::border_title_lead` - the leading `─ ` span that blends a box title into its top border. For a title line that carries more than a label (a dirty marker, a badge) or a box that tints its own border.
- `editor::edit_in_editor_as` - like `edit_in_editor`, but the temp file carries a caller-chosen extension, so `$EDITOR` picks the right syntax and filetype settings. The extension must be a bare ASCII-alphanumeric suffix; anything else is rejected with `InvalidInput` rather than escaping the temp directory.

- `tree::TreeItem::leaf_with_id` plus `tree::TreeView::selected_id` and `selected_is_leaf` – a leaf may now carry a caller-defined id, and the view hands it back for the node under the cursor. Labels are not unique, so an id is the only reliable way to map a selection back to the caller's data. `TreeItem::leaf`/`node` keep their signatures and simply carry no id.
- `layout::fit` – grows a size to a preferred minimum, then caps it at the available space. The single seam for popup sizing.
- `modal::input_wide` – a single-line text prompt in a box spanning ~90% of the terminal width, so long values (such as file paths) stay visible instead of scrolling in a narrow box.
- `modal::number_input_bounded` – like `number_input`, but the accepted value is clamped to `[min, max]`.
- `chrome::border_title` – public helper that builds the inset border-title line (`╭─ Title ───`); the single source of truth every framed box titles with.
- `chrome::render_badge` and `chrome::position_badge` – the single seam for the bottom-right `position/total` indicator and its 1-based label.
- `chrome::render_corner_badge` and `list::render_counted` – the same indicator for a widget with **no** frame to hang it on: right-aligned in a reserved bottom row, so it never overlays an entry. An area too short to spare that row keeps its content instead.
- `style::muted` and the palette color `foreground_muted` – a text tone between `foreground_dim` and `border`, for chrome annotations that must not compete with the content.
- `style::border_focus` and the color `border_focus` – the border of a *focused* box. A focused field brightens its own fill, and a fixed border loses most of its contrast against it; `border_focus` is lifted above `border` so the frame stays legible in both states. It exists both as a `ThemeColors` base color (a `[themes.<name>]` may ship its own) and as a `Palette` color (a host may override it). Left out, it follows `border` – and an override on `border` alone drags it along, so the pair can never drift apart.
- `ThemeColors::KEYS` – the color names `ThemeColors::from_lookup` actually reads, so a host can validate a `[themes.<name>]` table against them. Checking a theme against `Palette::KEYS` instead accepts derived colors (`selection`, `cursor`, `input_bg`, …) that a theme cannot contribute, and drops them silently.
- `nav::scroll_percent` – how far a `ScrollView` is scrolled, in percent.
- A global `F1` chord toggling every shortcut-hint footer (shown by default). `driver::run` and `overlay::popup` consume it, so every screen and every modal inherits it and the host wires up nothing. Hiding the hints reclaims their rows, the blank spacer above them included, so widget boxes shrink to fit. New: `shortcut_hints::{visible, set_visible, toggle, footer_height, default_toggle_key, toggle_key, set_toggle_key, global_bindings}`. The chord is rebindable, and unbinding it with `set_toggle_key(None)` hands the key back to the host. `global_bindings` yields the chords the toolkit itself intercepts – the toggle (named after its current binding) and the hard `Ctrl+Q` – for a host to splice into its footer and help overlay; with the hints hidden the toggle is nowhere else to be seen.
- `shortcut_hints::consume_toggle` is now public, so an app that drives its own event loop instead of `driver::run` can inherit the hints toggle with one line at the top of its key dispatch, rather than matching the chord by hand.
- `quit` – an opt-in confirmation before quitting. `quit::set_confirm` picks whether the hard `Ctrl+Q`, the host's own quit action, both or neither are questioned (neither, by default); `quit::set_guard` registers how the dialog is drawn. `run` and `popup` ask for the hard chord themselves; a host calls `quit::request` in its own quit action, which is the only place that knows where that quit came from.
- `input::query_spans` and `InputField::caret_spans` – a text line with a block caret and no field background, scrolling horizontally to keep the caret in view. The single source every filter/search line now draws its caret with.

### Changed

- **Breaking:** `ThemeColors` gained a `border_focus` field. Every struct literal has to name it; `ThemeColors::derived`, `from_accent` and `from_lookup` fill it on their own. Hosts building a theme from a color table are unaffected.
- The `position/total` indicator now always sits in a frame's bottom border (`─ 3/12 ─╯`) instead of floating over the last list row, and is drawn in the new, dimmer `foreground_muted`. Popups over a scrollable list – `path_picker`, `finder`, `command_palette`, `help`, `swatches`, `modal::select`, `modal::multi_select` and the `table` modal – gained one; `form` gained a focused-field counter. A frame too narrow for the badge drops it rather than overwriting a corner.
- `pager` and the `markdown` viewer show their scroll percentage in the bottom border; the pager's footer no longer repeats it.
- A boxed `table`'s badge now reads `12/80` (cursor position and row count) instead of the bare row count. Its status line is unchanged.
- `shortcut_hints::height` returns `0` while the hints are hidden (previously at least `1`), and `lines`/`group_lines` return an empty `Vec`, so a host that budgets its footer with `height` reclaims the top margin along with the hints.
- Filter and search lines (`finder`, `command_palette`, `help`, `swatches`, `sidebar`, `table`, `pager`) scroll horizontally and mark a scrolled-off head with `…`, instead of being cut off at the line end. They share one caret renderer rather than each rebuilding the caret span inline.
- Modal frames now fill with a slightly lightened background, lifting the box above the dimmed backdrop so it reads as an elevated surface.
- Form and toast frames now title via the shared `chrome::border_title`, so their leading `─` connector takes the border color (matching modals) instead of the title/accent color.
- `modal::number_input` now falls back to the initial value instead of `0` when the entered text cannot be parsed as an integer.
- `textarea` wraps **word-aware**: a soft break falls on the last space that fits and consumes it, instead of hard-splitting mid-word at the column. A word longer than the width is still hard-split. `TextArea::render` and the new `wrap_offsets` share the one implementation.

### Removed

- The overlay `position/total` chip `list::render` used to draw over its last row. Plain `list::render` no longer shows a count; use `list::render_counted` (a reserved bottom row), `list::render_boxed` with a `BoxDecor`, or let the surrounding popup frame carry the badge.

### Fixed

- A `toast` box grows to its message. Every box was drawn three rows tall ("border + one wrapped line"), so anything longer than the inner width was cut off after the first line - the message wrapped, but the rows were not there. `render` now measures the message with `text::wrap` and sizes the box to it, capped at six lines with a trailing `…`. The wrapped lines are handed to the `Paragraph` directly instead of re-wrapping them with `Wrap`, so measuring and drawing cannot disagree.
- A caret line no longer overflows its width. With both a head and a tail `…` marker, a window of two columns drew three: the markers were taken before the text was given a column. Markers are now dropped, tail first, until the text and its block caret keep at least one column (`input::caret_spans`).
- `AltGr` characters are typed again instead of being swallowed as command chords. `input::apply_edit_key` and `textarea::TextArea::handle_key` tested `KeyModifiers::CONTROL` alone, but crossterm reports `AltGr` as `Control + Alt` – so on a German keyboard `\ @ [ ] { } | ~` never reached the buffer. Both now go through the new `input::is_command` (`Control` *without* `Alt`), which is public so hosts with their own key dispatch can share it.
- Opening a popup in a terminal narrower or shorter than the popup's preferred minimum panicked with `assertion failed: min <= max`. `modal::confirm`, `message`, the list pickers, `input_wide`, `command_palette` and `layout::centered_fraction` reached `Ord::clamp(min, max)` with `max < min`. They now use `layout::fit`, where the available space wins over the preferred minimum. A `confirm` dialog in a 20x6 terminal used to crash the host application.
- `path_picker` shows the block caret in its filter line again. The field is a full `InputField`, but the render path drew only its value, so nothing marked where typing would insert – including on an empty filter.
- The hints toggle compared only the key code, so `Shift+F1` toggled the hints as well. Modifiers are now matched exactly.
- An unboxed `tree` shows its `position/total` counter again. It had vanished with the overlay chip, since a frameless widget had nowhere to put one.

## [0.2.0] - 2026-07-07

### Added

- `markdown` module: a CommonMark renderer (headings, lists, task lists, code blocks, blockquotes, GFM tables and callouts, links, plus a `==highlight==` extension) that produces styled `ratatui` lines. Includes a themeable `StyleSheet` (`Default` plus `StyleSheet::from_skin`), a scrollable `MarkdownView` widget with link navigation, and a blocking `viewer` modal. Backed by the new `pulldown-cmark` dependency.
- `text::wrap` – unicode-width-aware word wrapping (hard-splits over-long words).
- `log` diagnostics for degraded conditions (`warn`/`error`): a failed terminal restore on exit, a missing clipboard tool, an unreadable directory, a `canonicalize` fallback that weakens path confinement, an invalid color override, and an unknown theme name.

### Changed

- Complete rustdoc coverage, enforced crate-wide with `#![warn(missing_docs)]`.

## [0.1.0] - 2026-07-02

Crate-wide cleanup and API consolidation. This release contains breaking changes.

### Changed

- Grouped scroll parameters into `nav::ScrollView` (used by `scroll::render_scrollbar`/`render_hscrollbar` and `nav::keep_visible`) and list parameters into `list::ListView` (`list::render`/`render_boxed`).
- `path_picker::path_picker` now takes a `PathPickerConfig` with an optional `root` that confines navigation (checked via `canonicalize` + `starts_with`).
- Renamed the crate-root command-palette module `palette` → `command_palette` (`PaletteItem` → `CommandItem`), ending the name clash with `theme::palette`.
- Split the `table` widget into a `table/` submodule (model / interaction / render); behavior unchanged.
- `textarea` now reuses the shared single-line edit core from `input` (SSOT).
- Documentation synced with the code and the public API surface tightened.

[Unreleased]: https://github.com/cgroening/rs-ratada/compare/v0.4.0...HEAD
[0.4.0]: https://github.com/cgroening/rs-ratada/compare/v0.3.1...v0.4.0
[0.3.1]: https://github.com/cgroening/rs-ratada/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/cgroening/rs-ratada/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/cgroening/rs-ratada/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/cgroening/rs-ratada/releases/tag/v0.1.0

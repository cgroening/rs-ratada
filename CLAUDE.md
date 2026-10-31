# CLAUDE.md

Guide for working on this repository. `ratada` is a reusable **ratatui widget toolkit** for Rust terminal apps: a generic event-loop driver, widgets, modals, forms, pickers, and a framework-agnostic theming layer on top of a lean core of `ratatui`, `crossterm`, and a few other crates.

## Project Overview

- **Library, not a binary.** `ratada` has no `main`, no CLI, no domain/persistence layer. It provides the generic TUI building blocks on which consuming apps (e.g. `clibase`) build their views.
- **Core idea:** The host implements the `Screen` trait and passes it to `run`, which drives the event loop inside a `Tui` guard (raw mode + alternate screen, RAII). Lifecycle hooks come via `Tui::with_hooks`. The toolkit owns the terminal, navigation, rendering, and modal building blocks; the host owns the application state.
- **No application types.** The modules depend solely on external crates and their own `theme` submodule, never on host types. This keeps the toolkit universally usable.

### Module Layout

- **Crate root (`src/lib.rs`):** declares the widget modules flat (`pub mod modal;`, `pub mod table;`, …) plus `pub mod theme;` and re-exports a small prelude:
  ```rust
  pub use driver::{Flow, Screen, run};
  pub use modal::ModalSignal;
  pub use overlay::{PopupFlow, popup, popup_with_paste};
  pub use terminal::{Tui, TuiEvent};
  ```
  Most widgets are reached via their module path (`ratada::table`, `ratada::modal`, …), not via the prelude.
- **Widget modules (flat in the root):** `terminal`, `driver`, `overlay`, `modal`, `chrome`, `layout`, `nav`, `scroll`, `style`, input/editing (`input`, `textarea`, `autocomplete`, `editor`, `clipboard`), `keymap` (the chord grammar + user-remappable bindings; an app supplies its action table via the `Action` trait, the chords live here once), pickers (`color_picker`, `swatches`, `date_picker`, `date_range_picker`, `month_picker`, `path_picker`, `slider`), display (`table`, `tree`, `list`, `tabs`, `pager`, `gauge`, `spinner`, `toast`, `text`, `markdown`, `sidebar`), as well as `form`, `finder`, `fuzzy`, `help`, `command_palette`, `header`, `shortcut_hints`, `statusbar`, `quit`, `opener`, `theme_preview`, `double_press`. Cross-references between modules go via `super::` (the crate root). There is no `footer` module - the hint footers live in `shortcut_hints`.
- **Split modules:** where a module outgrew one file it became a directory whose `mod.rs` holds the type and whose children hold its `impl` blocks by responsibility (`input/`, `modal/`, `swatches/`, `sidebar/`, `form/`, `path_picker/`, `color_picker/`, `command_palette/`, `keymap/`, `shortcut_hints/`, `textarea/`, `markdown/render/`, `theme/color/`, plus the pre-existing `table/`). A child reaches the crate root with `crate::`, not `super::` - inside a child, `super` is the parent *module*. The public API is unchanged by these splits.
- **`filter_list` (crate-internal):** the query/cursor/scroll state and key dispatch shared by `finder`, `help` and `command_palette`.
- **`week_column` (crate-internal):** the ISO calendar-week column of `date_picker` and `date_range_picker` – the `WeekColumn` setting (re-exported as `date_picker::{WeekColumn, set_week_column, week_column}`) and the drawing both share.
- **`date_entry` (crate-internal):** the typed-entry field `e` opens over the header of `date_picker`, `date_range_picker` and `month_picker`, together with the snapshot a cancelling `Esc` restores and the rendering of that line. Value-agnostic like `filter_list`: each picker keeps its own format, parse and preview.
- **`date_text` (crate-internal):** the typed date forms all three calendar pickers accept – `parse_date`, `parse_month`, `parse_range` over ISO and German notation (`DD.MM.YYYY`, `DD.MM.` without a year, `MM/YYYY`, `MM.YYYY`, two-digit years as `20YY`). Hand-parsed, not via chrono's `%Y`, which takes a one- to three-digit year literally.
- **`theme/` (submodule):** framework-agnostic theming that a UI layer (even a pure CLI) can share: `Color` (+ `parse_color`, and the OKLCH variants `darken`/`lighten`/`vivid`/`dim`/`shade`/`mix`), `Palette` (+ `resolve`, `ColorOverrides`), `Skin` (bundle of palette and glyphs), `Glyphs`/`GlyphVariant`, as well as `ThemeRegistry`/`ThemeColors`/`DEFAULT_THEME` with the built-in themes. `style.rs` is the **only** seam that maps `theme::Color` onto `ratatui::style::Color`.
- **Dependencies:** only `ratatui`, `crossterm`, `unicode-width`, `nucleo-matcher`, `pulldown-cmark` (CommonMark parser for the `markdown` module, `default-features = false`), `chrono`, `log`, `serde` (the latter for the persistable enums `GlyphVariant` etc.), plus `clipboard-win` (Windows-only, the native Win32 clipboard for `clipboard`). Nothing else.

### Commands

```bash
cargo build
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

### Consumers

These crates depend on `ratada` as a path dependency (`path = "../../libs/ratada"`) with a `version` requirement, so a change here reaches all of them:

- `apps/calcli`
- `apps/hop`
- `apps/mdeck`
- `apps/mdtask`
- `apps/numcli`
- `templates/clibase` (the template new apps are created from – a change that is missing here is inherited by every future app)

`apps/pdfc` does not use `ratada`. Re-check the list before relying on it – it goes stale when an app is added:

```shell
cd ~/Developer/Rust && grep -rln --include=Cargo.toml ratada apps libs templates tools samples
```

When a change here affects consumers:

- **Version bump:** raising the minor version (`0.6` → `0.7`) breaks every consumer whose requirement still names the old one – cargo no longer resolves it and the crate stops building. Raise the `version` in each consumer's `ratada` line in the same go.
- **Breaking API or key changes:** check each consumer for uses of the changed items (`grep` for the module or function) – not only compile errors, also hint texts, help rows and README tables that spell out the old keys (as with the swatch picker's `Enter`/`e` in the `clibase` gallery).
- **Verify each consumer in its own directory** (`cd apps/<name> && cargo build && cargo test && cargo clippy --all-targets -- -D warnings`), never via `--manifest-path`, so its `.cargo/config.toml` target dir applies.
- **Changelog:** a consumer whose visible behaviour changes gets its own entry; a pure requirement bump needs none.

This crate is the **SSOT** of the TUI conventions described below under TUI conventions. New widgets and consuming TUIs build on top of it instead of reimplementing them.

## Library conventions

`ratada` is a library with no binary, no argument parsing and no stdout output; rules for command-line programs do not apply here. What follows is specific to this crate.

- **No application types:** the toolkit never depends on host types; variable behavior comes via traits/callbacks from the host (the `Screen` trait, `Tui::with_hooks`). Stateful widgets are types with their own state, stateless helpers (rendering, navigation) are free functions.
- **Public API is a contract:** changes to `pub` signatures are breaking changes – make them deliberately and document them. Keep the public API small; prelude re-exports via `pub use` in `lib.rs`. Well-maintained rustdoc is especially important here.
- **Errors:** own error enums via `thiserror`; **`anyhow` does not belong in the public API.** The `Screen` trait lets the host choose its own error type (`type Error: From<io::Error>`).
- **Target folder:** `.cargo/config.toml` redirects `build.target-dir` to `~/Temp/cargo-target/ratada`, so no build artifacts land in the iCloud-synced project tree. Always invoke cargo from the crate root, or that setting is missed (see the parent `Rust/CLAUDE.md`).
- **Driving a terminal app from a test:** `terminal.rs` owns both halves. `Tui::for_test(w, h)` renders into an in-memory `TestBackend` (no raw mode, no alternate screen, `Drop` restores nothing), and `script_keys` installs a thread-local key queue every reader in this crate draws from, so a modal can be answered instead of blocking on stdin. An installed queue that runs dry returns `UnexpectedEof` and **never falls back to the terminal** – that is what turns an under-fed test into a fast failure rather than a hung run; `scripted_remaining` lets a test prove its answers were consumed at all.
- **`read_raw_event`/`poll_raw_event` are the one seam through which input enters.** Nothing in this crate may call `crossterm::event::read`/`poll` directly, and neither may a consuming app with its own event source: the queue is shared, so a keypress crossing the app's loop and one of our modals stays in order only if both read through here.
- **Security hot spots:** `editor` and the macOS/Linux `clipboard` path call external tools – `std::process::Command` with `.arg()`/`.args()`, never `sh -c` with composed strings (Windows `clipboard` uses the native API). `path_picker` checks paths from outside with `canonicalize()` + `starts_with()`. Terminal geometry (u16/usize conversions) is bounded by the screen size.
- **No CI:** the gates in `CONTRIBUTING.md` are run by hand before a change is proposed.

## TUI conventions

Applies to ratatui-based terminal apps. **This crate implements the following conventions** as reusable building blocks; new widgets and consuming views build on top of it. Points without an addition apply in principle; those marked "(optional)" are proven patterns that are adopted where they fit.

**Lists & Navigation**

- **Navigate cyclically:** Selection lists wrap at both ends via a shared helper (`nav::cycle` or `rem_euclid`), not via `saturating_add/sub`. An empty list yields index 0.
- **Page-wise navigation:** `PageUp`/`PageDown` move by one screen page (visible rows, at least 1); clamped at the edge (not cyclic).
- **Jumps to start/end:** `Home`/`End` jump to the start/end of the list and clamp there. Optionally additionally vim (`g`/`G`, `j`/`k`).
- **Direct jump to a value (optional):** small picker, jumps to the next existing row.
- **Multi-selection (optional):** `Space` toggles; `Shift`+arrow/`PageUp/Down` extend a range from an anchor.

**Scrolling & Scrollbar**

- **Scrollbar on overflow:** vertical scrollbar on the right as soon as the content overflows the viewport, otherwise omitted. Dim style without arrows, shared helper (`scroll::render_scrollbar`); the position number is `total - viewport + 1`.
- **Position indicator `XX/YY` never obscures content:** For a framed widget it sits in the lower border (`chrome::render_badge`), for a borderless one right-aligned in a reserved last line (`chrome::render_corner_badge`, for lists `list::render_counted`). If the area is too low to spare the line, content wins.
- **Scroll offset follows the cursor:** the list scrolls only at the edge, not page-wise per step.
- **Box with its own wrapping:** A widget that wraps its text itself has no column left for a `Scrollbar` over its own `Rect`. It fetches the thumb/track cell per visible row via `scroll::row_indicator` and appends it to the `Line` instead of overdrawing the content.

**Modals & Widgets**

- **Reusable modal widgets:** a shared set – `confirm`, `confirm_default`, `confirm_styled`, `select`, `multi_select`, `number_input`, `message` – not reimplemented per call site. `confirm_styled` is the yes/no dialog for a host that wants its own frame title and a styled body (`StyledQuestion::lines` builds the body for a given width on every draw, so a wrapped body follows a resize).
- **Destructive actions are declined by default:** An ordinary yes/no question goes via `confirm`, where `Enter` confirms. Deletion and other irreversible actions go via `confirm_default` with `Question::declining` – there an accidental `Enter` answers "no". `y`/`n` always answer explicitly, `Esc` declines; the footer hint binds `enter` to the answer it actually gives.
- **Calendar date picker (optional):** shared calendar modal with a uniform look/shortcuts. All three calendar pickers additionally take their value typed: `e` turns the header into a text field (prefilled and fully selected) whose value is validated live - the calendar follows it as soon as it parses, an unparseable one is drawn in the error color and cannot be confirmed with `Enter`, and `Esc` restores the state the field opened over. Besides ISO the field takes German dates (`DD.MM.YYYY`, `DD.MM.` without a year, `MM/YYYY`, `MM.YYYY`, two-digit years as `20YY`), defined once in `date_text`. The field itself lives once in `date_entry`. `date_picker` and `date_range_picker` show the ISO calendar week left of each row (label `Wk`, separator in the border color, cursor week accent bold, today's week between `foreground_dim` and `foreground`); it is on by default and `date_picker::set_week_column(WeekColumn::Hidden)` switches it off for both. The setting and the drawing live once in `week_column`. In all three pickers the cursor cell is filled with the accent (text via `Color::readable_on`, bold) and today is plain accent text – shared through `date_picker::cursor_cell_style` / `today_cell_style`.
- **Fuzzy matching:** filter and selection pickers match fuzzy (`fuzzy`, backed by `nucleo-matcher`).
- **Autocomplete (optional):** `autocomplete::Autocomplete` matches suggestion values by case-insensitive substring and draws them as a floating, borderless menu (`render_popup`, colors via `PopupStyle`) under the field's value, flipped above it when the room below runs out and kept inside the host's bounds; `lines` is the inline alternative for a host that reserves the room. Keys follow blink.cmp: nothing is preselected, `↑`/`↓`, `Ctrl+J`/`Ctrl+K` and `Shift+Tab` move, `Tab` takes the selection or else the first match, `Enter` only an active selection, `←`/`→` close and still move the caret, `Esc` closes just the menu. At most `MAX_VISIBLE` rows show; more scroll, with a solid-cell scrollbar.

**Forms**

- **Structure & control:** all fields visible simultaneously; `Tab`/`BackTab` step (wrapping), `Ctrl+S` saves, `Esc` cancels; the focused row highlighted via background tint.
- **Dirty marker & reset (optional):** changed fields carry `*`; `r` resets the focused field.
- **External editor (optional):** `Ctrl+G` hands the field to `$EDITOR`.
- **Read/pan mode (optional):** `Ctrl`+arrows pan a multiline box.

**Text Fields**

- **Complete editing shortcuts:** single- and multi-line fields share the same set of shortcuts via the shared editing core `input::apply_edit_key` (one caret with an optional selection anchor) – single source (SSOT/DRY). `input::EditMode` selects the geometry: `SingleLine` drives `InputField`, `Multiline { width }` the `TextArea`. The editor core handles only editing keys; the field's control keys (`Esc`, confirming `Enter`, other chords) belong to the caller.
  - **Movement:** arrows character-wise; `Home`/`End`; `Up`/`Down` and `PageUp`/`PageDown` (by one viewport page) only multi-line.
  - **Selection:** `Shift`+movement extends, without `Shift` clears; `Ctrl+A` everything.
  - **Deletion:** `Backspace`/`Delete`; `Ctrl+U`/`Ctrl+K` to the start/end of the line (multi-line: to the start/end of the **display** line, not the logical one).
  - **Clipboard:** `Ctrl+C`/`X`/`V`; typing/pasting replace the selection. A command chord is `Control` **without** `Alt` (`input::is_command`) – otherwise a field swallows the `AltGr` characters that crossterm reports as `Control+Alt`. `Ctrl+V` reads the platform clipboard via `clipboard`: on Windows the native Win32 clipboard (`clipboard-win`), instant and returning correct Unicode; on macOS/Linux the small CLI tools (`pbcopy`/`pbpaste`, `wl-copy`/`xclip`/`xsel`). Umlauts and multi-line order survive.
  - **Bracketed paste:** terminal-native paste (right-click, `Ctrl+Shift+V`) arrives as `TuiEvent::Paste(String)` (newlines normalized to `\n` in `terminal::classify`) and is inserted through the same seam as `Ctrl+V` – `input::paste_text` / `InputField::paste` / `TextArea::paste` (control chars stripped, newlines kept only when multi-line). The driver routes it via `Screen::handle_paste` (default no-op); modals with a text field opt in via `overlay::popup_with_paste` (plain `popup` ignores paste). **Windows caveat:** crossterm's Windows event source emits no `Event::Paste`, so bracketed paste is left disabled there (`terminal::enter_screen`) and a paste comes through the `Ctrl+V` key path instead; only macOS/Linux see `TuiEvent::Paste`.
  - **Rendering:** block cursor (color optional via config); single-line scrolling horizontally with `…` clipping, multi-line wrapped word-wise (soft wrap at the last fitting space, overlong words hard).
  - **Embeddable:** A host that does its text layout itself uses `input::line_spans` / `scrolled_line_spans` / `query_spans_at` along with the `LinePaint` style overlay, instead of reimplementing the caret logic.

**Presentation**

- **Coloring – subtle instead of garish:** a single accent tone (soft RGB) for header/active tab/highlight, `DIM`/gray for secondary text, muted background tints for selection/focus. Colors carry meaning and are held centrally as named constants (in the `theme` submodule), not scattered across widgets.
- **Border style:** boxes/modals with rounded borders (`BorderType::Rounded`).
- **Glyphs/icons – two variants, config-selectable:** each icon in two levels (Unicode + ASCII fallback), selectable via `GlyphVariant`. No colorful emojis.
- **Footer hint line:** active keyboard shortcuts in a footer – `(key, description)` tokens, key in the accent tone, description dim, separated with ` · `, wrapping when the width is too narrow. Shared helper (`footer::lines`).
- **Help overlay:** a full overlay callable via `?` with all shortcuts; scrollable fuzzy finder. The footer points to it with `? help`. When adding a shortcut, keep footer, help overlay, and the docs in sync.
- **Transient status line:** feedback as a short message in the footer (accent color) that disappears on the next keypress. Errors from actions are reported this way and never lead to a crash; only severe cases use a modal.
- **Truncate overflow with `…`:** too-wide text truncated to the visible width (shared `text::truncate` helper).
- **Sticky header line (optional);** column head with unit (optional); tab bar (optional); theming (optional, colors via `theme` with a default fallback).

**Global Keys & App Frame**

- **Global keys:** `Ctrl+Q` quits hard (everywhere, incl. modals, with saving of the session). `F1` toggles **only the host's main-app footer** (the grouped `shortcut_hints::group_lines`/`height`/`render` API; default: on, including the blank line above it). Popup/modal key hints (the flat `shortcut_hints::lines`/`footer_height` API) always show regardless of `F1`, since a modal's key prompt is essential; a popup opts into `F1`-gating by guarding with `shortcut_hints::visible()`. The toolkit itself sets both: `Ctrl+Q` in `terminal::classify`, `F1` in `driver::run` and `overlay::popup` – every screen and every modal inherit them, the host never sees the key. `F1` is rebindable and disableable via `shortcut_hints::set_toggle_key`; `Ctrl+Q` stays hard-wired so that the emergency exit from the alternate screen does not accidentally disappear. `shortcut_hints::global_bindings` names both, with the current binding – the host splices them into the footer and help overlay. An app with its own event loop (instead of `driver::run`) calls `shortcut_hints::consume_toggle(key)` at the very front of its key handling; never compare the chord by hand.
- **Quit prompt (optional):** `quit::set_confirm` determines whether a confirmation is asked before the hard `Ctrl+Q`, before the host's own quit action, before both, or before neither (default: neither). `quit::set_guard` registers how the dialog is drawn. The hard chord the toolkit queries itself; the host asks about its own quit action via `quit::request` – only it knows where it came from.
- `Ctrl+C` remains reserved for the clipboard. Number keys select top-level views (persisted). `u` undoes the last action (one-level undo), `y` copies to the clipboard. `q`/`Esc` quit softly, `?` opens the help. (These the host sets; `q` must not be intercepted by the toolkit – it is an ordinary character and would be input in every filter.)
- **Terminal guard (RAII):** a guard type (`Tui`) activates raw mode + alternate screen on creation and restores both on drop; the event wrapper delivers keys and `Resize`, the surface redraws on resize.
- **Debounced save (optional):** fast, repeated changes bundled and written with a delay.

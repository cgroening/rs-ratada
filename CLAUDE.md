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
- **`theme/` (submodule):** framework-agnostic theming that a UI layer (even a pure CLI) can share: `Color` (+ `parse_color`, and the OKLCH variants `darken`/`lighten`/`vivid`/`dim`/`shade`/`mix`), `Palette` (+ `resolve`, `ColorOverrides`), `Skin` (bundle of palette and glyphs), `Glyphs`/`GlyphVariant`, as well as `ThemeRegistry`/`ThemeColors`/`DEFAULT_THEME` with the built-in themes. `style.rs` is the **only** seam that maps `theme::Color` onto `ratatui::style::Color`.
- **Dependencies:** only `ratatui`, `crossterm`, `unicode-width`, `nucleo-matcher`, `pulldown-cmark` (CommonMark parser for the `markdown` module, `default-features = false`), `chrono`, `log`, `serde` (the latter for the persistable enums `GlyphVariant` etc.), plus `clipboard-win` (Windows-only, the native Win32 clipboard for `clipboard`). Nothing else.

### Commands

```bash
cargo build
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

This crate is the **SSOT** of the TUI conventions described below in §7.10. New widgets and consuming TUIs build on top of it instead of reimplementing them.

---

The following style guide is binding. In case of conflicts, more specific (language-related) rules take precedence over the general ones. These documented rules take precedence over automatic formatters/linters.

Note on provenance: the sections below restate the global style guide (`KI-Anweisungen/01 Softwareentwicklungsstandards.md`) with this crate's own numbering, plus refinements specific to `ratada`. The global guide is the SSOT - where the two disagree, it wins, and this file is the one to correct. Its CLI sections do not apply here: `ratada` is a library with no binary, no argument parsing and no stdout output.

## 1 Clean Code / Design Principles & Patterns

The top priority is readable, maintainable code – comprehensibility takes precedence over brevity or cleverness when in doubt. Equally ranked: robustness and security (§2.7).

### 1.1 Simplicity & Repetition

- **KISS / YAGNI** – no speculative abstraction or configurability "for later".
- **DRY** (code/logic) and **SSOT** (data/knowledge source).
- **Consistency:** Solve similar things similarly.
- **No magic numbers/strings:** named constants.

### 1.2 Functions

- **SLAP** – one level of abstraction per function.
- **At most two levels of nesting** (with early return).
- **No flag arguments:** instead of a boolean parameter, two functions with descriptive names.
- **Command-query separation;** prefer pure functions.

### 1.3 OO & Design

- **Polymorphism instead of type branching.**
- **Composition over inheritance;** inheritance only for a true "is-a" relationship (LSP).
- **Tell, Don't Ask / Law of Demeter.**
- **SOLID** (DIP via dependency injection).
- **High cohesion, loose coupling.**
- **Design patterns (GoF):** apply where they solve a real problem – never as an end in itself, KISS/YAGNI take precedence.

### 1.4 Code Smells

- **Recognize code smells & eliminate them via refactoring** (Long Method, Duplicate Code, Feature Envy, Primitive Obsession, …). A smell is a hint, not an automatism. Procedure see §3.

### 1.5 Names

- **Booleans/predicates** as a yes/no question: `is_`, `has_`, `can_`, `should_`.
- **Methods = verbs, classes/types = nouns** – no catch-all names like `Manager`, `Data`, `Helper`.

### 1.6 Error Handling

- **Fail fast:** catch invalid states/inputs as early as possible.
- **Do not return `None`/`null`:** instead an empty collection, special-case object, or exception.
- **Exceptions with context** (what, where, why) – language-specific see the respective section.

### 1.7 Tests & Performance

- **Tests per FIRST;** one concept per test.
- **Measure first, then optimize:** optimization only with profiling proof.

## 2 General Rules (all languages)

### 2.1 Formatting & Tools

- **Indentation:** 4 spaces as standard.
- **Line length:** At most 80 characters. Break longer lines readably. Applies to code files (`.rs`, …), not to text files (`.txt`, `.md`).
- **Markdown body text:** Do NOT hard-wrap paragraphs and list items – each paragraph and list item is on exactly one line (use editor soft-wrap). Blank lines, list structure, headings, and code blocks are preserved.
- **Line breaks:** Break long expressions/calls readably – operators at the beginning of the line, with many arguments one per line; indent continuation lines consistently.
- **Signature & call breaking:** If a signature/call does not fit in 80 characters, first put all parameters/arguments on one indented line between the parentheses; if that is not enough, one per line. Fully fanned-out signatures are an indicator of too many parameters – then group (struct).
- **Whitespace & file hygiene:** Only spaces (no tabs); no trailing whitespace; file ends with exactly one newline; UTF-8; line endings LF.
- **Numeric literals:** Digit separators for large numbers (`1_000_000`); hex in lowercase (`0xff`).
- **Trailing commas:** In multi-line lists, argument lists, enums, and initializers a trailing comma; not single-line. (rustfmt enforces this.)
- **Alignment:** Column alignment with additional spaces is allowed for better readability.
- **Hyphens / dash:** Never the em dash "—". In code only the minus sign ("-"). In Markdown body text use the dash "–" as a dash, not a hyphen surrounded by spaces. Hyphens in compound words remain hyphens.
- **Quotation marks:** Always straight quotation marks "…" – never typographic ones.
- **Precedence over tools:** These rules take precedence over rustfmt/clippy.
- **Portability/versions:** Prefer the latest standards.

### 2.2 Comments & Language

- **Comments:** Moderate. Functions and important logic blocks are commented, not every line. A comment does not repeat the identifier – it explains what the reader cannot derive from the name (semantics, units, sentinel values, invariants, rationale). Comments explain above all the **why**, not the what.
- **TODO comments:** Format `// TODO: <text>`.
- **Language:** Throughout English – identifiers, comments, docstrings, and visible texts (errors, logs, TUI outputs).
- **Single-line comments:** Allowed after a statement at ≤ 80 characters; then two spaces before `//`. If the comment consists of a single sentence, no period at the end.

### 2.3 Naming

- **Naming:** `snake_case` for variables and functions; `UPPER_CASE` for module/class constants; `PascalCase` for types.
- **File/module names:** snake_case.
- **Meaningful variable names:** The purpose is evident from the name. No cryptic abbreviations. Exception: the counter variable of a single, non-nested loop may be called `i`.
- **Acronyms in identifiers:** like normal words (`UserId`, `HttpClient`, `parse_url`) – not `UserID`/`HTTPClient`.
- **No negative booleans:** name positively; avoid double negation.

### 2.4 Types & Data

- **Truth values:** use `bool` – no `int` flags.
- **Strong typing:** the domain explicitly typed – `enum`s instead of magic numbers/strings, structs instead of loose primitives.
- **Immutability:** Keep immutable where sensible; do not mutate function inputs.

### 2.5 Functions & Control Flow

- **Function length:** Keep small (single responsibility). Decompose larger functions.
- **Function parameters:** Keep the count low (ideally ≤ 3). Group related parameters in a struct. Situational values remain explicit parameters, not as transient state in the object.
- **Explicit passing:** With multiple values pass by name instead of positionally.
- **Control flow:** Early return (guard clauses) instead of deep nesting.
- **Reading order:** Code readable from top to bottom – extracted helper functions appear below their callers.
- **Declaration order:** public before private members; group related members.

### 2.6 Architecture & Dependencies

- **Programming style:** Object-oriented for stateful components (widgets with their own state); free functions for stateless utilities (rendering helpers, navigation).
- **Layered architecture:** Separate responsibilities; depend on abstractions instead of concretions (DIP). As a library, `ratada` never depends on application types; variable behavior comes via traits/callbacks from the host (e.g. the `Screen` trait, `Tui::with_hooks`).
- **External libraries:** Prefer built-in means and the standard library, minimize dependencies. External libraries may be proposed – but ask first before introducing a dependency.

### 2.7 Robustness, Errors & Logging

- **Robustness & security:** As robust against crashes as possible and as secure as possible. Program defensively: validate inputs, safeguard edge cases, check return values/errors. Better to fail in a controlled way than to crash or silently continue incorrectly.
- **Logging:** For diagnostics, structured logging via the logging framework (`log`) instead of direct console output (`println!`); choose log levels sensibly. Visible TUI outputs are not logging.

### 2.8 Tests

- **Tests:** Always shipped along. Prefer fakes over mocks; group related tests and let test names describe the expected behavior.

### 2.9 Proportionality: small and personal scripts

For individual functions, small scripts, or code only for personal use, the scope may be reduced – but never silently:

- **Consult with a proposal:** Before omitting security aspects or tests, ask – with a concrete proposal (what is dropped, what stays, risk in one sentence each).
- **Never negotiable:** no `eval`/`exec` on foreign inputs, no shell/command injection, no secrets in code, no destructive operations on unvalidated paths.

## 3 Maintenance / Refactoring / Code Adjustments

- **Respect local style:** When changing existing code, adopt the existing style/idioms; do not push through the style guide halfway in the middle of a file (consistency over "my style"). Exception: explicitly commissioned refactoring. With large deviations (architecture, tooling, …) no changes without prior agreement.
- **Minimal, focused changes:** Only touch what the task requires; do not reformat unrelated code, do not expand the scope.
- **Separate refactoring from behavior:** Pure refactoring does not change behavior; behavior changes are a separate step. Boy Scout rule.
- **Bring callers & tests along:** For signature/behavior changes, adjust all call sites, update/add tests and run them. As a library with a public API: changes to `pub` signatures are breaking changes – make them deliberately and documented.
- **Keep comments/docs current:** For renames/changes, bring comments and doc blocks along.
- **Leave no corpses behind:** Remove dead and commented-out code.
- **Cause instead of symptom:** Fix bugs at the root.
- **Documentation:** With every change, check whether documentation (README.md, rustdoc, …) needs to be adjusted, and do it.
- **Tests:** With every change, check whether tests need to be adjusted/added. **After every change re-run all tests and ensure that all pass.**

## 7 Rust

### 7.1 Toolchain & Standard

- **Edition:** 2024 (latest stable). `rust-version` only with a concrete MSRV.
- **Formatting:** rustfmt with default settings. Enable import grouping via `group_imports = "StdExternalCrate"` and `imports_granularity` where a `rustfmt.toml` is present.
- **Linting:** clippy must run warning-free (`cargo clippy -- -D warnings`). `clippy::pedantic` optional, project-wide via `#![warn(...)]`, not through scattered `#[allow]`.
- **Logging:** `tracing` or `log` (with an implementation such as `env_logger`) for diagnostics instead of `println!`/`eprintln!`. Agree on the crate beforehand.
- **Precedence over tools:** These rules take precedence over rustfmt/clippy.

### 7.2 Project Structure & Architecture

- **Module declaration:** Submodules via `mod` declarations in `lib.rs` or the parent file; file names `snake_case`. Cross-references between the widget modules lying flat in the root via `super::`.
- **Dependency injection:** Abstract variable behavior via traits; inject the implementation via generic (`fn f<T: Screen>(…)`) or `dyn Trait`/`Box<dyn Trait>`. The host brings in its behavior via the `Screen` trait and lifecycle hooks.
- **Target folder:** `.cargo/config.toml` redirects `build.target-dir` to `~/Temp/cargo-target/ratada`, so no build artifacts land in the iCloud-synced project tree. Always invoke cargo from the crate root, or that setting is missed (see the parent `Rust/CLAUDE.md`).

### 7.3 Error Handling

- **`Result` + `?`:** Propagate errors via `Result<T, E>` and `?`; no string errors as a permanent pattern.
- **`thiserror` for libraries:** Own error enums with `#[derive(thiserror::Error)]` and descriptive `#[error("…")]` messages; one error type per module/domain. Foreign errors via `#[from]`. `ratada` is a library – **`anyhow` does not belong in the public API.** The `Screen` trait lets the host choose its own error type (`type Error: From<io::Error>`).
- **`unwrap()` forbidden; `expect()` only in provably infallible places.** Every `expect("…")` justifies why it cannot fail. In the normal flow `?`.
- **No `panic!` in the normal flow:** only for real programming errors/invariants.
- **`unsafe`:** Avoid. If unavoidable, ask first, encapsulate, justify with `// SAFETY: …`.

### 7.4 Documentation (rustdoc)

- **`///` doc comments** above every public item. First line a concise one-sentence summary. As a library with a public API, well-maintained rustdoc is especially important.
- **Idiomatic rustdoc, no `# Arguments` lists:** parameters/return in prose. Standard sections where applicable: `# Examples` (with a runnable doctest, if not obvious), `# Errors`, `# Panics`, `# Safety`.
- **Identifiers in doc comments** in `` `inline code ``; intra-doc links where sensible.
- **Module doc:** Every module a `//!` doc comment at the top with a short description.
- **Private items:** A short single-line `///` comment suffices.

### 7.5 Types & Idioms

- **Strong typing:** `enum`s for states/variants instead of magic strings; `struct`s instead of loose tuples. Consider newtypes for domain values.
- **Derivations:** Derive sensible traits (`Debug, Clone, PartialEq, …`); `Serialize`/`Deserialize` via serde-derive. `Copy` only for small types.
- **Constructors:** `pub fn new() -> Self`; for default constructors additionally `impl Default`. Do not maintain `new` without arguments redundantly to `Default`.
- **Ownership:** Prefer borrows (`&T`/`&mut T`); avoid unnecessary `.clone()`/`.to_string()`. Do not mutate inputs unnecessarily.
- **Optionality:** `Option<T>` for "may be absent"; no sentinel value.
- **Control flow:** `match`/`if let` with guard clauses; avoid deep nesting.
- **Iterators instead of manual loops** for simple map/filter/fold; with complex logic an explicit `for` loop.
- **Visibility:** As private as possible; keep the public API small (important for a library). Prelude re-exports via `pub use` in `lib.rs`.

### 7.6 Concurrency

- **Synchronous as default,** as long as there is no real need (KISS/YAGNI).
- **`async`/`await` only with a real I/O concurrency need,** then with a runtime (`tokio`); agree on the runtime beforehand.
- **No blocking calls in the `async` context;** CPU-intensive work via `spawn_blocking`/dedicated threads.
- **Shared state** preferably via ownership/channels instead of shared locks; only where necessary `Arc<Mutex<…>>`.

### 7.7 External Crates

- Prefer the standard library, minimize dependencies – agree on new crates beforehand. Document established crates above the `use` or in `Cargo.toml`: `// https://crates.io/crates/<name>`.

### 7.8 Tests

- **Unit tests** in the respective file under `#[cfg(test)] mod tests { … }` with `use super::*;`; **integration tests** in the `tests/` directory.
- Test functions `#[test]`, names describe the expected behavior; error cases where appropriate with `#[should_panic]` or `Result` return. FIRST and fakes-over-mocks apply.
- Doctests in `# Examples` count as tests and must run.
- **Driving a terminal app from a test:** `terminal.rs` owns both halves. `Tui::for_test(w, h)` renders into an in-memory `TestBackend` (no raw mode, no alternate screen, `Drop` restores nothing), and `script_keys` installs a thread-local key queue every reader in this crate draws from, so a modal can be answered instead of blocking on stdin. An installed queue that runs dry returns `UnexpectedEof` and **never falls back to the terminal** – that is what turns an under-fed test into a fast failure rather than a hung run; `scripted_remaining` lets a test prove its answers were consumed at all.
- **`read_raw_event`/`poll_raw_event` are the one seam through which input enters.** Nothing in this crate may call `crossterm::event::read`/`poll` directly, and neither may a consuming app with its own event source: the queue is shared, so a keypress crossing the app's loop and one of our modals stays in order only if both read through here.

### 7.9 Security

- **`unsafe` discipline:** avoid, encapsulate, `// SAFETY:` + `# Safety` doc.
- **Integer overflow:** for values from outside explicitly `checked_*`/`saturating_*`/`wrapping_*`. Terminal geometry (u16/usize conversions) is bounded by the screen size.
- **Command injection:** `std::process::Command` with `.arg()`/`.args()`; no `sh -c` with composed strings (relevant for `editor` and the macOS/Linux `clipboard` path, which call external tools; Windows `clipboard` uses the native API).
- **Path traversal:** Check paths from outside with `canonicalize()` + `starts_with()` (relevant for `path_picker`).
- **Secrets:** no secrets in code/log; if needed `zeroize`.
- **Limit inputs:** size/length limits when parsing foreign data.
- **Dependencies:** minimize dependencies (§7.7). There is no CI in this repository; the gates in `CONTRIBUTING.md` are run by hand before a change is proposed.

### 7.10 TUI Conventions (Rust terminal apps)

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

- **Reusable modal widgets:** a shared set – `confirm`, `confirm_default`, `select`, `multi_select`, `number_input`, `message` – not reimplemented per call site.
- **Destructive actions are declined by default:** An ordinary yes/no question goes via `confirm`, where `Enter` confirms. Deletion and other irreversible actions go via `confirm_default` with `Question::declining` – there an accidental `Enter` answers "no". `y`/`n` always answer explicitly, `Esc` declines; the footer hint binds `enter` to the answer it actually gives.
- **Calendar date picker (optional):** shared calendar modal with a uniform look/shortcuts.
- **Fuzzy matching:** filter and selection pickers match fuzzy (`fuzzy`, backed by `nucleo-matcher`).
- **Autocomplete dropdown (optional):** inline dropdown for suggestion values.

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

### 7.11 Miscellaneous

- The remaining general rules (naming, line length, comments, robustness) apply unchanged.

## 11 Git

**DO NOT MAKE YOUR OWN COMMITS.**

At the end of changes, make a proposal for a commit message (title only) – in English, in the imperative (e.g. "add X", "update Y"), unless something else is specified. Use the style of [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/).

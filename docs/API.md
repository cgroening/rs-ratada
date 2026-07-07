# API overview

A compact map of `ratada`'s public surface. The authoritative reference is the
rustdoc (`cargo doc --open`); this file is a quick index. Every widget takes a
`&theme::Skin` (or `&theme::Palette`) for styling and never depends on host
types.

## Prelude

`use ratada::prelude::*;` brings in the driver essentials plus `BoxDecor`:
`Tui`, `TuiEvent`, `Screen`, `Flow`, `run`, `ModalSignal`, `PopupFlow`, `popup`,
`BoxDecor`.

## Driver & terminal

- `trait Screen { type Error: From<io::Error>; fn render(&self, &mut Frame); fn handle_key(&mut self, KeyEvent, &mut Tui) -> Result<Flow, Error>; fn tick(&mut self) {} }`
- `enum Flow { Continue, Quit }`
- `fn run<S: Screen>(&mut Tui, &mut S) -> Result<(), S::Error>` — draw/read/handle loop with an idle tick.
- `struct Tui` — RAII raw-mode + alternate-screen guard.
  - `Tui::new() -> io::Result<Self>`, `with_hooks(on_enter, on_leave)`,
    `draw(|frame| ...)`, `read_event()`, `poll_event(timeout)`, `suspend(action)`.
- `enum TuiEvent { Key(KeyEvent), Resize, Quit }`

## Theme (`ratada::theme`)

- `enum Color { Default, Rgb(u8,u8,u8) }`; constructors `Color::hex("#rrggbb")` (const, compile-time checked) and `parse_color` (`#rgb`/`#rrggbb`/`rgb(r,g,b)`/named, runtime); `Color::{rgb, to_hex, darken, lighten, vivid, dim, shade, mix, luminance, readable_on, distance}` — OKLCH-based variants (hue-stable); model conversions `Color::{to_hsl, from_hsl, to_oklch, from_oklch}`.
- `struct Palette` (19 colors: accent, accent_dim, accent_vivid, foreground, foreground_dim, background, header, footer, panel, surface, selection, cursor, input_bg, input_bg_active, border, success, warning, error, info) + `Palette::resolve(ThemeColors, &ColorOverrides)`, `Palette::entries() -> Vec<(name, Color)>`, `Palette::KEYS`. The whole set is declared once via a `define_palette!` macro (SSOT).
- `struct ColorOverrides<'a>` — per-color override strings; `ColorOverrides::from_lookup(|name| ...)` builds it from a `name -> value` lookup.
- `struct Skin { palette, glyphs }` + `new(palette, glyphs)` — the visual context every widget takes.
- `enum GlyphVariant { Unicode, Ascii }`, `struct Glyphs` + `Glyphs::new(variant)`.
- `struct ThemeColors` (12 base colors; `derived(accent, foreground, background)`, `from_accent`, `from_lookup`), `struct ThemeRegistry` (`builtin`, `with_custom`, `get`, `resolve`, `contains`, `names`, `next`), `const DEFAULT_THEME`. Built-ins: `default`, `monochrome`.

## Boxed decoration (`chrome`)

- `struct BoxDecor` + `new`, `caption(text)`, `badge(text)`, `no_badge` — caption in the top border, badge bottom-right.
- `enum Badge { Auto, Text(String), Hidden }`.
- `fn framed_decor(&mut Frame, Rect, &Skin, &BoxDecor, auto_badge) -> Rect` — the shared box seam; returns the inner content area.
- `fn panel`, `menu_panel`, `modal_block` — view/section/modal frames per `Mode`.

## Input & editing

- `struct InputField` (single-line) — `new`, `max_len(n)`, `boxed(decor)`, `handle_key`, `value`, `render_line(&Palette, width, focused) -> Line`, `render(&mut Frame, Rect, &Skin, focused)`.
- `struct TextArea` (multi-line) — `new`, `max_len`, `boxed`, `text`, `set_text`, `handle_key`, `render(&mut Frame, Rect, &Skin, focused)`. Reuses the single-line edit core from `input` (SSOT), layered with wrapped multi-line navigation.
- `struct TextCursor { pos, anchor }` + `at_end`, `selection` (the shared caret; the edit primitives are crate-internal, unicode-width aware).
- `struct Autocomplete` — `new(candidates)`, `refresh(query)`, `is_open`, `on_key -> AcOutcome`, `lines(&Palette, indent, base)`; `enum AcOutcome { Accepted, Navigated, Closed, Ignored }`.
- `editor::{resolve_editor, edit_in_editor}`, `clipboard::{copy, paste}`.

## Data display

- `struct Table` — `new(columns, rows)`, `with_select_mode`, `with_filter_scope`, `with_status`, `with_header_style`, `boxed(decor)`, `handle_key -> TableAction`, `cursor_row`, `cursor_cell`, `selected_rows`, `selected_keys`, `selected_cells`, `is_filtering`, `render(&mut Frame, Rect, &Skin)`. (Implemented across the `table` submodule: `model`, `interaction`, `render`.)
  - `struct Column` (`text`/`number`/`date`, `widths`, `align`, `wrap`, `header_style`, `cell_style`), `struct Row` (`new`, `with_style`, `with_key`).
  - `enum ColumnKind`, `Align`, `SelectMode`, `SortDir`, `FilterScope`, `TableAction`; `fn table_select(...)` modal.
- `struct TreeView` — `new(roots)`, `boxed(decor)`, `selected_label`, `handle_key`, `render`; `struct TreeItem` (`leaf`, `node`).
- `struct Sidebar` — sectioned menu column: `new(sections)`, `overflow`, `filterable`, `handle_key -> SidebarOutcome`, `render`; `struct SidebarSection`/`SidebarItem`, `enum Overflow`, `enum SidebarOutcome`.
- `struct ListView<'a> { rows: Vec<Line>, selected, offset: &Cell<usize> }`; `list::render(&mut Frame, Rect, &Skin, ListView)` and `list::render_boxed(..., ListView, &BoxDecor, force)` — selectable list with a scrollbar.
- `tabs::{render, height}`, `pager::pager(...)`, `gauge::render`, `struct Spinner` (`new`, `advance`, `frame`), `struct Toasts` (`new`, `push`, `push_with_ttl`, `prune`, `is_empty`, `render`) + `enum ToastKind`.

## Pickers & modals (blocking, over a background)

Each opens over a caller-supplied `render_bg` and returns a `ModalSignal<T>`
(`Value(T)` / `Cancelled` / `Quit`).

- `modal::{confirm, input, select, multi_select, select_reorderable, select_styled, multi_select_styled, number_input, message}`; `enum ModalSignal<T>`, `enum ListAction`.
- `struct Form` — `new(title, fields)`, `fields`, `run(&mut Tui, &Skin, render_bg) -> FormOutcome`; `struct Field` (`text`/`multiline`/`checkbox`/`choice`/`date`, `label`, `value`, `is_dirty`); `enum FieldValue`, `FormOutcome`.
- `swatches::{swatch_picker, color_chooser}` (multi-mode swatch/color picker; `enum Start`; `NAMED_COLORS` table), `finder::{finder, filter}` (fuzzy pick), `color_picker::color_picker` (returns `enum ColorExit`), `date_picker::date_picker`, `date_range_picker::date_range_picker`, `month_picker::month_picker`, `path_picker::path_picker(&mut Tui, &Skin, PathPickerConfig, render_bg)` (`struct PathPickerConfig { title, start, allow_files, root }`; `Ctrl+H` toggles hidden; an optional `root` confines navigation), `slider::slider` (+ `SliderConfig`).

## Overlays & chrome

- `help::show(&mut Tui, &Skin, &[HelpSection], render_bg)` — sectioned, fuzzy help overlay; `Tab`/`Shift+Tab` jump sections. `struct HelpSection<'a, B> { title, bindings }`.
- `command_palette::command_palette(&mut Tui, &Skin, title, &[CommandItem], render_bg) -> ModalSignal<usize>` — fuzzy command palette; grouped when empty, ranked while searching, `Enter` runs the highlighted command. `struct CommandItem<'a> { label, category, key_hint, enabled }` (disabled items render dimmed and are not selectable).
- `overlay::{popup, framed, dim}`, `enum PopupFlow`.
- `header::render`, `statusbar::render`, `struct DoublePress` (`new`, `register`).
- `shortcut_hints::{lines, group_lines, height, render}` — flat or grouped, label-aligned, wrapping key hints. `struct HintGroup<'a, S> { label, hints }` (empty label = flat), `struct HintStyle { label, key, description, top_margin, background }` (a `Style` per part; `Default` = dim labels/descriptions, bold keys).

## Markdown (`ratada::markdown`)

Renders CommonMark (plus strikethrough, task lists, GFM tables/callouts and a
`==highlight==` extension) into styled `ratatui` primitives. The engine takes a
`StyleSheet` and never depends on host types.

- `markdown::render_block(src, width, &StyleSheet) -> Vec<Line>` — wrapped,
  decorated block layout; `render_inline(src, width, &StyleSheet) -> Vec<Span>` —
  inline-only, clipped; `measure_block(src, width, &StyleSheet) -> usize`.
- `markdown::links(src) -> Vec<Link>` (`struct Link { text, url }`);
  `style_overlay(src, &StyleSheet) -> Vec<Style>` (per-char, for edit surfaces);
  `clip_spans(spans, max, ellipsis) -> Vec<Span>`.
- `struct StyleSheet { base, headings[6], strong, emphasis, strikethrough,
  inline_code, code_block, quote, highlight, link, rule, bullet, checkbox,
  callout, table_border, smart_punctuation, html, ellipsis }` (sub-structs
  `HeadingStyle`/`CodeBlockStyle`/`QuoteStyle`/`BulletStyle`/`CheckboxStyle`/
  `RuleStyle`/`CalloutStyle`/`CalloutTheme`). `StyleSheet::default()` is the
  built-in look; `StyleSheet::from_skin(&Skin)` keeps it but swaps ASCII glyph
  fallbacks for an ASCII skin.
- `struct MarkdownView` — scrollable inline widget: `new(src)`, `boxed(decor)`,
  `with_stylesheet(sheet)`, `set_source`, `links()`, `selected_link()`,
  `handle_key -> bool` (scroll + `Tab` link cycle), `render(&mut Frame, Rect,
  &Skin)`.
- `markdown::viewer(&mut Tui, &Skin, title, src, render_bg) -> ModalSignal<Link>`
  — blocking viewer; `Enter`/`o` returns the highlighted link, `Esc` cancels.

## Utilities

- `nav::{cycle, step_clamped, keep_visible}` — wrapping/clamped/scroll-offset helpers; `struct ScrollView { total, offset, viewport }` groups a scroll window.
- `scroll::{render_scrollbar, render_hscrollbar}(&mut Frame, Rect, &Skin, ScrollView)` — draws only on overflow, skips empty areas.
- `style::{to_ratatui, fg, bg, base, dim, darken}` — theme→ratatui adapter.
- `style` semantic roles (take `&Palette`): `primary, secondary, title, accent, accent_dim, accent_vivid, key, selected, cursor, border, disabled, success, warning, error, info` — the single source for how each UI part is colored.
- `theme_preview::render(&mut Frame, Rect, &Skin)` — draws every palette color as a labeled swatch (hex printed on it via `readable_on`) plus the accent variant ladder; drop into a gallery to verify a theme.
- `text::{truncate, window, pad_end, wrap}` — unicode-width aware clip (appends
  `…`), horizontal-scroll slice, trailing-space pad, and word-wrap (`Vec<String>`,
  hard-splits over-long words).
- `layout::{centered_rect(width, height, area), centered_fraction(area, num, den, min_w, min_h)}` — the shared centered-popup sizing.
- `fuzzy::{score, match_indices, highlight}` — nucleo-backed matching.

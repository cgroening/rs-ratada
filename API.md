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

- `enum Color { Default, Rgb(u8,u8,u8) }` + `parse_color`, `dim_color`, `lighten`; `Color::rgb()`, `Color::to_hex()`.
- `struct Palette { accent, accent_dim, accent_dark, selection_bg, cursor, background, surface, surface_alt, surface_bar, input_bg, input_bg_active, error, warning, success, info }` + `Palette::resolve(ThemeColors, &ColorOverrides)`.
- `struct ColorOverrides<'a>` — per-color config override strings.
- `struct Skin { palette, glyphs, mode }` + `new`, `is_fancy`, `is_panels`.
- `enum GlyphVariant { Unicode, Ascii }`, `struct Glyphs` + `Glyphs::new(variant)`.
- `enum Mode { Minimal, Fancy, Panels }` + `ALL`, `next`, `is_fancy`, `is_panels`, `label`.
- `struct ThemeColors` (+ `new`/`with_semantics`/`with_surfaces`), `struct Surfaces`, `derive_surfaces`, `struct ThemeRegistry` (`builtin`, `with_custom`, `get`, `resolve`, `contains`, `names`, `next`), `const DEFAULT_THEME`.

## Boxed decoration (`chrome`)

- `struct BoxDecor` + `new`, `caption(text)`, `badge(text)`, `no_badge` — caption in the top border, badge bottom-right.
- `enum Badge { Auto, Text(String), Hidden }`.
- `fn framed_decor(&mut Frame, Rect, &Skin, &BoxDecor, auto_badge) -> Rect` — the shared box seam; returns the inner content area.
- `fn panel`, `menu_panel`, `modal_block` — view/section/modal frames per `Mode`.

## Input & editing

- `struct InputField` (single-line) — `new`, `max_len(n)`, `boxed(decor)`, `boxed_always(decor)`, `minimal()`, `handle_key`, `value`, `render_line(&Palette, width, focused) -> Line`, `render(&mut Frame, Rect, &Skin, focused)`.
- `struct TextArea` (multi-line) — `new`, `max_len`, `boxed`/`boxed_always`/`minimal`, `text`, `set_text`, `handle_key`, `render(&mut Frame, Rect, &Skin, focused)`.
- `struct TextCursor { pos, anchor }` + `at_end`, `selection`; free `apply_edit_key`, `render_line` (shared edit core; widths are unicode-aware).
- `struct Autocomplete` — `new(candidates)`, `refresh(query)`, `is_open`, `on_key -> AcOutcome`, `lines(&Palette, indent, base)`; `enum AcOutcome { Accepted, Navigated, Closed, Ignored }`.
- `editor::{resolve_editor, edit_in_editor}`, `clipboard::{copy, paste}`.

## Data display

- `struct Table` — `new(columns, rows)`, `with_select_mode`, `with_filter_scope`, `with_status`, `with_header_style`, `boxed`/`boxed_always`/`minimal`, `handle_key -> TableAction`, `cursor_row`, `cursor_cell`, `selected_rows`, `selected_keys`, `selected_cells`, `is_filtering`, `render(&mut Frame, Rect, &Skin)`.
  - `struct Column` (`text`/`number`/`date`, `widths`, `align`, `wrap`, `header_style`, `cell_style`), `struct Row` (`new`, `with_style`, `with_key`).
  - `enum ColumnKind`, `Align`, `SelectMode`, `SortDir`, `FilterScope`, `TableAction`; `fn allocate_columns`; `fn table_select(...)` modal.
- `struct TreeView` — `new(roots)`, `boxed`/`boxed_always`/`minimal`, `selected_label`, `handle_key`, `render`; `struct TreeItem` (`leaf`, `node`).
- `list::render(&mut Frame, Rect, &Skin, Vec<Line>, selected, &Cell<usize>)` and `list::render_boxed(..., &BoxDecor, force)` — selectable list with a scrollbar.
- `tabs::render`, `pager::pager(...)`, `gauge::render`, `struct Spinner` (`new`, `advance`, `frame`), `struct Toasts` (`new`, `push`, `push_with_ttl`, `prune`, `is_empty`, `render`) + `enum ToastKind`.

## Pickers & modals (blocking, over a background)

Each opens over a caller-supplied `render_bg` and returns a `ModalSignal<T>`
(`Value(T)` / `Cancelled` / `Quit`).

- `modal::{confirm, input, select, multi_select, select_reorderable, select_styled, multi_select_styled, number_input, message}`; `enum ModalSignal<T>`, `enum ListAction`.
- `struct Form` — `new(title, fields)`, `fields`, `run(&mut Tui, &Skin, render_bg) -> FormOutcome`; `struct Field` (`text`/`multiline`/`checkbox`/`choice`/`date`, `label`, `value`, `is_dirty`); `enum FieldValue`, `FormOutcome`.
- `finder::finder(...)` (fuzzy pick), `color_picker::color_picker`, `date_picker::date_picker`, `date_range_picker::date_range_picker`, `month_picker::month_picker`, `path_picker::path_picker` (`Ctrl+H` toggles hidden), `slider::slider` (+ `SliderConfig`).

## Overlays & chrome

- `help::show(&mut Tui, &Skin, &[HelpSection], render_bg)` — sectioned, fuzzy help overlay; `Tab`/`Shift+Tab` jump sections. `struct HelpSection<'a, B> { title, bindings }`.
- `overlay::{popup, framed, box_width, dim}`, `const SCRIM_FACTOR`, `enum PopupFlow`.
- `header::render`, `footer::{lines, height, render}`, `statusbar::render`, `struct DoublePress` (`new`, `register`).

## Utilities

- `nav::{cycle, step_clamped, keep_visible}` — wrapping/clamped/scroll-offset helpers.
- `scroll::render_scrollbar(&mut Frame, Rect, total, offset, viewport)` — draws only on overflow, skips empty areas.
- `style::{to_ratatui, fg, bg, dim, darken}` — the theme→ratatui adapter.
- `text::truncate(text, width)` — unicode-width aware, appends `…`.
- `layout::centered_rect(width, height, area)`.
- `fuzzy::{score, match_indices, highlight}` — nucleo-backed matching.

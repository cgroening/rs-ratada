# ratada

[![crates.io](https://img.shields.io/crates/v/ratada.svg)](https://crates.io/crates/ratada)
[![docs.rs](https://img.shields.io/docsrs/ratada)](https://docs.rs/ratada)
[![license](https://img.shields.io/crates/l/ratada.svg)](https://github.com/cgroening/rs-ratada/blob/main/LICENSE)
![MSRV](https://img.shields.io/badge/rustc-1.88+-blue.svg)

A reusable [ratatui](https://ratatui.rs) widget toolkit for Rust terminal apps.

`ratada` provides the generic building blocks for a TUI over `ratatui` and
`crossterm`: a terminal guard and event loop, modals, forms, text editing,
pickers, tables, trees, a fuzzy finder, a help overlay, a command palette,
footers and status bars, plus a framework-agnostic theming layer (colors,
palette and glyphs) that maps onto ratatui styles.

![ratada widget gallery](https://raw.githubusercontent.com/cgroening/rs-ratada/main/images/screenshot.png)

## Overview

- **Driver** – a `Tui` RAII terminal guard (raw mode + alternate screen) and a
  generic `run` loop over a `Screen` trait.
- **Widgets** – modals (`confirm`, `input`, `input_wide`, `select`,
  `multi_select`, `number_input`, `message`), forms, single- and multi-line text
  editing with a shared editor core, autocomplete, tables, trees, selectable
  lists, sectioned sidebars, tabs, pagers, gauges, spinners, toasts.
- **Pickers** – color, swatch, date, date-range, month, path (with an optional
  confinement root) and slider pickers.
- **Overlays & chrome** – a fuzzy help overlay, a command palette, box framing,
  header and status bars, and wrapping shortcut-hint footers.
- **Markdown** – a CommonMark renderer (headings, lists, task lists, code
  blocks, quotes, GFM tables/callouts, links) producing styled lines, plus a
  scrollable `MarkdownView` widget and a viewer modal.
- **Theming** – a `Skin` bundling a `Palette` (semantic colors) and `Glyphs`
  (Unicode/ASCII variants); framework-agnostic so a CLI can share it, with a
  single ratatui adapter in `style`. A theme names a few base colors and the
  rest is derived: `border_focus` follows `border` unless it is set, and an
  override on `border` alone drags it along, so a focused frame never sinks
  into its own border.
- **Diagnostics** – degraded conditions (a missing clipboard tool, an unreadable
  directory, an invalid color override, a failed terminal restore) are emitted
  through the [`log`](https://crates.io/crates/log) facade at `warn`/`error`;
  install a logger to surface them.

## Usage

```toml
[dependencies]
ratada = "0.2"
```

Requires Rust 1.88 or newer (the crate uses `let`-chains).

Implement the `Screen` trait and hand it to `run`, which owns the draw/input
loop inside a raw-mode `Tui` guard. The `prelude` re-exports the driver
essentials:

```rust,no_run
use ratada::prelude::*;
use ratatui::{Frame, text::Line};
use crossterm::event::{KeyCode, KeyEvent};

struct App {
    count: u32,
}

impl Screen for App {
    type Error = std::io::Error;

    fn render(&self, frame: &mut Frame) {
        frame.render_widget(
            Line::from(format!("count: {}  (space +1, q quit)", self.count)),
            frame.area(),
        );
    }

    fn handle_key(&mut self, key: KeyEvent, _tui: &mut Tui) -> std::io::Result<Flow> {
        match key.code {
            KeyCode::Char('q') => Ok(Flow::Quit),
            KeyCode::Char(' ') => {
                self.count += 1;
                Ok(Flow::Continue)
            }
            _ => Ok(Flow::Continue),
        }
    }
}

fn main() -> std::io::Result<()> {
    let mut tui = Tui::new()?;
    run(&mut tui, &mut App { count: 0 })
}
```

Widgets that support the opt-in boxed style take a `BoxDecor` (caption in the
top border, an automatic or fixed badge bottom-right), e.g.
`InputField::new("").max_len(40).boxed(BoxDecor::new().caption("Name"))`.

## Documentation

- [API reference on docs.rs](https://docs.rs/ratada) – the authoritative,
  complete rustdoc (or `cargo doc --open` locally).
- [`DEVELOPMENT.md`](https://github.com/cgroening/rs-ratada/blob/main/docs/DEVELOPMENT.md)
  – module layout, conventions and how to add a widget.
- [`CLAUDE.md`](https://github.com/cgroening/rs-ratada/blob/main/CLAUDE.md) – the
  binding style guide.

## License

Licensed under the MIT License. See
[`LICENSE`](https://github.com/cgroening/rs-ratada/blob/main/LICENSE).

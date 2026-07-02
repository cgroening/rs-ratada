# ratada

A reusable [ratatui](https://ratatui.rs) widget toolkit for Rust terminal apps.

`ratada` provides the generic building blocks for a TUI over `ratatui` and
`crossterm`: a terminal guard and event loop, modals, forms, text editing,
pickers, tables, trees, a fuzzy finder, a help overlay, footers and status
bars, plus a framework-agnostic theming layer (colors, palette, glyphs and
layout mode) that maps onto ratatui styles.

## Overview

- **Driver** – a `Tui` RAII terminal guard (raw mode + alternate screen) and a
  generic `run` loop over a `Screen` trait.
- **Widgets** – modals (`confirm`, `select`, `multi_select`, `number_input`,
  `message`), forms, single- and multi-line text editing with a shared editor
  core, autocomplete, tables, trees, tabs, pagers, gauges, spinners, toasts.
- **Pickers** – color, date, date-range, month and path pickers.
- **Theming** – a `Skin` bundling a `Palette` (semantic colors), `Glyphs`
  (Unicode/ASCII variants) and a layout `Mode`; framework-agnostic so a CLI can
  share it, with a single ratatui adapter in `style`.

## Usage

```toml
[dependencies]
ratada = "0.0.1"
```

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

- [`API.md`](API.md) — a compact index of the public surface (rustdoc via
  `cargo doc --open` is the authoritative reference).
- [`DEVELOPMENT.md`](DEVELOPMENT.md) — module layout, conventions and how to add
  a widget.
- [`CLAUDE.md`](CLAUDE.md) — the binding style guide.

## License

MIT

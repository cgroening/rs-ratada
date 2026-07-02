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

## License

MIT

# Contributing to ratada

Thanks for your interest in improving `ratada`.

## Style guide

The binding coding conventions live in [`CLAUDE.md`](CLAUDE.md) (the single source of truth) and its referenced global style guide. Match the surrounding code; the documented rules take precedence over rustfmt/clippy where they differ.

## Gates

Every change must pass, before it is proposed:

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test            # unit tests, doctests and tests/
cargo doc --no-deps   # rustdoc must build warning-free
```

`cargo build` must be warning-free too – the crate enables `#![warn(missing_docs)]`, so every public item needs a `///` doc comment.

## Guidelines

- **Public API is a contract.** `ratada` is a published library; changes to a `pub` signature are breaking changes. Make them deliberately and record them in [`CHANGELOG.md`](CHANGELOG.md).
- **Keep the docs in sync.** When you change the public surface, update the rustdoc (the authoritative API reference), [`README.md`](README.md) and [`docs/DEVELOPMENT.md`](docs/DEVELOPMENT.md) as needed.
- **Reuse the shared building blocks** (navigation, scrollbar, framing, styling) described in [`docs/DEVELOPMENT.md`](docs/DEVELOPMENT.md) rather than reinventing them.
- **Ship tests** for logic-bearing code; add a render smoke case in `tests/render.rs` for a new frame-based widget.

See [`docs/DEVELOPMENT.md`](docs/DEVELOPMENT.md) for the module layout and how to add a widget.

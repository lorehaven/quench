---
title: quench-text
parent: Crates
nav_order: 52
has_children: true
---

`quench-text` (crate `quench_text`) turns a string into pixels with no third-party dependencies: a TrueType parser, pair kerning, line breaking, four weights of Noto Sans embedded so text looks the same on every machine, and a fallback to fonts installed on the machine for characters Noto lacks. It draws through [quench-raster](../quench-raster/index.md). [quench-layout](../quench-layout/index.md) measures text with it and [quench-paint](../quench-paint/index.md) draws with it.

* Version: 0.1.4. Edition 2024. No features. No `rust-version`.
* Dependencies: `quench-raster` (paths and the canvas). Dev: `quench-core`, `quench-test`.
* The published package includes `src/**`, `tests/**`, `fonts/**`, `Cargo.toml` (`include` in the manifest).

```toml
[dependencies]
quench-text = "0.1"
quench-raster = "0.1"
```

## Where to go next

* [Usage](usage.md)
* [API reference](api.md): `Font`, `FontError`, `Outline`, `Point`, `FontSet`, `Layout`, `Line`, `Glyph`, the `noto_sans*` functions, `SystemFonts`.
* [Internals](internals.md): what the TrueType parser reads, the kerning, line-breaking rules, caches, the embedded fonts and their licence, and what is not supported.
* [Testing](testing.md)

## Minimal example

```rust
use quench_raster::{Canvas, Color};
use quench_text::FontSet;

let fonts = FontSet::noto_sans();
let layout = fonts.layout("Zażółć gęślą jaźń: Привет", 16.0, Some(120.0));
let mut canvas = Canvas::new(140, 120);
fonts.draw(&mut canvas, &layout, 8.0, 8.0, Color::WHITE);
assert!(layout.lines.len() >= 2 && layout.width <= 120.0);
```

(This is the crate's doctest.)

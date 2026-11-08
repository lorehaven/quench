---
title: quench-text
parent: Crates
nav_order: 55
has_children: true
permalink: /crates/quench-text/
---

`quench-text` (crate `quench_text`) turns a string into pixels with no third-party dependencies. It has a TrueType parser, pair kerning, line breaking, four weights of Noto Sans embedded so text looks the same on every machine, and a fallback to fonts installed on the machine for characters Noto lacks. For math typesetting it reads a font's OpenType `MATH` table (constants, italic corrections, accent attachments, extended shapes, math kerns, glyph variants and assemblies) and, with the `math-font` feature, embeds Noto Sans Math. It draws through [quench-raster](../quench-raster/index.md). [quench-layout](../quench-layout/index.md) measures text with it, [quench-paint](../quench-paint/index.md) draws with it, and [quench-tex](../quench-tex/index.md) typesets formulas with its `MATH` reader.

* Version: `0.1.5`. Edition 2024. No `rust-version`. `[lints] workspace = true`; no `unsafe`.
* `autotests = false`; one test target, `unit` (`tests/unit.rs`, `harness = false`).
* The published package includes `src/**`, `tests/**`, `fonts/**`, `Cargo.toml` (`include` in the manifest).

## Install

```toml
[dependencies]
quench-text = { version = "0.1", registry = "ennor" }     # in the workspace: { workspace = true }
quench-raster = { version = "0.1", registry = "ennor" }   # for Canvas, Color and Path
```

With the embedded math font:

```toml
[dependencies]
quench-text = { version = "0.1", registry = "ennor", features = ["math-font"] }
```

## Features

| Feature | Default | What it adds |
|---|---|---|
| `math-font` | off | Embeds `fonts/NotoSansMath-Regular.ttf` (Noto Sans Math 3.000, 1 015 396 bytes) and exports `noto_sans_math() -> Arc<Font>`. Nothing else changes: `Font::math` and the `MATH` types are there without the feature. |

## Dependencies

| Dependency | Kind | Why |
|---|---|---|
| `quench-raster` | normal | `Path` (glyph outlines), `Canvas` and `Color` (drawing) |
| `quench-core` | dev | used by the tests |
| `quench-test` | dev | the custom test harness (`#[quench_test::test]`, `quench_test::main!()`) |

No third-party crates.

## Where to go next

* [Usage](usage.md): measuring, wrapping, carets, fallback faces, own fonts, outlines, and reading a math font.
* [API reference](api.md): `Font`, `FontError`, `Bounds`, `Outline`, `Point`, the `MATH` types (`Math`, `MathConstants`, `MathKern`, `MathKernCorner`, `MathKernInfo`, `GlyphConstruction`, `GlyphVariant`, `GlyphAssembly`, `GlyphPart`), `FontSet`, `Layout`, `Line`, `Glyph`, the `noto_sans*` functions, `SystemFonts`.
* [Internals](internals.md): what the TrueType and `MATH` parsers read, the kerning, line-breaking rules, caches, the embedded fonts (Noto Sans Math included) and their licence, and what is not supported.
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

(This is the crate's doctest.) Reading the `MATH` table of the embedded math font (feature `math-font`; the values are the ones `tests/unit.rs` asserts):

```rust
use quench_text::noto_sans_math;

let font = noto_sans_math();
let math = font.math().unwrap();
assert_eq!(math.constants().unwrap().axis_height, 278);   // font units, 1000 to the em
let paren = math.vertical_construction(font.glyph('(').unwrap()).unwrap();
assert!(paren.assembly.is_some());
```

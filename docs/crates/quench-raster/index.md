---
title: quench-raster
parent: Crates
nav_order: 37
has_children: true
---

`quench-raster` (crate `quench_raster`) is a software rasteriser with no dependencies. It draws anti-aliased rectangles, rounded rectangles, shadows, images, layers and arbitrary paths into a premultiplied BGRA pixel buffer. It is the lowest layer of the native renderer: [quench-text](../quench-text/index.md) turns glyph outlines into `Path`s, [quench-paint](../quench-paint/index.md) turns boxes into `Canvas` calls.

* Version: 0.1.3. Edition 2024 (workspace). No `rust-version` is set.
* No `unsafe` in the crate sources. No features. No dependencies (`[dependencies]` is empty; dev-dependency `quench-test`).

```toml
[dependencies]
quench-raster = "0.1"
```

## Where to go next

* [Usage](usage.md): worked examples.
* [API reference](api.md): every public item.
* [Internals](internals.md): pixel format, blending arithmetic, the coverage algorithms.
* [Testing](testing.md).

## Minimal example

```rust
use quench_raster::{Canvas, Color, Path};

let mut canvas = Canvas::new(64, 32);
canvas.clear(Color::from_hex("#1b1d23").unwrap());
canvas.fill_rounded_rect(4.0, 4.0, 40.0, 16.0, 6.0, Color::rgb(70, 130, 220));

let mut triangle = Path::new();
triangle.move_to(50.0, 4.0).line_to(60.0, 28.0).line_to(40.0, 28.0).close();
canvas.fill_path(&triangle, 0.0, 0.0, Color::rgba(255, 200, 0, 200));

assert_eq!(canvas.bytes().len(), 64 * 32 * 4);
assert_eq!(canvas.pixel(0, 0), Color::rgb(0x1b, 0x1d, 0x23));
```

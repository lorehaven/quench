---
title: quench-paint
parent: Crates
nav_order: 32
has_children: true
---

`quench-paint` (crate `quench_paint`) paints a laid-out box tree ([quench-layout](../quench-layout/index.md) `LayoutBox`) into a [quench-raster](../quench-raster/index.md) `Canvas`: shadows, backgrounds, borders with radii and dashes, inset shadows, overflow clipping, text, outlines, opacity (through a layer), `translate`/`rotate`/`scale`, scroll bars, and host-supplied decorations (check marks, images, carets).

* Version: 0.1.5. Edition 2024. No features. No `rust-version`.
* Dependencies: `quench-css` (computed style types), `quench-layout` (the box tree, `Fonts`), `quench-raster` (the canvas). Dev: `quench-test`.

```toml
[dependencies]
quench-paint = "0.1"
```

## Where to go next

* [Usage](usage.md)
* [API reference](api.md)
* [Painting](painting.md): paint order and what each style property does (supported, partial, ignored).
* [Testing](testing.md)

## Minimal example

```rust
use quench_layout::{Fonts, layout};
use quench_paint::paint;
use quench_raster::{Canvas, Color};

// `root` is an Rc<quench_layout::Node> built as in the quench-layout usage page.
let fonts = Fonts::embedded();
let boxes = layout(&root, (200.0, 100.0), &fonts);
let mut canvas = Canvas::new(200, 100);
canvas.clear(Color::WHITE);
paint(&mut canvas, &boxes, &fonts, &|_| (0.0, 0.0), &|_| Vec::new());
```

A complete runnable version (cascade, layout, paint) is on the [usage page](usage.md); it was run and gave a `#3b82f6` pixel inside the card.

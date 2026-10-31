# quench-paint
Paints a laid-out box tree into a pixel canvas: backgrounds, borders, radii, shadows, outlines, opacity, clipping and text.

## What it is
`quench-paint` paints a `quench-layout` `LayoutBox` tree into a `quench-raster` `Canvas`. It handles shadows, backgrounds, borders with radii and dashes, inset shadows, overflow clipping, text, outlines, opacity, transforms, scroll bars and host-supplied decorations such as check marks, images and carets.

It is part of the native renderer: `quench-css` supplies computed styles, `quench-layout` the box tree and fonts, and `quench-raster` the drawing surface. Use the same `Fonts` for layout and paint.

## Use cases
- Render a styled box tree to a pixel buffer without a browser.
- Draw scrollable containers by supplying scroll offsets per box key.
- Add host-drawn extras (checkbox ticks, images, carets) through `Decoration`.
- Walk children in paint order with `Layers` for hit-testing.

## Example
```rust
use quench_layout::{Fonts, layout};
use quench_paint::paint;
use quench_raster::{Canvas, Color};

// `root` is an Rc<quench_layout::Node> built as in the quench-layout docs.
let fonts = Fonts::embedded();
let boxes = layout(&root, (200.0, 100.0), &fonts);
let mut canvas = Canvas::new(200, 100);
canvas.clear(Color::WHITE);
// scroll offsets and decorations are callbacks keyed by box key
paint(&mut canvas, &boxes, &fonts, &|_| (0.0, 0.0), &|_| Vec::new());
```

## Documentation
Full documentation: https://lorehaven.github.io/quench/crates/quench-paint/

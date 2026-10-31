# quench-raster
A software rasteriser: anti-aliased paths, rectangles and rounded rectangles composited into a premultiplied BGRA canvas.

## What it is
`quench-raster` is a software rasteriser with no dependencies. It draws anti-aliased rectangles, rounded rectangles, shadows, images, layers and arbitrary paths into a premultiplied BGRA pixel buffer, either one it owns or memory you supply with `Canvas::over`.

It is the lowest layer of the native renderer: `quench-text` turns glyph outlines into `Path`s, and `quench-paint` turns boxes into `Canvas` calls.

## Use cases
- Draw shapes into an in-memory image or a frame buffer.
- Render CSS-like boxes with radii, borders and blurred shadows.
- Fill glyph-like paths with anti-aliasing.
- Clip drawing to nested rectangles for overflow handling.

## Example
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

## Documentation
Full documentation: https://lorehaven.github.io/quench/crates/quench-raster/

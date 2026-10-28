# Quench Raster

`quench-raster` (crate `quench_raster`) is a software rasteriser with no dependencies: it draws anti-aliased shapes into a pixel buffer. It is the lowest piece of the [native renderer](../plans/NATIVE_RENDERER.md): text, boxes, borders and everything else a screen is made of end up as the calls below.

```rust
use quench_raster::{Canvas, Color, Path};

let mut canvas = Canvas::new(320, 200);
canvas.clear(Color::from_hex("#1b1d23").unwrap());
canvas.fill_rounded_rect(16.0, 16.0, 120.0, 40.0, 8.0, Color::rgb(70, 130, 220));
canvas.stroke_rounded_rect(16.0, 16.0, 120.0, 40.0, 8.0, 2.0, Color::WHITE);

let mut triangle = Path::new();
triangle.move_to(200.0, 40.0).line_to(280.0, 120.0).line_to(180.0, 140.0).close();
canvas.fill_path(&triangle, 0.0, 0.0, Color::rgba(255, 200, 0, 200));
// canvas.bytes() is the frame, ready to hand to a compositor.
```

## The canvas

A `Canvas` is `width * height` pixels of **premultiplied BGRA**, `width * 4` bytes per row: the memory layout of Wayland's `ARGB8888` (the same as a little-endian `u32` `0xAARRGGBB`), so a frame goes to a compositor as it is, with no conversion. `pixel(x, y)` reads one back as a straight-alpha `Color`.

Drawing is **source-over** compositing, and everything respects a **clip** (`set_clip` returns the previous one, `restore_clip` puts it back). Coordinates are `f32` pixels, origin at the top left; pixel `(x, y)` covers `x..x+1, y..y+1`.

| Call | What it draws |
|---|---|
| `clear(color)` | replaces every pixel |
| `fill_rect(x, y, w, h, color)` | a rectangle on whole pixels (the coordinates are rounded): the fast path for boxes and backgrounds |
| `fill_rounded_rect(x, y, w, h, radius, color)` | a rounded rectangle at fractional coordinates, anti-aliased from the distance to its edge; a radius beyond half the short side is a pill |
| `stroke_rounded_rect(x, y, w, h, radius, width, color)` | its outline, `width` thick and inside the bounds |
| `fill_path(&path, dx, dy, color)` | any `Path`, moved by `(dx, dy)` |

## Paths

A `Path` is contours of lines and curves (`move_to`, `line_to`, `quad_to` for TrueType's curves, `cubic_to` for CFF and SVG's, `close`, plus `rect` and `ellipse`). Curves are flattened as they are added, to within 0.02 pixels of the true curve. A fill closes every contour and uses **non-zero winding**: a contour drawn the other way is a hole, and two overlapping contours of the same direction are one shape, not twice as opaque. This is what glyph outlines need.

## How the anti-aliasing works

`fill_path` is exact signed-area coverage, not supersampling: each edge adds the area it sweeps to the pixels it crosses in one pass, and a running sum along each row turns that into coverage. It costs the edges plus the covered area, and it is the same cost for a thin shape as for a large one. Edges outside the canvas are split at its sides, so the part inside keeps its exact slope and the part outside still contributes its winding. Only the part of a path's box that is inside the clip is rasterised.

## Tested

13 tests, 99.5% line coverage: pixel layout, source-over arithmetic, clipping of every kind of drawing, exact edges on whole pixels, half-covered edges at half pixels, the area of a triangle, circle, parabola and rounded rectangle against the formulas (within 0.5–1%), holes, overlap, shapes partly or wholly outside the canvas, and strokes.

## Boxes as CSS draws them

The calls above take one radius. A CSS box needs more, so there are four more:

| Call | What it draws |
|---|---|
| `fill_rrect(rect, [tl, tr, br, bl], color)` | a rectangle whose corners have their own radii (the signed distance to a rounded box with one radius per quadrant) |
| `stroke_rrect(rect, radii, width, color)` | its outline, inside the bounds |
| `shadow_rrect(rect, radii, blur, color, inset)` | a `box-shadow`: the edge fades as a Gaussian of `blur / 2` (the integral of the bell curve, with an error-function approximation accurate to 1.5e-7), or, with `inset: Some((box, radii))`, the area of that box the shape does not cover |
| `draw_layer(&layer, x, y, opacity)` | another canvas over this one with an opacity: how `opacity` on a box with children is done |
| `clip_to(x, y, w, h)` | narrows the clip to the part of the rectangle already inside it (`set_clip` replaces it): how nested `overflow: hidden` boxes stack |

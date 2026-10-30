---
title: Usage
parent: quench-raster
nav_order: 1
---

Worked examples, from the simple to the advanced. All coordinates are `f32` pixels, origin top left; pixel `(x, y)` covers `x..x+1, y..y+1`.

## 1. Draw and read back

```rust
use quench_raster::{Canvas, Color};

let mut c = Canvas::new(8, 8);                 // transparent
c.fill_rect(2.0, 3.0, 3.0, 2.0, Color::rgb(255, 0, 0));
assert_eq!(c.pixel(2, 3), Color::rgb(255, 0, 0));
assert_eq!(c.pixel(0, 0), Color::TRANSPARENT);
assert_eq!(c.pixel(99, 99), Color::TRANSPARENT);   // outside: transparent, no panic
```

`bytes()` is blue, green, red, alpha per pixel, premultiplied. Pitfall: `pixel()` converts back to straight alpha and rounds, so a very low alpha does not round-trip exactly.

## 2. Draw into memory you own

`Canvas::over` wraps any `AsRef<[u8]> + AsMut<[u8]>` (a `&mut [u8]`, a memory-mapped frame buffer) without copying. The bytes are used as they are: clear them first for a transparent canvas.

```rust
use quench_raster::{Canvas, Color};

let mut frame = vec![0u8; 16 * 16 * 4];
let mut c = Canvas::over(16, 16, frame.as_mut_slice()).expect("buffer long enough");
c.fill_rect(0.0, 0.0, 16.0, 16.0, Color::WHITE);
assert!(Canvas::over(16, 16, vec![0u8; 10]).is_none());   // too short
```

## 3. Clipping

```rust
use quench_raster::{Canvas, Color};

let mut c = Canvas::new(10, 10);
let before = c.set_clip(2, 2, 4, 4);       // replaces the clip (still limited to the canvas)
c.fill_rect(0.0, 0.0, 10.0, 10.0, Color::BLACK);
c.restore_clip(before);
// nested overflow: hidden boxes
let outer = c.clip_to(0, 0, 5, 5);         // intersects with the current clip
let inner = c.clip_to(3, 3, 5, 5);
c.restore_clip(inner);
c.restore_clip(outer);
```

`set_clip` replaces; `clip_to` intersects. Both return the previous clip to give back to `restore_clip`.

## 4. CSS-like boxes

```rust
use quench_raster::{Canvas, Color};

let mut c = Canvas::new(100, 60);
let rect = (10.0, 10.0, 80.0, 40.0);
c.shadow_rrect(rect, [8.0; 4], 12.0, Color::rgba(0, 0, 0, 120), None);   // box-shadow, blur 12
c.fill_rrect(rect, [8.0, 8.0, 0.0, 0.0], Color::WHITE);                  // tl, tr, br, bl
c.stroke_rrect(rect, [8.0, 8.0, 0.0, 0.0], 2.0, Color::rgb(70, 130, 220)); // inside the bounds
```

An inset shadow takes the box it is inset into: `shadow_rrect(hole, radii, blur, color, Some((outer, outer_radii)))`.

## 5. Paths (glyph-like shapes, holes)

```rust
use quench_raster::{Canvas, Color, Path};

let mut ring = Path::new();
ring.ellipse(50.0, 50.0, 40.0, 40.0);        // clockwise
// A hole needs the opposite direction: draw it by hand, counter-clockwise.
ring.move_to(70.0, 50.0)
    .cubic_to(70.0, 38.9, 61.1, 30.0, 50.0, 30.0)
    .cubic_to(38.9, 30.0, 30.0, 38.9, 30.0, 50.0)
    .cubic_to(30.0, 61.1, 38.9, 70.0, 50.0, 70.0)
    .cubic_to(61.1, 70.0, 70.0, 61.1, 70.0, 50.0)
    .close();
let mut c = Canvas::new(100, 100);
c.fill_path(&ring, 0.0, 0.0, Color::BLACK);
assert_eq!(c.pixel(50, 50), Color::TRANSPARENT);   // the hole
```

Pitfall: two overlapping contours of the same direction are one shape (non-zero winding), not a hole.

## 6. Images and layers

`draw_image(rgba, w, h, rect, opacity)` scales straight-alpha RGBA with bilinear filtering. `draw_layer(&layer, x, y, opacity)` composites another (premultiplied) canvas; `draw_layer_transformed` also rotates and scales around a pivot. This is how `opacity` and `transform` on a box with children are painted.

```rust
use quench_raster::{Canvas, Color};

let mut layer = Canvas::new(20, 20);
layer.fill_rect(0.0, 0.0, 20.0, 20.0, Color::rgb(200, 0, 0));
let mut page = Canvas::new(50, 50);
page.draw_layer(&layer, 5, 5, 0.5);
page.draw_layer_transformed(&layer, (25, 25), 1.0, (35.0, 35.0), 45.0, (1.0, 1.0));
```

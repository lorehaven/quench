---
title: API reference
parent: quench-raster
nav_order: 2
---

Crate root (`src/lib.rs`) re-exports: `pub use canvas::{Canvas, Rounded}; pub use color::Color; pub use path::Path;`. Modules `canvas`, `color`, `path` are private. Nothing is feature-gated; there is no `unsafe`. All types are plain data: `Color` is `Send + Sync`, `Path` and `Canvas<Vec<u8>>` are `Send + Sync`.

Public items: 3 types (`Canvas`, `Color`, `Path`), 1 type alias (`Rounded`), 3 constants, 4 `Color` functions, 10 `Path` methods and 22 `Canvas` methods (the `new`/`over` constructors included).

## `Color`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color { pub r: u8, pub g: u8, pub b: u8, pub a: u8 }
```

A colour with **straight** (not premultiplied) alpha.

| Item | Signature | Notes |
|---|---|---|
| `Color::TRANSPARENT` | `pub const TRANSPARENT: Self` | `rgba(0,0,0,0)` |
| `Color::BLACK` | `pub const BLACK: Self` | `rgb(0,0,0)` |
| `Color::WHITE` | `pub const WHITE: Self` | `rgb(255,255,255)` |
| `rgb` | `#[must_use] pub const fn rgb(r: u8, g: u8, b: u8) -> Self` | alpha 255 |
| `rgba` | `#[must_use] pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self` | |
| `with_opacity` | `#[must_use] pub fn with_opacity(self, opacity: f32) -> Self` | alpha multiplied by `opacity` clamped to 0..=1, rounded |
| `from_hex` | `#[must_use] pub fn from_hex(text: &str) -> Option<Self>` | `#rgb`, `#rrggbb`, `#rrggbbaa`, the `#` optional. `#rgb` digits are doubled (`v * 17`). Other lengths, non-hex or non-ASCII text give `None`. |

## `Path`

```rust
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Path { /* private: contours: Vec<Vec<(f32, f32)>>, current: Vec<(f32, f32)> */ }
```

A sequence of contours of lines and curves. Curves are **flattened as they are added** (tolerance 0.02 px, at least 2 and at most 128 segments per curve). A contour is closed implicitly when the next starts and when the path ends (a fill closes every contour). Opposite winding directions leave a hole. All builder methods return `&mut Self` for chaining. The coordinate pair type used in `contours()` is `(f32, f32)` (the alias `Point` is private).

| Method | Signature | Behaviour |
|---|---|---|
| `new` | `#[must_use] pub fn new() -> Self` | empty path (also `Default`) |
| `move_to` | `pub fn move_to(&mut self, x: f32, y: f32) -> &mut Self` | finishes the current contour, starts a new one |
| `line_to` | `pub fn line_to(&mut self, x: f32, y: f32) -> &mut Self` | |
| `quad_to` | `pub fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) -> &mut Self` | quadratic curve (TrueType). With no current point it acts as `move_to(x, y)`. |
| `cubic_to` | `pub fn cubic_to(&mut self, c1x: f32, c1y: f32, c2x: f32, c2y: f32, x: f32, y: f32) -> &mut Self` | cubic curve (CFF, SVG). With no current point it acts as `move_to(x, y)`. |
| `close` | `pub fn close(&mut self) -> &mut Self` | ends the current contour; contours with fewer than 2 points are dropped |
| `rect` | `pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32) -> &mut Self` | clockwise on screen |
| `ellipse` | `pub fn ellipse(&mut self, cx: f32, cy: f32, rx: f32, ry: f32) -> &mut Self` | four cubics, constant K = 0.5522848, clockwise on screen |
| `contours` | `pub fn contours(&self) -> impl Iterator<Item = &[(f32, f32)]>` | every contour, the open current one included if it has 2 or more points |
| `bounds` | `#[must_use] pub fn bounds(&self) -> Option<(f32, f32, f32, f32)>` | `(min_x, min_y, max_x, max_y)` of the flattened points; `None` when empty |

## `Rounded`

```rust
pub type Rounded = ((f32, f32, f32, f32), [f32; 4]);
```

A rectangle `(x, y, width, height)` and the radii of its corners `[top-left, top-right, bottom-right, bottom-left]`. Used for the `inset` argument of `shadow_rrect`.

## `Canvas`

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Canvas<S = Vec<u8>> { /* width, height, pixels: S, clip */ }
```

A pixel buffer: premultiplied BGRA, `width * 4` bytes per row. It owns a `Vec<u8>` by default; any `S: AsRef<[u8]> + AsMut<[u8]>` can back it. The initial clip is the whole canvas. All drawing is source-over and limited to the clip; drawing outside the canvas is silently skipped.

### Constructors

| Item | Signature | Notes |
|---|---|---|
| `new` (on `Canvas<Vec<u8>>`) | `#[must_use] pub fn new(width: usize, height: usize) -> Self` | transparent; allocates `width * height * 4` bytes |
| `over` | `pub fn over(width: usize, height: usize, pixels: S) -> Option<Self>` | `None` if `pixels` is shorter than `width*height*4` (or the product overflows). Longer buffers are allowed; bytes beyond are never touched. The content is not cleared. |

### Reading

| Method | Signature | Notes |
|---|---|---|
| `width` | `#[must_use] pub const fn width(&self) -> usize` | |
| `height` | `#[must_use] pub const fn height(&self) -> usize` | |
| `bytes` | `#[must_use] pub fn bytes(&self) -> &[u8]` | exactly `width*height*4` bytes: B, G, R, A, premultiplied |
| `bytes_mut` | `pub fn bytes_mut(&mut self) -> &mut [u8]` | same slice, mutable (not `#[must_use]`) |
| `pixel` | `#[must_use] pub fn pixel(&self, x: usize, y: usize) -> Color` | straight-alpha colour (un-premultiplied with rounding); `Color::TRANSPARENT` outside the canvas or when alpha is 0 |
| `clip` | `#[must_use] pub const fn clip(&self) -> (i32, i32, i32, i32)` | `(x0, y0, x1, y1)`, end excluded |

### Clipping

| Method | Signature | Behaviour |
|---|---|---|
| `set_clip` | `pub fn set_clip(&mut self, x: i32, y: i32, w: i32, h: i32) -> (i32, i32, i32, i32)` | replaces the clip with the rectangle limited to the canvas; returns the previous clip |
| `clip_to` | `pub fn clip_to(&mut self, x: i32, y: i32, w: i32, h: i32) -> (i32, i32, i32, i32)` | intersects the rectangle with the current clip; returns the previous clip |
| `restore_clip` | `pub const fn restore_clip(&mut self, clip: (i32, i32, i32, i32))` | stores the value as it is |

An empty or inverted clip draws nothing.

### Filling

| Method | Signature | Behaviour |
|---|---|---|
| `clear` | `pub fn clear(&mut self, color: Color)` | replaces every pixel with the colour premultiplied by its own alpha (not blended; the clip is **not** applied) |
| `fill_rect` | `pub fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: Color)` | whole pixels: `x`, `y`, `x+w`, `y+h` are each rounded to the nearest integer. Opaque colours are written row by row; translucent ones are blended row by row; alpha 0 draws nothing. |
| `fill_rounded_rect` | `pub fn fill_rounded_rect(&mut self, x: f32, y: f32, w: f32, h: f32, radius: f32, color: Color)` | one radius for all corners, clamped to `0..=min(w,h)/2` (so a large radius is a pill); anti-aliased at fractional coordinates |
| `stroke_rounded_rect` | `pub fn stroke_rounded_rect(&mut self, x: f32, y: f32, w: f32, h: f32, radius: f32, width: f32, color: Color)` | outline `width` thick, inside the bounds; same radius clamp |
| `fill_rrect` | `pub fn fill_rrect(&mut self, rect: (f32, f32, f32, f32), radii: [f32; 4], color: Color)` | per-corner radii `[tl, tr, br, bl]`, each clamped to `0..=min(w,h)/2`. When `x, y, w, h` are whole numbers and `w >= 1`, `h >= 1` and the corner bands fit, the straight middle is filled with `fill_rect` and only the corner bands use the distance calculation. |
| `stroke_rrect` | `pub fn stroke_rrect(&mut self, rect: (f32, f32, f32, f32), radii: [f32; 4], width: f32, color: Color)` | outline inside the bounds |
| `shadow_rrect` | `pub fn shadow_rrect(&mut self, rect: (f32, f32, f32, f32), radii: [f32; 4], blur: f32, color: Color, inset: Option<Rounded>)` | `blur` is the CSS blur radius; the edge is a Gaussian of sigma `blur/2` (`blur <= 0` gives a hard edge). Outer shadow (`None`): painted over the box grown by `3*sigma` on each side (the shadow of `rect`, which includes the area under the box itself). Inset (`Some((outer, outer_radii))`): the part of `outer` that `rect` does not cover. Offsets and spread are the caller's job (change `rect`). |
| `fill_path` | `pub fn fill_path(&mut self, path: &Path, dx: f32, dy: f32, color: Color)` | non-zero winding, exact-area anti-aliasing, path moved by `(dx, dy)`; every contour is closed; empty path draws nothing |

Shapes with `w <= 0` or `h <= 0` draw nothing (`fill_rect` with negative size draws nothing because the end is before the start).

### Images and layers

| Method | Signature | Behaviour |
|---|---|---|
| `draw_image` | `pub fn draw_image(&mut self, rgba: &[u8], image_width: usize, image_height: usize, rect: (f32, f32, f32, f32), opacity: f32)` | straight-alpha RGBA scaled to `rect` (its corners rounded to whole pixels) with bilinear filtering on premultiplied values, `opacity` clamped to 0..=1. Nothing is drawn when a size is 0, when `rgba.len() != image_width*image_height*4`, or when the rectangle is empty. |
| `draw_layer` | `pub fn draw_layer<T: AsRef<[u8]> + AsMut<[u8]>>(&mut self, layer: &Canvas<T>, x: i32, y: i32, opacity: f32)` | source-over of a premultiplied canvas at integer offset, alpha and colour scaled by `opacity` (rounded to 1/255) |
| `draw_layer_transformed` | `pub fn draw_layer_transformed<T: AsRef<[u8]> + AsMut<[u8]>>(&mut self, layer: &Canvas<T>, (x, y): (i32, i32), opacity: f32, pivot: (f32, f32), degrees: f32, scale: (f32, f32))` | the layer at `(x, y)` scaled by `scale` then turned `degrees` clockwise around `pivot` (destination pixels); each destination pixel samples four layer pixels bilinearly. Does nothing for an empty layer or a zero scale component. Pixels with interpolated alpha <= 0.5 (of 255) are skipped. |

The `clip` of the source layer is ignored; the destination clip applies.

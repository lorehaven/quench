---
title: API reference
parent: quench-paint
nav_order: 2
---

Everything is in `src/lib.rs` (no modules). No `unsafe`, no features. Items: 1 function `paint`, 1 function `to_raster`, 1 constant `SCROLLBAR`, 2 type aliases `Scroll`, `Decorations`, 1 enum `Decoration` (5 variants), 1 struct `Layers` with `of`.

## `paint`

```rust
pub fn paint<S: AsRef<[u8]> + AsMut<[u8]>>(
    canvas: &mut Canvas<S>,
    root: &LayoutBox,
    fonts: &Fonts,
    scroll: Scroll<'_>,
    decorations: Decorations<'_>,
)
```

Paints `root` (from `quench_layout::layout`) into `canvas`. The root is painted with its own `x`, `y` as origin. The canvas is **not** cleared; the clip of the canvas is respected (and narrowed for clipping boxes). Any `Canvas<S>` works, including one over a frame buffer. See [painting](painting.md) for the order.

## Type aliases

```rust
pub type Scroll<'a> = &'a dyn Fn(usize) -> (f32, f32);
pub type Decorations<'a> = &'a dyn Fn(usize) -> Vec<Decoration>;
```

* `Scroll`: box key -> `(x, y)` scroll offset in pixels. Asked for boxes that clip and for the box with key `0`.
* `Decorations`: box key -> decorations for that box. Called twice per visible box (once for background images, once for the rest).

## `Decoration`

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum Decoration {
    Check(quench_css::Color),
    Image { rgba: Rc<Vec<u8>>, width: u32, height: u32 },
    Background { rgba: Rc<Vec<u8>>, width: u32, height: u32, size: quench_css::BackgroundSize,
                 repeat: bool, position: (f32, f32), scale: f32 },
    Dot(quench_css::Color),
    Bar { x: f32, y: f32, width: f32, height: f32, color: quench_css::Color },
}
```

| Variant | Effect |
|---|---|
| `Check(color)` | A check mark filling the content box (two strokes, thickness 0.16 of the smaller side), drawn inside the box's clip, after its children. |
| `Image` | RGBA image stretched over the content box (bilinear). |
| `Background` | A `background-image`: drawn over the **padding box**, clipped to it, after the background colour and before inset shadows and borders. `size`: `Auto` = image size times `scale` (device pixels per CSS pixel), `Cover`, `Contain`. `position` are fractions of the free space (`(0.5, 0.5)` = center). `repeat` tiles in both directions (at most 256 x 256 tiles). |
| `Dot(color)` | A filled circle of half the smaller content side in the middle of the content box (radio). |
| `Bar { x, y, width, height, color }` | A rectangle at `(x, y)` from the border box top left (rounded to whole pixels): a caret. |

## `Layers`

```rust
#[derive(Debug, Default)]
pub struct Layers<'a> {
    pub below: Vec<&'a LayoutBox>,   // positioned, negative z-index
    pub blocks: Vec<&'a LayoutBox>,  // in-flow blocks
    pub inline: Vec<&'a LayoutBox>,  // inline-block and inline-flex (note: not inline-grid)
    pub above: Vec<&'a LayoutBox>,   // positioned, z-index >= 0, sorted by z-index (stable)
}
impl<'a> Layers<'a> { #[must_use] pub fn of(b: &'a LayoutBox) -> Self }
```

The children of a box in paint order. A child with `position != static` goes to `below` (z < 0; `None` counts as 0) or `above`; otherwise `display: inline-block | inline-flex` goes to `inline`, everything else to `blocks`. `inline-grid` children land in `blocks`.

## Other

```rust
pub const SCROLLBAR: f32 = 6.0;      // thickness of the scroll bar
#[must_use] pub const fn to_raster(c: quench_css::Color) -> quench_raster::Color   // field-wise copy
```

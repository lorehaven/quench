# Quench Paint

`quench-paint` (crate `quench_paint`) draws a laid out box tree into a [`quench-raster`](./quench-raster.md) canvas: shadows, backgrounds, borders, radii, outlines, clipping, opacity and text. It is the fifth piece of the [native renderer](../plans/NATIVE_RENDERER.md).

```rust
use quench_paint::paint;

let boxes = quench_layout::layout(&root, (390.0, 844.0), &fonts);
paint(&mut canvas, &boxes, &fonts, &|_key| (0.0, 0.0), &|_key| Vec::new());
//                                  scroll offsets        decorations
```

## What is painted, and in which order

A box paints its **outer shadows** (the first on top), its **background** (colour, with its corner radii; then a `background-image`, sized, placed and tiled inside the padding box and not clipped to the radii), its **inset shadows**, its **border**, then, inside the clip of its padding box when `overflow` is not `visible`: children with a negative `z-index`, in-flow blocks, the backgrounds and borders of inline elements, text, inline blocks, positioned children by `z-index` (then tree order). Last, after the clip is lifted, its **outline**. This is CSS's painting order for what a page of ordinary boxes needs; it is not a full stacking-context implementation (a `z-index` orders siblings, and an ancestor's context is not separate from its neighbours').

- **Pixel snapping**: box edges are rounded to whole pixels for backgrounds and borders, so edges are crisp as in a browser. Text is placed at its baseline on a whole pixel.
- **Borders**: one ring when all four sides match; otherwise side by side (rectangles when square; clipped strokes when rounded). `dashed` and `dotted` are drawn along each edge in segments of three and one times the width. Inline elements have borders and radii at their first and last line only.
- **Radii** are scaled together when two would not fit on a side, as CSS does; `%` is of the short side (an ellipse is drawn as a circle).
- **Shadows** use the Gaussian edge of `quench_raster::Canvas::shadow_rrect`; `spread` grows or shrinks the shape.
- **Opacity** below 1 draws the box and everything in it into a layer sized to what it covers (plus a margin for shadows) and composites it once, so a child is not blended twice. `translate()` moves a box and its contents; `scale` and `rotate` are not painted.
- **Scrolling**: the scroll callback gives each box's offset by key; children of a scroll container move by it and are clipped to its padding box. No scrollbars are drawn.
- **Text** is drawn with `quench-text` in the box's colour, bold from weight 600; `visibility: hidden` boxes paint nothing but their visible children do.

## Decorations

What is not CSS goes through a callback by key: `Decoration::Check(colour)` (the mark in a checked checkbox, drawn as two thick strokes over the content box), `Dot(colour)` (a selected radio button) and `Bar { x, y, width, height, colour }` (a caret). `quench-ui-native` supplies them from the state of its form controls.

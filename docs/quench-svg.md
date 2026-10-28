# Quench SVG

`quench-svg` (crate `quench_svg`) draws SVG with no dependencies: a file in, RGBA pixels out, through [`quench-raster`](./quench-raster.md).

```rust
let image = quench_svg::render(svg_text, 2.0)?;   // quench_image::Image, at 2 device pixels per CSS pixel
let (w, h) = quench_svg::size(svg_text)?;         // its own size in CSS pixels
```

**Drawn**: `rect` (with `rx`/`ry`), `circle`, `ellipse`, `line`, `polyline`, `polygon`, `path` (every command: `M L H V C S Q T A Z`, relative or not, numbers written the compact way, arcs in endpoint form), `g`, `a`, `switch`, nested `svg`, and `use` of an element with an `id`; `transform` (`matrix`, `translate`, `scale`, `rotate` about a point, `skewX`, `skewY`); `viewBox` fitted into `width` x `height` (meet, centred); `fill` and `stroke` as hex, `rgb()`, names, `currentColor`, with `fill-opacity`, `stroke-opacity` and `opacity`; `stroke-width`, `stroke-linecap` (butt, round, square) and `stroke-linejoin` (miter with `stroke-miterlimit`, round, bevel); the `style` attribute and inheritance down the tree; `display: none` and `visibility: hidden`.

**Not drawn**: text, gradients and patterns (a `url(#gradient)` paint is the gradient's first stop), filters, masks, clip paths, markers, dashes, `fill-rule: evenodd` (fills are non-zero), CSS in `<style>`, and `opacity` of a group as a whole (it is applied to each child, so overlaps inside the group show through).

The XML reader is small and strict: elements, attributes, text, comments, CDATA and the five entities and numeric references; a doctype is skipped (no entities are read from it), nesting is limited to 64, drawn elements to 100000, and a picture to 16384 pixels a side and 64 million in all. A `use` that points at itself stops after eight levels.

The native renderer uses it for `<img src>` with a `data:image/svg+xml` URL (base64 or percent-escaped), registered bytes or a loader: the drawing is made at the page's density when it is loaded and again when the density changes, so it is sharp at any scale.

7 tests and the doctest: shapes and colours, view boxes and density, strokes, paths and arcs, transforms and `use`, colour notations, and hostile files.

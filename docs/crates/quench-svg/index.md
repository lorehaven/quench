---
title: quench-svg
parent: Crates
nav_order: 46
has_children: true
permalink: /crates/quench-svg/
---

`quench-svg` (crate `quench_svg`) draws SVG: a document string in, straight-alpha RGBA pixels out (a `quench_image::Image`), through the software rasteriser [quench-raster](../quench-raster/index.md). It exists so the [native renderer](../quench-ui-native/index.md) can show `<img src="*.svg">` and so tools can rasterize icons without `resvg`/`usvg`.

* Version: `0.1.3`. Edition 2024. No features. `unsafe_code = "deny"`; none present.
* Two public functions and one error enum. It is a **subset** renderer: no text, gradients, filters, masks, clip paths, markers, dashes, `evenodd`, or CSS in `<style>` (see [Supported SVG](supported.md)).
* Limits: 16 384 pixels per side and 64 Mi pixels (`TooLarge`), at most 100 000 drawn elements, XML nesting 64, `use` depth 8.

## Install

```toml
[dependencies]
quench-svg = { version = "0.1", registry = "ennor" }   # in the workspace: { workspace = true }
```

## Dependencies

| Crate | Why |
|---|---|
| `quench-css` | `tokenize`, `parse_values`, `parse_color`, `ColorValue` for colour syntax |
| `quench-image` | the `Image` type returned by `render` |
| `quench-raster` | `Canvas`, `Path`, `Color`: fills and strokes |
| `quench-test` (dev) | test harness |

## Where to go next

* [Usage](usage.md)
* [API reference](api.md)
* [Supported SVG](supported.md): every element, attribute and unit, and what is ignored.
* [Testing](testing.md)
* Guide: [Working with files and formats](../../guides/files-and-formats.md).

## Minimal example

```rust
let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10">
    <rect width="10" height="10" fill="#f00"/><circle cx="15" cy="5" r="4" fill="blue"/></svg>"##;
let image = quench_svg::render(svg, 1.0)?;
assert_eq!((image.width, image.height), (20, 10));
assert_eq!(&image.rgba[(2 * 20 + 2) * 4..][..4], &[255, 0, 0, 255]); // inside the red square
# Ok::<(), quench_svg::Error>(())
```

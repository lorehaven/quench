---
title: API reference
parent: quench-svg
nav_order: 2
---

The crate root exports `Error`, `size` and `render`. The `xml` module and everything else are private.

## `Error`

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Invalid(&'static str),   // the XML is not well formed (what is wrong with it)
    NotSvg,                  // the root element is not `svg`
    TooLarge,                // bigger than 16384 pixels a side or 64 million pixels
}
impl std::fmt::Display for Error;
impl std::error::Error for Error {}
```

Display: `Invalid(w)` -> `invalid SVG: {w}`; `NotSvg` -> `not an SVG file`; `TooLarge` -> `the picture is too large`.

`Invalid` reasons produced by the XML reader: `a comment is not closed`, `CDATA is not closed`, `a declaration is not closed`, `a doctype is not closed`, `a tag is not closed`, `a closing tag without an opening one`, `tags are not nested`, `elements are nested too deep`, `an element is not closed`, `a tag has no name`, `an attribute has no value`, `an attribute value is not quoted`, `an attribute value is not closed`, and a document with no root element.

## `size`

```rust
pub fn size(svg: &str) -> Result<(f32, f32), Error>
```

The natural size in CSS pixels. `width` and `height` attributes (not percentages, must be > 0) win; otherwise the `viewBox` width/height; otherwise **300 x 150**. Errors: `Invalid` (bad XML), `NotSvg`. Never `TooLarge` (the size is not checked here).

Units accepted in lengths: none or `px`, `pt` (x 4/3), `pc` (x 16), `mm` (x 96/25.4), `cm` (x 96/2.54), `in` (x 96), `em` (x 16, fixed), `%` (relative to a reference; not valid for the root `width`/`height`). An unknown unit makes the value unreadable (the default applies).

## `render`

```rust
pub fn render(svg: &str, scale: f32) -> Result<quench_image::Image, Error>
```

Draws at `scale` device pixels per CSS pixel on a transparent background. If `scale` is not finite or not > 0, `1.0` is used. Output size: `ceil(w * scale)` by `ceil(h * scale)`, each at least 1; `TooLarge` when a side exceeds 16 384 or the product exceeds `64 * 1024 * 1024`. A `viewBox` (4 numbers, positive width and height) is fitted into the natural size, "meet", centred (any `preserveAspectRatio` value is ignored; it is always meet + centre). The canvas is premultiplied BGRA internally and converted to straight RGBA on the way out (a pixel with alpha 0 becomes 0,0,0,0).

`quench_image::Image` is `{ width: u32, height: u32, rgba: Vec<u8> }` (see [quench-image](../quench-image/api.md)).

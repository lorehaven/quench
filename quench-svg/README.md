# quench-svg
An SVG renderer with no dependencies on external renderers: shapes, paths, transforms, fills and strokes into RGBA.

## What it is
`quench-svg` draws SVG: a document string in, straight-alpha RGBA pixels out (a `quench_image::Image`), through the software rasteriser `quench-raster`. It exists so the native renderer (`quench-ui-native`) can show `<img src="*.svg">` and so tools can rasterize icons without `resvg`/`usvg`.

It is a subset renderer: no text, gradients, filters, masks, clip paths, markers, dashes, `evenodd`, or CSS in `<style>`. It has two public functions and one error enum, with limits on image size and element count.

## Use cases
- Rasterize icons at a given device density.
- Save an SVG as a PNG together with `quench-image`.
- Render the SVG produced by `quench-qr`.
- Read the intrinsic size of an SVG with `size`.
- Reject files that are not SVG or are too large.
- Fit a `viewBox` into a width and height (meet and centre).

## Example
The result is `ceil(width * scale)` by `ceil(height * scale)` pixels on a transparent background.

```rust
let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24">
  <path d="M12 2 L22 22 L2 22 Z" fill="#2a7"/></svg>"##;

let (w, h) = quench_svg::size(svg)?;            // (24.0, 24.0) CSS pixels
let image = quench_svg::render(svg, 2.0)?;      // 2 device pixels per CSS pixel
assert_eq!((image.width, image.height), (48, 48));
# Ok::<(), quench_svg::Error>(())
```

```rust
use quench_svg::Error;
assert_eq!(quench_svg::render("<html/>", 1.0).unwrap_err(), Error::NotSvg);
```

## Documentation
Full documentation: https://lorehaven.github.io/quench/crates/quench-svg/

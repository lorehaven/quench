---
title: Usage
parent: quench-svg
nav_order: 1
---

## 1. Render at a density

```rust
let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24">
  <path d="M12 2 L22 22 L2 22 Z" fill="#2a7"/></svg>"##;

let (w, h) = quench_svg::size(svg)?;            // (24.0, 24.0) CSS pixels
let image = quench_svg::render(svg, 2.0)?;      // 2 device pixels per CSS pixel
assert_eq!((image.width, image.height), (48, 48));
# Ok::<(), quench_svg::Error>(())
```

The result is `ceil(width * scale)` by `ceil(height * scale)` pixels, minimum 1 by 1, on a transparent background.

## 2. Save the rendering as a PNG

```rust
let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"><rect width="8" height="8" fill="red"/></svg>"##;
let image = quench_svg::render(svg, 1.0)?;
let png = quench_image::encode_png(image.width, image.height, &image.rgba);
std::fs::write("icon.png", png)?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

## 3. Render a QR code SVG

```rust
let svg = quench_qr::QrCode::new("https://example.com/")?.to_svg(300);
let image = quench_svg::render(&svg, 1.0)?;        // the QR SVG uses only <rect> and <path>
assert!(image.width >= 300);
# Ok::<(), Box<dyn std::error::Error>>(())
```

Both crates are built to meet: `quench-qr` emits a `<path>` of `M h v h z` runs plus a white `<rect>`; `quench-svg` fills it non-zero.

## 4. Fit a view box into a size

```rust
let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="50" viewBox="0 0 10 10">
  <rect width="10" height="10" fill="blue"/></svg>"#;
let image = quench_svg::render(svg, 1.0)?;
// meet + centre: the 10x10 box becomes 50x50 and sits in the middle of 100x50
assert_eq!(image.rgba[(25 * 100 + 50) * 4 + 2], 255);       // blue in the middle
assert_eq!(image.rgba[(25 * 100 + 5) * 4 + 3], 0);          // transparent at the left edge
# Ok::<(), quench_svg::Error>(())
```

## 5. Reject files that are not SVG

```rust
use quench_svg::Error;
assert_eq!(quench_svg::render("<html/>", 1.0).unwrap_err(), Error::NotSvg);
assert!(matches!(quench_svg::render("<svg", 1.0), Err(Error::Invalid(_))));
assert_eq!(
    quench_svg::render(r#"<svg width="99999" height="10"/>"#, 1.0).unwrap_err(),
    Error::TooLarge
);
```

Pitfalls: a `scale` that is NaN, infinite or not positive is silently replaced by `1.0`. Elements that are not drawn (text, `defs`, `style`, `title`) are skipped without error, so a file that depends on them renders incompletely rather than failing.

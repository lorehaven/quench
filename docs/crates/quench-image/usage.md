---
title: Usage
parent: quench-image
nav_order: 1
---

## 1. Decode whatever arrives

```rust
use quench_image::{decode, Error};

fn load(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), Error> {
    let img = decode(bytes)?;           // PNG, JPEG, GIF or WebP, by signature
    Ok((img.width, img.height, img.rgba))
}

match decode(b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>") {
    Err(Error::Unsupported(name)) => assert_eq!(name, "SVG"),   // named, not decoded: use quench-svg
    other => panic!("{other:?}"),
}
```

The pixel buffer is `width * height * 4` bytes: R, G, B, A per pixel, rows top to bottom, **not** premultiplied.

## 2. Handle untrusted input

```rust
use quench_image::{decode, Error};

fn thumbnail_source(bytes: &[u8]) -> Option<quench_image::Image> {
    match decode(bytes) {
        Ok(img) => Some(img),
        Err(Error::TooLarge) => None,        // over 16384 px a side or 64 Mi pixels
        Err(Error::Corrupt(_)) | Err(Error::Unsupported(_)) => None,
    }
}
```

Dimensions are checked from the headers *before* pixel buffers are allocated, so a tiny file cannot request more than 64 Mi pixels (256 MiB of RGBA).

## 3. Write a PNG

```rust
let rgba = [255u8, 0, 0, 255,  0, 255, 0, 255];     // two pixels
let png = quench_image::encode_png(2, 1, &rgba);
let back = quench_image::decode(&png)?;
assert_eq!((back.width, back.height, back.rgba.as_slice()), (2, 1, &rgba[..]));
# Ok::<(), quench_image::Error>(())
```

`encode_png` panics when `rgba.len() != width * height * 4` (documented `# Panics`). The encoder does not filter rows, so files are valid but not small.

## 4. Rasterize an SVG, then save it

```rust
let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"><rect width="8" height="8" fill="red"/></svg>"##;
let img = quench_svg::render(svg, 2.0)?;                 // a quench_image::Image, 16 x 16
let png = quench_image::encode_png(img.width, img.height, &img.rgba);
assert_eq!(quench_image::decode(&png)?, img);
# Ok::<(), Box<dyn std::error::Error>>(())
```

`quench_svg::render` returns this crate's `Image`; see [quench-svg](../quench-svg/api.md).

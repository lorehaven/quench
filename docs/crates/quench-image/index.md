---
title: quench-image
parent: Crates
nav_order: 26
has_children: true
permalink: /crates/quench-image/
---

`quench-image` (crate `quench_image`) decodes raster images to straight-alpha RGBA and encodes PNG, with one dependency: [quench-archive](../quench-archive/index.md) (inflate, deflate, CRC-32, Adler-32). It replaces the `image`, `png`, `jpeg-decoder`, `gif` and `webp` crates for the [native renderer](../quench-ui-native/index.md)'s `<img>`. Vector drawings are [quench-svg](../quench-svg/index.md).

* Version: `0.1.0`. Edition 2024. No features. `unsafe_code = "deny"`; none present.
* Formats read, chosen by the first bytes: **PNG**, **JPEG**, **GIF**, **WebP** (lossless VP8L, lossy VP8 key frames, alpha, extended container). Formats written: **PNG** (8-bit RGBA only).
* Limits: 16 384 pixels per side and 64 Mi pixels (`64 << 20 = 67 108 864`) in total, otherwise `Error::TooLarge`.
* Damaged input is an `Error`, never a panic (the tests cut and flip bytes of real files).

> The crate's own `lib.rs` documentation and its `Cargo.toml` description still say JPEG and GIF are not read and that only PNG is decoded. The code decodes all four formats (see the gap report). This book follows the code.

## Install

```toml
[dependencies]
quench-image = { version = "0.1", registry = "ennor" }   # in the workspace: { workspace = true }
```

## Dependencies

| Crate | Why |
|---|---|
| `quench-archive` | `deflate::{decompress, compress}`, `checksum::{crc32, adler32}` for PNG |
| `quench-test` (dev) | test harness |

## Where to go next

* [Usage](usage.md)
* [API reference](api.md)
* [Formats](formats.md): exactly what each decoder supports and rejects.
* [Testing](testing.md)
* Guide: [Working with files and formats](../../guides/files-and-formats.md).

## Minimal example

```rust
let png_bytes = std::fs::read("photo.jpg")?;
let image = quench_image::decode(&png_bytes)?;       // Image { width, height, rgba }
assert_eq!(image.rgba.len(), image.width as usize * image.height as usize * 4);
let png = quench_image::encode_png(image.width, image.height, &image.rgba);
std::fs::write("photo.png", png)?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

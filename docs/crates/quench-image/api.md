---
title: API reference
parent: quench-image
nav_order: 2
---

The crate root exports everything; the codec modules (`gif`, `jpeg`, `vp8`, `vp8_tables`, `webp`) are private.

## `Image`

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,    // width * height * 4 bytes, straight (not premultiplied) alpha
}
```

Row-major, top row first. Fields are public; `rgba.len()` equals `width * height * 4` for every value `decode` returns.

## `Error`

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Unsupported(&'static str),   // not a format this crate reads (the name of what it looks like, when known)
    Corrupt(&'static str),       // damaged or hostile
    TooLarge,                    // beyond 16384 px a side or 64 Mi pixels
}
impl fmt::Display for Error;
impl std::error::Error for Error {}
```

Display texts: `Unsupported(w)` -> `{w} images are not supported`; `Corrupt(w)` -> `damaged image: {w}`; `TooLarge` -> `the image is too large`.

Values of `Unsupported` produced: `"SVG"` (input starts with `<svg` or `<?xml`), `"unknown"` (anything else with no known signature), `"JPEG with other than 8 bits"`, `"JPEG with 4 colour channels"`, `"JPEG (arithmetic or lossless)"`, `"WebP frames that are not key frames"`. (The branch that would name `"WebP"` in the fall-through is unreachable: WebP files are decoded earlier.)

`Corrupt` messages include (PNG) `a chunk runs past the end`, `a chunk's checksum is wrong`, `the header has the wrong size`, `an image with no pixels`, `a colour type, depth or method that PNG does not have`, `a palette of the wrong size`, `no header`, `the image data is missing`, `a palette image with no palette`, `the compressed data is not zlib`, `the compressed data is damaged`, `the image data is the wrong size`, `an unknown row filter`; GIF, JPEG and WebP use `the data is damaged`.

## `decode`

```rust
pub fn decode(bytes: &[u8]) -> Result<Image, Error>
```

Dispatches on the first bytes: `89 50 4E 47 0D 0A 1A 0A` PNG; `FF D8` JPEG; `GIF8` GIF; `RIFF....WEBP` WebP. Everything else is `Unsupported("SVG")` or `Unsupported("unknown")`. Pure function; no I/O; `Send + Sync` result.

## `encode_png`

```rust
#[must_use]
pub fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Vec<u8>
```

Writes an 8-bit RGBA (colour type 6) PNG: signature, `IHDR` (bit depth 8, colour type 6, compression 0, filter 0, interlace 0), one `IDAT` holding a zlib stream (`78 9C`, raw deflate from `quench_archive::deflate::compress`, big-endian Adler-32), `IEND`. Every scanline uses filter type 0 (none). No ancillary chunks. **Panics** when `rgba.len() != width * height * 4` (message "an RGBA image is width * height * 4 bytes"). There is no size limit check on encode.

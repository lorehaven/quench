---
title: API reference
parent: quench-qr
nav_order: 2
---

The crate root (`lib.rs`) exports three items. The modules `galois`, `layout` and `tables` are private.

## `EcLevel` (re-exported from the private `tables` module)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EcLevel {
    L,  // about 7 % of the symbol can be lost
    M,  // about 15 %
    Q,  // about 25 %
    H,  // about 30 %
}
```

The two format-information bits for the level are `L=01, M=00, Q=11, H=10` (internal). No `Hash`, `Ord` or `Default` impl.

## `Error`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// More bytes than version 40 holds at the chosen level (`max` bytes).
    TooLong { len: usize, max: usize },
}
```

`Display`: `{len} bytes do not fit a QR code at this error-correction level (at most {max})`. Implements `std::error::Error`. `max` is `QrCode::capacity(40, level)`.

## `QrCode`

```rust
#[derive(Clone, PartialEq, Eq)]
pub struct QrCode { /* version: u8, level: EcLevel, mask: u8, size: usize, modules: Vec<bool> */ }
```

`Debug` prints `QrCode { version, level, mask, .. }` (not the modules). The type is `Send + Sync`.

```rust
impl QrCode {
    pub fn new(data: impl AsRef<[u8]>) -> Result<Self, Error>;
    pub fn with_level(data: impl AsRef<[u8]>, level: EcLevel) -> Result<Self, Error>;
    #[must_use] pub const fn capacity(version: u8, level: EcLevel) -> usize;
    #[must_use] pub const fn size(&self) -> usize;
    #[must_use] pub const fn version(&self) -> u8;
    #[must_use] pub const fn level(&self) -> EcLevel;
    #[must_use] pub const fn mask(&self) -> u8;
    #[must_use] pub fn is_dark(&self, x: usize, y: usize) -> bool;
    #[must_use] pub fn to_text(&self) -> String;
    #[must_use] pub fn to_svg(&self, min_pixels: u32) -> String;
}
```

* `new(data)`: `with_level(data, EcLevel::M)`.
* `with_level(data, level)`: picks the smallest version 1..=40 whose byte capacity at `level` holds `data.len()`; `Error::TooLong` if none does. Empty data is valid (version 1). Deterministic: same input, same symbol.
* `capacity(version, level)`: the number of data bytes of a version in byte mode: `(data_codewords * 8 - 4 - count_bits) / 8` with `count_bits` 8 for versions below 10, else 16. Valid for `version` 1 to 40; **other values are not checked**: version 0 returns a meaningless `6` (checked while writing this page) and any version above 40 panics with an index out of bounds. Version 40: L 2953, M 2331, Q 1663, H 1273. Version 1: L 17, M 14, H 7.
* `size()`: modules per side, `4 * version + 17` (21 to 177).
* `version()`: 1 to 40. `level()`: the level used. `mask()`: the chosen mask pattern 0 to 7.
* `is_dark(x, y)`: column `x`, row `y`, origin top-left; `false` outside the symbol (no panic).
* `to_text()`: `size` lines each ending in `\n`; per module `"##"` (dark) or two spaces (light). No quiet zone.
* `to_svg(min_pixels)`: a standalone SVG document string (see [internals](internals.md#svg-output) for the exact shape). The size is `(size + 8) * scale` pixels where `scale = max(1, ceil(min_pixels / (size + 8)))`, so the result is at least `min_pixels` wide and tall (more than that when it does not divide evenly) and every module is a whole number of pixels. `min_pixels = 0` gives scale 1.

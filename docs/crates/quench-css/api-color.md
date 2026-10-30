---
title: "API: color"
parent: "quench-css"
nav_order: 2
---

Module `color`. Syntaxes and named colours are tabulated on [Supported CSS](supported.md#colours).

## `Color`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color { pub r: u8, pub g: u8, pub b: u8, pub a: u8 }
```

An sRGB colour with straight (non-premultiplied) alpha, 0 to 255.

| Item | Signature | Notes |
|---|---|---|
| `Color::TRANSPARENT` | `pub const TRANSPARENT: Self` | `0, 0, 0, 0` |
| `Color::BLACK` | `pub const BLACK: Self` | `0, 0, 0, 255` |
| `Color::WHITE` | `pub const WHITE: Self` | `255, 255, 255, 255` |
| `Color::rgba` | `#[must_use] pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self` | |
| `Color::with_opacity` | `#[must_use] pub fn with_opacity(self, opacity: f32) -> Self` | Alpha scaled by `opacity.clamp(0.0, 1.0)`, rounded. |

## `ColorValue`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorValue { Color(Color), CurrentColor }
```

A colour, or `currentcolor` (the element's own `color`, known only after the cascade).

```rust
#[must_use]
pub const fn resolve(self, current: Color) -> Color
```

`Color(c)` gives `c`, `CurrentColor` gives `current`. `ComputedStyle::resolve` does the same with the style's `color`.

## `parse_color`

```rust
#[must_use]
pub fn parse_color(value: &Value) -> Option<ColorValue>
```

A colour from one component value, or `None`. Takes `Hash`, `Ident` or `Function` values only.

```rust
use quench_css::{Color, ColorValue, parse_color, parse_values, tokenize};

let v = parse_values(&tokenize("rgb(0 128 255 / 50%)"));
assert_eq!(
    parse_color(&v[0]),
    Some(ColorValue::Color(Color::rgba(0, 128, 255, 128)))
);
```

Channel handling, from the code:

* Hex: 3, 4, 6 or 8 digits; a short digit `d` becomes `d * 17`. Other lengths and non-hex digits fail.
* `rgb()`/`rgba()`: 3 or 4 components after removing commas and `/` (so comma and space syntax, and even mixtures, are all accepted). Each of r, g, b is a bare number or a percentage (`p / 100 * 255`), clamped to 0..255, rounded. Alpha is a number or percentage, clamped to 0..1, rounded after `* 255`. Other component kinds (`none`, dimensions) fail.
* `hsl()`/`hsla()`: 3 or 4 components. Hue: a bare number or `deg` (other angle units fail), wrapped modulo 360. Saturation and lightness must be percentages (clamped 0..1). Alpha as in `rgb`.
* `color-mix(in <space>, A [p%], B [p%])`: see [Supported](supported.md#colours).

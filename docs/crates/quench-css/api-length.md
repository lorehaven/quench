---
title: "API: length"
parent: "quench-css"
nav_order: 3
---

Module `length`: lengths and `calc()`-family expressions. Units and functions are tabulated on [Supported CSS](supported.md#units).

## `Units`

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Units {
    pub font_size: f32,
    pub root_font_size: f32,
    pub viewport_width: f32,
    pub viewport_height: f32,
    pub zoom: f32,
}
```

What a unit is measured against: `em` is `font_size`, `rem` is `root_font_size`, viewport units use the viewport size, absolute units are multiplied by `zoom` (device pixels per CSS pixel). `Default` is `16, 16, 1280, 800, 1.0`. The cascade builds it from `Cascade::media` and `zoom` (`viewport_width = media.width * zoom`).

## `Length`

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum Length { Px(f32), Percent(f32), Calc(Rc<Calc>) }
```

A length as the cascade leaves it: percentages (and expressions mixing them with pixels) wait for layout. `Calc` holds an expression only when it contains a percentage; an expression without one is folded to `Px` at parse time. `!Send`.

| Item | Signature | Behaviour |
|---|---|---|
| `Length::ZERO` | `pub const ZERO: Self` | `Px(0.0)` |
| `resolve` | `#[must_use] pub fn resolve(&self, basis: f32) -> f32` | Pixels given what 100% is: `Percent(p)` is `basis * p / 100`. |
| `as_px` | `#[must_use] pub fn as_px(&self) -> Option<f32>` | The pixel value when no percentage is involved. |
| `is_relative` | `#[must_use] pub fn is_relative(&self) -> bool` | `as_px().is_none()`. |

## `Calc`

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum Calc {
    Px(f32), Percent(f32),
    Add(Rc<Calc>, Rc<Calc>), Sub(Rc<Calc>, Rc<Calc>),
    Mul(Rc<Calc>, f32),
    Min(Vec<Calc>), Max(Vec<Calc>),
}
```

An expression over lengths. Division by a number is `Mul(x, 1/k)`. `clamp(a, b, c)` is `Max([a, Min([b, c])])`.

* `pub fn resolve(&self, basis: f32) -> f32` (`#[must_use]`): evaluates with `Percent(p) = basis * p / 100`. `Min` folds with `f32::min` from `INFINITY`, `Max` from `NEG_INFINITY` (an empty list, which the parser never builds, gives those).
* `pub fn constant(&self) -> Option<f32>` (`#[must_use]`): the value when no percentage occurs.

## `parse_length`

```rust
#[must_use]
pub fn parse_length(value: &Value, units: &Units) -> Option<Length>
```

One length from one component:

* `Dimension(n, unit)` with a length unit: `Px` ([units table](supported.md#units)); any other unit (`fr`, `s`, `deg`) gives `None`.
* `Percent(p)`: `Length::Percent(p)`.
* `Number(0)`: `Length::ZERO`. Any other bare number: `None`.
* `Function` named `calc`, `min`, `max`, `clamp`: parsed, folded to `Px`/`Percent` when possible, otherwise `Length::Calc`.
* Anything else: `None`.

```rust
use quench_css::{Length, Units, parse_length, parse_values, tokenize};

let v = parse_values(&tokenize("calc(100% - 2rem)"));
let len = parse_length(&v[0], &Units::default()).unwrap();
assert!(len.is_relative());
assert_eq!(len.resolve(500.0), 468.0);
assert_eq!(parse_length(&parse_values(&tokenize("2em"))[0], &Units::default()), Some(Length::Px(32.0)));
```

---
title: "API: media"
parent: "quench-css"
nav_order: 4
---

Module `media`: `@media` preludes and their evaluation. The supported features are tabulated on [Supported CSS](supported.md#media-queries).

## `ColorScheme`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorScheme { Light, Dark }
```

## `Pointer`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pointer { None, Coarse, Fine }
```

## `MediaEnv`

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MediaEnv {
    pub width: f32,
    pub height: f32,
    pub color_scheme: ColorScheme,
    pub reduced_motion: bool,
    pub hover: bool,
    pub pointer: Pointer,
}
```

The environment a query is asked about (`width`/`height` in CSS pixels). `Default`: `1280.0 x 800.0`, `ColorScheme::Dark`, `reduced_motion: false`, `hover: true`, `Pointer::Fine`.

## `MediaQuery`

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct MediaQuery { /* private fields */ }
```

One query of a comma list: an optional media type, conditions that must all hold, possibly negated. Fields are private; build with `parse_media_queries`.

```rust
#[must_use]
pub fn matches(&self, env: &MediaEnv) -> bool
```

True when the media type is absent, `all` or `screen` (`print` and `speech` never match) and every condition holds, XOR the query was negated with a leading `not`.

## `parse_media_queries`

```rust
#[must_use]
pub fn parse_media_queries(prelude: &[Value]) -> Vec<MediaQuery>
```

One `MediaQuery` per comma-separated part. Within a part: `not` at the first position negates; `only` and `and` are skipped; `all`, `screen`, `print`, `speech` set the type; a `( ... )` block is a condition; anything else (another identifier, `not` in the middle, a function, a stray token) makes the whole part malformed, and a malformed part becomes a query that never matches (`not all`). An empty prelude gives one part with no type and no conditions, which matches always.

## `all_match`

```rust
#[must_use]
pub fn all_match(levels: &[Vec<MediaQuery>], env: &MediaEnv) -> bool
```

`Rule::media` semantics: every level must hold, and a level holds when any of its queries matches. An empty `levels` holds.

```rust
use quench_css::{MediaEnv, all_match, parse_media_queries, parse_values, tokenize};

let q = parse_media_queries(&parse_values(&tokenize("(min-width: 600px) and (hover: hover)")));
assert!(all_match(&[q], &MediaEnv::default())); // 1280 wide, hover true
```

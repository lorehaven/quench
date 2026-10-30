---
title: "API: properties"
parent: "quench-css"
nav_order: 5
---

Module `properties`: the property lists. The exact meaning of each property is on the [Supported CSS](supported.md) page.

## `SUPPORTED`

```rust
pub const SUPPORTED: &[&str]
```

The 116 properties the engine understands (changes layout or drawing). Names are lowercase. The list is in `src/properties.rs` order: `display`, `position`, `top`, `right`, `bottom`, `left`, `inset`, `z-index`, `width`, `height`, `min-width`, `max-width`, `min-height`, `max-height`, `box-sizing`, the `margin*`, `padding*` and `border*` families, `border-radius`, `outline*`, `background*`, `opacity`, `box-shadow`, `overflow`, `overflow-x`, `overflow-y`, the flex properties, `gap`, `row-gap`, `column-gap`, the grid properties, `transition*`, `animation*`, `color`, `font*`, `line-height`, `text-align`, `text-transform`, `list-style`, `visibility`, `touch-action`, `transform`, `rotate`. [Supported CSS](supported.md#properties) lists every entry.

## `IGNORED`

```rust
pub const IGNORED: &[&str]
```

Properties that are accepted and do nothing: `cursor`, `user-select`, `will-change`, `overscroll-behavior`, `overscroll-behavior-x`, `overscroll-behavior-y`, `overflow-anchor`, `scrollbar-width`, `scrollbar-color`, `accent-color`, `-webkit-tap-highlight-color`, `-webkit-touch-callout`, `-webkit-overflow-scrolling`. Their values are not validated.

## `is_known`

```rust
#[must_use]
pub fn is_known(name: &str) -> bool
```

`true` for a custom property (`name.starts_with("--")`), a name in `SUPPORTED`, or a name in `IGNORED`. The comparison is case-sensitive: pass a lowercase name. The parser uses it to fill `Stylesheet::ignored` (`"property <name>"`) and `@supports (<name>: ...)` uses it for its truth value.

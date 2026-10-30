---
title: Usage
parent: quench-css
nav_order: 1
---

Task-oriented examples. All of them use an `Element` implementation like the one on the [overview page](index.md#minimal-example); `El` below stands for it.

## 1. Compute the style of a tree

Goal: a `ComputedStyle` for every element. The cascade needs the parent's computed style, so walk the tree from the root.

```rust
use quench_css::{Cascade, ComputedStyle, MediaEnv, parse};

fn compute_chain(cascade: &Cascade, chain_root_first: &[El]) -> ComputedStyle {
    let mut style: Option<ComputedStyle> = None;
    for element in chain_root_first {
        style = Some(cascade.compute(element, style.as_ref(), 16.0));
    }
    style.expect("a non-empty chain")
}
```

`compute(element, parent, root_font_size)` takes `None` as the parent only for the root; the root then starts from a default style whose font size is `16.0 * cascade.zoom`. `root_font_size` is what `rem` means, in device pixels (it is not multiplied by the zoom again). Pitfall: computing a child without its parent's style loses inheritance of `color`, the font properties and custom properties.

## 2. Several sheets, in order

```rust
let mut cascade = Cascade::new(MediaEnv::default());
cascade.add(&parse(base_css));
cascade.add(&parse(theme_css));
```

Rules of a later sheet come later in source order. Pitfall: see [Internals, sheet order](internals.md#order-across-sheets): the offset applied to the second sheet is the number of rules already added, which can be smaller than the highest order used by the first sheet when that sheet has empty rules; a rule in the later sheet then loses an equal-specificity tie it should win. Keep tie-breaking out of the sheet boundary (use higher specificity) until this is fixed.

## 3. Media environments

`MediaEnv` holds the width, height, colour scheme, reduced-motion flag, hover capability and pointer of the window. Media queries are evaluated when a style is computed, against `Cascade::media`:

```rust
use quench_css::{ColorScheme, MediaEnv, Pointer};

let phone = MediaEnv {
    width: 390.0,
    height: 844.0,
    color_scheme: ColorScheme::Light,
    reduced_motion: false,
    hover: false,
    pointer: Pointer::Coarse,
};
let mut cascade = quench_css::Cascade::new(phone);
cascade.zoom = 2.6;                         // device pixels per CSS pixel
cascade.safe_area = [88.0, 0.0, 34.0, 0.0]; // env(safe-area-inset-*), device pixels
```

When the window resizes, set `cascade.media` again and recompute: the cascade keeps the rules and evaluates `@media` per `compute` call. `zoom` multiplies absolute lengths (`px`, `pt`, `in`, ...) and the font-size keywords, not `em`/`rem`/`%`/viewport units; the viewport units use `media.width * zoom`.

## 4. An element's `style` attribute

```rust
use quench_css::parse_declarations;

let inline = parse_declarations("color: red; width: 10px");
let style = cascade.compute_with(&element, parent.as_ref(), 16.0, &inline);
```

Inline declarations beat every rule; an inline `!important` beats an `!important` in a rule. `parse_declarations` never fails; a declaration it cannot read is dropped.

## 5. Custom properties and `var()`

```css
:root { --accent: #3b82f6; --pad: 8px }
button { background: var(--accent); padding: var(--pad, 4px) calc(var(--pad) * 2) }
```

Custom properties inherit and are stored (in `ComputedStyle::custom`) with their `var()` already substituted, as lists of `Value` components. A property whose value uses a missing variable without a fallback is not applied at all (it does not become `initial`). Pitfalls: names are case-sensitive; a fallback is everything after the first comma, commas included; a cycle (or nesting deeper than 16 levels) removes the custom property.

## 6. Check what an engine would ignore

```rust
let sheet = quench_css::parse("p { color: red; text-decoration: underline } @font-face { font-family: x }");
assert!(sheet.ignored.iter().any(|i| i == "property text-decoration"));
assert!(sheet.ignored.iter().any(|i| i == "@font-face"));
```

`Stylesheet::ignored` lists, as strings, selectors, at-rules, nesting and properties the engine dropped or does not know ([formats](api-sheet.md#stylesheet)). `quench_css::accepts("width", &values)` tells whether a *known* property accepts a value (after `var()`/`env()` substitution).

## 7. Animations and transitions

`animation` and `transition` (and their longhands) become `ComputedStyle::animations` and `::transitions`. The cascade does not play them. To get the style at a point of an animation, look up the keyframes and apply a frame's declarations on top of the element's base style:

```rust
let kf = cascade.keyframes("fade").expect("declared");
let (_offset, declarations) = &kf.frames[0];
let at_frame = cascade.style_with(&base_style, &parent_style, 16.0, declarations);
```

`style_with` substitutes `var()` from `base_style.custom` and applies each non-custom declaration. Interpolation between frames is the renderer's job ([quench-ui-native](../quench-ui-native/index.md)).

## 8. Matching a selector without the cascade

```rust
use quench_css::{parse_selector_list, tokenize};

let list = parse_selector_list(&tokenize("ul > li:first-child, a[href^='https']"));
assert_eq!(list.len(), 2);
let specificity = list[0].specificity();      // (0 << 16) | (1 << 8) | 2
let hit = list[0].matches(&element);
```

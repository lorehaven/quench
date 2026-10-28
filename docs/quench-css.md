# Quench CSS

`quench-css` (crate `quench_css`) is a CSS engine with no dependencies: a tokenizer and parser (with **nesting**, `@media`, `@supports`, `@keyframes`), selectors that match any tree that implements `Element`, the **cascade** with inheritance and **custom properties**, and typed computed values (lengths with `calc()`/`min()`/`max()`/`clamp()`, colours with `color-mix()`). It is the third piece of the [native renderer](../plans/NATIVE_RENDERER.md): the same stylesheets [`quench-style`](./quench-style.md) emits for the browser are what a native window reads.

```rust
use quench_css::{Cascade, MediaEnv, parse};

let mut cascade = Cascade::new(MediaEnv::default()); // 1280 x 800, dark, mouse
cascade.add(&parse(&quench_style::components::stylesheet()));
let style = cascade.compute(&element, parent_style.as_ref(), 16.0); // ComputedStyle
```

## What it reads

- **Syntax**: comments, strings, numbers with units, hashes, functions, at-rules, `!important`. Broken input is skipped to the next `;` or `}`, as a browser does, never fatal. Nesting is resolved while parsing: `&`, `&:hover`, `&.on .x`, nested rules without `&` (a descendant, or keeping a leading `>`/`+`), and `@media`/`@supports` inside a rule.
- **Selectors**: type, `*`, `.class`, `#id`, attributes (`=`, `~=`, `|=`, `^=`, `$=`, `*=`, `i`), descendant/child/adjacent/sibling combinators, `:not()`, `:root`, `:first-child`, and the states `:hover`, `:focus`, `:focus-visible`, `:active`, `:disabled`, `:enabled`, `:checked` (asked of the tree through `Element::state`). Anything else, and every pseudo-element, never matches.
- **At-rules**: `@media` (width/height min/max and range syntax, `prefers-color-scheme`, `prefers-reduced-motion`, `hover`, `pointer`, `orientation`; `and`, `not`, commas) is kept on each rule and evaluated against a `MediaEnv` when a style is computed; `@supports` is evaluated while parsing (`(property: value)` is true for properties this engine supports; `not`/`and`/`or`); `@keyframes` are stored; other at-rules are listed in `Stylesheet::ignored`.
- **Values**: `var(--x, fallback)` (resolved where the custom property is declared; a cycle or a missing variable without a fallback drops the declaration), `env(safe-area-inset-*)` from `Cascade::safe_area` (0 by default), units `px em rem ex ch vw vh dvh svh lvh vmin vmax pt pc in cm mm`, percentages, colours as hex, `rgb()/rgba()` in both syntaxes, `hsl()`, names, `transparent`, `currentcolor`, `color-mix(in srgb, …)`.

## The cascade

`Cascade` holds the rules of the sheets added, in order. `compute(element, parent_style, root_font_size)` collects the matching rules whose media hold, orders their declarations by importance, specificity and position, and applies them to a style that starts from what the parent passes on: `color`, the font properties, `line-height`, `text-align`, `text-transform`, `list-style`, `visibility` and the custom properties. Font size is settled first (`em` in `font-size` is the parent's, in every other property the element's). Shorthands expand: `margin`, `padding`, `inset`, `border` (and each side), `border-radius`, `background` (the colour layer; images are not drawn), `flex`, `font`, `gap`, `overflow`, `outline`, `box-shadow`, `transform` and `rotate`. A value that does not parse leaves the style as it was. A border or outline without a style has no width, and an absolutely positioned or fixed box is a block.

`ComputedStyle` is typed: `Display`, `Position`, `Length` (pixels, a percentage, or a `Calc` tree resolved with `length.resolve(basis)` once layout knows the box), `ColorValue` (a colour or `currentcolor`, resolved with `style.resolve(…)`), shadows, a folded `Transform`, flex properties and so on.

## The subset

`SUPPORTED` and `IGNORED` list the properties the engine understands and those it accepts and leaves without effect (`cursor`, `transition`, `animation`, `scrollbar-*` and the like). The test suite parses every stylesheet `quench-style` emits and fails on a property in neither list or a supported property whose value the engine refuses, so the subset cannot drift from the CSS the framework ships.

Not supported: grid, floats, tables, `calc()` inside colours, `@font-face`, container queries, `:is()`/`:where()`/`:has()`, pseudo-elements (so `::before` content), ordering of mixed `transform` functions (translations add, scales multiply, angles add), animations (keyframes are parsed and kept; nothing plays them).

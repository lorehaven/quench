# quench-css
A CSS engine with no dependencies: parser, selectors, the cascade, custom properties, calc() and media queries.

## What it is
`quench-css` tokenizes and parses stylesheets (with nesting, `@media`, `@supports`, `@keyframes`), matches selectors against any tree that implements the `Element` trait, runs the cascade with inheritance and custom properties, and returns a typed `ComputedStyle` per element. It uses `std` only.

It exists so the native renderer can read the same stylesheets that `quench-style` emits for the browser, without a WebView or a third-party CSS crate. `quench-layout` and `quench-paint` read `ComputedStyle`; `quench-ui-native` drives the pipeline.

## Use cases
- Compute the style of every element in a custom tree.
- Layer several stylesheets in the cascade.
- Evaluate `@media` queries against a window size, colour scheme or pointer type.
- Apply an element's inline `style` attribute with `parse_declarations`.
- Resolve `var()` custom properties and `calc()` values.

## Example
```rust
use quench_css::{Cascade, Color, Display, MediaEnv, parse};

// `El` implements `quench_css::Element`; `root` is a div and `p` a `p.lead` child.
let mut cascade = Cascade::new(MediaEnv::default());
cascade.add(&parse(
    ":root { --gap: 8px } div { font-size: 20px; color: navy } \
     p.lead { margin: var(--gap) 2em; display: flex }",
));
let root_style = cascade.compute(&root, None, 16.0);
let style = cascade.compute(&p, Some(&root_style), 16.0);

assert_eq!(style.display, Display::Flex);
assert_eq!(style.color, Color::rgba(0, 0, 128, 255)); // inherited from div
assert_eq!(style.font_size, 20.0);
```

```rust
use quench_css::{ColorScheme, MediaEnv, Pointer};

let phone = MediaEnv {
    width: 390.0, height: 844.0, color_scheme: ColorScheme::Light,
    reduced_motion: false, hover: false, pointer: Pointer::Coarse,
};
let cascade = quench_css::Cascade::new(phone);
```

## Documentation
Full documentation: https://lorehaven.github.io/quench/crates/quench-css/

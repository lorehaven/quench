---
title: quench-css
parent: Crates
nav_order: 15
has_children: true
---

`quench-css` (crate `quench_css`) is a CSS engine with no dependencies. It tokenizes and parses stylesheets (with nesting, `@media`, `@supports`, `@keyframes`), matches selectors against any tree that implements the `Element` trait, runs the cascade with inheritance and custom properties, and returns a typed `ComputedStyle` per element. It exists so that the native renderer can read the same stylesheets that `quench-style` emits for the browser, without a WebView or a third-party CSS crate. [quench-layout](../quench-layout/index.md) and [quench-paint](../quench-paint/index.md) read `ComputedStyle`; [quench-ui-native](../quench-ui-native/index.md) drives the pipeline.

* Version: 0.1.2. Edition 2024. No `rust-version` is set.
* Features: none (the crate has no `[features]` section).
* `unsafe`: none; the workspace sets `unsafe_code = "deny"`.
* Thread safety: `Stylesheet`, `Rule`, `Cascade`, `ComputedStyle`, `Length` and `Calc` hold `Rc` and are `!Send` and `!Sync`. `Color`, `MediaEnv`, `Token`, `Value` and the plain enums are `Send + Sync`.

```toml
[dependencies]
quench-css = "0.1"
```

## Dependencies

| Crate | Kind | Why |
|---|---|---|
| none | normal | The engine uses `std` only. |
| `quench-style` | dev | The test suite parses every stylesheet it emits and checks that the engine accepts each declaration. |
| `quench-test` | dev | The test harness (`harness = false`, `quench_test::main!()`). |

## Where to go next

* [Usage](usage.md): compute styles for a tree, media environments, inline styles, custom properties, animations.
* [Supported CSS](supported.md): the exact table of what is parsed, what is ignored, per selector, at-rule, property, unit, colour, function and media feature, and the cascade rules.
* API reference by module: [token](api-token.md), [value](api-value.md), [selector](api-selector.md), [sheet](api-sheet.md), [style](api-style.md) (cascade and `ComputedStyle`), [properties](api-properties.md), [color](api-color.md), [length](api-length.md), [media](api-media.md).
* [Internals](internals.md): cascade order, specificity encoding, inheritance, custom properties, `calc()`, nesting, error recovery, known quirks.
* [Testing](testing.md)

## Minimal example

```rust
use quench_css::{Cascade, Color, Display, Element, MediaEnv, State, parse};

#[derive(Clone)]
struct El {
    tag: &'static str,
    class: Option<&'static str>,
    parent: Option<Box<El>>,
}

impl Element for El {
    fn tag(&self) -> String { self.tag.into() }
    fn id(&self) -> Option<String> { None }
    fn has_class(&self, c: &str) -> bool { self.class == Some(c) }
    fn attribute(&self, _: &str) -> Option<String> { None }
    fn parent(&self) -> Option<Self> { self.parent.as_deref().cloned() }
    fn previous_sibling(&self) -> Option<Self> { None }
    fn state(&self, _: State) -> bool { false }
}

let root = El { tag: "div", class: None, parent: None };
let p = El { tag: "p", class: Some("lead"), parent: Some(Box::new(root.clone())) };

let mut cascade = Cascade::new(MediaEnv::default()); // 1280 x 800, dark, mouse
cascade.add(&parse(
    ":root { --gap: 8px } div { font-size: 20px; color: navy } \
     p.lead { margin: var(--gap) 2em; display: flex }",
));
let root_style = cascade.compute(&root, None, 16.0);
let style = cascade.compute(&p, Some(&root_style), 16.0);

assert_eq!(style.display, Display::Flex);
assert_eq!(style.color, Color::rgba(0, 0, 128, 255)); // inherited from div
assert_eq!(style.font_size, 20.0);                    // inherited
assert_eq!(style.margin[0].as_ref().unwrap().as_px(), Some(8.0));  // var(--gap)
assert_eq!(style.margin[1].as_ref().unwrap().as_px(), Some(40.0)); // 2em of 20px
```

Compiled and run against the crate while writing this page (the values in the assertions are what the run printed). The test suite builds its trees the same way (`tests/unit.rs`, `style_of`).

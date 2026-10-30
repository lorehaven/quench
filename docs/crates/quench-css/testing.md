---
title: Testing
parent: quench-css
nav_order: 13
---

## How the crate is tested

`quench-css/tests/unit.rs` (`[[test]] name = "unit"`, `harness = false`, ending in `quench_test::main!()`) holds 37 tests marked `#[quench_test::test]`. Run them with `cargo test -p quench-css`. The tests build a small tree (`Tree` of `Data` nodes, `Node` handles implementing `Element`), parse CSS strings and assert on `ComputedStyle` fields. The groups:

* Tokenizer, values, selectors (kinds, specificity, relations, interaction states, nesting), the cascade order (importance, specificity, position), inheritance, custom properties.
* Units (every unit, with and without `zoom`), `calc()`, colours (every notation and failure), shorthands and longhands for every supported property, text and `font` variants, transforms, shadows and borders, background images, grid tracks/lines/areas, transitions, animations and keyframes, `touch-action`.
* Media queries (all features, range forms, `not`, lists), `@supports`, unknown at-rules, broken CSS.
* The inline `style` attribute, `env()` and the safe area.
* `real_stylesheets_use_only_what_the_engine_knows`: parses every stylesheet `quench-style` emits (`tokens`, `mobile`, `components`, `desktop`, and the default dark and light themes), asserts that `Stylesheet::ignored` is empty, that every declaration name is in `SUPPORTED` or `IGNORED` (custom properties aside) and that `accepts` holds for every supported property without `var()`/`env()` (more than 100 checked). This keeps the subset in step with the CSS the framework ships: adding a property to `quench-style` without teaching the engine fails this test.
* `real_stylesheets_compute_a_button_that_looks_like_one`: computes a button through the real component CSS.

## Testing code built on the crate

Implement `Element` for a small arena-index handle and compute styles root-first, as `style_of` does in the test file:

```rust
fn style_of(css: &str, chain_root_first: &[El]) -> quench_css::ComputedStyle {
    let mut cascade = quench_css::Cascade::new(quench_css::MediaEnv::default());
    cascade.add(&quench_css::parse(css));
    let mut style = None;
    for el in chain_root_first {
        style = Some(cascade.compute(el, style.as_ref(), 16.0));
    }
    style.unwrap()
}
```

To check that your CSS stays inside the subset, assert `parse(css).ignored.is_empty()`, and `accepts(&d.name, &d.value)` for each declaration of properties in `SUPPORTED` (as the real-stylesheet test does). Remember `accepts` is `true` for any keyword on the [fallback properties](supported.md#value-fallbacks).

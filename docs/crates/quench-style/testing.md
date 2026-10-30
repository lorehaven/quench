---
title: Testing
parent: quench-style
nav_order: 6
---

* Harness: `tests/unit.rs` (`harness = false`) includes the files in `tests/unit/` as modules; the library itself also uses `quench_test::main!()`. Run with `cargo test -p quench-style`.
* Palettes: every theme defines the same tokens; dark and light ramps run in opposite directions; malformed hex is rejected. Tokens: unique `--q-` names, increasing spacing, 44px touch minimum, reduced-motion override. Mobile/desktop layers: namespacing, every desktop rule sits inside the pointer media query, no collision with the shared classes. Components: every pair in `TEXT_PAIRS` meets 4.5:1 and every pair in `EDGE_PAIRS` meets 3:1 in all four themes. Assets: written into a temp directory, including an unwritable root. i18n: only the "no `i18n/` directory" branches are reachable from the integration tests (the crate ships no `.ftl` files); the `.ftl` parser has inline tests.
* The old page states 98 % line coverage; that figure was not re-measured.
* To test your own stylesheet code, compare `CssRule::render()` output as strings (deterministic) and use `Palette::color` + `Color::contrast` for accessibility checks.

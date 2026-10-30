---
title: Testing
parent: quench-svg
nav_order: 4
---

* `tests/unit.rs` (`harness = false`, `quench_test::main!()`): shapes and colours; view box scaling, centring and density; strokes with widths, caps and joins; paths with lines, curves and arcs; transforms, groups, `use` and inheritance; every colour notation and gradients as their first stop; non-SVG and hostile files (`NotSvg`, malformed XML, `TooLarge` for a 99 999-pixel width and for a 4000x scale, 200 nested groups, a self-referencing `use`, wild numbers such as `1e999` and `nan`, entities); lengths in every unit and transforms in every form; smooth curves and arc flags without separators; the XML reader.
* The crate documentation contains a doctest (the 20x10 red square and blue circle).
* Run: `cargo test -p quench-svg`.
* To test code that renders SVG, compare single pixels of `Image::rgba` (the unit tests use a helper that reads `[r, g, b, a]` at `(x, y)`).

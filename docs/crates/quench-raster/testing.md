---
title: Testing
parent: quench-raster
nav_order: 4
---

`tests/unit.rs` (25 `#[quench_test::test]` functions, `harness = false`, run with `cargo test -p quench-raster`) checks: the canvas layout (BGRA, premultiplied, transparent outside), source-over arithmetic, clipping for every kind of drawing, exact edges on whole pixels, half-covered edges at half pixels, areas of a triangle, circle, parabola and rounded rectangle against the formulas, holes and overlaps by winding, shapes partly or wholly outside the canvas, strokes, shadows, images and layers.

To test code that draws, make a small `Canvas`, draw, and compare `pixel(x, y)` or the sum of `bytes()[i*4+3]` (alpha) as area. For whole-pixel shapes, assert exact bytes; for curves, assert area within about 1%.

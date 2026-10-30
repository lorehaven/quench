---
title: Testing
parent: quench-layout
nav_order: 5
---

`cargo test -p quench-layout` runs `tests/unit.rs` (42 `#[quench_test::test]` functions, custom harness). The tests build trees from CSS declarations through the real cascade (`Cascade`, `parse`, a one-element `Element` impl, `ComputedStyle::inherited()` for text), lay them out in an 800 x 600 viewport with `Fonts::embedded()`, and assert box rectangles via `position_of`/`find`. They cover stacking, padding/border/box-sizing, percentages and limits, auto margins, margin collapsing (siblings, through parents), text wrapping and whitespace, `text-align`, inline blocks, per-line inline fragments, flex (grow/shrink, justify, gap, align, wrap, column, auto margins, min-content minimum, text items, baselines, reverse), positioning (absolute, fixed, relative), `display: none`, overflow extents, replaced boxes and proportions, `LINE_BREAK`, shrink-to-fit of every content kind, `text-transform`, font metrics and line heights, and grid (fr and fixed tracks, flow, line/span placement, `repeat`, content tracks, stretched rows, named areas).

Testing your own code: always use `Fonts::embedded()` so results do not depend on installed fonts.

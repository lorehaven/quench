---
title: Testing
parent: quench-paint
nav_order: 4
---

`cargo test -p quench-paint` runs `tests/unit.rs` (20 `#[quench_test::test]` functions). Each builds a tree from CSS declarations with the real cascade, lays it out with `Fonts::embedded()`, paints it on a white canvas with `paint(&mut canvas, &boxes, &fonts, &|_| (0.0, 0.0), &|_| Vec::new())`, and asserts pixels: backgrounds and padding, rounded corners, per-side borders, outer and inset shadows, opacity, `overflow: hidden` clipping and scrolling, text colour and position, inline backgrounds, outlines, positioned boxes by z-index and `translate`, `visibility: hidden`, decorations, scroll bars, the document scroll and fixed boxes, dashed/dotted borders, background images (size, position, tiling, clipping), italic lean, rotate/scale, and culling of far-away boxes.

For your own tests use `Fonts::embedded()` and `Canvas::pixel`.

---
title: Testing
parent: quench-text
nav_order: 4
---

`cargo test -p quench-text` runs `tests/unit.rs` (18 `#[quench_test::test]` functions, custom harness) and the lib doctest. Covered: the licence strings of the embedded fonts, characters in both `cmap` formats, advances, kerning of Latin and Cyrillic pairs and of non-existent glyph ids, contour counts of simple and composite glyphs, wrapping at several widths without losing a word, a word wider than the line, hyphens, byte offsets of multi-byte characters, control characters, fallback order, the system index over a directory with a broken font, a text file and a nested directory, drawn text inside its box, fractional positioning, non-font input and each required table missing, a scrambled font, and golden fingerprints of known text in both faces at four sizes.

Environment variables read by the tests: `QUENCH_TEXT_SHOW=1` prints a rendering as characters, `QUENCH_TEXT_GOLDEN=1` prints fingerprints to update after a deliberate change.

To test code that uses text, lay out with `FontSet::noto_sans()` (deterministic across machines) and avoid `with_system_fonts()`, whose results depend on the installed fonts.

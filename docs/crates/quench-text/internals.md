---
title: Internals
parent: quench-text
nav_order: 3
---

## What the TrueType parser reads

`head`, `maxp`, `hhea`, `hmtx`, `cmap`, `loca`, `glyf`, `name` and the pair kerning of `GPOS`. Every read is bounds-checked (`u16_at`, `u32_at` return `None` past the end), so a damaged file gives an error or a missing glyph rather than a panic. Tables are located in the directory once; glyph data is read from the kept bytes on demand.

* **cmap**: the best Unicode subtable by rank: platform 3 encoding 10 format 12 (4), platform 0 encoding 4 or 6 format 12 (3), platform 3 encoding 1 format 4 (2), platform 0 any encoding format 4 (1). Other tables are ignored. Formats 4 and 12 are looked up by binary search.
* **glyf**: simple glyphs (flags with repeat, short/long deltas, instructions skipped) and composite glyphs (see `Font::outline`). Limits: depth 8, 64 components, 20 000 points.
* **loca**: short or long format from `head`.
* **GPOS kerning**: features tagged `kern` are collected; their lookups are of type 2 (pair adjustment) or type 9 (extension wrapping type 2); the script and language lists are not consulted. For a pair, every `kern` lookup contributes the XAdvance of the first glyph from its **first matching subtable** (format 1 pair sets with binary search, format 2 class pairs); contributions of different lookups are added and clamped to the `i16` range. Limits: 64 lookups, 256 subtables per lookup. Value records other than XAdvance are ignored. A font without `GPOS`, or one that fails to parse there, simply has no kerning.
* The legacy `kern` table is not read.

## Line breaking

`FontSet::layout` splits the text at `\n` (`split_inclusive`), drops `\n` and `\r` from each paragraph, shapes it and breaks it:

* **Shaping** is one glyph per `char`. `U+200B` and every control character except tab are skipped. A tab is shaped as a space with 4 times the advance. The face is the first that has the character (primary, fallbacks, system); none: glyph 0 of the primary face. Kerning of a glyph with the next is added only when both are in the same face.
* **Break opportunities** are after space, tab, `-`, U+2010, U+2013 and U+2014.
* A line takes glyphs while `x + advance <= max_width + 0.001`; spaces and tabs never overflow (they hang). On overflow the line ends after the last break opportunity if one lies after the line start, else between characters. The first glyph of a line is always taken, so layout terminates even when `max_width` is smaller than one glyph.
* Line `width` is the pen position after the last non-space glyph. `Layout::width` is the maximum.
* An empty paragraph is one empty line. An empty text, or a text ending in `\n`, appends one more empty line with `bytes = len..len`.
* No bidirectional text, no combining-mark positioning, no ligatures, no complex scripts, no hyphenation. NFC text lays out correctly; a base letter plus combining accent gets two glyphs at consecutive pen positions.

## Caches

`FontSet` keeps a map of characters to the face that has them (misses are remembered too) and a map of finished `Layout`s keyed by `(text, size.to_bits(), max_width bits)`. The layout map is cleared when it reaches 4096 entries. No glyph outline cache: each `draw` parses glyphs and rasterises them again.

## Drawing

`draw_slanted` places line `n` with its baseline at `y + layout.ascent + layout.line_height * n`, and each glyph at `x + glyph.x`. Whitespace glyphs (by `char::is_whitespace`) are skipped. The scale of a glyph is `layout.size / units_per_em` of **its own** face, so a fallback face with another em size is scaled correctly. No hinting, no subpixel (LCD) positioning beyond fractional pen positions, no bitmap (colour emoji) glyphs.

## The embedded fonts and their licence

`fonts/NotoSans-Light.ttf`, `-Regular.ttf`, `-Medium.ttf`, `-Bold.ttf` are the Latin, Greek and Cyrillic build of Noto Sans (2022), embedded with `include_bytes!` and used unmodified, about 600 KiB each. They are licensed under the **SIL Open Font License 1.1** (copyright 2022 The Noto Project Authors); `fonts/OFL.txt` has the licence text and `fonts/README.md` the notice. Tests read the licence strings (name IDs 13 and 14) from the fonts and fail if they change. Anything redistributing the crate must keep `fonts/`.

## Not supported

Shaping beyond pairs (ligatures, mark positioning, contextual forms, Indic and Arabic), bidirectional and vertical text, hinting, subpixel rendering, variable fonts, CFF/OpenType outlines, `.ttc` collections, colour emoji, selecting a system font by weight or style (the first font with the character wins), Windows/macOS font directories.

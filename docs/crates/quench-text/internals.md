---
title: Internals
parent: quench-text
grand_parent: Crates
nav_order: 3
---

## What the TrueType parser reads

`head`, `maxp`, `hhea`, `hmtx`, `cmap`, `loca`, `glyf`, `name`, the pair kerning of `GPOS`, and `MATH` (see [below](#what-the-math-parser-reads)). Every read is bounds-checked (`u16_at`, `i16_at`, `u32_at` return `None` past the end), so a damaged file gives an error or a missing glyph rather than a panic. Tables are located in the directory once; glyph data is read from the kept bytes on demand.

* **sfnt header**: `00 01 00 00` or `true` is read; `OTTO` (CFF outlines) is refused with `FontError::Cff` before anything else is looked at; `ttcf` (a collection) and everything else is `FontError::NotTrueType`.
* **cmap**: the best Unicode subtable by rank: platform 3 encoding 10 format 12 (4), platform 0 encoding 4 or 6 format 12 (3), platform 3 encoding 1 format 4 (2), platform 0 any encoding format 4 (1). Other tables are ignored. Formats 4 and 12 are looked up by binary search. A math font needs format 12 for the Mathematical Alphanumeric Symbols (U+1D400 and up).
* **glyf**: simple glyphs (flags with repeat, short/long deltas, instructions skipped) and composite glyphs (see `Font::outline`). Limits: depth 8, 64 components, 20 000 points. `Font::bounds` reads only the 10-byte glyph header (`xMin`, `yMin`, `xMax`, `yMax` at bytes 2 to 9).
* **loca**: short or long format from `head`. An empty range (`end <= start`) is a glyph without outline: no `Outline`, no `Bounds`.
* **GPOS kerning**: features tagged `kern` are collected; their lookups are of type 2 (pair adjustment) or type 9 (extension wrapping type 2); the script and language lists are not consulted. For a pair, every `kern` lookup contributes the XAdvance of the first glyph from its **first matching subtable** (format 1 pair sets with binary search, format 2 class pairs); contributions of different lookups are added and clamped to the `i16` range. Limits: 64 lookups, 256 subtables per lookup. Value records other than XAdvance are ignored. A font without `GPOS`, or one that fails to parse there, simply has no kerning.
* **MATH**: only its position is recorded by `Font::parse`; the table is parsed on the first `Font::math()` call and kept in a `OnceLock<Option<Math>>`.
* The legacy `kern` table is not read.

## What the MATH parser reads

`Math::parse` takes the bytes of the `MATH` table alone. Every offset is resolved against the subtable it belongs to and every read is checked against the table's length (not the file's), so a table cut short in the font directory shrinks rather than reading the next table.

* **Header**: major version must be 1 (the minor version is not checked) and the table at least 10 bytes; otherwise `parse` returns `None`. The three offsets (`MathConstants`, `MathGlyphInfo`, `MathVariants`) may be null: that part is then absent.
* **MathConstants**: 56 fields in table order, 214 bytes: two `i16` percentages, two `u16` minimum heights, 51 `MathValueRecord`s (value plus a device offset that is skipped), and the final `i16` percentage. Read all or nothing.
* **MathGlyphInfo**: four optional subtables.
  * `MathItalicsCorrectionInfo` and `MathTopAccentAttachment`: a coverage table and one `MathValueRecord` per covered glyph, kept as `(glyph, value)` pairs sorted by glyph.
  * `ExtendedShapeCoverage`: a coverage table, kept as a sorted, deduplicated glyph list.
  * `MathKernInfo`: a coverage table and per glyph four offsets (top right, top left, bottom right, bottom left), measured from the start of `MathKernInfo`; a null offset is an empty `MathKern` (kerns by 0). Each `MathKern` is a height count `n`, `n` heights and `n + 1` kerns, all value records.
* **MathVariants**: `minConnectorOverlap`, the vertical and horizontal coverage offsets and counts, then one `MathGlyphConstruction` offset per covered glyph (vertical first). A construction is an optional `GlyphAssembly` offset, a variant count and `(glyph, advance)` records. An assembly is an italic correction (value record), a part count and 10-byte part records (`glyph`, start and end connector lengths, full advance, flags; bit 0 is `extender`).
* **Coverage** formats 1 (glyph list) and 2 (ranges with a start index) are read; any other format makes that subtable absent. A format 2 coverage that would name 65 536 glyphs or more, or has a range with end below start, is refused. A coverage index at or past the record count is skipped.
* **Damage**: within one subtable a failed read drops the whole subtable (`unwrap_or_default`): no italic corrections rather than some. The vertical and horizontal constructions are read independently: a damaged construction empties its own direction only, and the other keeps all its entries (the horizontal offsets are found from the vertical count, not from the vertical constructions). The tests cut and scramble a handmade table and the real one and require only that nothing panics and that each subtable is read whole or not at all.
* **Lookups**: per-glyph lists are sorted by glyph id with the first entry of a duplicated glyph kept, and looked up by binary search. `MathKern::at` uses `partition_point(|&h| h < height)`: a height equal to a correction height falls in the band below.
* Device tables (`DeviceTable`/`VariationIndex` corrections for pixel sizes or variable fonts) are not read.

The crate reads the table; it does not lay out formulas. [quench-tex](../quench-tex/index.md) does, with these values.

## Line breaking

`FontSet::layout` splits the text at `\n` (`split_inclusive`), drops `\n` and `\r` from each paragraph, shapes it and breaks it:

* **Shaping** is one glyph per `char`. `U+200B` and every control character except tab are skipped. A tab is shaped as a space with 4 times the advance. The face is the first that has the character (primary, fallbacks, system); none: glyph 0 of the primary face. Kerning of a glyph with the next is added only when both are in the same face.
* **Break opportunities** are after space, tab, `-`, U+2010, U+2013 and U+2014.
* A line takes glyphs while `x + advance <= max_width + 0.001`; spaces and tabs never overflow (they hang). On overflow the line ends after the last break opportunity if one lies after the line start, else between characters. The first glyph of a line is always taken, so layout terminates even when `max_width` is smaller than one glyph.
* Line `width` is the pen position after the last non-space glyph. `Layout::width` is the maximum.
* An empty paragraph is one empty line. An empty text, or a text ending in `\n`, appends one more empty line with `bytes = len..len`.
* No bidirectional text, no combining-mark positioning, no ligatures, no complex scripts, no hyphenation. NFC text lays out correctly; a base letter plus combining accent gets two glyphs at consecutive pen positions.

## Caches

`FontSet` keeps a map of characters to the face that has them (misses are remembered too) and a map of finished `Layout`s keyed by `(text, size.to_bits(), max_width bits)`. The layout map is cleared when it reaches 4096 entries. No glyph outline cache: each `draw` parses glyphs and rasterises them again. A `Font` caches only its parsed `MATH` table.

## Drawing

`draw_slanted` places line `n` with its baseline at `y + layout.ascent + layout.line_height * n`, and each glyph at `x + glyph.x`. Whitespace glyphs (by `char::is_whitespace`) are skipped. The scale of a glyph is `layout.size / units_per_em` of **its own** face, so a fallback face with another em size is scaled correctly. No hinting, no subpixel (LCD) positioning beyond fractional pen positions, no bitmap (colour emoji) glyphs.

## The embedded fonts and their licence

`fonts/NotoSans-Light.ttf`, `-Regular.ttf`, `-Medium.ttf`, `-Bold.ttf` are the Latin, Greek and Cyrillic build of Noto Sans (2022), embedded with `include_bytes!` and used unmodified, about 600 KiB each. They are licensed under the **SIL Open Font License 1.1** (copyright 2022 The Noto Project Authors); `fonts/OFL.txt` has the licence text and `fonts/README.md` the notice. Tests read the licence strings (name IDs 13 and 14) from the fonts and fail if they change. Anything redistributing the crate must keep `fonts/`.

`fonts/NotoSansMath-Regular.ttf` is embedded only with the `math-font` feature. `fonts/README.md` records its source: downloaded on 2026-11-08 from Google Fonts (`https://github.com/google/fonts/raw/main/ofl/notosansmath/NotoSansMath-Regular.ttf`), 1 015 396 bytes, SHA-256 `3f495fe933c06786e4d5f6d86b8ee70b6753a68ee3b9d87528726de0f6e2c47d`, used unmodified. Same licence, SIL Open Font License 1.1 (name ID 13; ID 14 is `https://openfontlicense.org`); its `name` table gives the copyright as "Copyright 2022 Google LLC. All Rights Reserved." The tests read the family (`Noto Sans Math`), version (`Version 3.000...`), copyright and licence strings, and check that `fonts/README.md` mentions the file.

## Noto Sans Math

What the embedded math font has, as read by this crate (values asserted by the tests are marked *t*; the others were printed from the font):

* TrueType `glyf` outlines (not CFF), 1000 units per em *t*, 5130 glyphs *t*, ascent 1069, descent -423, line gap 0 *t*. The `MATH` table is 20 200 bytes *t*, constants at offset 10.
* **Constants**: listed in the [API reference](api.md#mathconstants); script scale 60 % and 50 % *t*, axis height 278 *t*, accent base height 556 *t*.
* **No `MathKernInfo` at all** *t*: `kerned_glyphs()` is empty and `kern` is 0 for every glyph and corner, so a typesetter falls back to italic corrections alone.
* **Italic corrections and accent attachments** for the math alphanumerics: math italic *f* (U+1D453) has an italic correction of 145 *t*; bold A (U+1D400) has none *t*; accent attachments of bold A, ℝ, `a` and italic *f* lie inside their ink *t*; the integral has none *t*. U+1D455 has no glyph (the hole for italic h, which is U+210E) *t*.
* **Vertical constructions**: 215 glyphs (more than 100 *t*). `(`, `)`, `[`, `]`, `{`, `}`, `|`, `‖` (U+2016), `√` and `∫` have at least two variants and an assembly, are extended shapes and have no horizontal construction *t*. The parenthesis has 13 variants (942 to 3768 units) and a three-part assembly, hook, extender, hook *t*; the brace has five parts (two extenders). The integral's assembly has a positive italic correction *t*. `⌊ ⌋ ⌈ ⌉`, `⟮ ⟯` and `↑` also have assemblies. **No assembly**: `∑` (2 variants, 954 and 1336 units) *t*, `⟨ ⟩` (21 variants) *t*, `/` and `\`. Ordinary letters have no construction and are not extended shapes *t*.
* **Horizontal constructions**: 29 glyphs (more than 10 *t*). The over- and under-brace (U+23DE, U+23DF) have 6 variants and a five-part assembly, end, extender, middle tip, extender, end *t*. `→` (U+2192) and `←` (U+2190) have **no variants, only an assembly** (a shaft that repeats and a head) *t*. The combining hat U+0302 has 6 variants and no assembly, and its first variant is not the combining glyph itself *t* (likewise U+0303 and U+030C). The combining arrows U+20D7, U+20D6 and U+20E1 have variants and an assembly; `↔` (U+2194) and the long arrows have no construction.
* **Minimum connector overlap**: 100 *t*.
* **Quirks** (not tested as such; printed from the font): in the `→` assembly the shaft (the extender, first part) has a full advance of 658 but an end connector of 777, longer than the part itself (the head is 910 with a start connector of 500); in the `↑` and U+20D7 assemblies the first part has a non-zero start connector (600), which the specification expects to be 0. A consumer that takes connector lengths at face value overlaps parts by more than their length; quench-tex clamps each overlap to both parts' full advances. The first part of `⌈`'s assembly is the extender.

## Not supported

Shaping beyond pairs (ligatures, mark positioning, contextual forms, Indic and Arabic), bidirectional and vertical text, hinting, subpixel rendering, variable fonts, CFF/OpenType outlines (`FontError::Cff`), `.ttc` collections, colour emoji, selecting a system font by weight or style (the first font with the character wins), Windows/macOS font directories, `MATH` device tables, the `MATH` table's use in layout (that is quench-tex's job), and math variants through `GSUB` (`ssty` script-size glyphs are not read: scripts are scaled outlines).

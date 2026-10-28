# Quench Text

`quench-text` (crate `quench_text`) turns a string into pixels with no dependencies: a TrueType parser, pair kerning, line breaking, **Noto Sans embedded** so text looks the same on every machine, and a fallback to the fonts installed on the machine for what Noto lacks. It draws through [`quench-raster`](./quench-raster.md), and is the second piece of the [native renderer](../plans/NATIVE_RENDERER.md).

```rust
use quench_raster::{Canvas, Color};
use quench_text::FontSet;

let fonts = FontSet::noto_sans().with_system_fonts();
let layout = fonts.layout("Zażółć gęślą jaźń: Привет", 16.0, Some(200.0)); // wrapped to 200 px
let mut canvas = Canvas::new(220, 80);
fonts.draw(&mut canvas, &layout, 8.0, 8.0, Color::WHITE);
// layout.lines, .width, .height, .line_height: what a box around the text needs
```

## The font

`Font::parse(bytes)` reads a TrueType font (`glyf` outlines; `OTTO`/CFF is refused) and keeps the bytes (borrowed when they are `'static`, as an embedded file is): tables are read on demand, nothing is copied out but their positions. It reads `head`, `maxp`, `hhea`, `hmtx`, `cmap` (formats 4 and 12, the best Unicode table the font has), `loca`, `glyf` (simple and composite glyphs: offsets, scales and 2x2 transforms; components placed by matching points are left out), `name`, and the **pair kerning of `GPOS`** (the `kern` feature's pair adjustments, formats 1 and 2, directly or through an extension lookup; each lookup's first matching subtable is applied and the adjustments added). Every read is bounds-checked: a damaged or hostile file is an error or a missing glyph, never a panic or an overrun, and depth, component and point counts are capped (a test scrambles the file 150 ways and calls everything).

`font.glyph(c)`, `advance(glyph)`, `kerning(left, right)`, `outline(glyph)`, the vertical metrics and `name(id)`. An `Outline` is the glyph's contours in font units with y up; `outline.to_path(scale, x, y)` makes a `quench_raster::Path` in pixels (y down), flattening the curves in pixel space.

## The embedded fonts and their licence

`NotoSans-Regular.ttf` and `NotoSans-Bold.ttf` are the Latin, Greek and Cyrillic build of Noto Sans (2022, 3 884 glyphs each, about 620 KiB), embedded with `include_bytes!`. Their licence is the **SIL Open Font License 1.1**: the fonts' own `name` table says so, `quench-text`'s tests read those strings (and fail if they change), and `fonts/OFL.txt` carries the licence text with the Noto copyright notice. The fonts are used unmodified; a published `quench-text` package includes `fonts/`. `noto_sans()` and `noto_sans_bold()` return them parsed once.

## Layout

`FontSet` is a primary face, explicit fallbacks (`with_fallback`) and optionally the machine's fonts. `layout(text, size, max_width)` returns a `Layout`: lines of `Glyph`s (face, glyph id, x from the start of the line, advance with kerning folded in, and the character and byte offset it came from, which is what a caret needs), the widest line's `width`, and the `height`.

- **One glyph per character**, advanced by the font's metrics and pulled together by kerning between neighbours of the same face. No ligatures, no combining marks (precomposed text, NFC, lays out correctly; a base letter followed by a combining accent does not get the mark positioned), no right-to-left or complex scripts.
- **Lines break** after spaces and hyphens (and en and em dashes) and, for a word wider than the line, between characters (a line that is narrower than one glyph still holds one, so layout always ends). Trailing spaces hang past the end of a line and do not count toward its width. `\n` breaks, `\r` is ignored, a trailing `\n` leaves an empty last line (for a caret), a tab is four spaces, other control characters and U+200B are skipped.
- **Line height** is the font's: `(ascent - descent + line gap) * size / units per em`, which for Noto Sans is 1.362 em.
- **A character no face has** is drawn as the primary face's `.notdef` box, so it is visible that something is missing.
- `measure(text, size)` is the width of one line.

`draw(&mut canvas, &layout, x, y, color)` fills each glyph's path with exact anti-aliased coverage at its fractional pen position (there is no hinting and no glyph cache yet: a glyph is parsed and rasterised each time it is drawn).

## System fonts

`with_system_fonts()` (or `with_system_directories`) adds the fonts installed on the machine for characters none of the faces has: `/usr/share/fonts`, `/usr/local/share/fonts`, `~/.local/share/fonts` and `~/.fonts`, searched in that order and in name order within a directory. The directories are indexed once, lazily, by the code points each file's `cmap` covers (read from the file's table directory and `cmap` alone, never the whole font); a font is read in full only when it is the one that has a character that was asked for, and kept for the next one. A search that finds nothing is remembered too, so the directories are not walked again for the same character. Only `.ttf` files are read (a `.ttc` collection or a CFF `.otf` is skipped).

## Tested

17 tests and a doctest, 94% line coverage: the licence strings, characters in both `cmap` formats, advances, kerning of Latin and Cyrillic pairs and of glyph ids that do not exist, contour counts of simple and composite glyphs (`é` is `e` and an accent), a glyph drawn large with its counter empty, wrapping at four widths without losing a word, a word wider than the line, hyphens, byte offsets of multi-byte characters, control characters, fallback order, the system index over a directory with a broken font, a text file and a nested directory, the real machine's fonts when one has a probe character, a drawn string inside its box in the colour asked for with the second line one line height lower, fractional positioning, files that are not fonts and each table missing, a scrambled font, and **golden fingerprints** of known text in both faces at four sizes: `QUENCH_TEXT_SHOW=1` prints a rendering as characters, and `QUENCH_TEXT_GOLDEN=1` prints the fingerprints to update after a change you have looked at and judged better.

## Not done

- **Shaping beyond pairs:** ligatures, combining-mark positioning (`GPOS` mark lookups), contextual forms, Indic and Arabic scripts, bidirectional text, vertical text.
- **Hinting and subpixel (LCD) rendering**, **a glyph cache** (the next measurement will say whether it is needed), **variable fonts**, **CFF/OpenType outlines**, **`.ttc` collections**, **colour emoji**.
- **Font matching by weight and style** among the system fonts: the fallback takes the first font that has the character.
- Windows and macOS font directories (Android's `/system/fonts` comes with the Android host).

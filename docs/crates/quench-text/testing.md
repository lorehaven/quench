---
title: Testing
parent: quench-text
grand_parent: Crates
nav_order: 4
---

`cargo test -p quench-text` runs `tests/unit.rs` (custom harness, `harness = false`, `quench_test::main!()`) with 21 `#[quench_test::test]` functions, and the lib doctest. `cargo test -p quench-text --features math-font` adds the `math_font` module of the same file: 27 tests in all.

## Text and TrueType

Covered: the licence strings of the embedded fonts, characters in both `cmap` formats, advances, kerning of Latin and Cyrillic pairs and of non-existent glyph ids, contour counts of simple and composite glyphs, a large glyph's counter and stem, wrapping at several widths without losing a word, a word wider than the line, hyphens, byte offsets of multi-byte characters, control characters, fallback order, the system index over a directory with a broken font, a text file and a nested directory, a real system font supplying a character, drawn text inside its box, fractional positioning, layout caching per size and width, golden fingerprints of known text in both faces at four sizes, and a deterministic scramble of the font file on which every call must not panic.

`files_that_are_not_fonts_are_refused_by_what_is_wrong_with_them` checks each error: empty input is `NotTrueType`; `OTTO` is `Cff` and displays `CFF outlines are not read`; a 4-byte header is `Truncated("directory")`; the file cut at 300 bytes is `Missing(_)`; each required table renamed away is `Missing(name)`; a zero units-per-em is `Truncated("head")`; without `name` and `GPOS` the font still parses, with no names and no kerning.

`glyph_bounds_come_from_the_glyph_header`: `o` has `y_min < 0 < y_max`, `H` sits on the baseline and is taller; a space and a glyph id past the last have no bounds.

## The MATH table (always)

* `math_kerns_are_looked_up_by_height_band`: a handmade 88-byte `MATH` table (no constants, no variants; italic corrections for glyphs 5 and 9 through a format 1 coverage; corner kerns for glyph 7 through a format 2 coverage: top right with heights 100, 300 and kerns -50, -20, 10, bottom left one kern -30) checks the band rule at the boundaries, absent corners and glyphs kerning by 0, the italic corrections, and that the same table spliced into Noto Sans (as a renamed `post` entry) is what `Font::math` returns. Noto Sans itself has no `MATH`.
* `a_damaged_math_table_reads_as_absent_or_empty_and_never_panics`: the handmade table cut at every length (it parses from 10 bytes on, and each subtable is whole or absent), another major version (`None`), and every 16-bit field overwritten with 0, 1, 2, 3, `0x7FFF`, `0xFFFF`.

## The math font (feature `math-font`)

* `the_math_font_is_embedded_with_its_constants`: 1000 units per em, 5130 glyphs, the family, version 3.000, copyright and licence strings, `fonts/README.md` naming the file, ascent/descent/line gap, script percentages 60/50, axis height 278, accent base height 556, the ranges and signs of 18 other constants, and that `math()` returns the same reference each time.
* `delimiters_radicals_and_integrals_grow_by_variants_then_assemblies`: the vertical constructions of the delimiters, radical and integral (variants that grow and start at the glyph itself, assemblies of real glyphs with extenders, first part without start connector and last without end connector), the parenthesis' three parts, the integral's italic correction, `∑` and `⟨` without assembly, letters without constructions, more than 100 vertical glyphs, and a minimum connector overlap of 100.
* `braces_and_arrows_grow_wider`: the over-brace's five parts, `→` and `←` with only an assembly, the wide hat with variants only.
* `math_letters_have_italic_corrections_and_accent_attachments`: format 12 lookups beyond the BMP, the hole at U+1D455, italic *f*'s correction of 145, accent attachments inside the ink, and no math kerns in the font.
* `damage_to_one_direction_of_growth_leaves_the_other`: the real table with its first vertical construction offset pointed past the table has no vertical constructions but all its horizontal ones (the over-brace's construction unchanged), and the mirror case keeps all vertical ones (the parenthesis unchanged); constants and the connector overlap are untouched.
* `the_real_math_table_cut_short_or_scrambled_never_panics`: the real 20 200-byte table with its directory length cut to 0 to 20 199 bytes (constants present from 224 bytes on), longer than the file (no table), every 97th cut of the table bytes, 40 rounds of scrambled bytes and a block of garbage, each followed by every query a typesetter makes.

## Environment and use from other crates

Environment variables read by the tests: `QUENCH_TEXT_SHOW=1` prints a rendering as characters, `QUENCH_TEXT_GOLDEN=1` prints fingerprints to update after a deliberate change.

To test code that uses text, lay out with `FontSet::noto_sans()` (deterministic across machines) and avoid `with_system_fonts()`, whose results depend on the installed fonts. To test code that reads `MATH`, enable `math-font` in `[dev-dependencies]` (as quench-tex does) and use `noto_sans_math()`, or build a small table by hand and give it to `Math::parse` as `tests/unit.rs` does.

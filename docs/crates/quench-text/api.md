---
title: API reference
parent: quench-text
grand_parent: Crates
nav_order: 2
---

The crate root (`src/lib.rs`) re-exports:

```rust
pub use font::{Bounds, Font, FontError, Outline, Point};
#[cfg(feature = "math-font")]
pub use layout::noto_sans_math;
pub use layout::{FontSet, Glyph, Layout, Line, noto_sans, noto_sans_bold, noto_sans_light, noto_sans_medium};
pub use math::{
    GlyphAssembly, GlyphConstruction, GlyphPart, GlyphVariant, Math, MathConstants, MathKern,
    MathKernCorner, MathKernInfo,
};
pub use system::SystemFonts;
```

Modules (`font`, `gpos`, `layout`, `math`, `system`) are private. 19 types, 4 free functions (5 with `math-font`), and the methods below. No `unsafe`. Units: everything that comes from a font (metrics, outlines, bounds, `MATH` values) is in **font units with y pointing up and the baseline at y = 0**; layout results are in pixels.

## Font (`font.rs`)

### `FontError`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontError {
    NotTrueType,             // not an sfnt font file at all (also a `.ttc` collection)
    Cff,                     // an OpenType font with CFF outlines (`OTTO`), which this crate does not read
    Missing(&'static str),   // a table the font cannot work without is missing
    Truncated(&'static str), // a table is cut short or points outside the file
}
```

Implements `Display` and `std::error::Error`:

| Variant | `Display` |
|---|---|
| `NotTrueType` | `not a TrueType font` |
| `Cff` | `CFF outlines are not read` |
| `Missing(table)` | ``the `{table}` table is missing`` |
| `Truncated(table)` | ``the `{table}` table is cut short`` |

### `Point`, `Bounds` and `Outline`

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point { pub x: f32, pub y: f32, pub on: bool }   // font units, y up; on = on-curve

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bounds { pub x_min: i16, pub y_min: i16, pub x_max: i16, pub y_max: i16 }   // font units, y up

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Outline { pub contours: Vec<Vec<Point>> }
```

`Bounds` is a glyph's bounding box as its `glyf` header records it (see `Font::bounds`).

| Method | Signature | Notes |
|---|---|---|
| `Outline::to_path` | `#[must_use] pub fn to_path(&self, scale: f32, x: f32, y: f32) -> Path` | Pixel `Path` (`quench_raster::Path`): point `(px, py)` maps to `(x + px*scale, y - py*scale)`, so `(x, y)` is the glyph origin and y points down. Quadratic curves; contours with fewer than 2 points are skipped; a contour with no on-curve point starts at the midpoint of its last and first points; two consecutive off-curve points get an implied on-curve midpoint. |
| `Outline::to_path_slanted` | `#[must_use] pub fn to_path_slanted(&self, scale: f32, x: f32, y: f32, slant: f32) -> Path` | As above, x additionally shifted by `slant * py * scale` (0.2 = synthetic italic). |

### `Font`

```rust
pub struct Font { /* private */ }   // Debug prints family, glyph count, units_per_em
```

`Font` holds its bytes and plain offsets plus a `OnceLock` for the `MATH` table, so it is `Send + Sync`; the embedded faces are shared as `Arc<Font>`.

| Method | Signature | Behaviour |
|---|---|---|
| `parse` | `pub fn parse(data: impl Into<Cow<'static, [u8]>>) -> Result<Self, FontError>` | Accepts sfnt version `0x00010000` or `true`; `OTTO` is `FontError::Cff`; anything else (a `ttcf` collection, garbage, fewer than 4 bytes) is `FontError::NotTrueType`. No table count: `Truncated("directory")`. Required tables: `head`, `maxp`, `hhea`, `hmtx`, `loca`, `glyf`, `cmap` (missing: `FontError::Missing(name)`; a cmap without a usable Unicode subtable reports `Missing("cmap (a Unicode table)")`). `head` shorter than 54 bytes or units-per-em 0: `Truncated("head")`; `maxp`/`hhea` fields unreadable: `Truncated("maxp")`/`Truncated("hhea")`. Tables running past the end of the file count as missing. `name`, `GPOS` and `MATH` are optional. `GPOS` kerning is read here; `MATH` only on the first `math()` call. Bytes are kept (borrowed if `'static`). |
| `units_per_em` | `#[must_use] pub const fn units_per_em(&self) -> u16` | Font units per em (the size a glyph is designed at; 1000 or 2048, usually) |
| `ascent` | `#[must_use] pub const fn ascent(&self) -> i16` | `hhea` ascender: from the baseline up to the top of a line, font units |
| `descent` | `#[must_use] pub const fn descent(&self) -> i16` | `hhea` descender: from the baseline down to the bottom of a line, font units (negative) |
| `line_gap` | `#[must_use] pub const fn line_gap(&self) -> i16` | `hhea` line gap: extra space between lines, font units |
| `glyph_count` | `#[must_use] pub const fn glyph_count(&self) -> u16` | `maxp` numGlyphs; glyph ids are below this |
| `glyph` | `#[must_use] pub fn glyph(&self, c: char) -> Option<u16>` | cmap lookup (format 4 for code points up to U+FFFF, format 12 for all, whichever subtable was chosen); `None` if absent, if glyph 0 (`.notdef`) or an id `>= glyph_count` |
| `advance` | `#[must_use] pub fn advance(&self, glyph: u16) -> u16` | `hmtx` advance in font units; glyphs past `numberOfHMetrics` use the last metric; 0 if the table is unreadable |
| `kerning` | `#[must_use] pub fn kerning(&self, left: u16, right: u16) -> i16` | GPOS pair adjustment (x advance of the first glyph), font units; negative pulls together; 0 when the font has none |
| `name` | `#[must_use] pub fn name(&self, id: u16) -> Option<String>` | From `name`: ID 1 family, 2 style, 0 copyright, 5 version, 13 licence, 14 licence URL. Prefers platform 3 (encodings 1, 10) or platform 0 (UTF-16BE), falls back to Mac Roman platform 1 encoding 0 read byte per char. First match wins. |
| `math` | `#[must_use] pub fn math(&self) -> Option<&Math>` | The `MATH` table, parsed on the first call (`OnceLock`) and returned by reference afterwards (the same `&Math` every time). `None` when the font has no `MATH` table, the table runs past the file, or its header cannot be read (see `Math::parse`). |
| `bounds` | `#[must_use] pub fn bounds(&self, glyph: u16) -> Option<Bounds>` | The bounding box from the glyph's `glyf` header (bytes 2..10), font units, y up. `None` for a glyph with no outline (a space: empty `loca` range), an id `>= glyph_count`, or a header cut short. For a composite glyph it is the box the header records; components are not combined. |
| `outline` | `#[must_use] pub fn outline(&self, glyph: u16) -> Option<Outline>` | `None` for a glyph with no outline (space), a damaged glyph, or one over the caps (component depth 8, 64 components, 20 000 points). Composite glyphs: offsets (args as words or signed bytes), scale, x/y scale, 2x2 transform. A component positioned by point matching (flag `ARGS_ARE_XY_VALUES` clear) is left out. Hinting instructions are skipped. |

## The `MATH` table (`math.rs`)

The OpenType `MATH` table, read once into owned values. Every value is in **font units, y up**; percentages are plain numbers (60 means 60 %). Device tables (pixel-size corrections) are not read. A damaged subtable reads as absent or empty, never as a panic (see [internals](internals.md#what-the-math-parser-reads)).

### `Math`

```rust
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Math { /* private: constants, per-glyph lists sorted by glyph id, constructions */ }
```

Get one with `Font::math()`, or parse the bytes of a `MATH` table directly with `Math::parse`. Lookups by glyph are binary searches.

| Method | Signature | Behaviour |
|---|---|---|
| `parse` | `#[must_use] pub fn parse(table: &[u8]) -> Option<Self>` | Reads a `MATH` table (the table's bytes alone, not the font). `None` when the major version is not 1 or the table is shorter than its 10-byte header. A damaged or missing subtable is left out: absent constants, no entries. |
| `constants` | `#[must_use] pub const fn constants(&self) -> Option<&MathConstants>` | `None` when the font has none or they are cut short (all 214 bytes or nothing). |
| `italics_correction` | `#[must_use] pub fn italics_correction(&self, glyph: u16) -> Option<i16>` | `MathItalicsCorrectionInfo`: how far a superscript after the glyph moves right. |
| `top_accent_attachment` | `#[must_use] pub fn top_accent_attachment(&self, glyph: u16) -> Option<i16>` | `MathTopAccentAttachment`: x from the glyph origin where an accent over it is centred. Without one, the specification says to centre on the ink (half the bounding box width plus its left side); use `Font::bounds`. |
| `is_extended_shape` | `#[must_use] pub fn is_extended_shape(&self, glyph: u16) -> bool` | In the `ExtendedShapeCoverage`: a tall delimiter or large operator, whose scripts are placed by its ink. |
| `kern_info` | `#[must_use] pub fn kern_info(&self, glyph: u16) -> Option<&MathKernInfo>` | The four corner kerns of `glyph`, when the font has them. |
| `kern` | `#[must_use] pub fn kern(&self, glyph: u16, corner: MathKernCorner, height: i16) -> i16` | The kern at `corner` of `glyph` at `height` (font units, y up): `kern_info(glyph)` then `MathKern::at(height)`; 0 when the font has none for that glyph or corner. |
| `kerned_glyphs` | `pub fn kerned_glyphs(&self) -> impl Iterator<Item = u16> + '_` | Glyphs that have corner kerns, ascending. |
| `min_connector_overlap` | `#[must_use] pub const fn min_connector_overlap(&self) -> u16` | `MathVariants.minConnectorOverlap`: the least overlap between two parts of an assembly; 0 without a `MathVariants` subtable. |
| `vertical_construction` | `#[must_use] pub fn vertical_construction(&self, glyph: u16) -> Option<&GlyphConstruction>` | How the glyph grows taller (delimiters, radicals, integrals, large operators). |
| `horizontal_construction` | `#[must_use] pub fn horizontal_construction(&self, glyph: u16) -> Option<&GlyphConstruction>` | How the glyph grows wider (wide accents, braces, arrows). |
| `vertical_glyphs` | `pub fn vertical_glyphs(&self) -> impl Iterator<Item = u16> + '_` | Glyphs with a vertical construction, ascending. |
| `horizontal_glyphs` | `pub fn horizontal_glyphs(&self) -> impl Iterator<Item = u16> + '_` | Glyphs with a horizontal construction, ascending. |

### `MathConstants`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MathConstants { /* 56 public fields */ }
```

Every field of the `MathConstants` subtable, in table order, named as the specification names them in snake case. Two fields are `u16`, the rest `i16`. Device-table corrections are ignored. The right column is the value in the embedded Noto Sans Math (1000 units per em).

| Field | Type | Noto Sans Math |
|---|---|---|
| `script_percent_scale_down` | `i16` (%) | 60 |
| `script_script_percent_scale_down` | `i16` (%) | 50 |
| `delimited_sub_formula_min_height` | `u16` | 2300 |
| `display_operator_min_height` | `u16` | 2300 |
| `math_leading` | `i16` | 150 |
| `axis_height` | `i16` | 278 |
| `accent_base_height` | `i16` | 556 |
| `flattened_accent_base_height` | `i16` | 714 |
| `subscript_shift_down` | `i16` | 210 |
| `subscript_top_max` | `i16` | 430 |
| `subscript_baseline_drop_min` | `i16` | 140 |
| `superscript_shift_up` | `i16` | 390 |
| `superscript_shift_up_cramped` | `i16` | 280 |
| `superscript_bottom_min` | `i16` | 134 |
| `superscript_baseline_drop_max` | `i16` | 240 |
| `sub_superscript_gap_min` | `i16` | 160 |
| `superscript_bottom_max_with_subscript` | `i16` | 430 |
| `space_after_script` | `i16` | 40 |
| `upper_limit_gap_min` | `i16` | 100 |
| `upper_limit_baseline_rise_min` | `i16` | 244 |
| `lower_limit_gap_min` | `i16` | 100 |
| `lower_limit_baseline_drop_min` | `i16` | 630 |
| `stack_top_shift_up` | `i16` | 529 |
| `stack_top_display_style_shift_up` | `i16` | 704 |
| `stack_bottom_shift_down` | `i16` | 320 |
| `stack_bottom_display_style_shift_down` | `i16` | 673 |
| `stack_gap_min` | `i16` | 178 |
| `stack_display_style_gap_min` | `i16` | 336 |
| `stretch_stack_top_shift_up` | `i16` | 244 |
| `stretch_stack_bottom_shift_down` | `i16` | 559 |
| `stretch_stack_gap_above_min` | `i16` | 100 |
| `stretch_stack_gap_below_min` | `i16` | 100 |
| `fraction_numerator_shift_up` | `i16` | 529 |
| `fraction_numerator_display_style_shift_up` | `i16` | 704 |
| `fraction_denominator_shift_down` | `i16` | 320 |
| `fraction_denominator_display_style_shift_down` | `i16` | 673 |
| `fraction_numerator_gap_min` | `i16` | 71 |
| `fraction_num_display_style_gap_min` | `i16` | 150 |
| `fraction_rule_thickness` | `i16` | 71 |
| `fraction_denominator_gap_min` | `i16` | 71 |
| `fraction_denom_display_style_gap_min` | `i16` | 150 |
| `skewed_fraction_horizontal_gap` | `i16` | 350 |
| `skewed_fraction_vertical_gap` | `i16` | 71 |
| `overbar_vertical_gap` | `i16` | 150 |
| `overbar_rule_thickness` | `i16` | 71 |
| `overbar_extra_ascender` | `i16` | 71 |
| `underbar_vertical_gap` | `i16` | 150 |
| `underbar_rule_thickness` | `i16` | 71 |
| `underbar_extra_descender` | `i16` | 71 |
| `radical_vertical_gap` | `i16` | 89 |
| `radical_display_style_vertical_gap` | `i16` | 190 |
| `radical_rule_thickness` | `i16` | 71 |
| `radical_extra_ascender` | `i16` | 71 |
| `radical_kern_before_degree` | `i16` | 51 |
| `radical_kern_after_degree` | `i16` | -450 |
| `radical_degree_bottom_raise_percent` | `i16` (%) | 56 |

(The Noto Sans Math column was printed from the embedded font with this crate; the tests assert `60`, `50`, `278`, `556` and the signs and ranges of the others.)

### `MathKernCorner`, `MathKern`, `MathKernInfo`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MathKernCorner { TopRight, TopLeft, BottomRight, BottomLeft }

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MathKern {
    pub correction_heights: Vec<i16>,   // heights (y, font units) at which the kern changes, ascending
    pub kern_values: Vec<i16>,          // one more than the heights; negative moves the script closer
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MathKernInfo {
    pub top_right: MathKern,
    pub top_left: MathKern,
    pub bottom_right: MathKern,
    pub bottom_left: MathKern,
}
```

| Method | Signature | Behaviour |
|---|---|---|
| `MathKern::at` | `#[must_use] pub fn at(&self, height: i16) -> i16` | The **height-band rule**: `kern_values[i]` applies between `correction_heights[i - 1]` and `correction_heights[i]`; the first value below the lowest height, the last above the highest. A height equal to a correction height belongs to the band **below** it (as HarfBuzz reads the specification). A corner the font leaves out has no values and kerns by 0. |
| `MathKernInfo::corner` | `#[must_use] pub const fn corner(&self, corner: MathKernCorner) -> &MathKern` | The kern of one corner. |

With heights `[100, 300]` and kerns `[-50, -20, 10]` (the handmade table of the tests): `at(i16::MIN)`, `at(0)` and `at(100)` are -50; `at(101)` and `at(300)` are -20; `at(301)` and `at(i16::MAX)` are 10.

### `GlyphConstruction`, `GlyphVariant`, `GlyphAssembly`, `GlyphPart`

```rust
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GlyphConstruction {
    pub variants: Vec<GlyphVariant>,      // smallest first; the first is usually the glyph itself
    pub assembly: Option<GlyphAssembly>,  // for sizes past the largest variant
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GlyphVariant {
    pub glyph: u16,
    pub advance: u16,   // size along the growth direction: height plus depth (vertical) or width (horizontal)
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GlyphAssembly {
    pub italics_correction: i16,   // of the assembled glyph
    pub parts: Vec<GlyphPart>,     // bottom to top (vertical) or left to right (horizontal)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GlyphPart {
    pub glyph: u16,
    pub start_connector_length: u16,   // how much of the part's start may overlap the previous part
    pub end_connector_length: u16,     // how much of the part's end may overlap the next part
    pub full_advance: u16,             // the part's whole length
    pub extender: bool,                // may be repeated (or left out) to reach the size wanted
}
```

The crate gives the data; choosing a variant or building an assembly is the caller's job. The OpenType rule is: take the first variant whose `advance` reaches the size wanted; past the largest, put the assembly together with extenders repeated as needed and each overlap between neighbours at least `Math::min_connector_overlap()` and at most the smaller of the two connectors. [quench-tex](../quench-tex/internals.md#growing-glyphs) has an implementation.

## Layout (`layout.rs`)

### Embedded faces

```rust
#[must_use] pub fn noto_sans() -> Arc<Font>          // Regular
#[must_use] pub fn noto_sans_bold() -> Arc<Font>
#[must_use] pub fn noto_sans_light() -> Arc<Font>    // doc: weights up to 350
#[must_use] pub fn noto_sans_medium() -> Arc<Font>   // doc: weights from 450 to 599
#[cfg(feature = "math-font")]
#[must_use] pub fn noto_sans_math() -> Arc<Font>     // Noto Sans Math, with a MATH table
```

Each parses its embedded file once (`OnceLock`) and returns a clone of the `Arc`. They carry a `# Panics` section: never for the embedded file (a test parses it). The Noto Sans faces have no `MATH` table (`noto_sans().math()` is `None`). `FontSet` has no `noto_sans_math` constructor: a math font is used through its `Font` methods, not for laying out paragraphs.

### `Glyph`, `Line`, `Layout`

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Glyph { pub face: usize, pub id: u16, pub x: f32, pub advance: f32, pub character: char, pub byte: usize }

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Line { pub glyphs: Vec<Glyph>, pub width: f32, pub bytes: std::ops::Range<usize> }

#[derive(Debug, Clone, PartialEq)]
pub struct Layout { pub lines: Vec<Line>, pub size: f32, pub line_height: f32, pub ascent: f32, pub width: f32, pub height: f32 }
```

| Field | Meaning |
|---|---|
| `Glyph::face` | index into the `FontSet`'s faces |
| `Glyph::id` | glyph id in that face (0 = `.notdef` for a missing character) |
| `Glyph::x` | pen position from the start of the line, pixels |
| `Glyph::advance` | pen movement after the glyph, kerning with the next glyph of the same face folded in; a tab has 4 times the advance of a space |
| `Glyph::character`, `byte` | the source `char` and its byte offset in the whole text |
| `Line::width` | width of what is drawn: trailing spaces and tabs hang and are not counted |
| `Line::bytes` | byte range of the text on the line, trailing spaces and the line break included |
| `Layout::size` | pixels per em as passed |
| `Layout::line_height` | `(ascent - descent + line_gap) * size / units_per_em` of the **primary** face |
| `Layout::ascent` | baseline offset from the top of a line (primary ascent scaled) |
| `Layout::width` | widest line |
| `Layout::height` | `line_height * lines.len()` |

### `FontSet`

```rust
pub struct FontSet { /* private: faces, resolved-character cache, shaped-layout cache, system fonts */ }
// Debug prints the number of faces
```

Interior mutability is by `Mutex` (poisoning is ignored), so a `FontSet` can be shared by reference across threads.

| Method | Signature | Behaviour |
|---|---|---|
| `new` | `#[must_use] pub fn new(primary: Arc<Font>) -> Self` | face 0 |
| `noto_sans` / `noto_sans_light` / `noto_sans_medium` / `noto_sans_bold` | `#[must_use] pub fn noto_sans() -> Self` (etc.) | `new` of the embedded face |
| `with_fallback` | `#[must_use] pub fn with_fallback(self, face: Arc<Font>) -> Self` | appended after existing faces |
| `with_system_fonts` | `#[must_use] pub fn with_system_fonts(mut self) -> Self` | `SystemFonts::default()` |
| `with_system_directories` | `#[must_use] pub fn with_system_directories(mut self, directories: Vec<PathBuf>) -> Self` | `SystemFonts::in_directories` |
| `face` | `#[must_use] pub fn face(&self, index: usize) -> Arc<Font>` | **panics** if out of range (indexes come from a `Layout` of this set) |
| `face_count` | `#[must_use] pub fn face_count(&self) -> usize` | includes system fonts loaded so far |
| `layout` | `#[must_use] pub fn layout(&self, text: &str, size: f32, max_width: Option<f32>) -> Layout` | see [internals](internals.md#line-breaking). Results are cached by (text, size bits, max_width bits); the cache is cleared when it reaches 4096 entries. |
| `measure` | `#[must_use] pub fn measure(&self, text: &str, size: f32) -> f32` | width of the text up to the first `\n`, unwrapped |
| `draw` | `pub fn draw<S: AsRef<[u8]> + AsMut<[u8]>>(&self, canvas: &mut Canvas<S>, layout: &Layout, x: f32, y: f32, color: Color)` | `draw_slanted` with 0 |
| `draw_slanted` | `pub fn draw_slanted<S: AsRef<[u8]> + AsMut<[u8]>>(&self, canvas: &mut Canvas<S>, layout: &Layout, x: f32, y: f32, color: Color, slant: f32)` | fills each non-whitespace glyph's path with `Canvas::fill_path`; advances are unchanged by `slant` |

## System fonts (`system.rs`)

```rust
#[derive(Debug)]
pub struct SystemFonts { /* directories, lazily built index behind a Mutex */ }
impl Default for SystemFonts   // in_directories(default list)
```

Default directories: `/usr/share/fonts`, `/usr/local/share/fonts`, `/system/fonts`, `/product/fonts`, and if `HOME` is set `$HOME/.local/share/fonts` and `$HOME/.fonts`.

| Method | Signature | Behaviour |
|---|---|---|
| `in_directories` | `#[must_use] pub const fn in_directories(directories: Vec<PathBuf>) -> Self` | |
| `candidates` | `#[must_use] pub fn candidates(&self, c: char) -> Vec<PathBuf>` | Files whose `cmap` covers `c`, in directory order. Builds the index on first call. A file may still lack the glyph. |
| `indexed` | `#[must_use] pub fn indexed(&self) -> usize` | Number of indexed files (0 before the first search). |

Indexing rules: recursion depth up to 6, at most 4096 files collected in total, entries sorted by file name; only files with extension `ttf` (case-insensitive) whose file name does not contain `emoji` (case-insensitive); the file must start with `00 01 00 00` or `true` (so CFF fonts are never indexed), at most 64 tables are read from the directory, `cmap` longer than 4 MiB is ignored. Files whose cmap cannot be read are dropped.

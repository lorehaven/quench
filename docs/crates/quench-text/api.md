---
title: API reference
parent: quench-text
nav_order: 2
---

The crate root (`src/lib.rs`) re-exports:

```rust
pub use font::{Font, FontError, Outline, Point};
pub use layout::{FontSet, Glyph, Layout, Line, noto_sans, noto_sans_bold, noto_sans_light, noto_sans_medium};
pub use system::SystemFonts;
```

Modules are private. 9 types, 4 free functions, and the methods below. No `unsafe`, no features.

## Font (`font.rs`)

### `FontError`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontError {
    NotTrueType,            // not an sfnt with TrueType outlines (OTTO/CFF is not read)
    Missing(&'static str),  // a required table is missing
    Truncated(&'static str),// a table is cut short or points outside the file
}
```

Implements `Display` (`"not a TrueType font"`, ``"the `{table}` table is missing"``, ``"the `{table}` table is cut short"``) and `std::error::Error`.

### `Point` and `Outline`

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point { pub x: f32, pub y: f32, pub on: bool }   // font units, y up; on = on-curve

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Outline { pub contours: Vec<Vec<Point>> }
```

| Method | Signature | Notes |
|---|---|---|
| `Outline::to_path` | `#[must_use] pub fn to_path(&self, scale: f32, x: f32, y: f32) -> Path` | Pixel `Path` (`quench_raster::Path`): point `(px, py)` maps to `(x + px*scale, y - py*scale)`. Quadratic curves; contours with fewer than 2 points are skipped; a contour with no on-curve point starts at the midpoint of its last and first points; two consecutive off-curve points get an implied on-curve midpoint. |
| `Outline::to_path_slanted` | `#[must_use] pub fn to_path_slanted(&self, scale: f32, x: f32, y: f32, slant: f32) -> Path` | As above, x additionally shifted by `slant * py * scale` (0.2 = synthetic italic). |

### `Font`

```rust
pub struct Font { /* private */ }   // Debug prints family, glyph count, units_per_em
```

| Method | Signature | Behaviour |
|---|---|---|
| `parse` | `pub fn parse(data: impl Into<Cow<'static, [u8]>>) -> Result<Self, FontError>` | Accepts sfnt version `0x00010000` or `true`. Required tables: `head`, `maxp`, `hhea`, `hmtx`, `loca`, `glyf`, `cmap` (missing: `FontError::Missing(name)`; a missing or non-Unicode cmap reports `Missing("cmap (a Unicode table)")`). `head` shorter than 54 bytes or units-per-em 0: `Truncated("head")`. Tables running past the end of the file count as missing. `name` and `GPOS` are optional. Bytes are kept (borrowed if `'static`). |
| `units_per_em` | `#[must_use] pub const fn units_per_em(&self) -> u16` | |
| `ascent` | `#[must_use] pub const fn ascent(&self) -> i16` | `hhea` ascender, font units |
| `descent` | `#[must_use] pub const fn descent(&self) -> i16` | `hhea` descender (negative) |
| `line_gap` | `#[must_use] pub const fn line_gap(&self) -> i16` | `hhea` line gap |
| `glyph_count` | `#[must_use] pub const fn glyph_count(&self) -> u16` | `maxp` numGlyphs |
| `glyph` | `#[must_use] pub fn glyph(&self, c: char) -> Option<u16>` | cmap lookup (format 4 for code points up to U+FFFF, format 12 otherwise); `None` if absent, if glyph 0 (`.notdef`) or an id `>= glyph_count` |
| `advance` | `#[must_use] pub fn advance(&self, glyph: u16) -> u16` | `hmtx` advance in font units; glyphs past `numberOfHMetrics` use the last metric; 0 if the table is unreadable |
| `kerning` | `#[must_use] pub fn kerning(&self, left: u16, right: u16) -> i16` | GPOS pair adjustment (x advance of the first glyph), font units; negative pulls together; 0 when the font has none |
| `name` | `#[must_use] pub fn name(&self, id: u16) -> Option<String>` | From `name`: ID 1 family, 2 style, 0 copyright, 13 licence, 14 licence URL. Prefers platform 3 (encodings 1, 10) or platform 0 (UTF-16BE), falls back to Mac Roman platform 1 encoding 0 read byte per char. First match wins. |
| `outline` | `#[must_use] pub fn outline(&self, glyph: u16) -> Option<Outline>` | `None` for a glyph with no outline (space), a damaged glyph, or one over the caps (component depth 8, 64 components, 20 000 points). Composite glyphs: offsets (args as words or signed bytes), scale, x/y scale, 2x2 transform. A component positioned by point matching (flag `ARGS_ARE_XY_VALUES` clear) is left out. Hinting instructions are skipped. |

## Layout (`layout.rs`)

### Embedded faces

```rust
#[must_use] pub fn noto_sans() -> Arc<Font>          // Regular
#[must_use] pub fn noto_sans_bold() -> Arc<Font>
#[must_use] pub fn noto_sans_light() -> Arc<Font>    // doc: weights up to 350
#[must_use] pub fn noto_sans_medium() -> Arc<Font>   // doc: weights from 450 to 599
```

Each parses its embedded file once (`OnceLock`) and returns a clone of the `Arc`. They carry a `# Panics` section: never for the embedded file.

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

Indexing rules: recursion depth up to 6, at most 4096 files collected in total, entries sorted by file name; only files with extension `ttf` (case-insensitive) whose file name does not contain `emoji` (case-insensitive); the file must start with `00 01 00 00` or `true`, at most 64 tables are read from the directory, `cmap` longer than 4 MiB is ignored. Files whose cmap cannot be read are dropped.

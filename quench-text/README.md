# quench-text
Fonts and text with no dependencies: a TrueType parser, kerning, the OpenType MATH table, line breaking, Noto Sans embedded (Noto Sans Math optional), drawn through `quench-raster`.

## What it is
`quench-text` turns a string into pixels with no third-party dependencies. It has a TrueType parser, pair kerning, line breaking, four weights of Noto Sans embedded so text looks the same on every machine, and a fallback to fonts installed on the machine for characters Noto lacks.

For math typesetting it reads a font's OpenType `MATH` table (`Font::math`): the constants (axis height, fraction and
script shifts, rule thicknesses, radical gaps, ...), italic corrections, top accent attachments, extended shapes,
math kerns, and the larger variants and glyph assemblies that grow delimiters, radicals and arrows. Glyph bounding
boxes come from `Font::bounds`. The `math-font` feature (off by default; 1 MB) embeds Noto Sans Math
(`noto_sans_math()`). CFF (`OTTO`) fonts are refused with `FontError::Cff`.

It draws through `quench-raster`. `quench-layout` measures text with it and `quench-paint` draws with it.

## Use cases
- Measure the width of a label before laying it out.
- Wrap a paragraph to a given width and draw it onto a `quench-raster` canvas.
- Find a caret position from a byte offset in a text input.
- Load your own TrueType font, with Noto or system fonts as fallback.
- Get glyph outlines as paths for custom drawing.
- Read a math font's `MATH` table to typeset formulas.

## Example
```rust
use quench_raster::{Canvas, Color};
use quench_text::FontSet;

let fonts = FontSet::noto_sans();
let layout = fonts.layout("Zażółć gęślą jaźń: Привет", 16.0, Some(120.0));
let mut canvas = Canvas::new(140, 120);
fonts.draw(&mut canvas, &layout, 8.0, 8.0, Color::WHITE);
assert!(layout.lines.len() >= 2 && layout.width <= 120.0);
```

Fallback faces and system fonts:

```rust
use quench_text::{FontSet, noto_sans, noto_sans_bold};

let fonts = FontSet::noto_sans_bold()
    .with_fallback(noto_sans())
    .with_system_fonts();
```

Math font (with the `math-font` feature):

```rust,ignore
use quench_text::noto_sans_math;

let font = noto_sans_math();
let math = font.math().unwrap();
let axis = math.constants().unwrap().axis_height; // 278 font units of 1000
let paren = math.vertical_construction(font.glyph('(').unwrap()).unwrap();
assert!(paren.assembly.is_some());
```

## Documentation
Full documentation: https://lorehaven.github.io/quench/crates/quench-text/

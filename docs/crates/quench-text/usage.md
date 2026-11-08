---
title: Usage
parent: quench-text
grand_parent: Crates
nav_order: 1
---

## 1. Measure text

```rust
use quench_text::FontSet;

let fonts = FontSet::noto_sans();
let w = fonts.measure("Hello", 16.0);          // width of one line, in pixels
assert!(w > 0.0);
```

`measure` lays out only the part of the text before the first `\n`, with no wrapping.

## 2. Wrap to a width and draw

```rust
use quench_raster::{Canvas, Color};
use quench_text::FontSet;

let fonts = FontSet::noto_sans();
let layout = fonts.layout("a long sentence that wraps", 16.0, Some(100.0));
let mut canvas = Canvas::new(120, (layout.height.ceil() as usize) + 8);
fonts.draw(&mut canvas, &layout, 4.0, 4.0, Color::BLACK);
println!("{} lines, {}x{}", layout.lines.len(), layout.width, layout.height);
```

`(x, y)` is the top left of the text box; the baseline of line `n` is `y + layout.ascent + layout.line_height * n`.

## 3. A caret position from a byte offset

Each `Glyph` carries `byte`, the byte offset of its character in the text, and `x`, its pen position from the start of its line. Each `Line` has `bytes`, the byte range of the text it holds.

```rust
use quench_text::FontSet;

let fonts = FontSet::noto_sans();
let text = "ab\ncd";
let layout = fonts.layout(text, 16.0, None);
let caret = 4;      // before 'd'
let (row, line) = layout.lines.iter().enumerate()
    .find(|(_, l)| l.bytes.contains(&caret)).unwrap();
let x = line.glyphs.iter().find(|g| g.byte == caret).map_or(line.width, |g| g.x);
assert_eq!(row, 1);
assert!(x > 0.0);
```

A text that is empty or ends with `\n` gets a final empty line (`bytes` is `len..len`) for the caret.

## 4. Another weight, a fallback face, system fonts

```rust
use quench_text::{FontSet, noto_sans, noto_sans_bold};

let fonts = FontSet::noto_sans_bold()
    .with_fallback(noto_sans())      // tried after the primary
    .with_system_fonts();            // then the machine's .ttf files
assert!(fonts.face_count() >= 2);
```

Characters are resolved face by face in order: the primary, the fallbacks in the order added, then the system fonts. A character no face has is drawn as the primary face's `.notdef` glyph (id 0).

## 5. Load your own TrueType font

```rust
use std::sync::Arc;
use quench_text::{Font, FontSet, noto_sans};

let bytes = std::fs::read("MyFont.ttf").unwrap();            // Vec<u8> -> Cow::Owned
let font = Font::parse(bytes).expect("a TrueType font");
let fonts = FontSet::new(Arc::new(font)).with_fallback(noto_sans());
```

`Font::parse` takes anything `Into<Cow<'static, [u8]>>`: a `Vec<u8>` or a `&'static [u8]` (such as `include_bytes!`). Fonts with CFF outlines (`OTTO`, usually `.otf`) are refused with `FontError::Cff` (displayed as `CFF outlines are not read`), so a caller can tell the user why; `.ttc` collections and anything else that is not a TrueType sfnt are refused with `FontError::NotTrueType`.

```rust
use quench_text::{Font, FontError};

match Font::parse(std::fs::read("Some.otf").unwrap()) {
    Ok(font) => println!("{:?}", font.name(1)),
    Err(FontError::Cff) => eprintln!("CFF fonts are not supported; use the TrueType build"),
    Err(e) => eprintln!("not a usable font: {e}"),
}
```

## 6. Glyph outlines as paths

```rust
use quench_text::noto_sans;
use quench_raster::{Canvas, Color};

let font = noto_sans();
let glyph = font.glyph('A').unwrap();
let outline = font.outline(glyph).unwrap();
let scale = 48.0 / f32::from(font.units_per_em());
let path = outline.to_path(scale, 10.0, 50.0);     // origin of the glyph at (10, 50), y down
let mut c = Canvas::new(80, 60);
c.fill_path(&path, 0.0, 0.0, Color::BLACK);
```

## 7. Glyph metrics and bounding boxes

```rust
use quench_text::noto_sans;

let font = noto_sans();
let o = font.bounds(font.glyph('o').unwrap()).unwrap();      // Bounds { x_min, y_min, x_max, y_max }
let h = font.bounds(font.glyph('H').unwrap()).unwrap();
assert!(o.y_min < 0 && o.y_max > 0);                          // 'o' overshoots the baseline
assert!(h.y_min == 0 && h.y_max > o.y_max);
assert_eq!(font.bounds(font.glyph(' ').unwrap()), None);     // no outline, no box
let em = f32::from(font.units_per_em());
let ascent_px = f32::from(font.ascent()) * 16.0 / em;        // font units to pixels at 16 px per em
```

`bounds` reads the box from the glyph's header without parsing its outline, so it is cheap. All values are font units with y up; scale by `size / units_per_em` for pixels.

## 8. Read a math font's constants

With the `math-font` feature:

```rust
use quench_text::noto_sans_math;

let font = noto_sans_math();
let math = font.math().expect("Noto Sans Math has a MATH table");
let k = math.constants().unwrap();
assert_eq!((k.script_percent_scale_down, k.script_script_percent_scale_down), (60, 50));
assert_eq!(k.axis_height, 278);                 // the math axis: fraction bars and centred delimiters
let rule = k.fraction_rule_thickness;           // 71 units in this font
let em = f32::from(font.units_per_em());        // 1000
let axis_px = f32::from(k.axis_height) * 24.0 / em;
```

The table is parsed on the first `math()` call and kept: later calls return the same reference. A text font gives `None` (`quench_text::noto_sans().math()` is `None`). Any TrueType font with a `MATH` table works the same way through `Font::parse`.

## 9. Italic corrections, accents and math kerns

```rust
use quench_text::{MathKernCorner, noto_sans_math};

let font = noto_sans_math();
let math = font.math().unwrap();
let f = font.glyph('\u{1D453}').unwrap();                  // MATHEMATICAL ITALIC SMALL F (format 12 cmap)
assert_eq!(math.italics_correction(f), Some(145));         // a superscript after it moves 145 units right
let accent_x = math.top_accent_attachment(f).unwrap();     // inside the ink: between x_min and x_max
let b = font.bounds(f).unwrap();
assert!(b.x_min < accent_x && accent_x < b.x_max);
assert_eq!(math.kern(f, MathKernCorner::TopRight, 500), 0); // Noto Sans Math has no math kerns
```

Pitfalls: math letters are in the Mathematical Alphanumeric Symbols block, and that block has holes for letters encoded earlier: `font.glyph('\u{1D455}')` (italic h) is `None`; italic h is U+210E. `kern` returns 0 for a glyph or corner the font says nothing about, so the call is safe on any font; for the band rule see [`MathKern::at`](api.md#mathkerncorner-mathkern-mathkerninfo).

## 10. Grow a delimiter: variants, then the assembly

```rust
use quench_text::noto_sans_math;

let font = noto_sans_math();
let math = font.math().unwrap();
let paren = font.glyph('(').unwrap();
let grow = math.vertical_construction(paren).unwrap();
assert_eq!(grow.variants[0].glyph, paren);                 // the first variant is the glyph itself
let wanted = 2000;                                          // height plus depth, font units
match grow.variants.iter().find(|v| v.advance >= wanted) {
    Some(v) => println!("variant glyph {} ({} units)", v.glyph, v.advance),   // glyph 2200, 2119 units
    None => {
        let assembly = grow.assembly.as_ref().unwrap();     // bottom hook, extender, top hook
        let overlap = math.min_connector_overlap();         // 100 in Noto Sans Math
        println!("{} parts, overlap at least {overlap}", assembly.parts.len());
    }
}
```

Horizontal growth (`horizontal_construction`) works the same for braces (`U+23DE`, `U+23DF`), arrows and wide accents. What Noto Sans Math has is listed in [internals](internals.md#noto-sans-math). Pitfall: an assembly's connector lengths are not always within the part's own length in real fonts (see the same section), so clamp them to `full_advance` before using them as overlaps.

## Pitfalls

Layouts are cached by `(text, size bits, width bits)` in a `FontSet`; the set is `Sync` (internal mutexes) but a `Layout` is only valid for the `FontSet` that made it (its `face` indexes refer to that set's face list).

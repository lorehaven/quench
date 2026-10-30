---
title: Usage
parent: quench-text
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

`Font::parse` takes anything `Into<Cow<'static, [u8]>>`: a `Vec<u8>` or a `&'static [u8]` (such as `include_bytes!`). Fonts with CFF outlines (`OTTO`) and `.ttc` collections are refused with `FontError::NotTrueType`.

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

Pitfalls: layouts are cached by `(text, size bits, width bits)` in a `FontSet`; the set is `Sync` (internal mutexes) but a `Layout` is only valid for the `FontSet` that made it (its `face` indexes refer to that set's face list).

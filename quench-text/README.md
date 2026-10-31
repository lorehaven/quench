# quench-text
Fonts and text with no dependencies: a TrueType parser, kerning, line breaking, Noto Sans embedded, drawn through `quench-raster`.

## What it is
`quench-text` turns a string into pixels with no third-party dependencies. It has a TrueType parser, pair kerning, line breaking, four weights of Noto Sans embedded so text looks the same on every machine, and a fallback to fonts installed on the machine for characters Noto lacks.

It draws through `quench-raster`. `quench-layout` measures text with it and `quench-paint` draws with it.

## Use cases
- Measure the width of a label before laying it out.
- Wrap a paragraph to a given width and draw it onto a `quench-raster` canvas.
- Find a caret position from a byte offset in a text input.
- Load your own TrueType font, with Noto or system fonts as fallback.
- Get glyph outlines as paths for custom drawing.

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

## Documentation
Full documentation: https://lorehaven.github.io/quench/crates/quench-text/

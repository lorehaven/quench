---
title: Usage
parent: quench-tex
grand_parent: Crates
nav_order: 1
---

Examples 1 to 5 need no feature; 6 to 10 need `features = ["typeset"]`. Outputs are the ones the tests assert (`tests/unit/inline_tests.rs`, `block_tests.rs`, `macro_tests.rs`, `error_tests.rs`, `tests/typeset/*.rs`) or were printed from the crate as it is.

## 1. A formula as one line of Unicode

Goal: show inline math (`$…$`) where only text is possible: a terminal line, a log, an alt text.

```rust
use quench_tex::{parse, to_unicode};

let text = |tex: &str| to_unicode(&parse(tex).unwrap());
assert_eq!(text(r"\frac{a+b}{2} \le \sqrt{x}"), "(a + b)/2 ≤ √x");
assert_eq!(text(r"\sum_{i=1}^n i^2 \le \frac{n^3}{2}"), "∑ᵢ₌₁ⁿ i² ≤ n³/2");
assert_eq!(text(r"e^{i\pi}"), "e^(iπ)");                 // no superscript π in Unicode
assert_eq!(text(r"\lim_{x\to0} f(x)"), "lim_(x→0) f(x)");
assert_eq!(text(r"\frac{1}{2}x"), "(1/2)x");             // not 1/2x
assert_eq!(text(r"\mathbb{R}^n"), "ℝⁿ");
assert_eq!(text(r"\begin{pmatrix}a&b\\c&d\end{pmatrix}"), "(a b; c d)");
assert_eq!(text(r"|x|=\begin{cases}x&x\ge0\\-x&\text{otherwise}\end{cases}"),
           "|x| = {x, x ≥ 0; −x, otherwise}");
```

Pitfalls: the output uses combining characters for accents (`\hat x` is `x` + U+0302) and characters outside the Basic Multilingual Plane for alphabets (`\mathbb{A}` is `𝔸`); measure it with `display_width`, not `len()` or `chars().count()`. Colours are dropped. Spacing follows TeX's classes, so `a-b` is `a − b` but `-x` is `−x`.

## 2. Display math as a block of terminal cells

Goal: show `$$…$$` in a terminal with fractions stacked, limits above and below, delimiters grown.

```rust
use quench_tex::{parse, to_text_block};

let block = to_text_block(&parse(r"\sum_{n=0}^{\infty} \frac{x^n}{n!}")?);
assert_eq!(block.lines, [" ∞  xⁿ", " ∑  ──", "n=0 n!"]);
assert_eq!((block.baseline, block.width), (1, 6));

let terminal_width: usize = 80;
let indent = " ".repeat(terminal_width.saturating_sub(block.width) / 2);
for line in &block.lines {
    println!("{indent}{line}");          // the caller centres the block
}
# Ok::<(), quench_tex::Error>(())
```

`baseline` is the row the formula's baseline is on: put a one-line neighbour (an equation number, a trailing comma) on that row. More shapes, from `tests/unit/block_tests.rs`:

```text
\int_0^1 f(x)\,dx          \begin{bmatrix} 1 & 0 & 0 \\ 0 & 1 & 0 \\ 0 & 0 & 1 \end{bmatrix}
 1                         ⎡1  0  0⎤
∫  f(x) dx                 ⎢0  1  0⎥
 0                         ⎣0  0  1⎦

\overbrace{a+b+c}^{3}      \sqrt{x+1}         \left\{ \frac{a}{b} \middle| c \right\}
    3                       _____             ⎧a │  ⎫
╭───┴───╮                  √x + 1             ⎨─ │ c⎬
a + b + c                                     ⎩b │  ⎭
```

Pitfalls: the block needs a terminal font with the bracket pieces (U+239B to U+23AD) and box-drawing characters. Short scripts stay inline (`x² + aᵢ`); scripts without Unicode forms are raised a row (`e^{i\pi}` becomes two rows).

## 3. Errors: show the source instead

Goal: never show half a formula. Anything the reader does not know is an error with a byte offset.

```rust
use quench_tex::{Error, parse, to_unicode};

fn inline(tex: &str) -> String {
    match parse(tex) {
        Ok(formula) => to_unicode(&formula),
        Err(error) => {
            eprintln!("{error}");                 // unsupported TeX command `\foo` at byte 4
            format!("${tex}$")                    // fall back to the source
        }
    }
}

let tex = r"a + \foo{b}";
match parse(tex) {
    Err(Error::Unsupported { command, at }) => {
        assert_eq!((command.as_str(), at), (r"\foo", 4));
        assert_eq!(&tex[at..], r"\foo{b}");       // `at` is a byte offset into the source
    }
    other => panic!("{other:?}"),
}
let error = parse(r"\frac{a}{b").unwrap_err();
assert_eq!(error.to_string(), "TeX syntax error at byte 8: unbalanced braces: `{` without `}`");
assert_eq!(error.at(), 8);
assert_eq!(inline(r"\frac12"), "1/2");
```

Use this to validate the TeX of a page before it reaches KaTeX or MathJax: point at `at` in the source. Pitfall: `at` counts bytes, so convert to a column with `tex[..at].chars().count()` before showing a caret under non-ASCII text. All error messages are listed in [supported TeX](supported.md#errors).

## 4. Macros across the formulas of a page

Goal: a page defines `\R` once and uses it in later formulas.

```rust
use quench_tex::{Macros, parse_with, to_unicode};

let mut macros = Macros::new();
parse_with(r"\newcommand{\R}{\mathbb{R}}", &mut macros)?;          // a formula that only defines
assert!(macros.contains("R"));
let f = parse_with(r"f\colon \R \to \R", &mut macros)?;
assert_eq!(to_unicode(&f), "f: ℝ → ℝ");

// A failing formula leaves the definitions as they were.
assert!(parse_with(r"\newcommand{\Z}{\mathbb{Z}} \unknown", &mut macros).is_err());
assert!(!macros.contains("Z"));

// Definitions from code, as \newcommand{\abs}[1]{\left|#1\right|} would make.
macros.define("abs", 1, r"\left|#1\right|")?;
assert_eq!(to_unicode(&parse_with(r"\abs{-1}", &mut macros)?), "|−1|");
# Ok::<(), quench_tex::Error>(())
```

Optional arguments, `\def`, `\DeclareMathOperator` and `\providecommand` work too (`\DeclareMathOperator*{\esssup}{ess\,sup} \esssup_x f` is `ess supₓ f`). Pitfall: `parse` (without `_with`) forgets definitions after each formula; a recursive macro is an error (`macro expansion too deep …`), not a hang.

## 5. Walk the syntax tree

Goal: inspect a formula, for a linter or another output format.

```rust
use quench_tex::{Class, Node, parse};

fn fractions(nodes: &[Node]) -> usize {
    nodes.iter().map(|node| match node {
        Node::Frac { num, den, .. } => 1 + fractions(num) + fractions(den),
        Node::Group(body) | Node::Sqrt { body, .. } | Node::Fenced { body, .. } => fractions(body),
        Node::Scripts { base, sub, sup } => {
            fractions(std::slice::from_ref(&**base))
                + sub.as_deref().map_or(0, fractions)
                + sup.as_deref().map_or(0, fractions)
        }
        _ => 0,
    }).sum()
}

let formula = parse(r"\frac{1}{1+\frac{1}{x}} + \sqrt{\frac{a}{b}}")?;
assert_eq!(fractions(&formula.body), 3);

let classes: Vec<Option<Class>> = parse(r"a + b = -c")?.body.iter().map(Node::class).collect();
// The tree keeps the class as written: the `-` after `=` is still `Bin`; the outputs treat it as ordinary.
assert_eq!(classes, [Some(Class::Ord), Some(Class::Bin), Some(Class::Ord), Some(Class::Rel), Some(Class::Bin), Some(Class::Ord)]);
# Ok::<(), quench_tex::Error>(())
```

The tree keeps what TeX's layout needs: classes, `Limits`, styles, delimiters, column alignment, explicit spaces in their unit (`Space::Mu`, `Space::Em`). Letters are plain (`x`, not `𝑥`); alphabets are `Node::Font` around them, and `math_alphanumeric` maps a character into one.

## 6. PNG and SVG files

Goal: render a formula to an image (feature `typeset`).

```rust
use quench_tex::typeset::{Color, RenderOptions, SvgOptions, tex_to_png, tex_to_svg};

let png: Vec<u8> = tex_to_png(
    r"x = \frac{-b \pm \sqrt{b^2 - 4ac}}{2a}",
    true,                                             // display style ($$…$$)
    &RenderOptions { pixels_per_em: 40.0, background: Some(Color::WHITE), ..RenderOptions::default() },
)?;
std::fs::write("quadratic.png", &png).unwrap();

let svg: String = tex_to_svg(r"\int_0^1 x^2\,dx", false, &SvgOptions::default())?;
std::fs::write("integral.svg", &svg).unwrap();
# Ok::<(), quench_tex::typeset::Error>(())
```

`RenderOptions::default()` is 32 px per em, black on transparent, 2 px of padding. The PNG is straight-alpha RGBA. Errors are `typeset::Error`: `Parse` (the TeX), `MissingGlyph` (`\text{漢}`: the math font has no glyph for `漢` (U+6F22)), `UnknownColour` (`\color{nocolour}`). Pitfall: `tex_to_png` and `tex_to_svg` use `parse`, so a page's macros are not applied: use `parse_with` and then `typeset` (example 8).

## 7. Inline SVG in HTML, on the text's baseline

Goal: static math in a web page, no script, in the page's text colour and size.

```rust
use quench_tex::typeset::{SvgOptions, tex_to_svg};

let svg = tex_to_svg(r"\int_0^1 x^2\,dx", false, &SvgOptions::default())?;
assert!(svg.starts_with(r#"<svg xmlns="http://www.w3.org/2000/svg" width="3.6867em" height="1.4654em" viewBox="0 -1.0319 3.6867 1.4654" style="vertical-align: -0.4335em" fill="currentColor">"#));
let html = format!("<p>The area is {svg}, which is 1/3.</p>");
# Ok::<(), quench_tex::typeset::Error>(())
```

The `<svg>` has no XML declaration, so it can go straight into HTML. Its size is in `em`, so it follows the surrounding font size; `fill="currentColor"` takes the text colour (dark mode included); `vertical-align` is minus what lies below the baseline, so the formula sits on the line. `\color{red}` parts carry their own `fill="#ff0000"`. For a fixed colour use `SvgOptions { colour: Some(Color::rgb(0x20, 0x40, 0x60)), padding: 0.1 }`. Pitfalls: every glyph is its own `<path>` (no shared `<defs>`), so a page with hundreds of formulas is large; the SVG has no `<title>`, so add `role="img"` and an `aria-label` (from `to_unicode`) yourself. An HTML pipeline that builds pages with [quench-html](../quench-html/index.md) can insert the string as raw markup.

## 8. Show a typeset formula in the terminal (kitty, sixel)

Goal: a terminal Markdown viewer shows display math as an image where the terminal can draw one, and as text elsewhere. [quench-cli](../quench-cli/index.md)'s `graphics` module speaks the protocols; [quench-markdown-term](../quench-markdown-term/index.md) is the viewer that uses quench-tex's text output (its `tex` feature).

```rust
use quench_cli::graphics::{Rgb, kitty, sixel};
use quench_tex::typeset::{RenderOptions, to_png, to_rgba, typeset};
use quench_tex::{Macros, parse_with};

let mut macros = Macros::new();
let formula = parse_with(r"\sum_{i=1}^n i = \frac{n(n+1)}{2}", &mut macros)?;
let set = typeset(&formula, true)?;
let options = RenderOptions { pixels_per_em: 24.0, ..RenderOptions::default() };   // about one cell height

// kitty: send a PNG.
let png = to_png(&set, &options);
print!("{}", kitty::transmit_png(&png, 1, kitty::Placement::default()));

// sixel: send the RGBA pixels, blended onto the terminal's background.
let raster = to_rgba(&set, &options);
let sixels = sixel::encode(&raster.rgba, raster.width as usize, raster.height as usize, Rgb::WHITE)?;
print!("{sixels}");
# Ok::<(), Box<dyn std::error::Error>>(())
```

Choose the foreground colour (`RenderOptions::colour`) from the terminal's (quench-cli's `detect` can ask for it), and `pixels_per_em` from its cell height in pixels. `Raster::baseline` is the pixel row of the baseline: to line the image up with the text row, place it so that row falls on the text baseline. Fall back to `to_text_block` (example 2) when the terminal has no graphics.

## 9. The display list: positions, ink and your own renderer

Goal: draw a formula with another renderer, or lay it out next to text yourself.

```rust
use quench_tex::parse;
use quench_tex::typeset::{Item, typeset};

let set = typeset(&parse("x^2")?, false)?;
// Font units of the math font (1000 per em), y up, baseline at 0.
assert_eq!(set.units_per_em, 1000.0);
for item in &set.items {
    match *item {
        Item::Glyph { font, glyph, x, y, size, colour } => {
            // x^2: glyph 1056 (math italic x) at (0, 0) size 1000, glyph 1781 ('2') at (567, 390) size 600
            let outline = set.fonts.face(font).outline(glyph);       // quench_text::Outline, y up
            let _ = (x, y, size, colour, outline);
        }
        Item::Rule { x, y, width, height, .. } => { let _ = (x, y, width, height); }
    }
}
let (x0, y0, x1, y1) = set.ink().unwrap();       // the ink may reach past the box
assert!(set.ascent > 0.0 && set.descent >= 0.0 && x1 > x0 && y1 > y0);
# Ok::<(), quench_tex::typeset::Error>(())
```

To place glyphs in pixels with y down, use `scale = size / face.units_per_em() × px_per_unit` for the outline and `(x · px_per_unit, −y · px_per_unit)` for its origin, as `to_rgba` does. Pitfall: a glyph of `Face::TextItalic` must be drawn slanted (`Face::slant()` is 0.2); the box metrics (`width`, `ascent`, `descent`) are TeX's, so use `ink()` to size an image.

## 10. Another math font

Goal: typeset with a TrueType math font of your own.

```rust
use std::sync::Arc;
use quench_tex::parse;
use quench_tex::typeset::{Error, Fonts, typeset_with};
use quench_text::Font;

let math = Font::parse(std::fs::read("SomeMath.ttf").unwrap()).expect("a TrueType font");
let fonts = Fonts::with_math(Arc::new(math))?;          // Error::NotMathFont without MATH constants
let set = typeset_with(&parse(r"\sqrt{x^2+1}")?, true, &fonts)?;

assert_eq!(Fonts::with_math(quench_text::noto_sans()).err(), Some(Error::NotMathFont));
# Ok::<(), Box<dyn std::error::Error>>(())
```

The text faces stay Noto Sans. Pitfalls: CFF math fonts (most `.otf` builds, such as Latin Modern Math or STIX Two OTF) are refused by `quench_text::Font::parse` with `FontError::Cff`; use a TrueType build. Math kerns of the font are not used (see [internals](internals.md#appendix-g-what-is-followed-and-what-is-approximated)). Positions are in the new font's units (`set.units_per_em`).

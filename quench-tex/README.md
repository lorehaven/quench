# quench-tex
A reader for TeX math, with Unicode and two-dimensional text output, no dependencies; and, with the `typeset` feature, a typesetter that lays formulas out by TeX's rules with an OpenType MATH font and draws them to PNG and SVG.

## What it is
`quench-tex` reads the TeX math people write in Markdown pages (`$…$`, `$$…$$`) into a syntax tree, and writes it as text: one line of Unicode for inline math (`\frac{a+b}{2} \le \sqrt{x}` is `(a + b)/2 ≤ √x`), or a block of terminal cells for display math, with stacked fractions, raised scripts, limits above and below operators, grown delimiters and aligned matrices.

The reader covers the math of plain TeX and amsmath that pages use: Greek and the usual symbols with their TeX classes, large and named operators with their limits, fractions and binomials, radicals, `\left`/`\middle`/`\right` and `\big` sizes, accents, over- and underbraces, `\mathbb` and the other alphabets, `\text`, spacing, colour, the `matrix`, `cases`, `aligned`, `gathered` and `array` environments, and macros (`\newcommand`, `\def`, `\DeclareMathOperator`) kept across a page's formulas. Anything else is an error naming the command, never half a translation, so the caller can show the TeX source instead.

The tree keeps what TeX's layout needs (atom classes, limits, styles, delimiters, column alignment), for a typesetter to work from. There is no TeX installation, font or renderer involved.

## The typesetter (feature `typeset`)
`quench_tex::typeset` turns the same tree into a display list of glyphs and rules, laid out by *The TeXbook*'s Appendix G with the font's OpenType `MATH` constants in place of TeX's font parameters (as the OpenType MATH specification and MathML Core describe): display, text, script and scriptscript styles and their cramped forms, scaled by the font's script percentages; the spacing table between atom classes with TeX's binary-to-ordinary rule; fractions and `\binom` with their shifts and gap minima and the rule on the math axis; radicals grown from the font's variants and assemblies, with degrees; sub- and superscripts with italic corrections; large operators (and `\mathop{…}`) at display size with limits above and below or beside, `\limits` and `\nolimits` included; `\left`, `\middle`, `\right` grown by TeX's `\delimiterfactor` / `\delimitershortfall` rule and `\big` sizes; accents on the top accent attachment, wide accents and arrows; over- and underlines, braces with labels, `\overset`-style stacks; arrays with column alignment, rules and delimiters; `\text` in Noto Sans; `\mathbb` and the other alphabets by the Unicode math alphanumerics; colours by name, as hex (`#1a2b3c`, `#f80`) or in the `HTML`, `rgb`, `RGB` and `gray` models.

The math font is Noto Sans Math (SIL Open Font License), embedded through `quench-text`'s `math-font` feature, so formulas typeset on any machine. A character the fonts lack is an error naming it, never a half-drawn formula. The display list is drawn antialiased to RGBA or PNG (`quench-raster`, `quench-image`), or written as SVG with one path per glyph, in ems, in `currentColor`, with the baseline as its `vertical-align`.

What is simplified: the font's math kerns (`MathKernInfo`) are not used, so scripts are placed with italic corrections alone (Noto Sans Math has none, so the bundled output is unaffected); the `MATH` constants `MathLeading`, `DelimitedSubFormulaMinHeight`, `FlattenedAccentBaseHeight`, `StretchStackTopShiftUp`, `StretchStackBottomShiftDown`, `SkewedFractionHorizontalGap` and `SkewedFractionVerticalGap` are not used; `\cfrac` is set as `\dfrac`; primes are set at the size of their base; script glyphs are the text glyphs scaled (no `ssty` variants); `\textit` is Noto Sans slanted.

Without the feature, the crate has no dependencies.

## Use cases
- Show the math of a Markdown page in a terminal (loupe): as text, or typeset as an image where the terminal shows images.
- Render formulas as static SVG in HTML, with no script, or as PNG.
- Turn a formula into plain Unicode for a commit message, a log line or an alt text.
- Validate the TeX of a page before it reaches KaTeX or MathJax, and point at the unknown command.
- Read formulas into a tree for a typesetter or another output format.

## Example
```rust
use quench_tex::{Macros, parse, parse_with, to_text_block, to_unicode};

let formula = parse(r"\sum_{i=1}^n i^2 \le \frac{n^3}{2}")?;
assert_eq!(to_unicode(&formula), "∑ᵢ₌₁ⁿ i² ≤ n³/2");

let block = to_text_block(&parse(r"\begin{pmatrix} a & b \\ c & d \end{pmatrix}")?);
assert_eq!(block.lines, ["⎛a  b⎞", "⎝c  d⎠"]);

let mut macros = Macros::new();
parse_with(r"\newcommand{\R}{\mathbb{R}}", &mut macros)?;
assert_eq!(to_unicode(&parse_with(r"f\colon \R \to \R", &mut macros)?), "f: ℝ → ℝ");

assert!(parse(r"\unknown{x}").is_err());
```

With the `typeset` feature:
```rust
use quench_tex::typeset::{RenderOptions, SvgOptions, tex_to_png, tex_to_svg, to_rgba, typeset};

let png = tex_to_png(r"x = \frac{-b \pm \sqrt{b^2 - 4ac}}{2a}", true, &RenderOptions::default())?;
let svg = tex_to_svg(r"\int_0^1 x^2\,dx", false, &SvgOptions::default())?;

// The display list, in font units with the baseline at y = 0, and its pixels for other encoders (sixel).
let set = typeset(&quench_tex::parse(r"\sum_{i=1}^n i")?, true)?;
let raster = to_rgba(&set, &RenderOptions { pixels_per_em: 24.0, ..RenderOptions::default() });
assert!(raster.baseline < raster.height);
```

## Documentation
Full documentation: https://lorehaven.github.io/quench/crates/quench-tex/

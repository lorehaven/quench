---
title: quench-tex
parent: Crates
nav_order: 54
has_children: true
permalink: /crates/quench-tex/
---

`quench-tex` (crate `quench_tex`) reads the TeX math people write in Markdown pages (`$…$`, `$$…$$`) into a syntax tree, and writes it as text: one line of Unicode for inline math, or a block of terminal cells for display math, with stacked fractions, raised scripts, limits above and below operators, grown delimiters and aligned matrices. With the `typeset` feature it also lays formulas out by *The TeXbook*'s Appendix G with an OpenType `MATH` font (Noto Sans Math, embedded through [quench-text](../quench-text/index.md)) and draws them to PNG or SVG. There is no TeX installation, no KaTeX or MathJax, and without the feature no dependency at all.

A formula with anything the reader does not know is an [`Error`](api.md#errors) naming the command and its byte offset, never a partial result, so a caller can show the TeX source instead of half a translation.

* Version: `0.1.0`. Edition 2024. No `rust-version`. `[lints] workspace = true`; no `unsafe`.
* `autotests = false`; two test targets, `unit` and `typeset` (the second needs the `typeset` feature). The manifest has no `include` list.
* Users in this workspace: [quench-markdown-term](../quench-markdown-term/index.md) shows `$…$` and `$$…$$` in the terminal with `parse_with`, `to_unicode` and `to_text_block` (its `tex` feature).

## Install

```toml
[dependencies]
quench-tex = { version = "0.1", registry = "ennor" }                              # reader and text output
quench-tex = { version = "0.1", registry = "ennor", features = ["typeset"] }      # plus PNG and SVG
```

(In the workspace: `{ workspace = true }`, with `features = ["typeset"]` as needed.)

## Features

| Feature | Default | What it adds |
|---|---|---|
| `typeset` | off | The public module `quench_tex::typeset`: the typesetter, `to_png`/`to_rgba`/`to_svg`, `tex_to_png`/`tex_to_svg`. Turns on the optional dependencies `quench-text` (with its `math-font` feature, which embeds Noto Sans Math, about 1 MB), `quench-raster` and `quench-image`. |

## Dependencies

| Dependency | Kind | Why |
|---|---|---|
| (none) | | without `typeset` the crate depends on nothing |
| `quench-text` (`features = ["math-font"]`) | optional (`typeset`) | the `MATH` table reader, glyph outlines and bounds, the embedded Noto Sans Math and Noto Sans for `\text` |
| `quench-raster` | optional (`typeset`) | antialiased filling of glyph outlines and rules; `Color` |
| `quench-image` | optional (`typeset`) | `encode_png` |
| `quench-test` | dev | the custom test harness |
| `quench-svg` | dev | the SVG tests render the output back |
| `quench-text` (`math-font`) | dev | font data in the tests |

No third-party crates.

## What it covers

* **Reading** ([supported TeX](supported.md)): the math of plain TeX and amsmath that pages use: Greek and the usual symbols with their TeX classes, large and named operators with their limits, fractions and binomials, radicals, `\left`/`\middle`/`\right` and `\big` sizes, accents, over- and underbraces, `\overset`-style stacks, `\mathbb` and the other alphabets, `\text`, spacing, colour, the `matrix`, `cases`, `aligned`, `gathered` and `array` environments, and macros (`\newcommand`, `\def`, `\DeclareMathOperator`) kept across a page's formulas.
* **Text output**: `to_unicode` (`\frac{a+b}{2} \le \sqrt{x}` is `(a + b)/2 ≤ √x`) and `to_text_block` (rows of terminal cells with a baseline row).
* **Typesetting** (feature `typeset`): a display list of glyphs and rules in font units, drawn antialiased to RGBA or PNG, or written as SVG with one path per glyph, sized in ems, filled with `currentColor`, with the baseline as its `vertical-align`. What follows TeX's rules exactly and what is approximated is listed in [internals](internals.md#appendix-g-what-is-followed-and-what-is-approximated).

## Where to go next

* [Usage](usage.md): inline and display text, errors, page macros, walking the tree, PNG and SVG, HTML, the terminal (kitty and sixel through [quench-cli](../quench-cli/index.md)), other fonts.
* [API reference](api.md): every public item of the crate root and of `typeset`.
* [Supported TeX](supported.md): every command, environment and form the reader accepts, and what it refuses.
* [Internals](internals.md): the reader, the text layout rules, the typesetter's rules and approximations, growing glyphs, drawing, limits.
* [Testing](testing.md): the inline table, the two-dimensional snapshots, geometry assertions, PNG hash snapshots, fuzzing.
* Related: [quench-raster](../quench-raster/index.md) (drawing), [quench-image](../quench-image/index.md) (PNG), [quench-svg](../quench-svg/index.md) (renders the SVG back), [quench-html](../quench-html/index.md) (for pages that embed the SVG).

## Minimal example

```rust
use quench_tex::{parse, to_text_block, to_unicode};

let formula = parse(r"\sum_{i=1}^n i^2 \le \frac{n^3}{2} \quad \forall n \in \mathbb{N}")?;
assert_eq!(to_unicode(&formula), "∑ᵢ₌₁ⁿ i² ≤ n³/2  ∀n ∈ ℕ");

let block = to_text_block(&parse(r"x = \frac{-b \pm \sqrt{b^2 - 4ac}}{2a}")?);
assert_eq!(block.lines, [
    "          ________",
    "    −b ± √b² − 4ac",
    "x = ──────────────",
    "          2a",
]);
# Ok::<(), quench_tex::Error>(())
```

(This is the crate's doctest.) With `typeset` (the module's doctest):

```rust
use quench_tex::typeset::{RenderOptions, SvgOptions, tex_to_png, tex_to_svg, typeset};

let formula = quench_tex::parse(r"\frac{a}{b}")?;
let set = typeset(&formula, true)?;
assert!(set.ascent > 278.0 && set.descent > 0.0);

let png = tex_to_png(r"\sqrt{x^2+1}", false, &RenderOptions::default())?;
assert_eq!(&png[1..4], b"PNG");
let svg = tex_to_svg(r"e^{i\pi}", false, &SvgOptions::default())?;
assert!(svg.starts_with("<svg"));
# Ok::<(), quench_tex::typeset::Error>(())
```

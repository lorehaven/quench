---
title: Testing
parent: quench-tex
grand_parent: Crates
nav_order: 5
---

Two test targets, both with the custom harness (`harness = false`, `quench_test::main!()`, `#[quench_test::test]`), plus the doctests of `src/lib.rs` and `src/typeset/mod.rs`:

```bash
cargo test -p quench-tex                      # unit: 74 tests, and the lib doctest
cargo test -p quench-tex --features typeset   # also typeset: 32 tests, and the typeset doctest
```

The `typeset` target has `required-features = ["typeset"]`, so it is skipped without the feature. There are no `#[cfg(test)]` unit tests in `src/`.

## `tests/unit.rs`: the reader and the text output

| File | Tests | What |
|---|---|---|
| `unit/inline_tests.rs` | 2 | The **inline table**: `CASES`, pairs of TeX and the exact `to_unicode` output (characters and classes, primes and scripts, Greek and symbols, large and named operators, fractions and binomials, radicals, delimiters, accents, fonts, text, spacing, styles and colour, arrays). One test checks every pair and reports all mismatches at once; another requires at least 80 pairs. |
| `unit/block_tests.rs` | 30 | **Two-dimensional snapshots**: for each formula the exact rows of `to_text_block`, its baseline row and its width: the quadratic formula, stacked and nested fractions, raised and inline scripts, sums and products with limits, integrals with limits beside, `\lim`, `pmatrix`, `bmatrix`, `vmatrix`, `Bmatrix`, `Vmatrix`, `cases` (two rows and grown), radicals and a cube root of a fraction, `aligned`, `gathered`, a superscript on a tall delimiter, `\middle`, over- and underbraces with labels, binomials, an `array` with rules, wide accents. |
| `unit/tree_tests.rs` | 18 | The syntax tree: symbol and typed-character classes, operator limits and `\limits`/`\nolimits`, scripts in either order, primes, fractions' bar/delimiters/style, radical degrees, `\left`/`\middle`/`\right` delimiters, accents, braces and stacks, fonts, text, styles and colours, spacing units, arrays (columns, rows, `\hline`s, delimiters), `math_alphanumeric` holes, `display_width`. |
| `unit/macro_tests.rs` | 10 | `\newcommand` with and without arguments, optional arguments, `\renewcommand` and `\providecommand` (and a macro shadowing `\frac`), `\def` (with parameters, delimited refused), `\DeclareMathOperator` and its star, definitions carried by `parse_with` and untouched by a failing formula, `Macros::define`, recursive macros as errors, bad definitions. |
| `unit/error_tests.rs` | 6 | Unknown commands and environments named with exact offsets (`\foo` at 4, `\qux` at 9), unbalanced braces with offsets, misplaced structure (`\right`, `&`, `\end`, `\limits`, `\hline`), missing arguments and double scripts (`x^2^3` at 3), other syntax errors (lone `\`, `#1`, `$`, bad lengths and columns, nesting 500 deep), comments and spaces ignored. |
| `unit/regression_tests.rs` | 5 | Hex colours read verbatim whatever digit they start with (`#1a2b3c`, `#123`, with spaces, with a model, through a macro's `#1`), non-characters still refused; `\limits`/`\nolimits`/`\displaylimits` on `\mathop{…}` and `\operatorname`, refused on other classes; the block layout of `\mathop` limits; `Macros::define` rejecting more than 9 parameters; every accent's spacing form one cell wide and not combining (`\dddot` over a wide base). |
| `unit/fuzz_tests.rs` | 3 | **Fuzz**: 4000 random formulas of 1 to 14 pieces from a list of tokens and commands (a deterministic xorshift generator), parsed with one shared `Macros`; each one that parses is written with both outputs, the block's baseline must be inside it and no row wider than `width`; more than 200 must parse. Every prefix and suffix of five long formulas. Nesting within the limit (70 groups, 40 radicals, 30 fractions, 40 superscripts, 40 `\left(`) must render. |

## `tests/typeset.rs`: the typesetter (feature `typeset`)

| File | Tests | What |
|---|---|---|
| `typeset/geometry_tests.rs` | 26 | **Geometry assertions** against the font's constants, within half a font unit: the fraction rule on the axis with `fraction_rule_thickness`, gaps around a tall numerator, `\binom` without rule and with the stack gap, superscript shifts (plain and cramped), scripts at 60 % and 50 %, the sub/superscript gap, the display `∑` variant with limits above and below, text-style `∑` with limits beside, integral scripts offset by the italic correction, `\left`/`\right` covering their content around the axis, `\middle`, the radical sign covering radicand and gap, the radical degree, spacing by atom classes, accents on the top accent attachment, matrix column alignment, growing `\big` sizes, braces with labels, over- and underline constants, math italic and the alphabets, `\text` in the text font, box metrics holding the ink, `MissingGlyph` (`\text{漢}`, message with `U+6F22`), `UnknownColour`, `Parse`, colours carried to the items, hex colours starting with a digit, `\mathop` with `\limits`/`\nolimits` (and `\operatorname*`) in both styles. |
| `typeset/snapshot_tests.rs` | 5 | **PNG hash snapshots** of 16 reference formulas (`FORMULAS`: quadratic, sum, integral, inline, pmatrix, cases, radicals, accents, greek, blackboard, fenced, limits, brace, aligned, bigs, colour) drawn at 40 px per em black on white, compared by an FNV-1a hash of the RGBA bytes with the table `HASHES`; a **torture set** (`TORTURE`, 16 more formulas: nested fractions, tall radicals, assemblies, primes, operators, integrals, arrows, stacks, matrices, arrays, text, dots, styles, gathered, script-heavy, wide accents) that must typeset with the ink inside the box; SVG well-formed with one `<path>` per drawn glyph and one `<rect>` per rule, rendered back with quench-svg; the PNG transparent by default, straight alpha, the baseline row, and decoded back with quench-image; 26 empty or degenerate formulas (`""`, `\left.\right.`, `\sqrt{}`, `\frac{}{}`, `\color{red}`, `''`, ...) in both styles. |
| `typeset/fuzz_tests.rs` | 1 | 3000 random formulas; each that parses must typeset without error, with finite metrics and the ink inside the box; every tenth is drawn to RGBA and SVG. More than 200 must parse. |

### Updating the snapshots

A deliberate change to the layout or drawing changes the hashes. Run:

```bash
QUENCH_TEX_SNAPSHOTS=1 cargo test -p quench-tex --features typeset --test typeset
```

With `QUENCH_TEX_SNAPSHOTS` set (to anything), `reference_formulas_render` writes `<name>.png` and `<name>.svg` for each reference formula, and `torture_set_typesets` writes `<name>.png` for the torture set, to `target/quench-tex-snapshots/` at the workspace root (the path is `CARGO_MANIFEST_DIR/../target/quench-tex-snapshots`), and the new `("name", 0x…)` lines are printed to standard output. Look at the images, then paste the lines into `HASHES` in `tests/typeset/snapshot_tests.rs`. The hashes depend on `quench-raster`'s rasteriser and on the embedded font, so a change in either changes them too.

## Testing code that uses the crate

* For the text output, compare `to_unicode` strings and `TextBlock::lines` exactly, as the tests do; they do not depend on the machine.
* For the typesetter, the bundled fonts make the display list and the pixels deterministic across machines (no system fonts are read). Assert on geometry (`Typeset::ink`, `width`, `ascent`, item positions) rather than on PNG bytes, or hash the RGBA as `snapshot_tests.rs` does.
* `parse` is fuzzed for panics (4000 random formulas, every prefix and suffix of long ones); `typeset` of a parsed formula fails only with `MissingGlyph` or `UnknownColour` with the bundled fonts.

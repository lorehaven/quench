---
title: Testing
parent: quench-markdown-term
grand_parent: Crates
nav_order: 4
---

## How the crate is tested

The renderer is pure, so it is tested with fixed widths and snapshot text. No terminal is involved. Both integration tests use `harness = false` with `quench_test::main!()` (see [quench-test](../quench-test/index.md)). The library has no unit tests of its own. The four doc tests (crate example, `slug`, `display_width`, `replace_baseurl`) use the standard harness.

| Target | File | Tests | Covers |
|---|---|---|---|
| `unit` (`tests/unit.rs`) | `unit/block_tests.rs` | 8 | paragraphs (wrapping, soft and hard breaks, an over-long word), headings, code frames (language, tabs, `↪` continuations), quotes, lists (depth bullets, task boxes, right-aligned numbers, loose items), rule and HTML, nesting (a list with a quote and code in a quote), ASCII mode |
| | `unit/table_tests.rs` | 3 | alignments and a shrinking, wrapping column; natural widths; the stacked form at 20 and 24 cells |
| | `unit/footnote_tests.rs` | 6 | `End` and `Inline` placement; `FootnoteSpot` lines; reference and back-mark links (`#fn-1`, `#fnref-1`); inline definitions dim; a footnote inside a list item; ASCII marks `[^1]` apart from link numbers ` [1]` |
| | `unit/inline_tests.rs` | 10 | styles without markup; link numbering and shared numbers; OSC 8 on and off; relative links not hyperlinked; ANSI colours by role and resets; images; inline HTML; control characters removed; ASCII-only output |
| | `unit/jekyll_tests.rs` | 3 | callouts (`.note`, `.warning` on a quote, `.important-title`); attribute lists, `{:toc}`, `{% include %}`, other Liquid tags, `{{ site.baseurl }}`, `{% raw %}` in code; `{{ site.baseurl }}` kept inside `{% raw %}` in text, code spans and code blocks |
| | `unit/math_tests.rs` | 5 | a test `MathText` (`Known`): translated inline and display formulas, unknown ones as source; `FormulaSpot` positions; `MathMode::Source`; `SourceMath`; a display translation wider than the line falls back |
| | `unit/width_tests.rs` | 12 | `display_width`/`char_width` (ASCII, combining marks, CJK, Hangul, emoji, fullwidth); emoji sequences (U+FE0F, ZWJ, skin tones, flags) and their wrapping within the width; wrapping wide text by cells; `slug`, `Slugger`; `HeadingSpot` and `heading_line` |
| | `unit/page_tests.rs` | 3 | whole pages: the forge book at 40/80/120 cells; 600 generated pages; deep nesting |
| `tex` (`tests/tex.rs`, `required-features = ["tex"]`) | | 4 | `TexMath`: inline Unicode (`πr²`, `α ≤ β`), a two-dimensional fraction, `\boxed{x}` and a syntax error shown as source, macros carried over |

Most snapshot tests render the same page at **40, 80 and 120 cells**. The helper `snapshot` compares `to_plain` with the expected text, and `assert_fits` checks that no line exceeds the width in `display_width` cells. Both helpers are in `tests/unit.rs`.

`page_tests.rs`:

* `the_forge_book_fits_its_widths` renders every `.md` file under `../../forge/docs`, relative to the crate directory, which is the Forge book when the `forge` repository is checked out beside `quench`. It renders each file at 40, 80 and 120 cells in three option sets:
  * the defaults;
  * ASCII with inline footnotes;
  * no link numbers, `MathMode::Source` and `baseurl: "/forge"`.

  It asserts that every line fits. **When the directory is missing, the test finds no files and passes.**
* `generated_pages_fit_and_do_not_panic` builds 600 pages from random fragments with a fixed xorshift seed, so every run is the same. The fragments include Markdown syntax, CJK, combining marks, emoji and emoji sequences (U+FE0F, ZWJ, skin tone, flag, a lone U+FE0F and U+200D), Liquid, IALs, `\u{1b}[1m` and `\r\n`. It renders each page at a random width from 1 to 60, in the same three option sets, and checks that the lines fit `max(width, 4)`.
* `deep_nesting_fits` renders 40 nested quotes and 30 nested list levels at 4, 10 and 40 cells.

## Running

```bash
cargo test -p quench-markdown-term                 # unit tests and doc tests
cargo test -p quench-markdown-term --features tex  # plus tests/tex.rs
```

## Testing code that uses it

* Fix the width in `Options` and compare `to_plain(&rendered)` with expected text. The output is deterministic and contains no escape codes.
* To check styles, look at `rendered.lines[i].spans` (`Span::style.role`, `.bold`, `.link`) rather than the ANSI string.
* For your own `MathText`, use a small hook that knows a few formulas, as `Known` does in `tests/unit/math_tests.rs`. Assert on `rendered.formulas`.
* To check that output fits, copy `assert_fits`: iterate over `rendered.lines` and compare `display_width(&line.text())` with the width.

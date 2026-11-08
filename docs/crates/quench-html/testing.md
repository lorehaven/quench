---
title: Testing
parent: quench-html
nav_order: 5
---

Both test files use the `quench-test` harness (`harness = false`, `quench_test::main!()`), and each needs its feature (`required-features`).

* `tests/html.rs` (`required-features = ["parse"]`): tree skeleton, implicit closing, tables, attributes, entities, raw-text elements, comments and doctypes, every selector form, navigation and serialisation.
* `tests/markdown.rs` (`required-features = ["markdown"]`):

| Test | What it checks |
|---|---|
| `passes_the_commonmark_spec` | every example of `tests/fixtures/commonmark-spec.json` through `to_html`; asserts there are exactly **652** and that all pass (CommonMark 0.31) |
| `passes_the_gfm_extension_examples` | every example of `tests/fixtures/gfm-extensions.json` through `to_html_with(_, Options::GFM)`; asserts there are exactly **24** and that all pass |
| `inline_math_follows_githubs_dollar_rules` | `$x^2$`, prices (`$5 and $10`), `$ x $`, `$x$5`, `$HOME`, `\$`, `\$` inside TeX, HTML escaping of TeX, non-ASCII, inline `$$a+b$$`, code spans keep dollars, dollars are text without `math` |
| `display_math_comes_from_dollar_blocks_paragraphs_and_math_fences` | the three display forms giving the same `<div class="math display">`, line breaks kept, display math in a list item and a block quote, a `math` fence stays code without `math` |
| `footnotes_are_numbered_by_first_reference_and_unused_ones_dropped` | numbers and reference counts by first reference (including a footnote referenced only from another footnote, and a label matched case-insensitively), the `occurrence` of each reference, unused definitions dropped, an undefined reference stays text |
| `tables_work_inside_containers_and_give_their_cells` | a table in a list item, alignments, `\|` in a code span, short rows padded, `---` without a pipe stays a setext heading, earlier paragraph lines stay a paragraph |
| `task_items_in_a_loose_list_keep_their_paragraph` | checkbox inside `<p>` in a loose list, `[X]` is checked, `[y]` is not a task, `List::marker` |
| `commonmark_options_leave_the_extensions_as_text` | `to_html` writes no `<table>`, `<del>`, `<input` or footnotes, and reads `[^e]: f` as a link reference definition |
| `no_input_makes_it_panic` | 3 000 pseudo-random strings of 12 fragments (`$`, `$$`, `\|`, `[^a]:`, `~~`, `[x]`, `` ```math ``, non-ASCII, ...) and every prefix of each, through `to_html_with(_, Options::ALL)` |
| `nesting_thousands_deep_does_not_exhaust_a_small_stack` | 10 000 nested `>`, 5 000 nested `- `, 20 000 nested `*`, `_`, `~`, `[` and `![`, 5 000 footnote markers, on a thread with a 2 MiB stack, each in under 2 s; mixed delimiters still match after an unmatched closer; 20 nested quotes (within the limit) still nest |

The fixtures are JSON arrays of `{"example", "markdown", "html", "section"}` read by a small string scanner in the test (no JSON library). `gfm-extensions.json` holds examples from cmark-gfm's `test/spec.txt` (tables, strikethrough) and `test/extensions.txt` (tables, strikethrough, footnotes, task lists); the examples that need GFM extended autolinks are left out, since that extension is not implemented. Math has no external fixture: its rules are the tests above.

* Doctests in `lib.rs` and `markdown/mod.rs` (the latter covers `to_html`, `to_html_with` and `parse` with `Options::GFM`).
* Run: `cargo test -p quench-html`; one feature alone: `cargo test -p quench-html --no-default-features --features parse` (or `--features markdown`).
* Testing code built on the crate: compare `to_html_with` output with expected strings (the output is deterministic), or match on the tree from `parse`, which derives `PartialEq` and `Debug` (`assert_eq!(doc.blocks[0], Block::Paragraph(vec![Inline::Text("x".into())]))`).
* *Unverified here:* the old page's comparisons with `pulldown-cmark` on 200 000 random inputs and with `scraper` on 400 HTML files are not part of the repository's tests.

---
title: quench-markdown-term
parent: Crates
nav_order: 32
has_children: true
permalink: /crates/quench-markdown-term/
---

`quench-markdown-term` (library `quench_markdown_term`) lays Markdown out for a terminal. It is a second renderer beside `quench-html`'s HTML one. Input is the [quench-html](../quench-html/index.md) Markdown syntax tree: CommonMark with GFM tables, strikethrough, task lists, footnotes and math. Output is styled lines that fit a given width in terminal cells, plus writers for ANSI and plain text.

* Paragraphs wrap at spaces, and headings are marked by level.
* Code blocks sit in a frame. Long code lines wrap with a continuation mark and are never reflowed.
* Quotes and lists are indented and nested.
* Tables are sized to their content. When they do not fit, they shrink, and when even that fails, rows are stacked as `header: value` records.
* Footnotes go at the end or after their paragraph.
* Formulas go through a hook, or are shown as their TeX source.
* The common parts of Jekyll / just-the-docs pages are handled as the site shows them: callouts, `{:toc}`, `{% include %}`, `{{ site.baseurl }}`, `{#id}`, `{% raw %}`.

The crate is pure: no I/O, no terminal queries, no environment variables. The output keeps semantic roles (heading, link, code, quote, callout ...) instead of colours. A `Theme` turns them into colours only when `to_ansi` writes the lines. Next to the lines, the renderer records where headings (with GitHub-style slugs), links, footnotes and formulas ended up. A pager can use these spots to follow `#anchor` links, jump to a footnote, or draw a typeset formula over its text (for example with [quench-cli's graphics module](../quench-cli/api-graphics.md)). `loupe` uses the crate to show Markdown and the documentation books in a terminal.

It has no third-party dependencies: only `quench-html` and, optionally, `quench-tex`.

## Facts

| | |
|---|---|
| Version | 0.1.0 |
| Edition | 2024 (workspace); no `rust-version` set |
| License | MIT (workspace) |
| Lints | `[lints] workspace = true`; no `unsafe` |
| Test harness | integration tests `unit` and `tex` with `harness = false` and `quench_test::main!()`; the doc tests use the standard harness |

```toml
[dependencies]
quench-markdown-term = { version = "0.1", registry = "ennor" }
# formulas as Unicode / two-dimensional text through quench-tex:
# quench-markdown-term = { version = "0.1", registry = "ennor", features = ["tex"] }
# in the workspace: quench-markdown-term = { workspace = true }
```

## Features

| Feature | Default | Enables |
|---|---|---|
| `tex` | no | `TexMath`, a `MathText` built on [quench-tex](../quench-tex/index.md) (`dep:quench-tex`) |

Without `tex`, formulas show as their source unless you supply your own `MathText`.

## Dependencies

| Crate | Why |
|---|---|
| `quench-html` (`default-features = false`, `features = ["markdown"]`) | the Markdown parser and syntax tree (`quench_html::markdown::{parse, Options, Document, Block, Inline, ...}`); no HTML DOM |
| `quench-tex` (optional, feature `tex`) | `parse_with`, `Macros`, `to_unicode` and `to_text_block` for `TexMath`; the crate's default features only (no typesetter) |
| `quench-test` (dev) | test harness |

## Where to go next

* [Usage](usage.md): printing a page, pagers, Jekyll pages, formulas, ANSI output.
* [API reference](api.md): every public item.
* [Internals](internals.md): how each block is laid out, tables, wrapping, width.
* [Testing](testing.md)
* The syntax tree and `markdown::parse`: [quench-html](../quench-html/index.md) ([API](../quench-html/api.md)).
* Formulas as text, and the typesetter: [quench-tex](../quench-tex/index.md).

## Minimal example

From the crate documentation (a doc test):

```rust
use quench_html::markdown::{self, Options as Syntax};
use quench_markdown_term::{Options, render, to_plain};

let document = markdown::parse("# Title\n\nSome *text*, and a list:\n\n- one\n- two\n", Syntax::ALL);
let rendered = render(&document, &Options { width: 20, ..Options::default() });
assert_eq!(
    to_plain(&rendered),
    "Title\n════════════════════\n\nSome text, and a\nlist:\n\n• one\n• two\n",
);
assert_eq!(rendered.headings[0].slug, "title");
```

`render_markdown(text, &options)` combines the parse and the layout, with every extension switched on.

## Limitations

* **Front matter is not stripped.** That is the caller's job. A page that starts with `---` / `title: x` / `---` renders as a rule, the YAML lines run together as a paragraph, and a second rule: the closing `---` turns the paragraph into a setext level-2 heading.
* **Tables stack** as `header: value` lines when the longest words of the columns do not fit side by side. Columns never shrink below their longest word.
* **Code wraps by characters**, not at word boundaries, with a `↪ ` mark on the continuation lines.
* **The width table is approximate.** It covers the scripts and emoji ranges text uses, not the whole Unicode database. Emoji sequences count as terminals with emoji support draw them: `❤️` (U+FE0F) and `👨‍👩‍👧` (ZWJ) are 2 cells, a flag is 2. A terminal that draws a ZWJ sequence as separate emoji shows it wider. East Asian ambiguous-width characters count 1.
* **Narrow layouts drop decoration.** When a frame, quote bar, callout bar or list indent would leave the content fewer than 4 cells, the decoration is dropped. A code block below 8 cells loses its frame, a quote or callout below 6 cells loses its bar, and a list marker moves to a line of its own. No line ever exceeds the width (minimum 4).

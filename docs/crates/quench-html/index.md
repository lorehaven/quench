---
title: quench-html
parent: Crates
nav_order: 23
has_children: true
permalink: /crates/quench-html/
---

`quench-html` (crate `quench_html`) is HTML without dependencies: a tolerant parser that builds a document tree, a CSS selector engine to query it, an HTML character-reference decoder, and a Markdown parser with an HTML renderer. The Markdown side reads CommonMark 0.31 (its 652 spec examples pass) and, as options, the GFM extensions (tables, strikethrough, task lists, footnotes, written as cmark-gfm writes them) and math (`$…$`, `$$…$$`, ```` ```math ````). `markdown::parse` gives the document as a public syntax tree for other renderers. It replaces `scraper` (with `html5ever`, `selectors`, `cssparser`) and `pulldown-cmark`.

* Version: `0.1.3`. Edition 2024. `[lints] workspace = true` (`unsafe_code = "deny"`; none present). **No dependencies** (not even `quench-core`).
* The HTML parser never fails and does not implement the whole HTML5 algorithm (no adoption agency, no foster parenting).
* Markdown not read: smart punctuation, GFM's extended autolinks (`www.example.com` without `<>`), GFM's tag filter, definition lists, heading ids, front matter.
* **There is no HTML sanitizer in this crate.** The parser keeps scripts, event handler attributes and `javascript:` URLs as it finds them; the Markdown renderer passes raw HTML through unchanged, whatever the options. If the input is untrusted, sanitise the output yourself. See [Security notes](internals.md#security-notes).

## Install

```toml
[dependencies]
quench-html = { version = "0.1", registry = "ennor" }                       # parse + markdown
# quench-html = { version = "0.1", registry = "ennor", default-features = false, features = ["markdown"] }
```

In the workspace: `quench-html = { workspace = true }`.

## Feature flags

| Feature | Default | Enables |
|---|---|---|
| `parse` | yes | `dom` (`Document`, `Node`, `NodeKind`), `selector` (`Selector`, `SelectorError`) |
| `markdown` | yes | `markdown`: `to_html`, `to_html_with`, `parse`, `render_html`, and the syntax tree in `markdown::ast` (`Options`, `Document`, `Block`, `Inline`, ..., re-exported from `markdown`) |

Always available: `entities` and `decode_entities`. The two features are independent: `markdown` alone gives the Markdown parser and renderer without the HTML parser (this is how [quench-markdown-term](../quench-markdown-term/index.md) depends on it).

## Dependencies

None. Dev: `quench-test` (test harness).

## Where to go next

* [Usage](usage.md): HTML extraction, CommonMark, GFM tables and task lists, footnotes, math, the syntax tree.
* [API reference](api.md)
* [Parser behaviour](internals.md): tree building rules, selector grammar, entity rules, how Markdown and its extensions are parsed, security notes.
* [Errors](errors.md)
* [Testing](testing.md)
* The terminal renderer in [quench-markdown-term](../quench-markdown-term/index.md) lays out the tree from `markdown::parse`; TeX formulas can be typeset with [quench-tex](../quench-tex/index.md).

## Minimal example

```rust
use quench_html::{Document, Selector, markdown};
use quench_html::markdown::{Block, Options};

let html = markdown::to_html("# Title\n\nSome *emphasis*.\n");
let document = Document::parse(&html);
let em = Selector::parse("p em").unwrap();
assert_eq!(document.select(&em).next().unwrap().text(), "emphasis");

assert_eq!(
    markdown::to_html_with("~~gone~~ $x^2$\n", Options::ALL),
    "<p><del>gone</del> <span class=\"math inline\">\\(x^2\\)</span></p>\n"
);
let tree = markdown::parse("| a |\n| - |\n| 1 |\n", Options::GFM);
assert!(matches!(tree.blocks[0], Block::Table(_)));
```

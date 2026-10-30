---
title: quench-html
parent: Crates
nav_order: 23
has_children: true
permalink: /crates/quench-html/
---

`quench-html` (crate `quench_html`) is HTML without dependencies: a tolerant parser that builds a document tree, a CSS selector engine to query it, an HTML character-reference decoder, and a CommonMark to HTML renderer. It replaces `scraper` (with `html5ever`, `selectors`, `cssparser`) and `pulldown-cmark`.

* Version: `0.1.2`. Edition 2024. `unsafe_code = "deny"`; none present. **No dependencies** (not even `quench-core`).
* The parser never fails and does not implement the whole HTML5 algorithm (no adoption agency, no foster parenting).
* **There is no HTML sanitizer in this crate.** The parser keeps scripts, event handler attributes and `javascript:` URLs as it finds them; the Markdown renderer passes raw HTML through unchanged. If the input is untrusted, sanitise the output yourself. See [Security notes](internals.md#security-notes).

## Install

```toml
[dependencies]
quench-html = { version = "0.1", registry = "ennor" }                       # parse + markdown
# quench-html = { version = "0.1", registry = "ennor", default-features = false, features = ["markdown"] }
```

## Feature flags

| Feature | Default | Enables |
|---|---|---|
| `parse` | yes | `dom` (`Document`, `Node`, `NodeKind`), `selector` (`Selector`, `SelectorError`) |
| `markdown` | yes | `markdown::to_html` |

Always available: `entities` and `decode_entities`.

## Dependencies

None. Dev: `quench-test`.

## Where to go next

* [Usage](usage.md)
* [API reference](api.md)
* [Parser behaviour](internals.md): tree building rules, selector grammar, entity rules, Markdown scope, security notes.
* [Errors](errors.md)
* [Testing](testing.md)

## Minimal example

```rust
use quench_html::{Document, Selector, markdown};

let html = markdown::to_html("# Title\n\nSome *emphasis*.\n");
let document = Document::parse(&html);
let em = Selector::parse("p em").unwrap();
assert_eq!(document.select(&em).next().unwrap().text(), "emphasis");
```

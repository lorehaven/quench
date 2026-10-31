# quench-html
HTML without dependencies: a tolerant parser with CSS selectors for extraction, and a CommonMark to HTML renderer.

## What it is
`quench-html` is a tolerant HTML parser that builds a document tree, a CSS selector engine to query it, an HTML character-reference decoder, and a CommonMark to HTML renderer. It replaces `scraper` (with `html5ever`, `selectors`, `cssparser`) and `pulldown-cmark`, and has no dependencies, not even `quench-core`.

The parser never fails and does not implement the whole HTML5 algorithm. There is no sanitizer: scripts, event handler attributes and `javascript:` URLs are kept, and the Markdown renderer passes raw HTML through, so sanitise output made from untrusted input yourself.

## Use cases
- Scrape links and text from a page with CSS selectors.
- Walk and re-serialise a parsed HTML tree.
- Render Markdown documents to HTML.
- Decode HTML character references.

## Example
```rust
use quench_html::{Document, Selector, markdown};

let html = markdown::to_html("# Title\n\nSome *emphasis*.\n");
let document = Document::parse(&html);
let em = Selector::parse("p em").unwrap();
assert_eq!(document.select(&em).next().unwrap().text(), "emphasis");

assert_eq!(quench_html::decode_entities("&lt;b&gt; &amp; &#65;"), "<b> & A");
```

## Features
- `parse` (default): `Document`, `Node`, `NodeKind`, `Selector`, `SelectorError`.
- `markdown` (default): `markdown::to_html`.

## Documentation
Full documentation: https://lorehaven.github.io/quench/crates/quench-html/

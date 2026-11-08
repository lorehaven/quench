# quench-html
HTML without dependencies: a tolerant parser with CSS selectors for extraction, and a Markdown (CommonMark, GFM, math) parser and HTML renderer.

## What it is
`quench-html` is a tolerant HTML parser that builds a document tree, a CSS selector engine to query it, an HTML character-reference decoder, and a Markdown parser and HTML renderer: `CommonMark` 0.31 (its whole test suite passes), plus as options the GFM extensions (tables, strikethrough, task lists, footnotes, written as cmark-gfm writes them and tested with its examples) and math (`$…$`, `$$…$$`, ```` ```math ````). `markdown::parse` gives the document as a syntax tree for other renderers, such as a terminal one. It replaces `scraper` (with `html5ever`, `selectors`, `cssparser`) and `pulldown-cmark`, and has no dependencies, not even `quench-core`.

The parser never fails and does not implement the whole HTML5 algorithm. There is no sanitizer: scripts, event handler attributes and `javascript:` URLs are kept, and the Markdown renderer passes raw HTML through, so sanitise output made from untrusted input yourself.

## Use cases
- Scrape links and text from a page with CSS selectors.
- Walk and re-serialise a parsed HTML tree.
- Render Markdown documents to HTML, with GitHub's extensions and math.
- Parse Markdown into a syntax tree for another renderer.
- Decode HTML character references.

## Example
```rust
use quench_html::{Document, Selector, markdown};

let html = markdown::to_html("# Title\n\nSome *emphasis*.\n");
let document = Document::parse(&html);
let em = Selector::parse("p em").unwrap();
assert_eq!(document.select(&em).next().unwrap().text(), "emphasis");

assert_eq!(quench_html::decode_entities("&lt;b&gt; &amp; &#65;"), "<b> & A");

use markdown::{Block, Options};
assert_eq!(markdown::to_html_with("~~gone~~ $x^2$\n", Options::ALL),
           "<p><del>gone</del> <span class=\"math inline\">\\(x^2\\)</span></p>\n");
let doc = markdown::parse("| a |\n| - |\n| 1 |\n", Options::GFM);
assert!(matches!(doc.blocks[0], Block::Table(_)));
```

## Features
- `parse` (default): `Document`, `Node`, `NodeKind`, `Selector`, `SelectorError`.
- `markdown` (default): `markdown::to_html`, `to_html_with`, `parse`, `render_html`, `Options`, and the syntax tree in `markdown::ast`.

## Documentation
Full documentation: https://lorehaven.github.io/quench/crates/quench-html/

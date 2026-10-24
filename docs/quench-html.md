# Quench HTML

`quench-html` (crate `quench_html`) is HTML without dependencies: a tolerant parser with CSS selectors for pulling text and attributes out of pages, and a CommonMark to HTML renderer. It replaces `scraper` (with `html5ever`, `selectors` and `cssparser`) and `pulldown-cmark`.

| Feature | Module | |
| --- | --- | --- |
| `parse` | `dom`, `selector` | `Document::parse`, `Selector::parse`, `Node` navigation |
| `markdown` | `markdown` | `markdown::to_html` |

## Parsing and selecting

```rust
use quench_html::{Document, Selector};

let document = Document::parse(&page);
let links = Selector::parse("main a[href^='http'], article a")?;
for a in document.select(&links) {
    println!("{} -> {}", a.text(), a.attr("href").unwrap_or_default());
}
```

- `Document::parse` never fails. It builds `html`, `head` and `body`, puts `title`, `meta`, `link`, `style` and `script` in the head and everything else in the body, closes `p`, `li`, `dt`/`dd`, `td`/`th`/`tr`, `option`, headings, `a` and `button` implicitly, adds the implied `tbody` and `tr`, ignores stray end tags, treats `script`, `style`, `iframe`, `noscript` and the like as raw text and `title` and `textarea` as text, drops the newline right after `<pre>`, and decodes character references (all 2,125 HTML named ones; the legacy ones also without `;`; attribute values follow the attribute rules).
- `Node` (a copyable handle): `kind`, `name`, `attr`, `attrs`, `id`, `classes`, `has_class`, `text`, `text_nodes`, `as_text`, `parent`, `children`, `ancestors`, `descendants` (itself first, in document order), `prev_element`, `next_element`, `select`, `html`.
- `Selector`: type, `*`, `#id`, `.class`, attribute selectors (`=`, `~=`, `|=`, `^=`, `$=`, `*=`, `i` flag), the combinators ` `, `>`, `+`, `~`, lists, and `:first-child`, `:last-child`, `:only-child`, the `-of-type` forms, `:nth-child(an+b)` (and `-last-child`, `-of-type`, `-last-of-type`), `:not()`, `:is()`, `:where()`, `:empty`, `:root`.
- Not implemented: the adoption agency algorithm (misnested formatting elements such as `<a><p></a>` are not re-opened) and foster parenting (text directly inside a table stays there instead of moving before it).

## Markdown

`markdown::to_html(text)` implements CommonMark 0.31 (no tables, footnotes, strikethrough, task lists or smart punctuation). Raw HTML in the input passes through unchanged, so sanitise the output if the Markdown is untrusted.

## Verification

- All 652 examples of the CommonMark 0.31 specification pass (they are in `tests/fixtures/commonmark-spec.json`). On 200,000 random inputs built from Markdown fragments the output equals `pulldown-cmark`'s except for its own deviations (no trailing newline at the end of the input, tabs in a few positions, raw HTML blocks that run to the end of the document).
- The parser was compared with `scraper` on 400 HTML files found on a Linux system: the text of 399 is identical, and 8,000 selector queries (type, class, attribute, combinator and pseudo-class forms) over those files differ in 5, all in documents with misnested formatting elements or text inside tables.

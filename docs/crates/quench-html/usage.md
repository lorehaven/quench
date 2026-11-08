---
title: Usage
parent: quench-html
nav_order: 1
---

## 1. Extract links and text from a page

```rust
use quench_html::{Document, Selector};

let page = r#"<main><a href="http://a.example/x">A</a><a href="/local">B</a></main>
              <article><a href="http://b.example/">C</a></article>"#;
let document = Document::parse(page);
let links = Selector::parse("main a[href^='http'], article a").unwrap();
let found: Vec<(String, &str)> = document
    .select(&links)
    .map(|a| (a.text(), a.attr("href").unwrap_or_default()))
    .collect();
assert_eq!(found, [("A".to_string(), "http://a.example/x"), ("C".to_string(), "http://b.example/")]);
```

Elements come back in document order, once each, even when several alternatives of the list match.

Pitfall: a selector outside the supported grammar is an error, not a silent mismatch (see [Errors](errors.md)):

```rust
use quench_html::Selector;
let err = Selector::parse("a:hover").unwrap_err();
assert_eq!(err.to_string(), "invalid selector: unsupported pseudo-class :hover");
```

## 2. The tree is always there

```rust
use quench_html::Document;

let doc = Document::parse("<p>one<p>two");
let html = doc.root_element();
assert_eq!(html.name(), Some("html"));
let kids: Vec<_> = html.children().filter_map(|c| c.name()).collect();
assert_eq!(kids, ["head", "body"]);
assert_eq!(doc.root_element().html(), "<html><head></head><body><p>one</p><p>two</p></body></html>");
```

`Document::parse` has no error path; garbage in gives a tree out.

## 3. Walk the tree

```rust
use quench_html::{Document, Selector};

let doc = Document::parse("<ul><li>a<li class='x'>b</ul>");
let li = Selector::parse("li.x").unwrap();
let second = doc.select(&li).next().unwrap();
assert_eq!(second.parent().and_then(|p| p.name()), Some("ul"));
assert_eq!(second.prev_element().map(|n| n.text()), Some("a".to_string()));
assert!(second.has_class("x"));
assert_eq!(second.ancestors().filter_map(|n| n.name()).collect::<Vec<_>>(), ["ul", "body", "html"]);
```

## 4. Serialise a subtree

```rust
use quench_html::Document;
let doc = Document::parse(r#"<p title='a"b'>1 < 2 &amp; 3</p>"#);
let p = doc.root_element().descendants().find(|n| n.name() == Some("p")).unwrap();
assert_eq!(p.html(), r#"<p title="a&quot;b">1 &lt; 2 &amp; 3</p>"#);
assert_eq!(p.inner_html(), "1 &lt; 2 &amp; 3");
```

Text and attribute values are re-escaped (`& < >`, and `"` in attributes); void elements have no end tag; the contents of `script` and `style` are written as they are.

## 5. Markdown to HTML

```rust
use quench_html::markdown::to_html;

assert_eq!(to_html("Hello, *world*!\n"), "<p>Hello, <em>world</em>!</p>\n");
assert_eq!(to_html("- a\n- b\n"), "<ul>\n<li>a</li>\n<li>b</li>\n</ul>\n");
```

`to_html` is plain CommonMark 0.31. Pitfall: `to_html("[x](javascript:alert(1))")` yields `<a href="javascript:alert(1)">x</a>` and raw `<script>` blocks pass through, with any options. Do not show the result of untrusted Markdown to other users without a sanitizer. Tables (`| a | b |`), `~~strike~~` and `- [ ] task` are text in plain CommonMark; use `to_html_with` (below) for them.

## 6. GitHub Markdown: tables, strikethrough, task lists

```rust
use quench_html::markdown::{Options, to_html_with};

let table = "| Name | Qty |\n| :--- | --: |\n| tea | 2 |\n| cake |\n";
assert_eq!(
    to_html_with(table, Options::GFM),
    "<table>\n<thead>\n<tr>\n<th align=\"left\">Name</th>\n<th align=\"right\">Qty</th>\n</tr>\n</thead>\n\
     <tbody>\n<tr>\n<td align=\"left\">tea</td>\n<td align=\"right\">2</td>\n</tr>\n\
     <tr>\n<td align=\"left\">cake</td>\n<td align=\"right\"></td>\n</tr>\n</tbody>\n</table>\n"
);
assert_eq!(
    to_html_with("~~old~~ ~new~\n\n- [x] done\n- [ ] to do\n", Options::GFM),
    "<p><del>old</del> <del>new</del></p>\n<ul>\n\
     <li><input type=\"checkbox\" checked=\"\" disabled=\"\" /> done</li>\n\
     <li><input type=\"checkbox\" disabled=\"\" /> to do</li>\n</ul>\n"
);
```

`Options::GFM` turns on what GitHub reads: tables, strikethrough, task lists and footnotes. The short row `| cake |` is filled with an empty cell; extra cells would be dropped.

Pitfalls:

* The delimiter row must contain a pipe and have as many cells as the header line: `abc\n---\n` stays a setext heading, `| a | b |\n| - |\n` is a paragraph.
* A table ends at a blank line or at a line that starts another block; any other line, with or without pipes, is a row.
* `\|` is a literal pipe in a cell, also inside a code span (`` `x\|y` `` gives `<code>x|y</code>`).
* `~~~text~~~` is not strikethrough (only runs of one or two tildes, equal on both sides), and `~~~` at the start of a line opens a fenced code block.
* `[ ]`, `[x]` or `[X]` must be followed by a space, tab or the end of the line: `- [ ]x` is an ordinary item.

## 7. Footnotes

```rust
use quench_html::markdown::{Options, to_html_with};

let html = to_html_with("Text[^n] and again[^N].\n\n[^n]: The note.\n", Options::GFM);
assert_eq!(
    html,
    "<p>Text<sup class=\"footnote-ref\"><a href=\"#fn-n\" id=\"fnref-n\" data-footnote-ref>1</a></sup> \
     and again<sup class=\"footnote-ref\"><a href=\"#fn-n\" id=\"fnref-n-2\" data-footnote-ref>1</a></sup>.</p>\n\
     <section class=\"footnotes\" data-footnotes>\n<ol>\n<li id=\"fn-n\">\n\
     <p>The note. <a href=\"#fnref-n\" class=\"footnote-backref\" data-footnote-backref \
     data-footnote-backref-idx=\"1\" aria-label=\"Back to reference 1\">↩</a> \
     <a href=\"#fnref-n-2\" class=\"footnote-backref\" data-footnote-backref \
     data-footnote-backref-idx=\"1-2\" aria-label=\"Back to reference 1-2\">↩<sup class=\"footnote-ref\">2</sup></a></p>\n\
     </li>\n</ol>\n</section>\n"
);
```

Footnotes are numbered 1, 2, ... in the order of their first reference, wherever the definitions are; labels match without regard to case (`[^N]` finds `[^n]:`), and ids use the label as defined. The section goes at the end of the output, one back link per reference.

Pitfalls:

* A reference to an undefined label stays text: `x[^none]` is `<p>x[^none]</p>`.
* Definitions that are never referenced are dropped.
* A definition continues over lines indented by 4 or more columns (further paragraphs, block quotes, code blocks), and lazy continuation lines of its paragraph.
* With plain CommonMark (`to_html`), `[^e]: f` is a link reference definition and `[^e]` a link to `f`.

## 8. Math

```rust
use quench_html::markdown::{Options, to_html_with};

assert_eq!(
    to_html_with("Area $\\pi r^2$, price $5.\n\n$$\nE = mc^2\n$$\n", Options::ALL),
    "<p>Area <span class=\"math inline\">\\(\\pi r^2\\)</span>, price $5.</p>\n\
     <div class=\"math display\">\\[E = mc^2\\]</div>\n"
);
assert_eq!(
    to_html_with("so $$a+b$$ here\n\n$$x$$\n\n```math\ny\n```\n", Options::ALL),
    "<p>so <span class=\"math inline\">\\(a+b\\)</span> here</p>\n\
     <div class=\"math display\">\\[x\\]</div>\n<div class=\"math display\">\\[y\\]</div>\n"
);
```

Math is not in `Options::GFM`: use `Options::ALL` or `Options { math: true, ..Options::GFM }`. The output is meant for the auto-render extension of KaTeX or MathJax (their default delimiters are `\(...\)` and `\[...\]`); load one of them on the page. To typeset without JavaScript, take the TeX from the tree (example 9) and render it with [quench-tex](../quench-tex/index.md).

Pitfalls:

* Dollar rules (GitHub's): the opening `$` must not be followed by a space and the closing `$` must not be preceded by a space nor followed by a digit. So `costs $5 and $10`, `$ x $`, `$x$5` and `echo $HOME` stay text.
* `\$` outside math is a dollar; inside math it stays TeX (`$a\$b$` is the formula `a\$b`).
* A code span wins over math that starts later: `` `$x$` `` is code.
* `$$...$$` is display math only when it is the whole paragraph; next to other text it is inline.
* A `$$` block that is never closed runs to the end of its container (document, list item, block quote).

## 9. The syntax tree: parse, inspect, render

```rust
use quench_html::markdown::{Block, Inline, Options, parse, render_html};

let doc = parse("# Intro to *Rust*\n\nText.\n\n## Next\n", Options::GFM);
let headings: Vec<(u8, String)> = doc
    .blocks
    .iter()
    .filter_map(|b| match b {
        Block::Heading { level, content } => Some((*level, Inline::plain_text(content))),
        _ => None,
    })
    .collect();
assert_eq!(headings, [(1, "Intro to Rust".to_string()), (2, "Next".to_string())]);

// Edit the tree, then write it as HTML.
let mut doc = parse("# Title\n\nBody.\n", Options::COMMONMARK);
doc.blocks.retain(|b| !matches!(b, Block::Heading { .. }));
assert_eq!(render_html(&doc), "<p>Body.</p>\n");
```

`parse` reads everything a renderer needs (inlines, link references resolved, footnotes numbered, task markers and display math recognised), so a renderer only walks the tree; [quench-markdown-term](../quench-markdown-term/index.md) lays it out for a terminal this way. `to_html_with(text, options)` is exactly `render_html(&parse(text, options))`.

Pitfall: after editing, keep `Document::footnotes` consistent with the `Inline::FootnoteReference` items left in the blocks; `render_html` writes the footnotes section from `footnotes` as it is.

## 10. Decode character references

```rust
assert_eq!(quench_html::decode_entities("&lt;b&gt; &amp; &#65; &#x42; &copy"), "<b> & A B ©");
assert_eq!(quench_html::decode_entities("&bogus; &#0;"), "&bogus; \u{FFFD}");
```

The legacy references (`&copy`, `&amp`, `&lt`, ... 106 names) are also accepted without the semicolon, as browsers do in text.

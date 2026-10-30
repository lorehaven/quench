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

Pitfall: `to_html("[x](javascript:alert(1))")` yields `<a href="javascript:alert(1)">x</a>` and raw `<script>` blocks pass through. Do not show the result of untrusted Markdown to other users without a sanitizer. Tables (`| a | b |`) and `~~strike~~` are not Markdown in this renderer and come out as plain paragraph text.

## 6. Decode character references

```rust
assert_eq!(quench_html::decode_entities("&lt;b&gt; &amp; &#65; &#x42; &copy"), "<b> & A B ©");
assert_eq!(quench_html::decode_entities("&bogus; &#0;"), "&bogus; \u{FFFD}");
```

The legacy references (`&copy`, `&amp`, `&lt`, ... 106 names) are also accepted without the semicolon, as browsers do in text.

## 7. Selector errors

```rust
use quench_html::Selector;
let err = Selector::parse("a:hover").unwrap_err();
assert_eq!(err.to_string(), "invalid selector: unsupported pseudo-class :hover");
```

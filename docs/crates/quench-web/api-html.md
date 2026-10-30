---
title: HTML builders
parent: quench-web
grand_parent: Crates
nav_order: 4
---

Reference for `quench_web::html` (source: `src/html/`). Everything here is also reachable at the crate root (`quench_web::Element`, `quench_web::div`, ...) because `lib.rs` has `pub use html::*;`, and through `quench_web::prelude`.

## `html::builder::element`

### `Element`

```rust
#[derive(Clone, Debug)]
pub struct Element { /* private */ }
```

One HTML element: a tag, attributes, an optional text, child elements, and optional `onclick` / `onchange` handlers. There is no public constructor: create elements with [`element`](#element-1) or one of the tag helpers. All methods consume `self` and return `Self` (`#[must_use]`), except `render`.

| Method | Signature | Behaviour |
|---|---|---|
| `attr` | `pub fn attr(self, key: impl ToString, value: impl ToString) -> Self` | Sets attribute `key`; a second call with the same key replaces the value. |
| `class` | `pub fn class(self, class: impl ToString) -> Self` | Appends to the `class` attribute, separated by one space. Calling `.class("a").class("b")` gives `class="a b"`. `.attr("class", ..)` replaces all classes. |
| `text` | `pub fn text(self, text: impl ToString) -> Self` | Sets the element's text (a second call replaces the first). Escaped on render unless `.raw()` was called. |
| `child` | `pub fn child(self, child: Self) -> Self` | Appends a child element. |
| `child_opt` | `pub fn child_opt(self, child: Option<Self>) -> Self` | Appends the child when `Some`, else does nothing. |
| `on_click` | `pub fn on_click(self, js_code: impl ToString) -> Self` | Sets the `onclick` attribute (escaped on render). One handler; a second call replaces it. |
| `on_change` | `pub fn on_change(self, js_code: impl ToString) -> Self` | Sets the `onchange` attribute. |
| `raw` | `pub const fn raw(self) -> Self` | Marks the element's text as raw: it is written without escaping. Only the text is affected; attributes and child elements are unchanged. |
| `defer` | `pub const fn defer(self) -> Self` | Writes the boolean attribute ` defer` after all other attributes. |
| `render` | `pub fn render(&self) -> String` | Serialises the element and its subtree. `#[must_use]`. |

Render rules (from `render`):

1. `<tag`, then every attribute from the `attr` map in **sorted key order** (`BTreeMap`) as ` key="value"`, value escaped.
2. ` onclick="..."` then ` onchange="..."` when set (escaped), after the attribute map; then ` defer` when set.
3. `>`, then the text (escaped, or verbatim when `.raw()`), then the children in insertion order, then `</tag>`.
4. Every element gets a closing tag, void elements included (`<input ...></input>`, `<meta ...></meta>`). The pretty printer in `PageBuilder::build` normalises this; `Element::render` does not.

Escaping (`html_escape`, crate-private) replaces `&` `<` `>` `"` `'` with `&amp;` `&lt;` `&gt;` `&quot;` `&#39;`. Tag names are not validated or escaped.

```rust
use quench_web::prelude::*;

let html = div()
    .class("card")
    .class("wide")
    .attr("id", "a&b")
    .child(span().text("<hi>"))
    .render();
assert_eq!(
    html,
    r#"<div class="card wide" id="a&amp;b"><span>&lt;hi&gt;</span></div>"#
);
```

## `html::builder::elements`

Tag helpers. Each is `#[must_use] pub fn name() -> Element` (no arguments) unless noted.

| Function | Creates |
|---|---|
| `pub fn element(tag: &str) -> Element` | any tag, named by the caller |
| `button()` | `<button>` |
| `div()` | `<div>` |
| `strong()` | `<strong>` |
| `header()` | `<header>` |
| `content()` | `<content>` (not a standard HTML element; the shell uses `div.content` instead) |
| `footer()` | `<footer>` |
| `nav()` | `<nav>` |
| `a()` | `<a>` |
| `h1()`, `h2()`, `h3()` | headings |
| `p()` | `<p>` |
| `pre()` | `<pre>` |
| `span()` | `<span>` |
| `form()` | `<form>` |
| `label()` | `<label>` |
| `ul()`, `li()` | list |
| `i()` | `<i>` (icons) |
| `input()` | `<input>` |
| `textarea()` | `<textarea>` |
| `select()` | `<select>` |
| `option()` | `<option>` |
| `checkbox()` | `<input type="checkbox">` |
| `meta()` | `<meta>` |
| `pub fn style(content: String) -> Element` | `<style>` with `content` as its text (escaped, call `.raw()` to keep CSS punctuation like `>` intact) |
| `pub fn script(content: String) -> Element` | `<script>` with `content` as its text (call `.raw()`; otherwise `&`, `<`, `>`, quotes are escaped and the script breaks) |

That is 28 functions. There are no helpers for `img`, `table`, `ol`, `h4`..`h6`, `br`, `hr`: use `element("img")` etc.

## `html::builder::page`

### `Link`

```rust
#[derive(Clone, Debug, Default)]
pub struct Link {
    pub rel: String,
    pub href: String,
    pub attrs: BTreeMap<String, String>,
}
```

A `<link>` in `<head>`.

| Method | Signature |
|---|---|
| `new` | `#[must_use] pub fn new(rel: &str, href: &str) -> Self` |
| `attr` | `#[must_use] pub fn attr(self, key: &str, value: &str) -> Self` (adds an extra attribute, e.g. `id`, `as`) |

### `Script`

```rust
#[derive(Clone, Debug, Default)]
pub struct Script {
    pub src: String,
    pub content: Option<String>,
    pub crossorigin: Option<String>,
    pub defer: bool,
}
```

A `<script>` in `<head>`: external (`src`) or inline (`content`). Note `Default::default()` has `defer: false`, while `Script::new` sets `defer: true`.

| Method | Signature | Behaviour |
|---|---|---|
| `new` | `#[must_use] pub fn new(src: &str) -> Self` | External script, `defer = true`. |
| `inline` | `pub fn inline(content: impl ToString) -> Self` | Inline script, `defer = false`. Not `#[must_use]`. |
| `defer` | `#[must_use] pub const fn defer(self) -> Self` | Sets `defer = true`. |
| `immediate` | `#[must_use] pub const fn immediate(self) -> Self` | Sets `defer = false`. |
| `crossorigin` | `#[must_use] pub fn crossorigin(self, crossorigin: impl ToString) -> Self` | Sets the `crossorigin` attribute value. |
| `is_inline` | `#[must_use] pub const fn is_inline(&self) -> bool` | `content.is_some()`. |
| `render` | `#[must_use] pub fn render(&self) -> String` | `<script` + ` src="..."` (when `src` is non-empty) + ` defer` + ` crossorigin="..."` + `>` + content + `</script>`. **Nothing is escaped.** |

Conversions: `impl From<String> for Script`, `impl From<&str> for Script`, `impl From<&String> for Script`, all equal to `Script::inline(content)`.

```rust
assert_eq!(Script::new("/app.js").render(), "<script src=\"/app.js\" defer></script>");
assert_eq!(Script::new("/app.js").immediate().crossorigin("anonymous").render(),
           "<script src=\"/app.js\" crossorigin=\"anonymous\"></script>");
assert_eq!(Script::inline("console.log(1)").render(), "<script>console.log(1)</script>");
```

### `js!`

```rust
#[macro_export]
macro_rules! js {
    ($source:literal $(, $($arg:tt)+)?) => { $crate::Script::inline(format!($source $(, $($arg)+)?)) };
    ($source:expr) => { $crate::Script::inline($source) };
}
pub use js;
```

An inline `Script`. With a string literal the source is a `format!` template, so literal braces in JavaScript must be doubled (`{{` `}}`); with any other expression it is passed to `Script::inline` unchanged. Also exported as `quench_web::js` and `quench_web::prelude::js`.

```rust
let name = "world";
let script = js!("console.log('hello {}')", name);
assert_eq!(script.render(), "<script>console.log('hello world')</script>");
```

### `PageBuilder`

```rust
#[derive(Clone, Debug, Default)]
pub struct PageBuilder { /* title, links, head_link_static_attrs, scripts, content */ }
```

Assembles a complete HTML document. All setters are `#[must_use]` and consume `self`.

| Method | Signature | Behaviour |
|---|---|---|
| `new` | `pub fn new() -> Self` | Empty title, no links, no scripts, no content. |
| `title` | `pub fn title(self, value: impl Into<String>) -> Self` | The `<title>`. |
| `links` | `pub fn links(self, value: Vec<Link>) -> Self` | Replaces the link list. |
| `head_link_static_attr` | `pub fn head_link_static_attr(self, key: &str, value: &str) -> Self` | Adds one attribute applied to **every** link. |
| `head_link_static_attrs` | `pub fn head_link_static_attrs(self, value: BTreeMap<String, String>) -> Self` | Merges a map of such attributes (extends, does not replace). |
| `scripts` | `pub fn scripts(self, value: Vec<Script>) -> Self` | Replaces the script list. |
| `content` | `pub fn content(self, value: Element) -> Self` | The `<body>` content. |
| `build` | `pub fn build(self) -> String` | Renders and pretty-prints the document. **Panics** if `content` was never set (`Option::unwrap()` on `None`). |

Link attribute merge order in `build`: the static attributes first, then the link's own `attrs` (a link's value wins), then `rel` and `href` (always win). Attributes are written as `key="value"` in sorted key order, **without escaping**, and each link is written as `<link ...></link>`.

The document skeleton before pretty-printing is:

```html
<!DOCTYPE html>
<html>
    <head>
        <meta charset="utf-8">
        <meta name="viewport" content="width=device-width, initial-scale=1">
        {links, one per line}
        {scripts, one per line}
        <title>{title}</title>
    </head>
    <body>{content}</body>
</html>
```

Links come before scripts, and both come before the title. The title is HTML-escaped (`&`, `<`, `>`). The result is passed through `pretty_print_html`, which writes void elements with an explicit closing tag; `build` puts `<!DOCTYPE html>` and a newline in front of the printed text (the printer itself drops `<!...>` declarations), so the string starts with `<!DOCTYPE html>`. The golden files `tests/fixtures/baseline/pages/*.html` show the exact output.

### `pretty_print_html`

```rust
#[must_use]
pub fn pretty_print_html(html_string: &str) -> String
```

Parses `html_string` with the crate's small HTML reader and writes it back indented by four spaces per level. A fragment is wrapped in `<html><head></head><body>...</body></html>`; the output always ends with a newline. `pre`, `code`, `textarea`, `script` and `style` bodies keep their whitespace. See [Internals](internals.md#the-pretty-printer) for the exact rules.

```rust
assert_eq!(
    pretty_print_html("<div><span>hi</span></div>"),
    "<html>\n    <head>\n    </head>\n    <body>\n        <div>\n            <span>\n                hi\n            </span>\n        </div>\n    </body>\n</html>\n"
);
```

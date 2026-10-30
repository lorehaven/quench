---
title: API reference
parent: quench-html
nav_order: 2
---

Crate root (`lib.rs`):

```rust
pub mod entities;
#[cfg(feature = "markdown")] pub mod markdown;
#[cfg(feature = "parse")] pub mod dom;
#[cfg(feature = "parse")] pub mod selector;
#[cfg(feature = "parse")] pub use dom::{Document, Node, NodeKind};
#[cfg(feature = "parse")] pub use selector::{Selector, SelectorError};

#[must_use] pub fn decode_entities(text: &str) -> String;
```

`decode_entities(text)` = `entities::decode(text, false)`: decodes `&amp;`, `&#65;`, `&#x41;` and named references; legacy names also without `;`; unknown references stay as written.

## `entities`

```rust
pub static ENTITIES: [(&str, &str); 2125];   // HTML named references that end in ';', sorted by name, (name without & and ;, text)
pub static LEGACY: [&str; 106];              // names also recognised without the closing ';'
#[must_use] pub fn named(name: &str) -> Option<&'static str>;
#[must_use] pub fn numeric(code: u32) -> char;
#[must_use] pub fn decode(text: &str, strict: bool) -> String;
#[must_use] pub fn reference(text: &str, strict: bool) -> Option<(String, usize)>;
```

* `named("amp") == Some("&")`. Binary search; `name` is without `&` and `;`.
* `numeric(code)`: `char::from_u32(code)`, with 0, surrogates and values past U+10FFFF mapped to U+FFFD. Windows-1252 remapping of 0x80-0x9F is **not** done (`&#128;` is U+0080).
* `decode(text, strict)`: with `strict = true` (CommonMark) a reference needs its closing `;`; with `false` the 106 legacy names (longest prefix, 2 to 6 characters) are also accepted bare. Numeric references take at most 7 decimal or 6 hex digits, otherwise they stay as text.
* `reference(text, strict)`: `text` must start with `&`. Returns the decoded string and the number of bytes consumed, or `None`.

## `markdown` (feature `markdown`)

```rust
#[must_use] pub fn to_html(text: &str) -> String
```

CommonMark 0.31 to HTML. Output conventions (as in the CommonMark spec examples): block elements end with `\n`; `<img src alt title />`; `<br />`; code blocks `<pre><code class="language-x">`. Not supported: tables, footnotes, strikethrough, task lists, smart punctuation, heading ids, autolinking of bare URLs (CommonMark has none). Raw HTML, `javascript:` URLs and everything else the spec allows pass through. Deterministic; never panics on any `&str` input per the tests (no explicit guarantee is stated).

## `dom` (feature `parse`)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind { Document, Element, Text, Comment, Doctype }

#[derive(Debug)] pub struct Document { /* arena of nodes */ }
#[derive(Debug, Clone, Copy)] pub struct Node<'a> { /* &'a Document, id */ }
impl PartialEq for Node<'_>;   // same document and same node
impl Eq for Node<'_> {}
```

`Document` owns the tree; a `Node<'a>` is a cheap `Copy` handle that borrows it. `Document` is `Send + Sync` (plain data).

### `Document`

```rust
impl Document {
    #[must_use] pub fn parse(html: &str) -> Self;
    #[must_use] pub const fn root(&self) -> Node<'_>;
    #[must_use] pub fn root_element(&self) -> Node<'_>;
    pub fn select<'a, 's>(&'a self, selector: &'s Selector) -> impl Iterator<Item = Node<'a>> + use<'a, 's>;
}
```

* `parse`: never fails; the tree always has `html`, `head` and `body`. See [Parser behaviour](internals.md#tree-building).
* `root`: the document node (kind `Document`), parent of the doctype, leading comments and the `html` element.
* `root_element`: the `html` element (no panic: the parser always creates it).
* `select`: all elements below the root matching `selector`, in document order.

### `Node`

```rust
impl<'a> Node<'a> {
    #[must_use] pub fn kind(&self) -> NodeKind;
    #[must_use] pub fn is_element(&self) -> bool;
    #[must_use] pub fn is_document(&self) -> bool;
    #[must_use] pub fn is_comment(&self) -> bool;
    #[must_use] pub fn name(&self) -> Option<&'a str>;
    #[must_use] pub fn attr(&self, name: &str) -> Option<&'a str>;
    pub fn attrs(self) -> impl Iterator<Item = (&'a str, &'a str)>;
    #[must_use] pub fn id(&self) -> Option<&'a str>;
    pub fn classes(self) -> impl Iterator<Item = &'a str>;
    #[must_use] pub fn has_class(&self, class: &str) -> bool;
    #[must_use] pub fn as_text(&self) -> Option<&'a str>;
    #[must_use] pub fn parent(&self) -> Option<Self>;
    #[must_use] pub fn children(self) -> impl DoubleEndedIterator<Item = Node<'a>> + 'a;
    pub fn ancestors(self) -> impl Iterator<Item = Node<'a>>;
    pub fn descendants(self) -> impl Iterator<Item = Node<'a>>;
    pub fn select<'s>(self, selector: &'s Selector) -> impl Iterator<Item = Node<'a>> + use<'a, 's>;
    pub fn text_nodes(self) -> impl Iterator<Item = &'a str>;
    #[must_use] pub fn text(&self) -> String;
    #[must_use] pub fn prev_element(&self) -> Option<Self>;
    #[must_use] pub fn next_element(&self) -> Option<Self>;
    #[must_use] pub fn html(&self) -> String;
    #[must_use] pub fn inner_html(&self) -> String;
}
```

* `kind`, `is_*`: node type. There is no `is_text`; use `as_text().is_some()`. Doctype and comment nodes have `name() == None`.
* `name`: the lower-case tag name of an element, else `None`. `attr(name)`: exact (lower-case) match on the attribute name; `None` for non-elements and missing attributes; an attribute without value is `Some("")`. `attrs`: source order; duplicates were dropped at parse time (first wins).
* `id`: `attr("id")`. `classes`: the `class` attribute split on ASCII whitespace; `has_class`: exact, case-sensitive.
* `as_text`: content of a text node (decoded), else `None`. Comments and doctypes are not reachable through `as_text`.
* `parent`: `None` for the document node. `children`: direct children in order (all node kinds), double-ended. `ancestors`: nearest first, the document node last. `descendants`: this node first, then all below in document order (iterative, no recursion). `select`: elements strictly below this node that match.
* `text_nodes` / `text`: text nodes below this node (comments excluded) and their concatenation; includes the text of `script`, `style` and `title` elements.
* `prev_element` / `next_element`: nearest sibling that is an element (O(siblings)).
* `html` / `inner_html`: serialisation. Text and attribute values are escaped (`&` `<` `>`, and `"` in attributes); attributes always use double quotes; void elements (`area base br col embed hr img input link meta param source track wbr`) have no end tag; text inside `script` and `style` is written unescaped; comments are `<!--...-->`; doctype `<!DOCTYPE ...>`. The serialiser is **recursive** (see [Security notes](internals.md#security-notes)).

## `selector` (feature `parse`)

```rust
#[derive(Debug, Clone, PartialEq, Eq)] pub struct SelectorError(String);   // Display: "invalid selector: {0}"; std::error::Error
#[derive(Debug, Clone)] pub struct Selector { /* alternatives */ }
impl Selector {
    pub fn parse(text: &str) -> Result<Self, SelectorError>;
    #[must_use] pub fn matches(&self, node: Node<'_>) -> bool;
}
```

* `parse`: a selector list (see grammar in [Parser behaviour](internals.md#selector-grammar)). `SelectorError` for syntax that is malformed or unsupported.
* `matches`: `false` for non-elements; true when any alternative matches the element.

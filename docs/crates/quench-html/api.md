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
pub mod ast;
pub use ast::{Alignment, Block, Document, Footnote, Inline, List, ListItem, Options, Table};

#[must_use] pub fn parse(text: &str, options: Options) -> Document;
#[must_use] pub fn to_html(text: &str) -> String;
#[must_use] pub fn to_html_with(text: &str, options: Options) -> String;
#[must_use] pub fn render_html(document: &Document) -> String;
```

* `parse(text, options)`: reads `text` with the extensions turned on in `options` into a `Document` (see `markdown::ast` below). Cannot fail: any `&str` gives a tree.
* `to_html(text)`: plain CommonMark 0.31 to HTML; the same as `to_html_with(text, Options::COMMONMARK)`. Unchanged by the extensions: `~~a~~`, `| a |` and `- [ ] x` stay text, and `[^e]: f` is a link reference definition.
* `to_html_with(text, options)`: `render_html(&parse(text, options))`.
* `render_html(document)`: writes a tree as HTML. The tree may come from `parse` or be built or edited by hand (all its fields are `pub`).

HTML output conventions (as in the CommonMark spec examples): block elements end with `\n`; `<img src="..." alt="..." title="..." />`; `<br />`; `<hr />`; code blocks `<pre><code class="language-x">` (the first word of the info string). The extensions are written as cmark-gfm writes them:

| Construct | HTML |
|---|---|
| Table | `<table>\n<thead>\n<tr>\n<th align="left">a</th>\n</tr>\n</thead>\n<tbody>\n<tr>\n<td align="left">1</td>\n</tr>\n</tbody>\n</table>\n`; `align` is left out for `Alignment::None`; no `<tbody>` without body rows |
| Strikethrough | `<del>...</del>` |
| Task item | `<li><input type="checkbox" checked="" disabled="" /> done</li>`, unchecked without `checked=""`; in a loose list the checkbox goes inside the item's `<p>` |
| Footnote reference | `<sup class="footnote-ref"><a href="#fn-LABEL" id="fnref-LABEL" data-footnote-ref>N</a></sup>`; the k-th reference (k > 1) to one footnote has `id="fnref-LABEL-k"` |
| Footnotes | after all blocks: `<section class="footnotes" data-footnotes>\n<ol>\n<li id="fn-LABEL">\n...</li>\n</ol>\n</section>\n`; back links `<a href="#fnref-LABEL" class="footnote-backref" data-footnote-backref data-footnote-backref-idx="N" aria-label="Back to reference N">↩</a>`, for k > 1 with `-k` on href and idx and `<sup class="footnote-ref">k</sup>` after the arrow; they go at the end of the last paragraph, or on a line of their own when the footnote does not end with a paragraph |
| Inline math | `<span class="math inline">\(TEX\)</span>` |
| Display math | `<div class="math display">\[TEX\]</div>\n` |

`LABEL` is the label as written in the definition, percent-encoded like a URL and attribute-escaped. TeX is HTML-escaped (`<` becomes `&lt;`) and otherwise kept as written, for the auto-render extension of KaTeX or MathJax (default delimiters `\(...\)` and `\[...\]`) to typeset.

Raw HTML, `javascript:` URLs and everything else the spec allows pass through. Deterministic; the tests feed 3 000 pseudo-random combinations of construct fragments, and every prefix of each, through `to_html_with(_, Options::ALL)` without a panic (no explicit guarantee is stated; very deep nesting can overflow the stack, see [Security notes](internals.md#security-notes)).

## `markdown::ast` (feature `markdown`)

The parsed document as plain data: blocks with their inlines already read, footnotes numbered. Every type is `Debug + Clone + PartialEq + Eq` and `Send + Sync` (only `String`, `Vec`, numbers and `bool` inside). All fields are `pub`.

### `Options`

```rust
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Options {
    pub tables: bool,          // GFM tables: a header row, a delimiter row (`| --- | :-: |`), then rows
    pub strikethrough: bool,   // `~text~` and `~~text~~`
    pub task_lists: bool,      // `- [ ] to do`, `- [x] done`
    pub footnotes: bool,       // `text[^note]` and `[^note]: the note` (GFM and kramdown syntax)
    pub math: bool,            // `$x$`, `$$x$$` in a paragraph, display math (see below)
}
impl Options {
    pub const COMMONMARK: Self;   // all false; the same as Options::default()
    pub const GFM: Self;          // tables, strikethrough, task_lists, footnotes; math false
    pub const ALL: Self;          // every field true
}
```

The fields can be combined freely, e.g. `Options { math: true, ..Options::COMMONMARK }`. With `math`, display math is a paragraph that is only `$$...$$`, a block between two lines that are only `$$`, or a fenced code block whose info string starts with the word `math`. See [Markdown extensions](internals.md#markdown-extensions) for the exact rules.

### `Document`

```rust
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Document {
    pub blocks: Vec<Block>,
    pub footnotes: Vec<Footnote>,
}
```

`footnotes` holds only footnotes that are referenced, numbered from 1 in the order of their first reference (references inside footnotes count, after those in the body); unreferenced definitions are dropped. Footnote definitions do not appear in `blocks`. Not to be confused with the HTML `quench_html::Document` (feature `parse`); the Markdown one is `quench_html::markdown::Document`.

### `Block`

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    Paragraph(Vec<Inline>),
    Heading { level: u8, content: Vec<Inline> },                  // level 1 to 6
    ThematicBreak,
    CodeBlock { info: String, literal: String, fenced: bool },
    Html(String),                                                 // a raw HTML block, as it is
    BlockQuote(Vec<Self>),
    List(List),
    Table(Table),
    Math(String),                                                 // display formula: TeX without `$$` or fence
}
```

* `CodeBlock::info`: the info string of a fenced block, backslash escapes and entities resolved (`rust`, `toml title="x"`); empty for an indented block. `literal`: the code, ending with `\n` unless empty.
* `Math`: the TeX of a display formula, without its trailing newlines (of a `$$` block or `math` fence) or its surrounding whitespace (of a `$$...$$` paragraph). Line breaks inside are kept.
* A task item's checkbox is not a block: see `ListItem::task`.

### `List`, `ListItem`

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct List {
    pub ordered: bool,
    pub start: usize,          // first number of an ordered list; 0 for a bullet list
    pub tight: bool,           // no blank lines between items: paragraphs in items are not wrapped in <p>
    pub marker: char,          // '-', '+' or '*' for a bullet list; '.' or ')' for an ordered one
    pub items: Vec<ListItem>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListItem {
    pub task: Option<bool>,    // Some(true) checked, Some(false) not, None for an ordinary item
    pub blocks: Vec<Block>,
}
```

For a task item the `[ ]` / `[x]` marker is taken off the first paragraph; that paragraph stays in `blocks` (it may be empty: `- [ ]` alone gives `Some(false)` and an empty paragraph).

### `Table`, `Alignment`

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Table {
    pub alignments: Vec<Alignment>,
    pub header: Vec<Vec<Inline>>,        // one Vec<Inline> per cell
    pub rows: Vec<Vec<Vec<Inline>>>,     // body rows, each with alignments.len() cells
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Alignment { #[default] None, Left, Center, Right }
```

Every row has as many cells as the header: short rows are filled with empty cells (`Vec::new()`), extra cells are dropped. `rows` is empty for a table with only a header. `Alignment` comes from the delimiter row: `---` `None`, `:--` `Left`, `:-:` `Center`, `--:` `Right`.

### `Footnote`

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Footnote {
    pub label: String,         // as written in the definition (`note` of `[^note]:`)
    pub number: usize,         // from 1, in the order of the first reference
    pub references: usize,     // how many times it is referenced (at least 1)
    pub blocks: Vec<Block>,
}
```

### `Inline`

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Inline {
    Text(String),
    Code(String),
    Emph(Vec<Self>),
    Strong(Vec<Self>),
    Strikethrough(Vec<Self>),
    Link { destination: String, title: String, children: Vec<Self> },
    Image { destination: String, title: String, children: Vec<Self> },   // children: the alt text
    Html(String),                                                         // raw inline HTML
    SoftBreak,
    HardBreak,
    FootnoteReference { label: String, number: usize, occurrence: usize },
    Math(String),
}

impl Inline {
    #[must_use] pub fn plain_text(inlines: &[Self]) -> String;
}
```

* `Text` is decoded (entities and backslash escapes resolved); a run of text may be split over several adjacent `Text` items (e.g. around a bracket that did not become a link), so join them when comparing. `Code` is the code span's content with the spec's space stripping and line endings turned into spaces.
* `Link` / `Image`: `destination` with backslash escapes and entities resolved, not percent-encoded (the HTML renderer encodes it); `title` empty when there is none.
* `FootnoteReference`: produced only for `[^label]` naming a defined footnote (with `footnotes` on). `label` is the definition's label; `number` the footnote's number; `occurrence` 1 for the first reference to that footnote, 2 for the second, and so on.
* `Math(tex)`: an inline formula, `$x$` or `$$x$$` inside a paragraph, the TeX trimmed and with newlines turned into spaces. A paragraph that is only `$$x$$` becomes a display `Block::Math` instead.
* `plain_text(inlines)`: the text without markup, what a link or an image's alt text says. `Text`, `Code` and `Math` (its TeX) contribute their text, `Emph`/`Strong`/`Strikethrough`/`Link`/`Image` their children, `SoftBreak` and `HardBreak` a `\n`, `FootnoteReference` its number, `Html` nothing.

```rust
use quench_html::markdown::{Block, Inline, Options, parse};

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
```

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
    pub fn children(self) -> impl DoubleEndedIterator<Item = Node<'a>> + 'a;
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

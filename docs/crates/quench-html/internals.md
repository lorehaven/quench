---
title: Parser behaviour
parent: quench-html
nav_order: 4
---

What the parser, the selector engine, the entity decoder and the Markdown parser and renderer do, read from `src/dom.rs`, `selector.rs`, `entities.rs` and `markdown/`.

## Tree building

`Document::parse` follows the *shape* of HTML5 parsing, not the algorithm. It cannot fail.

* **Skeleton.** `html`, `head` and `body` always exist. Whitespace before the first element is dropped; text or a non-head element starts the body. A second `<html>` merges missing attributes into the first; `<head>` and `<body>` start tags replace the attributes of the implicit element only if it was not already created; end tags `</html>` and `</body>` are ignored.
* **Head content.** `title meta link base style script template` seen before the body go into `head`; everything else starts the body.
* **Tokens.** `<!-- ... -->` comments (`<!-->` is an empty comment; an unterminated comment runs to the end of the input); `<!DOCTYPE ...>` becomes a Doctype node only before the `html` element is created, otherwise a comment; `<?...>` and other `<!...>` become comments; `</name` end tags; `<letter...` start tags; a `<` followed by anything else is text.
* **Start tags.** Names and attribute names are lower-cased (ASCII). Attributes: quoted with `"` or `'`, unquoted up to whitespace or `>`, or without value (empty string). A duplicate attribute name is dropped (first wins). A stray `=` is skipped. `/>` is recorded but **ignored on non-void elements** (`<div/>` stays open). Void elements: `area base br col embed hr img input link meta param source track wbr`.
* **Implicit end tags** (when a start tag arrives): `p` is closed by one of 36 block-level names (`address article aside blockquote center details dialog dir div dl fieldset figcaption figure footer form h1`-`h6 header hgroup hr main menu nav ol p pre section summary table ul li dd`); `li` closes an open `li` (looking no further than `ul ol table td th template html`); `dd`/`dt` close `dd`/`dt`; `option`, `optgroup`, `tr`, `td`/`th`, `tbody`/`thead`/`tfoot`, headings (a heading directly inside a heading), `a` (within scope), `button`.
* **Implied table parts.** `tr` directly in `table` opens a `tbody`; `td`/`th` directly in `table` open `tbody` and `tr`; `td`/`th` in a section open `tr`.
* **End tags.** Pop to the nearest matching open element within the scope boundaries (`html table td th caption template button object marquee`; table elements use only `html`/`template`). An end tag with no matching open element is ignored, except `</p>` (creates an empty `<p></p>`) and `</br>` (becomes `<br>`).
* **Raw text.** After `script style xmp iframe noembed noframes noscript` (not self-closed) everything up to `</name` (case-insensitive) is a single text node, **not** entity-decoded. After `textarea` and `title` the same, but entity-decoded. A newline right after `<pre>`, `<listing>` or `<textarea>` is dropped.
* **Entities** in text and attribute values are decoded. In attribute values a legacy name without `;` is left alone when followed by `=` or an ASCII letter or digit (`?a=1&copy=2` keeps `&copy=2` and serialises as `&amp;copy=2`). Numeric references follow `entities::numeric`.
* **Not implemented:** the adoption agency algorithm (misnested formatting such as `<a><p></a>` is not repaired), foster parenting (text or elements misplaced directly in a table stay where they are), `foreignObject`/SVG/MathML namespaces, `<template>` content isolation, `<select>` special modes, form owner, `<frameset>`.
* **Case:** text is kept as is; `name()` and `attr()` use lower case.

## Selector grammar

`Selector::parse` accepts a comma-separated list of complex selectors:

```text
list      := complex ( ',' complex )*
complex   := compound ( combinator compound )*
combinator:= ' ' (descendant) | '>' | '+' | '~'
compound  := ( type | '*' )? ( '#id' | '.class' | '[attr]' | ':pseudo' )*
attr      := name | name op value ( ' i' )?      op := '=' '~=' '|=' '^=' '$=' '*='
pseudo    := first-child | last-child | only-child | first-of-type | last-of-type | only-of-type
           | nth-child(an+b) | nth-last-child(an+b) | nth-of-type(an+b) | nth-last-of-type(an+b)
           | not(list) | is(list) | where(list) | empty | root
an+b      := odd | even | integer | [-]an[+-b]
```

* Matching: `~=` word in the whitespace list (empty value or a value containing whitespace never matches); `|=` equal or prefix followed by `-`; `^=`, `$=`, `*=` never match an empty value; the `i` flag lowercases both sides (Unicode lowercase); values may be quoted with `"` or `'` or bare identifiers.
* `:empty`: no children except comments (whitespace text counts as content). `:root`: the element whose parent is the document node (`html`).
* `:nth-*` positions count only element siblings (and same-name siblings for the `-of-type` forms); evaluation walks the siblings, so it is O(siblings) per node.
* Not supported (error): other pseudo-classes (`:hover`, `:checked`, `:has()`, `:nth-child(... of S)`), pseudo-elements, namespaces (`ns|a`), `|`. Type, attribute and pseudo-class names in a selector are lower-cased when parsed, so `DIV[CLASS]` works against the lower-cased tree; ids, classes and attribute values are case-sensitive (except with the `i` flag).
* No selector caching: `Selector` is parsed once and reused (`Clone`).

## Entities

`ENTITIES` has 2 125 names (those that end in `;` in the HTML named-reference table, e.g. `amp`, `zwnj`); 106 legacy names (`AElig` ... `yuml`, including `amp lt gt quot nbsp copy reg`) also work without `;`. Without `;` only the longest legacy name that fits in the next 2 to 6 letters is used (`&notit;` decodes to `¬it;`). Numeric references: `&#N;`, `&#xH;` (digit caps 7 and 6), the `;` is optional in non-strict mode; 0, surrogates, out of range give U+FFFD; C1 control codes are **not** remapped to Windows-1252.

## Markdown

`markdown::parse` runs in two passes, then any renderer walks the result:

1. **Blocks** (`markdown/blocks.rs`): the CommonMark block algorithm, line by line, into an internal tree of containers and leaves with their raw text. Link reference definitions are taken off the start of paragraphs when the paragraph closes. With the options it also reads tables, footnote definitions and `$$` math blocks.
2. **Build** (`markdown/build.rs`): the internal tree becomes the public `ast::Document`. It parses the inlines of every paragraph, heading and table cell (`markdown/inlines.rs`), numbers the footnotes, takes task markers off their items and recognises display math.
3. **Render** (`markdown/render.rs`): `render_html` writes the `ast::Document` as HTML; it reads nothing but the tree.

* CommonMark 0.31: passes the 652 spec examples with `Options::COMMONMARK`. Output formatting follows the spec's HTML (newlines between blocks, `<img ... />`, `<br />`, `<hr />`).
* The GFM extensions pass the 24 cmark-gfm examples kept in `tests/fixtures/gfm-extensions.json` (see [Testing](testing.md)), so their HTML matches cmark-gfm's and GitHub's byte for byte.
* Not read, with any options: smart punctuation, GFM extended autolinks (`www.example.com`, bare `https://...` without `<>`), the GFM tag filter (`<script>`, `<iframe>`, ... are not neutralised), definition lists, heading ids, front matter, emoji shortcodes, `==highlight==`, sub/superscript.
* Raw HTML blocks and inline HTML are copied through unchanged; link destinations are not filtered by scheme (`javascript:` is kept; characters are percent-encoded per the spec's URL normalisation).

## Markdown extensions

How each option changes parsing. Each rule is in the source file named.

### Tables (`tables`, `blocks.rs`, `build.rs`)

* A table starts when a line is a **delimiter row** directly under a paragraph line (not indented 4 or more). The delimiter row is split into cells at unescaped `|`, with one leading and one trailing `|` dropped and every cell trimmed; each cell must be one or more `-` with an optional `:` at either end. It **must contain a `|`**, so `text\n---` stays a setext heading. The check runs **before** setext headings, so `a\n|---` is a one-column table.
* The header is the last line of the paragraph and must split into exactly as many cells as the delimiter row; otherwise there is no table (the lines stay paragraph text). Earlier lines of the paragraph stay a paragraph above the table.
* Every following non-blank line is a row (pipes optional, indentation allowed) until a blank line or a line that starts another block (`>`, `#`, a list item, a fence, ...).
* Cells: split like the delimiter row; `\|` is a cell character and becomes `|` before the cell's inlines are parsed, also inside code spans. Rows are cut or padded with empty cells to the header's width.
* Tables work in any container (list items, block quotes, footnotes).

### Strikethrough (`strikethrough`, `inlines.rs`)

`~` is a delimiter character like `*` (same left/right-flanking rules). A closing run matches only an opener run of the **same length**, and only lengths 1 and 2 match: `~a~`, `~~a~~` are struck; `~a~~`, `~~~a~~~` stay text. The whole run is used, so there is no nesting by splitting runs.

### Task lists (`task_lists`, `build.rs`)

An item is a task when its first block is a paragraph whose text starts with `[ ]`, `[x]` or `[X]` followed by a space, a tab or a line end. The marker is removed and `ListItem::task` is set; the rest of the paragraph (possibly empty) stays the item's first paragraph. Tight lists put the checkbox straight into `<li>`, loose lists inside the `<p>`.

### Footnotes (`footnotes`, `blocks.rs`, `inlines.rs`, `build.rs`)

* **Definition:** `[^label]:` at the start of a line (indented at most 3 columns). The label is not empty and contains no whitespace and no `[`; otherwise the line is read as CommonMark (e.g. a link reference definition). The definition is a container: the rest of its first line starts its first block, and it continues over blank lines and lines indented by 4 or more columns (those 4 columns are removed), plus lazy continuation lines of an open paragraph. Labels are normalised like link labels (case-folded, whitespace collapsed); the first definition of a label wins.
* **Reference:** `[^label]` naming a **defined** footnote. It is checked before links, so it never becomes a link; an undefined `[^label]` is parsed as ordinary brackets (text, or a link if a link reference matches). `![^1]` is `!` followed by a reference, not an image.
* **Numbering** (`build.rs`): references are numbered while the inlines are built, in document order. Footnotes get numbers 1, 2, ... by first reference. After the body, the referenced footnotes are built in that order, and references found inside them add further footnotes at the end of the list, until no new one appears (so a footnote referenced only from another footnote is kept). Each reference records its `occurrence` (1, 2, ...); `Footnote::references` is the total. Unreferenced definitions are dropped.

### Math (`math`, `blocks.rs`, `inlines.rs`, `build.rs`)

* **Inline** (`inlines.rs`), by GitHub's dollar rules so that prices and shell variables are not math: `$` opens when not followed by a space, tab, newline or another `$`; the closing `$` is the next unescaped `$` that is not preceded by whitespace and not followed by an ASCII digit, with at least one character in between. `$$` opens a span closed by the next unescaped `$$` (non-empty content, no other condition): this is kramdown's inline `$$...$$`. A backslash skips the next character while searching, so `\$` inside the TeX is kept as TeX. Without a match the dollars are text. Inside, nothing is parsed: newlines become spaces and the TeX is trimmed. Code spans and other constructs that start earlier take precedence (`` `$x$` `` is code); a `\$` outside math is an escaped dollar.
* **Display**, three forms, all giving `Block::Math`:
  * a block between a line that is only `$$` (indented at most 3, trailing spaces allowed) and the next such line. Lines in between are kept verbatim, with up to the opening line's indentation removed. Without a closing line it runs to the end of its container, like a code fence;
  * a paragraph whose trimmed text starts and ends with `$$`, has non-blank content and no other `$$` inside (`build.rs`); the TeX is trimmed;
  * a fenced code block (backticks or tildes) whose info string's first word is `math`; its final newline is removed.
* Without `math`, a `math` fence is an ordinary code block with `class="language-math"`.

## Security notes

* No sanitizer. Never insert `to_html` / `to_html_with` / `render_html` output or `Node::html()` of untrusted input into a page without filtering `script`, event-handler attributes (`on*`), `javascript:`/`data:` URLs, `style`, `iframe`, `object`, `embed`, `svg` and friends.
* In the Markdown output, text, code, TeX, table cells, attribute values and footnote labels are escaped (footnote labels are also percent-encoded in `href`/`id`, so `[^"><script>...]` cannot break out, as cmark-gfm's example 21 in the fixtures checks); only raw HTML from the input and link/image destinations by scheme are left unfiltered.
* `Node::html()` re-escapes text and attribute values, but writes `script`/`style` text and comment bodies verbatim; text of `noscript`, `iframe`, `xmp`, `noembed`, `noframes` is parsed raw and re-escaped on output, so a round trip of those elements changes their text.
* Limits: no size, depth or node-count limit is applied by the parser. Observed in a debug build while writing this page: 20 000 consecutive `<script>x</script>` took about 4 s (every raw-text start tag lower-cases the remainder of the input, so cost is quadratic in input size times the number of raw-text elements); 5 000 nested `<div>` took about 0.5 s and 200 000 did not finish within the time available (implicit-end-tag scans walk the open-element stack, so cost grows quadratically with depth). Bound the input size before parsing untrusted HTML.
* `Node::html()` / `inner_html()` serialise recursively: a document nested deeper than the thread's stack allows can overflow it (from the source; not tested). `descendants`, `select` and `text` are iterative.
* Markdown nesting is limited, so building the tree and rendering it (both recursive: `build.rs`, `render.rs`) stay shallow whatever the input. Containers (block quotes, list items, footnote definitions) nest at most `MAX_DEPTH` = 64 deep (`blocks.rs`); a marker beyond that is text. Emphasis, strikethrough, links and images nest at most `MAX_NESTING` = 64 deep (`inlines.rs`): every made node carries the depth of its tree, a match that would go deeper is refused and its delimiters stay text, and no more than 64 brackets are open at once. Emphasis matching uses cmark's *openers bottom* (where the search for an opener of a closer of the same character, the same can-open, and the same run length mod 3 last failed), so unmatched closers are not walked over again and deep input stays linear. The tests run 10 000 nested `>`, 5 000 nested `- `, 20 000 nested `*`, `_`, `~`, `[` and `![`, and 5 000 footnote markers on a thread with a 2 MiB stack (a tokio worker's), each in well under 2 s in a debug build.

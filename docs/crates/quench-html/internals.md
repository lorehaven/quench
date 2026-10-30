---
title: Parser behaviour
parent: quench-html
nav_order: 4
---

What the parser, the selector engine, the entity decoder and the Markdown renderer do, read from `src/dom.rs`, `selector.rs`, `entities.rs` and `markdown/`.

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

* CommonMark 0.31 only. Passes the 652 spec examples. Output formatting follows the spec's HTML (newlines between blocks, `<img ... />`, `<br />`, `<hr />`).
* Not part of this renderer: GFM tables, task lists, strikethrough, autolink literals, footnotes, heading ids, smart punctuation, front matter. `| a | b |` lines are paragraph text.
* Raw HTML blocks and inline HTML are copied through unchanged; link destinations are not filtered by scheme (`javascript:` is kept; characters are percent-encoded per the spec's URL normalisation).

## Security notes

* No sanitizer. Never insert `to_html` output or `Node::html()` of untrusted input into a page without filtering `script`, event-handler attributes (`on*`), `javascript:`/`data:` URLs, `style`, `iframe`, `object`, `embed`, `svg` and friends.
* `Node::html()` re-escapes text and attribute values, but writes `script`/`style` text and comment bodies verbatim; text of `noscript`, `iframe`, `xmp`, `noembed`, `noframes` is parsed raw and re-escaped on output, so a round trip of those elements changes their text.
* Limits: no size, depth or node-count limit is applied by the parser. Observed in a debug build while writing this page: 20 000 consecutive `<script>x</script>` took about 4 s (every raw-text start tag lower-cases the remainder of the input, so cost is quadratic in input size times the number of raw-text elements); 5 000 nested `<div>` took about 0.5 s and 200 000 did not finish within the time available (implicit-end-tag scans walk the open-element stack, so cost grows quadratically with depth). Bound the input size before parsing untrusted HTML.
* `Node::html()` / `inner_html()` serialise recursively: a document nested deeper than the thread's stack allows can overflow it (from the source; not tested). `descendants`, `select` and `text` are iterative.
* Markdown nesting of 2 000 blockquote levels rendered in under a millisecond in the same debug build; no recursion limit is documented by the crate.

---
title: Internals
parent: quench-web
grand_parent: Crates
nav_order: 8
---

Design notes that affect how pages behave. Source files are named so each claim can be checked.

## Rendering model

`Element` (`html/builder/element.rs`) stores a tag, a `BTreeMap<String, String>` of attributes, a `Vec<Element>` of children, an optional text, `onclick` and `onchange` strings and two flags (`raw`, `defer`). Rendering is a recursive string build; it is deterministic because attributes are sorted. The text is written **before** the children, so an element with both text and children shows the text first.

## Escaping

| Where | Escaped? |
|---|---|
| `Element` attribute values, `onclick`, `onchange` | yes (`& < > " '`) |
| `Element` text | yes, unless `.raw()` |
| Tag names | no |
| `Script::render` (`src`, `crossorigin`, content) | no |
| `PageBuilder` title | yes (`& < >`) |
| `Link` `rel`, `href`, extra attributes | no, in `render_head_link` (the pretty printer re-escapes attribute values after parsing, which neutralises `<`, `&` but cannot repair a value containing `"`, because the quote has already ended the attribute when it is parsed) |
| DOM snippet helpers (`dom::*`) | no |

Consequence: never pass untrusted data as a page title, a link/script URL, a DOM-helper argument or `.raw()` text. Untrusted data is safe in `Element::text` and `Element::attr`.

## The pretty printer

`html/builder/pretty.rs` is a purpose-built reader and writer (the crate has no HTML dependency). It is used by `PageBuilder::build` and is public as `pretty_print_html`.

Reader:

* Recognises start tags with quoted/unquoted/valueless attributes (attribute names lower-cased; the first of a duplicated attribute wins), end tags (lower-cased; an end tag with no matching open element is ignored; closing an outer element closes the inner ones that are still open), comments (dropped), and `<!...>` / `<?...>` declarations (dropped; `PageBuilder::build` writes the doctype itself after printing).
* Void elements (`area base br col embed hr img input link meta param source track wbr`) and `<x/>` forms have no children.
* `script`, `style` bodies are read verbatim up to the next `</script` / `</style` (case-insensitive); `textarea` and `title` bodies are read up to their end tag and character references are decoded. A leading newline in `textarea` and `pre` is dropped.
* Character references: `&amp; &lt; &gt; &quot; &apos; &nbsp;` and numeric `&#N;` / `&#xH;` (the `;` must be within 10 characters); anything else is kept literally.
* No HTML5 tree construction: no implied end tags (`<p>` is not closed by a following `<div>`), no table fix-ups. Input must be well nested.

Wrapper: a top-level `<html>` is used as is (a missing `<head>` is inserted first, a missing `<body>` appended); otherwise the leading `head`-only elements (`base link meta script style title`) go in a new `<head>` and everything from the first other node on goes in `<body>`.

Writer:

* Four spaces of indentation per level; every element has an explicit end tag, including void elements (`<meta ...>` then `</meta>` on the next line).
* Attributes are written in the order they were read, as `name="value"` with the value escaped. A valueless attribute is written as `name=""`.
* `pre`, `code`, `textarea`, `script`, `style` are written on one line with content verbatim (`script`/`style` text unescaped; the others escaped).
* Other text nodes are trimmed, escaped, and put on their own indented line; whitespace-only text is dropped. Inline text next to inline elements therefore loses its surrounding spaces.
* Output ends with `\n`.

## The shell's JavaScript

`locale_script`, `theme_script` and `session_script` are format templates in `framework/app/scripts.rs`. They define top-level `function`s and `const`s (all scripts share the global scope, so a custom script cannot redeclare `getCookie`, `DEFAULT_LOCALE`, `THEMES`, ...). They depend on each other in this order: `translations.js` then locale then theme (uses `getCookie`/`setCookie`) then session. Polling: the locale and theme watchers poll the cookie every 500 ms from page load for as long as the page lives.

## Asset generation

`quench-style` writes the files with `std::fs`. Concurrent processes building shells in the same directory write the same files; there is no locking or atomic rename.

## Thread-safety

All public types are plain owned data (`String`, `Vec`, `BTreeMap`, `Option`) and are `Send + Sync + Clone`. No interior mutability, no global state. The only process-global effect is the file writing in `AppShellBuilder::try_build`.

---
title: Errors
parent: quench-convert
grand_parent: Crates
nav_order: 3
---

One error type, `Error`, with `format()` (`"csv"`, `"xml"`, `"axml"`, `"gguf"`), `message()` and `position()` (1-based line for the text formats; for the binary formats a byte offset; 0 when unknown). `Display` is `format: message (at position)`.

| Format | Messages (the `message()` text) |
|---|---|
| csv | `a quoted field is never closed`, `text after a closing quote` (strict only); `record N has a different number of fields` (`flexible = false`) |
| xml | `a comment is never closed`, `a processing instruction is never closed`, `the doctype is never closed`, `expected a name`, `` attribute `x` has no value ``, `` attribute `x` is not quoted ``, `an attribute value is never closed`, `` element `a` is never closed ``, `` `</b>` closes `<a>` ``, `` expected `>` ``, `CDATA is never closed`, `` an entity is missing its `;` ``, `` unknown entity `&x;` ``, `elements nest too deeply`, `expected a root element`, `content after the root element` |
| axml | `not Android binary XML`, `unexpected end of data`, `chunk outside the file`, `string pool larger than its chunk`, `string outside its pool`, `string index out of range`, `attribute outside its element`, `an element is closed that was never opened`, `an element is never closed`, `the document has no element` |
| gguf | `not a GGUF file`, `unsupported version N (big-endian files are not supported)`, `cannot read the file: <io error>`, `a string is implausibly long`, `implausible entry counts`, `a tensor has too many dimensions`, `unknown value type N`, `arrays nest too deeply` |

The binary readers check offsets through bounds-checked accessors, so truncated input gives an error. XML positions are line numbers where the parser stopped, which may be later than the mistake.

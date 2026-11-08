---
title: Errors
parent: quench-html
nav_order: 3
---

The only error type is `selector::SelectorError`, returned by `Selector::parse`. Parsing HTML, parsing Markdown (`markdown::parse`, with any `Options`) and rendering it (`to_html`, `to_html_with`, `render_html`) cannot fail: they return a value, not a `Result`. Markdown that is not valid for an extension is read as ordinary text (an undefined `[^x]`, an unclosed `$`, a table whose header and delimiter row differ in width). Nesting deeper than 64 is read as text rather than nested (see [Security notes](internals.md#security-notes)), so no input can overflow the stack.

`Display` is `invalid selector: {message}`. Messages produced (positions are byte offsets into the selector text):

| Message | Cause |
|---|---|
| `expected a selector at N` | empty text, trailing comma or combinator, `:not()` / `:is()` with nothing inside, `div + + p` |
| `expected a name at N` | `a[` (no attribute name), `::before` |
| `unterminated string` | a quoted attribute value is not closed |
| `bad attribute selector` | an operator character that is not one of `= ~= \|= ^= $= *=` |
| `expected `=` in an attribute selector` | e.g. `[a~]` |
| `expected `]`` | `[a=b` |
| `:NAME needs an argument` | `:nth-child` and friends or `:not` without parentheses |
| `expected `)`` | an unclosed pseudo-class argument |
| `unsupported pseudo-class :NAME` | anything outside the supported list, e.g. `:hover`, `:checked`, `:has(...)` |
| `bad an+b expression `X`` | `p:nth-child(x)` |
| `unexpected `\|`` | namespace selectors such as `*\|*` |
| other messages | produced for unexpected characters by the tokenizer (`src/selector.rs` line 191) |

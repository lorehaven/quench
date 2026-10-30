---
title: Errors
parent: quench-html
nav_order: 3
---

The only error type is `selector::SelectorError`, returned by `Selector::parse`. Parsing HTML and rendering Markdown cannot fail.

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

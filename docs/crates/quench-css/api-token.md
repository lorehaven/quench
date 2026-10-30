---
title: "API: token"
parent: "quench-css"
nav_order: 9
---

Module `token` (private module; items re-exported at the crate root). The tokenizer is a subset of CSS Syntax Level 3. It never fails.

## `Token`

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Ident(String),
    Function(String),
    AtKeyword(String),
    Hash(String),
    Str(String),
    Number { value: f32, unit: String },
    Delim(char),
    Whitespace,
    Colon,
    Semicolon,
    Comma,
    Open(char),
    Close(char),
}
```

| Variant | Meaning |
|---|---|
| `Ident(name)` | An identifier, case kept (`--x`, `-webkit-box`, `color`). |
| `Function(name)` | A name followed directly by `(`; the `(` is consumed and there is no matching `Open`. The closing `)` arrives as `Close(')')`. |
| `AtKeyword(name)` | `@media` is `AtKeyword("media")` (no `@`). |
| `Hash(s)` | `#fff` is `Hash("fff")` (no `#`); also used for ids. |
| `Str(s)` | A quoted string without its quotes, or the raw content of an unquoted `url(...)`. |
| `Number { value, unit }` | `16px` is `value: 16.0, unit: "px"`; `50%` has unit `"%"`; a bare number has unit `""`. The unit is lowercased. |
| `Delim(c)` | Any other character (`.`, `>`, `+`, `~`, `*`, `&`, `=`, `/`, `!`, `\`, `<`, ...). |
| `Whitespace` | A run of whitespace (comments are dropped; they do not produce whitespace). |
| `Colon`, `Semicolon`, `Comma` | `:`, `;`, `,`. |
| `Open(c)` / `Close(c)` | `{ ( [` / `} ) ]`. |

## `tokenize`

```rust
#[must_use]
pub fn tokenize(text: &str) -> Vec<Token>
```

Cuts `text` into tokens. Exact rules, from the code:

* Comments `/* ... */` are dropped; an unterminated comment runs to the end of the input.
* Whitespace is any `char::is_whitespace` run, one `Whitespace` token.
* Strings: `"` or `'`. A backslash makes the next character literal (so `\"` is a quote and `\<newline>` yields the newline); hex escapes (`\62`) are **not** decoded. An unterminated string runs to the end of the input.
* `#` followed by an identifier character is `Hash`; otherwise `Delim('#')`.
* `@` followed by an identifier start or `-` is `AtKeyword`; otherwise `Delim('@')`.
* Numbers start with a digit, `.digit`, or a sign followed by a digit or `.digit`. An optional fraction, then an optional exponent (`1e3`, `1E+3`, `2e-1`; the `e` of `em`/`ex` is not an exponent because no digit follows). Unit: `%`, or a run of alphabetic characters and hyphens (a hyphen is allowed only after the first unit character). The text is parsed as `f32`; a parse failure yields `0.0`.
* Identifier start: alphabetic (Unicode), `_`, or any non-ASCII character. Identifier characters add ASCII digits and `-`. A `-` starts an identifier when followed by an identifier start or another `-`. **Backslash escapes in identifiers are not supported** (`.a\:b` tokenizes as `Delim('.')`, `Ident("a")`, `Delim('\\')`, `Colon`, ...).
* An identifier directly followed by `(` is `Function`. For `url(` (case-insensitive name) not followed by a quote, everything up to the first `)` is one trimmed `Str` token: `Function("url")`, `Str(raw)`, then `Close(')')`. This keeps the colons, semicolons and commas of a data URL in one token. Without a closing `)`, the string runs to the end of the input.
* `<!--` and `-->` are not recognised (they become `Delim`s).
* `!` is `Delim('!')`; `!important` is `Delim('!')`, `Ident("important")`.

```rust
use quench_css::{Token, tokenize};

assert_eq!(
    tokenize("1.5rem"),
    vec![Token::Number { value: 1.5, unit: "rem".into() }]
);
assert_eq!(
    tokenize("url(a:b;c)"),
    vec![Token::Function("url".into()), Token::Str("a:b;c".into()), Token::Close(')')]
);
```

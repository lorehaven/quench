---
title: "API: value"
parent: "quench-css"
nav_order: 10
---

Module `value`: the tokens of a declaration's value grouped into component values.

## `Value`

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Ident(String),
    Number(f32),
    Percent(f32),
    Dimension(f32, String),
    Hash(String),
    Str(String),
    Function(String, Vec<Value>),
    Block(char, Vec<Value>),
    Comma,
    Delim(char),
}
```

| Variant | Built from |
|---|---|
| `Ident(s)` | `Token::Ident`; also `Token::AtKeyword` as `"@name"`. Case kept. |
| `Number(n)` | `Token::Number` with unit `""`. |
| `Percent(p)` | unit `"%"`; `50%` is `Percent(50.0)`. |
| `Dimension(n, unit)` | any other unit (lowercase). `16px` is `Dimension(16.0, "px")`; `1fr` is `Dimension(1.0, "fr")`. |
| `Hash(s)` | `Token::Hash`. |
| `Str(s)` | `Token::Str`. |
| `Function(name, args)` | `Token::Function` up to its `)`; `name` is lowercased; `args` are the nested components (commas are `Value::Comma`). |
| `Block(open, items)` | `(`, `[` or `{` group up to the closing bracket; `open` is the opening character. |
| `Comma` | `Token::Comma`. |
| `Delim(c)` | `Token::Delim(c)`; also a `Colon` (`':'`), a `Semicolon` (`';'`) and any unmatched `Close(c)` (as `c`), so that media-query values like `(hover: hover)` survive. |

Whitespace is dropped, so `10px -2px` is two `Dimension`s and `a b` is two `Ident`s.

## `parse_values`

```rust
#[must_use]
pub fn parse_values(tokens: &[Token]) -> Vec<Value>
```

Builds the components of `tokens`. An unclosed function or block ends at the end of the input.

## `Value::ident`

```rust
#[must_use]
pub fn ident(&self) -> Option<String>
```

The identifier, ASCII-lowercased, if the value is `Ident`; `None` otherwise.

## `Value::number`

```rust
#[must_use]
pub const fn number(&self) -> Option<f32>
```

The number of a bare `Value::Number`; `None` for `Percent` and `Dimension`.

## `split_commas`

```rust
#[must_use]
pub fn split_commas(values: &[Value]) -> Vec<Vec<Value>>
```

Splits at top-level `Value::Comma`s (commas inside a `Function` or `Block` are already nested). `a b, c` gives `[[a, b], [c]]`; an empty slice gives `[[]]` (one empty part); a trailing comma gives a trailing empty part.

```rust
use quench_css::{Value, parse_values, split_commas, tokenize};

let v = parse_values(&tokenize("1px solid, rgb(0 0 0)"));
let parts = split_commas(&v);
assert_eq!(parts.len(), 2);
assert_eq!(parts[0][0], Value::Dimension(1.0, "px".into()));
assert!(matches!(&parts[1][0], Value::Function(name, _) if name == "rgb"));
```

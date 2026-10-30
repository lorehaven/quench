---
title: "API: RESP codec"
parent: quench-redis
grand_parent: Crates
nav_order: 4
---

The public module `quench_redis::resp` (also `quench_redis::Value`): the RESP2/RESP3 value type, an incremental decoder and a command encoder. No I/O.

## Limits

| Constant (private) | Value | Effect |
|---|---|---|
| `MAX_BULK` | 512 MiB | A bulk, blob-error or verbatim string longer than this is `Error::Protocol("bulk of N bytes is over the limit")`. |
| `MAX_ELEMENTS` | 16 Mi (16 * 1024 * 1024) | An aggregate with more elements (for maps and attributes: pairs times two) is `Error::Protocol("aggregate of N elements is over the limit")`. |
| `MAX_DEPTH` | 32 | Nesting deeper than 32 is `Error::Protocol("replies nested too deeply")`. |

## `Value`

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Nil,
    Int(i64),
    Bulk(Vec<u8>),
    Simple(String),
    Array(Vec<Self>),
    Map(Vec<(Self, Self)>),   // RESP3 map, in the order sent
    Set(Vec<Self>),           // RESP3 set
    Double(f64),
    Bool(bool),
    BigNumber(String),
    Verbatim(String),         // RESP3 verbatim string; the three-letter format is dropped
    Push(Vec<Self>),          // RESP3 out-of-band push
    Error(ServerError),       // an error inside an aggregate; top level becomes Error::Server
}
```

Mapping from the wire (first byte of a value):

| Byte | Wire type | Becomes |
|---|---|---|
| `+` | simple string | `Simple` (lossy UTF-8) |
| `-` | simple error | `Error(ServerError::parse(text))` |
| `:` | integer | `Int` |
| `_` | RESP3 null | `Nil` |
| `#` | boolean (`t`/`f`) | `Bool`; anything else is `Error::Protocol("bad boolean")` |
| `,` | double (`inf`, `-inf`, `nan` or a decimal) | `Double`; unparsable is `Error::Protocol("bad double")` |
| `(` | big number | `BigNumber` (text) |
| `$` | bulk string; length `-1` is nil | `Bulk`, or `Nil` for `$-1` |
| `!` | blob error | `Error(ServerError::parse(text))` |
| `=` | verbatim string (`txt:` + text) | `Verbatim` with the first four bytes (`fmt:`) removed (if the body is shorter than 4 bytes it is kept whole) |
| `*` | array; count `-1` is nil | `Array`, or `Nil` for `*-1` |
| `~` | set | `Set` |
| `>` | push | `Push` |
| `%` | map | `Map` |
| `\|` | attribute | parsed and **discarded**; the value that follows it is returned |
| other | | `Error::Protocol("unknown type byte 'x'")` |

A length other than `-1` that is negative is `Error::Protocol("negative length N")`; a bulk body not followed by CRLF is `Error::Protocol("bulk not terminated by CRLF")`; a non-numeric number is `Error::Protocol("bad number ..")`.

```rust
#[must_use]
pub fn as_str(&self) -> Option<&str>
```
Text of a `Bulk` (if UTF-8), `Simple` or `Verbatim`; `None` for every other variant.

## Functions

```rust
pub fn encode_command(out: &mut Vec<u8>, args: &[Vec<u8>])
```
Appends one command in multi-bulk format: `*<n>\r\n` then for each argument `$<len>\r\n<bytes>\r\n`. Appends to `out` without clearing it.

```rust
pub fn decode(input: &[u8]) -> Result<Option<(Value, usize)>>
```
Decodes one value from the front of `input`. Returns `Ok(Some((value, bytes_used)))`, or `Ok(None)` when `input` holds only part of a value (call again with more bytes). `bytes_used` includes any skipped attribute. Errors: `Error::Protocol` for input that cannot be RESP or exceeds the limits. The decoder is stateless: after `Ok(None)` the caller re-parses from the start with the larger buffer (`Connection` does exactly that).

```rust
use quench_redis::resp::{decode, Value};
assert_eq!(decode(b"*2\r\n:1\r\n$2\r\nhi\r\n")?,
           Some((Value::Array(vec![Value::Int(1), Value::Bulk(b"hi".to_vec())]), 17)));
assert_eq!(decode(b"%1\r\n+k\r\n:1\r\n")?,
           Some((Value::Map(vec![(Value::Simple("k".into()), Value::Int(1))]), 13)));
```

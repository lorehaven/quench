---
title: Errors
parent: quench-serde
grand_parent: Crates
nav_order: 4
---

One error type serves every codec and every conversion.

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error { /* message: String, location: Option<(usize, usize)>, path: String */ }
pub type Result<T> = std::result::Result<T, Error>;
impl std::error::Error for Error {}
```

| Method | Signature | Notes |
|---|---|---|
| `new` | `pub fn new(message: impl fmt::Display) -> Self` | no location, empty path |
| `at` | `pub fn at(message: impl fmt::Display, text: &str, offset: usize) -> Self` | 1-based line and column of byte `offset` in `text`; an offset past the end clamps |
| `message` | `#[must_use] pub fn message(&self) -> &str` | |
| `location` | `#[must_use] pub const fn location(&self) -> Option<(usize, usize)>` | `(line, column)`, both 1-based |
| `path` | `#[must_use] pub fn path(&self) -> &str` | the place in the data, e.g. `user.tags[2]`; empty at top level |
| `in_field` | `#[must_use] pub fn in_field(self, name: &str) -> Self` | prefixes the path with a field name |
| `in_index` | `#[must_use] pub fn in_index(self, index: usize) -> Self` | prefixes the path with `[index]` |

Paths are built from the inside out as the error travels up: `in_index(2)`, then `in_field("tags")`, then `in_field("user")` gives `user.tags[2]`; two indexes give `grid[0][1]`.

`Display` is `"{path}: "` (when the path is non-empty) followed by `{message}` or `{message} at line {line} column {column}`. Example: `f: oops at line 2 column 1`.

## Where errors come from

| Source | Message shape |
|---|---|
| wrong type | `invalid type: expected a string, found number` |
| integer range | `integer 300 is out of range for u8` |
| missing field (derive) | `missing field `name`` |
| unknown field (`deny_unknown_fields`) | ``unknown field `x`, expected one of `a`, `b` `` |
| unknown variant | ``unknown variant `X`, expected one of `A`, `B` `` |
| enum object with several keys | `expected an object with a single key naming the variant, found {n} keys` |
| internally tagged enum, no tag | `missing field `type`` |
| untagged enum | ``data did not match any variant of untagged enum `E` `` |
| uninhabited enum | ``cannot deserialize the uninhabited enum `E` `` |
| array length | `expected an array of length N, found M` |
| text parsers | the messages listed in [API - formats](api-formats.md), with line and column |

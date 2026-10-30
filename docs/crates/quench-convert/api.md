---
title: API reference
parent: quench-convert
grand_parent: Crates
nav_order: 2
---

Public items of `quench_convert`, by module. Every module is behind the feature of the same name. No `unsafe`.

## Crate root

```rust
pub use error::{Error, Result};
pub use quench_serde::Value;
```

## `Error` and `Result`

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error { /* format, message, position */ }
impl Error {
    #[must_use] pub const fn format(&self) -> &'static str       // "csv", "xml", "axml" or "gguf"
    #[must_use] pub fn message(&self) -> &str
    #[must_use] pub const fn position(&self) -> usize           // 1-based line (text formats), byte offset (binary), 0 if unknown
}
impl fmt::Display for Error     // "{format}: {message} (at {position})", or without "(at ..)" when position is 0
impl std::error::Error for Error
pub type Result<T> = std::result::Result<T, Error>;
```

## Module `csv` (feature `csv`)

### `struct Options`

```rust
#[derive(Debug, Clone)]
pub struct Options {
    pub delimiter: u8,
    pub header: bool,
    pub flexible: bool,
    pub trim: bool,
    pub strict: bool,
}
impl Default for Options   // delimiter b',', header true, flexible true, trim false, strict false
```

| Field | Meaning |
|---|---|
| `delimiter` | the field separator byte (use ASCII; it is converted with `char::from`) |
| `header` | the first record names the columns: records become objects keyed by the names (a field past the last name, or under an empty name, is keyed `column<N>`, N counting from 1); otherwise records are arrays of strings, header row included as data |
| `flexible` | records may have different numbers of fields; when `false`, a record whose length differs from the first (header) record is an error |
| `trim` | trim whitespace around every unquoted field and header name (quoted fields keep their spaces) |
| `strict` | refuse malformed quoting (an unclosed quote, text after a closing quote) |

### Functions

```rust
pub fn records(text: &str, delimiter: u8, trim: bool) -> Result<Vec<Vec<String>>>
pub fn parse(text: &str, options: &Options) -> Result<Vec<Value>>
#[must_use] pub fn to_string(rows: &[Value], delimiter: u8) -> String
```

* `records`: the raw records with no header handling; lenient (a quote inside an unquoted field is a character, text after a closing quote joins the field, an open quote runs to the end). Never fails.
* `parse`: a UTF-8 BOM is skipped; `\n`, `\r\n` and `\r` end records; completely empty lines are skipped; a final record needs no terminator. Returns objects (header on) or arrays of strings (header off). Errors: `strict` quoting (`a quoted field is never closed`, `text after a closing quote`) and `flexible = false` (`record N has a different number of fields`).
* `to_string`: see [usage](usage.md#4-writing-csv). Every row ends with `\n`. A non-object, non-array row is written as a single field.

## Module `xml` (feature `xml`)

An element is an object `{"tag": name, "attrs": {name: string}, "children": [element or string]}`. A document is its root element.

### `struct Options`

```rust
#[derive(Debug, Clone, Default)]
pub struct Options { pub keep_whitespace: bool }
```

`keep_whitespace`: keep text nodes that are only whitespace (dropped by default).

### Functions

```rust
#[must_use] pub fn element(tag: &str, attrs: Map, children: Vec<Value>) -> Value
#[must_use] pub fn tag(element: &Value) -> Option<&str>
#[must_use] pub fn attr<'a>(element: &'a Value, name: &str) -> Option<&'a str>
#[must_use] pub fn children(element: &Value) -> &[Value]
pub fn elements<'a>(element: &'a Value, name: &'a str) -> impl Iterator<Item = &'a Value>
#[must_use] pub fn child<'a>(element: &'a Value, name: &str) -> Option<&'a Value>
#[must_use] pub fn text(element: &Value) -> String
pub fn parse(text: &str) -> Result<Value>
pub fn parse_with(text: &str, options: &Options) -> Result<Value>
#[must_use] pub fn to_string(value: &Value) -> String
#[must_use] pub fn to_string_pretty(value: &Value, width: usize) -> String
```

| Function | Behaviour |
|---|---|
| `element` | builds an element value (keys in the order `tag`, `attrs`, `children`) |
| `tag`, `attr`, `children` | read an element value; `attr` returns `None` for a non-string attribute; `children` is empty when absent |
| `elements(e, name)` | child elements with that tag |
| `child(e, name)` | the first such child |
| `text(e)` | all text inside, descendants included, concatenated |
| `parse`, `parse_with` | see [formats](formats.md#xml). Errors: mismatched or unclosed tags, unquoted or valueless attributes, unknown entities, no root element, content after the root, nesting deeper than 256 |
| `to_string` | compact XML; a childless element is written self-closing (`<e/>`); attribute values escape `& < > "`; text escapes `& < >`; a value that is not an element is written with the tag `element` |
| `to_string_pretty(v, width)` | elements that hold elements go on their own lines indented by `width` spaces per level; elements with only text stay on one line |

## Module `axml` (features `axml`, implies `xml`)

```rust
pub fn parse(data: &[u8]) -> Result<Value>
```

Reads a compiled Android XML file (the manifest or a layout inside an APK) into its root element, in the shape of `xml`. Errors: not Android binary XML (the first u16 must be `0x0003`), truncated or out-of-bounds chunks, string indexes outside the pool, an element closed but never opened, an element never closed, no element.

## Module `gguf` (feature `gguf`)

### `struct Options`

```rust
#[derive(Debug, Clone)]
pub struct Options { pub max_array: usize, pub tensors: bool }
impl Default for Options   // max_array 1024, tensors false
```

* `max_array`: metadata arrays with more elements than this are replaced by `{"array_length": n}` (they are still read through, not held in memory).
* `tensors`: also read the tensor table after the metadata.

### Functions

```rust
pub fn read<R: Read>(reader: R, options: &Options) -> Result<Value>
pub fn parse(bytes: &[u8], options: &Options) -> Result<Value>
```

`parse` is `read` on a byte slice. The result is `{"version", "tensor_count", "metadata": {key: value}}` plus `"tensors": [{"name", "dimensions", "type", "offset"}]` when `tensors` is set. Errors: bad magic, versions other than 1-3 (big-endian files are not supported), implausible counts or string lengths, unknown value types, nested arrays deeper than 4, truncated streams.

---
title: API - formats
parent: quench-serde
grand_parent: Crates
nav_order: 3
---

The format modules: `json`, `urlencoded`, `toml` (feature `toml`) and `yaml` (feature `yaml`). Every format goes through `Value`; all return `quench_serde::Result<_>` (see [Errors](errors.md)). The traits and `Value` are in [API - values and traits](api.md).

## `json`

JSON (RFC 8259) to and from `Value`. The parser is strict: no comments, trailing commas, leading zeros, single quotes, unescaped control characters, lone surrogates or trailing content. Integers beyond the 64-bit range parse as floats; `-0` parses as the float `-0.0`.

```rust
pub const MAX_DEPTH: usize = 128;
pub fn parse(text: &str) -> Result<Value>
pub fn parse_slice(bytes: &[u8]) -> Result<Value>
pub fn from_str<T: Deserialize>(text: &str) -> Result<T>
pub fn from_slice<T: Deserialize>(bytes: &[u8]) -> Result<T>
#[must_use] pub fn to_string<T: Serialize + ?Sized>(value: &T) -> String
#[must_use] pub fn to_string_pretty<T: Serialize + ?Sized>(value: &T) -> String
#[must_use] pub fn to_vec<T: Serialize + ?Sized>(value: &T) -> Vec<u8>
#[must_use] pub fn float_text(f: f64) -> String
```

| Item | Behaviour |
|---|---|
| `MAX_DEPTH` | how deeply arrays and objects may nest (128); deeper input fails with `recursion limit exceeded` |
| `parse` | one document; surrounding whitespace allowed; errors carry line and column |
| `parse_slice` | bytes must be UTF-8: `invalid UTF-8 in JSON at byte {n}` |
| `from_str`, `from_slice` | `parse` then `T::deserialize` |
| `to_string` | compact: `{"a":[1,2]}`. Cannot fail |
| `to_string_pretty` | two-space indent, one element per line |
| `to_vec` | `to_string(..).into_bytes()` |
| `float_text` | a float as JSON writes it (`1.0`, `0.00001`, `1e+20`); `null` if not finite. Plain decimal from `1e-5` up to (not including) `1e16` with `.0` on whole numbers, scientific beyond (`serde_json` layout) |

Parser error messages: `expected a JSON value`, `unexpected end of input`, `expected `:``, `expected a string key`, `expected `,` or `]``, `expected `,` or `}``, `invalid literal`, `invalid number`, `expected digits after the decimal point`, `expected digits in the exponent`, `invalid escape`, `unterminated escape`, `invalid unicode escape`, `truncated unicode escape`, `unpaired high surrogate`, `unpaired low surrogate`, `invalid low surrogate`, `unterminated string`, `unescaped control character in string`, `recursion limit exceeded`, `trailing characters after the JSON value`.

## `urlencoded`

`application/x-www-form-urlencoded`: query strings and form bodies.

```rust
pub fn from_str<T: Deserialize>(text: &str) -> Result<T>
pub fn from_bytes<T: Deserialize>(bytes: &[u8]) -> Result<T>
#[must_use] pub fn parse_pairs(text: &str) -> Vec<(String, String)>
pub fn to_string<T: Serialize + ?Sized>(value: &T) -> Result<String>
#[must_use] pub fn encode(text: &str) -> String
#[must_use] pub fn decode(text: &str) -> String
#[must_use] pub fn percent_encode(text: &str) -> String
#[must_use] pub fn percent_decode(text: &str) -> String
```

* `from_str`: reads `a=1&b=two` into `T`. A repeated key becomes a list. While reading, the scalar `Deserialize` impls accept numeric and boolean text (`n=5` into a `u32`) and a lone value reads as a one-element `Vec` (`tags=a`). JSON stays strict.
* `from_bytes`: decodes bytes as UTF-8 lossily (invalid sequences become U+FFFD), then `from_str`.
* `parse_pairs`: the decoded `(key, value)` pairs in order.
* `to_string`: writes a struct or map (object), or a list of `(key, value)` pairs (an array of two-element arrays); `Null` gives an empty string. Lists become repeated keys; `None`/`null` values are omitted; nested objects have no form: `a {type} cannot be written as a form value`; a list element that is not a pair: `a form list must hold (key, value) pairs`; a root that is none of these: `invalid type: expected a map, struct or list of pairs, found ...`.
* `encode`: form encoding; space is `+`; ASCII alphanumerics and `* - . _` stay; every other byte is `%XX`. `decode` reverses it (`+` is a space; a malformed escape is kept as written; invalid UTF-8 becomes U+FFFD).
* `percent_encode`: RFC 3986: space is `%20`, `A-Z a-z 0-9 - . _ ~` stay, no `+`. `percent_decode` reverses it (`+` stays a plus).

## `toml` (feature `toml`, default)

TOML 1.0 to and from `Value`. A table is a `Value::Object`.

```rust
pub fn from_str<T: Deserialize>(text: &str) -> Result<T>
pub fn parse(text: &str) -> Result<Value>
pub fn to_string<T: Serialize + ?Sized>(value: &T) -> Result<String>
pub fn to_string_pretty<T: Serialize + ?Sized>(value: &T) -> Result<String>
```

* Reading (no nesting limit: see [Internals](internals.md#limits)): besides TOML 1.0 the parser accepts what 1.1 added (newlines and a trailing comma inside inline tables, optional seconds in times, the `\e` and `\xHH` escapes). It enforces the table rules: a key cannot be defined twice, a `[table]` cannot be defined twice, a table made with dotted keys cannot later get a `[header]`, inline tables and arrays are closed once written. Integers are 64-bit (hex/octal/binary and `_` separators accepted); `inf` and `nan` read as non-finite floats.
* Dates and times have no `Value` form: a datetime reads as its RFC 3339 text (`1979-05-27T07:32:00Z`, or the local date/time form) and can be read into `quench_core::time::Time` when it has an offset.
* Writing requires an object at the root (`a TOML document is a table, found {type}`). Simple values first, then `[tables]`, then `[[arrays of tables]]`; `to_string` keeps arrays on one line (`a = [1, 2]`); `to_string_pretty` puts array elements one per line and uses `"""` strings for text with line breaks. A `null` entry in a table is omitted; a `null` inside an array is an error (`TOML cannot represent null here`).
* Parse errors include line and column; messages include `expected a key`, `expected `=` after the key`, `expected a value`, `expected a new line after the value`, `expected `]` closing the table header`, `expected `,` or `]` in the array`, `expected `,` or `}` in the inline table`, `unterminated string`, `invalid escape sequence`, `control character in string`.

## `yaml` (feature `yaml`)

YAML 1.2 on `Value`.

```rust
pub use crate::{Error, Map, Result, Value};
pub use parse::{parse, parse_all};
pub use write::{to_string, to_string_all};
pub fn from_str<T: Deserialize>(text: &str) -> Result<T>
pub fn from_str_all<T: Deserialize>(text: &str) -> Result<Vec<T>>
pub fn to_value<T: Serialize + ?Sized>(value: &T) -> Value
pub fn parse(text: &str) -> Result<Value>
pub fn parse_all(text: &str) -> Result<Vec<Value>>
pub fn to_string<T: Serialize + ?Sized>(value: &T) -> Result<String>
pub fn to_string_all<T: Serialize>(values: &[T]) -> Result<String>
```

| Function | Behaviour |
|---|---|
| `parse` | one document; an empty stream is `Null`; several documents are an error (`expected a single YAML document, found several (use parse_all)`) |
| `parse_all` | every document of a stream (`---`, `...` separators; `%` directives skipped; a leading byte-order mark is skipped) |
| `from_str` | `parse` then `T::deserialize` |
| `from_str_all` | one `T` per document |
| `to_value` | `value.serialize()` |
| `to_string` | one document, no `---` line |
| `to_string_all` | documents separated by `---` lines |

Reads: block and flow collections (compact `- key: v` and `- - x` forms; a sequence flush with its key), comments, plain / single-quoted / double-quoted scalars (all escapes, line folding), `|` and `>` block scalars with indentation indicators and `-`/`+` chomping, anchors and aliases (an alias is a copy), tags (`!!str`, `!!int`, `!!float`, `!!bool`, `!!null` change the type; others are ignored), several documents. Plain scalars follow the 1.2 core schema: `null`/`~`/empty, `true`/`false` in the spellings `true True TRUE`, integers (decimal, `0x`, `0o`, `0b`), floats (`.5`, `5.`, `1e3`, `.inf`, `-.inf`, `.nan`). Mapping keys that are not strings are read as their text.

The YAML reader has no nesting limit either ([Internals](internals.md#limits)). Not supported: complex `? ` keys (`complex mapping keys (`? `) are not supported`), tabs as indentation (`tabs cannot be used for indentation`). YAML 1.1 extras (`yes`/`on` booleans, `<<` merge keys, sexagesimal numbers) are not interpreted: the text stays a string and `<<` stays an ordinary key.

Writes block style, two-space indentation, sequences flush with their key (the `serde_yaml`/`kubectl` layout). A string is written plain when it reads back as the same string (`is_plain_safe`); otherwise single-quoted (`'` doubled); double-quoted (escapes `\"`, `\\`, `\n`, `\r`, `\t`, `\xHH`, `\uHHHH`) when it contains control characters or U+2028/U+2029; text with line breaks (and no other control characters, not starting with a space or newline) becomes a `|` block with chomp indicator `-` (no final newline), none (one final newline) or `+` (several). Null is `null`, non-finite floats `.nan`/`.inf`/`-.inf`, exponents are written `e20` (not `e+20`), empty collections `[]` and `{}`.

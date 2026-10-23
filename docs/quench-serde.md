# Quench Serde

`quench-serde` (crate `quench_serde`) replaces `serde`, `serde_json`, `serde_urlencoded` and `toml` and `serde_norway` (YAML) with one small crate whose dependencies are `quench-core` and its derive macros. It builds for `wasm32-unknown-unknown`.

Types do not implement a visitor protocol. They convert to and from a dynamic [`Value`](#value) and every format parses into or prints from that same `Value`, so each format only has to know `Value`.

## Features

`json` and `urlencoded` are always on (`Value` prints as JSON). `toml` (default on) is the TOML 1.0 reader and writer; `yaml` (default off) is YAML 1.2, see [YAML](./quench-serde-yaml.md). `quench-config` enables `yaml`; `quench-db` uses the default `toml`.

## Value

`Value` is `Null | Bool | Int | UInt | Float | String | Array | Object`.

- **Canonical integers.** A non-negative integer is always `UInt`, a negative one `Int`, so equal numbers compare equal. A `Float` never equals an integer.
- **Ordered objects.** `Map` keeps insertion order (a struct keeps its field order through JSON, a parsed document is not reshuffled). Equality ignores order. A repeated key keeps the last value in the first position.
- **Non-finite floats** (NaN, infinities) have no JSON form and become `Null`.
- Accessors (`as_str`, `as_i64`, ...), `get`/`get_mut` by key or position, `value["key"][0]` (a missing entry reads as `Null`), `pointer("/a/0")` (RFC 6901), `From` for the std scalar types, `Vec`, `Option`, `Map`. `value["key"] = ..` creates the key (a `Null` becomes an object; indexing a scalar panics, as in `serde_json`). A `Value` compares with `&str`, `String`, `bool`, `f64` and every integer type, `usize`/`isize` included.

## JSON (`json`)

- `parse(&str)`, `parse_slice(&[u8])` — strict RFC 8259: no comments, trailing commas, leading zeros, unescaped control characters, lone surrogates or trailing content; nesting is capped at 128; errors carry line and column.
- `to_string`, `to_string_pretty` (two-space indent), `to_vec`.
- Output matches `serde_json` byte for byte on 140k generated documents (compact and pretty), including float layout (`0.00001`, `1e-6`, `1e+20`, `1.0`). The one known difference: when the shortest round-trip digits tie on the last digit, Rust's formatter and `serde_json` can pick neighbouring digits (about 3 in 10,000 random floats); both texts read back as the same number.
- Integers beyond the 64-bit range parse as floats; `-0` parses as the float `-0.0`.

## URL-encoded forms (`urlencoded`)

`from_str::<T>` / `from_bytes` read `a=1&b=two` (query strings, form bodies) into a typed value; `to_string` writes a struct, map or list of pairs. `encode` / `decode` are the form percent-encoding on its own (`+` for space; `*-._` and alphanumerics stay).

- Every value is text, so while a form is being read the scalar impls accept numeric and boolean text (`n=5` into a `u32`), and a lone value is a one-element list (`tags=a` into a `Vec`). JSON stays strict: `"5"` is not a number there.
- A key that repeats becomes a list (`tags=a&tags=b`); `None` and `null` are left out when writing; nested objects have no form and are an error.
- Output and decoding match `serde_urlencoded` on 200k generated and hostile inputs.
- `urlencoded::percent_encode` / `percent_decode` are RFC 3986 percent-encoding for URL paths and query values (a space is `%20`, `+` is a plus), the replacement for the `urlencoding` crate; `encode` / `decode` are the form variant (space is `+`).

## TOML (`toml`)

`parse` / `from_str::<T>` read TOML 1.0 (and the 1.1 additions: newlines and a trailing comma in inline tables, optional seconds in times, the `\e` and `\xHH` escapes); `to_string` / `to_string_pretty` write it. A table is an object.

- Strict about structure, like the spec: duplicate keys, a `[table]` defined twice, a header after dotted keys made the same table, extending an inline table or a static array are all errors, with line and column.
- Integers are 64-bit signed (hex/octal/binary and `_` separators accepted); `inf` and `nan` read as non-finite floats.
- Dates and times have no `Value` form: they read as RFC 3339 text (`1979-05-27T07:32:00Z`; local dates and times keep their shorter form) and an offset datetime reads straight into `quench_core::time::Time`.
- Writing: simple values first, then `[tables]`, then `[[arrays of tables]]`; a table that holds only sub-tables gets no header of its own; `None` / `null` table entries are omitted (a null inside an array is an error). `to_string_pretty` puts array elements one per line and uses `"""` for text with line breaks.
- Parsed and written output agree with the `toml` crate on a spec corpus, 300k mutated documents and 30k random trees (both writer modes read back by both parsers). Format-preserving editing (`toml_edit`) is not provided.

## Serialize and Deserialize

```rust
pub trait Serialize { fn serialize(&self) -> Value; }            // cannot fail
pub trait Deserialize: Sized { fn deserialize(v: &Value) -> Result<Self>; }
```

`DeserializeOwned` is implied by `Deserialize` (everything here owns its data), so existing `T: DeserializeOwned` bounds keep working. Implemented for: `bool`, all integer widths (range-checked), `f32`/`f64` (integers are accepted for floats), `String`/`&str`/`char`, `()`, `Option`, `Vec`/arrays/`VecDeque`/sets, `HashMap`/`BTreeMap` (keys via `ToKey`/`FromKey`: strings, integers, `bool`, `char`, `Uuid`), tuples up to 6, `Box`/`Rc`/`Arc`/`Cow`, `PathBuf`, `std::time::Duration` (`{"secs","nanos"}`), and from `quench-core` `Time` (RFC 3339 string) and `Uuid`. Their JSON matches what `chrono`, `uuid` and `serde` produce today.

- `Deserialize::missing_field()` is how a struct field behaves when its key is absent: `Option<T>` defaults to `None`, everything else is required.
- Errors say what was expected and found (`invalid type: expected a string, found number`) and where: `user.tags[2]: ...`.
- Typed JSON: `json::from_str::<T>`, `from_slice`, `to_string`, `to_string_pretty`, `to_vec`; `to_value` / `from_value` for the in-memory form.

### `json!`

`json!({"name": user, "tags": ["a", "b"], "n": 1 + 2, (computed_key): null})` builds a `Value`; any `Serialize` expression can be interpolated, and the object keeps the written order. 

## Derives

`#[derive(Serialize, Deserialize)]` (from `quench-serde-macros`, re-exported here) with the same `#[serde(..)]` helper attribute, so types annotated for serde need no changes:

- container: `rename_all` (`lowercase`, `UPPERCASE`, `PascalCase`, `camelCase`, `snake_case`, `SCREAMING_SNAKE_CASE`, `kebab-case`, `SCREAMING-KEBAB-CASE`), `deny_unknown_fields`, `default` / `default = "path"`, `tag = ".."` (internally tagged enums), `untagged`;
- variant: `rename`, `alias`, `rename_all` (for the variant's fields);
- field: `rename`, `alias` (repeatable), `default` / `default = "path"`, `skip`, `skip_serializing`, `skip_deserializing`, `skip_serializing_if = "path"`, `flatten`. A flattened `Option<T>` is `None` when the leftover keys do not make a `T`, as in serde.

Anything else (`with`, `serialize_with`, `bound`, ...) is a compile error that names what is supported, never a silent no-op. Enums are externally tagged by default (`"Unit"`, `{"Newtype": ..}`, `{"Struct": {..}}`); a tuple variant cannot be internally tagged. Generic parameters get a `Serialize`/`Deserialize` bound; deserialising into borrowed data (`&str`) is not supported. Errors carry the path (`items[1].count: ..`, `Struct.a: ..`).

## Errors

One `Error` for every codec: a message, an optional line/column (for parse errors) and a data path (`in_field`, `in_index`).

## Testing

Unit tests next to the code; parsing was cross-checked against `serde_json` on 200k generated and mutated documents (accept/reject decisions identical).

[Home](../README.md)

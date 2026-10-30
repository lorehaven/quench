---
title: API - types
parent: quench-pg
grand_parent: Crates
nav_order: 6
---

Module `quench_pg::types` (`src/types.rs`), with `Column`, `FromSql`, `PgArray`, `ToSql` also re-exported at the crate root.

Everything is text format. Parameters are sent with type OID 0 so the server infers their type from the statement. Results are converted by the column's OID.

## `mod oid`

`pub mod oid` with `pub const ...: u32` for the type OIDs from `pg_type` that the client knows:

| Const | OID | | Const | OID |
|---|---|---|---|---|
| `BOOL` | 16 | | `FLOAT8` | 701 |
| `BYTEA` | 17 | | `BPCHAR` | 1042 |
| `CHAR` | 18 | | `VARCHAR` | 1043 |
| `NAME` | 19 | | `DATE` | 1082 |
| `INT8` | 20 | | `TIME` | 1083 |
| `INT2` | 21 | | `TIMESTAMP` | 1114 |
| `INT4` | 23 | | `TIMESTAMPTZ` | 1184 |
| `TEXT` | 25 | | `INTERVAL` | 1186 |
| `OID` | 26 | | `NUMERIC` | 1700 |
| `XID` | 28 | | `UUID` | 2950 |
| `JSON` | 114 | | `JSONB` | 3802 |
| `FLOAT4` | 700 | | | |

## `struct Column`

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Column {
    pub name: String,
    pub type_oid: u32,
    pub table_oid: u32,
    pub column_id: i16,
    pub type_modifier: i32,
}
```

From the `RowDescription`.

## Conversion of result columns: `decode_text`

```rust
#[must_use] pub fn decode_text(type_oid: u32, text: &str) -> Value
```

| Type OID | Result `Value` | Fallback when it does not parse |
|---|---|---|
| `BOOL` | `Bool(text == "t")` | |
| `INT2`, `INT4`, `INT8` | integer (`Value::from_i64`) | `String(text)` |
| `OID`, `XID` | `UInt` | `String` |
| `FLOAT4`, `FLOAT8` | `Float` if finite | `String` (so `NaN`, `Infinity` stay text) |
| `NUMERIC` | integer if it parses as `i64`, else `Float` if finite | `String` (big or non-finite numerics stay text) |
| `JSON`, `JSONB` | parsed with `quench_serde::json::parse` | `String` |
| `BYTEA` | `Array` of `UInt` bytes (hex output format `\x..`) | `String` |
| `TIMESTAMPTZ` | `String` in RFC 3339 UTC (`Time::to_rfc3339`) | `String(text)` |
| `TIMESTAMP` | same, text taken as UTC | `String` |
| arrays of `BOOL`, `BYTEA`, `CHAR`, `NAME`, `INT2`, `INT4`, `INT8`, `TEXT`, `BPCHAR`, `VARCHAR`, `FLOAT4`, `FLOAT8`, `OID`, `TIMESTAMP`, `DATE`, `TIME`, `TIMESTAMPTZ`, `NUMERIC`, `JSON`, `UUID`, `JSONB` | `Array` of decoded elements (nested arrays nest) | `String` |
| anything else (`text`, `varchar`, `uuid`, `date`, `time`, `interval`, `char`, `name`, enums, ...) | `String(text)` | |

Array element OIDs by array OID: 1000 bool, 1001 bytea, 1002 char, 1003 name, 1005 int2, 1007 int4, 1009 text, 1014 bpchar, 1015 varchar, 1016 int8, 1021 float4, 1022 float8, 1028 oid, 1115 timestamp, 1182 date, 1183 time, 1185 timestamptz, 1231 numeric, 199 json, 2951 uuid, 3807 jsonb. Array types not listed (for example `interval[]`, `int4range`) come back as the raw text `String`.

## Helpers

```rust
#[must_use] pub fn decode_bytea(text: &str) -> Option<Vec<u8>>
#[must_use] pub fn encode_bytea(bytes: &[u8]) -> String
#[must_use] pub fn parse_timestamp(text: &str, naive: bool) -> Option<Time>
#[must_use] pub fn parse_array(text: &str, element_oid: u32) -> Option<Value>
```

* `decode_bytea`: `\xDEADBEEF` to bytes; `None` without the `\x` prefix or on bad hex.
* `encode_bytea`: bytes to `\x..` (lowercase hex).
* `parse_timestamp`: parses `2026-10-23 01:38:00.123456+02` (date, space, clock, optional fraction of up to 9 digits, optional offset `+hh[:mm[:ss]]`/`-...`). With `naive = false` an offset is required; with `naive = true` a missing offset means UTC. Returns `None` for `BC` dates, `infinity`, an invalid date, or any malformed text. Result is a `quench_core::time::Time` normalised to UTC.
* `parse_array`: parses `{1,2,3}`, `{"a b",NULL,"say \"hi\""}`, `{{1,2},{3,4}}`, skipping an optional `[1:3]=` bounds prefix; unquoted `NULL` (any case) is `Value::Null`; elements are decoded with `decode_text(element_oid, ..)`. `None` if the text is not a well-formed array or has trailing text.

## `trait ToSql`

```rust
pub trait ToSql: Sync {
    fn to_sql(&self) -> Option<String>;   // the parameter's text, None for NULL
}
```

Implementations and their text:

| Type | Text sent |
|---|---|
| `str`, `String` | the string |
| `bool` | `t` / `f` |
| `i8`, `i16`, `i32`, `i64`, `u8`, `u16`, `u32`, `u64`, `usize`, `isize` | decimal |
| `f64` | Rust `Display`; `NaN`, `Infinity`, `-Infinity` for non-finite |
| `f32` | Rust `Display` of the `f32` (shortest round-trip), `NaN`/`Infinity`/`-Infinity` for non-finite |
| `quench_core::time::Time` | `to_rfc3339()` |
| `quench_core::uuid::Uuid` | hyphenated text (`Display`) |
| `quench_serde::Value` | JSON text (`json::to_string`); for `json`/`jsonb` parameters |
| `[u8]`, `Vec<u8>` | `\x..` hex (bytea) |
| `&T` (`T: ToSql + ?Sized`), `Box<T>` (`T: ToSql + ?Sized + Send`) | as `T` |
| `Option<T>` | `None` for `None`, else as `T` |
| `PgArray<T>` (`T: ToSql`) | an array literal, see below |

There is no `ToSql` for `i128`/`u128`, `Duration`, `chrono` types, tuples or slices of non-`u8` (use `PgArray`).

## `struct PgArray<T>`

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PgArray<T>(pub Vec<T>);
```

A parameter for array columns or `= ANY($1)`. Rendered as `{"a","b"}`: each element is double-quoted with `"` and `\` backslash-escaped; a `None`/NULL element is `NULL`; an empty vector is `{}`. The server types the literal from context (`text[]`, `int4[]`, ...); the quoted form is accepted for numbers too. One-dimensional only (nest by hand-built text if you need more). There is no `FromSql` for `PgArray`; read arrays as `Value` (an `Array`) or through `deserialize` into `Vec<_>` fields.

## `trait FromSql`

```rust
pub trait FromSql: Sized {
    fn from_sql(column: &Column, text: Option<&str>) -> Result<Self>;
}
```

A Rust type readable from a column (`text` is `None` for NULL). Errors are `Error::Decode`: ``column `{name}` is NULL`` for NULL into a non-`Option`, and ``column `{name}` (type {oid}) is `{text}`, not {expected}`` for text that does not fit. Implementations:

| Type | Accepts |
|---|---|
| `String` | any non-NULL text |
| `bool` | `t`, `true`, `f`, `false` |
| `i16`, `i32`, `i64`, `u32`, `u64`, `f32`, `f64` | text parsed with Rust's `FromStr` for that type (no check of the column's OID) |
| `quench_core::time::Time` | `parse_timestamp(text, column.type_oid == oid::TIMESTAMP)` |
| `quench_core::uuid::Uuid` | `Uuid::parse(text)` |
| `Vec<u8>` | bytea hex form (`decode_bytea`) |
| `quench_serde::Value` | like `Row::value`: `Null` for NULL, else `decode_text` (never fails) |
| `Option<T>` (`T: FromSql`) | `None` for NULL, else `Some(T::from_sql(..)?)` |

There is no `FromSql` for `i8`, `u8`, `u16`, `usize`, `i128`, `Vec<T>` (other than `Vec<u8>`) or `PgArray`; read those via `Value` or `deserialize`.

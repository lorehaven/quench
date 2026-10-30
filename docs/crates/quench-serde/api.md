---
title: API - values and traits
parent: quench-serde
grand_parent: Crates
nav_order: 2
---

Root-level items of `quench_serde` (`src/lib.rs`): `error`, `Map`, `Value`, `ValueIndex`, the conversion traits and the `json!` macro. The formats are in [API - formats](api-formats.md).

## Root re-exports

```rust
pub use error::{Error, Result};
pub use map::Map;
pub use traits::{Deserialize, DeserializeOwned, FromKey, Serialize, ToKey, from_value, invalid_type, to_value};
pub use value::{Value, ValueIndex};
pub use quench_serde_macros::{Deserialize, Serialize};   // derives; same names as the traits
#[doc(hidden)] pub use private as __private;
```

Macros and traits live in different namespaces, so `use quench_serde::{Serialize, Deserialize};` imports both the trait and the derive.

## `error`

See [Errors](errors.md) for the full reference of `Error` and `Result`.

## `Value`

```rust
#[derive(Debug, Clone, PartialEq, Default)]
pub enum Value {
    #[default] Null,
    Bool(bool),
    Int(i64),
    UInt(u64),
    Float(f64),
    String(String),
    Array(Vec<Value>),
    Object(Map),
}
```

Integers are canonical: a non-negative integer is always `UInt`, a negative one `Int`, so equal numbers compare equal. A `Float` never equals an integer.

| Method | Signature | Notes |
|---|---|---|
| `from_i64` | `#[must_use] pub fn from_i64(n: i64) -> Self` | canonical integer |
| `from_f64` | `#[must_use] pub const fn from_f64(f: f64) -> Self` | NaN and infinities become `Null` |
| `is_null` / `is_bool` / `is_string` / `is_array` / `is_object` / `is_number` | `#[must_use] pub const fn is_null(&self) -> bool` etc. | |
| `as_bool` | `#[must_use] pub const fn as_bool(&self) -> Option<bool>` | |
| `as_i64` | `#[must_use] pub fn as_i64(&self) -> Option<i64>` | integer that fits `i64`; floats are not converted |
| `as_u64` | `#[must_use] pub const fn as_u64(&self) -> Option<u64>` | non-negative integer |
| `as_f64` | `#[must_use] pub const fn as_f64(&self) -> Option<f64>` | any number; integers may lose precision beyond 2^53 |
| `as_str` | `#[must_use] pub fn as_str(&self) -> Option<&str>` | |
| `as_array` / `as_array_mut` | `pub const fn as_array(&self) -> Option<&Vec<Self>>` / `as_array_mut(&mut self) -> Option<&mut Vec<Self>>` | |
| `as_object` / `as_object_mut` | `pub const fn as_object(&self) -> Option<&Map>` / `as_object_mut(&mut self) -> Option<&mut Map>` | |
| `type_name` | `#[must_use] pub const fn type_name(&self) -> &'static str` | `"null"`, `"boolean"`, `"number"`, `"string"`, `"array"`, `"object"` |
| `get` | `pub fn get<I: ValueIndex>(&self, index: I) -> Option<&Self>` | key or position |
| `get_mut` | `pub fn get_mut<I: ValueIndex>(&mut self, index: I) -> Option<&mut Self>` | |
| `take` | `#[must_use = "..."] pub fn take(&mut self) -> Self` | leaves `Null` |
| `pointer` | `#[must_use] pub fn pointer(&self, pointer: &str) -> Option<&Self>` | RFC 6901: `/user/tags/0` with `~0`, `~1`; `""` is the value itself; array tokens must be canonical decimal (no sign, no leading zeros) |

### Trait implementations

* `Index<I: ValueIndex>` / `IndexMut<I>`: `value["key"]`, `value[0]`. Reading a missing entry yields `Null` (no panic). Writing `value["key"] = ..` creates a missing key (a `Null` becomes an object first) and panics, as `serde_json` does, when the value is not an object (or an array index is out of range).
* `Display`: compact JSON; `{:#}` is pretty-printed (two-space indent).
* `From`: `i8 i16 i32 i64 isize u8 u16 u32 u64 usize f32 f64 bool () &str String &String Map Vec<T: Into<Value>> Option<T: Into<Value>>`.
* `FromIterator<T: Into<Value>>`: collects into an `Array`.
* `PartialEq` with `str`, `&str`, `String`, `bool`, `f64`, and every integer type (`i8..i64`, `u8..u64`, `isize`, `usize`), in both directions.

### `trait ValueIndex`

```rust
pub trait ValueIndex: sealed::Sealed { /* hidden methods */ }
```

Sealed. Implemented for `str`, `String`, `usize` and `&T` where `T: ValueIndex`. Strings index objects; `usize` indexes arrays.

## `Map`

```rust
#[derive(Clone, Default)]
pub struct Map { /* entries: Vec<(String, Value)>, index: HashMap<String, usize> */ }
```

An insertion-ordered string-keyed map (the `Object` of a `Value`). Order is data: a struct keeps its field order, a parsed document is not reshuffled. Equality ignores order. `Debug` prints as a map.

| Method | Signature |
|---|---|
| `new` | `#[must_use] pub fn new() -> Self` |
| `len` / `is_empty` | `#[must_use] pub const fn len(&self) -> usize` / `is_empty(&self) -> bool` |
| `get` | `#[must_use] pub fn get(&self, key: &str) -> Option<&Value>` |
| `get_mut` | `pub fn get_mut(&mut self, key: &str) -> Option<&mut Value>` |
| `contains_key` | `#[must_use] pub fn contains_key(&self, key: &str) -> bool` |
| `insert` | `pub fn insert(&mut self, key: impl Into<String>, value: Value) -> Option<Value>`; an existing key keeps its position and returns the old value |
| `remove` | `pub fn remove(&mut self, key: &str) -> Option<Value>`; keeps the order of the rest |
| `iter` | `#[must_use] pub fn iter(&self) -> impl ExactSizeIterator<Item = (&String, &Value)>` |
| `keys` | `#[must_use] pub fn keys(&self) -> impl ExactSizeIterator<Item = &String>` |
| `values` | `#[must_use] pub fn values(&self) -> impl ExactSizeIterator<Item = &Value>` |
| `values_mut` | `pub fn values_mut(&mut self) -> impl ExactSizeIterator<Item = &mut Value>` |

Traits: `FromIterator<(K: Into<String>, Value)>` (later duplicates replace the value in the first position), `IntoIterator for Map` (`(String, Value)`) and for `&Map` (`(&String, &Value)`).

## `Serialize`, `Deserialize` and friends

```rust
pub trait Serialize { fn serialize(&self) -> Value; }

pub trait Deserialize: Sized {
    fn deserialize(value: &Value) -> Result<Self>;
    #[must_use] fn missing_field() -> Option<Self> { None }
}

pub trait DeserializeOwned: Deserialize {}      // blanket: impl<T: Deserialize> DeserializeOwned for T

pub trait ToKey { fn to_key(&self) -> String; }
pub trait FromKey: Sized { fn from_key(key: &str) -> Result<Self>; }

#[must_use] pub fn invalid_type(expected: &str, found: &Value) -> Error
pub fn to_value<T: Serialize + ?Sized>(value: &T) -> Value
pub fn from_value<T: Deserialize>(value: &Value) -> Result<T>
```

* `serialize` cannot fail; `deserialize` can. No visitors, no data model.
* `missing_field()` is what a struct field of this type is when its key is absent. `None` means the key is required; `Option<T>` returns `Some(None)`.
* `DeserializeOwned` is implied by `Deserialize` (everything here owns its data).
* `invalid_type("a string", &v)` builds `invalid type: expected a string, found number` (using `Value::type_name`).
* `ToKey` / `FromKey` write and read map keys as JSON object keys.

### Implementations provided

| Type | Serialize | Deserialize | Notes |
|---|---|---|---|
| `Value`, `Map` | yes | yes | identity |
| `bool` | `Bool` | `Bool` | |
| `i8 i16 i32 i64 isize u8 u16 u32 u64 usize` | canonical integer | range-checked: `integer {value} is out of range for {type}`; other types: `invalid type: expected an integer, ...` | also `ToKey` / `FromKey` (`invalid {type} map key "{key}"`) |
| `u128`, `i128` | integer when it fits 64 bits, else an inexact `Float` | via `u64` / `i64` (so 64-bit range only) | |
| `f64`, `f32` | `Value::from_f64` | any number (integers accepted); `f32` narrows | non-finite floats serialize as `Null` |
| `str`, `String` | `String` | `String` (`Cow<'_, str>` too) | `&str` cannot be deserialized |
| `char` | one-char string | exactly one char | also `ToKey`/`FromKey` |
| `()` | `Null` | `Null` | |
| `&T`, `&mut T`, `Box<T>`, `Rc<T>`, `Arc<T>`, `Cow<T>` | delegate | `Box`, `Rc`, `Arc` (owned `T`), `Cow<str>` | |
| `Option<T>` | `Null` or inner | `Null` is `None`; absent field is `None` | |
| `[T]`, `Vec<T>`, `[T; N]`, `VecDeque<T>`, `HashSet<T>`, `BTreeSet<T>` | `Array` | `Array`; arrays check the length (`expected an array of length N, found M`); errors carry `[index]` | |
| `HashMap<K, V, S>`, `BTreeMap<K, V>` | `Object` via `ToKey` | via `FromKey` | key types with `ToKey`/`FromKey`: `String`, `str`, `&T`, integers, `bool`, `char`, `Uuid` |
| tuples of 1 to 12 elements | `Array` | `Array` of that length | |
| `PathBuf` | lossy string | string | |
| `std::time::Duration` | `{"secs": u64, "nanos": u64}` | requires both fields (`missing field `secs``), `nanos` must fit `u32` (`nanos is out of range`) | |
| `quench_core::time::Time` | RFC 3339 string | RFC 3339 string | |
| `quench_core::uuid::Uuid` | hyphenated string | string | also map key |
| `quench_core::version::{Version, VersionReq}` | string | string | |

Under a form read ([`urlencoded`](api-formats.md)) the scalar impls (`bool`, integers, floats) additionally accept text, and a lone string reads as a one-element `Vec`.

## `json!`

```rust
#[macro_export(local_inner_macros)]
macro_rules! json { ($($json:tt)+) => { ... } }
```

Builds a `Value` from JSON-looking syntax. Literals `null`, `true`, `false`, arrays and objects nest; any other expression is converted with `Serialize` (so any serialisable value can be interpolated). Object keys may be literals, identifiers-as-expressions in parentheses (`(computed): v`) and keep their written order. Trailing commas are accepted. The helper macros `json_internal!`, `json_unexpected!` and `json_expect_expr_comma!` are `#[doc(hidden)]`.

```rust
use quench_serde::json;
let user = "ada";
let value = json!({ "name": user, "tags": ["a", "b"], "active": true, "limit": null, "n": 1 + 2 });
assert_eq!(value["tags"][1], json!("b"));
```

## `__private` (not public API)

`#[doc(hidden)] pub mod private`, re-exported as `__private`. The derive-generated code calls it; it may change with the macros: `Lenient` (guard that makes scalar impls accept text), `lenient()`, `expect_object`, `expect_null`, `expect_seq`, `element`, `field`, `field_or`, `rest`, `flatten`, `flatten_into`, `check_unknown`, `unknown_variant`, `single_entry`, `tag_of`. Do not call these by hand.

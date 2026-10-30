---
title: API
parent: quench-serde-macros
grand_parent: Crates
nav_order: 1
---

Two derives are exported from `src/lib.rs`:

```rust
#[proc_macro_derive(Serialize, attributes(serde))]
pub fn derive_serialize(input: TokenStream) -> TokenStream

#[proc_macro_derive(Deserialize, attributes(serde))]
pub fn derive_deserialize(input: TokenStream) -> TokenStream
```

* `Serialize` generates `impl ::quench_serde::Serialize for T` (`fn serialize(&self) -> ::quench_serde::Value`).
* `Deserialize` generates `impl ::quench_serde::Deserialize for T` (`fn deserialize(__value: &::quench_serde::Value) -> ::quench_serde::Result<Self>`).
* Both add the derived trait as a bound on every **type** parameter (`where T: ::quench_serde::Serialize`); lifetimes get none. Unions are rejected (`unions are not supported`). The expansion lives in plain functions (`expand_serialize`, `expand_deserialize`, `ser::expand`, `de::expand`) so it can be unit-tested.

Anything not listed below is a compile error that names what is supported, never a silent no-op.

## Container attributes (`#[serde(...)]` on the struct or enum)

| Attribute | Effect |
|---|---|
| `rename_all = "rule"` | renames struct fields; on an enum renames the **variants** (not their fields) |
| `deny_unknown_fields` | deserialising fails on an unknown key: ``unknown field `k`, expected one of `a`, `b` ``. Ignored when the type has a `flatten` field. Reaches the fields of struct variants |
| `default` | missing fields are taken from `Default::default()` of the whole type |
| `default = "path"` | same, with `path()` returning `Self` |
| `tag = "name"` | internally tagged enum: `{"name": "Variant", ...fields}` |
| `untagged` | untagged enum |

`tag` and `untagged` together: the message `tag` and `untagged` cannot be combined (with backticks around each name). Rules for `rename_all`: `lowercase`, `UPPERCASE`, `PascalCase`, `camelCase`, `snake_case`, `SCREAMING_SNAKE_CASE`, `kebab-case`, `SCREAMING-KEBAB-CASE`. Any other string: ``unknown rename rule, expected one of `lowercase`, `UPPERCASE`, ...``.

Other container attribute: ``unsupported serde container attribute; supported: `rename_all`, `deny_unknown_fields`, `default`, `tag`, `untagged` ``.

## Variant attributes

| Attribute | Effect |
|---|---|
| `rename = "name"` | the variant's name in data |
| `alias = "name"` | extra accepted name (repeatable) when deserialising |
| `rename_all = "rule"` | renames the fields of this (struct) variant |

Other: ``unsupported serde variant attribute; supported: `rename`, `alias`, `rename_all` ``.

## Field attributes

| Attribute | Effect |
|---|---|
| `rename = "name"` | key in data |
| `alias = "name"` | extra accepted key (repeatable) |
| `default` / `default = "path"` | value when the key is absent: `Default::default()` or `path()` (a `fn() -> T`) |
| `skip` | neither serialised nor deserialised |
| `skip_serializing` | not serialised |
| `skip_deserializing` | not read; takes the type default (the container's `default`, else `Default::default()`) |
| `skip_serializing_if = "path"` | omitted when `path(&field)` is true |
| `flatten` | merges the field's keys into the enclosing object (serialise) and reads the remaining keys (deserialise) |

`flatten` on a tuple field: the message `flatten` needs a named field (with backticks around `flatten`). Other: ``unsupported serde field attribute; supported: `rename`, `alias`, `default`, `skip`, `skip_serializing`, `skip_deserializing`, `skip_serializing_if`, `flatten` ``. Not supported: `with`, `serialize_with`, `deserialize_with`, `bound`, `borrow`, `transparent`, `from`/`into`, etc.

Key names: raw identifiers lose their prefix (`r#type` is `type`). Without `rename`, a field gets `rename_all` of its container (struct) or of its variant (enum variant) applied to its snake_case name.

## Shapes: structs

| Rust | Data |
|---|---|
| `struct S { a, b }` | object, keys in field order |
| `struct S(T)` (newtype) | the inner value |
| `struct S(A, B)` | array of the elements; deserialising checks the length |
| `struct S;` | `null` |

A missing field is an error (``missing field `a` ``) unless it has a `default` or its type's `Deserialize::missing_field()` is `Some` (`Option<T>`). Errors from a field carry its key in the path. A flattened `Option<T>` is `None` when the leftover keys do not make a `T`. With `flatten` serialising, a non-object value is inserted under the key `value`; `Null` contributes no keys.

## Shapes: enums

| Representation | Unit variant | Newtype `V(T)` | Tuple `V(A, B)` | Struct `V { .. }` |
|---|---|---|---|---|
| external (default) | `"V"` | `{"V": inner}` | `{"V": [a, b]}` | `{"V": {..}}` |
| `tag = "t"` | `{"t": "V"}` | `{"t": "V", ...inner object fields}` | compile error | `{"t": "V", ...fields}` |
| `untagged` | `null` | inner | array | object |

* External: a string is looked up among unit variants (aliases accepted); an object must have exactly one key: `expected an object with a single key naming the variant, found {n} keys`. Anything else: `invalid type: expected a string or a single-key object, found ...`. Unknown name: ``unknown variant `X`, expected one of `A`, `B` ``. Errors inside a variant are tagged with the variant name in the path (`Struct.a: ...`).
* Internally tagged: the value must be an object; the tag field is read (`missing field `t``, or `invalid type: expected a string` at the tag); the tag key is known (not "unknown"); a tuple variant with other than one field is a compile error: ``tuple variant `V` cannot be internally tagged (tag = "t"); use a struct or newtype variant``. A unit variant's other keys are ignored.
* Untagged: variants are tried in declaration order; the first that deserialises wins; none: ``data did not match any variant of untagged enum `E` ``.
* An enum with no variants serialises with `match *self {}` and fails to deserialise: ``cannot deserialize the uninhabited enum `E` ``.

## Example

```rust
#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum Internal {
    Ping,
    Message { text: String },
    Wrapped(Inner),
}
```

(`tests/derive.rs` in `quench-serde`.)

## Pitfalls

* `&str` fields cannot be deserialised (there is no borrowed deserialisation); use `String` or `Cow<'static, str>`.
* The generated code requires `quench_serde` by that exact crate name.
* With a container `default`, the container default also applies to struct variants of an enum (it builds `Self::default()` for the fallback).

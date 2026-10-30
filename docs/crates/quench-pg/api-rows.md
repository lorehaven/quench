---
title: API - rows
parent: quench-pg
grand_parent: Crates
nav_order: 5
---

`quench_pg::{Row, RowIndex}` (`src/row.rs`).

## `trait RowIndex`

```rust
pub trait RowIndex: sealed::Sealed {
    #[doc(hidden)] fn find(&self, columns: &[Column]) -> Option<usize>;
}
```

Picks a column. Sealed: implemented for `usize` (position, must be below the column count), `str` and `String` (first column with that exact name), and `&T` for any implementor. Callers write `row.get("name")`, `row.get(&name_string)` or `row.get(&0usize)`.

## `struct Row`

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row { /* columns: Arc<Vec<Column>>, values: Vec<Option<String>> */ }
```

One result row. Values are kept as text (`None` is NULL); the column list is shared by all rows of a result. Rows are created by the client only (no public constructor).

```rust
#[must_use] pub const fn len(&self) -> usize
#[must_use] pub const fn is_empty(&self) -> bool
#[must_use] pub fn columns(&self) -> &[Column]
pub fn get<T: FromSql, I: RowIndex + ?Sized>(&self, index: &I) -> Result<T>
pub fn get_raw<I: RowIndex + ?Sized>(&self, index: &I) -> Result<Option<&str>>
pub fn is_null<I: RowIndex + ?Sized>(&self, index: &I) -> Result<bool>
pub fn value<I: RowIndex + ?Sized>(&self, index: &I) -> Result<Value>
#[must_use] pub fn to_value(&self) -> Value
pub fn deserialize<T: Deserialize>(&self) -> Result<T>
#[must_use] pub fn to_array(&self) -> Value
pub fn deserialize_tuple<T: Deserialize>(&self) -> Result<T>
```

| Method | Behaviour |
|---|---|
| `len`, `is_empty` | number of columns |
| `columns` | the [`Column`](api-types.md#struct-column) descriptions |
| `get::<T>` | `T::from_sql(column, text)`. `Error::Decode("no such column")` for an unknown name or out-of-range position; `Decode` for NULL into a non-`Option` type or text that does not fit. |
| `get_raw` | the column's raw text (`None` for NULL); `Decode("no such column")` if unknown |
| `is_null` | `get_raw(..)?.is_none()` |
| `value` | the column as a `quench_serde::Value`, converted by type OID (`decode_text`); `Value::Null` for NULL |
| `to_value` | `Value::Object` keyed by column name, each value converted as in `value`. If two columns share a name, the later wins. |
| `deserialize::<T>` | `T::deserialize(&self.to_value())`; conversion errors become `Error::Decode`. `T` is typically a struct whose fields are the column names. |
| `to_array` | `Value::Array` of the converted values in column order |
| `deserialize_tuple::<T>` | `T::deserialize(&self.to_array())`, e.g. `(String, i64)` |

`Row` is `Send + Sync` (plain owned data and an `Arc`).

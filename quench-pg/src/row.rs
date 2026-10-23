//! Result rows.

use crate::error::{Error, Result};
use crate::types::{Column, FromSql, decode_text};
use quench_serde::{Deserialize, Map, Value};
use std::sync::Arc;

/// One row of a result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    columns: Arc<Vec<Column>>,
    values: Vec<Option<String>>,
}

mod sealed {
    pub trait Sealed {}
    impl Sealed for usize {}
    impl Sealed for str {}
    impl Sealed for String {}
    impl<T: Sealed + ?Sized> Sealed for &T {}
}

/// Something that picks a column: its position or its name.
pub trait RowIndex: sealed::Sealed {
    #[doc(hidden)]
    fn find(&self, columns: &[Column]) -> Option<usize>;
}

impl RowIndex for usize {
    fn find(&self, columns: &[Column]) -> Option<usize> {
        (*self < columns.len()).then_some(*self)
    }
}

impl RowIndex for str {
    fn find(&self, columns: &[Column]) -> Option<usize> {
        columns.iter().position(|c| c.name == self)
    }
}

impl RowIndex for String {
    fn find(&self, columns: &[Column]) -> Option<usize> {
        RowIndex::find(self.as_str(), columns)
    }
}

impl<T: RowIndex + ?Sized> RowIndex for &T {
    fn find(&self, columns: &[Column]) -> Option<usize> {
        (**self).find(columns)
    }
}

impl Row {
    pub(crate) const fn new(columns: Arc<Vec<Column>>, values: Vec<Option<String>>) -> Self {
        Self { columns, values }
    }

    /// The number of columns.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.values.len()
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    #[must_use]
    pub fn columns(&self) -> &[Column] {
        &self.columns
    }

    fn position<I: RowIndex + ?Sized>(&self, index: &I) -> Result<usize> {
        index
            .find(&self.columns)
            .ok_or_else(|| Error::Decode("no such column".to_string()))
    }

    /// The column read as `T`.
    ///
    /// # Errors
    ///
    /// [`Error::Decode`] for an unknown column, a NULL read as a non-`Option` type, or a value that does not fit.
    pub fn get<T: FromSql, I: RowIndex + ?Sized>(&self, index: &I) -> Result<T> {
        let at = self.position(index)?;
        T::from_sql(&self.columns[at], self.values[at].as_deref())
    }

    /// The column's raw text, `None` for NULL.
    ///
    /// # Errors
    ///
    /// [`Error::Decode`] for an unknown column.
    pub fn get_raw<I: RowIndex + ?Sized>(&self, index: &I) -> Result<Option<&str>> {
        let at = self.position(index)?;
        Ok(self.values[at].as_deref())
    }

    /// Whether the column is NULL.
    ///
    /// # Errors
    ///
    /// [`Error::Decode`] for an unknown column.
    pub fn is_null<I: RowIndex + ?Sized>(&self, index: &I) -> Result<bool> {
        Ok(self.get_raw(index)?.is_none())
    }

    /// The column as a [`Value`] converted by its type.
    ///
    /// # Errors
    ///
    /// [`Error::Decode`] for an unknown column.
    pub fn value<I: RowIndex + ?Sized>(&self, index: &I) -> Result<Value> {
        let at = self.position(index)?;
        Ok(self.values[at].as_deref().map_or(Value::Null, |text| {
            decode_text(self.columns[at].type_oid, text)
        }))
    }

    /// The whole row as an object keyed by column name (a later column of the same name wins).
    #[must_use]
    pub fn to_value(&self) -> Value {
        let mut object = Map::new();
        for (column, text) in self.columns.iter().zip(&self.values) {
            object.insert(
                column.name.clone(),
                text.as_deref()
                    .map_or(Value::Null, |text| decode_text(column.type_oid, text)),
            );
        }
        Value::Object(object)
    }

    /// The row read into any `Deserialize` type - a struct whose fields are the column names,
    /// or a tuple/`Vec` for positional access.
    ///
    /// # Errors
    ///
    /// [`Error::Decode`] when the row does not fit `T`.
    pub fn deserialize<T: Deserialize>(&self) -> Result<T> {
        Ok(T::deserialize(&self.to_value())?)
    }

    /// The row as an array of its values in column order, for tuple targets.
    #[must_use]
    pub fn to_array(&self) -> Value {
        Value::Array(
            self.columns
                .iter()
                .zip(&self.values)
                .map(|(c, t)| {
                    t.as_deref()
                        .map_or(Value::Null, |t| decode_text(c.type_oid, t))
                })
                .collect(),
        )
    }

    /// The row read positionally, e.g. as `(String, i64)`.
    ///
    /// # Errors
    ///
    /// [`Error::Decode`] when the row does not fit `T`.
    pub fn deserialize_tuple<T: Deserialize>(&self) -> Result<T> {
        Ok(T::deserialize(&self.to_array())?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::oid;
    use quench_serde::json;

    fn column(name: &str, type_oid: u32) -> Column {
        Column {
            name: name.into(),
            type_oid,
            table_oid: 0,
            column_id: 0,
            type_modifier: -1,
        }
    }

    fn row() -> Row {
        Row::new(
            Arc::new(vec![
                column("id", oid::INT4),
                column("name", oid::TEXT),
                column("note", oid::TEXT),
                column("meta", oid::JSONB),
            ]),
            vec![
                Some("7".into()),
                Some("ann".into()),
                None,
                Some(r#"{"k": 1}"#.into()),
            ],
        )
    }

    #[test]
    fn typed_access_by_name_and_position() {
        let row = row();
        assert_eq!((row.len(), row.is_empty()), (4, false));
        assert_eq!(row.get::<i32, _>("id").unwrap(), 7);
        assert_eq!(row.get::<String, _>(&1usize).unwrap(), "ann");
        assert_eq!(row.get::<String, _>(&String::from("name")).unwrap(), "ann");
        assert_eq!(row.get::<Option<String>, _>("note").unwrap(), None);
        assert!(row.get::<String, _>("note").is_err());
        assert!(row.get::<i32, _>("missing").is_err());
        assert!(row.get::<i32, _>(&9usize).is_err());
        assert_eq!(row.get_raw("id").unwrap(), Some("7"));
        assert_eq!(row.get_raw("note").unwrap(), None);
        assert!(row.is_null("note").unwrap() && !row.is_null("id").unwrap());
        assert!(row.is_null("zzz").is_err());
        assert_eq!(row.columns().len(), 4);
    }

    #[test]
    fn values_and_whole_rows() {
        let row = row();
        assert_eq!(row.value("id").unwrap(), json!(7));
        assert_eq!(row.value("note").unwrap(), Value::Null);
        assert_eq!(row.value("meta").unwrap(), json!({"k": 1}));
        assert_eq!(
            row.to_value(),
            json!({"id": 7, "name": "ann", "note": null, "meta": {"k": 1}})
        );
        assert_eq!(row.to_array(), json!([7, "ann", null, {"k": 1}]));
        assert!(row.value("zzz").is_err());
    }

    #[derive(Debug, PartialEq, quench_serde::Deserialize)]
    struct Person {
        id: i32,
        name: String,
        note: Option<String>,
    }

    #[test]
    fn rows_deserialize_into_structs_and_tuples() {
        let row = row();
        assert_eq!(
            row.deserialize::<Person>().unwrap(),
            Person {
                id: 7,
                name: "ann".into(),
                note: None
            }
        );
        let narrow = Row::new(
            Arc::new(vec![column("a", oid::TEXT), column("b", oid::INT8)]),
            vec![Some("x".into()), Some("-4".into())],
        );
        assert_eq!(
            narrow.deserialize_tuple::<(String, i64)>().unwrap(),
            ("x".to_string(), -4)
        );
        assert!(narrow.deserialize_tuple::<(i64, i64)>().is_err());
        assert!(narrow.deserialize::<Person>().is_err());
        assert!(Row::new(Arc::new(vec![]), vec![]).is_empty());
    }
}

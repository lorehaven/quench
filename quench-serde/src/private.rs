//! Support code for the derive macros. Not part of the public API: the
//! generated code calls these, and they may change with it.

use std::cell::Cell;

use crate::{Deserialize, Error, Map, Result, Value, invalid_type};

thread_local! {
    static LENIENT: Cell<bool> = const { Cell::new(false) };
}

/// While alive, scalar `Deserialize` impls accept numeric and boolean text.
///
/// Form and query strings carry every value as text, so reading `n=5` into a
/// `u32` has to parse. JSON stays strict: a `"5"` there is a string.
#[derive(Debug)]
pub struct Lenient(bool);

impl Lenient {
    #[must_use]
    pub fn enter() -> Self {
        Self(LENIENT.with(|flag| flag.replace(true)))
    }
}

impl Drop for Lenient {
    fn drop(&mut self) {
        LENIENT.with(|flag| flag.set(self.0));
    }
}

/// Is text accepted for numbers and booleans right now?
#[must_use]
pub fn lenient() -> bool {
    LENIENT.with(Cell::get)
}

pub fn expect_object(value: &Value) -> Result<&Map> {
    value
        .as_object()
        .ok_or_else(|| invalid_type("an object", value))
}

pub fn expect_null(value: &Value) -> Result<()> {
    if value.is_null() {
        Ok(())
    } else {
        Err(invalid_type("null", value))
    }
}

/// An array of exactly `len` elements.
pub fn expect_seq(value: &Value, len: usize) -> Result<&Vec<Value>> {
    let items = value
        .as_array()
        .ok_or_else(|| invalid_type(&format!("an array of length {len}"), value))?;
    if items.len() == len {
        Ok(items)
    } else {
        Err(Error::new(format!(
            "expected an array of length {len}, found {}",
            items.len()
        )))
    }
}

pub fn element<T: Deserialize>(items: &[Value], index: usize) -> Result<T> {
    T::deserialize(&items[index]).map_err(|e| e.in_index(index))
}

/// The first of `names` present in `object`, with the name it was found under.
fn lookup<'o>(object: &'o Map, names: &[&str]) -> Option<(&'o Value, usize)> {
    names
        .iter()
        .enumerate()
        .find_map(|(i, name)| object.get(name).map(|v| (v, i)))
}

/// A field that must be present unless its type has a `missing_field` default
/// (`Option` does).
pub fn field<T: Deserialize>(object: &Map, names: &[&str]) -> Result<T> {
    match lookup(object, names) {
        Some((value, _)) => T::deserialize(value).map_err(|e| e.in_field(names[0])),
        None => {
            T::missing_field().ok_or_else(|| Error::new(format!("missing field `{}`", names[0])))
        }
    }
}

/// A field with a fallback when the key is absent.
pub fn field_or<T: Deserialize>(
    object: &Map,
    names: &[&str],
    default: impl FnOnce() -> T,
) -> Result<T> {
    match lookup(object, names) {
        Some((value, _)) => T::deserialize(value).map_err(|e| e.in_field(names[0])),
        None => Ok(default()),
    }
}

/// The entries of `object` not named in `known`.
#[must_use]
pub fn rest(object: &Map, known: &[&str]) -> Map {
    object
        .iter()
        .filter(|(k, _)| !known.contains(&k.as_str()))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

/// A flattened field reads whatever the other fields left over.
///
/// Like serde, a flattened `Option<T>` that cannot be read from the leftovers (a required field of `T` is
/// not there) is `None` rather than an error.
pub fn flatten<T: Deserialize>(rest: &Map) -> Result<T> {
    T::deserialize(&Value::Object(rest.clone())).or_else(|error| T::missing_field().ok_or(error))
}

/// Merges a serialised flattened field into the enclosing object.
pub fn flatten_into(map: &mut Map, value: Value) {
    match value {
        Value::Object(entries) => {
            for (key, item) in entries {
                map.insert(key, item);
            }
        }
        // Nothing to merge (a unit, or an `Option::None`): contributes no keys.
        Value::Null => {}
        other => {
            map.insert("value", other);
        }
    }
}

/// `deny_unknown_fields`.
pub fn check_unknown(object: &Map, known: &[&str]) -> Result<()> {
    object
        .keys()
        .find(|k| !known.contains(&k.as_str()))
        .map_or(Ok(()), |key| {
            Err(Error::new(format!(
                "unknown field `{key}`, expected one of {}",
                quoted(known)
            )))
        })
}

pub fn unknown_variant<T>(name: &str, expected: &[&str]) -> Result<T> {
    Err(Error::new(format!(
        "unknown variant `{name}`, expected one of {}",
        quoted(expected)
    )))
}

/// The one `(variant, content)` of an externally tagged enum.
pub fn single_entry(map: &Map) -> Result<(&str, &Value)> {
    let mut entries = map.iter();
    match (entries.next(), entries.next()) {
        (Some((key, value)), None) => Ok((key, value)),
        _ => Err(Error::new(format!(
            "expected an object with a single key naming the variant, found {} keys",
            map.len()
        ))),
    }
}

/// The variant named by the tag field of an internally tagged enum.
pub fn tag_of<'o>(object: &'o Map, tag: &str) -> Result<&'o str> {
    match object.get(tag) {
        Some(Value::String(name)) => Ok(name),
        Some(other) => Err(invalid_type("a string", other).in_field(tag)),
        None => Err(Error::new(format!("missing field `{tag}`"))),
    }
}

fn quoted(names: &[&str]) -> String {
    names
        .iter()
        .map(|n| format!("`{n}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json::parse;

    fn obj(text: &str) -> Value {
        parse(text).unwrap()
    }

    #[quench_test::test]
    fn lenient_mode_nests_and_restores() {
        assert!(!lenient());
        {
            let _outer = Lenient::enter();
            assert!(lenient());
            {
                let _inner = Lenient::enter();
                assert!(lenient());
            }
            assert!(lenient());
        }
        assert!(!lenient());
    }

    #[quench_test::test]
    fn expectations() {
        assert!(expect_object(&obj("{}")).is_ok());
        assert!(
            expect_object(&obj("[]"))
                .unwrap_err()
                .to_string()
                .contains("expected an object, found array")
        );
        assert!(expect_null(&Value::Null).is_ok());
        assert!(
            expect_null(&obj("1"))
                .unwrap_err()
                .to_string()
                .contains("expected null")
        );
        assert_eq!(expect_seq(&obj("[1,2]"), 2).unwrap().len(), 2);
        assert!(
            expect_seq(&obj("[1]"), 2)
                .unwrap_err()
                .to_string()
                .contains("length 2, found 1")
        );
        assert!(
            expect_seq(&obj("{}"), 2)
                .unwrap_err()
                .to_string()
                .contains("array of length 2")
        );
        assert_eq!(element::<u8>(&[Value::from(5)], 0).unwrap(), 5);
        assert_eq!(
            element::<u8>(&[Value::from("x")], 0).unwrap_err().path(),
            "[0]"
        );
    }

    #[quench_test::test]
    fn fields_honour_aliases_defaults_and_missing() {
        let o = obj(r#"{"old":1,"n":"x"}"#);
        let map = o.as_object().unwrap();
        assert_eq!(field::<u8>(map, &["new", "old"]).unwrap(), 1);
        assert_eq!(field::<Option<u8>>(map, &["absent"]).unwrap(), None);
        assert_eq!(
            field::<u8>(map, &["absent", "gone"])
                .unwrap_err()
                .to_string(),
            "missing field `absent`"
        );
        assert_eq!(field::<u8>(map, &["n"]).unwrap_err().path(), "n");
        assert_eq!(field_or::<u8>(map, &["absent"], || 9).unwrap(), 9);
        assert_eq!(field_or::<u8>(map, &["old"], || 9).unwrap(), 1);
        assert_eq!(field_or::<u8>(map, &["n"], || 9).unwrap_err().path(), "n");
    }

    #[quench_test::test]
    fn rest_and_flatten() {
        let o = obj(r#"{"a":1,"b":2,"c":3}"#);
        let map = o.as_object().unwrap();
        let left = rest(map, &["a"]);
        assert_eq!(
            left.keys().map(String::as_str).collect::<Vec<_>>(),
            ["b", "c"]
        );
        let extra: std::collections::BTreeMap<String, u8> = flatten(&left).unwrap();
        assert_eq!(extra.len(), 2);
    }

    #[quench_test::test]
    fn flatten_into_merges_objects_only() {
        let mut map = Map::new();
        flatten_into(&mut map, obj(r#"{"x":1}"#));
        flatten_into(&mut map, Value::Null);
        assert_eq!(map.len(), 1);
        flatten_into(&mut map, Value::from(7));
        assert_eq!(map.get("value"), Some(&Value::from(7)));
    }

    #[quench_test::test]
    fn unknown_fields_and_variants() {
        let o = obj(r#"{"a":1,"zzz":2}"#);
        let map = o.as_object().unwrap();
        assert!(check_unknown(map, &["a", "zzz"]).is_ok());
        let e = check_unknown(map, &["a"]).unwrap_err().to_string();
        assert_eq!(e, "unknown field `zzz`, expected one of `a`");
        let e = unknown_variant::<()>("X", &["A", "B"])
            .unwrap_err()
            .to_string();
        assert_eq!(e, "unknown variant `X`, expected one of `A`, `B`");
    }

    #[quench_test::test]
    fn single_entries_and_tags() {
        let one = obj(r#"{"V":1}"#);
        assert_eq!(single_entry(one.as_object().unwrap()).unwrap().0, "V");
        for bad in ["{}", r#"{"a":1,"b":2}"#] {
            assert!(
                single_entry(obj(bad).as_object().unwrap())
                    .unwrap_err()
                    .to_string()
                    .contains("single key")
            );
        }
        let tagged = obj(r#"{"t":"A"}"#);
        assert_eq!(tag_of(tagged.as_object().unwrap(), "t").unwrap(), "A");
        assert_eq!(
            tag_of(obj("{}").as_object().unwrap(), "t")
                .unwrap_err()
                .to_string(),
            "missing field `t`"
        );
        assert_eq!(
            tag_of(obj(r#"{"t":1}"#).as_object().unwrap(), "t")
                .unwrap_err()
                .path(),
            "t"
        );
    }
}

//! A dynamically typed value: what every format here parses into.

use std::fmt;
use std::ops::{Index, IndexMut};
use std::sync::LazyLock;

use crate::Map;

/// Any JSON-shaped value.
///
/// Integers are canonical: a non-negative integer is always [`Value::UInt`],
/// a negative one [`Value::Int`], so equal numbers compare equal. A
/// [`Value::Float`] never equals an integer, as in JSON libraries generally.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum Value {
    #[default]
    Null,
    Bool(bool),
    Int(i64),
    UInt(u64),
    Float(f64),
    String(String),
    Array(Vec<Self>),
    Object(Map),
}

static NULL: LazyLock<Value> = LazyLock::new(|| Value::Null);

impl Value {
    /// The canonical value for a signed integer.
    #[must_use]
    pub fn from_i64(n: i64) -> Self {
        u64::try_from(n).map_or(Self::Int(n), Self::UInt)
    }

    /// A float value; NaN and infinities have no JSON form and become `Null`.
    #[must_use]
    pub const fn from_f64(f: f64) -> Self {
        if f.is_finite() {
            Self::Float(f)
        } else {
            Self::Null
        }
    }

    #[must_use]
    pub const fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }

    #[must_use]
    pub const fn is_bool(&self) -> bool {
        matches!(self, Self::Bool(_))
    }

    #[must_use]
    pub const fn is_string(&self) -> bool {
        matches!(self, Self::String(_))
    }

    #[must_use]
    pub const fn is_array(&self) -> bool {
        matches!(self, Self::Array(_))
    }

    #[must_use]
    pub const fn is_object(&self) -> bool {
        matches!(self, Self::Object(_))
    }

    #[must_use]
    pub const fn is_number(&self) -> bool {
        matches!(self, Self::Int(_) | Self::UInt(_) | Self::Float(_))
    }

    #[must_use]
    pub const fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// An integer that fits `i64` (a float is not converted).
    #[must_use]
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Int(n) => Some(*n),
            Self::UInt(n) => i64::try_from(*n).ok(),
            _ => None,
        }
    }

    /// A non-negative integer (a float is not converted).
    #[must_use]
    pub const fn as_u64(&self) -> Option<u64> {
        match self {
            Self::UInt(n) => Some(*n),
            _ => None,
        }
    }

    /// Any number as a float (integers may lose precision beyond 2^53).
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub const fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Float(f) => Some(*f),
            Self::Int(n) => Some(*n as f64),
            Self::UInt(n) => Some(*n as f64),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(s) => Some(s),
            _ => None,
        }
    }

    #[must_use]
    pub const fn as_array(&self) -> Option<&Vec<Self>> {
        match self {
            Self::Array(a) => Some(a),
            _ => None,
        }
    }

    pub const fn as_array_mut(&mut self) -> Option<&mut Vec<Self>> {
        match self {
            Self::Array(a) => Some(a),
            _ => None,
        }
    }

    #[must_use]
    pub const fn as_object(&self) -> Option<&Map> {
        match self {
            Self::Object(o) => Some(o),
            _ => None,
        }
    }

    pub const fn as_object_mut(&mut self) -> Option<&mut Map> {
        match self {
            Self::Object(o) => Some(o),
            _ => None,
        }
    }

    /// `"null"`, `"boolean"`, `"number"`, `"string"`, `"array"` or `"object"`.
    #[must_use]
    pub const fn type_name(&self) -> &'static str {
        match self {
            Self::Null => "null",
            Self::Bool(_) => "boolean",
            Self::Int(_) | Self::UInt(_) | Self::Float(_) => "number",
            Self::String(_) => "string",
            Self::Array(_) => "array",
            Self::Object(_) => "object",
        }
    }

    /// Looks up an object key or an array position.
    pub fn get<I: ValueIndex>(&self, index: I) -> Option<&Self> {
        index.get(self)
    }

    pub fn get_mut<I: ValueIndex>(&mut self, index: I) -> Option<&mut Self> {
        index.get_mut(self)
    }

    /// Takes the value out, leaving `Null`.
    #[must_use = "the taken value is dropped otherwise"]
    pub fn take(&mut self) -> Self {
        std::mem::take(self)
    }

    /// Follows an RFC 6901 JSON pointer (`/user/tags/0`, with `~0` and `~1`
    /// escapes). The empty pointer is the value itself.
    #[must_use]
    pub fn pointer(&self, pointer: &str) -> Option<&Self> {
        if pointer.is_empty() {
            return Some(self);
        }
        let rest = pointer.strip_prefix('/')?;
        rest.split('/').try_fold(self, |value, token| {
            let token = token.replace("~1", "/").replace("~0", "~");
            match value {
                Self::Object(map) => map.get(&token),
                Self::Array(items) => {
                    // No signs or leading zeros: those are not array indexes.
                    let canonical = token == "0"
                        || (!token.starts_with('0')
                            && token.bytes().all(|b| b.is_ascii_digit())
                            && !token.is_empty());
                    if canonical {
                        token.parse::<usize>().ok().and_then(|i| items.get(i))
                    } else {
                        None
                    }
                }
                _ => None,
            }
        })
    }
}

/// Something that can pick a child out of a [`Value`].
pub trait ValueIndex: sealed::Sealed {
    #[doc(hidden)]
    fn get<'v>(&self, value: &'v Value) -> Option<&'v Value>;
    #[doc(hidden)]
    fn get_mut<'v>(&self, value: &'v mut Value) -> Option<&'v mut Value>;
    /// For `value[index] = ..`: a missing object key is created (a `Null` becomes an object first).
    #[doc(hidden)]
    fn index_or_insert<'v>(&self, value: &'v mut Value) -> &'v mut Value;
}

mod sealed {
    pub trait Sealed {}
    impl Sealed for str {}
    impl Sealed for String {}
    impl Sealed for usize {}
    impl<T: Sealed + ?Sized> Sealed for &T {}
}

impl ValueIndex for str {
    fn get<'v>(&self, value: &'v Value) -> Option<&'v Value> {
        value.as_object().and_then(|map| map.get(self))
    }

    fn get_mut<'v>(&self, value: &'v mut Value) -> Option<&'v mut Value> {
        value.as_object_mut().and_then(|map| map.get_mut(self))
    }

    fn index_or_insert<'v>(&self, value: &'v mut Value) -> &'v mut Value {
        if value.is_null() {
            *value = Value::Object(Map::new());
        }
        let kind = value.type_name();
        let Some(map) = value.as_object_mut() else {
            panic!("cannot index a {kind} with the key `{self}`");
        };
        if !map.contains_key(self) {
            map.insert(self, Value::Null);
        }
        map.get_mut(self).expect("the key was just inserted")
    }
}

impl ValueIndex for String {
    fn get<'v>(&self, value: &'v Value) -> Option<&'v Value> {
        ValueIndex::get(self.as_str(), value)
    }

    fn get_mut<'v>(&self, value: &'v mut Value) -> Option<&'v mut Value> {
        ValueIndex::get_mut(self.as_str(), value)
    }

    fn index_or_insert<'v>(&self, value: &'v mut Value) -> &'v mut Value {
        ValueIndex::index_or_insert(self.as_str(), value)
    }
}

impl ValueIndex for usize {
    fn get<'v>(&self, value: &'v Value) -> Option<&'v Value> {
        value.as_array().and_then(|items| items.get(*self))
    }

    fn get_mut<'v>(&self, value: &'v mut Value) -> Option<&'v mut Value> {
        value.as_array_mut().and_then(|items| items.get_mut(*self))
    }

    fn index_or_insert<'v>(&self, value: &'v mut Value) -> &'v mut Value {
        let kind = value.type_name();
        match value.as_array_mut() {
            Some(items) if *self < items.len() => &mut items[*self],
            Some(items) => panic!(
                "index {self} is out of bounds for an array of {}",
                items.len()
            ),
            None => panic!("cannot index a {kind} with {self}"),
        }
    }
}

impl<T: ValueIndex + ?Sized> ValueIndex for &T {
    fn get<'v>(&self, value: &'v Value) -> Option<&'v Value> {
        (**self).get(value)
    }

    fn get_mut<'v>(&self, value: &'v mut Value) -> Option<&'v mut Value> {
        (**self).get_mut(value)
    }

    fn index_or_insert<'v>(&self, value: &'v mut Value) -> &'v mut Value {
        (**self).index_or_insert(value)
    }
}

impl<I: ValueIndex> Index<I> for Value {
    type Output = Self;

    /// `value["key"]` / `value[0]`; a missing entry reads as `Null` instead of panicking.
    fn index(&self, index: I) -> &Self {
        index.get(self).unwrap_or(&NULL)
    }
}

impl<I: ValueIndex> IndexMut<I> for Value {
    /// `value["key"] = ..`: creates the key when it is missing (a `Null` value becomes an object).
    /// Panics, like `serde_json`, when the value is not an object (or the array index is out of range).
    fn index_mut(&mut self, index: I) -> &mut Self {
        index.index_or_insert(self)
    }
}

macro_rules! from_signed {
    ($($t:ty),*) => {$(
        impl From<$t> for Value {
            fn from(n: $t) -> Self {
                Self::from_i64(i64::from(n))
            }
        }
    )*};
}

macro_rules! from_unsigned {
    ($($t:ty),*) => {$(
        impl From<$t> for Value {
            fn from(n: $t) -> Self {
                Self::UInt(u64::from(n))
            }
        }
    )*};
}

from_signed!(i8, i16, i32, i64);
from_unsigned!(u8, u16, u32, u64);

impl From<isize> for Value {
    fn from(n: isize) -> Self {
        Self::from_i64(i64::try_from(n).unwrap_or(i64::MAX))
    }
}

impl From<usize> for Value {
    fn from(n: usize) -> Self {
        Self::UInt(u64::try_from(n).unwrap_or(u64::MAX))
    }
}

impl From<f32> for Value {
    fn from(f: f32) -> Self {
        Self::from_f64(f64::from(f))
    }
}

impl From<f64> for Value {
    fn from(f: f64) -> Self {
        Self::from_f64(f)
    }
}

impl From<bool> for Value {
    fn from(b: bool) -> Self {
        Self::Bool(b)
    }
}

impl From<()> for Value {
    fn from((): ()) -> Self {
        Self::Null
    }
}

impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Self::String(s.to_string())
    }
}

impl From<String> for Value {
    fn from(s: String) -> Self {
        Self::String(s)
    }
}

impl From<&String> for Value {
    fn from(s: &String) -> Self {
        Self::String(s.clone())
    }
}

impl From<Map> for Value {
    fn from(map: Map) -> Self {
        Self::Object(map)
    }
}

impl<T: Into<Self>> From<Vec<T>> for Value {
    fn from(items: Vec<T>) -> Self {
        Self::Array(items.into_iter().map(Into::into).collect())
    }
}

impl<T: Into<Self>> From<Option<T>> for Value {
    fn from(item: Option<T>) -> Self {
        item.map_or(Self::Null, Into::into)
    }
}

impl<T: Into<Self>> FromIterator<T> for Value {
    /// Collects into an array.
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        Self::Array(iter.into_iter().map(Into::into).collect())
    }
}

impl PartialEq<str> for Value {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == Some(other)
    }
}

impl PartialEq<&str> for Value {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == Some(*other)
    }
}

impl PartialEq<String> for Value {
    fn eq(&self, other: &String) -> bool {
        self.as_str() == Some(other.as_str())
    }
}

impl PartialEq<bool> for Value {
    fn eq(&self, other: &bool) -> bool {
        self.as_bool() == Some(*other)
    }
}

impl PartialEq<Value> for str {
    fn eq(&self, other: &Value) -> bool {
        other == self
    }
}

impl PartialEq<Value> for &str {
    fn eq(&self, other: &Value) -> bool {
        other == *self
    }
}

impl PartialEq<Value> for String {
    fn eq(&self, other: &Value) -> bool {
        other == self
    }
}

impl PartialEq<Value> for bool {
    fn eq(&self, other: &Value) -> bool {
        other == self
    }
}

macro_rules! eq_signed {
    ($($t:ty),*) => {$(
        impl PartialEq<$t> for Value {
            fn eq(&self, other: &$t) -> bool {
                self.as_i64() == Some(i64::from(*other))
            }
        }

        impl PartialEq<Value> for $t {
            fn eq(&self, other: &Value) -> bool {
                other == self
            }
        }
    )*};
}

macro_rules! eq_unsigned {
    ($($t:ty),*) => {$(
        impl PartialEq<$t> for Value {
            fn eq(&self, other: &$t) -> bool {
                self.as_u64() == Some(u64::from(*other))
            }
        }

        impl PartialEq<Value> for $t {
            fn eq(&self, other: &Value) -> bool {
                other == self
            }
        }
    )*};
}

// `usize` / `isize` have no lossless `From` into 64 bits, so they compare through a checked conversion.
impl PartialEq<usize> for Value {
    fn eq(&self, other: &usize) -> bool {
        u64::try_from(*other).is_ok_and(|n| self.as_u64() == Some(n))
    }
}

impl PartialEq<Value> for usize {
    fn eq(&self, other: &Value) -> bool {
        other == self
    }
}

impl PartialEq<isize> for Value {
    fn eq(&self, other: &isize) -> bool {
        i64::try_from(*other).is_ok_and(|n| self.as_i64() == Some(n))
    }
}

impl PartialEq<Value> for isize {
    fn eq(&self, other: &Value) -> bool {
        other == self
    }
}

eq_signed!(i8, i16, i32, i64);
eq_unsigned!(u8, u16, u32, u64);

impl PartialEq<f64> for Value {
    fn eq(&self, other: &f64) -> bool {
        matches!(self, Self::Float(f) if f == other)
    }
}

impl PartialEq<Value> for f64 {
    fn eq(&self, other: &Value) -> bool {
        other == self
    }
}

impl fmt::Display for Value {
    /// Compact JSON; `{:#}` for pretty-printed.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = if f.alternate() {
            crate::json::stringify_pretty(self)
        } else {
            crate::json::stringify(self)
        };
        f.write_str(&text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obj(entries: &[(&str, Value)]) -> Value {
        Value::Object(entries.iter().map(|(k, v)| (*k, v.clone())).collect())
    }

    #[quench_test::test]
    fn integers_are_canonical() {
        assert_eq!(Value::from(5_i32), Value::UInt(5));
        assert_eq!(Value::from(5_u8), Value::from(5_i64));
        assert_eq!(Value::from(-5_i8), Value::Int(-5));
        assert_eq!(Value::from(0_i64), Value::UInt(0));
        assert_eq!(Value::from(7_usize), Value::UInt(7));
        assert_eq!(Value::from(-7_isize), Value::Int(-7));
        assert_ne!(Value::from(1.0_f64), Value::UInt(1));
        assert_eq!(Value::from(u64::MAX), Value::UInt(u64::MAX));
        assert_eq!(Value::from(i64::MIN), Value::Int(i64::MIN));
    }

    #[quench_test::test]
    fn non_finite_floats_become_null() {
        assert_eq!(Value::from(f64::NAN), Value::Null);
        assert_eq!(Value::from(f64::INFINITY), Value::Null);
        assert_eq!(Value::from(f32::NEG_INFINITY), Value::Null);
        assert_eq!(Value::from(1.5_f32), Value::Float(1.5));
    }

    #[quench_test::test]
    fn conversions() {
        assert_eq!(Value::from(true), Value::Bool(true));
        assert_eq!(Value::from(()), Value::Null);
        assert_eq!(Value::from("s"), Value::String("s".into()));
        assert_eq!(Value::from(String::from("s")), Value::String("s".into()));
        assert_eq!(Value::from(&String::from("s")), Value::String("s".into()));
        assert_eq!(
            Value::from(vec![1, 2]),
            Value::Array(vec![Value::UInt(1), Value::UInt(2)])
        );
        assert_eq!(Value::from(Some(1)), Value::UInt(1));
        assert_eq!(Value::from(None::<u8>), Value::Null);
        assert_eq!(Value::from(Map::new()), Value::Object(Map::new()));
        assert_eq!((1..=3).collect::<Value>(), Value::from(vec![1, 2, 3]));
    }

    #[quench_test::test]
    fn accessors() {
        assert!(Value::Null.is_null());
        assert_eq!(Value::Bool(true).as_bool(), Some(true));
        assert_eq!(Value::from(5).as_i64(), Some(5));
        assert_eq!(Value::from(-5).as_i64(), Some(-5));
        assert_eq!(Value::UInt(u64::MAX).as_i64(), None);
        assert_eq!(Value::from(5).as_u64(), Some(5));
        assert_eq!(Value::from(-5).as_u64(), None);
        assert_eq!(Value::from(1.5).as_i64(), None);
        assert_eq!(Value::from(5).as_f64(), Some(5.0));
        assert_eq!(Value::from(-5).as_f64(), Some(-5.0));
        assert_eq!(Value::from(1.5).as_f64(), Some(1.5));
        assert_eq!(Value::from("x").as_f64(), None);
        assert_eq!(Value::from("x").as_str(), Some("x"));
        assert_eq!(Value::Null.as_str(), None);
        assert!(
            Value::from(1).is_number()
                && Value::from(-1).is_number()
                && Value::from(1.0).is_number()
        );
        assert!(!Value::from("1").is_number());
        assert!(Value::Bool(false).is_bool() && !Value::Null.is_bool());
        assert!(Value::from("s").is_string() && !Value::Null.is_string());
        assert!(Value::from(vec![1]).is_array() && !Value::Null.is_array());
        assert!(Value::Object(Map::new()).is_object() && !Value::Null.is_object());
        assert!(Value::from(vec![1]).as_array().is_some());
        assert!(Value::Null.as_array().is_none());
        assert!(Value::Null.as_object().is_none());
        assert!(Value::Null.as_bool().is_none());
    }

    #[quench_test::test]
    fn mutable_accessors() {
        let mut v = Value::from(vec![1]);
        v.as_array_mut().unwrap().push(Value::from(2));
        assert_eq!(v, Value::from(vec![1, 2]));
        let mut o = obj(&[("a", Value::from(1))]);
        o.as_object_mut().unwrap().insert("b", Value::from(2));
        assert_eq!(o["b"], Value::from(2));
        assert!(Value::Null.as_array_mut().is_none());
        assert!(Value::Null.as_object_mut().is_none());
        let taken = o.take();
        assert!(o.is_null());
        assert!(taken.as_object().is_some());
    }

    #[quench_test::test]
    fn type_names() {
        let names: Vec<_> = [
            Value::Null,
            Value::Bool(true),
            Value::Int(-1),
            Value::UInt(1),
            Value::Float(1.0),
            Value::from("s"),
            Value::Array(vec![]),
            Value::Object(Map::new()),
        ]
        .iter()
        .map(Value::type_name)
        .collect();
        assert_eq!(
            names,
            [
                "null", "boolean", "number", "number", "number", "string", "array", "object"
            ]
        );
    }

    #[quench_test::test]
    fn get_and_index() {
        let v = obj(&[("a", Value::from(vec![10, 20])), ("b", Value::from("x"))]);
        assert_eq!(v.get("b"), Some(&Value::from("x")));
        assert_eq!(v.get(String::from("b")), Some(&Value::from("x")));
        assert_eq!(v["a"][1], Value::from(20));
        assert_eq!(v["a"].get(0usize), Some(&Value::from(10)));
        assert_eq!(v.get("missing"), None);
        assert_eq!(v["missing"], Value::Null);
        assert_eq!(v["a"][9], Value::Null);
        assert_eq!(v["b"]["deeper"], Value::Null);
        let key = "b";
        assert_eq!(v.get(key), Some(&Value::from("x")));
        assert_eq!(Value::Null.get(0usize), None);
    }

    #[quench_test::test]
    fn get_mut_edits_in_place() {
        let mut v = obj(&[("a", Value::from(vec![1, 2]))]);
        *v.get_mut("a").unwrap().get_mut(1usize).unwrap() = Value::from(99);
        assert_eq!(v["a"][1], Value::from(99));
        assert!(v.get_mut("nope").is_none());
        assert!(v.get_mut(String::from("a")).is_some());
        assert!(v.get_mut(0usize).is_none());
    }

    #[quench_test::test]
    fn json_pointer() {
        let v = obj(&[
            ("a", obj(&[("b", Value::from(vec![1, 2, 3]))])),
            ("a/b", Value::from("slash")),
            ("m~n", Value::from("tilde")),
            ("", Value::from("empty")),
        ]);
        assert_eq!(v.pointer(""), Some(&v));
        assert_eq!(v.pointer("/a/b/1"), Some(&Value::from(2)));
        assert_eq!(v.pointer("/a~1b"), Some(&Value::from("slash")));
        assert_eq!(v.pointer("/m~0n"), Some(&Value::from("tilde")));
        assert_eq!(v.pointer("/"), Some(&Value::from("empty")));
        assert_eq!(v.pointer("a"), None);
        assert_eq!(v.pointer("/a/b/3"), None);
        assert_eq!(v.pointer("/a/b/01"), None);
        assert_eq!(v.pointer("/a/b/-1"), None);
        assert_eq!(v.pointer("/a/b/+1"), None);
        assert_eq!(v.pointer("/a/b/"), None);
        assert_eq!(v.pointer("/a/b/0"), Some(&Value::from(1)));
        assert_eq!(v.pointer("/a/x"), None);
        assert_eq!(v.pointer("/a/b/0/deeper"), None);
    }

    #[quench_test::test]
    fn display_is_json() {
        let v = obj(&[("a", Value::from(vec![1, 2]))]);
        assert_eq!(v.to_string(), r#"{"a":[1,2]}"#);
        assert_eq!(format!("{v:#}"), "{\n  \"a\": [\n    1,\n    2\n  ]\n}");
    }

    #[quench_test::test]
    #[allow(clippy::float_cmp)]
    fn compares_with_plain_values_both_ways() {
        let v = obj(&[
            ("s", Value::from("x")),
            ("n", Value::from(5)),
            ("neg", Value::from(-5)),
            ("b", Value::Bool(true)),
            ("f", Value::Float(1.5)),
        ]);
        assert_eq!(v["s"], "x");
        assert_eq!(v["s"], *"x");
        assert_eq!(v["s"], String::from("x"));
        assert_eq!("x", v["s"]);
        assert_eq!(String::from("x"), v["s"]);
        assert_eq!(*"x", v["s"]);
        assert_ne!(v["s"], "y");
        assert_ne!(v["n"], "5");
        assert_eq!(v["n"], 5);
        assert_eq!(v["n"], 5_u8);
        assert_eq!(5_u64, v["n"]);
        assert_eq!(v["neg"], -5);
        assert_eq!(-5_i64, v["neg"]);
        assert_ne!(v["neg"], 5_u32);
        assert_eq!(v["b"], true);
        assert_eq!(true, v["b"]);
        assert_ne!(v["b"], false);
        assert_eq!(v["f"], 1.5);
        assert_eq!(1.5, v["f"]);
        assert_ne!(v["f"], 1);
        assert_ne!(v["missing"], "x");
    }

    #[quench_test::test]
    fn default_is_null() {
        assert_eq!(Value::default(), Value::Null);
    }

    #[quench_test::test]
    fn assigning_through_an_index_creates_keys() {
        let mut v = Value::Null;
        v["a"] = Value::from(1);
        v["b"] = Value::from("x");
        v["a"] = Value::from(2);
        assert_eq!(v, crate::json!({"a": 2, "b": "x"}));
        let mut nested = crate::json!({"list": [1, 2]});
        nested["list"][1] = Value::from(9);
        nested[String::from("k")] = Value::Bool(true);
        assert_eq!(nested, crate::json!({"list": [1, 9], "k": true}));
    }

    #[quench_test::test]
    #[should_panic(expected = "cannot index a string with the key `k`")]
    fn assigning_a_key_into_a_scalar_panics() {
        let mut v = Value::from("s");
        v["k"] = Value::Null;
    }

    #[quench_test::test]
    #[should_panic(expected = "out of bounds")]
    fn assigning_past_the_end_of_an_array_panics() {
        let mut v = crate::json!([1]);
        v[3] = Value::Null;
    }

    #[quench_test::test]
    #[should_panic(expected = "cannot index a number with 0")]
    fn assigning_an_index_into_a_scalar_panics() {
        let mut v = Value::from(1);
        v[0] = Value::Null;
    }

    #[quench_test::test]
    fn values_compare_with_pointer_sized_integers() {
        let items = [1, 2, 3];
        assert_eq!(Value::from(3u64), items.len());
        assert_eq!(items.len(), Value::from(3u64));
        assert_eq!(Value::from(-4i64), -4isize);
        assert_eq!(-4isize, Value::from(-4i64));
        assert_ne!(Value::from("3"), items.len());
        assert_ne!(Value::from(-1i64), items.len());
    }
}

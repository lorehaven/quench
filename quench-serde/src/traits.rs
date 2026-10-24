//! `Serialize` and `Deserialize`: converting types to and from a [`Value`].
//!
//! No visitors, no data-model: a type turns itself into a `Value` (which can
//! never fail) and reads itself out of one (which can). The formats in this
//! crate only deal in `Value`.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::hash::{BuildHasher, Hash};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use quench_core::time::Time;
use quench_core::uuid::Uuid;
use quench_core::version::{Version, VersionReq};

use crate::{Error, Map, Result, Value};

/// Types that can describe themselves as a [`Value`].
pub trait Serialize {
    fn serialize(&self) -> Value;
}

/// Types that can be read out of a [`Value`].
pub trait Deserialize: Sized {
    fn deserialize(value: &Value) -> Result<Self>;

    /// What a struct field of this type is when its key is absent. `None`
    /// means the key is required; `Option<T>` overrides this to default to
    /// `None`.
    #[must_use]
    fn missing_field() -> Option<Self> {
        None
    }
}

/// `Deserialize` for owned data: what generic code bounds on when it must not
/// borrow from its input. (Everything here owns, so it is `Deserialize`.)
pub trait DeserializeOwned: Deserialize {}

impl<T: Deserialize> DeserializeOwned for T {}

/// How a map key is written as a JSON object key.
pub trait ToKey {
    fn to_key(&self) -> String;
}

/// How a map key is read back from a JSON object key.
pub trait FromKey: Sized {
    fn from_key(key: &str) -> Result<Self>;
}

/// `expected a string, found number`.
#[must_use]
pub fn invalid_type(expected: &str, found: &Value) -> Error {
    Error::new(format!(
        "invalid type: expected {expected}, found {}",
        found.type_name()
    ))
}

/// Converts any serialisable value.
pub fn to_value<T: Serialize + ?Sized>(value: &T) -> Value {
    value.serialize()
}

/// Reads a `T` out of a value.
pub fn from_value<T: Deserialize>(value: &Value) -> Result<T> {
    T::deserialize(value)
}

impl Serialize for Value {
    fn serialize(&self) -> Value {
        self.clone()
    }
}

impl Deserialize for Value {
    fn deserialize(value: &Value) -> Result<Self> {
        Ok(value.clone())
    }
}

impl Serialize for Map {
    fn serialize(&self) -> Value {
        Value::Object(self.clone())
    }
}

impl Deserialize for Map {
    fn deserialize(value: &Value) -> Result<Self> {
        value
            .as_object()
            .cloned()
            .ok_or_else(|| invalid_type("an object", value))
    }
}

impl Serialize for bool {
    fn serialize(&self) -> Value {
        Value::Bool(*self)
    }
}

impl Deserialize for bool {
    fn deserialize(value: &Value) -> Result<Self> {
        if let (Value::String(text), true) = (value, crate::private::lenient()) {
            return text
                .parse()
                .map_err(|_| Error::new(format!("invalid boolean {text:?}")));
        }
        value
            .as_bool()
            .ok_or_else(|| invalid_type("a boolean", value))
    }
}

macro_rules! integers {
    ($($t:ty),*) => {$(
        impl Serialize for $t {
            fn serialize(&self) -> Value {
                Value::from(*self)
            }
        }

        impl Deserialize for $t {
            fn deserialize(value: &Value) -> Result<Self> {
                let converted = match value {
                    Value::UInt(n) => Self::try_from(*n).ok(),
                    Value::Int(n) => Self::try_from(*n).ok(),
                    Value::String(text) if crate::private::lenient() => {
                        return text.parse().map_err(|_| Error::new(format!("invalid integer {text:?} for {}", stringify!($t))));
                    }
                    other => return Err(invalid_type("an integer", other)),
                };
                converted.ok_or_else(|| Error::new(format!("integer {value} is out of range for {}", stringify!($t))))
            }
        }

        impl ToKey for $t {
            fn to_key(&self) -> String {
                self.to_string()
            }
        }

        impl FromKey for $t {
            fn from_key(key: &str) -> Result<Self> {
                key.parse().map_err(|_| Error::new(format!("invalid {} map key {key:?}", stringify!($t))))
            }
        }
    )*};
}

integers!(i8, i16, i32, i64, isize, u8, u16, u32, u64, usize);

impl Serialize for f64 {
    fn serialize(&self) -> Value {
        Value::from_f64(*self)
    }
}

impl Deserialize for f64 {
    fn deserialize(value: &Value) -> Result<Self> {
        if let (Value::String(text), true) = (value, crate::private::lenient()) {
            return text
                .parse()
                .map_err(|_| Error::new(format!("invalid number {text:?}")));
        }
        value
            .as_f64()
            .ok_or_else(|| invalid_type("a number", value))
    }
}

impl Serialize for f32 {
    fn serialize(&self) -> Value {
        Value::from(*self)
    }
}

impl Deserialize for f32 {
    #[allow(clippy::cast_possible_truncation)]
    fn deserialize(value: &Value) -> Result<Self> {
        f64::deserialize(value).map(|f| f as Self)
    }
}

impl Serialize for str {
    fn serialize(&self) -> Value {
        Value::String(self.to_string())
    }
}

impl Serialize for String {
    fn serialize(&self) -> Value {
        Value::String(self.clone())
    }
}

impl Deserialize for String {
    fn deserialize(value: &Value) -> Result<Self> {
        value
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| invalid_type("a string", value))
    }
}

impl Serialize for char {
    fn serialize(&self) -> Value {
        Value::String(self.to_string())
    }
}

impl Deserialize for char {
    fn deserialize(value: &Value) -> Result<Self> {
        let text = value
            .as_str()
            .ok_or_else(|| invalid_type("a single character", value))?;
        let mut chars = text.chars();
        match (chars.next(), chars.next()) {
            (Some(c), None) => Ok(c),
            _ => Err(Error::new(format!(
                "expected a single character, found {text:?}"
            ))),
        }
    }
}

impl Serialize for () {
    fn serialize(&self) -> Value {
        Value::Null
    }
}

impl Deserialize for () {
    fn deserialize(value: &Value) -> Result<Self> {
        if value.is_null() {
            Ok(())
        } else {
            Err(invalid_type("null", value))
        }
    }
}

impl<T: Serialize + ?Sized> Serialize for &T {
    fn serialize(&self) -> Value {
        (**self).serialize()
    }
}

impl<T: Serialize + ?Sized> Serialize for &mut T {
    fn serialize(&self) -> Value {
        (**self).serialize()
    }
}

impl<T: Serialize + ?Sized> Serialize for Box<T> {
    fn serialize(&self) -> Value {
        (**self).serialize()
    }
}

impl<T: Deserialize> Deserialize for Box<T> {
    fn deserialize(value: &Value) -> Result<Self> {
        T::deserialize(value).map(Self::new)
    }

    fn missing_field() -> Option<Self> {
        T::missing_field().map(Self::new)
    }
}

impl<T: Serialize + ?Sized> Serialize for Rc<T> {
    fn serialize(&self) -> Value {
        (**self).serialize()
    }
}

impl<T: Deserialize> Deserialize for Rc<T> {
    fn deserialize(value: &Value) -> Result<Self> {
        T::deserialize(value).map(Self::new)
    }
}

impl<T: Serialize + ?Sized> Serialize for Arc<T> {
    fn serialize(&self) -> Value {
        (**self).serialize()
    }
}

impl<T: Deserialize> Deserialize for Arc<T> {
    fn deserialize(value: &Value) -> Result<Self> {
        T::deserialize(value).map(Self::new)
    }
}

impl<T: Serialize + ToOwned + ?Sized> Serialize for Cow<'_, T> {
    fn serialize(&self) -> Value {
        (**self).serialize()
    }
}

impl Deserialize for Cow<'_, str> {
    fn deserialize(value: &Value) -> Result<Self> {
        String::deserialize(value).map(Cow::Owned)
    }
}

impl<T: Serialize> Serialize for Option<T> {
    fn serialize(&self) -> Value {
        self.as_ref().map_or(Value::Null, Serialize::serialize)
    }
}

impl<T: Deserialize> Deserialize for Option<T> {
    fn deserialize(value: &Value) -> Result<Self> {
        if value.is_null() {
            Ok(None)
        } else {
            T::deserialize(value).map(Some)
        }
    }

    fn missing_field() -> Option<Self> {
        Some(None)
    }
}

fn array<'v>(value: &'v Value, expected: &str) -> Result<&'v Vec<Value>> {
    value
        .as_array()
        .ok_or_else(|| invalid_type(expected, value))
}

fn elements<T: Deserialize>(items: &[Value]) -> Result<Vec<T>> {
    items
        .iter()
        .enumerate()
        .map(|(i, item)| T::deserialize(item).map_err(|e| e.in_index(i)))
        .collect()
}

impl<T: Serialize> Serialize for [T] {
    fn serialize(&self) -> Value {
        Value::Array(self.iter().map(Serialize::serialize).collect())
    }
}

impl<T: Serialize> Serialize for Vec<T> {
    fn serialize(&self) -> Value {
        self.as_slice().serialize()
    }
}

impl<T: Deserialize> Deserialize for Vec<T> {
    fn deserialize(value: &Value) -> Result<Self> {
        // A form has no way to write a one-element list: `tags=a` is `["a"]`.
        if let (Value::String(_), true) = (value, crate::private::lenient()) {
            return T::deserialize(value)
                .map(|only| vec![only])
                .map_err(|e| e.in_index(0));
        }
        elements(array(value, "an array")?)
    }
}

impl<T: Serialize, const N: usize> Serialize for [T; N] {
    fn serialize(&self) -> Value {
        self.as_slice().serialize()
    }
}

impl<T: Deserialize, const N: usize> Deserialize for [T; N] {
    fn deserialize(value: &Value) -> Result<Self> {
        let items = elements::<T>(array(value, "an array")?)?;
        let len = items.len();
        items
            .try_into()
            .map_err(|_| Error::new(format!("expected an array of length {N}, found {len}")))
    }
}

impl<T: Serialize> Serialize for VecDeque<T> {
    fn serialize(&self) -> Value {
        Value::Array(self.iter().map(Serialize::serialize).collect())
    }
}

impl<T: Deserialize> Deserialize for VecDeque<T> {
    fn deserialize(value: &Value) -> Result<Self> {
        Vec::<T>::deserialize(value).map(Self::from)
    }
}

impl<T: Serialize, S> Serialize for HashSet<T, S> {
    fn serialize(&self) -> Value {
        Value::Array(self.iter().map(Serialize::serialize).collect())
    }
}

impl<T: Deserialize + Eq + Hash, S: BuildHasher + Default> Deserialize for HashSet<T, S> {
    fn deserialize(value: &Value) -> Result<Self> {
        Vec::<T>::deserialize(value).map(|items| items.into_iter().collect())
    }
}

impl<T: Serialize> Serialize for BTreeSet<T> {
    fn serialize(&self) -> Value {
        Value::Array(self.iter().map(Serialize::serialize).collect())
    }
}

impl<T: Deserialize + Ord> Deserialize for BTreeSet<T> {
    fn deserialize(value: &Value) -> Result<Self> {
        Vec::<T>::deserialize(value).map(|items| items.into_iter().collect())
    }
}

impl ToKey for String {
    fn to_key(&self) -> String {
        self.clone()
    }
}

impl ToKey for str {
    fn to_key(&self) -> String {
        self.to_string()
    }
}

impl<T: ToKey + ?Sized> ToKey for &T {
    fn to_key(&self) -> String {
        (**self).to_key()
    }
}

impl FromKey for String {
    fn from_key(key: &str) -> Result<Self> {
        Ok(key.to_string())
    }
}

impl ToKey for char {
    fn to_key(&self) -> String {
        self.to_string()
    }
}

impl FromKey for char {
    fn from_key(key: &str) -> Result<Self> {
        Self::deserialize(&Value::String(key.to_string()))
    }
}

impl ToKey for bool {
    fn to_key(&self) -> String {
        self.to_string()
    }
}

impl FromKey for bool {
    fn from_key(key: &str) -> Result<Self> {
        key.parse()
            .map_err(|_| Error::new(format!("invalid bool map key {key:?}")))
    }
}

impl ToKey for Uuid {
    fn to_key(&self) -> String {
        self.to_string()
    }
}

impl FromKey for Uuid {
    fn from_key(key: &str) -> Result<Self> {
        Self::parse(key).map_err(Error::new)
    }
}

fn object(value: &Value) -> Result<&Map> {
    value
        .as_object()
        .ok_or_else(|| invalid_type("an object", value))
}

fn entries<K: FromKey, V: Deserialize>(value: &Value) -> Result<Vec<(K, V)>> {
    object(value)?
        .iter()
        .map(|(key, item)| {
            let parsed = K::from_key(key).map_err(|e| e.in_field(key))?;
            Ok((parsed, V::deserialize(item).map_err(|e| e.in_field(key))?))
        })
        .collect()
}

impl<K: ToKey, V: Serialize, S> Serialize for HashMap<K, V, S> {
    fn serialize(&self) -> Value {
        Value::Object(
            self.iter()
                .map(|(k, v)| (k.to_key(), v.serialize()))
                .collect(),
        )
    }
}

impl<K: FromKey + Eq + Hash, V: Deserialize, S: BuildHasher + Default> Deserialize
    for HashMap<K, V, S>
{
    fn deserialize(value: &Value) -> Result<Self> {
        entries(value).map(|pairs| pairs.into_iter().collect())
    }
}

impl<K: ToKey, V: Serialize> Serialize for BTreeMap<K, V> {
    fn serialize(&self) -> Value {
        Value::Object(
            self.iter()
                .map(|(k, v)| (k.to_key(), v.serialize()))
                .collect(),
        )
    }
}

impl<K: FromKey + Ord, V: Deserialize> Deserialize for BTreeMap<K, V> {
    fn deserialize(value: &Value) -> Result<Self> {
        entries(value).map(|pairs| pairs.into_iter().collect())
    }
}

// 128-bit integers: stored as 64-bit when they fit; a larger value is an inexact float rather than an error.
impl Serialize for u128 {
    #[allow(clippy::cast_precision_loss)]
    fn serialize(&self) -> Value {
        u64::try_from(*self).map_or(Value::Float(*self as f64), Value::UInt)
    }
}

impl Deserialize for u128 {
    fn deserialize(value: &Value) -> Result<Self> {
        u64::deserialize(value).map(Self::from)
    }
}

impl Serialize for i128 {
    #[allow(clippy::cast_precision_loss)]
    fn serialize(&self) -> Value {
        i64::try_from(*self).map_or(Value::Float(*self as f64), Value::from_i64)
    }
}

impl Deserialize for i128 {
    fn deserialize(value: &Value) -> Result<Self> {
        i64::deserialize(value).map(Self::from)
    }
}

macro_rules! tuples {
    ($(($len:expr; $($name:ident $index:tt),+)),*) => {$(
        impl<$($name: Serialize),+> Serialize for ($($name,)+) {
            fn serialize(&self) -> Value {
                Value::Array(vec![$(self.$index.serialize()),+])
            }
        }

        impl<$($name: Deserialize),+> Deserialize for ($($name,)+) {
            fn deserialize(value: &Value) -> Result<Self> {
                let items = array(value, concat!("an array of length ", $len))?;
                if items.len() != $len {
                    return Err(Error::new(format!("expected an array of length {}, found {}", $len, items.len())));
                }
                Ok(($($name::deserialize(&items[$index]).map_err(|e| e.in_index($index))?,)+))
            }
        }
    )*};
}

tuples!(
    (1; A 0),
    (2; A 0, B 1),
    (3; A 0, B 1, C 2),
    (4; A 0, B 1, C 2, D 3),
    (5; A 0, B 1, C 2, D 3, E 4),
    (6; A 0, B 1, C 2, D 3, E 4, F 5),
    (7; A 0, B 1, C 2, D 3, E 4, F 5, G 6),
    (8; A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7),
    (9; A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7, I 8),
    (10; A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7, I 8, J 9),
    (11; A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7, I 8, J 9, K 10),
    (12; A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7, I 8, J 9, K 10, L 11)
);

impl Serialize for PathBuf {
    fn serialize(&self) -> Value {
        Value::String(self.to_string_lossy().into_owned())
    }
}

impl Deserialize for PathBuf {
    fn deserialize(value: &Value) -> Result<Self> {
        String::deserialize(value).map(Self::from)
    }
}

impl Serialize for Time {
    /// An RFC 3339 string, `2026-09-29T12:00:00Z`.
    fn serialize(&self) -> Value {
        Value::String(self.to_rfc3339())
    }
}

impl Deserialize for Time {
    fn deserialize(value: &Value) -> Result<Self> {
        let text = value
            .as_str()
            .ok_or_else(|| invalid_type("an RFC 3339 string", value))?;
        Self::parse_rfc3339(text).map_err(Error::new)
    }
}

impl Serialize for Uuid {
    fn serialize(&self) -> Value {
        Value::String(self.to_string())
    }
}

impl Deserialize for Uuid {
    fn deserialize(value: &Value) -> Result<Self> {
        let text = value
            .as_str()
            .ok_or_else(|| invalid_type("a UUID string", value))?;
        Self::parse(text).map_err(Error::new)
    }
}

impl Serialize for Version {
    /// `1.2.3`, with any pre-release and build text.
    fn serialize(&self) -> Value {
        Value::String(self.to_string())
    }
}

impl Deserialize for Version {
    fn deserialize(value: &Value) -> Result<Self> {
        let text = value
            .as_str()
            .ok_or_else(|| invalid_type("a version string", value))?;
        Self::parse(text).map_err(Error::new)
    }
}

impl Serialize for VersionReq {
    fn serialize(&self) -> Value {
        Value::String(self.to_string())
    }
}

impl Deserialize for VersionReq {
    fn deserialize(value: &Value) -> Result<Self> {
        let text = value
            .as_str()
            .ok_or_else(|| invalid_type("a version requirement string", value))?;
        Self::parse(text).map_err(Error::new)
    }
}

impl Serialize for std::time::Duration {
    /// `{"secs": 5, "nanos": 0}`.
    fn serialize(&self) -> Value {
        let mut map = Map::new();
        map.insert("secs", Value::UInt(self.as_secs()));
        map.insert("nanos", Value::UInt(u64::from(self.subsec_nanos())));
        Value::Object(map)
    }
}

impl Deserialize for std::time::Duration {
    fn deserialize(value: &Value) -> Result<Self> {
        let map = object(value)?;
        let field = |name: &str| -> Result<u64> {
            let found = map
                .get(name)
                .ok_or_else(|| Error::new(format!("missing field `{name}`")))?;
            u64::deserialize(found).map_err(|e| e.in_field(name))
        };
        let nanos =
            u32::try_from(field("nanos")?).map_err(|_| Error::new("nanos is out of range"))?;
        Ok(Self::new(field("secs")?, nanos))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip<T: Serialize + Deserialize + PartialEq + std::fmt::Debug>(value: &T) {
        let back = T::deserialize(&value.serialize()).unwrap();
        assert_eq!(&back, value);
    }

    fn err<T: Deserialize>(value: impl Into<Value>) -> String {
        T::deserialize(&value.into())
            .err()
            .map(|e| e.to_string())
            .expect("should fail")
    }

    #[quench_test::test]
    fn scalars_round_trip() {
        round_trip(&true);
        round_trip(&'x');
        round_trip(&"text".to_string());
        round_trip(&());
        round_trip(&1.5_f64);
        round_trip(&2.5_f32);
        for n in [i8::MIN, 0, i8::MAX] {
            round_trip(&n);
        }
        round_trip(&i64::MIN);
        round_trip(&u64::MAX);
        round_trip(&usize::MAX);
        round_trip(&isize::MIN);
        round_trip(&u16::MAX);
        round_trip(&-5_i32);
    }

    #[quench_test::test]
    fn integer_ranges_are_checked() {
        assert!(err::<u8>(Value::from(256)).contains("out of range for u8"));
        assert!(err::<u8>(Value::from(-1)).contains("out of range"));
        assert!(err::<i8>(Value::from(128)).contains("out of range for i8"));
        assert!(err::<u64>(Value::from(-1)).contains("out of range"));
        assert!(err::<i64>(Value::UInt(u64::MAX)).contains("out of range"));
        assert_eq!(u8::deserialize(&Value::from(255)).unwrap(), 255);
        assert_eq!(i8::deserialize(&Value::from(-128)).unwrap(), -128);
    }

    #[quench_test::test]
    fn type_mismatches_name_both_types() {
        assert_eq!(
            err::<bool>(Value::from("x")),
            "invalid type: expected a boolean, found string"
        );
        assert_eq!(
            err::<String>(Value::from(1)),
            "invalid type: expected a string, found number"
        );
        assert_eq!(
            err::<i32>(Value::from(1.5)),
            "invalid type: expected an integer, found number"
        );
        assert_eq!(
            err::<f64>(Value::Null),
            "invalid type: expected a number, found null"
        );
        assert_eq!(
            err::<()>(Value::from(1)),
            "invalid type: expected null, found number"
        );
        assert_eq!(
            err::<Vec<u8>>(Value::Null),
            "invalid type: expected an array, found null"
        );
        assert_eq!(
            err::<Map>(Value::Null),
            "invalid type: expected an object, found null"
        );
        assert_eq!(
            err::<HashMap<String, u8>>(Value::from(1)),
            "invalid type: expected an object, found number"
        );
        assert!(err::<char>(Value::from(1)).contains("single character"));
    }

    #[quench_test::test]
    fn floats_accept_integers() {
        assert!((f64::deserialize(&Value::from(3)).unwrap() - 3.0).abs() < f64::EPSILON);
        assert!((f64::deserialize(&Value::from(-3)).unwrap() + 3.0).abs() < f64::EPSILON);
        assert!((f32::deserialize(&Value::from(2)).unwrap() - 2.0).abs() < f32::EPSILON);
        assert_eq!(f64::NAN.serialize(), Value::Null);
    }

    #[quench_test::test]
    fn chars_must_be_single() {
        assert!(err::<char>(Value::from("ab")).contains("single character"));
        assert!(err::<char>(Value::from("")).contains("single character"));
        assert_eq!(char::deserialize(&Value::from("é")).unwrap(), 'é');
    }

    #[quench_test::test]
    fn options_and_missing_fields() {
        round_trip(&Some(3_u8));
        round_trip(&None::<u8>);
        assert_eq!(Option::<u8>::deserialize(&Value::Null).unwrap(), None);
        assert_eq!(Option::<u8>::missing_field(), Some(None));
        assert_eq!(u8::missing_field(), None);
        assert_eq!(Box::<Option<u8>>::missing_field(), Some(Box::new(None)));
        assert_eq!(Box::<u8>::missing_field(), None);
        assert!(err::<Option<u8>>(Value::from("x")).contains("expected an integer"));
    }

    #[quench_test::test]
    fn sequences() {
        round_trip(&vec![1_u8, 2, 3]);
        round_trip(&Vec::<String>::new());
        round_trip(&[1_u8, 2, 3]);
        round_trip(&VecDeque::from(vec![1_u8, 2]));
        round_trip(&BTreeSet::from([3_u8, 1, 2]));
        round_trip(&HashSet::<u8>::from([1, 2]));
        assert_eq!([1_u8, 2].serialize(), Value::from(vec![1, 2]));
        assert_eq!(vec!["a"].serialize(), Value::from(vec!["a"]));
        assert!(err::<[u8; 3]>(Value::from(vec![1, 2])).contains("length 3, found 2"));
    }

    #[quench_test::test]
    fn sequence_errors_point_at_the_element() {
        let e = Vec::<u8>::deserialize(&Value::from(vec![Value::from(1), Value::from("x")]))
            .unwrap_err();
        assert_eq!(e.path(), "[1]");
        assert!(e.to_string().starts_with("[1]: invalid type"));
    }

    #[quench_test::test]
    fn maps() {
        let mut hash = HashMap::new();
        hash.insert("a".to_string(), 1_u8);
        hash.insert("b".to_string(), 2);
        round_trip(&hash);
        let tree: BTreeMap<u32, Vec<String>> =
            BTreeMap::from([(2, vec!["x".to_string()]), (10, vec![])]);
        round_trip(&tree);
        assert_eq!(tree.serialize().to_string(), r#"{"2":["x"],"10":[]}"#);
        let by_id: BTreeMap<Uuid, bool> = BTreeMap::from([(Uuid::new_v4(), true)]);
        round_trip(&by_id);
        round_trip(&BTreeMap::from([(true, 1_u8), (false, 2)]));
        round_trip(&BTreeMap::from([('a', 1_u8)]));
        let by_str: BTreeMap<&str, u8> = BTreeMap::from([("k", 1)]);
        assert_eq!(by_str.serialize().to_string(), r#"{"k":1}"#);
    }

    #[quench_test::test]
    fn map_errors_point_at_the_key() {
        let value = crate::json::parse(r#"{"ok":1,"bad":"x"}"#).unwrap();
        let e = HashMap::<String, u8>::deserialize(&value).unwrap_err();
        assert_eq!(e.path(), "bad");
        let e = BTreeMap::<u8, u8>::deserialize(&crate::json::parse(r#"{"x":1}"#).unwrap())
            .unwrap_err();
        assert!(e.to_string().contains("invalid u8 map key"), "{e}");
        let e = BTreeMap::<bool, u8>::deserialize(&crate::json::parse(r#"{"maybe":1}"#).unwrap())
            .unwrap_err();
        assert!(e.to_string().contains("invalid bool map key"), "{e}");
        let e = BTreeMap::<Uuid, u8>::deserialize(&crate::json::parse(r#"{"nope":1}"#).unwrap())
            .unwrap_err();
        assert!(e.to_string().contains("invalid UUID"), "{e}");
        let e = BTreeMap::<char, u8>::deserialize(&crate::json::parse(r#"{"ab":1}"#).unwrap())
            .unwrap_err();
        assert!(e.to_string().contains("single character"), "{e}");
    }

    #[quench_test::test]
    fn wide_integers_go_through_64_bits() {
        round_trip(&7_u128);
        round_trip(&-7_i128);
        assert!(matches!(u128::MAX.serialize(), Value::Float(f) if f > 3.0e38));
        assert!(matches!(i128::MIN.serialize(), Value::Float(f) if f < -1.0e38));
        assert!(
            err::<u128>(Value::from(-1)).contains("u64")
                || !err::<u128>(Value::from(-1)).is_empty()
        );
    }

    #[quench_test::test]
    fn tuples() {
        round_trip(&(1_u8,));
        round_trip(&(1_u8, "two".to_string()));
        round_trip(&(1_u8, 2_i8, 3_u16, 4_u32, 5_i64, 6_usize));
        round_trip(&(
            1_u8,
            2_i8,
            3_u16,
            4_u32,
            5_i64,
            6_usize,
            7_u8,
            8_u8,
            9_u8,
            10_u8,
            11_u8,
            "twelve".to_string(),
        ));
        assert_eq!(
            (1_u8, "a").serialize(),
            Value::Array(vec![Value::from(1), Value::from("a")])
        );
        assert!(err::<(u8, u8)>(Value::from(vec![1])).contains("length 2, found 1"));
        assert!(err::<(u8, u8)>(Value::Null).contains("expected an array of length 2"));
        let e = <(u8, u8)>::deserialize(&Value::from(vec![Value::from(1), Value::from("x")]))
            .unwrap_err();
        assert_eq!(e.path(), "[1]");
    }

    #[quench_test::test]
    fn pointers_and_wrappers_are_transparent() {
        assert_eq!(Box::new(5_u8).serialize(), Value::from(5));
        assert_eq!(Rc::new(5_u8).serialize(), Value::from(5));
        assert_eq!(Arc::new(5_u8).serialize(), Value::from(5));
        assert_eq!((&&5_u8).serialize(), Value::from(5));
        let mut n = 5_u8;
        let by_mut: &mut u8 = &mut n;
        assert_eq!(Serialize::serialize(&by_mut), Value::from(5));
        assert_eq!(Cow::Borrowed("x").serialize(), Value::from("x"));
        round_trip(&Box::new(5_u8));
        assert_eq!(*Rc::<u8>::deserialize(&Value::from(5)).unwrap(), 5);
        assert_eq!(*Arc::<u8>::deserialize(&Value::from(5)).unwrap(), 5);
        assert_eq!(Cow::<str>::deserialize(&Value::from("x")).unwrap(), "x");
        assert_eq!("str".serialize(), Value::from("str"));
    }

    #[quench_test::test]
    fn value_and_map_are_identity() {
        let v = crate::json::parse(r#"{"a":[1,{"b":null}]}"#).unwrap();
        assert_eq!(v.serialize(), v);
        assert_eq!(Value::deserialize(&v).unwrap(), v);
        let m = v.as_object().unwrap().clone();
        assert_eq!(m.serialize(), v);
        assert_eq!(Map::deserialize(&v).unwrap(), m);
    }

    #[quench_test::test]
    fn time_uuid_path_duration() {
        let t = Time::from_ymd_hms(2026, 9, 29, 12, 0, 0).unwrap();
        assert_eq!(t.serialize(), Value::from("2026-09-29T12:00:00Z"));
        round_trip(&t);
        assert_eq!(
            Time::deserialize(&Value::from("2026-09-29T14:00:00+02:00")).unwrap(),
            t
        );
        assert!(err::<Time>(Value::from("yesterday")).contains("invalid RFC 3339"));
        assert!(err::<Time>(Value::from(5)).contains("RFC 3339 string"));

        let id = Uuid::new_v4();
        assert_eq!(id.serialize(), Value::from(id.to_string()));
        round_trip(&id);
        assert!(err::<Uuid>(Value::from("zz")).contains("invalid UUID"));
        assert!(err::<Uuid>(Value::Null).contains("UUID string"));

        round_trip(&PathBuf::from("/tmp/x"));

        let d = std::time::Duration::new(5, 7);
        assert_eq!(d.serialize().to_string(), r#"{"secs":5,"nanos":7}"#);
        round_trip(&d);
        assert!(err::<std::time::Duration>(Value::Null).contains("expected an object"));
        assert!(
            err::<std::time::Duration>(crate::json::parse(r#"{"secs":1}"#).unwrap())
                .contains("missing field `nanos`")
        );
        assert!(
            err::<std::time::Duration>(
                crate::json::parse(r#"{"secs":1,"nanos":4294967296}"#).unwrap()
            )
            .contains("nanos is out of range")
        );
        assert!(
            err::<std::time::Duration>(crate::json::parse(r#"{"secs":"x","nanos":0}"#).unwrap())
                .starts_with("secs: ")
        );
    }

    #[quench_test::test]
    fn lenient_mode_parses_text_scalars_only_while_active() {
        let text = |s: &str| Value::from(s);
        assert!(u8::deserialize(&text("5")).is_err());
        assert!(bool::deserialize(&text("true")).is_err());
        assert!(f64::deserialize(&text("1.5")).is_err());
        let _lenient = crate::private::Lenient::enter();
        assert_eq!(u8::deserialize(&text("5")).unwrap(), 5);
        assert_eq!(i64::deserialize(&text("-5")).unwrap(), -5);
        assert!(bool::deserialize(&text("true")).unwrap());
        assert!((f64::deserialize(&text("1.5")).unwrap() - 1.5).abs() < f64::EPSILON);
        assert!((f32::deserialize(&text("2")).unwrap() - 2.0).abs() < f32::EPSILON);
        assert!(
            u8::deserialize(&text("300"))
                .unwrap_err()
                .to_string()
                .contains("invalid integer")
        );
        assert!(u8::deserialize(&text("x")).is_err());
        assert!(
            bool::deserialize(&text("maybe"))
                .unwrap_err()
                .to_string()
                .contains("invalid boolean")
        );
        assert!(
            f64::deserialize(&text("x"))
                .unwrap_err()
                .to_string()
                .contains("invalid number")
        );
        // non-text input is unaffected
        assert_eq!(u8::deserialize(&Value::from(5)).unwrap(), 5);
        assert!(u8::deserialize(&Value::Null).is_err());
        // a lone text value is a one-element list; a bad one points at element 0
        assert_eq!(Vec::<u8>::deserialize(&text("7")).unwrap(), vec![7]);
        assert_eq!(
            Vec::<u8>::deserialize(&text("x")).unwrap_err().path(),
            "[0]"
        );
        assert!(Vec::<u8>::deserialize(&Value::from(5)).is_err());
    }

    #[quench_test::test]
    fn versions_are_strings() {
        let v = Version::parse("1.2.3-rc.1").unwrap();
        assert_eq!(v.serialize(), Value::from("1.2.3-rc.1"));
        round_trip(&v);
        assert!(err::<Version>(Value::from("nope")).contains("invalid version"));
        assert!(err::<Version>(Value::from(1)).contains("version string"));
        let r = VersionReq::parse("^0.2, <0.5").unwrap();
        assert_eq!(r.serialize(), Value::from("^0.2, <0.5"));
        round_trip(&r);
        assert!(err::<VersionReq>(Value::from("^^")).contains("invalid version requirement"));
        assert!(err::<VersionReq>(Value::Null).contains("requirement string"));
    }

    #[quench_test::test]
    fn deserialize_owned_is_implied() {
        fn takes<T: DeserializeOwned>(value: &Value) -> T {
            T::deserialize(value).unwrap()
        }
        assert_eq!(takes::<u8>(&Value::from(3)), 3);
        assert_eq!(
            from_value::<Vec<u8>>(&Value::from(vec![1])).unwrap(),
            vec![1]
        );
        assert_eq!(to_value(&vec![1_u8]), Value::from(vec![1]));
        assert_eq!(to_value("s"), Value::from("s"));
    }
}

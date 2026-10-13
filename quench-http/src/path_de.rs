//! Serde deserializer over captured path parameters, backing
//! [`crate::extract::Path`]: a single value (`Path<String>`, `Path<u64>`)
//! reads the only capture, a tuple reads captures positionally, and a
//! struct / map reads them by name. Values are the raw (not
//! percent-decoded) path text, parsed into numbers / bools on demand.

use serde::Deserializer;
use serde::de::{self, DeserializeSeed, IntoDeserializer, MapAccess, SeqAccess, Visitor};
use std::fmt;

#[derive(Debug)]
pub(crate) struct Error(String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

impl de::Error for Error {
    fn custom<T: fmt::Display>(msg: T) -> Self {
        Error(msg.to_string())
    }
}

/// Deserializes the captures into `T`. Values are percent-decoded first (a
/// `%2f` in a capture becomes `/`, so a handler validating a name sees what
/// the client meant, not the wire form); invalid UTF-8 is an error.
pub(crate) fn from_params<T: de::DeserializeOwned>(
    params: &[(String, String)],
) -> Result<T, Error> {
    let decoded = params
        .iter()
        .map(|(name, value)| Ok((name.clone(), percent_decode(value)?)))
        .collect::<Result<Vec<_>, Error>>()?;
    T::deserialize(Params(&decoded))
}

fn percent_decode(value: &str) -> Result<String, Error> {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| (b as char).to_digit(16);
        if bytes[i] == b'%'
            && let (Some(h), Some(l)) = (
                bytes.get(i + 1).and_then(|b| hex(*b)),
                bytes.get(i + 2).and_then(|b| hex(*b)),
            )
        {
            out.push((h * 16 + l) as u8);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).map_err(|_| de::Error::custom("path parameter is not valid UTF-8"))
}

struct Params<'a>(&'a [(String, String)]);

macro_rules! forward_scalar {
    ($($method:ident)*) => {$(
        fn $method<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
            self.single()?.$method(visitor)
        }
    )*};
}

impl<'de, 'a> Deserializer<'de> for Params<'a>
where
    'a: 'de,
{
    type Error = Error;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        self.deserialize_map(visitor)
    }

    fn deserialize_map<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_map(ParamsMap {
            iter: self.0.iter(),
            value: None,
        })
    }

    fn deserialize_struct<V: Visitor<'de>>(
        self,
        _: &'static str,
        _: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error> {
        self.deserialize_map(visitor)
    }

    fn deserialize_seq<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_seq(ParamsSeq(self.0.iter()))
    }

    fn deserialize_tuple<V: Visitor<'de>>(self, len: usize, visitor: V) -> Result<V::Value, Error> {
        if self.0.len() < len {
            return Err(de::Error::custom(format!(
                "wrong number of path parameters: expected {len}, got {}",
                self.0.len()
            )));
        }
        self.deserialize_seq(visitor)
    }

    fn deserialize_tuple_struct<V: Visitor<'de>>(
        self,
        _: &'static str,
        len: usize,
        visitor: V,
    ) -> Result<V::Value, Error> {
        self.deserialize_tuple(len, visitor)
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        _: &'static str,
        visitor: V,
    ) -> Result<V::Value, Error> {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_some(self)
    }

    fn deserialize_unit<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_unit()
    }

    /// Any scalar: read the single capture.
    fn deserialize_str<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        self.single()?.deserialize_str(visitor)
    }

    fn deserialize_string<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        self.deserialize_str(visitor)
    }

    fn deserialize_enum<V: Visitor<'de>>(
        self,
        _: &'static str,
        _: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error> {
        visitor.visit_enum(self.single()?.0.into_deserializer())
    }

    fn deserialize_identifier<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        self.deserialize_str(visitor)
    }

    forward_scalar! {
        deserialize_bool deserialize_i8 deserialize_i16 deserialize_i32 deserialize_i64
        deserialize_i128 deserialize_u8 deserialize_u16 deserialize_u32 deserialize_u64
        deserialize_u128 deserialize_f32 deserialize_f64 deserialize_char
    }

    serde::forward_to_deserialize_any! {
        bytes byte_buf ignored_any unit_struct
    }
}

impl<'a> Params<'a> {
    fn single(&self) -> Result<Value<'a>, Error> {
        match self.0 {
            [(_, value)] => Ok(Value(value)),
            _ => Err(de::Error::custom(format!(
                "expected exactly one path parameter, got {}",
                self.0.len()
            ))),
        }
    }
}

/// One captured value; parses itself as whatever scalar the target asks for.
struct Value<'a>(&'a str);

macro_rules! parse_value {
    ($($method:ident => $visit:ident,)*) => {$(
        fn $method<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
            match self.0.parse() {
                Ok(v) => visitor.$visit(v),
                Err(_) => Err(de::Error::custom(format!(
                    "can not parse `{}` as {}",
                    self.0,
                    stringify!($visit).trim_start_matches("visit_")
                ))),
            }
        }
    )*};
}

impl<'de, 'a> Deserializer<'de> for Value<'a>
where
    'a: 'de,
{
    type Error = Error;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_borrowed_str(self.0)
    }

    parse_value! {
        deserialize_bool => visit_bool,
        deserialize_i8 => visit_i8,
        deserialize_i16 => visit_i16,
        deserialize_i32 => visit_i32,
        deserialize_i64 => visit_i64,
        deserialize_i128 => visit_i128,
        deserialize_u8 => visit_u8,
        deserialize_u16 => visit_u16,
        deserialize_u32 => visit_u32,
        deserialize_u64 => visit_u64,
        deserialize_u128 => visit_u128,
        deserialize_f32 => visit_f32,
        deserialize_f64 => visit_f64,
        deserialize_char => visit_char,
    }

    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_some(self)
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        _: &'static str,
        visitor: V,
    ) -> Result<V::Value, Error> {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_enum<V: Visitor<'de>>(
        self,
        _: &'static str,
        _: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error> {
        visitor.visit_enum(self.0.into_deserializer())
    }

    serde::forward_to_deserialize_any! {
        str string bytes byte_buf unit unit_struct seq tuple tuple_struct
        map struct identifier ignored_any
    }
}

struct ParamsMap<'a> {
    iter: std::slice::Iter<'a, (String, String)>,
    value: Option<&'a str>,
}

impl<'de, 'a> MapAccess<'de> for ParamsMap<'a>
where
    'a: 'de,
{
    type Error = Error;

    fn next_key_seed<K: DeserializeSeed<'de>>(
        &mut self,
        seed: K,
    ) -> Result<Option<K::Value>, Error> {
        let Some((key, value)) = self.iter.next() else {
            return Ok(None);
        };
        self.value = Some(value);
        seed.deserialize(Value(key)).map(Some)
    }

    fn next_value_seed<S: DeserializeSeed<'de>>(&mut self, seed: S) -> Result<S::Value, Error> {
        match self.value.take() {
            Some(value) => seed.deserialize(Value(value)),
            None => Err(de::Error::custom("value is missing")),
        }
    }
}

struct ParamsSeq<'a>(std::slice::Iter<'a, (String, String)>);

impl<'de, 'a> SeqAccess<'de> for ParamsSeq<'a>
where
    'a: 'de,
{
    type Error = Error;

    fn next_element_seed<T: DeserializeSeed<'de>>(
        &mut self,
        seed: T,
    ) -> Result<Option<T::Value>, Error> {
        match self.0.next() {
            Some((_, value)) => seed.deserialize(Value(value)).map(Some),
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    fn p(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn single_string_and_number() {
        assert_eq!(from_params::<String>(&p(&[("a", "x/y")])).unwrap(), "x/y");
        assert_eq!(from_params::<u64>(&p(&[("id", "42")])).unwrap(), 42);
        assert!(from_params::<u64>(&p(&[("id", "nope")])).is_err());
    }

    #[test]
    fn tuple_reads_positionally() {
        let v: (String, i64) = from_params(&p(&[("a", "n"), ("b", "-7")])).unwrap();
        assert_eq!(v, ("n".to_string(), -7));
        assert!(from_params::<(String, String)>(&p(&[("a", "n")])).is_err());
    }

    #[test]
    fn captures_are_percent_decoded() {
        assert_eq!(
            from_params::<String>(&p(&[("n", "..%2fetc%20x")])).unwrap(),
            "../etc x"
        );
        assert_eq!(from_params::<String>(&p(&[("n", "100%")])).unwrap(), "100%");
        assert!(from_params::<String>(&p(&[("n", "%ff")])).is_err());
    }

    #[test]
    fn struct_reads_by_name() {
        #[derive(Deserialize)]
        struct Blob {
            name: String,
            digest: String,
        }
        let b: Blob = from_params(&p(&[("name", "a/b"), ("digest", "sha256:1")])).unwrap();
        assert_eq!((b.name.as_str(), b.digest.as_str()), ("a/b", "sha256:1"));
    }
}

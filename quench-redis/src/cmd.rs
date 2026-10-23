//! Building commands and reading replies as Rust types.

use crate::error::{Error, Result};
use crate::resp::{self, Value};

/// A command under construction: `cmd("SET").arg("key").arg(5)`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Cmd {
    args: Vec<Vec<u8>>,
}

/// Starts a command with its name.
#[must_use]
pub fn cmd(name: &str) -> Cmd {
    Cmd {
        args: vec![name.as_bytes().to_vec()],
    }
}

/// A value that can be a command argument.
pub trait ToArg {
    fn to_arg(&self) -> Vec<u8>;
}

impl ToArg for str {
    fn to_arg(&self) -> Vec<u8> {
        self.as_bytes().to_vec()
    }
}

impl ToArg for String {
    fn to_arg(&self) -> Vec<u8> {
        self.as_bytes().to_vec()
    }
}

impl ToArg for [u8] {
    fn to_arg(&self) -> Vec<u8> {
        self.to_vec()
    }
}

impl ToArg for Vec<u8> {
    fn to_arg(&self) -> Vec<u8> {
        self.clone()
    }
}

macro_rules! to_arg_display {
    ($($ty:ty),*) => {$(
        impl ToArg for $ty {
            fn to_arg(&self) -> Vec<u8> {
                self.to_string().into_bytes()
            }
        }
    )*};
}
to_arg_display!(i8, i16, i32, i64, isize, u8, u16, u32, u64, usize, f32, f64);

impl<T: ToArg + ?Sized> ToArg for &T {
    fn to_arg(&self) -> Vec<u8> {
        (**self).to_arg()
    }
}

impl Cmd {
    /// Appends an argument.
    #[must_use]
    pub fn arg<T: ToArg + ?Sized>(mut self, value: &T) -> Self {
        self.args.push(value.to_arg());
        self
    }

    /// Appends several arguments.
    #[must_use]
    pub fn args<T: ToArg>(mut self, values: &[T]) -> Self {
        self.args.extend(values.iter().map(ToArg::to_arg));
        self
    }

    /// The command name, uppercased.
    #[must_use]
    pub fn name(&self) -> String {
        self.args
            .first()
            .map(|n| String::from_utf8_lossy(n).to_ascii_uppercase())
            .unwrap_or_default()
    }

    /// The argument at `index` (0 is the command name).
    #[must_use]
    pub fn get(&self, index: usize) -> Option<&[u8]> {
        self.args.get(index).map(Vec::as_slice)
    }

    /// The key a cluster routes this command by: the first argument after the name.
    ///
    /// Correct for the single-key commands this client is used with; commands without a key
    /// (`SCAN`, `CLUSTER`, `PING`, ...) have none that matters and are routed by the caller.
    #[must_use]
    pub fn key(&self) -> Option<&[u8]> {
        self.get(1)
    }

    pub(crate) fn encode(&self, out: &mut Vec<u8>) {
        resp::encode_command(out, &self.args);
    }
}

/// A reply readable as a Rust type.
pub trait FromValue: Sized {
    /// # Errors
    ///
    /// [`Error::Type`] when the reply does not fit.
    fn from_value(value: Value) -> Result<Self>;
}

fn mismatch<T>(expected: &str, value: &Value) -> Result<T> {
    Err(Error::Type(format!("expected {expected}, got {value:?}")))
}

impl FromValue for Value {
    fn from_value(value: Value) -> Result<Self> {
        Ok(value)
    }
}

impl FromValue for () {
    fn from_value(_: Value) -> Result<Self> {
        Ok(())
    }
}

impl FromValue for String {
    fn from_value(value: Value) -> Result<Self> {
        match value {
            Value::Bulk(bytes) => {
                Self::from_utf8(bytes).map_err(|_| Error::Type("a string that is not UTF-8".into()))
            }
            Value::Simple(text) | Value::Verbatim(text) | Value::BigNumber(text) => Ok(text),
            Value::Int(n) => Ok(n.to_string()),
            other => mismatch("a string", &other),
        }
    }
}

impl FromValue for Vec<u8> {
    fn from_value(value: Value) -> Result<Self> {
        match value {
            Value::Bulk(bytes) => Ok(bytes),
            Value::Simple(text) | Value::Verbatim(text) => Ok(text.into_bytes()),
            other => mismatch("bytes", &other),
        }
    }
}

impl FromValue for i64 {
    fn from_value(value: Value) -> Result<Self> {
        match value {
            Value::Int(n) => Ok(n),
            Value::Bulk(_) | Value::Simple(_) => {
                let text = String::from_value(value)?;
                text.parse()
                    .map_err(|_| Error::Type(format!("`{text}` is not an integer")))
            }
            other => mismatch("an integer", &other),
        }
    }
}

impl FromValue for u64 {
    fn from_value(value: Value) -> Result<Self> {
        let n = i64::from_value(value)?;
        Self::try_from(n).map_err(|_| Error::Type(format!("{n} is negative")))
    }
}

impl FromValue for f64 {
    fn from_value(value: Value) -> Result<Self> {
        match value {
            Value::Double(d) => Ok(d),
            #[allow(clippy::cast_precision_loss)]
            // Redis integers are small counters here; a float reply is the precise form
            Value::Int(n) => Ok(n as Self),
            Value::Bulk(_) | Value::Simple(_) => {
                let text = String::from_value(value)?;
                text.parse()
                    .map_err(|_| Error::Type(format!("`{text}` is not a number")))
            }
            other => mismatch("a number", &other),
        }
    }
}

impl FromValue for bool {
    fn from_value(value: Value) -> Result<Self> {
        match value {
            Value::Bool(b) => Ok(b),
            Value::Int(n) => Ok(n != 0),
            Value::Simple(ref s) if s == "OK" => Ok(true),
            other => mismatch("a boolean", &other),
        }
    }
}

impl<T: FromValue> FromValue for Option<T> {
    fn from_value(value: Value) -> Result<Self> {
        match value {
            Value::Nil => Ok(None),
            other => T::from_value(other).map(Some),
        }
    }
}

impl<T: FromValue> FromValue for Vec<T> {
    fn from_value(value: Value) -> Result<Self> {
        match value {
            Value::Array(items) | Value::Set(items) => {
                items.into_iter().map(T::from_value).collect()
            }
            Value::Nil => Ok(Self::new()),
            other => mismatch("a list", &other),
        }
    }
}

impl<A: FromValue, B: FromValue> FromValue for (A, B) {
    fn from_value(value: Value) -> Result<Self> {
        match value {
            Value::Array(items) if items.len() == 2 => {
                let mut it = items.into_iter();
                match (it.next(), it.next()) {
                    (Some(a), Some(b)) => Ok((A::from_value(a)?, B::from_value(b)?)),
                    _ => mismatch("a pair", &Value::Nil),
                }
            }
            other => mismatch("a two-element list", &other),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_encode_their_arguments() {
        let c = cmd("set")
            .arg("key")
            .arg(&5i64)
            .arg(&b"raw"[..])
            .arg(&2.5f64)
            .arg(&String::from("s"));
        let mut out = Vec::new();
        c.encode(&mut out);
        assert_eq!(
            out,
            b"*6\r\n$3\r\nset\r\n$3\r\nkey\r\n$1\r\n5\r\n$3\r\nraw\r\n$3\r\n2.5\r\n$1\r\ns\r\n"
        );
        assert_eq!(c.name(), "SET");
        assert_eq!(c.key(), Some(&b"key"[..]));
        assert_eq!(c.get(0), Some(&b"set"[..]));
        assert_eq!(cmd("PING").key(), None);
        assert_eq!(Cmd::default().name(), "");
        let many = cmd("DEL").args(&["a", "b"]).args(&[1u8, 2]);
        assert_eq!(many.get(4), Some(&b"2"[..]));
        assert_eq!(cmd("X").arg(&vec![1u8, 2]).get(1), Some(&[1u8, 2][..]));
        assert_eq!(cmd("X").arg(&&"ref").get(1), Some(&b"ref"[..]));
    }

    #[test]
    fn replies_convert() {
        assert_eq!(String::from_value(Value::Bulk(b"x".to_vec())).unwrap(), "x");
        assert_eq!(
            String::from_value(Value::Simple("OK".into())).unwrap(),
            "OK"
        );
        assert_eq!(String::from_value(Value::Int(3)).unwrap(), "3");
        assert!(String::from_value(Value::Bulk(vec![0xff])).is_err());
        assert!(String::from_value(Value::Array(vec![])).is_err());
        assert_eq!(
            Vec::<u8>::from_value(Value::Bulk(vec![1, 2])).unwrap(),
            [1, 2]
        );
        assert_eq!(
            Vec::<u8>::from_value(Value::Simple("ab".into())).unwrap(),
            b"ab"
        );
        assert!(Vec::<u8>::from_value(Value::Int(1)).is_err());
        assert_eq!(i64::from_value(Value::Int(-4)).unwrap(), -4);
        assert_eq!(i64::from_value(Value::Bulk(b"12".to_vec())).unwrap(), 12);
        assert!(i64::from_value(Value::Bulk(b"x".to_vec())).is_err());
        assert!(i64::from_value(Value::Nil).is_err());
        assert_eq!(u64::from_value(Value::Int(4)).unwrap(), 4);
        assert!(u64::from_value(Value::Int(-1)).is_err());
        assert!((f64::from_value(Value::Double(1.5)).unwrap() - 1.5).abs() < f64::EPSILON);
        assert!((f64::from_value(Value::Int(2)).unwrap() - 2.0).abs() < f64::EPSILON);
        assert!(
            (f64::from_value(Value::Bulk(b"0.25".to_vec())).unwrap() - 0.25).abs() < f64::EPSILON
        );
        assert!(f64::from_value(Value::Bulk(b"x".to_vec())).is_err());
        assert!(f64::from_value(Value::Nil).is_err());
        assert!(bool::from_value(Value::Bool(true)).unwrap());
        assert!(!bool::from_value(Value::Int(0)).unwrap());
        assert!(bool::from_value(Value::Simple("OK".into())).unwrap());
        assert!(bool::from_value(Value::Nil).is_err());
        assert_eq!(Option::<String>::from_value(Value::Nil).unwrap(), None);
        assert_eq!(
            Option::<String>::from_value(Value::Simple("a".into())).unwrap(),
            Some("a".to_string())
        );
        assert_eq!(
            Vec::<String>::from_value(Value::Array(vec![
                Value::Bulk(b"a".to_vec()),
                Value::Bulk(b"b".to_vec())
            ]))
            .unwrap(),
            ["a", "b"]
        );
        assert_eq!(
            Vec::<i64>::from_value(Value::Set(vec![Value::Int(1)])).unwrap(),
            [1]
        );
        assert!(Vec::<i64>::from_value(Value::Nil).unwrap().is_empty());
        assert!(Vec::<i64>::from_value(Value::Int(1)).is_err());
        assert_eq!(
            <(u64, Vec<String>)>::from_value(Value::Array(vec![
                Value::Bulk(b"0".to_vec()),
                Value::Array(vec![Value::Bulk(b"k".to_vec())])
            ]))
            .unwrap(),
            (0, vec!["k".to_string()])
        );
        assert!(<(u64, u64)>::from_value(Value::Array(vec![Value::Int(1)])).is_err());
        assert!(<(u64, u64)>::from_value(Value::Nil).is_err());
        assert_eq!(<()>::from_value(Value::Int(1)).unwrap(), ());
        assert_eq!(Value::from_value(Value::Int(1)).unwrap(), Value::Int(1));
    }
}

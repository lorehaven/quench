//! Values attached to events and spans (`info!(count = 42, user = %name, ..)`).

use std::fmt::{self, Debug, Display};

/// A recorded value. Numbers and booleans stay typed so JSON output can write
/// them as numbers and booleans.
#[derive(Debug, Clone, PartialEq)]
pub enum FieldValue {
    Bool(bool),
    I64(i64),
    U64(u64),
    F64(f64),
    /// A string value; text output quotes it.
    Str(String),
    /// Already-rendered text from `%value` or `?value`; text output prints it as is.
    Text(String),
    /// Declared but not recorded yet (see [`Empty`]); not printed.
    Empty,
}

impl FieldValue {
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        matches!(self, Self::Empty)
    }
}

impl fmt::Display for FieldValue {
    /// The text form: strings quoted, everything else as written.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bool(b) => write!(f, "{b}"),
            Self::I64(n) => write!(f, "{n}"),
            Self::U64(n) => write!(f, "{n}"),
            Self::F64(n) => write!(f, "{n}"),
            Self::Str(s) => write!(f, "{s:?}"),
            Self::Text(s) => f.write_str(s),
            Self::Empty => Ok(()),
        }
    }
}

/// Types a field can be set to with a plain `name = value`. Anything else
/// goes through `%value` (Display) or `?value` (Debug).
pub trait IntoFieldValue {
    fn field_value(&self) -> FieldValue;
}

macro_rules! signed {
    ($($t:ty),*) => {$(
        impl IntoFieldValue for $t {
            fn field_value(&self) -> FieldValue {
                FieldValue::I64(i64::from(*self))
            }
        }
    )*};
}

macro_rules! unsigned {
    ($($t:ty),*) => {$(
        impl IntoFieldValue for $t {
            fn field_value(&self) -> FieldValue {
                FieldValue::U64(u64::from(*self))
            }
        }
    )*};
}

signed!(i8, i16, i32, i64);
unsigned!(u8, u16, u32, u64);

impl IntoFieldValue for isize {
    fn field_value(&self) -> FieldValue {
        FieldValue::I64(i64::try_from(*self).unwrap_or(i64::MAX))
    }
}

impl IntoFieldValue for usize {
    fn field_value(&self) -> FieldValue {
        FieldValue::U64(u64::try_from(*self).unwrap_or(u64::MAX))
    }
}

// 128-bit integers (`Duration::as_millis` is one) are recorded as 64-bit, saturating.
impl IntoFieldValue for u128 {
    fn field_value(&self) -> FieldValue {
        FieldValue::U64(u64::try_from(*self).unwrap_or(u64::MAX))
    }
}

impl IntoFieldValue for i128 {
    fn field_value(&self) -> FieldValue {
        FieldValue::I64(i64::try_from(*self).unwrap_or(if *self < 0 { i64::MIN } else { i64::MAX }))
    }
}

impl IntoFieldValue for f32 {
    fn field_value(&self) -> FieldValue {
        FieldValue::F64(f64::from(*self))
    }
}

impl IntoFieldValue for f64 {
    fn field_value(&self) -> FieldValue {
        FieldValue::F64(*self)
    }
}

impl IntoFieldValue for bool {
    fn field_value(&self) -> FieldValue {
        FieldValue::Bool(*self)
    }
}

impl IntoFieldValue for str {
    fn field_value(&self) -> FieldValue {
        FieldValue::Str(self.to_string())
    }
}

impl IntoFieldValue for String {
    fn field_value(&self) -> FieldValue {
        FieldValue::Str(self.clone())
    }
}

impl<T: IntoFieldValue + ?Sized> IntoFieldValue for &T {
    fn field_value(&self) -> FieldValue {
        (**self).field_value()
    }
}

impl<T: IntoFieldValue> IntoFieldValue for Option<T> {
    /// `None` records nothing.
    fn field_value(&self) -> FieldValue {
        self.as_ref()
            .map_or(FieldValue::Empty, IntoFieldValue::field_value)
    }
}

/// A field declared up front and filled in later with `Span::record`.
#[derive(Debug, Clone, Copy)]
pub struct Empty;

impl IntoFieldValue for Empty {
    fn field_value(&self) -> FieldValue {
        FieldValue::Empty
    }
}

/// `%value`: the `Display` form.
pub fn display(value: &dyn Display) -> FieldValue {
    FieldValue::Text(value.to_string())
}

/// `?value`: the `Debug` form.
pub fn debug(value: &dyn Debug) -> FieldValue {
    FieldValue::Text(format!("{value:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_types_stay_typed() {
        assert_eq!(42_u8.field_value(), FieldValue::U64(42));
        assert_eq!((-3_i32).field_value(), FieldValue::I64(-3));
        assert_eq!(7_usize.field_value(), FieldValue::U64(7));
        assert_eq!((-7_isize).field_value(), FieldValue::I64(-7));
        assert_eq!(true.field_value(), FieldValue::Bool(true));
        assert_eq!(1.5_f32.field_value(), FieldValue::F64(1.5));
        assert_eq!(9_u128.field_value(), FieldValue::U64(9));
        assert_eq!(u128::MAX.field_value(), FieldValue::U64(u64::MAX));
        assert_eq!((-9_i128).field_value(), FieldValue::I64(-9));
        assert_eq!(i128::MAX.field_value(), FieldValue::I64(i64::MAX));
        assert_eq!(i128::MIN.field_value(), FieldValue::I64(i64::MIN));
        assert_eq!(2.5_f64.field_value(), FieldValue::F64(2.5));
        assert_eq!("s".field_value(), FieldValue::Str("s".into()));
        assert_eq!(String::from("s").field_value(), FieldValue::Str("s".into()));
        assert_eq!((&&"s").field_value(), FieldValue::Str("s".into()));
        assert_eq!(Some(5_u8).field_value(), FieldValue::U64(5));
        assert_eq!(None::<u8>.field_value(), FieldValue::Empty);
        assert!(Empty.field_value().is_empty());
        assert!(!1_u8.field_value().is_empty());
    }

    #[test]
    fn display_and_debug_forms() {
        assert_eq!(display(&"x y").to_string(), "x y");
        assert_eq!(debug(&"x y").to_string(), "\"x y\"");
        assert_eq!(debug(&vec![1, 2]), FieldValue::Text("[1, 2]".into()));
    }

    #[test]
    fn text_form() {
        for (value, text) in [
            (FieldValue::Bool(true), "true"),
            (FieldValue::I64(-1), "-1"),
            (FieldValue::U64(2), "2"),
            (FieldValue::F64(0.5), "0.5"),
            (FieldValue::Str("a b".into()), "\"a b\""),
            (FieldValue::Text("a b".into()), "a b"),
            (FieldValue::Empty, ""),
        ] {
            assert_eq!(value.to_string(), text);
        }
    }
}

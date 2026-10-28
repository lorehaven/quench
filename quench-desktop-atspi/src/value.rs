//! D-Bus values and their wire format: the types, their alignment, and encoding and decoding of one value.
//!
//! Only little-endian is spoken (every peer on a machine that runs this is).

use std::io;

/// One value of the D-Bus type system.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Byte(u8),
    Bool(bool),
    I16(i16),
    U16(u16),
    I32(i32),
    U32(u32),
    I64(i64),
    U64(u64),
    F64(f64),
    Str(String),
    Path(String),
    Signature(String),
    /// An array of values whose element type is the signature.
    Array(String, Vec<Self>),
    Struct(Vec<Self>),
    DictEntry(Box<Self>, Box<Self>),
    Variant(Box<Self>),
}

fn bad(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.to_string())
}

impl Value {
    #[must_use]
    pub fn str(text: &str) -> Self {
        Self::Str(text.to_string())
    }

    #[must_use]
    pub fn path(text: &str) -> Self {
        Self::Path(text.to_string())
    }

    /// `(so)`: a reference to an object on a bus connection.
    #[must_use]
    pub fn reference(bus_name: &str, path: &str) -> Self {
        Self::Struct(vec![Self::str(bus_name), Self::path(path)])
    }

    /// The type signature of this value.
    #[must_use]
    pub fn signature(&self) -> String {
        match self {
            Self::Byte(_) => "y".into(),
            Self::Bool(_) => "b".into(),
            Self::I16(_) => "n".into(),
            Self::U16(_) => "q".into(),
            Self::I32(_) => "i".into(),
            Self::U32(_) => "u".into(),
            Self::I64(_) => "x".into(),
            Self::U64(_) => "t".into(),
            Self::F64(_) => "d".into(),
            Self::Str(_) => "s".into(),
            Self::Path(_) => "o".into(),
            Self::Signature(_) => "g".into(),
            Self::Array(element, _) => format!("a{element}"),
            Self::Struct(fields) => format!(
                "({})",
                fields.iter().map(Self::signature).collect::<String>()
            ),
            Self::DictEntry(k, v) => format!("{{{}{}}}", k.signature(), v.signature()),
            Self::Variant(_) => "v".into(),
        }
    }

    /// An array of strings (`as`).
    #[must_use]
    pub fn strings(items: &[&str]) -> Self {
        Self::Array("s".into(), items.iter().map(|s| Self::str(s)).collect())
    }

    /// The string of a `s`, `o` or `g`.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Str(s) | Self::Path(s) | Self::Signature(s) => Some(s),
            _ => None,
        }
    }

    /// The integer of any integer type.
    #[must_use]
    pub fn as_i64(&self) -> Option<i64> {
        Some(match self {
            Self::Byte(v) => i64::from(*v),
            Self::I16(v) => i64::from(*v),
            Self::U16(v) => i64::from(*v),
            Self::I32(v) => i64::from(*v),
            Self::U32(v) => i64::from(*v),
            Self::I64(v) => *v,
            _ => return None,
        })
    }

    /// Appends the wire form to `buf`, which holds the message so far (alignment counts from its start).
    pub fn encode(&self, buf: &mut Vec<u8>) {
        match self {
            Self::Byte(v) => buf.push(*v),
            Self::Bool(v) => {
                pad(buf, 4);
                buf.extend_from_slice(&u32::from(*v).to_le_bytes());
            }
            Self::I16(v) => {
                pad(buf, 2);
                buf.extend_from_slice(&v.to_le_bytes());
            }
            Self::U16(v) => {
                pad(buf, 2);
                buf.extend_from_slice(&v.to_le_bytes());
            }
            Self::I32(v) => {
                pad(buf, 4);
                buf.extend_from_slice(&v.to_le_bytes());
            }
            Self::U32(v) => {
                pad(buf, 4);
                buf.extend_from_slice(&v.to_le_bytes());
            }
            Self::I64(v) => {
                pad(buf, 8);
                buf.extend_from_slice(&v.to_le_bytes());
            }
            Self::U64(v) => {
                pad(buf, 8);
                buf.extend_from_slice(&v.to_le_bytes());
            }
            Self::F64(v) => {
                pad(buf, 8);
                buf.extend_from_slice(&v.to_le_bytes());
            }
            Self::Str(s) | Self::Path(s) => {
                pad(buf, 4);
                buf.extend_from_slice(&(s.len() as u32).to_le_bytes());
                buf.extend_from_slice(s.as_bytes());
                buf.push(0);
            }
            Self::Signature(s) => {
                buf.push(s.len() as u8);
                buf.extend_from_slice(s.as_bytes());
                buf.push(0);
            }
            Self::Array(element, items) => {
                pad(buf, 4);
                let length_at = buf.len();
                buf.extend_from_slice(&[0; 4]);
                // The elements start at their own alignment, even when there are none.
                pad(buf, alignment(element));
                let start = buf.len();
                for item in items {
                    item.encode(buf);
                }
                let length = (buf.len() - start) as u32;
                buf[length_at..length_at + 4].copy_from_slice(&length.to_le_bytes());
            }
            Self::Struct(fields) => {
                pad(buf, 8);
                for field in fields {
                    field.encode(buf);
                }
            }
            Self::DictEntry(k, v) => {
                pad(buf, 8);
                k.encode(buf);
                v.encode(buf);
            }
            Self::Variant(inner) => {
                Self::Signature(inner.signature()).encode(buf);
                inner.encode(buf);
            }
        }
    }

    /// Reads one value of the complete type `signature` from `data` at `at` (alignment counts from the start of
    /// `data`, which is the start of the message).
    ///
    /// # Errors
    /// Data that is short, mis-sized or not of that type.
    pub fn decode(signature: &str, data: &[u8], at: &mut usize) -> io::Result<Self> {
        decode_value(signature, data, at, 0)
    }
}

const fn alignment(signature: &str) -> usize {
    match signature.as_bytes().first() {
        Some(b'n' | b'q') => 2,
        Some(b'b' | b'i' | b'u' | b's' | b'o' | b'a') => 4,
        Some(b'x' | b't' | b'd' | b'(' | b'{') => 8,
        _ => 1,
    }
}

pub(crate) fn pad(buf: &mut Vec<u8>, to: usize) {
    while !buf.len().is_multiple_of(to) {
        buf.push(0);
    }
}

fn skip(data: &[u8], at: &mut usize, to: usize) -> io::Result<()> {
    while !(*at).is_multiple_of(to) {
        *at += 1;
    }
    if *at > data.len() {
        return Err(bad("padding past the end"));
    }
    Ok(())
}

fn take<'a>(data: &'a [u8], at: &mut usize, n: usize) -> io::Result<&'a [u8]> {
    let end = at
        .checked_add(n)
        .filter(|e| *e <= data.len())
        .ok_or_else(|| bad("short message"))?;
    let out = &data[*at..end];
    *at = end;
    Ok(out)
}

fn number<const N: usize>(data: &[u8], at: &mut usize) -> io::Result<[u8; N]> {
    skip(data, at, N)?;
    let mut out = [0; N];
    out.copy_from_slice(take(data, at, N)?);
    Ok(out)
}

fn string(data: &[u8], at: &mut usize) -> io::Result<String> {
    let length = u32::from_le_bytes(number::<4>(data, at)?) as usize;
    let bytes = take(data, at, length)?.to_vec();
    if take(data, at, 1)? != [0] {
        return Err(bad("string without its terminator"));
    }
    String::from_utf8(bytes).map_err(|_| bad("string is not UTF-8"))
}

/// Splits `signature` into its complete types: `a{sv}i(ss)` is `a{sv}`, `i`, `(ss)`.
///
/// # Errors
/// An unbalanced or unknown signature.
pub fn split_signature(signature: &str) -> io::Result<Vec<String>> {
    let bytes = signature.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let end = complete_type(bytes, i)?;
        out.push(signature[i..end].to_string());
        i = end;
    }
    Ok(out)
}

fn complete_type(bytes: &[u8], start: usize) -> io::Result<usize> {
    let c = *bytes
        .get(start)
        .ok_or_else(|| bad("signature ends early"))?;
    match c {
        b'y' | b'b' | b'n' | b'q' | b'i' | b'u' | b'x' | b't' | b'd' | b's' | b'o' | b'g'
        | b'v' => Ok(start + 1),
        b'a' => complete_type(bytes, start + 1),
        b'(' | b'{' => {
            let close = if c == b'(' { b')' } else { b'}' };
            let mut i = start + 1;
            while *bytes.get(i).ok_or_else(|| bad("unclosed container"))? != close {
                i = complete_type(bytes, i)?;
            }
            Ok(i + 1)
        }
        _ => Err(bad("unknown type in signature")),
    }
}

fn decode_value(signature: &str, data: &[u8], at: &mut usize, depth: usize) -> io::Result<Value> {
    if depth > 32 {
        return Err(bad("values nested too deeply"));
    }
    let first = *signature
        .as_bytes()
        .first()
        .ok_or_else(|| bad("empty type"))?;
    Ok(match first {
        b'y' => Value::Byte(take(data, at, 1)?[0]),
        b'b' => Value::Bool(u32::from_le_bytes(number::<4>(data, at)?) != 0),
        b'n' => Value::I16(i16::from_le_bytes(number::<2>(data, at)?)),
        b'q' => Value::U16(u16::from_le_bytes(number::<2>(data, at)?)),
        b'i' => Value::I32(i32::from_le_bytes(number::<4>(data, at)?)),
        b'u' => Value::U32(u32::from_le_bytes(number::<4>(data, at)?)),
        b'x' => Value::I64(i64::from_le_bytes(number::<8>(data, at)?)),
        b't' => Value::U64(u64::from_le_bytes(number::<8>(data, at)?)),
        b'd' => Value::F64(f64::from_le_bytes(number::<8>(data, at)?)),
        b's' => Value::Str(string(data, at)?),
        b'o' => Value::Path(string(data, at)?),
        b'g' => {
            let length = take(data, at, 1)?[0] as usize;
            let text = String::from_utf8(take(data, at, length)?.to_vec())
                .map_err(|_| bad("signature is not UTF-8"))?;
            if take(data, at, 1)? != [0] {
                return Err(bad("signature without its terminator"));
            }
            Value::Signature(text)
        }
        b'v' => {
            let mut sig_at = *at;
            let Value::Signature(inner) = decode_value("g", data, &mut sig_at, depth + 1)? else {
                unreachable!()
            };
            *at = sig_at;
            if split_signature(&inner)?.len() != 1 {
                return Err(bad("a variant holds exactly one value"));
            }
            Value::Variant(Box::new(decode_value(&inner, data, at, depth + 1)?))
        }
        b'a' => {
            let element = &signature[1..];
            let length = u32::from_le_bytes(number::<4>(data, at)?) as usize;
            if length > 1 << 26 {
                return Err(bad("array too long"));
            }
            skip(data, at, alignment(element))?;
            let end = at
                .checked_add(length)
                .filter(|e| *e <= data.len())
                .ok_or_else(|| bad("array past the end"))?;
            let mut items = Vec::new();
            while *at < end {
                items.push(decode_value(element, data, at, depth + 1)?);
            }
            if *at != end {
                return Err(bad("array elements overran its length"));
            }
            Value::Array(element.to_string(), items)
        }
        b'(' => {
            skip(data, at, 8)?;
            let inner = &signature[1..signature.len() - 1];
            Value::Struct(
                split_signature(inner)?
                    .iter()
                    .map(|t| decode_value(t, data, at, depth + 1))
                    .collect::<io::Result<_>>()?,
            )
        }
        b'{' => {
            skip(data, at, 8)?;
            let parts = split_signature(&signature[1..signature.len() - 1])?;
            let [k, v] = parts.as_slice() else {
                return Err(bad("a dict entry has a key and a value"));
            };
            let key = decode_value(k, data, at, depth + 1)?;
            Value::DictEntry(
                Box::new(key),
                Box::new(decode_value(v, data, at, depth + 1)?),
            )
        }
        _ => return Err(bad("unknown type")),
    })
}

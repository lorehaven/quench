//! The record format: a header of serial types, then the values. A table row is a record, and so is an index
//! entry (the indexed values and then the rowid).

use crate::error::{Error, Result};
use crate::varint;

/// One stored value. SQLite has these five storage classes.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Integer(i64),
    Real(f64),
    Text(String),
    Blob(Vec<u8>),
}

impl Value {
    /// The integer, if this is one.
    #[must_use]
    pub const fn as_integer(&self) -> Option<i64> {
        match self {
            Self::Integer(i) => Some(*i),
            _ => None,
        }
    }

    /// The text, if this is some.
    #[must_use]
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(s) => Some(s),
            _ => None,
        }
    }
}

/// The body size of a serial type, or `None` for the reserved types 10 and 11.
const fn body_size(serial: u64) -> Option<u64> {
    Some(match serial {
        0 | 8 | 9 => 0,
        1 => 1,
        2 => 2,
        3 => 3,
        4 => 4,
        5 => 6,
        6 | 7 => 8,
        10 | 11 => return None,
        n => (n - 12) / 2,
    })
}

fn be_int(bytes: &[u8]) -> i64 {
    let mut value = if bytes.first().is_some_and(|b| b & 0x80 != 0) {
        -1_i64
    } else {
        0
    };
    for &b in bytes {
        value = (value << 8) | i64::from(b);
    }
    value
}

/// Decodes a whole record.
pub fn decode(record: &[u8]) -> Result<Vec<Value>> {
    let bad = |why: &str| Error::corrupt(0, format!("record: {why}"));
    let (header_len, mut at) = varint::get(record).ok_or_else(|| bad("empty"))?;
    let header_len = usize::try_from(header_len).map_err(|_| bad("header size"))?;
    if header_len < at || header_len > record.len() {
        return Err(bad("header size outside the record"));
    }
    let mut serials = Vec::new();
    while at < header_len {
        let (serial, n) = varint::get(&record[at..header_len]).ok_or_else(|| bad("serial type"))?;
        serials.push(serial);
        at += n;
    }
    let mut out = Vec::with_capacity(serials.len());
    let mut body = header_len;
    for serial in serials {
        let size = body_size(serial).ok_or_else(|| bad("reserved serial type"))?;
        let size = usize::try_from(size).map_err(|_| bad("value size"))?;
        let end = body
            .checked_add(size)
            .filter(|&e| e <= record.len())
            .ok_or_else(|| bad("value past the end"))?;
        let bytes = &record[body..end];
        out.push(match serial {
            0 => Value::Null,
            1..=6 => Value::Integer(be_int(bytes)),
            7 => {
                let mut raw = [0_u8; 8];
                raw.copy_from_slice(bytes);
                Value::Real(f64::from_be_bytes(raw))
            }
            8 => Value::Integer(0),
            9 => Value::Integer(1),
            n if n % 2 == 0 => Value::Blob(bytes.to_vec()),
            _ => Value::Text(
                String::from_utf8(bytes.to_vec()).map_err(|_| bad("text is not UTF-8"))?,
            ),
        });
        body = end;
    }
    Ok(out)
}

/// The serial type of a value and the bytes of its body.
fn serial(value: &Value) -> (u64, Vec<u8>) {
    match value {
        Value::Null => (0, Vec::new()),
        Value::Integer(0) => (8, Vec::new()),
        Value::Integer(1) => (9, Vec::new()),
        Value::Integer(i) => {
            let i = *i;
            let (code, size) = if i8::try_from(i).is_ok() {
                (1, 1)
            } else if i16::try_from(i).is_ok() {
                (2, 2)
            } else if (-(1 << 23)..(1 << 23)).contains(&i) {
                (3, 3)
            } else if i32::try_from(i).is_ok() {
                (4, 4)
            } else if (-(1_i64 << 47)..(1_i64 << 47)).contains(&i) {
                (5, 6)
            } else {
                (6, 8)
            };
            (code, i.to_be_bytes()[8 - size..].to_vec())
        }
        Value::Real(r) => (7, r.to_be_bytes().to_vec()),
        Value::Text(s) => (13 + 2 * s.len() as u64, s.as_bytes().to_vec()),
        Value::Blob(b) => (12 + 2 * b.len() as u64, b.clone()),
    }
}

/// Encodes values as a record.
#[must_use]
pub fn encode(values: &[Value]) -> Vec<u8> {
    let parts: Vec<(u64, Vec<u8>)> = values.iter().map(serial).collect();
    let types_len: usize = parts.iter().map(|(s, _)| varint::len(*s)).sum();
    // The header size counts its own varint, which is one byte unless the types are very many.
    let mut header_len = types_len + 1;
    if varint::len(header_len as u64) > 1 {
        header_len = types_len + varint::len((types_len + 2) as u64);
    }
    let mut out =
        Vec::with_capacity(header_len + parts.iter().map(|(_, b)| b.len()).sum::<usize>());
    varint::put(&mut out, header_len as u64);
    for (s, _) in &parts {
        varint::put(&mut out, *s);
    }
    for (_, body) in &parts {
        out.extend_from_slice(body);
    }
    out
}

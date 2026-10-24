//! Android binary XML: the compiled `AndroidManifest.xml` and layout files inside an APK.
//!
//! The result has the shape documented in [`xml`](crate::xml), so it is read with the same helpers:
//!
//! ```no_run
//! # let manifest_bytes: Vec<u8> = Vec::new();
//! use quench_convert::{axml, xml};
//!
//! let root = axml::parse(&manifest_bytes).unwrap();
//! let package = xml::attr(&root, "package");
//! let version_code = xml::attr(&root, "android:versionCode");
//! ```
//!
//! Attribute names carry their namespace prefix (`android:versionCode`); the declarations appear as
//! `xmlns:android` attributes on the first element after them. Values are rendered as `aapt` shows them:
//! strings as they are, integers in decimal, hexadecimal as `0x…`, booleans as `true`/`false`,
//! references as `@0x7f0b0001`, attribute references as `?0x…`, colors as `#aarrggbb`, dimensions as
//! `16dp`, fractions as `50%`.

use crate::error::{Error, Result};
use crate::xml::element;
use quench_serde::{Map, Value};

const XML: u16 = 0x0003;
const STRING_POOL: u16 = 0x0001;
const START_NAMESPACE: u16 = 0x0100;
const END_NAMESPACE: u16 = 0x0101;
const START_ELEMENT: u16 = 0x0102;
const END_ELEMENT: u16 = 0x0103;
const CDATA: u16 = 0x0104;
const NONE: u32 = 0xFFFF_FFFF;
const UTF8_FLAG: u32 = 0x100;

fn fail<T>(message: &str, at: usize) -> Result<T> {
    Err(Error::new("axml", message, at))
}

struct Bytes<'a> {
    data: &'a [u8],
}

impl Bytes<'_> {
    fn u8(&self, at: usize) -> Result<u8> {
        self.data
            .get(at)
            .copied()
            .ok_or_else(|| Error::new("axml", "unexpected end of data", at))
    }

    fn u16(&self, at: usize) -> Result<u16> {
        Ok(u16::from_le_bytes([self.u8(at)?, self.u8(at + 1)?]))
    }

    fn u32(&self, at: usize) -> Result<u32> {
        Ok(u32::from(self.u16(at)?) | u32::from(self.u16(at + 2)?) << 16)
    }
}

fn string_pool(b: &Bytes<'_>, start: usize, end: usize) -> Result<Vec<String>> {
    let count = b.u32(start + 8)? as usize;
    let flags = b.u32(start + 16)?;
    let strings_start = b.u32(start + 20)? as usize;
    if count > (end - start) / 4 {
        return fail("string pool larger than its chunk", start);
    }
    let utf8 = flags & UTF8_FLAG != 0;
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let offset = b.u32(start + b.u16(start + 2)? as usize + i * 4)? as usize;
        let at = start + strings_start + offset;
        if at >= end {
            return fail("string outside its pool", at);
        }
        out.push(if utf8 {
            utf8_string(b, at, end)?
        } else {
            utf16_string(b, at, end)?
        });
    }
    Ok(out)
}

fn utf16_string(b: &Bytes<'_>, mut at: usize, end: usize) -> Result<String> {
    let mut length = usize::from(b.u16(at)?);
    at += 2;
    if length & 0x8000 != 0 {
        length = (length & 0x7FFF) << 16 | usize::from(b.u16(at)?);
        at += 2;
    }
    if at + length * 2 > end {
        return fail("string outside its pool", at);
    }
    let units: Result<Vec<u16>> = (0..length).map(|i| b.u16(at + i * 2)).collect();
    Ok(String::from_utf16_lossy(&units?))
}

fn utf8_string(b: &Bytes<'_>, mut at: usize, end: usize) -> Result<String> {
    // Two lengths: in characters, then in bytes; each is one byte, or two when the high bit is set.
    if b.u8(at)? & 0x80 != 0 {
        at += 1;
    }
    at += 1;
    let mut length = usize::from(b.u8(at)?);
    at += 1;
    if length & 0x80 != 0 {
        length = (length & 0x7F) << 8 | usize::from(b.u8(at)?);
        at += 1;
    }
    let bytes = b.data.get(at..at + length).filter(|_| at + length <= end);
    match bytes {
        Some(bytes) => Ok(String::from_utf8_lossy(bytes).into_owned()),
        None => fail("string outside its pool", at),
    }
}

fn dimension(data: u32, fraction: bool) -> String {
    const RADIX: [f64; 4] = [1.0, 1.0 / 128.0, 1.0 / 32768.0, 1.0 / 8_388_608.0];
    let mantissa = f64::from((data >> 8) as i32) * RADIX[((data >> 4) & 3) as usize];
    let unit = if fraction {
        if data & 0xF == 1 { "%p" } else { "%" }
    } else {
        ["px", "dp", "sp", "pt", "in", "mm"]
            .get((data & 0xF) as usize)
            .copied()
            .unwrap_or("")
    };
    let scaled = if fraction { mantissa * 100.0 } else { mantissa };
    if scaled.fract() == 0.0 {
        format!("{scaled:.0}{unit}")
    } else {
        format!("{scaled}{unit}")
    }
}

fn typed_value(kind: u8, data: u32, pool: &[String], at: usize) -> Result<String> {
    Ok(match kind {
        0x00 => String::new(),
        0x01 => format!("@0x{data:08x}"),
        0x02 => format!("?0x{data:08x}"),
        0x03 => pool
            .get(data as usize)
            .cloned()
            .ok_or_else(|| Error::new("axml", "string index out of range", at))?,
        0x04 => f32::from_bits(data).to_string(),
        0x05 => dimension(data, false),
        0x06 => dimension(data, true),
        0x10 => (data as i32).to_string(),
        0x11 => format!("0x{data:x}"),
        0x12 => (data != 0).to_string(),
        0x1c => format!("#{data:08x}"),
        0x1d => format!("#{:06x}", data & 0xFF_FFFF),
        0x1e => format!("#{:04x}", data & 0xFFFF),
        0x1f => format!("#{:03x}", data & 0xFFF),
        _ => data.to_string(),
    })
}

/// Reads a compiled Android XML file into its root element.
///
/// # Errors
///
/// A file that is not Android binary XML, truncated chunks, string indexes outside the pool, or no
/// element.
#[allow(clippy::too_many_lines)]
pub fn parse(data: &[u8]) -> Result<Value> {
    let b = Bytes { data };
    if b.u16(0)? != XML {
        return fail("not Android binary XML", 0);
    }
    let total = (b.u32(4)? as usize).min(data.len());
    let mut at = usize::from(b.u16(2)?);
    let mut pool: Vec<String> = Vec::new();
    let mut namespaces: Vec<(String, String)> = Vec::new(); // (uri, prefix)
    let mut declared: Vec<(String, String)> = Vec::new(); // waiting for the next element
    let mut stack: Vec<(String, Map, Vec<Value>)> = Vec::new();
    let mut root: Option<Value> = None;

    while at + 8 <= total {
        let kind = b.u16(at)?;
        let header = usize::from(b.u16(at + 2)?);
        let size = b.u32(at + 4)? as usize;
        if size < 8 || at + size > total {
            return fail("chunk outside the file", at);
        }
        let end = at + size;
        let name_at = |index: u32| -> Result<String> {
            pool.get(index as usize)
                .cloned()
                .ok_or_else(|| Error::new("axml", "string index out of range", at))
        };
        match kind {
            STRING_POOL => pool = string_pool(&b, at, end)?,
            START_NAMESPACE => {
                let prefix = name_at(b.u32(at + 16)?)?;
                let uri = name_at(b.u32(at + 20)?)?;
                declared.push((prefix.clone(), uri.clone()));
                namespaces.push((uri, prefix));
            }
            END_NAMESPACE => {
                namespaces.pop();
            }
            START_ELEMENT => {
                let ext = at + header.max(16);
                let tag = name_at(b.u32(ext + 4)?)?;
                let attr_start = usize::from(b.u16(ext + 8)?);
                let attr_size = usize::from(b.u16(ext + 10)?).max(20);
                let count = usize::from(b.u16(ext + 12)?);
                let mut attrs = Map::new();
                for (prefix, uri) in std::mem::take(&mut declared) {
                    attrs.insert(format!("xmlns:{prefix}"), Value::String(uri));
                }
                for i in 0..count {
                    let a = ext + attr_start + i * attr_size;
                    if a + 20 > end {
                        return fail("attribute outside its element", a);
                    }
                    let ns = b.u32(a)?;
                    let mut name = name_at(b.u32(a + 4)?)?;
                    if ns != NONE {
                        let uri = name_at(ns)?;
                        if let Some((_, prefix)) = namespaces.iter().rev().find(|(u, _)| *u == uri)
                            && !prefix.is_empty()
                        {
                            name = format!("{prefix}:{name}");
                        }
                    }
                    let raw = b.u32(a + 8)?;
                    let value = if raw == NONE {
                        typed_value(b.u8(a + 15)?, b.u32(a + 16)?, &pool, a)?
                    } else {
                        name_at(raw)?
                    };
                    attrs.insert(name, Value::String(value));
                }
                stack.push((tag, attrs, Vec::new()));
            }
            END_ELEMENT => {
                let Some((tag, attrs, children)) = stack.pop() else {
                    return fail("an element is closed that was never opened", at);
                };
                let done = element(&tag, attrs, children);
                match stack.last_mut() {
                    Some(parent) => parent.2.push(done),
                    None => root = Some(done),
                }
            }
            CDATA => {
                let text = name_at(b.u32(at + 16)?)?;
                if let Some(parent) = stack.last_mut() {
                    parent.2.push(Value::String(text));
                }
            }
            _ => {}
        }
        at = end;
    }
    if !stack.is_empty() {
        return fail("an element is never closed", at);
    }
    root.map_or_else(|| fail("the document has no element", at), Ok)
}

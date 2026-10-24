//! GGUF model files: the header and the metadata key/value pairs (and, if asked, the tensor table).
//!
//! The reader stops before the tensor data, so reading a multi-gigabyte model touches only its first few
//! kilobytes (or megabytes, when the file embeds a large tokenizer):
//!
//! ```no_run
//! use quench_convert::gguf::{self, Options};
//!
//! let file = std::fs::File::open("model.gguf").unwrap();
//! let header = gguf::read(file, &Options::default()).unwrap();
//! let layers = header["metadata"]["llama.block_count"].as_u64();
//! ```
//!
//! The value is `{"version", "tensor_count", "metadata": {key: value}}`, plus `"tensors"` (a list of
//! `{"name", "dimensions", "type", "offset"}`) when [`Options::tensors`] is set. Metadata arrays longer
//! than [`Options::max_array`] (token lists, scores) are replaced by `{"array_length": n}` instead of being
//! held in memory.

use crate::error::{Error, Result};
use quench_serde::{Map, Value};
use std::io::Read;

const MAGIC: [u8; 4] = *b"GGUF";
/// A string longer than this is refused rather than allocated.
const MAX_STRING: u64 = 16 << 20;
const MAX_ENTRIES: u64 = 1 << 20;
const MAX_DEPTH: usize = 4;

/// What to read.
#[derive(Debug, Clone)]
pub struct Options {
    /// Metadata arrays with more elements than this are summarised as `{"array_length": n}`.
    pub max_array: usize,
    /// Also read the tensor table that follows the metadata.
    pub tensors: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            max_array: 1024,
            tensors: false,
        }
    }
}

struct Source<R> {
    reader: R,
    read: usize,
}

impl<R: Read> Source<R> {
    fn bytes<const N: usize>(&mut self) -> Result<[u8; N]> {
        let mut buf = [0u8; N];
        self.reader
            .read_exact(&mut buf)
            .map_err(|e| Error::new("gguf", format!("cannot read the file: {e}"), self.read))?;
        self.read += N;
        Ok(buf)
    }

    fn u32(&mut self) -> Result<u32> {
        self.bytes().map(u32::from_le_bytes)
    }

    fn u64(&mut self) -> Result<u64> {
        self.bytes().map(u64::from_le_bytes)
    }

    fn string(&mut self, wide: bool) -> Result<String> {
        let length = if wide {
            self.u64()?
        } else {
            u64::from(self.u32()?)
        };
        if length > MAX_STRING {
            return Err(Error::new(
                "gguf",
                "a string is implausibly long",
                self.read,
            ));
        }
        let mut buf = vec![0u8; length as usize];
        self.reader
            .read_exact(&mut buf)
            .map_err(|e| Error::new("gguf", format!("cannot read the file: {e}"), self.read))?;
        self.read += buf.len();
        Ok(String::from_utf8_lossy(&buf).into_owned())
    }
}

fn scalar<R: Read>(s: &mut Source<R>, kind: u32, wide: bool, depth: usize) -> Result<Value> {
    Ok(match kind {
        0 => Value::from(u64::from(s.bytes::<1>()?[0])),
        1 => Value::from(i64::from(i8::from_le_bytes(s.bytes()?))),
        2 => Value::from(u64::from(u16::from_le_bytes(s.bytes()?))),
        3 => Value::from(i64::from(i16::from_le_bytes(s.bytes()?))),
        4 => Value::from(u64::from(s.u32()?)),
        5 => Value::from(i64::from(i32::from_le_bytes(s.bytes()?))),
        6 => Value::Float(f64::from(f32::from_le_bytes(s.bytes()?))),
        7 => Value::Bool(s.bytes::<1>()?[0] != 0),
        8 => Value::String(s.string(wide)?),
        10 => Value::from(s.u64()?),
        11 => Value::from(i64::from_le_bytes(s.bytes()?)),
        12 => Value::Float(f64::from_le_bytes(s.bytes()?)),
        9 => {
            return Err(Error::new(
                "gguf",
                format!("nested arrays beyond depth {depth}"),
                s.read,
            ));
        }
        other => {
            return Err(Error::new(
                "gguf",
                format!("unknown value type {other}"),
                s.read,
            ));
        }
    })
}

fn value<R: Read>(
    s: &mut Source<R>,
    kind: u32,
    wide: bool,
    options: &Options,
    depth: usize,
) -> Result<Value> {
    if kind != 9 {
        return scalar(s, kind, wide, depth);
    }
    if depth >= MAX_DEPTH {
        return Err(Error::new("gguf", "arrays nest too deeply", s.read));
    }
    let element = s.u32()?;
    let length = if wide { s.u64()? } else { u64::from(s.u32()?) };
    let keep = length <= options.max_array as u64;
    let mut items = Vec::new();
    for _ in 0..length {
        let v = value(s, element, wide, options, depth + 1)?;
        if keep {
            items.push(v);
        }
    }
    if keep {
        Ok(Value::Array(items))
    } else {
        let mut map = Map::new();
        map.insert("array_length", Value::from(length));
        Ok(Value::Object(map))
    }
}

/// Reads the header and metadata of a GGUF stream.
///
/// # Errors
///
/// Not a GGUF file, a big-endian file (not supported), an unknown value type, or a stream that ends early.
pub fn read<R: Read>(reader: R, options: &Options) -> Result<Value> {
    let mut s = Source { reader, read: 0 };
    if s.bytes::<4>()? != MAGIC {
        return Err(Error::new("gguf", "not a GGUF file", 0));
    }
    let version = s.u32()?;
    if !(1..=3).contains(&version) {
        return Err(Error::new(
            "gguf",
            format!("unsupported version {version} (big-endian files are not supported)"),
            4,
        ));
    }
    let wide = version >= 2;
    let tensor_count = if wide { s.u64()? } else { u64::from(s.u32()?) };
    let kv_count = if wide { s.u64()? } else { u64::from(s.u32()?) };
    if kv_count > MAX_ENTRIES || tensor_count > MAX_ENTRIES * 16 {
        return Err(Error::new("gguf", "implausible entry counts", s.read));
    }

    let mut metadata = Map::new();
    for _ in 0..kv_count {
        let key = s.string(wide)?;
        let kind = s.u32()?;
        let v = value(&mut s, kind, wide, options, 0)?;
        metadata.insert(key, v);
    }

    let mut out = Map::new();
    out.insert("version", Value::from(u64::from(version)));
    out.insert("tensor_count", Value::from(tensor_count));
    out.insert("metadata", Value::Object(metadata));
    if options.tensors {
        let mut tensors = Vec::new();
        for _ in 0..tensor_count {
            let name = s.string(wide)?;
            let dims = s.u32()?;
            if dims > 8 {
                return Err(Error::new(
                    "gguf",
                    "a tensor has too many dimensions",
                    s.read,
                ));
            }
            let mut dimensions = Vec::new();
            for _ in 0..dims {
                dimensions.push(Value::from(if wide {
                    s.u64()?
                } else {
                    u64::from(s.u32()?)
                }));
            }
            let kind = s.u32()?;
            let offset = if wide { s.u64()? } else { u64::from(s.u32()?) };
            let mut t = Map::new();
            t.insert("name", Value::String(name));
            t.insert("dimensions", Value::Array(dimensions));
            t.insert("type", Value::from(u64::from(kind)));
            t.insert("offset", Value::from(offset));
            tensors.push(Value::Object(t));
        }
        out.insert("tensors", Value::Array(tensors));
    }
    Ok(Value::Object(out))
}

/// [`read`] on bytes already in memory.
///
/// # Errors
///
/// As [`read`].
pub fn parse(bytes: &[u8], options: &Options) -> Result<Value> {
    read(bytes, options)
}

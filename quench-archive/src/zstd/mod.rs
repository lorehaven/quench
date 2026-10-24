//! Zstandard (RFC 8878): frames of raw, run-length and compressed blocks.
//!
//! The decoder handles everything a dictionary-less stream can contain: all block types, Huffman and FSE
//! tables (sent, predefined, repeated), skippable frames, and the content checksum. The encoder writes
//! single-segment frames of LZ77 sequences coded with the predefined FSE tables and Huffman-coded
//! literals; its output is a valid stream that any zstd decoder reads, compressing text well though not
//! as tightly as `zstd -19`.
//!
//! ```
//! let packed = quench_archive::zstd::compress(b"hello hello hello hello");
//! assert_eq!(quench_archive::zstd::decompress(&packed, 1024).unwrap(), b"hello hello hello hello");
//! ```

mod bits;
mod decode;
mod encode;
mod fse;
mod huffman;
mod tables;

use crate::checksum::xxh64;
use crate::error::{Error, Result};

const MAGIC: u32 = 0xFD2F_B528;
const SKIPPABLE_MASK: u32 = 0xFFFF_FFF0;
const SKIPPABLE_MAGIC: u32 = 0x184D_2A50;

/// Decompresses every frame of `data` (concatenated frames are one stream, skippable frames are ignored),
/// refusing to produce more than `limit` bytes.
///
/// # Errors
///
/// [`Error::Corrupt`] / [`Error::Truncated`] for a malformed stream, [`Error::Checksum`] when the content
/// checksum fails, [`Error::Unsupported`] for dictionaries, [`Error::Limit`] past `limit`.
pub fn decompress(data: &[u8], limit: usize) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut rest = data;
    if rest.is_empty() {
        return Err(Error::Truncated);
    }
    while !rest.is_empty() {
        let magic = u32::from_le_bytes(
            rest.get(..4)
                .ok_or(Error::Truncated)?
                .try_into()
                .expect("four bytes"),
        );
        if magic & SKIPPABLE_MASK == SKIPPABLE_MAGIC {
            let size = rest.get(4..8).ok_or(Error::Truncated)?;
            let size = u32::from_le_bytes(size.try_into().expect("four bytes")) as usize;
            rest = rest.get(8 + size..).ok_or(Error::Truncated)?;
        } else if magic == MAGIC {
            let used = frame(&rest[4..], &mut out, limit)?;
            rest = &rest[4 + used..];
        } else {
            return Err(Error::corrupt("not a zstd frame"));
        }
    }
    Ok(out)
}

/// Decodes one frame after its magic number; returns the bytes it occupied.
#[allow(clippy::too_many_lines)]
fn frame(data: &[u8], out: &mut Vec<u8>, limit: usize) -> Result<usize> {
    let descriptor = *data.first().ok_or(Error::Truncated)?;
    let content_size_flag = descriptor >> 6;
    let single_segment = descriptor & 0x20 != 0;
    let has_checksum = descriptor & 0x04 != 0;
    let dictionary_flag = descriptor & 3;
    if descriptor & 0x08 != 0 {
        return Err(Error::corrupt("reserved frame header bit"));
    }

    let mut at = 1;
    if !single_segment {
        // The window descriptor; whole output is kept in memory, so the size itself is not needed.
        data.get(at).ok_or(Error::Truncated)?;
        at += 1;
    }
    let dictionary_size = [0usize, 1, 2, 4][usize::from(dictionary_flag)];
    let dictionary = data
        .get(at..at + dictionary_size)
        .ok_or(Error::Truncated)?
        .iter()
        .rev()
        .fold(0u32, |acc, &b| acc << 8 | u32::from(b));
    if dictionary != 0 {
        return Err(Error::Unsupported("zstd dictionaries".to_string()));
    }
    at += dictionary_size;

    let content_size_bytes = match content_size_flag {
        0 => usize::from(single_segment),
        1 => 2,
        2 => 4,
        _ => 8,
    };
    let mut content_size = None;
    if content_size_bytes > 0 {
        let raw = data
            .get(at..at + content_size_bytes)
            .ok_or(Error::Truncated)?;
        let mut value = raw
            .iter()
            .rev()
            .fold(0u64, |acc, &b| acc << 8 | u64::from(b));
        if content_size_flag == 1 {
            value += 256;
        }
        content_size = Some(value);
        at += content_size_bytes;
    }

    let frame_start = out.len();
    let mut state = decode::State::new();
    loop {
        let header = data.get(at..at + 3).ok_or(Error::Truncated)?;
        let header = u32::from(header[0]) | u32::from(header[1]) << 8 | u32::from(header[2]) << 16;
        at += 3;
        let last = header & 1 == 1;
        let size = (header >> 3) as usize;
        match (header >> 1) & 3 {
            0 => {
                if size > decode::BLOCK_MAX {
                    return Err(Error::corrupt("raw block larger than 128 KiB"));
                }
                if out.len() + size > limit {
                    return Err(Error::Limit(format!("more than {limit} bytes")));
                }
                out.extend_from_slice(data.get(at..at + size).ok_or(Error::Truncated)?);
                at += size;
            }
            1 => {
                if size > decode::BLOCK_MAX {
                    return Err(Error::corrupt("run-length block larger than 128 KiB"));
                }
                if out.len() + size > limit {
                    return Err(Error::Limit(format!("more than {limit} bytes")));
                }
                let byte = *data.get(at).ok_or(Error::Truncated)?;
                out.resize(out.len() + size, byte);
                at += 1;
            }
            2 => {
                if size > decode::BLOCK_MAX {
                    return Err(Error::corrupt("compressed block larger than 128 KiB"));
                }
                let body = data.get(at..at + size).ok_or(Error::Truncated)?;
                decode::block(body, &mut state, out, frame_start, limit)?;
                at += size;
            }
            _ => return Err(Error::corrupt("reserved block type")),
        }
        if last {
            break;
        }
    }

    if let Some(expected) = content_size
        && expected != (out.len() - frame_start) as u64
    {
        return Err(Error::corrupt("frame content size does not match"));
    }
    if has_checksum {
        let stored = data.get(at..at + 4).ok_or(Error::Truncated)?;
        let stored = u32::from_le_bytes(stored.try_into().expect("four bytes"));
        at += 4;
        if stored != xxh64(&out[frame_start..], 0) as u32 {
            return Err(Error::Checksum("zstd content checksum".to_string()));
        }
    }
    Ok(at)
}

pub use encode::compress;

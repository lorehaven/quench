//! gzip (RFC 1952) members around a deflate stream.

use crate::checksum::crc32;
use crate::deflate;
use crate::error::{Error, Result};

const FLAG_HEADER_CRC: u8 = 0x02;
const FLAG_EXTRA: u8 = 0x04;
const FLAG_NAME: u8 = 0x08;
const FLAG_COMMENT: u8 = 0x10;
const FLAG_RESERVED: u8 = 0xE0;

/// Compresses `data` into one gzip member (no name, modification time 0, so the output is reproducible).
#[must_use]
pub fn compress(data: &[u8]) -> Vec<u8> {
    let mut out = vec![0x1F, 0x8B, 8, 0, 0, 0, 0, 0, 0, 0xFF];
    out.extend_from_slice(&deflate::compress(data));
    out.extend_from_slice(&crc32(data).to_le_bytes());
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out
}

/// Decompresses every member of a gzip file (concatenated members are one stream, as `gzip -d` reads them),
/// refusing to produce more than `limit` bytes.
///
/// # Errors
///
/// [`Error::Corrupt`], [`Error::Truncated`], [`Error::Checksum`] for a damaged file, [`Error::Limit`] past `limit`.
pub fn decompress(data: &[u8], limit: usize) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut rest = data;
    loop {
        let (member, used) = member(rest, limit - out.len())?;
        out.extend_from_slice(&member);
        rest = &rest[used..];
        if rest.is_empty() {
            return Ok(out);
        }
    }
}

fn member(data: &[u8], limit: usize) -> Result<(Vec<u8>, usize)> {
    if data.len() < 10 {
        return Err(Error::Truncated);
    }
    if data[0] != 0x1F || data[1] != 0x8B {
        return Err(Error::corrupt("not a gzip stream"));
    }
    if data[2] != 8 {
        return Err(Error::Unsupported(format!(
            "gzip compression method {}",
            data[2]
        )));
    }
    let flags = data[3];
    if flags & FLAG_RESERVED != 0 {
        return Err(Error::corrupt("reserved gzip flags"));
    }
    let mut at = 10;
    if flags & FLAG_EXTRA != 0 {
        let length = data.get(at..at + 2).ok_or(Error::Truncated)?;
        at += 2 + usize::from(u16::from_le_bytes([length[0], length[1]]));
    }
    for flag in [FLAG_NAME, FLAG_COMMENT] {
        if flags & flag != 0 {
            let end = data
                .get(at..)
                .and_then(|rest| rest.iter().position(|&b| b == 0))
                .ok_or(Error::Truncated)?;
            at += end + 1;
        }
    }
    if flags & FLAG_HEADER_CRC != 0 {
        at += 2;
    }
    let body = data.get(at..).ok_or(Error::Truncated)?;
    let (out, used) = deflate::inflate(body, limit)?;
    let trailer = body.get(used..used + 8).ok_or(Error::Truncated)?;
    let crc = u32::from_le_bytes(trailer[..4].try_into().expect("four bytes"));
    let size = u32::from_le_bytes(trailer[4..].try_into().expect("four bytes"));
    if crc != crc32(&out) {
        return Err(Error::Checksum("gzip CRC-32".to_string()));
    }
    if size != out.len() as u32 {
        return Err(Error::Checksum("gzip length".to_string()));
    }
    Ok((out, at + used + 8))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_concatenates() {
        let a = compress(b"hello ");
        let b = compress(b"world");
        let both = [a.clone(), b].concat();
        assert_eq!(decompress(&a, 100).unwrap(), b"hello ");
        assert_eq!(decompress(&both, 100).unwrap(), b"hello world");
        assert!(matches!(decompress(&both, 8), Err(Error::Limit(_))));
    }

    #[test]
    fn optional_header_fields_are_skipped() {
        let plain = compress(b"data");
        let body = &plain[10..];
        let mut file = vec![
            0x1F,
            0x8B,
            8,
            FLAG_EXTRA | FLAG_NAME | FLAG_COMMENT | FLAG_HEADER_CRC,
        ];
        file.extend_from_slice(&[0; 6]);
        file.extend_from_slice(&[3, 0, 1, 2, 3]); // extra
        file.extend_from_slice(b"name\0");
        file.extend_from_slice(b"comment\0");
        file.extend_from_slice(&[0, 0]); // header crc (not checked)
        file.extend_from_slice(body);
        assert_eq!(decompress(&file, 100).unwrap(), b"data");
    }

    #[test]
    fn damage_is_reported() {
        let good = compress(b"some text some text some text");
        assert!(matches!(decompress(&good[..5], 100), Err(Error::Truncated)));
        assert!(matches!(
            decompress(b"not gzip at all", 100),
            Err(Error::Corrupt(_))
        ));
        let mut method = good.clone();
        method[2] = 7;
        assert!(matches!(
            decompress(&method, 100),
            Err(Error::Unsupported(_))
        ));
        let mut flags = good.clone();
        flags[3] = 0x20;
        assert!(matches!(decompress(&flags, 100), Err(Error::Corrupt(_))));
        let mut crc = good.clone();
        let n = crc.len();
        crc[n - 8] ^= 1;
        assert!(matches!(decompress(&crc, 100), Err(Error::Checksum(_))));
        let mut size = good.clone();
        size[n - 1] ^= 1;
        assert!(matches!(decompress(&size, 100), Err(Error::Checksum(_))));
        assert!(matches!(
            decompress(&good[..n - 3], 100),
            Err(Error::Truncated)
        ));
    }
}

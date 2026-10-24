//! Stream filters: `Flate`, `LZW`, `ASCII85`, `ASCIIHex`, `RunLength`, and the PNG/TIFF predictors.

use crate::error::{Error, Result};
use crate::object::Dict;

/// The most a stream may expand to: far above any real page, below what would exhaust memory.
const MAX_DECODED: usize = 512 << 20;

pub fn apply(name: &str, data: &[u8], params: Option<&Dict>) -> Result<Vec<u8>> {
    match name {
        "FlateDecode" | "Fl" => predictor(flate(data)?, params),
        "LZWDecode" | "LZW" => {
            let early = params
                .and_then(|p| p.get("EarlyChange"))
                .and_then(crate::object::Object::as_int)
                .unwrap_or(1);
            predictor(lzw(data, early != 0)?, params)
        }
        "ASCII85Decode" | "A85" => ascii85(data),
        "ASCIIHexDecode" | "AHx" => Ok(ascii_hex(data)),
        "RunLengthDecode" | "RL" => Ok(run_length(data)),
        other => Err(Error::Unsupported(format!("stream filter {other}"))),
    }
}

fn flate(data: &[u8]) -> Result<Vec<u8>> {
    // Skip leading whitespace some writers leave, then the two-byte zlib header.
    let start = data
        .iter()
        .position(|b| !b.is_ascii_whitespace())
        .unwrap_or(0);
    let body = &data[start..];
    let has_header = body.len() >= 2
        && body[0] & 0x0F == 8
        && (u16::from(body[0]) << 8 | u16::from(body[1])) % 31 == 0;
    let raw = if has_header { &body[2..] } else { body };
    quench_archive::deflate::decompress(raw, MAX_DECODED)
        .map_err(|e| Error::Malformed(format!("FlateDecode: {e}")))
}

fn lzw(data: &[u8], early: bool) -> Result<Vec<u8>> {
    let mut out: Vec<u8> = Vec::new();
    // Codes 258 and up; 0-255 are single bytes, 256 clears the table, 257 ends the data.
    let mut table: Vec<Vec<u8>> = Vec::new();
    let mut previous: Option<Vec<u8>> = None;
    let mut width = 9u32;
    let (mut buffer, mut bits) = (0u32, 0u32);
    for &byte in data {
        buffer = buffer << 8 | u32::from(byte);
        bits += 8;
        while bits >= width {
            let code = (buffer >> (bits - width)) as usize & ((1 << width) - 1);
            bits -= width;
            buffer &= (1 << bits) - 1;
            match code {
                256 => {
                    table.clear();
                    previous = None;
                    width = 9;
                    continue;
                }
                257 => return Ok(out),
                _ => {}
            }
            let current = if code < 256 {
                vec![code as u8]
            } else if let Some(entry) = code.checked_sub(258).and_then(|i| table.get(i)) {
                entry.clone()
            } else if let (true, Some(p)) = (code == 258 + table.len(), previous.as_ref()) {
                let mut bytes = p.clone();
                bytes.push(p[0]);
                bytes
            } else {
                return Err(Error::Malformed("LZWDecode: bad code".into()));
            };
            out.extend_from_slice(&current);
            if out.len() > MAX_DECODED {
                return Err(Error::Malformed("LZWDecode: output too large".into()));
            }
            if let Some(mut grown) = previous.take() {
                grown.push(current[0]);
                table.push(grown);
            }
            previous = Some(current);
            width = match 258 + table.len() + usize::from(early) {
                0..=511 => 9,
                512..=1023 => 10,
                1024..=2047 => 11,
                _ => 12,
            };
        }
    }
    Ok(out)
}

fn ascii85(data: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut group = [0u32; 5];
    let mut count = 0;
    let body = data.strip_prefix(b"<~").unwrap_or(data);
    for &b in body {
        match b {
            b'~' => break,
            b'z' if count == 0 => out.extend_from_slice(&[0; 4]),
            b'!'..=b'u' => {
                group[count] = u32::from(b - b'!');
                count += 1;
                if count == 5 {
                    let value = group
                        .iter()
                        .fold(0u32, |acc, &d| acc.wrapping_mul(85).wrapping_add(d));
                    out.extend_from_slice(&value.to_be_bytes());
                    count = 0;
                }
            }
            b if b.is_ascii_whitespace() => {}
            _ => return Err(Error::Malformed("ASCII85Decode: bad byte".into())),
        }
    }
    if count > 1 {
        for slot in group.iter_mut().skip(count) {
            *slot = 84;
        }
        let value = group
            .iter()
            .fold(0u32, |acc, &d| acc.wrapping_mul(85).wrapping_add(d));
        out.extend_from_slice(&value.to_be_bytes()[..count - 1]);
    }
    Ok(out)
}

fn ascii_hex(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut high: Option<u8> = None;
    for &b in data {
        if b == b'>' {
            break;
        }
        let Some(v) = (b as char).to_digit(16) else {
            continue;
        };
        match high.take() {
            Some(h) => out.push(h << 4 | v as u8),
            None => high = Some(v as u8),
        }
    }
    if let Some(h) = high {
        out.push(h << 4);
    }
    out
}

fn run_length(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < data.len() {
        let length = data[i];
        i += 1;
        match length {
            128 => break,
            0..=127 => {
                let end = (i + usize::from(length) + 1).min(data.len());
                out.extend_from_slice(&data[i..end]);
                i = end;
            }
            _ => {
                if let Some(&b) = data.get(i) {
                    out.extend(std::iter::repeat_n(b, 257 - usize::from(length)));
                }
                i += 1;
            }
        }
    }
    out
}

fn predictor(data: Vec<u8>, params: Option<&Dict>) -> Result<Vec<u8>> {
    let get = |key: &str, default: i64| {
        params
            .and_then(|p| p.get(key))
            .and_then(crate::object::Object::as_int)
            .unwrap_or(default)
    };
    let predictor = get("Predictor", 1);
    if predictor < 2 {
        return Ok(data);
    }
    let colors = usize::try_from(get("Colors", 1)).unwrap_or(1).max(1);
    let bpc = usize::try_from(get("BitsPerComponent", 8))
        .unwrap_or(8)
        .max(1);
    let columns = usize::try_from(get("Columns", 1)).unwrap_or(1).max(1);
    let bytes_per_pixel = (colors * bpc).div_ceil(8);
    let row = (colors * bpc * columns).div_ceil(8);

    if predictor == 2 {
        if bpc != 8 {
            return Err(Error::Unsupported(
                "TIFF predictor on other than 8-bit samples".into(),
            ));
        }
        let mut out = data;
        for line in out.chunks_mut(row) {
            for i in bytes_per_pixel..line.len() {
                line[i] = line[i].wrapping_add(line[i - bytes_per_pixel]);
            }
        }
        return Ok(out);
    }

    let mut out: Vec<u8> = Vec::with_capacity(data.len());
    let mut previous = vec![0u8; row];
    for chunk in data.chunks(row + 1) {
        let kind = chunk[0];
        let mut line = chunk[1..].to_vec();
        line.resize(row, 0);
        for i in 0..row {
            let left = if i >= bytes_per_pixel {
                line[i - bytes_per_pixel]
            } else {
                0
            };
            let up = previous[i];
            let up_left = if i >= bytes_per_pixel {
                previous[i - bytes_per_pixel]
            } else {
                0
            };
            let add = match kind {
                1 => left,
                2 => up,
                3 => u16::midpoint(u16::from(left), u16::from(up)) as u8,
                4 => paeth(left, up, up_left),
                _ => 0,
            };
            line[i] = line[i].wrapping_add(add);
        }
        out.extend_from_slice(&line);
        previous = line;
    }
    Ok(out)
}

fn paeth(a: u8, b: u8, c: u8) -> u8 {
    let (p, pa, pb, pc) = {
        let p = i32::from(a) + i32::from(b) - i32::from(c);
        (
            p,
            (p - i32::from(a)).abs(),
            (p - i32::from(b)).abs(),
            (p - i32::from(c)).abs(),
        )
    };
    let _ = p;
    if pa <= pb && pa <= pc {
        a
    } else if pb <= pc {
        b
    } else {
        c
    }
}

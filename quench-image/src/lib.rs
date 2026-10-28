//! Image decoding with no dependencies.
//!
//! [`decode`] reads a PNG (every colour type and bit depth, interlaced or not, palettes and transparency) into
//! straight-alpha RGBA; [`encode_png`] writes one. JPEG and GIF are not read: [`decode`] says so by name.

// Pixel and channel arithmetic: narrowing casts are of values that fit, and r, g, b, a are the names of the channels.
#![allow(
    clippy::cast_possible_truncation,
    clippy::many_single_char_names,
    clippy::missing_const_for_fn
)]

use std::fmt;

mod gif;
mod jpeg;
mod vp8;
mod vp8_tables;
mod webp;

use quench_archive::{checksum, deflate};

/// A decoded image: `width * height` pixels of RGBA, straight (not premultiplied) alpha.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Why an image could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// Not a format this crate reads (the name of what it looks like, when known).
    Unsupported(&'static str),
    /// An image that is damaged or hostile.
    Corrupt(&'static str),
    /// Bigger than the limits (16384 pixels a side, 64 million pixels).
    TooLarge,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported(what) => write!(f, "{what} images are not supported"),
            Self::Corrupt(why) => write!(f, "damaged image: {why}"),
            Self::TooLarge => f.write_str("the image is too large"),
        }
    }
}

impl std::error::Error for Error {}

const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
const MAX_SIDE: u32 = 16384;
const MAX_PIXELS: u64 = 64 << 20;

/// Reads an image, by what its first bytes say it is.
///
/// # Errors
/// [`Error::Unsupported`] for a format that is not PNG, [`Error::Corrupt`] or [`Error::TooLarge`] for a PNG that
/// is damaged or too big.
pub fn decode(bytes: &[u8]) -> Result<Image, Error> {
    if bytes.starts_with(&SIGNATURE) {
        return decode_png(bytes);
    }
    if bytes.starts_with(&[0xff, 0xd8]) {
        return jpeg::decode(bytes);
    }
    if bytes.starts_with(b"GIF8") {
        return gif::decode(bytes);
    }
    if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        return webp::decode(bytes);
    }
    Err(Error::Unsupported(
        if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
            "WebP"
        } else if bytes.starts_with(b"<svg") || bytes.starts_with(b"<?xml") {
            "SVG"
        } else {
            "unknown"
        },
    ))
}

fn be32(b: &[u8]) -> u32 {
    u32::from_be_bytes([b[0], b[1], b[2], b[3]])
}

/// The seven passes of Adam7: starting column and row, and the steps between pixels.
const ADAM7: [(usize, usize, usize, usize); 7] = [
    (0, 0, 8, 8),
    (4, 0, 8, 8),
    (0, 4, 4, 8),
    (2, 0, 4, 4),
    (0, 2, 2, 4),
    (1, 0, 2, 2),
    (0, 1, 1, 2),
];

struct Header {
    width: usize,
    height: usize,
    depth: usize,
    color: u8,
    interlaced: bool,
}

impl Header {
    const fn channels(&self) -> usize {
        match self.color {
            0 | 3 => 1,
            2 => 3,
            4 => 2,
            _ => 4,
        }
    }

    /// Bytes per pixel for filtering (at least 1).
    const fn bpp(&self) -> usize {
        (self.depth * self.channels()).div_ceil(8)
    }

    const fn row_bytes(&self, width: usize) -> usize {
        (width * self.depth * self.channels()).div_ceil(8)
    }
}

#[allow(
    clippy::too_many_lines,
    reason = "the chunks, then the data, then the pixels: one pass through the format"
)]
fn decode_png(bytes: &[u8]) -> Result<Image, Error> {
    let mut at = SIGNATURE.len();
    let mut header: Option<Header> = None;
    let mut palette: Vec<[u8; 3]> = Vec::new();
    let mut transparency: Vec<u8> = Vec::new();
    let mut compressed: Vec<u8> = Vec::new();
    let mut ended = false;
    while at + 12 <= bytes.len() {
        let length = be32(&bytes[at..]) as usize;
        let kind = &bytes[at + 4..at + 8];
        let end = at
            .checked_add(8 + length)
            .and_then(|e| e.checked_add(4))
            .filter(|e| *e <= bytes.len())
            .ok_or(Error::Corrupt("a chunk runs past the end"))?;
        let data = &bytes[at + 8..at + 8 + length];
        if checksum::crc32(&bytes[at + 4..at + 8 + length]) != be32(&bytes[at + 8 + length..]) {
            return Err(Error::Corrupt("a chunk's checksum is wrong"));
        }
        match kind {
            b"IHDR" => {
                if length != 13 {
                    return Err(Error::Corrupt("the header has the wrong size"));
                }
                let (w, h) = (be32(data), be32(&data[4..]));
                if w == 0 || h == 0 {
                    return Err(Error::Corrupt("an image with no pixels"));
                }
                if w > MAX_SIDE || h > MAX_SIDE || u64::from(w) * u64::from(h) > MAX_PIXELS {
                    return Err(Error::TooLarge);
                }
                let (depth, color) = (usize::from(data[8]), data[9]);
                let ok = matches!(
                    (color, depth),
                    (0, 1 | 2 | 4 | 8 | 16) | (2 | 4 | 6, 8 | 16) | (3, 1 | 2 | 4 | 8)
                );
                if !ok || data[10] != 0 || data[11] != 0 || data[12] > 1 {
                    return Err(Error::Corrupt(
                        "a colour type, depth or method that PNG does not have",
                    ));
                }
                header = Some(Header {
                    width: w as usize,
                    height: h as usize,
                    depth,
                    color,
                    interlaced: data[12] == 1,
                });
            }
            b"PLTE" => {
                if !length.is_multiple_of(3) || length > 768 {
                    return Err(Error::Corrupt("a palette of the wrong size"));
                }
                palette = data
                    .as_chunks::<3>()
                    .0
                    .iter()
                    .map(|c| [c[0], c[1], c[2]])
                    .collect();
            }
            b"tRNS" => transparency = data.to_vec(),
            b"IDAT" => compressed.extend_from_slice(data),
            b"IEND" => {
                ended = true;
                break;
            }
            _ => {}
        }
        at = end;
    }
    let header = header.ok_or(Error::Corrupt("no header"))?;
    if !ended || compressed.len() < 6 {
        return Err(Error::Corrupt("the image data is missing"));
    }
    if header.color == 3 && palette.is_empty() {
        return Err(Error::Corrupt("a palette image with no palette"));
    }
    // zlib: two header bytes, the deflate stream, four bytes of checksum.
    if compressed[0] & 0x0f != 8
        || (u16::from(compressed[0]) << 8 | u16::from(compressed[1])) % 31 != 0
    {
        return Err(Error::Corrupt("the compressed data is not zlib"));
    }
    let expected = expected_size(&header);
    let raw = deflate::decompress(&compressed[2..], expected)
        .map_err(|_| Error::Corrupt("the compressed data is damaged"))?;
    if raw.len() != expected {
        return Err(Error::Corrupt("the image data is the wrong size"));
    }
    let mut rgba = vec![0u8; header.width * header.height * 4];
    if header.interlaced {
        let mut offset = 0;
        for (x0, y0, dx, dy) in ADAM7 {
            let (w, h) = (
                (header.width.saturating_sub(x0)).div_ceil(dx),
                (header.height.saturating_sub(y0)).div_ceil(dy),
            );
            if w == 0 || h == 0 {
                continue;
            }
            let size = (header.row_bytes(w) + 1) * h;
            let pixels = unfilter(&header, w, h, &raw[offset..offset + size])?;
            offset += size;
            for y in 0..h {
                for x in 0..w {
                    let px = pixel(
                        &header,
                        &pixels[y * header.row_bytes(w)..],
                        x,
                        &palette,
                        &transparency,
                    );
                    let at = ((y0 + y * dy) * header.width + x0 + x * dx) * 4;
                    rgba[at..at + 4].copy_from_slice(&px);
                }
            }
        }
    } else {
        let pixels = unfilter(&header, header.width, header.height, &raw)?;
        let row = header.row_bytes(header.width);
        for y in 0..header.height {
            for x in 0..header.width {
                let px = pixel(&header, &pixels[y * row..], x, &palette, &transparency);
                let at = (y * header.width + x) * 4;
                rgba[at..at + 4].copy_from_slice(&px);
            }
        }
    }
    Ok(Image {
        width: header.width as u32,
        height: header.height as u32,
        rgba,
    })
}

fn expected_size(h: &Header) -> usize {
    if h.interlaced {
        ADAM7
            .iter()
            .map(|(x0, y0, dx, dy)| {
                let (w, ht) = (
                    h.width.saturating_sub(*x0).div_ceil(*dx),
                    h.height.saturating_sub(*y0).div_ceil(*dy),
                );
                if w == 0 || ht == 0 {
                    0
                } else {
                    (h.row_bytes(w) + 1) * ht
                }
            })
            .sum()
    } else {
        (h.row_bytes(h.width) + 1) * h.height
    }
}

/// Undoes the per-row filters of one (sub)image: the rows without their filter bytes.
fn unfilter(h: &Header, width: usize, height: usize, raw: &[u8]) -> Result<Vec<u8>, Error> {
    let row = h.row_bytes(width);
    let bpp = h.bpp();
    let mut out = vec![0u8; row * height];
    for y in 0..height {
        let line = &raw[y * (row + 1)..(y + 1) * (row + 1)];
        let filter = line[0];
        let (before, now) = out.split_at_mut(y * row);
        let prev: &[u8] = if y == 0 {
            &[]
        } else {
            &before[(y - 1) * row..]
        };
        let cur = &mut now[..row];
        for i in 0..row {
            let x = line[i + 1];
            let a = if i >= bpp { cur[i - bpp] } else { 0 };
            let b = if y > 0 { prev[i] } else { 0 };
            let c = if y > 0 && i >= bpp { prev[i - bpp] } else { 0 };
            cur[i] = match filter {
                0 => x,
                1 => x.wrapping_add(a),
                2 => x.wrapping_add(b),
                3 => x.wrapping_add(u16::midpoint(u16::from(a), u16::from(b)) as u8),
                4 => x.wrapping_add(paeth(a, b, c)),
                _ => return Err(Error::Corrupt("an unknown row filter")),
            };
        }
    }
    Ok(out)
}

fn paeth(a: u8, b: u8, c: u8) -> u8 {
    let (ia, ib, ic) = (i32::from(a), i32::from(b), i32::from(c));
    let p = ia + ib - ic;
    let (pa, pb, pc) = ((p - ia).abs(), (p - ib).abs(), (p - ic).abs());
    if pa <= pb && pa <= pc {
        a
    } else if pb <= pc {
        b
    } else {
        c
    }
}

/// The RGBA of pixel `x` of a row.
fn pixel(h: &Header, row: &[u8], x: usize, palette: &[[u8; 3]], trns: &[u8]) -> [u8; 4] {
    // A sample of `depth` bits at index `i` of the row.
    let sample = |i: usize| -> u16 {
        match h.depth {
            8 => u16::from(row[i]),
            16 => u16::from(row[i * 2]) << 8 | u16::from(row[i * 2 + 1]),
            d => {
                let per = 8 / d;
                let byte = row[i / per];
                let shift = 8 - d * (i % per + 1);
                u16::from((byte >> shift) & ((1u8 << d) - 1))
            }
        }
    };
    let scale = |v: u16| -> u8 {
        match h.depth {
            16 => (v >> 8) as u8,
            8 => v as u8,
            d => (u32::from(v) * 255 / ((1u32 << d) - 1)) as u8,
        }
    };
    let key = |i: usize| {
        trns.get(i * 2..i * 2 + 2)
            .map(|b| u16::from(b[0]) << 8 | u16::from(b[1]))
    };
    match h.color {
        0 => {
            let v = sample(x);
            let g = scale(v);
            [g, g, g, if key(0) == Some(v) { 0 } else { 255 }]
        }
        2 => {
            let (r, g, b) = (sample(x * 3), sample(x * 3 + 1), sample(x * 3 + 2));
            let transparent = key(0) == Some(r) && key(1) == Some(g) && key(2) == Some(b);
            [
                scale(r),
                scale(g),
                scale(b),
                if transparent { 0 } else { 255 },
            ]
        }
        3 => {
            let i = usize::from(sample(x));
            let [r, g, b] = palette.get(i).copied().unwrap_or([0, 0, 0]);
            [r, g, b, trns.get(i).copied().unwrap_or(255)]
        }
        4 => {
            let g = scale(sample(x * 2));
            [g, g, g, scale(sample(x * 2 + 1))]
        }
        _ => [
            scale(sample(x * 4)),
            scale(sample(x * 4 + 1)),
            scale(sample(x * 4 + 2)),
            scale(sample(x * 4 + 3)),
        ],
    }
}

/// Writes `rgba` (`width * height * 4` bytes) as an 8-bit RGBA PNG.
///
/// # Panics
/// When `rgba` is not `width * height * 4` bytes long.
#[must_use]
pub fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    let row = width as usize * 4;
    assert_eq!(
        rgba.len(),
        row * height as usize,
        "an RGBA image is width * height * 4 bytes"
    );
    let mut raw = Vec::with_capacity((row + 1) * height as usize);
    for line in rgba.chunks_exact(row.max(1)) {
        raw.push(0);
        raw.extend_from_slice(line);
    }
    let mut zlib = vec![0x78, 0x9c];
    zlib.extend(deflate::compress(&raw));
    zlib.extend(checksum::adler32(&raw).to_be_bytes());
    let mut out = SIGNATURE.to_vec();
    let mut chunk = |kind: &[u8; 4], data: &[u8]| {
        out.extend((data.len() as u32).to_be_bytes());
        let mut body = kind.to_vec();
        body.extend_from_slice(data);
        out.extend_from_slice(&body);
        out.extend(checksum::crc32(&body).to_be_bytes());
    };
    let mut header = Vec::new();
    header.extend(width.to_be_bytes());
    header.extend(height.to_be_bytes());
    header.extend([8, 6, 0, 0, 0]);
    chunk(b"IHDR", &header);
    chunk(b"IDAT", &zlib);
    chunk(b"IEND", &[]);
    out
}

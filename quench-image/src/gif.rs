//! GIF decoding: the first frame, on the logical screen, with its transparent colour.

use crate::{Error, Image, MAX_PIXELS, MAX_SIDE};

fn le16(b: &[u8]) -> usize {
    usize::from(b[0]) | usize::from(b[1]) << 8
}

/// A cursor over the file.
struct Bytes<'a>(&'a [u8]);

impl<'a> Bytes<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], Error> {
        if self.0.len() < n {
            return Err(Error::Corrupt("the data is damaged"));
        }
        let (head, tail) = self.0.split_at(n);
        self.0 = tail;
        Ok(head)
    }

    fn byte(&mut self) -> Result<u8, Error> {
        Ok(self.take(1)?[0])
    }

    /// The data sub-blocks up to the empty one, joined.
    fn blocks(&mut self) -> Result<Vec<u8>, Error> {
        let mut out = Vec::new();
        loop {
            let n = usize::from(self.byte()?);
            if n == 0 {
                return Ok(out);
            }
            out.extend_from_slice(self.take(n)?);
        }
    }
}

/// Decodes the first image of a GIF file.
pub fn decode(file: &[u8]) -> Result<Image, Error> {
    let mut r = Bytes(file);
    let header = r.take(6)?;
    if header != b"GIF87a" && header != b"GIF89a" {
        return Err(Error::Corrupt("the data is damaged"));
    }
    let screen = r.take(7)?;
    let (width, height) = (le16(&screen[0..2]), le16(&screen[2..4]));
    if width == 0 || height == 0 {
        return Err(Error::Corrupt("the data is damaged"));
    }
    if width as u32 > MAX_SIDE || height as u32 > MAX_SIDE || (width * height) as u64 > MAX_PIXELS {
        return Err(Error::TooLarge);
    }
    let table = |r: &mut Bytes<'_>, flags: u8| -> Result<Vec<[u8; 3]>, Error> {
        if flags & 0x80 == 0 {
            return Ok(Vec::new());
        }
        let n = 1usize << ((flags & 7) + 1);
        Ok(r.take(n * 3)?
            .chunks(3)
            .map(|c| [c[0], c[1], c[2]])
            .collect())
    };
    let global = table(&mut r, screen[4])?;
    let mut transparent: Option<u8> = None;
    loop {
        match r.byte()? {
            0x21 => {
                let label = r.byte()?;
                let data = r.blocks()?;
                if label == 0xf9 && data.len() >= 4 && data[0] & 1 != 0 {
                    transparent = Some(data[3]);
                }
            }
            0x2c => {
                let d = r.take(9)?;
                let (left, top, w, h) = (
                    le16(&d[0..2]),
                    le16(&d[2..4]),
                    le16(&d[4..6]),
                    le16(&d[6..8]),
                );
                let flags = d[8];
                let local = table(&mut r, flags)?;
                let palette = if local.is_empty() { &global } else { &local };
                let min_code = r.byte()?;
                let data = r.blocks()?;
                let indices = lzw(&data, min_code, w * h)?;
                let mut rgba = vec![0u8; width * height * 4];
                // Interlaced rows come in four passes.
                let rows: Vec<usize> = if flags & 0x40 != 0 {
                    [(0, 8), (4, 8), (2, 4), (1, 2)]
                        .iter()
                        .flat_map(|(start, step)| (*start..h).step_by(*step))
                        .collect()
                } else {
                    (0..h).collect()
                };
                for (n, row) in rows.into_iter().enumerate() {
                    for x in 0..w {
                        let index = indices[n * w + x];
                        let (px, py) = (left + x, top + row);
                        if Some(index) == transparent || px >= width || py >= height {
                            continue;
                        }
                        let Some(color) = palette.get(usize::from(index)) else {
                            return Err(Error::Corrupt("the data is damaged"));
                        };
                        let at = (py * width + px) * 4;
                        rgba[at..at + 4].copy_from_slice(&[color[0], color[1], color[2], 255]);
                    }
                }
                return Ok(Image {
                    width: width as u32,
                    height: height as u32,
                    rgba,
                });
            }
            _ => return Err(Error::Corrupt("the data is damaged")),
        }
    }
}

/// Variable-length LZW as GIF uses it: `count` indices out of `data`.
fn lzw(data: &[u8], min_code: u8, count: usize) -> Result<Vec<u8>, Error> {
    if !(1..=11).contains(&min_code) {
        return Err(Error::Corrupt("the data is damaged"));
    }
    let clear = 1u16 << min_code;
    let end = clear + 1;
    let mut prefix = vec![0u16; 4096];
    let mut suffix = vec![0u8; 4096];
    let mut length = vec![0u16; 4096];
    for i in 0..clear {
        suffix[usize::from(i)] = i as u8;
        length[usize::from(i)] = 1;
    }
    let mut out = Vec::with_capacity(count);
    let (mut size, mut next, mut previous) = (u32::from(min_code) + 1, end + 1, None::<u16>);
    let (mut bits, mut have, mut at) = (0u32, 0u32, 0usize);
    // Writes the string of `code` at the end of `out`.
    let emit =
        |out: &mut Vec<u8>, code: u16, prefix: &[u16], suffix: &[u8], length: &[u16]| -> u8 {
            let n = usize::from(length[usize::from(code)]);
            let from = out.len();
            out.resize(from + n, 0);
            let mut c = code;
            for slot in out[from..].iter_mut().rev() {
                *slot = suffix[usize::from(c)];
                c = prefix[usize::from(c)];
            }
            out[from]
        };
    while out.len() < count {
        while have < size {
            let Some(byte) = data.get(at) else {
                // The data ended early: what there is is the picture.
                out.resize(count, 0);
                return Ok(out);
            };
            bits |= u32::from(*byte) << have;
            have += 8;
            at += 1;
        }
        let code = (bits & ((1 << size) - 1)) as u16;
        bits >>= size;
        have -= size;
        if code == clear {
            (size, next, previous) = (u32::from(min_code) + 1, end + 1, None);
            continue;
        }
        if code == end {
            break;
        }
        let Some(prev) = previous else {
            // The first code after a clear stands for itself and defines nothing.
            if code >= clear {
                return Err(Error::Corrupt("the data is damaged"));
            }
            emit(&mut out, code, &prefix, &suffix, &length);
            previous = Some(code);
            continue;
        };
        #[allow(
            clippy::comparison_chain,
            reason = "three cases of different kinds, not an ordering"
        )]
        let first = if code < next {
            emit(&mut out, code, &prefix, &suffix, &length)
        } else if code == next {
            // The string that is being defined: the previous one and its own first byte.
            let start = out.len();
            emit(&mut out, prev, &prefix, &suffix, &length);
            let head = out[start];
            out.push(head);
            head
        } else {
            return Err(Error::Corrupt("the data is damaged"));
        };
        if next < 4096 {
            prefix[usize::from(next)] = prev;
            suffix[usize::from(next)] = first;
            length[usize::from(next)] = length[usize::from(prev)] + 1;
            next += 1;
            if next == (1 << size) && size < 12 {
                size += 1;
            }
        }
        previous = Some(code);
    }
    out.resize(count, 0);
    Ok(out)
}

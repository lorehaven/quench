//! WebP: the RIFF container (plain, extended with alpha, and the first frame of an animation) and the lossless
//! VP8L codec. The lossy codec is in `vp8`.

// Pixel and bit arithmetic: values are small and bounded by the format before they are cast, the names are the
// specification's, and long functions follow its steps in order.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::too_many_lines,
    clippy::similar_names,
    clippy::many_single_char_names,
    clippy::useless_let_if_seq,
    clippy::trivially_copy_pass_by_ref,
    clippy::if_not_else,
    clippy::precedence,
    clippy::chunks_exact_to_as_chunks,
    clippy::needless_late_init
)]

use crate::{Error, Image, MAX_PIXELS, MAX_SIDE};

const fn corrupt() -> Error {
    Error::Corrupt("the data is damaged")
}

fn le24(b: &[u8]) -> usize {
    usize::from(b[0]) | usize::from(b[1]) << 8 | usize::from(b[2]) << 16
}

fn le32(b: &[u8]) -> usize {
    le24(b) | usize::from(b[3]) << 24
}

/// Decodes a WebP file (the bytes from `RIFF`).
pub fn decode(file: &[u8]) -> Result<Image, Error> {
    if file.len() < 20 || &file[8..12] != b"WEBP" {
        return Err(corrupt());
    }
    // The chunks inside the RIFF form (its length may not be trusted past the file).
    let end = (8 + le32(&file[4..8])).min(file.len());
    let mut chunks = Vec::new();
    let mut at = 12;
    while at + 8 <= end {
        let size = le32(&file[at + 4..at + 8]);
        let body = file.get(at + 8..at + 8 + size).ok_or_else(corrupt)?;
        chunks.push((&file[at..at + 4], body));
        at += 8 + size + (size & 1);
    }
    let find = |tag: &[u8; 4]| chunks.iter().find(|c| c.0 == tag).map(|c| c.1);
    if let Some(head) = find(b"VP8X") {
        if head.len() < 10 {
            return Err(corrupt());
        }
        let (width, height) = (le24(&head[4..7]) + 1, le24(&head[7..10]) + 1);
        check_size(width, height)?;
        if let Some(frame) = find(b"ANMF") {
            return first_frame(frame, width, height);
        }
        return still(&chunks, Some((width, height)));
    }
    still(&chunks, None)
}

fn check_size(width: usize, height: usize) -> Result<(), Error> {
    if width == 0
        || height == 0
        || width as u32 > MAX_SIDE
        || height as u32 > MAX_SIDE
        || (width * height) as u64 > MAX_PIXELS
    {
        return Err(Error::TooLarge);
    }
    Ok(())
}

/// A picture in the chunks: lossless, or lossy with its alpha.
fn still(chunks: &[(&[u8], &[u8])], canvas: Option<(usize, usize)>) -> Result<Image, Error> {
    let find = |tag: &[u8; 4]| chunks.iter().find(|c| c.0 == tag).map(|c| c.1);
    let image = if let Some(data) = find(b"VP8L") {
        decode_lossless(data)?
    } else if let Some(data) = find(b"VP8 ") {
        let mut image = crate::vp8::decode(data)?;
        if let Some(alpha) = find(b"ALPH") {
            apply_alpha(&mut image, alpha)?;
        }
        image
    } else {
        return Err(corrupt());
    };
    if let Some((w, h)) = canvas
        && (image.width as usize, image.height as usize) != (w, h)
    {
        return Err(corrupt());
    }
    Ok(image)
}

/// The first frame of an animation, on the canvas (frames are not blended or played).
fn first_frame(frame: &[u8], width: usize, height: usize) -> Result<Image, Error> {
    if frame.len() < 16 {
        return Err(corrupt());
    }
    let (x, y) = (le24(&frame[0..3]) * 2, le24(&frame[3..6]) * 2);
    let mut chunks = Vec::new();
    let mut at = 16;
    while at + 8 <= frame.len() {
        let size = le32(&frame[at + 4..at + 8]);
        let body = frame.get(at + 8..at + 8 + size).ok_or_else(corrupt)?;
        chunks.push((&frame[at..at + 4], body));
        at += 8 + size + (size & 1);
    }
    let part = still(&chunks, None)?;
    let mut rgba = vec![0u8; width * height * 4];
    for row in 0..part.height as usize {
        if y + row >= height || x >= width {
            break;
        }
        let take = (part.width as usize).min(width.saturating_sub(x));
        let from = row * part.width as usize * 4;
        let to = ((y + row) * width + x) * 4;
        rgba[to..to + take * 4].copy_from_slice(&part.rgba[from..from + take * 4]);
    }
    Ok(Image {
        width: width as u32,
        height: height as u32,
        rgba,
    })
}

/// Puts the alpha of an `ALPH` chunk into a lossy picture.
fn apply_alpha(image: &mut Image, chunk: &[u8]) -> Result<(), Error> {
    let (w, h) = (image.width as usize, image.height as usize);
    let (&header, data) = chunk.split_first().ok_or_else(corrupt)?;
    let (compression, filter) = (header & 3, (header >> 2) & 3);
    let mut alpha: Vec<u8> = match compression {
        0 => data.get(..w * h).ok_or_else(corrupt)?.to_vec(),
        1 => {
            let mut bits = Bits::new(data);
            let argb = decode_image_stream(&mut bits, w, h)?;
            argb.iter().map(|p| (p >> 8) as u8).collect()
        }
        _ => return Err(corrupt()),
    };
    // The filters predict each value from its neighbours; the stored value is the difference.
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            let above = (y > 0).then(|| alpha[i - w]);
            let left = (x > 0).then(|| alpha[i - 1]);
            let predicted = match filter {
                1 => left.or(above),
                2 => above.or(left),
                3 => match (left, above) {
                    (Some(l), Some(t)) => {
                        let g = i32::from(l) + i32::from(t) - i32::from(alpha[i - w - 1]);
                        Some(g.clamp(0, 255) as u8)
                    }
                    (l, t) => l.or(t),
                },
                _ => None,
            };
            alpha[i] = alpha[i].wrapping_add(predicted.unwrap_or(0));
        }
    }
    for (px, a) in image.rgba.chunks_exact_mut(4).zip(alpha) {
        px[3] = a;
    }
    Ok(())
}

// ---- the lossless codec ---------------------------------------------------------------------------------------

/// Bits, least significant first.
struct Bits<'a> {
    data: &'a [u8],
    at: usize,
    buffer: u64,
    count: u32,
}

impl<'a> Bits<'a> {
    const fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            at: 0,
            buffer: 0,
            count: 0,
        }
    }

    fn fill(&mut self) {
        while self.count <= 56 && self.at < self.data.len() {
            self.buffer |= u64::from(self.data[self.at]) << self.count;
            self.at += 1;
            self.count += 8;
        }
    }

    /// The next `n` (at most 32) bits; past the end it reads zeros, and `exhausted` tells.
    fn read(&mut self, n: u32) -> u32 {
        self.fill();
        let value = (self.buffer & ((1u64 << n) - 1)) as u32;
        self.buffer >>= n;
        self.count = self.count.saturating_sub(n);
        value
    }

    fn bit(&mut self) -> bool {
        self.read(1) == 1
    }

    /// Whether more bits were asked for than the data has.
    const fn exhausted(&self) -> bool {
        self.at >= self.data.len() && self.count == 0
    }
}

/// A prefix code: canonical, as deflate's.
#[derive(Default, Clone)]
struct Code {
    counts: [u16; 16],
    symbols: Vec<u16>,
    /// Set for a code with one symbol, which takes no bits.
    single: Option<u16>,
}

impl Code {
    fn new(lengths: &[u8]) -> Result<Self, Error> {
        let used: Vec<usize> = (0..lengths.len()).filter(|i| lengths[*i] > 0).collect();
        if used.is_empty() {
            return Err(corrupt());
        }
        if used.len() == 1 {
            return Ok(Self {
                single: Some(used[0] as u16),
                ..Self::default()
            });
        }
        let mut code = Self::default();
        for &l in lengths {
            if l > 15 {
                return Err(corrupt());
            }
            code.counts[usize::from(l)] += 1;
        }
        code.counts[0] = 0;
        // The code must be complete: every bit pattern is some symbol.
        let mut left = 1i32;
        for len in 1..16 {
            left = (left << 1) - i32::from(code.counts[len]);
            if left < 0 {
                return Err(corrupt());
            }
        }
        if left != 0 {
            return Err(corrupt());
        }
        let mut offsets = [0u16; 17];
        for len in 1..16 {
            offsets[len + 1] = offsets[len] + code.counts[len];
        }
        code.symbols = vec![0; used.len()];
        for &symbol in &used {
            let l = usize::from(lengths[symbol]);
            code.symbols[usize::from(offsets[l])] = symbol as u16;
            offsets[l] += 1;
        }
        Ok(code)
    }

    fn decode(&self, bits: &mut Bits<'_>) -> Result<u16, Error> {
        if let Some(s) = self.single {
            return Ok(s);
        }
        let (mut code, mut first, mut index) = (0i32, 0i32, 0i32);
        for len in 1..16 {
            code |= i32::from(bits.bit());
            let count = i32::from(self.counts[len]);
            if code - count < first {
                return Ok(self.symbols[(index + (code - first)) as usize]);
            }
            index += count;
            first = (first + count) << 1;
            code <<= 1;
        }
        Err(corrupt())
    }
}

/// How many pixels `size` pixels become with `bits` of sub-sampling.
const fn sub_size(size: usize, bits: u32) -> usize {
    (size + (1 << bits) - 1) >> bits
}

const LENGTH_ORDER: [usize; 19] = [
    17, 18, 0, 1, 2, 3, 4, 5, 16, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15,
];

fn read_code(bits: &mut Bits<'_>, alphabet: usize) -> Result<Code, Error> {
    let mut lengths = vec![0u8; alphabet];
    if bits.bit() {
        // A simple code: one or two symbols.
        let count = bits.read(1) + 1;
        let first_is_8 = bits.bit();
        let first = bits.read(if first_is_8 { 8 } else { 1 }) as usize;
        if first >= alphabet {
            return Err(corrupt());
        }
        lengths[first] = 1;
        if count == 2 {
            let second = bits.read(8) as usize;
            if second >= alphabet {
                return Err(corrupt());
            }
            lengths[second] = 1;
        }
        return Code::new(&lengths);
    }
    let wanted = 4 + bits.read(4) as usize;
    let mut meta = [0u8; 19];
    for &slot in LENGTH_ORDER.iter().take(wanted) {
        meta[slot] = bits.read(3) as u8;
    }
    let meta_code = Code::new(&meta)?;
    let mut budget = if bits.bit() {
        let n = 2 + 2 * bits.read(3);
        2 + bits.read(n) as usize
    } else {
        alphabet
    };
    let (mut i, mut previous) = (0usize, 8u8);
    while i < alphabet && budget > 0 {
        budget -= 1;
        let symbol = meta_code.decode(bits)?;
        if symbol < 16 {
            lengths[i] = symbol as u8;
            i += 1;
            if symbol != 0 {
                previous = symbol as u8;
            }
        } else {
            let (value, repeat) = match symbol {
                16 => (previous, 3 + bits.read(2) as usize),
                17 => (0, 3 + bits.read(3) as usize),
                _ => (0, 11 + bits.read(7) as usize),
            };
            if i + repeat > alphabet {
                return Err(corrupt());
            }
            lengths[i..i + repeat].fill(value);
            i += repeat;
        }
        if bits.exhausted() {
            return Err(corrupt());
        }
    }
    Code::new(&lengths)
}

/// Offsets of the 120 shortest backward distances, as (x, y) in pixels back and up.
const DISTANCES: [(i8, i8); 120] = [
    (0, 1),
    (1, 0),
    (1, 1),
    (-1, 1),
    (0, 2),
    (2, 0),
    (1, 2),
    (-1, 2),
    (2, 1),
    (-2, 1),
    (2, 2),
    (-2, 2),
    (0, 3),
    (3, 0),
    (1, 3),
    (-1, 3),
    (3, 1),
    (-3, 1),
    (2, 3),
    (-2, 3),
    (3, 2),
    (-3, 2),
    (0, 4),
    (4, 0),
    (1, 4),
    (-1, 4),
    (4, 1),
    (-4, 1),
    (3, 3),
    (-3, 3),
    (2, 4),
    (-2, 4),
    (4, 2),
    (-4, 2),
    (0, 5),
    (3, 4),
    (-3, 4),
    (4, 3),
    (-4, 3),
    (5, 0),
    (1, 5),
    (-1, 5),
    (5, 1),
    (-5, 1),
    (2, 5),
    (-2, 5),
    (5, 2),
    (-5, 2),
    (4, 4),
    (-4, 4),
    (3, 5),
    (-3, 5),
    (5, 3),
    (-5, 3),
    (0, 6),
    (6, 0),
    (1, 6),
    (-1, 6),
    (6, 1),
    (-6, 1),
    (2, 6),
    (-2, 6),
    (6, 2),
    (-6, 2),
    (4, 5),
    (-4, 5),
    (5, 4),
    (-5, 4),
    (3, 6),
    (-3, 6),
    (6, 3),
    (-6, 3),
    (0, 7),
    (7, 0),
    (1, 7),
    (-1, 7),
    (5, 5),
    (-5, 5),
    (7, 1),
    (-7, 1),
    (4, 6),
    (-4, 6),
    (6, 4),
    (-6, 4),
    (2, 7),
    (-2, 7),
    (7, 2),
    (-7, 2),
    (3, 7),
    (-3, 7),
    (7, 3),
    (-7, 3),
    (5, 6),
    (-5, 6),
    (6, 5),
    (-6, 5),
    (8, 0),
    (4, 7),
    (-4, 7),
    (7, 4),
    (-7, 4),
    (8, 1),
    (8, 2),
    (6, 6),
    (-6, 6),
    (8, 3),
    (5, 7),
    (-5, 7),
    (7, 5),
    (-7, 5),
    (8, 4),
    (6, 7),
    (-6, 7),
    (7, 6),
    (-7, 6),
    (8, 5),
    (7, 7),
    (-7, 7),
    (8, 6),
    (8, 7),
];

/// A length or distance from its prefix code and the extra bits that follow.
fn lz77_value(bits: &mut Bits<'_>, prefix: usize) -> usize {
    if prefix < 4 {
        return prefix + 1;
    }
    let extra = ((prefix - 2) >> 1) as u32;
    let offset = (2 + (prefix & 1)) << extra;
    offset + bits.read(extra) as usize + 1
}

/// The five codes of one group.
struct Group {
    green: Code,
    red: Code,
    blue: Code,
    alpha: Code,
    distance: Code,
}

/// Reads an entropy-coded image of `w` x `h` (the colour cache, the meta prefix codes when allowed, the codes
/// and the pixels).
fn decode_entropy(bits: &mut Bits<'_>, w: usize, h: usize, main: bool) -> Result<Vec<u32>, Error> {
    let cache_bits = if bits.bit() {
        let n = bits.read(4);
        if !(1..=11).contains(&n) {
            return Err(corrupt());
        }
        n
    } else {
        0
    };
    let mut tile_bits = 0;
    let mut tiles: Vec<u32> = Vec::new();
    let mut groups = 1usize;
    if main && bits.bit() {
        tile_bits = bits.read(3) + 2;
        let entropy = decode_entropy(bits, sub_size(w, tile_bits), sub_size(h, tile_bits), false)?;
        tiles = entropy.iter().map(|p| (p >> 8) & 0xffff).collect();
        groups = tiles.iter().copied().max().unwrap_or(0) as usize + 1;
    }
    let cache_size = if cache_bits > 0 {
        1usize << cache_bits
    } else {
        0
    };
    let mut codes = Vec::with_capacity(groups);
    for _ in 0..groups {
        codes.push(Group {
            green: read_code(bits, 256 + 24 + cache_size)?,
            red: read_code(bits, 256)?,
            blue: read_code(bits, 256)?,
            alpha: read_code(bits, 256)?,
            distance: read_code(bits, 40)?,
        });
    }
    let total = w * h;
    let mut pixels: Vec<u32> = Vec::with_capacity(total);
    let mut cache = vec![0u32; cache_size];
    let insert = |cache: &mut Vec<u32>, p: u32| {
        if cache_bits > 0 {
            cache[(p.wrapping_mul(0x1e35_a7bd) >> (32 - cache_bits)) as usize] = p;
        }
    };
    while pixels.len() < total {
        if bits.exhausted() {
            return Err(corrupt());
        }
        let i = pixels.len();
        let group = if tiles.is_empty() {
            0
        } else {
            tiles[(i / w >> tile_bits) * sub_size(w, tile_bits) + ((i % w) >> tile_bits)] as usize
        };
        let g = &codes[group];
        let s = usize::from(g.green.decode(bits)?);
        if s < 256 {
            let red = u32::from(g.red.decode(bits)?);
            let blue = u32::from(g.blue.decode(bits)?);
            let alpha = u32::from(g.alpha.decode(bits)?);
            let p = alpha << 24 | red << 16 | (s as u32) << 8 | blue;
            pixels.push(p);
            insert(&mut cache, p);
        } else if s < 280 {
            let length = lz77_value(bits, s - 256);
            let prefix = usize::from(g.distance.decode(bits)?);
            let code = lz77_value(bits, prefix);
            let distance = if code > 120 {
                code - 120
            } else {
                let (dx, dy) = DISTANCES[code - 1];
                (i32::from(dx) + i32::from(dy) * w as i32).max(1) as usize
            };
            if distance > i || i + length > total {
                return Err(corrupt());
            }
            for k in 0..length {
                let p = pixels[i + k - distance];
                pixels.push(p);
                insert(&mut cache, p);
            }
        } else {
            let key = s - 280;
            if key >= cache.len() {
                return Err(corrupt());
            }
            let p = cache[key];
            pixels.push(p);
            insert(&mut cache, p);
        }
    }
    Ok(pixels)
}

/// A transform read from the stream, with the width it applies to.
enum Transform {
    Predictor {
        bits: u32,
        data: Vec<u32>,
        width: usize,
    },
    Color {
        bits: u32,
        data: Vec<u32>,
        width: usize,
    },
    SubtractGreen,
    Indexing {
        table: Vec<u32>,
        bits: u32,
        width: usize,
    },
}

/// Decodes an image stream without the file header: the transforms, then the pixels.
fn decode_image_stream(bits: &mut Bits<'_>, w: usize, h: usize) -> Result<Vec<u32>, Error> {
    let mut transforms: Vec<Transform> = Vec::new();
    let mut width = w;
    let mut seen = [false; 4];
    while bits.bit() {
        let kind = bits.read(2) as usize;
        if std::mem::replace(&mut seen[kind], true) {
            return Err(corrupt());
        }
        transforms.push(match kind {
            0 | 1 => {
                let block_bits = bits.read(3) + 2;
                let data = decode_entropy(
                    bits,
                    sub_size(width, block_bits),
                    sub_size(h, block_bits),
                    false,
                )?;
                if kind == 0 {
                    Transform::Predictor {
                        bits: block_bits,
                        data,
                        width,
                    }
                } else {
                    Transform::Color {
                        bits: block_bits,
                        data,
                        width,
                    }
                }
            }
            2 => Transform::SubtractGreen,
            _ => {
                let size = bits.read(8) as usize + 1;
                let mut table = decode_entropy(bits, size, 1, false)?;
                // The table is stored as differences from the colour before.
                for i in 1..table.len() {
                    table[i] = add_pixels(table[i], table[i - 1]);
                }
                let pack = match size {
                    0..=2 => 3,
                    3..=4 => 2,
                    5..=16 => 1,
                    _ => 0,
                };
                let before = width;
                width = sub_size(width, pack);
                Transform::Indexing {
                    table,
                    bits: pack,
                    width: before,
                }
            }
        });
    }
    let mut pixels = decode_entropy(bits, width, h, true)?;
    for transform in transforms.iter().rev() {
        pixels = match transform {
            Transform::SubtractGreen => {
                for p in &mut pixels {
                    let green = (*p >> 8) & 0xff;
                    let red = (((*p >> 16) & 0xff) + green) & 0xff;
                    let blue = ((*p & 0xff) + green) & 0xff;
                    *p = (*p & 0xff00_ff00) | red << 16 | blue;
                }
                pixels
            }
            Transform::Color {
                bits: b,
                data,
                width,
            } => {
                let tile_w = sub_size(*width, *b);
                for y in 0..h {
                    for x in 0..*width {
                        let code = data[(y >> b) * tile_w + (x >> b)];
                        let p = &mut pixels[y * width + x];
                        let (green, mut red, mut blue) =
                            ((*p >> 8) as u8, (*p >> 16) as u8, *p as u8);
                        let delta =
                            |t: u8, c: u8| ((i32::from(t as i8) * i32::from(c as i8)) >> 5) as u8;
                        red = red.wrapping_add(delta(code as u8, green));
                        blue = blue.wrapping_add(delta((code >> 8) as u8, green));
                        blue = blue.wrapping_add(delta((code >> 16) as u8, red));
                        *p = (*p & 0xff00_ff00) | u32::from(red) << 16 | u32::from(blue);
                    }
                }
                pixels
            }
            Transform::Predictor {
                bits: b,
                data,
                width,
            } => {
                predict(&mut pixels, *width, h, *b, data);
                pixels
            }
            Transform::Indexing {
                table,
                bits: b,
                width,
            } => {
                let mut out = vec![0u32; width * h];
                let packed = sub_size(*width, *b);
                let per = 1usize << b;
                let depth = 8 >> b;
                for y in 0..h {
                    for x in 0..*width {
                        let source = pixels[y * packed + x / per];
                        let index = if *b == 0 {
                            ((source >> 8) & 0xff) as usize
                        } else {
                            (((source >> 8) & 0xff) >> (depth * (x % per))) as usize
                                & ((1 << depth) - 1)
                        };
                        // An index past the table is transparent black.
                        out[y * width + x] = table.get(index).copied().unwrap_or(0);
                    }
                }
                out
            }
        };
    }
    Ok(pixels)
}

fn add_pixels(a: u32, b: u32) -> u32 {
    let ag = (a & 0xff00_ff00).wrapping_add(b & 0xff00_ff00) & 0xff00_ff00;
    let rb = (a & 0x00ff_00ff).wrapping_add(b & 0x00ff_00ff) & 0x00ff_00ff;
    ag | rb
}

fn average2(a: u32, b: u32) -> u32 {
    (((a ^ b) & 0xfefe_fefe) >> 1).wrapping_add(a & b)
}

fn select(l: u32, t: u32, tl: u32) -> u32 {
    let distance = |a: u32, b: u32| -> i32 {
        (0..4)
            .map(|k| (i32::from((a >> (8 * k)) as u8) - i32::from((b >> (8 * k)) as u8)).abs())
            .sum()
    };
    // p = L + T - TL, so |p - L| is |T - TL| and |p - T| is |L - TL|.
    if distance(t, tl) < distance(l, tl) {
        l
    } else {
        t
    }
}

fn clamp_add_subtract_full(a: u32, b: u32, c: u32) -> u32 {
    (0..4).fold(0, |out, k| {
        let v = i32::from((a >> (8 * k)) as u8) + i32::from((b >> (8 * k)) as u8)
            - i32::from((c >> (8 * k)) as u8);
        out | (v.clamp(0, 255) as u32) << (8 * k)
    })
}

fn clamp_add_subtract_half(a: u32, b: u32) -> u32 {
    (0..4).fold(0, |out, k| {
        let (x, y) = (
            i32::from((a >> (8 * k)) as u8),
            i32::from((b >> (8 * k)) as u8),
        );
        out | ((x + (x - y) / 2).clamp(0, 255) as u32) << (8 * k)
    })
}

/// Undoes the predictor transform in place.
fn predict(pixels: &mut [u32], w: usize, h: usize, bits: u32, modes: &[u32]) {
    let tile_w = sub_size(w, bits);
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            let predicted = if y == 0 && x == 0 {
                0xff00_0000
            } else if y == 0 {
                pixels[i - 1]
            } else if x == 0 {
                pixels[i - w]
            } else {
                let (l, t, tl) = (pixels[i - 1], pixels[i - w], pixels[i - w - 1]);
                // The pixel up and to the right; on the last column it is the first of this row.
                let tr = pixels[i - w + 1];
                match (modes[(y >> bits) * tile_w + (x >> bits)] >> 8) & 0xf {
                    1 => l,
                    2 => t,
                    3 => tr,
                    4 => tl,
                    5 => average2(average2(l, tr), t),
                    6 => average2(l, tl),
                    7 => average2(l, t),
                    8 => average2(tl, t),
                    9 => average2(t, tr),
                    10 => average2(average2(l, tl), average2(t, tr)),
                    11 => select(l, t, tl),
                    12 => clamp_add_subtract_full(l, t, tl),
                    13 => clamp_add_subtract_half(average2(l, t), tl),
                    // Mode 0, and the two codes the format leaves unused.
                    _ => 0xff00_0000,
                }
            };
            pixels[i] = add_pixels(pixels[i], predicted);
        }
    }
}

/// Decodes the data of a `VP8L` chunk.
fn decode_lossless(data: &[u8]) -> Result<Image, Error> {
    if data.len() < 5 || data[0] != 0x2f {
        return Err(corrupt());
    }
    let mut bits = Bits::new(&data[1..]);
    let (width, height) = (bits.read(14) as usize + 1, bits.read(14) as usize + 1);
    let alpha_used = bits.bit();
    if bits.read(3) != 0 {
        return Err(corrupt());
    }
    check_size(width, height)?;
    let pixels = decode_image_stream(&mut bits, width, height)?;
    let mut rgba = Vec::with_capacity(width * height * 4);
    for p in pixels {
        let alpha = if alpha_used { (p >> 24) as u8 } else { 255 };
        rgba.extend_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, p as u8, alpha]);
    }
    Ok(Image {
        width: width as u32,
        height: height as u32,
        rgba,
    })
}

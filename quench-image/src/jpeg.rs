//! JPEG decoding: baseline and progressive Huffman, 8 bits, grey or YCbCr (or RGB marked by Adobe).
//!
//! The scans fill coefficient planes (progressive files need all their scans before anything can be shown), then
//! each block is dequantised, transformed back with a separable floating-point IDCT and colour-converted.

// Sample arithmetic: coefficients, sizes and channels are small integers and bounded floats, clamped before they
// become bytes.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::too_many_lines,
    clippy::suboptimal_flops
)]

use crate::{Error, Image, MAX_PIXELS, MAX_SIDE};

/// Where the coefficient at zigzag position `k` lives in an 8x8 block.
const ZIGZAG: [usize; 64] = [
    0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5, 12, 19, 26, 33, 40, 48, 41, 34, 27, 20,
    13, 6, 7, 14, 21, 28, 35, 42, 49, 56, 57, 50, 43, 36, 29, 22, 15, 23, 30, 37, 44, 51, 58, 59,
    52, 45, 38, 31, 39, 46, 53, 60, 61, 54, 47, 55, 62, 63,
];

/// A Huffman table in the form the standard's decoding procedure uses.
#[derive(Clone, Default)]
struct Huffman {
    /// The largest code of each length (-1 when there is none) and the index of the first symbol of the length.
    max_code: [i32; 18],
    value_offset: [i32; 17],
    symbols: Vec<u8>,
}

impl Huffman {
    fn new(counts: &[u8], symbols: Vec<u8>) -> Result<Self, Error> {
        let mut table = Self {
            symbols,
            ..Self::default()
        };
        let (mut code, mut index) = (0i32, 0i32);
        for length in 1..=16usize {
            let count = i32::from(counts[length - 1]);
            table.value_offset[length] = index - code;
            code += count;
            index += count;
            table.max_code[length] = if count == 0 { -1 } else { code - 1 };
            if code > (1 << length) {
                return Err(Error::Corrupt("the data is damaged"));
            }
            code <<= 1;
        }
        table.max_code[17] = i32::MAX;
        if usize::try_from(index).map_or(true, |n| n > table.symbols.len()) {
            return Err(Error::Corrupt("the data is damaged"));
        }
        Ok(table)
    }
}

/// The entropy-coded data, one bit at a time, with the byte stuffing taken out. A marker ends it (zeros follow).
struct Bits<'a> {
    data: &'a [u8],
    at: usize,
    buffer: u32,
    count: u32,
}

impl<'a> Bits<'a> {
    const fn new(data: &'a [u8], at: usize) -> Self {
        Self {
            data,
            at,
            buffer: 0,
            count: 0,
        }
    }

    fn bit(&mut self) -> u32 {
        if self.count == 0 {
            let byte = match self.data.get(self.at) {
                Some(0xff) => match self.data.get(self.at + 1) {
                    Some(0) => {
                        self.at += 2;
                        0xff
                    }
                    // A marker (or the end): stay on it and read zeros.
                    _ => 0,
                },
                Some(b) => {
                    self.at += 1;
                    *b
                }
                None => 0,
            };
            self.buffer = u32::from(byte);
            self.count = 8;
        }
        self.count -= 1;
        (self.buffer >> self.count) & 1
    }

    fn receive(&mut self, n: u32) -> i32 {
        (0..n).fold(0, |v, _| (v << 1) | self.bit() as i32)
    }

    /// `receive` and then the sign extension of the standard's F.2.2.1.
    fn receive_extend(&mut self, n: u32) -> i32 {
        if n == 0 {
            return 0;
        }
        let v = self.receive(n);
        if v < (1 << (n - 1)) {
            v - (1 << n) + 1
        } else {
            v
        }
    }

    fn decode(&mut self, table: &Huffman) -> Result<u8, Error> {
        let mut code = 0i32;
        for length in 1..=16usize {
            code = (code << 1) | self.bit() as i32;
            if table.max_code[length] >= 0 && code <= table.max_code[length] {
                let index = usize::try_from(code + table.value_offset[length])
                    .map_err(|_| Error::Corrupt("the data is damaged"))?;
                return table
                    .symbols
                    .get(index)
                    .copied()
                    .ok_or(Error::Corrupt("the data is damaged"));
            }
        }
        Err(Error::Corrupt("the data is damaged"))
    }

    /// Drops the bits left in the byte and steps over a restart marker.
    fn restart(&mut self) {
        self.count = 0;
        while self.data.get(self.at) == Some(&0xff) && self.data.get(self.at + 1) == Some(&0xff) {
            self.at += 1;
        }
        if self.data.get(self.at) == Some(&0xff)
            && matches!(self.data.get(self.at + 1), Some(0xd0..=0xd7))
        {
            self.at += 2;
        }
    }
}

struct Component {
    id: u8,
    h: usize,
    v: usize,
    quant: usize,
    /// Blocks per row and per column of the plane (padded to whole MCUs).
    blocks_wide: usize,
    blocks_high: usize,
    coefficients: Vec<i16>,
}

struct Scan {
    /// Indices into the frame's components, with their DC and AC table numbers.
    parts: Vec<(usize, usize, usize)>,
    start: usize,
    end: usize,
    high: u32,
    low: u32,
}

fn be16(b: &[u8]) -> usize {
    usize::from(b[0]) << 8 | usize::from(b[1])
}

/// Decodes a JPEG file.
pub fn decode(file: &[u8]) -> Result<Image, Error> {
    let mut at = 2;
    let mut quant = [[0u16; 64]; 4];
    let mut dc: [Huffman; 4] = Default::default();
    let mut ac: [Huffman; 4] = Default::default();
    let mut restart_interval = 0usize;
    let mut adobe: Option<u8> = None;
    let mut frame: Option<(usize, usize, bool, Vec<Component>)> = None;
    let mut scanned = false;
    loop {
        // Find the next marker.
        while file.get(at).is_some_and(|b| *b != 0xff) {
            at += 1;
        }
        while file.get(at) == Some(&0xff) && file.get(at + 1) == Some(&0xff) {
            at += 1;
        }
        let Some(&marker) = file.get(at + 1) else {
            break;
        };
        at += 2;
        if marker == 0xd9 {
            break;
        }
        if marker == 0x01 || (0xd0..=0xd8).contains(&marker) {
            continue;
        }
        let length = file
            .get(at..at + 2)
            .map(be16)
            .ok_or(Error::Corrupt("the data is damaged"))?;
        let body = file
            .get(at + 2..at + length)
            .ok_or(Error::Corrupt("the data is damaged"))?;
        at += length;
        match marker {
            0xee if body.starts_with(b"Adobe") && body.len() >= 12 => adobe = Some(body[11]),
            0xdb => {
                let mut b = body;
                while let Some((&head, rest)) = b.split_first() {
                    let (precision, id) = (head >> 4, usize::from(head & 15));
                    let size = if precision == 0 { 64 } else { 128 };
                    if id > 3 || rest.len() < size {
                        return Err(Error::Corrupt("the data is damaged"));
                    }
                    for k in 0..64 {
                        quant[id][ZIGZAG[k]] = if precision == 0 {
                            u16::from(rest[k])
                        } else {
                            be16(&rest[k * 2..]) as u16
                        };
                    }
                    b = &rest[size..];
                }
            }
            0xc4 => {
                let mut b = body;
                while b.len() >= 17 {
                    let (class, id) = (b[0] >> 4, usize::from(b[0] & 15));
                    let counts = &b[1..17];
                    let total: usize = counts.iter().map(|c| usize::from(*c)).sum();
                    if id > 3 || class > 1 || b.len() < 17 + total {
                        return Err(Error::Corrupt("the data is damaged"));
                    }
                    let table = Huffman::new(counts, b[17..17 + total].to_vec())?;
                    if class == 0 {
                        dc[id] = table;
                    } else {
                        ac[id] = table;
                    }
                    b = &b[17 + total..];
                }
            }
            0xdd => {
                restart_interval = body
                    .get(0..2)
                    .map(be16)
                    .ok_or(Error::Corrupt("the data is damaged"))?;
            }
            0xc0..=0xc2 => {
                if frame.is_some() || body.len() < 6 {
                    return Err(Error::Corrupt("the data is damaged"));
                }
                if body[0] != 8 {
                    return Err(Error::Unsupported("JPEG with other than 8 bits"));
                }
                let (height, width, n) =
                    (be16(&body[1..3]), be16(&body[3..5]), usize::from(body[5]));
                if width == 0 || height == 0 || !matches!(n, 1 | 3) || body.len() < 6 + n * 3 {
                    return Err(if matches!(n, 1 | 3) {
                        Error::Corrupt("the data is damaged")
                    } else {
                        Error::Unsupported("JPEG with 4 colour channels")
                    });
                }
                if width as u32 > MAX_SIDE
                    || height as u32 > MAX_SIDE
                    || (width * height) as u64 > MAX_PIXELS
                {
                    return Err(Error::TooLarge);
                }
                let mut components = Vec::new();
                for c in body[6..6 + n * 3].chunks(3) {
                    let (h, v) = (usize::from(c[1] >> 4), usize::from(c[1] & 15));
                    if !(1..=4).contains(&h) || !(1..=4).contains(&v) || c[2] > 3 {
                        return Err(Error::Corrupt("the data is damaged"));
                    }
                    components.push(Component {
                        id: c[0],
                        h,
                        v,
                        quant: usize::from(c[2]),
                        blocks_wide: 0,
                        blocks_high: 0,
                        coefficients: Vec::new(),
                    });
                }
                let hmax = components.iter().map(|c| c.h).max().unwrap_or(1);
                let vmax = components.iter().map(|c| c.v).max().unwrap_or(1);
                let (mcus_wide, mcus_high) = (width.div_ceil(8 * hmax), height.div_ceil(8 * vmax));
                for c in &mut components {
                    c.blocks_wide = mcus_wide * c.h;
                    c.blocks_high = mcus_high * c.v;
                    c.coefficients = vec![0; c.blocks_wide * c.blocks_high * 64];
                }
                frame = Some((width, height, marker == 0xc2, components));
            }
            0xc3 | 0xc5..=0xcf if marker != 0xc4 && marker != 0xc8 && marker != 0xcc => {
                return Err(Error::Unsupported("JPEG (arithmetic or lossless)"));
            }
            0xda => {
                let Some((width, height, progressive, components)) = frame.as_mut() else {
                    return Err(Error::Corrupt("the data is damaged"));
                };
                let n = usize::from(*body.first().ok_or(Error::Corrupt("the data is damaged"))?);
                if n == 0 || n > 3 || body.len() < 1 + n * 2 + 3 {
                    return Err(Error::Corrupt("the data is damaged"));
                }
                let mut parts = Vec::new();
                for c in body[1..=n * 2].chunks(2) {
                    let index = components
                        .iter()
                        .position(|k| k.id == c[0])
                        .ok_or(Error::Corrupt("the data is damaged"))?;
                    parts.push((
                        index,
                        usize::from(c[1] >> 4) & 3,
                        usize::from(c[1] & 15) & 3,
                    ));
                }
                let tail = &body[1 + n * 2..];
                let scan = if *progressive {
                    Scan {
                        parts,
                        start: usize::from(tail[0]),
                        end: usize::from(tail[1]).min(63),
                        high: u32::from(tail[2] >> 4),
                        low: u32::from(tail[2] & 15),
                    }
                } else {
                    Scan {
                        parts,
                        start: 0,
                        end: 63,
                        high: 0,
                        low: 0,
                    }
                };
                if scan.start > scan.end {
                    return Err(Error::Corrupt("the data is damaged"));
                }
                let mut bits = Bits::new(file, at);
                scan_data(
                    &mut bits,
                    &scan,
                    components,
                    (*width, *height),
                    &dc,
                    &ac,
                    restart_interval,
                    *progressive,
                )?;
                at = bits.at;
                scanned = true;
            }
            _ => {}
        }
    }
    let (width, height, _, components) = frame.ok_or(Error::Corrupt("the data is damaged"))?;
    if !scanned {
        return Err(Error::Corrupt("the data is damaged"));
    }
    Ok(output(width, height, &components, &quant, adobe))
}

/// One scan: every block it covers, in order, with restarts.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn scan_data(
    bits: &mut Bits<'_>,
    scan: &Scan,
    components: &mut [Component],
    (width, height): (usize, usize),
    dc: &[Huffman; 4],
    ac: &[Huffman; 4],
    restart_interval: usize,
    progressive: bool,
) -> Result<(), Error> {
    let hmax = components.iter().map(|c| c.h).max().unwrap_or(1);
    let vmax = components.iter().map(|c| c.v).max().unwrap_or(1);
    let interleaved = scan.parts.len() > 1;
    // The unit of work is an MCU: for one component, one block; for several, h x v blocks of each.
    let (units_wide, units_high) = if interleaved {
        (width.div_ceil(8 * hmax), height.div_ceil(8 * vmax))
    } else {
        let c = &components[scan.parts[0].0];
        (
            (width * c.h).div_ceil(hmax).div_ceil(8),
            (height * c.v).div_ceil(vmax).div_ceil(8),
        )
    };
    let mut predictions = [0i32; 3];
    let mut eob_run = 0u32;
    let mut done = 0usize;
    for unit_y in 0..units_high {
        for unit_x in 0..units_wide {
            if restart_interval > 0 && done > 0 && done.is_multiple_of(restart_interval) {
                bits.restart();
                predictions = [0; 3];
                eob_run = 0;
            }
            done += 1;
            for (slot, &(index, dc_table, ac_table)) in scan.parts.iter().enumerate() {
                let c = &mut components[index];
                let (across, down) = if interleaved { (c.h, c.v) } else { (1, 1) };
                for by in 0..down {
                    for bx in 0..across {
                        let (x, y) = (unit_x * across + bx, unit_y * down + by);
                        if x >= c.blocks_wide || y >= c.blocks_high {
                            continue;
                        }
                        let offset = (y * c.blocks_wide + x) * 64;
                        let block = &mut c.coefficients[offset..offset + 64];
                        if progressive {
                            progressive_block(
                                bits,
                                block,
                                scan,
                                &dc[dc_table],
                                &ac[ac_table],
                                &mut predictions[slot],
                                &mut eob_run,
                            )?;
                        } else {
                            baseline_block(
                                bits,
                                block,
                                &dc[dc_table],
                                &ac[ac_table],
                                &mut predictions[slot],
                            )?;
                        }
                    }
                }
            }
        }
    }
    bits.count = 0;
    Ok(())
}

fn baseline_block(
    bits: &mut Bits<'_>,
    block: &mut [i16],
    dc: &Huffman,
    ac: &Huffman,
    prediction: &mut i32,
) -> Result<(), Error> {
    let size = u32::from(bits.decode(dc)?);
    if size > 15 {
        return Err(Error::Corrupt("the data is damaged"));
    }
    *prediction += bits.receive_extend(size);
    block[0] = *prediction as i16;
    let mut k = 1;
    while k < 64 {
        let symbol = bits.decode(ac)?;
        let (run, size) = (usize::from(symbol >> 4), u32::from(symbol & 15));
        if size == 0 {
            if run == 15 {
                k += 16;
                continue;
            }
            break;
        }
        k += run;
        if k > 63 {
            return Err(Error::Corrupt("the data is damaged"));
        }
        block[ZIGZAG[k]] = bits.receive_extend(size) as i16;
        k += 1;
    }
    Ok(())
}

fn progressive_block(
    bits: &mut Bits<'_>,
    block: &mut [i16],
    scan: &Scan,
    dc: &Huffman,
    ac: &Huffman,
    prediction: &mut i32,
    eob_run: &mut u32,
) -> Result<(), Error> {
    if scan.start == 0 {
        if scan.high == 0 {
            let size = u32::from(bits.decode(dc)?);
            if size > 15 {
                return Err(Error::Corrupt("the data is damaged"));
            }
            *prediction += bits.receive_extend(size);
            block[0] = (*prediction << scan.low) as i16;
        } else if bits.bit() == 1 {
            block[0] |= 1 << scan.low;
        }
        return Ok(());
    }
    if scan.high == 0 {
        // The first pass over these coefficients.
        if *eob_run > 0 {
            *eob_run -= 1;
            return Ok(());
        }
        let mut k = scan.start;
        while k <= scan.end {
            let symbol = bits.decode(ac)?;
            let (run, size) = (u32::from(symbol >> 4), u32::from(symbol & 15));
            if size == 0 {
                if run < 15 {
                    *eob_run = (1 << run) - 1 + if run > 0 { bits.receive(run) as u32 } else { 0 };
                    break;
                }
                k += 16;
                continue;
            }
            k += run as usize;
            if k > 63 {
                return Err(Error::Corrupt("the data is damaged"));
            }
            block[ZIGZAG[k]] = (bits.receive_extend(size) << scan.low) as i16;
            k += 1;
        }
        return Ok(());
    }
    // A refinement pass: one more bit for each coefficient that is already non-zero, and new ones of +-1.
    let (plus, minus) = (1i16 << scan.low, -1i16 << scan.low);
    let mut k = scan.start;
    let refine = |bits: &mut Bits<'_>, value: &mut i16| {
        if bits.bit() == 1 && *value & plus == 0 {
            *value += if *value >= 0 { plus } else { minus };
        }
    };
    if *eob_run == 0 {
        while k <= scan.end {
            let symbol = bits.decode(ac)?;
            let (mut run, size) = (i32::from(symbol >> 4), symbol & 15);
            let mut new = 0i16;
            if size != 0 {
                new = if bits.bit() == 1 { plus } else { minus };
            } else if run != 15 {
                *eob_run = (1 << run)
                    + if run > 0 {
                        bits.receive(run as u32) as u32
                    } else {
                        0
                    };
                break;
            }
            while k <= scan.end {
                let value = &mut block[ZIGZAG[k]];
                if *value != 0 {
                    refine(bits, value);
                } else {
                    run -= 1;
                    if run < 0 {
                        break;
                    }
                }
                k += 1;
            }
            if new != 0 && k <= 63 {
                block[ZIGZAG[k]] = new;
            }
            k += 1;
        }
    }
    if *eob_run > 0 {
        while k <= scan.end {
            let value = &mut block[ZIGZAG[k]];
            if *value != 0 {
                refine(bits, value);
            }
            k += 1;
        }
        *eob_run -= 1;
    }
    Ok(())
}

/// Dequantises and inverts one component into a plane of samples.
fn plane(c: &Component, quant: &[u16; 64]) -> Vec<u8> {
    let width = c.blocks_wide * 8;
    let mut samples = vec![0u8; width * c.blocks_high * 8];
    // cos((2x + 1) u pi / 16), scaled by C(u) / 2.
    let mut basis = [[0f32; 8]; 8];
    for (x, row) in basis.iter_mut().enumerate() {
        for (u, value) in row.iter_mut().enumerate() {
            let scale = if u == 0 {
                std::f32::consts::FRAC_1_SQRT_2
            } else {
                1.0
            } / 2.0;
            *value = scale * ((2 * x + 1) as f32 * u as f32 * std::f32::consts::PI / 16.0).cos();
        }
    }
    let mut tmp = [0f32; 64];
    for by in 0..c.blocks_high {
        for bx in 0..c.blocks_wide {
            let block = &c.coefficients[(by * c.blocks_wide + bx) * 64..][..64];
            // Rows: for each row v, the 1-D transform over u.
            for v in 0..8 {
                for x in 0..8 {
                    tmp[v * 8 + x] = (0..8)
                        .map(|u| {
                            basis[x][u] * f32::from(block[v * 8 + u]) * f32::from(quant[v * 8 + u])
                        })
                        .sum();
                }
            }
            for x in 0..8 {
                for y in 0..8 {
                    let value: f32 = (0..8).map(|v| basis[y][v] * tmp[v * 8 + x]).sum();
                    samples[(by * 8 + y) * width + bx * 8 + x] =
                        (value + 128.0).round().clamp(0.0, 255.0) as u8;
                }
            }
        }
    }
    samples
}

fn output(
    width: usize,
    height: usize,
    components: &[Component],
    quant: &[[u16; 64]; 4],
    adobe: Option<u8>,
) -> Image {
    let planes: Vec<Vec<u8>> = components
        .iter()
        .map(|c| plane(c, &quant[c.quant]))
        .collect();
    let hmax = components.iter().map(|c| c.h).max().unwrap_or(1);
    let vmax = components.iter().map(|c| c.v).max().unwrap_or(1);
    let sample = |n: usize, x: usize, y: usize| -> f32 {
        let c = &components[n];
        let stride = c.blocks_wide * 8;
        f32::from(planes[n][(y * c.v / vmax) * stride + x * c.h / hmax])
    };
    let rgb_marked = adobe == Some(0);
    let mut rgba = vec![255u8; width * height * 4];
    for y in 0..height {
        for x in 0..width {
            let out = &mut rgba[(y * width + x) * 4..][..3];
            if components.len() == 1 {
                out.fill(sample(0, x, y) as u8);
            } else if rgb_marked {
                for (n, o) in out.iter_mut().enumerate() {
                    *o = sample(n, x, y) as u8;
                }
            } else {
                let (luma, blue, red) = (
                    sample(0, x, y),
                    sample(1, x, y) - 128.0,
                    sample(2, x, y) - 128.0,
                );
                out[0] = (luma + 1.402 * red).round().clamp(0.0, 255.0) as u8;
                out[1] = (luma - 0.344_136 * blue - 0.714_136 * red)
                    .round()
                    .clamp(0.0, 255.0) as u8;
                out[2] = (luma + 1.772 * blue).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    Image {
        width: width as u32,
        height: height as u32,
        rgba,
    }
}

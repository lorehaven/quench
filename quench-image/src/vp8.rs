//! The lossy WebP codec: VP8 key frames (RFC 6386). A frame is decoded macroblock by macroblock into three planes
//! (modes and prediction from the first partition, coefficients from the token partitions), the loop filter is run
//! over the whole frame, and the planes become RGBA with the chroma interpolated up to full size.

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

use crate::vp8_tables::{AC_QUANT, BMODE_PROBS, COEFF_PROBS, COEFF_UPDATE_PROBS, DC_QUANT};
use crate::{Error, Image, MAX_PIXELS, MAX_SIDE};

const fn corrupt() -> Error {
    Error::Corrupt("the data is damaged")
}

/// The boolean entropy decoder of the format.
struct Bool<'a> {
    data: &'a [u8],
    pos: usize,
    value: u32,
    range: u32,
    bits: u32,
}

impl<'a> Bool<'a> {
    fn new(data: &'a [u8]) -> Self {
        let byte = |i: usize| u32::from(data.get(i).copied().unwrap_or(0));
        Self {
            data,
            pos: 2,
            value: byte(0) << 8 | byte(1),
            range: 255,
            bits: 0,
        }
    }

    fn bool(&mut self, probability: u8) -> bool {
        let split = 1 + (((self.range - 1) * u32::from(probability)) >> 8);
        let big = split << 8;
        let bit = if self.value >= big {
            self.range -= split;
            self.value -= big;
            true
        } else {
            self.range = split;
            false
        };
        while self.range < 128 {
            self.value <<= 1;
            self.range <<= 1;
            self.bits += 1;
            if self.bits == 8 {
                self.bits = 0;
                self.value |= u32::from(self.data.get(self.pos).copied().unwrap_or(0));
                self.pos += 1;
            }
        }
        bit
    }

    fn flag(&mut self) -> bool {
        self.bool(128)
    }

    fn literal(&mut self, n: u32) -> u32 {
        (0..n).fold(0, |v, _| v << 1 | u32::from(self.flag()))
    }

    /// A magnitude of `n` bits and a sign.
    fn signed(&mut self, n: u32) -> i32 {
        let v = self.literal(n) as i32;
        if self.flag() { -v } else { v }
    }

    /// A value that is only there if a flag says so.
    fn optional_signed(&mut self, n: u32) -> i32 {
        if self.flag() { self.signed(n) } else { 0 }
    }

    /// Walks a tree whose leaves are `-value` (and `-0` is 0), with the probabilities of its nodes.
    fn tree(&mut self, tree: &[i8], probabilities: &[u8]) -> usize {
        let mut i = 0usize;
        loop {
            let next = tree[i + usize::from(self.bool(probabilities[i >> 1]))];
            if next <= 0 {
                return (-next) as usize;
            }
            i = next as usize;
        }
    }

    /// Whether the data ran out (the decoder has read past what there is).
    const fn exhausted(&self) -> bool {
        self.pos > self.data.len() + 4
    }
}

// Whole-block prediction modes, and the modes of 4x4 sub-blocks.
const DC_PRED: usize = 0;
const V_PRED: usize = 1;
const H_PRED: usize = 2;
const TM_PRED: usize = 3;
const B_PRED: usize = 4;

const B_DC_PRED: usize = 0;
const B_TM_PRED: usize = 1;
const B_VE_PRED: usize = 2;
const B_HE_PRED: usize = 3;
const B_LD_PRED: usize = 4;
const B_RD_PRED: usize = 5;
const B_VR_PRED: usize = 6;
const B_VL_PRED: usize = 7;
const B_HD_PRED: usize = 8;
const B_HU_PRED: usize = 9;

const YMODE_TREE: [i8; 8] = [
    -(B_PRED as i8),
    2,
    4,
    6,
    -(DC_PRED as i8),
    -(V_PRED as i8),
    -(H_PRED as i8),
    -(TM_PRED as i8),
];
const YMODE_PROBS: [u8; 4] = [145, 156, 163, 128];
const UV_MODE_TREE: [i8; 6] = [
    -(DC_PRED as i8),
    2,
    -(V_PRED as i8),
    4,
    -(H_PRED as i8),
    -(TM_PRED as i8),
];
const UV_MODE_PROBS: [u8; 3] = [142, 114, 183];
const BMODE_TREE: [i8; 18] = [
    -(B_DC_PRED as i8),
    2,
    -(B_TM_PRED as i8),
    4,
    -(B_VE_PRED as i8),
    6,
    8,
    12,
    -(B_HE_PRED as i8),
    10,
    -(B_RD_PRED as i8),
    -(B_VR_PRED as i8),
    -(B_LD_PRED as i8),
    14,
    -(B_VL_PRED as i8),
    16,
    -(B_HD_PRED as i8),
    -(B_HU_PRED as i8),
];
const SEGMENT_TREE: [i8; 6] = [2, 4, 0, -1, -2, -3];

const ZIGZAG: [usize; 16] = [0, 1, 4, 8, 5, 2, 3, 6, 9, 12, 13, 10, 7, 11, 14, 15];
const BANDS: [usize; 17] = [0, 1, 2, 3, 6, 4, 5, 6, 6, 6, 6, 6, 6, 6, 6, 7, 0];

/// The extra-bit probabilities of the large coefficient categories 3 to 6.
const CATEGORIES: [&[u8]; 4] = [
    &[173, 148, 140],
    &[176, 155, 140, 135],
    &[180, 157, 141, 134, 130],
    &[254, 254, 243, 230, 196, 177, 153, 140, 133, 130, 129],
];

/// What a segment quantises its coefficients by: DC and AC steps for the three kinds of block.
#[derive(Clone, Copy, Default)]
struct Quant {
    y1: [i32; 2],
    y2: [i32; 2],
    uv: [i32; 2],
}

#[derive(Clone, Copy, Default)]
struct FilterInfo {
    limit: i32,
    inner_level: i32,
    hev: i32,
    /// Whether the edges inside the macroblock are filtered too.
    inner: bool,
}

/// Decodes the data of a `VP8 ` chunk (a key frame).
#[allow(clippy::too_many_lines)]
pub fn decode(data: &[u8]) -> Result<Image, Error> {
    if data.len() < 10 {
        return Err(corrupt());
    }
    let tag = u32::from(data[0]) | u32::from(data[1]) << 8 | u32::from(data[2]) << 16;
    if tag & 1 != 0 {
        return Err(Error::Unsupported("WebP frames that are not key frames"));
    }
    let first_size = (tag >> 5) as usize;
    if data[3..6] != [0x9d, 0x01, 0x2a] {
        return Err(corrupt());
    }
    let width = usize::from(u16::from_le_bytes([data[6], data[7]]) & 0x3fff);
    let height = usize::from(u16::from_le_bytes([data[8], data[9]]) & 0x3fff);
    if width == 0
        || height == 0
        || width as u32 > MAX_SIDE
        || height as u32 > MAX_SIDE
        || (width * height) as u64 > MAX_PIXELS
    {
        return Err(Error::TooLarge);
    }
    let rest = &data[10..];
    if first_size > rest.len() {
        return Err(corrupt());
    }
    let mut b = Bool::new(&rest[..first_size]);

    // The frame header.
    let _color_space = b.flag();
    let _clamping = b.flag();
    let segmentation = b.flag();
    let (mut update_map, mut absolute) = (false, false);
    let mut seg_quant = [0i32; 4];
    let mut seg_filter = [0i32; 4];
    let mut seg_probs = [255u8; 3];
    if segmentation {
        update_map = b.flag();
        if b.flag() {
            absolute = b.flag();
            for q in &mut seg_quant {
                *q = b.optional_signed(7);
            }
            for f in &mut seg_filter {
                *f = b.optional_signed(6);
            }
        }
        if update_map {
            for p in &mut seg_probs {
                *p = if b.flag() { b.literal(8) as u8 } else { 255 };
            }
        }
    }
    let simple = b.flag();
    let level = b.literal(6) as i32;
    let sharpness = b.literal(3) as i32;
    let (mut ref_delta, mut mode_delta) = ([0i32; 4], [0i32; 4]);
    let delta_enabled = b.flag();
    if delta_enabled && b.flag() {
        for d in ref_delta.iter_mut().chain(mode_delta.iter_mut()) {
            if b.flag() {
                *d = b.signed(6);
            }
        }
    }
    let partitions = 1usize << b.literal(2);
    let base_q = b.literal(7) as i32;
    let deltas: [i32; 5] = std::array::from_fn(|_| b.optional_signed(4));
    let [y1_dc, y2_dc, y2_ac, uv_dc, uv_ac] = deltas;
    let _refresh_entropy = b.flag();
    let mut probs = COEFF_PROBS;
    for (i, plane) in COEFF_UPDATE_PROBS.iter().enumerate() {
        for (j, band) in plane.iter().enumerate() {
            for (k, ctx) in band.iter().enumerate() {
                for (l, &update) in ctx.iter().enumerate() {
                    if b.bool(update) {
                        probs[i][j][k][l] = b.literal(8) as u8;
                    }
                }
            }
        }
    }
    let skip_probability = if b.flag() {
        Some(b.literal(8) as u8)
    } else {
        None
    };

    // Quantisation and filter strengths, per segment.
    let clip = |v: i32, max: i32| v.clamp(0, max) as usize;
    let mut quant = [Quant::default(); 4];
    let mut filters = [[FilterInfo::default(); 2]; 4];
    for s in 0..4 {
        let q = if segmentation {
            if absolute {
                seg_quant[s]
            } else {
                seg_quant[s] + base_q
            }
        } else {
            base_q
        };
        quant[s] = Quant {
            y1: [DC_QUANT[clip(q + y1_dc, 127)], AC_QUANT[clip(q, 127)]],
            y2: [
                DC_QUANT[clip(q + y2_dc, 127)] * 2,
                ((AC_QUANT[clip(q + y2_ac, 127)] * 101_581) >> 16).max(8),
            ],
            uv: [
                DC_QUANT[clip(q + uv_dc, 117)],
                AC_QUANT[clip(q + uv_ac, 127)],
            ],
        };
        let mut base = level;
        if segmentation {
            base = if absolute {
                seg_filter[s]
            } else {
                seg_filter[s] + level
            };
        }
        for (i4, slot) in filters[s].iter_mut().enumerate() {
            let mut l = base;
            if delta_enabled {
                l += ref_delta[0];
                if i4 == 1 {
                    l += mode_delta[0];
                }
            }
            let l = l.clamp(0, 63);
            if l > 0 {
                let mut inner = l;
                if sharpness > 0 {
                    inner >>= if sharpness > 4 { 2 } else { 1 };
                    inner = inner.min(9 - sharpness);
                }
                let inner = inner.max(1);
                *slot = FilterInfo {
                    limit: 2 * l + inner,
                    inner_level: inner,
                    hev: if l >= 40 { 2 } else { i32::from(l >= 15) },
                    inner: i4 == 1,
                };
            }
        }
    }

    // The token partitions follow the first one: a table of the sizes of all but the last, then their data.
    let after = &rest[first_size..];
    let table = 3 * (partitions - 1);
    if after.len() < table {
        return Err(corrupt());
    }
    let mut part_data = Vec::with_capacity(partitions);
    let mut at = table;
    for i in 0..partitions {
        let size = if i + 1 < partitions {
            let t = &after[3 * i..3 * i + 3];
            usize::from(t[0]) | usize::from(t[1]) << 8 | usize::from(t[2]) << 16
        } else {
            after.len().saturating_sub(at)
        };
        let end = (at + size).min(after.len());
        part_data.push(&after[at.min(end)..end]);
        at = end;
    }
    let mut tokens: Vec<Bool<'_>> = part_data.into_iter().map(Bool::new).collect();

    // The macroblocks.
    let (mb_w, mb_h) = (width.div_ceil(16), height.div_ceil(16));
    let (y_stride, uv_stride) = (mb_w * 16, mb_w * 8);
    let mut y_plane = vec![0u8; y_stride * mb_h * 16];
    let mut u_plane = vec![0u8; uv_stride * mb_h * 8];
    let mut v_plane = vec![0u8; uv_stride * mb_h * 8];
    let mut infos = vec![FilterInfo::default(); mb_w * mb_h];
    let mut above_modes = vec![[B_DC_PRED; 4]; mb_w];
    // The "has coefficients" flags above and to the left: 4 luma, 2 + 2 chroma, 1 for the Y2 block.
    let mut above_nz = vec![[false; 9]; mb_w];
    for mb_y in 0..mb_h {
        let mut left_modes = [B_DC_PRED; 4];
        let mut left_nz = [false; 9];
        let t = mb_y % partitions;
        for mb_x in 0..mb_w {
            if b.exhausted() {
                return Err(corrupt());
            }
            let segment = if update_map {
                b.tree(&SEGMENT_TREE, &seg_probs)
            } else {
                0
            };
            let skip = skip_probability.is_some_and(|p| b.bool(p));
            let ymode = b.tree(&YMODE_TREE, &YMODE_PROBS);
            let mut modes = [B_DC_PRED; 16];
            if ymode == B_PRED {
                for y in 0..4 {
                    for x in 0..4 {
                        let top = if y == 0 {
                            above_modes[mb_x][x]
                        } else {
                            modes[(y - 1) * 4 + x]
                        };
                        let left = if x == 0 {
                            left_modes[y]
                        } else {
                            modes[y * 4 + x - 1]
                        };
                        modes[y * 4 + x] = b.tree(&BMODE_TREE, &BMODE_PROBS[top][left]);
                    }
                }
            } else {
                // The sub-block modes that stand for this whole-block mode, as context for the neighbours.
                let implied = match ymode {
                    V_PRED => B_VE_PRED,
                    H_PRED => B_HE_PRED,
                    TM_PRED => B_TM_PRED,
                    _ => B_DC_PRED,
                };
                modes = [implied; 16];
            }
            for i in 0..4 {
                above_modes[mb_x][i] = modes[12 + i];
                left_modes[i] = modes[i * 4 + 3];
            }
            let uv_mode = b.tree(&UV_MODE_TREE, &UV_MODE_PROBS);

            // The coefficients.
            let q = &quant[segment];
            let mut coeffs = [[0i32; 16]; 25];
            let mut nonzero = [false; 25];
            let has_y2 = ymode != B_PRED;
            if skip {
                above_nz[mb_x][..8].fill(false);
                left_nz[..8].fill(false);
                if has_y2 {
                    above_nz[mb_x][8] = false;
                    left_nz[8] = false;
                }
            } else {
                let tk = &mut tokens[t];
                let mut first = 0;
                let y_type = if has_y2 {
                    let ctx = usize::from(above_nz[mb_x][8]) + usize::from(left_nz[8]);
                    let n = read_coeffs(tk, &probs[1], ctx, &q.y2, 0, &mut coeffs[24]);
                    above_nz[mb_x][8] = n > 0;
                    left_nz[8] = n > 0;
                    nonzero[24] = n > 0;
                    first = 1;
                    0
                } else {
                    3
                };
                for by in 0..4 {
                    for bx in 0..4 {
                        let ctx = usize::from(above_nz[mb_x][bx]) + usize::from(left_nz[by]);
                        let n = read_coeffs(
                            tk,
                            &probs[y_type],
                            ctx,
                            &q.y1,
                            first,
                            &mut coeffs[by * 4 + bx],
                        );
                        above_nz[mb_x][bx] = n > first;
                        left_nz[by] = n > first;
                        nonzero[by * 4 + bx] = n > 1;
                    }
                }
                for plane in 0..2 {
                    for by in 0..2 {
                        for bx in 0..2 {
                            let (a, l) = (4 + plane * 2 + bx, 4 + plane * 2 + by);
                            let ctx = usize::from(above_nz[mb_x][a]) + usize::from(left_nz[l]);
                            let block = 16 + plane * 4 + by * 2 + bx;
                            let n = read_coeffs(tk, &probs[2], ctx, &q.uv, 0, &mut coeffs[block]);
                            above_nz[mb_x][a] = n > 0;
                            left_nz[l] = n > 0;
                            nonzero[block] = n > 1;
                        }
                    }
                }
                if has_y2 {
                    // The second-order block gives each luma block its DC.
                    let dcs = inverse_wht(&coeffs[24]);
                    for (block, dc) in dcs.iter().enumerate() {
                        coeffs[block][0] = *dc;
                    }
                }
                for (block, c) in coeffs.iter().enumerate().take(24) {
                    // A block counts as non-zero for the filter if it has AC terms or a DC.
                    nonzero[block] = nonzero[block] || c[0] != 0;
                }
            }
            let any = nonzero[..24].iter().any(|n| *n);
            infos[mb_y * mb_w + mb_x] = filters[segment][usize::from(ymode == B_PRED)];
            infos[mb_y * mb_w + mb_x].inner |= any;

            // Prediction and reconstruction.
            predict_luma(
                &mut y_plane,
                y_stride,
                (mb_x, mb_y, mb_w),
                ymode,
                &modes,
                &coeffs,
            );
            predict_chroma(
                &mut u_plane,
                uv_stride,
                (mb_x, mb_y),
                uv_mode,
                &coeffs[16..20],
            );
            predict_chroma(
                &mut v_plane,
                uv_stride,
                (mb_x, mb_y),
                uv_mode,
                &coeffs[20..24],
            );
        }
    }
    if level > 0 {
        loop_filter(
            &mut y_plane,
            &mut u_plane,
            &mut v_plane,
            (mb_w, mb_h),
            &infos,
            simple,
        );
    }
    Ok(to_rgba(
        &y_plane,
        &u_plane,
        &v_plane,
        (width, height),
        (y_stride, uv_stride),
    ))
}

/// Reads the coefficients of one block, dequantised and in raster order. Returns the position after the last
/// non-zero one (`first` when there is none).
fn read_coeffs(
    b: &mut Bool<'_>,
    probs: &[[[u8; 11]; 3]; 8],
    ctx: usize,
    dq: &[i32; 2],
    first: usize,
    out: &mut [i32; 16],
) -> usize {
    let mut n = first;
    let mut p = &probs[BANDS[n]][ctx];
    while n < 16 {
        if !b.bool(p[0]) {
            return n;
        }
        while !b.bool(p[1]) {
            n += 1;
            p = &probs[BANDS[n]][0];
            if n == 16 {
                return 16;
            }
        }
        let value;
        let next_ctx;
        if b.bool(p[2]) {
            value = large_value(b, p);
            next_ctx = 2;
        } else {
            value = 1;
            next_ctx = 1;
        }
        let signed = if b.flag() { -value } else { value };
        out[ZIGZAG[n]] = signed * dq[usize::from(n > 0)];
        n += 1;
        p = &probs[BANDS[n]][next_ctx];
    }
    16
}

/// A coefficient of 2 or more.
fn large_value(b: &mut Bool<'_>, p: &[u8; 11]) -> i32 {
    if !b.bool(p[3]) {
        if !b.bool(p[4]) {
            2
        } else {
            3 + i32::from(b.bool(p[5]))
        }
    } else if !b.bool(p[6]) {
        if !b.bool(p[7]) {
            5 + i32::from(b.bool(159))
        } else {
            let high = 2 * i32::from(b.bool(165));
            7 + high + i32::from(b.bool(145))
        }
    } else {
        let high = usize::from(b.bool(p[8]));
        let low = usize::from(b.bool(p[9 + high]));
        let category = 2 * high + low;
        let mut v = 0;
        for &prob in CATEGORIES[category] {
            v = v + v + i32::from(b.bool(prob));
        }
        v + 3 + (8 << category)
    }
}

/// The inverse Walsh-Hadamard transform of the second-order block: the DC of each of the 16 luma blocks.
fn inverse_wht(input: &[i32; 16]) -> [i32; 16] {
    let mut tmp = [0i32; 16];
    for i in 0..4 {
        let a0 = input[i] + input[12 + i];
        let a1 = input[4 + i] + input[8 + i];
        let a2 = input[4 + i] - input[8 + i];
        let a3 = input[i] - input[12 + i];
        tmp[i] = a0 + a1;
        tmp[8 + i] = a0 - a1;
        tmp[4 + i] = a3 + a2;
        tmp[12 + i] = a3 - a2;
    }
    let mut out = [0i32; 16];
    for i in 0..4 {
        let dc = tmp[i * 4] + 3;
        let a0 = dc + tmp[i * 4 + 3];
        let a1 = tmp[i * 4 + 1] + tmp[i * 4 + 2];
        let a2 = tmp[i * 4 + 1] - tmp[i * 4 + 2];
        let a3 = dc - tmp[i * 4 + 3];
        out[i * 4] = (a0 + a1) >> 3;
        out[i * 4 + 1] = (a3 + a2) >> 3;
        out[i * 4 + 2] = (a0 - a1) >> 3;
        out[i * 4 + 3] = (a3 - a2) >> 3;
    }
    out
}

/// Adds the inverse DCT of `c` to the 4x4 pixels at `at` in `buf` (rows `stride` apart). Wide integers keep a
/// damaged file's huge coefficients from overflowing.
fn add_idct(buf: &mut [u8], at: usize, stride: usize, c: &[i32; 16]) {
    let c: [i64; 16] = std::array::from_fn(|i| i64::from(c[i]));
    let mul1 = |a: i64| ((a * 20091) >> 16) + a;
    let mul2 = |a: i64| (a * 35468) >> 16;
    let mut tmp = [0i64; 16];
    for i in 0..4 {
        let a = c[i] + c[8 + i];
        let b = c[i] - c[8 + i];
        let cc = mul2(c[4 + i]) - mul1(c[12 + i]);
        let d = mul1(c[4 + i]) + mul2(c[12 + i]);
        tmp[i * 4] = a + d;
        tmp[i * 4 + 1] = b + cc;
        tmp[i * 4 + 2] = b - cc;
        tmp[i * 4 + 3] = a - d;
    }
    for i in 0..4 {
        let dc = tmp[i] + 4;
        let a = dc + tmp[8 + i];
        let b = dc - tmp[8 + i];
        let cc = mul2(tmp[4 + i]) - mul1(tmp[12 + i]);
        let d = mul1(tmp[4 + i]) + mul2(tmp[12 + i]);
        let row = at + i * stride;
        for (x, v) in [a + d, b + cc, b - cc, a - d].into_iter().enumerate() {
            let p = &mut buf[row + x];
            *p = (i64::from(*p) + (v >> 3)).clamp(0, 255) as u8;
        }
    }
}

const fn avg2(a: u8, b: u8) -> u8 {
    ((a as u16 + b as u16 + 1) >> 1) as u8
}

const fn avg3(a: u8, b: u8, c: u8) -> u8 {
    ((a as u16 + 2 * b as u16 + c as u16 + 2) >> 2) as u8
}

/// The work area of one macroblock: a row above it, a column to its left and a few pixels up and to the right.
const WORK: usize = 32;

fn clamp8(v: i32) -> u8 {
    v.clamp(0, 255) as u8
}

/// Fills row 0 and column 7 of a work area with what lies above and to the left of a block at `(x0, y0)`.
fn borders(
    work: &mut [u8],
    plane: &[u8],
    stride: usize,
    (x0, y0): (usize, usize),
    size: usize,
    top_right: Option<usize>,
) {
    if y0 == 0 {
        work[7..8 + size + 4].fill(127);
    } else {
        let above = (y0 - 1) * stride;
        work[8..8 + size].copy_from_slice(&plane[above + x0..above + x0 + size]);
        work[7] = if x0 == 0 { 129 } else { plane[above + x0 - 1] };
        if let Some(n) = top_right {
            for i in 0..4 {
                work[8 + size + i] = plane[above + (x0 + size + i).min(n)];
            }
        }
    }
    for j in 0..size {
        work[(1 + j) * WORK + 7] = if x0 == 0 {
            129
        } else {
            plane[(y0 + j) * stride + x0 - 1]
        };
    }
}

fn predict_luma(
    plane: &mut [u8],
    stride: usize,
    (mb_x, mb_y, mb_w): (usize, usize, usize),
    ymode: usize,
    modes: &[usize; 16],
    coeffs: &[[i32; 16]; 25],
) {
    let (x0, y0) = (mb_x * 16, mb_y * 16);
    let mut work = [0u8; WORK * 17];
    // The pixels above and to the right come from the macroblock up and to the right, or repeat the last above.
    let last_above = if mb_x + 1 < mb_w { None } else { Some(x0 + 15) };
    borders(
        &mut work,
        plane,
        stride,
        (x0, y0),
        16,
        Some(last_above.unwrap_or(usize::MAX)),
    );
    if y0 > 0 && mb_x + 1 >= mb_w {
        let last = work[8 + 15];
        work[24..28].fill(last);
    }
    if ymode == B_PRED {
        for by in 1..4 {
            for i in 0..4 {
                work[by * 4 * WORK + 24 + i] = work[24 + i];
            }
        }
        for by in 0..4 {
            for bx in 0..4 {
                let base = (1 + by * 4) * WORK + 8 + bx * 4;
                predict_sub(&mut work, base, modes[by * 4 + bx]);
                add_idct(&mut work, base, WORK, &coeffs[by * 4 + bx]);
            }
        }
    } else {
        predict_whole(&mut work, 16, ymode, (mb_x > 0, mb_y > 0));
        for by in 0..4 {
            for bx in 0..4 {
                add_idct(
                    &mut work,
                    (1 + by * 4) * WORK + 8 + bx * 4,
                    WORK,
                    &coeffs[by * 4 + bx],
                );
            }
        }
    }
    for j in 0..16 {
        let row = (1 + j) * WORK + 8;
        plane[(y0 + j) * stride + x0..(y0 + j) * stride + x0 + 16]
            .copy_from_slice(&work[row..row + 16]);
    }
}

fn predict_chroma(
    plane: &mut [u8],
    stride: usize,
    (mb_x, mb_y): (usize, usize),
    mode: usize,
    coeffs: &[[i32; 16]],
) {
    let (x0, y0) = (mb_x * 8, mb_y * 8);
    let mut work = [0u8; WORK * 9];
    borders(&mut work, plane, stride, (x0, y0), 8, None);
    predict_whole(&mut work, 8, mode, (mb_x > 0, mb_y > 0));
    for by in 0..2 {
        for bx in 0..2 {
            add_idct(
                &mut work,
                (1 + by * 4) * WORK + 8 + bx * 4,
                WORK,
                &coeffs[by * 2 + bx],
            );
        }
    }
    for j in 0..8 {
        let row = (1 + j) * WORK + 8;
        plane[(y0 + j) * stride + x0..(y0 + j) * stride + x0 + 8]
            .copy_from_slice(&work[row..row + 8]);
    }
}

/// Predicts a whole `size` x `size` block (16 for luma, 8 for chroma) from the borders in `work`.
fn predict_whole(work: &mut [u8], size: usize, mode: usize, (has_left, has_top): (bool, bool)) {
    let above = |w: &[u8], i: usize| w[8 + i];
    let left = |w: &[u8], j: usize| w[(1 + j) * WORK + 7];
    match mode {
        V_PRED => {
            for j in 0..size {
                for i in 0..size {
                    work[(1 + j) * WORK + 8 + i] = work[8 + i];
                }
            }
        }
        H_PRED => {
            for j in 0..size {
                let l = left(work, j);
                work[(1 + j) * WORK + 8..(1 + j) * WORK + 8 + size].fill(l);
            }
        }
        TM_PRED => {
            let p = i32::from(work[7]);
            for j in 0..size {
                let l = i32::from(left(work, j));
                for i in 0..size {
                    work[(1 + j) * WORK + 8 + i] = clamp8(l + i32::from(above(work, i)) - p);
                }
            }
        }
        _ => {
            let shift = if size == 16 { 3 } else { 2 };
            let sum_above: u32 = (0..size).map(|i| u32::from(above(work, i))).sum();
            let sum_left: u32 = (0..size).map(|j| u32::from(left(work, j))).sum();
            let value = match (has_top, has_left) {
                (true, true) => (sum_above + sum_left + size as u32) >> (shift + 2),
                (true, false) => (sum_above + (size as u32 >> 1)) >> (shift + 1),
                (false, true) => (sum_left + (size as u32 >> 1)) >> (shift + 1),
                (false, false) => 128,
            } as u8;
            for j in 0..size {
                work[(1 + j) * WORK + 8..(1 + j) * WORK + 8 + size].fill(value);
            }
        }
    }
}

/// Predicts one 4x4 sub-block whose top-left pixel is at `base` in the work area.
fn predict_sub(work: &mut [u8], base: usize, mode: usize) {
    let a = |i: usize| work[base - WORK + i];
    let p = work[base - WORK - 1];
    let l = |j: usize| work[base + j * WORK - 1];
    let (a0, a1, a2, a3, a4, a5, a6, a7) = (a(0), a(1), a(2), a(3), a(4), a(5), a(6), a(7));
    let (l0, l1, l2, l3) = (l(0), l(1), l(2), l(3));
    // Edge pixels from the bottom-left corner round to the top-right: L3 L2 L1 L0 P A0 A1 A2 A3.
    let e = [l3, l2, l1, l0, p, a0, a1, a2, a3];
    let mut out = [[0u8; 4]; 4];
    match mode {
        B_DC_PRED => {
            let sum: u32 = [a0, a1, a2, a3, l0, l1, l2, l3]
                .iter()
                .map(|v| u32::from(*v))
                .sum();
            out = [[((sum + 4) >> 3) as u8; 4]; 4];
        }
        B_TM_PRED => {
            for (r, row) in out.iter_mut().enumerate() {
                for (c, v) in row.iter_mut().enumerate() {
                    *v = clamp8(i32::from(l(r)) + i32::from(a(c)) - i32::from(p));
                }
            }
        }
        B_VE_PRED => {
            let top = [
                avg3(p, a0, a1),
                avg3(a0, a1, a2),
                avg3(a1, a2, a3),
                avg3(a2, a3, a4),
            ];
            out = [top; 4];
        }
        B_HE_PRED => {
            out[0] = [avg3(p, l0, l1); 4];
            out[1] = [avg3(l0, l1, l2); 4];
            out[2] = [avg3(l1, l2, l3); 4];
            out[3] = [avg3(l2, l3, l3); 4];
        }
        B_LD_PRED => {
            let aa = [a0, a1, a2, a3, a4, a5, a6, a7];
            for (r, row) in out.iter_mut().enumerate() {
                for (c, v) in row.iter_mut().enumerate() {
                    let i = r + c;
                    *v = if i == 6 {
                        avg3(a6, a7, a7)
                    } else {
                        avg3(aa[i], aa[i + 1], aa[i + 2])
                    };
                }
            }
        }
        B_RD_PRED => {
            for (r, row) in out.iter_mut().enumerate() {
                for (c, v) in row.iter_mut().enumerate() {
                    let i = 4 - r + c;
                    *v = avg3(e[i - 1], e[i], e[i + 1]);
                }
            }
        }
        B_VR_PRED => {
            out[3][0] = avg3(e[1], e[2], e[3]);
            out[2][0] = avg3(e[2], e[3], e[4]);
            out[3][1] = avg3(e[3], e[4], e[5]);
            out[1][0] = out[3][1];
            out[2][1] = avg2(e[4], e[5]);
            out[0][0] = out[2][1];
            out[3][2] = avg3(e[4], e[5], e[6]);
            out[1][1] = out[3][2];
            out[2][2] = avg2(e[5], e[6]);
            out[0][1] = out[2][2];
            out[3][3] = avg3(e[5], e[6], e[7]);
            out[1][2] = out[3][3];
            out[2][3] = avg2(e[6], e[7]);
            out[0][2] = out[2][3];
            out[1][3] = avg3(e[6], e[7], e[8]);
            out[0][3] = avg2(e[7], e[8]);
        }
        B_VL_PRED => {
            out[0][0] = avg2(a0, a1);
            out[1][0] = avg3(a0, a1, a2);
            out[2][0] = avg2(a1, a2);
            out[0][1] = out[2][0];
            out[1][1] = avg3(a1, a2, a3);
            out[3][0] = out[1][1];
            out[2][1] = avg2(a2, a3);
            out[0][2] = out[2][1];
            out[3][1] = avg3(a2, a3, a4);
            out[1][2] = out[3][1];
            out[2][2] = avg2(a3, a4);
            out[0][3] = out[2][2];
            out[3][2] = avg3(a3, a4, a5);
            out[1][3] = out[3][2];
            out[2][3] = avg3(a4, a5, a6);
            out[3][3] = avg3(a5, a6, a7);
        }
        B_HD_PRED => {
            out[3][0] = avg2(e[0], e[1]);
            out[3][1] = avg3(e[0], e[1], e[2]);
            out[2][0] = avg2(e[1], e[2]);
            out[3][2] = out[2][0];
            out[2][1] = avg3(e[1], e[2], e[3]);
            out[3][3] = out[2][1];
            out[2][2] = avg2(e[2], e[3]);
            out[1][0] = out[2][2];
            out[2][3] = avg3(e[2], e[3], e[4]);
            out[1][1] = out[2][3];
            out[1][2] = avg2(e[3], e[4]);
            out[0][0] = out[1][2];
            out[1][3] = avg3(e[3], e[4], e[5]);
            out[0][1] = out[1][3];
            out[0][2] = avg3(e[4], e[5], e[6]);
            out[0][3] = avg3(e[5], e[6], e[7]);
        }
        _ => {
            out[0][0] = avg2(l0, l1);
            out[0][1] = avg3(l0, l1, l2);
            out[0][2] = avg2(l1, l2);
            out[1][0] = out[0][2];
            out[0][3] = avg3(l1, l2, l3);
            out[1][1] = out[0][3];
            out[1][2] = avg2(l2, l3);
            out[2][0] = out[1][2];
            out[1][3] = avg3(l2, l3, l3);
            out[2][1] = out[1][3];
            for (r, c) in [(2, 2), (2, 3), (3, 0), (3, 1), (3, 2), (3, 3)] {
                out[r][c] = l3;
            }
        }
    }
    for (r, row) in out.iter().enumerate() {
        work[base + r * WORK..base + r * WORK + 4].copy_from_slice(row);
    }
}

// ---- the loop filter -------------------------------------------------------------------------------------------

fn sclip1(v: i32) -> i32 {
    v.clamp(-128, 127)
}

fn sclip2(v: i32) -> i32 {
    v.clamp(-16, 15)
}

/// A pixel `k` steps from the edge at `p` (negative `k` is before it).
fn at(buf: &[u8], p: usize, step: usize, k: isize) -> i32 {
    i32::from(buf[(p as isize + k * step as isize) as usize])
}

fn set(buf: &mut [u8], p: usize, step: usize, k: isize, v: i32) {
    buf[(p as isize + k * step as isize) as usize] = clamp8(v);
}

fn needs_filter(buf: &[u8], p: usize, step: usize, t: i32) -> bool {
    let (p1, p0, q0, q1) = (
        at(buf, p, step, -2),
        at(buf, p, step, -1),
        at(buf, p, step, 0),
        at(buf, p, step, 1),
    );
    4 * (p0 - q0).abs() + (p1 - q1).abs() <= t
}

fn needs_filter2(buf: &[u8], p: usize, step: usize, t: i32, it: i32) -> bool {
    if !needs_filter(buf, p, step, t) {
        return false;
    }
    let v = |k: isize| at(buf, p, step, k);
    (v(-4) - v(-3)).abs() <= it
        && (v(-3) - v(-2)).abs() <= it
        && (v(-2) - v(-1)).abs() <= it
        && (v(3) - v(2)).abs() <= it
        && (v(2) - v(1)).abs() <= it
        && (v(1) - v(0)).abs() <= it
}

fn hev(buf: &[u8], p: usize, step: usize, threshold: i32) -> bool {
    let v = |k: isize| at(buf, p, step, k);
    (v(-2) - v(-1)).abs() > threshold || (v(1) - v(0)).abs() > threshold
}

fn filter2(buf: &mut [u8], p: usize, step: usize) {
    let v = |buf: &[u8], k: isize| at(buf, p, step, k);
    let (p1, p0, q0, q1) = (v(buf, -2), v(buf, -1), v(buf, 0), v(buf, 1));
    let a = 3 * (q0 - p0) + sclip1(p1 - q1);
    let (a1, a2) = (sclip2((a + 4) >> 3), sclip2((a + 3) >> 3));
    set(buf, p, step, -1, p0 + a2);
    set(buf, p, step, 0, q0 - a1);
}

fn filter4(buf: &mut [u8], p: usize, step: usize) {
    let v = |buf: &[u8], k: isize| at(buf, p, step, k);
    let (p1, p0, q0, q1) = (v(buf, -2), v(buf, -1), v(buf, 0), v(buf, 1));
    let a = 3 * (q0 - p0);
    let (a1, a2) = (sclip2((a + 4) >> 3), sclip2((a + 3) >> 3));
    let a3 = (a1 + 1) >> 1;
    set(buf, p, step, -2, p1 + a3);
    set(buf, p, step, -1, p0 + a2);
    set(buf, p, step, 0, q0 - a1);
    set(buf, p, step, 1, q1 - a3);
}

fn filter6(buf: &mut [u8], p: usize, step: usize) {
    let v = |buf: &[u8], k: isize| at(buf, p, step, k);
    let (p2, p1, p0, q0, q1, q2) = (
        v(buf, -3),
        v(buf, -2),
        v(buf, -1),
        v(buf, 0),
        v(buf, 1),
        v(buf, 2),
    );
    let a = sclip1(3 * (q0 - p0) + sclip1(p1 - q1));
    let (a1, a2, a3) = ((27 * a + 63) >> 7, (18 * a + 63) >> 7, (9 * a + 63) >> 7);
    set(buf, p, step, -3, p2 + a3);
    set(buf, p, step, -2, p1 + a2);
    set(buf, p, step, -1, p0 + a1);
    set(buf, p, step, 0, q0 - a1);
    set(buf, p, step, 1, q1 - a2);
    set(buf, p, step, 2, q2 - a3);
}

/// One edge, `count` pixels long, filtered with the strong filter (a macroblock edge) or the inner one.
fn filter_edge(
    buf: &mut [u8],
    start: usize,
    (across, along): (usize, usize),
    count: usize,
    (threshold, inner, hev_threshold): (i32, i32, i32),
    macroblock_edge: bool,
) {
    let t = 2 * threshold + 1;
    for i in 0..count {
        let p = start + i * along;
        if needs_filter2(buf, p, across, t, inner) {
            if hev(buf, p, across, hev_threshold) {
                filter2(buf, p, across);
            } else if macroblock_edge {
                filter6(buf, p, across);
            } else {
                filter4(buf, p, across);
            }
        }
    }
}

fn simple_edge(buf: &mut [u8], start: usize, (across, along): (usize, usize), threshold: i32) {
    let t = 2 * threshold + 1;
    for i in 0..16 {
        let p = start + i * along;
        if needs_filter(buf, p, across, t) {
            filter2(buf, p, across);
        }
    }
}

fn loop_filter(
    y: &mut [u8],
    u: &mut [u8],
    v: &mut [u8],
    (mb_w, mb_h): (usize, usize),
    infos: &[FilterInfo],
    simple: bool,
) {
    let (ys, uvs) = (mb_w * 16, mb_w * 8);
    for mb_y in 0..mb_h {
        for mb_x in 0..mb_w {
            let info = infos[mb_y * mb_w + mb_x];
            if info.limit == 0 {
                continue;
            }
            let (yp, uvp) = (mb_y * 16 * ys + mb_x * 16, mb_y * 8 * uvs + mb_x * 8);
            let limit = info.limit;
            if simple {
                if mb_x > 0 {
                    simple_edge(y, yp, (1, ys), limit + 4);
                }
                if info.inner {
                    for k in 1..4 {
                        simple_edge(y, yp + 4 * k, (1, ys), limit);
                    }
                }
                if mb_y > 0 {
                    simple_edge(y, yp, (ys, 1), limit + 4);
                }
                if info.inner {
                    for k in 1..4 {
                        simple_edge(y, yp + 4 * k * ys, (ys, 1), limit);
                    }
                }
                continue;
            }
            let (inner, h) = (info.inner_level, info.hev);
            if mb_x > 0 {
                filter_edge(y, yp, (1, ys), 16, (limit + 4, inner, h), true);
                filter_edge(u, uvp, (1, uvs), 8, (limit + 4, inner, h), true);
                filter_edge(v, uvp, (1, uvs), 8, (limit + 4, inner, h), true);
            }
            if info.inner {
                for k in 1..4 {
                    filter_edge(y, yp + 4 * k, (1, ys), 16, (limit, inner, h), false);
                }
                filter_edge(u, uvp + 4, (1, uvs), 8, (limit, inner, h), false);
                filter_edge(v, uvp + 4, (1, uvs), 8, (limit, inner, h), false);
            }
            if mb_y > 0 {
                filter_edge(y, yp, (ys, 1), 16, (limit + 4, inner, h), true);
                filter_edge(u, uvp, (uvs, 1), 8, (limit + 4, inner, h), true);
                filter_edge(v, uvp, (uvs, 1), 8, (limit + 4, inner, h), true);
            }
            if info.inner {
                for k in 1..4 {
                    filter_edge(y, yp + 4 * k * ys, (ys, 1), 16, (limit, inner, h), false);
                }
                filter_edge(u, uvp + 4 * uvs, (uvs, 1), 8, (limit, inner, h), false);
                filter_edge(v, uvp + 4 * uvs, (uvs, 1), 8, (limit, inner, h), false);
            }
        }
    }
}

// ---- colour ------------------------------------------------------------------------------------------------------

/// YUV to RGB with the fixed-point factors the format's reference decoder uses.
fn yuv_to_rgb(y: i32, u: i32, v: i32) -> [u8; 3] {
    let mult = |a: i32, b: i32| (a * b) >> 8;
    let clip = |v: i32| ((v >> 6).clamp(0, 255)) as u8;
    [
        clip(mult(y, 19077) + mult(v, 26149) - 14234),
        clip(mult(y, 19077) - mult(u, 6419) - mult(v, 13320) + 8708),
        clip(mult(y, 19077) + mult(u, 33050) - 17685),
    ]
}

/// The planes as RGBA, with each chroma sample spread over its four pixels by a triangle filter (3/4 the nearest
/// sample, 1/4 the next, in each direction).
fn to_rgba(
    y: &[u8],
    u: &[u8],
    v: &[u8],
    (width, height): (usize, usize),
    (ys, uvs): (usize, usize),
) -> Image {
    let (cw, ch) = (width.div_ceil(2), height.div_ceil(2));
    let mut rgba = vec![255u8; width * height * 4];
    for py in 0..height {
        let near_y = py / 2;
        let far_y = if py % 2 == 1 {
            (near_y + 1).min(ch - 1)
        } else {
            near_y.saturating_sub(1)
        };
        for px in 0..width {
            let near_x = px / 2;
            let far_x = if px % 2 == 1 {
                (near_x + 1).min(cw - 1)
            } else {
                near_x.saturating_sub(1)
            };
            let mix = |plane: &[u8]| -> i32 {
                let s = |cy: usize, cx: usize| i32::from(plane[cy * uvs + cx]);
                (9 * s(near_y, near_x)
                    + 3 * s(near_y, far_x)
                    + 3 * s(far_y, near_x)
                    + s(far_y, far_x)
                    + 8)
                    >> 4
            };
            let rgb = yuv_to_rgb(i32::from(y[py * ys + px]), mix(u), mix(v));
            rgba[(py * width + px) * 4..(py * width + px) * 4 + 3].copy_from_slice(&rgb);
        }
    }
    Image {
        width: width as u32,
        height: height as u32,
        rgba,
    }
}

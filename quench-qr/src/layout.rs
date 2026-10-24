//! From bytes to modules: the bit stream, its blocks and error correction, the function patterns, the zigzag
//! placement of the data, and the mask that scores best.

use crate::QrCode;
use crate::galois::remainder;
use crate::tables::{EcLevel, blocks, ecc_per_block};

/// Modules per side.
const fn size_of(version: u8) -> usize {
    version as usize * 4 + 17
}

/// Modules that carry data or error correction (everything but the function patterns).
const fn raw_data_modules(version: u8) -> usize {
    let v = version as usize;
    let mut modules = (16 * v + 128) * v + 64;
    if v >= 2 {
        let aligns = v / 7 + 2;
        modules -= (25 * aligns - 10) * aligns - 55;
        if v >= 7 {
            modules -= 36;
        }
    }
    modules
}

/// All codewords of the symbol, data and error correction together.
const fn total_codewords(version: u8) -> usize {
    raw_data_modules(version) / 8
}

/// Codewords that carry data at this level.
const fn data_codewords(version: u8, level: EcLevel) -> usize {
    total_codewords(version) - ecc_per_block(version, level) * blocks(version, level)
}

/// Bits of the character count in byte mode.
const fn count_bits(version: u8) -> usize {
    if version < 10 { 8 } else { 16 }
}

/// How many bytes fit in byte mode.
pub const fn capacity(version: u8, level: EcLevel) -> usize {
    (data_codewords(version, level) * 8 - 4 - count_bits(version)) / 8
}

pub const fn fits(version: u8, level: EcLevel, len: usize) -> bool {
    len <= capacity(version, level)
}

struct BitBuffer(Vec<bool>);

impl BitBuffer {
    fn push(&mut self, value: u32, bits: usize) {
        for i in (0..bits).rev() {
            self.0.push((value >> i) & 1 == 1);
        }
    }
}

/// The data codewords: mode, count, bytes, terminator, padding.
fn data_stream(data: &[u8], version: u8, level: EcLevel) -> Vec<u8> {
    let capacity_bits = data_codewords(version, level) * 8;
    let mut bits = BitBuffer(Vec::with_capacity(capacity_bits));
    bits.push(0b0100, 4);
    bits.push(
        u32::try_from(data.len()).unwrap_or(u32::MAX),
        count_bits(version),
    );
    for &byte in data {
        bits.push(u32::from(byte), 8);
    }
    let terminator = (capacity_bits - bits.0.len()).min(4);
    bits.push(0, terminator);
    while !bits.0.len().is_multiple_of(8) {
        bits.0.push(false);
    }
    let mut out: Vec<u8> = bits
        .0
        .chunks(8)
        .map(|byte| {
            byte.iter()
                .fold(0u8, |acc, bit| (acc << 1) | u8::from(*bit))
        })
        .collect();
    let mut pad = [0xec, 0x11].into_iter().cycle();
    while out.len() < capacity_bits / 8 {
        out.extend(pad.next());
    }
    out
}

/// The data cut into blocks, each with its error correction, interleaved column by column.
fn interleaved(data: &[u8], version: u8, level: EcLevel) -> Vec<u8> {
    let block_count = blocks(version, level);
    let ecc_len = ecc_per_block(version, level);
    let total = total_codewords(version);
    let short_blocks = block_count - total % block_count;
    let short_len = total / block_count - ecc_len;

    let mut data_blocks: Vec<&[u8]> = Vec::with_capacity(block_count);
    let mut ecc_blocks: Vec<Vec<u8>> = Vec::with_capacity(block_count);
    let mut offset = 0;
    for index in 0..block_count {
        let len = short_len + usize::from(index >= short_blocks);
        let block = &data[offset..offset + len];
        offset += len;
        ecc_blocks.push(remainder(block, ecc_len));
        data_blocks.push(block);
    }

    let mut out = Vec::with_capacity(total);
    for column in 0..=short_len {
        for block in &data_blocks {
            if column < block.len() {
                out.push(block[column]);
            }
        }
    }
    for column in 0..ecc_len {
        for block in &ecc_blocks {
            out.push(block[column]);
        }
    }
    out
}

/// The grid under construction: dark modules, and which modules are function patterns (never masked, never
/// filled with data).
struct Grid {
    size: usize,
    dark: Vec<bool>,
    function: Vec<bool>,
}

impl Grid {
    fn new(size: usize) -> Self {
        Self {
            size,
            dark: vec![false; size * size],
            function: vec![false; size * size],
        }
    }

    fn set_function(&mut self, x: usize, y: usize, dark: bool) {
        self.dark[y * self.size + x] = dark;
        self.function[y * self.size + x] = true;
    }

    fn is_function(&self, x: usize, y: usize) -> bool {
        self.function[y * self.size + x]
    }

    fn finder(&mut self, cx: usize, cy: usize) {
        for dy in -4i32..=4 {
            for dx in -4i32..=4 {
                let (Some(x), Some(y)) = (
                    cx.checked_add_signed(dx as isize),
                    cy.checked_add_signed(dy as isize),
                ) else {
                    continue;
                };
                if x < self.size && y < self.size {
                    let distance = dx.abs().max(dy.abs());
                    self.set_function(x, y, distance != 2 && distance != 4);
                }
            }
        }
    }

    fn alignment(&mut self, cx: usize, cy: usize) {
        for dy in -2i32..=2 {
            for dx in -2i32..=2 {
                let x = cx.saturating_add_signed(dx as isize);
                let y = cy.saturating_add_signed(dy as isize);
                self.set_function(x, y, dx.abs().max(dy.abs()) != 1);
            }
        }
    }

    fn function_patterns(&mut self, version: u8) {
        let size = self.size;
        for i in 0..size {
            self.set_function(6, i, i % 2 == 0);
            self.set_function(i, 6, i % 2 == 0);
        }
        self.finder(3, 3);
        self.finder(size - 4, 3);
        self.finder(3, size - 4);
        let positions = alignment_positions(version);
        let last = positions.len().saturating_sub(1);
        for (i, &x) in positions.iter().enumerate() {
            for (j, &y) in positions.iter().enumerate() {
                let corner = (i == 0 && (j == 0 || j == last)) || (i == last && j == 0);
                if !corner {
                    self.alignment(x, y);
                }
            }
        }
        self.format_bits(EcLevel::L, 0);
        self.version_bits(version);
    }

    /// The 15 format bits (level and mask, BCH-protected, XOR-masked), written twice, and the fixed dark module.
    fn format_bits(&mut self, level: EcLevel, mask: u8) {
        let data = (level.format_bits() << 3) | u32::from(mask);
        let mut remainder = data;
        for _ in 0..10 {
            remainder = (remainder << 1) ^ ((remainder >> 9) * 0x537);
        }
        let bits = ((data << 10) | remainder) ^ 0x5412;
        let bit = |i: usize| (bits >> i) & 1 == 1;
        let size = self.size;
        for i in 0..6 {
            self.set_function(8, i, bit(i));
        }
        self.set_function(8, 7, bit(6));
        self.set_function(8, 8, bit(7));
        self.set_function(7, 8, bit(8));
        for i in 9..15 {
            self.set_function(14 - i, 8, bit(i));
        }
        for i in 0..8 {
            self.set_function(size - 1 - i, 8, bit(i));
        }
        for i in 8..15 {
            self.set_function(8, size - 15 + i, bit(i));
        }
        self.set_function(8, size - 8, true);
    }

    /// Versions 7 and up carry 18 version bits, twice.
    fn version_bits(&mut self, version: u8) {
        if version < 7 {
            return;
        }
        let mut remainder = u32::from(version);
        for _ in 0..12 {
            remainder = (remainder << 1) ^ ((remainder >> 11) * 0x1f25);
        }
        let bits = (u32::from(version) << 12) | remainder;
        for i in 0..18 {
            let dark = (bits >> i) & 1 == 1;
            let a = self.size - 11 + i % 3;
            let b = i / 3;
            self.set_function(a, b, dark);
            self.set_function(b, a, dark);
        }
    }

    /// Writes the codeword bits in the standard's zigzag: pairs of columns from the right, up then down,
    /// skipping the vertical timing column and every function module.
    fn place(&mut self, codewords: &[u8]) {
        let size = self.size;
        let mut bit = 0usize;
        let mut right = size - 1;
        while right >= 1 {
            if right == 6 {
                right = 5;
            }
            for vertical in 0..size {
                for j in 0..2 {
                    let x = right - j;
                    let upward = ((right + 1) & 2) == 0;
                    let y = if upward {
                        size - 1 - vertical
                    } else {
                        vertical
                    };
                    if !self.is_function(x, y) && bit < codewords.len() * 8 {
                        self.dark[y * size + x] = (codewords[bit >> 3] >> (7 - (bit & 7))) & 1 == 1;
                        bit += 1;
                    }
                }
            }
            if right < 2 {
                break;
            }
            right -= 2;
        }
    }

    fn apply_mask(&mut self, mask: u8) {
        for y in 0..self.size {
            for x in 0..self.size {
                if !self.is_function(x, y) && mask_flips(mask, x, y) {
                    self.dark[y * self.size + x] = !self.dark[y * self.size + x];
                }
            }
        }
    }

    /// The standard's four penalty rules; lower is better.
    fn penalty(&self) -> u32 {
        let size = self.size;
        let at = |x: usize, y: usize| self.dark[y * size + x];
        let mut score = 0u32;
        // Rule 1: runs of five or more same-coloured modules in a row or column.
        for line in 0..size {
            for vertical in [false, true] {
                let get = |i: usize| if vertical { at(line, i) } else { at(i, line) };
                let mut run = 1u32;
                for i in 1..size {
                    if get(i) == get(i - 1) {
                        run += 1;
                    } else {
                        if run >= 5 {
                            score += run - 2;
                        }
                        run = 1;
                    }
                }
                if run >= 5 {
                    score += run - 2;
                }
            }
        }
        // Rule 2: 2x2 blocks of one colour.
        for y in 0..size - 1 {
            for x in 0..size - 1 {
                let colour = at(x, y);
                if at(x + 1, y) == colour && at(x, y + 1) == colour && at(x + 1, y + 1) == colour {
                    score += 3;
                }
            }
        }
        // Rule 3: the finder-like pattern 1:1:3:1:1 with four light modules on one side.
        for line in 0..size {
            for vertical in [false, true] {
                // The symbol is surrounded by light modules.
                let padded: Vec<bool> = (0..size + 8)
                    .map(|i| {
                        i.checked_sub(4)
                            .filter(|j| *j < size)
                            .is_some_and(|j| if vertical { at(line, j) } else { at(j, line) })
                    })
                    .collect();
                for window in padded.windows(11) {
                    if window == PATTERN_A || window == PATTERN_B {
                        score += 40;
                    }
                }
            }
        }
        // Rule 4: how far the share of dark modules strays from half.
        let dark = self.dark.iter().filter(|d| **d).count();
        let total = size * size;
        let deviation = (dark * 20).abs_diff(total * 10);
        let steps = deviation.div_ceil(total).saturating_sub(1);
        score + u32::try_from(steps).unwrap_or(u32::MAX) * 10
    }
}

/// The finder-like pattern 1:1:3:1:1 followed by four light modules, and its mirror image.
const PATTERN_A: [bool; 11] = [
    true, false, true, true, true, false, true, false, false, false, false,
];
const PATTERN_B: [bool; 11] = [
    false, false, false, false, true, false, true, true, true, false, true,
];

const fn mask_flips(mask: u8, x: usize, y: usize) -> bool {
    match mask {
        0 => (x + y).is_multiple_of(2),
        1 => y.is_multiple_of(2),
        2 => x.is_multiple_of(3),
        3 => (x + y).is_multiple_of(3),
        4 => (x / 3 + y / 2).is_multiple_of(2),
        5 => x * y % 2 + x * y % 3 == 0,
        6 => (x * y % 2 + x * y % 3).is_multiple_of(2),
        _ => ((x + y) % 2 + x * y % 3).is_multiple_of(2),
    }
}

/// Where the alignment patterns sit, along one axis.
fn alignment_positions(version: u8) -> Vec<usize> {
    if version == 1 {
        return Vec::new();
    }
    let v = usize::from(version);
    let count = v / 7 + 2;
    let step = (v * 8 + count * 3 + 5) / (count * 4 - 4) * 2;
    // 6, then `count - 1` positions counted back from the far edge in equal steps.
    let last = size_of(version) - 7;
    let mut positions = vec![6];
    positions.extend((0..count - 1).rev().map(|i| last - i * step));
    positions
}

pub fn build(data: &[u8], version: u8, level: EcLevel) -> QrCode {
    let codewords = interleaved(&data_stream(data, version, level), version, level);
    let size = size_of(version);
    let mut grid = Grid::new(size);
    grid.function_patterns(version);
    grid.place(&codewords);

    let mut best: Option<(u32, u8)> = None;
    for mask in 0..8u8 {
        grid.apply_mask(mask);
        grid.format_bits(level, mask);
        let penalty = grid.penalty();
        if best.is_none_or(|(lowest, _)| penalty < lowest) {
            best = Some((penalty, mask));
        }
        grid.apply_mask(mask); // masks are involutions
    }
    let mask = best.map_or(0, |(_, mask)| mask);
    grid.apply_mask(mask);
    grid.format_bits(level, mask);
    QrCode {
        version,
        level,
        mask,
        size,
        modules: grid.dark,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[quench_test::test]
    fn capacities_match_the_standards_byte_mode_column() {
        // ISO 18004 table 7 (byte mode) for a few versions and levels.
        assert_eq!(capacity(1, EcLevel::L), 17);
        assert_eq!(capacity(1, EcLevel::M), 14);
        assert_eq!(capacity(1, EcLevel::H), 7);
        assert_eq!(capacity(10, EcLevel::M), 213);
        assert_eq!(capacity(40, EcLevel::L), 2953);
        assert_eq!(capacity(40, EcLevel::M), 2331);
        assert_eq!(capacity(40, EcLevel::Q), 1663);
        assert_eq!(capacity(40, EcLevel::H), 1273);
        assert!(fits(1, EcLevel::M, 14) && !fits(1, EcLevel::M, 15));
    }

    #[quench_test::test]
    fn version_geometry_is_the_standards() {
        assert_eq!(size_of(1), 21);
        assert_eq!(size_of(40), 177);
        assert_eq!(total_codewords(1), 26);
        assert_eq!(total_codewords(7), 196);
        assert_eq!(total_codewords(40), 3706);
        assert_eq!(alignment_positions(1), Vec::<usize>::new());
        assert_eq!(alignment_positions(2), [6, 18]);
        assert_eq!(alignment_positions(7), [6, 22, 38]);
        assert_eq!(alignment_positions(32), [6, 34, 60, 86, 112, 138]);
        assert_eq!(alignment_positions(40), [6, 30, 58, 86, 114, 142, 170]);
    }

    #[quench_test::test]
    fn the_data_stream_is_padded_to_the_capacity() {
        let stream = data_stream(b"hi", 1, EcLevel::M);
        assert_eq!(stream.len(), data_codewords(1, EcLevel::M));
        assert_eq!(stream[0], 0x40);
        assert_eq!(stream[1], 0x26);
        assert_eq!(&stream[stream.len() - 2..], [0xec, 0x11].map(|b| b));
    }

    #[quench_test::test]
    fn every_mask_pattern_is_an_involution_that_spares_function_modules() {
        let mut grid = Grid::new(21);
        grid.function_patterns(1);
        let before = grid.dark.clone();
        for mask in 0..8 {
            grid.apply_mask(mask);
            assert!(
                (0..21).all(|y| (0..21).all(
                    |x| !grid.is_function(x, y) || grid.dark[y * 21 + x] == before[y * 21 + x]
                ))
            );
            grid.apply_mask(mask);
            assert_eq!(grid.dark, before);
        }
    }
}

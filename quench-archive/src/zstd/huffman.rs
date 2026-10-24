//! zstd's Huffman literals: reading the weight table and decoding streams, and building codes to encode.

use super::bits::{BackwardReader, BackwardWriter};
use super::fse::{self, Table};
use crate::error::{Error, Result};
use crate::huffman::code_lengths;

/// The longest code zstd allows.
const MAX_BITS: u8 = 11;

#[derive(Debug, Clone)]
pub struct DecodeTable {
    max_bits: u8,
    /// Indexed by the next `max_bits` bits: the symbol and how many of them its code uses.
    cells: Vec<(u8, u8)>,
}

impl DecodeTable {
    fn from_weights(listed: &[u8]) -> Result<Self> {
        if listed.iter().any(|&w| w > MAX_BITS) {
            return Err(Error::corrupt("Huffman weight too large"));
        }
        let total: u32 = listed
            .iter()
            .filter(|&&w| w > 0)
            .map(|&w| 1u32 << (w - 1))
            .sum();
        if total == 0 {
            return Err(Error::corrupt("Huffman table has no symbols"));
        }
        let max_bits = (32 - total.leading_zeros()) as u8;
        if max_bits > MAX_BITS {
            return Err(Error::corrupt("Huffman code too long"));
        }
        let left = (1u32 << max_bits) - total;
        if !left.is_power_of_two() {
            return Err(Error::corrupt("Huffman weights do not complete the code"));
        }
        let mut weights = listed.to_vec();
        weights.push(left.trailing_zeros() as u8 + 1);

        let mut cells = vec![(0u8, 0u8); 1 << max_bits];
        let mut position = 0usize;
        for weight in 1..=max_bits {
            for (symbol, _) in weights.iter().enumerate().filter(|&(_, &w)| w == weight) {
                let run = 1usize << (weight - 1);
                cells[position..position + run].fill((symbol as u8, max_bits + 1 - weight));
                position += run;
            }
        }
        debug_assert_eq!(position, cells.len());
        Ok(Self { max_bits, cells })
    }

    /// Decodes exactly `count` symbols from one backward stream, which must be consumed completely.
    pub fn decode(&self, data: &[u8], count: usize, out: &mut Vec<u8>) -> Result<()> {
        let mut reader = BackwardReader::new(data)?;
        for _ in 0..count {
            let (symbol, bits) = self.cells[reader.peek(u32::from(self.max_bits)) as usize];
            reader.consume(u32::from(bits))?;
            out.push(symbol);
        }
        if reader.remaining() == 0 {
            Ok(())
        } else {
            Err(Error::corrupt("Huffman stream has unused bits"))
        }
    }
}

/// Reads a table description from the front of `data`; returns the table and the bytes it took.
pub fn read_table(data: &[u8]) -> Result<(DecodeTable, usize)> {
    let header = *data.first().ok_or(Error::Truncated)?;
    let (weights, used) = if header >= 128 {
        let count = usize::from(header) - 127;
        let bytes = count.div_ceil(2);
        let packed = data.get(1..1 + bytes).ok_or(Error::Truncated)?;
        let weights = (0..count)
            .map(|i| {
                if i % 2 == 0 {
                    packed[i / 2] >> 4
                } else {
                    packed[i / 2] & 15
                }
            })
            .collect();
        (weights, 1 + bytes)
    } else {
        let size = usize::from(header);
        let compressed = data.get(1..1 + size).ok_or(Error::Truncated)?;
        (fse_weights(compressed)?, 1 + size)
    };
    if weights.len() > 255 {
        return Err(Error::corrupt("too many Huffman weights"));
    }
    Ok((DecodeTable::from_weights(&weights)?, used))
}

/// Weights stored as an FSE stream: a distribution, then two interleaved states over a backward stream.
fn fse_weights(data: &[u8]) -> Result<Vec<u8>> {
    let (counts, accuracy, used) = fse::read_distribution(data, 12, 6)?;
    let table = Table::build(&counts, accuracy)?;
    let mut reader = BackwardReader::new(&data[used..])?;
    let mut states = [
        reader.read(u32::from(accuracy))? as usize,
        reader.read(u32::from(accuracy))? as usize,
    ];
    let mut weights = Vec::new();
    let mut turn = 0;
    loop {
        let cell = table.cells[states[turn]];
        weights.push(cell.symbol);
        if weights.len() > 255 {
            return Err(Error::corrupt("too many Huffman weights"));
        }
        if reader.remaining() < usize::from(cell.bits) {
            // This state cannot advance: the stream ends here, with the other state's symbol.
            weights.push(table.cells[states[1 - turn]].symbol);
            return Ok(weights);
        }
        states[turn] = usize::from(cell.baseline) + reader.read(u32::from(cell.bits))? as usize;
        turn = 1 - turn;
    }
}

// ---------------------------------------------------------------- encoding

pub struct Encoder {
    /// `(code, bits)` per byte value.
    codes: Vec<(u16, u8)>,
    /// Weights of the symbols before the last used one.
    weights: Vec<u8>,
}

impl Encoder {
    /// A code for the byte frequencies, or `None` when the table cannot be written in the direct
    /// form (a symbol above 128 is used) or fewer than two symbols occur.
    pub fn new(frequencies: &[u32; 256]) -> Option<Self> {
        let used = frequencies.iter().filter(|&&n| n > 0).count();
        if used < 2 {
            return None;
        }
        let last = frequencies.iter().rposition(|&n| n > 0)?;
        if last > 128 {
            return None;
        }
        let lengths = code_lengths(frequencies, MAX_BITS);
        let max_bits = *lengths.iter().max()?;
        let weight = |s: usize| {
            if lengths[s] == 0 {
                0
            } else {
                max_bits + 1 - lengths[s]
            }
        };

        let mut codes = vec![(0u16, 0u8); 256];
        let mut position = 0u32;
        for w in 1..=max_bits {
            for s in (0..256).filter(|&s| weight(s) == w) {
                codes[s] = ((position >> (w - 1)) as u16, lengths[s]);
                position += 1 << (w - 1);
            }
        }
        debug_assert_eq!(position, 1 << max_bits);
        let weights = (0..last).map(weight).collect();
        Some(Self { codes, weights })
    }

    /// The table description (direct weights).
    pub fn table(&self) -> Vec<u8> {
        let mut out = vec![127 + self.weights.len() as u8];
        for pair in self.weights.chunks(2) {
            out.push(pair[0] << 4 | pair.get(1).copied().unwrap_or(0));
        }
        out
    }

    /// One backward stream holding all of `data`.
    pub fn stream(&self, data: &[u8]) -> Vec<u8> {
        let mut writer = BackwardWriter::new();
        for &byte in data.iter().rev() {
            let (code, bits) = self.codes[usize::from(byte)];
            writer.add(u32::from(code), u32::from(bits));
        }
        writer.finish()
    }
}

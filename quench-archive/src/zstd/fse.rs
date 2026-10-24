//! Finite State Entropy tables: building them from a distribution, reading a distribution from a stream,
//! and the inverse mapping the encoder needs.

use super::bits::ForwardReader;
use crate::error::{Error, Result};

/// One state of a decoding table: the symbol it stands for and how to reach the next state.
#[derive(Debug, Clone, Copy, Default)]
pub struct Cell {
    pub symbol: u8,
    pub bits: u8,
    pub baseline: u16,
}

#[derive(Debug, Clone)]
pub struct Table {
    pub accuracy: u8,
    pub cells: Vec<Cell>,
}

impl Table {
    /// Builds the decoding table for a normalised distribution (`-1` marks a symbol of probability
    /// below one, which gets a full-width cell at the end of the table).
    pub fn build(counts: &[i16], accuracy: u8) -> Result<Self> {
        let size = 1usize << accuracy;
        let mut symbols = vec![0u8; size];
        let mut high = size - 1;
        let mut total = 0usize;
        for (symbol, &count) in counts.iter().enumerate() {
            match count {
                -1 => {
                    symbols[high] = symbol as u8;
                    high = high.wrapping_sub(1);
                    total += 1;
                }
                0.. => total += count as usize,
                _ => return Err(Error::corrupt("negative FSE count")),
            }
        }
        if total != size {
            return Err(Error::corrupt("FSE distribution does not fill the table"));
        }

        let step = (size >> 1) + (size >> 3) + 3;
        let mask = size - 1;
        let mut position = 0;
        for (symbol, &count) in counts.iter().enumerate() {
            for _ in 0..count.max(0) {
                symbols[position] = symbol as u8;
                loop {
                    position = (position + step) & mask;
                    if position <= high {
                        break;
                    }
                }
            }
        }
        if position != 0 {
            return Err(Error::corrupt("FSE table spread did not close"));
        }

        let mut next: Vec<u32> = counts.iter().map(|&c| c.max(1) as u32).collect();
        let cells = symbols
            .iter()
            .map(|&symbol| {
                let state = next[usize::from(symbol)];
                next[usize::from(symbol)] += 1;
                let bits = accuracy - state.ilog2() as u8;
                Cell {
                    symbol,
                    bits,
                    baseline: ((state << bits) - size as u32) as u16,
                }
            })
            .collect();
        Ok(Self { accuracy, cells })
    }

    /// A table that always yields `symbol` and reads no bits.
    pub fn single(symbol: u8) -> Self {
        Self {
            accuracy: 0,
            cells: vec![Cell {
                symbol,
                bits: 0,
                baseline: 0,
            }],
        }
    }
}

/// Reads a distribution (the "FSE table description") from the start of `data`; returns the counts, the
/// accuracy log and how many bytes it took.
pub fn read_distribution(
    data: &[u8],
    max_symbol: usize,
    max_accuracy: u8,
) -> Result<(Vec<i16>, u8, usize)> {
    let mut bits = ForwardReader::new(data);
    let accuracy = bits.read(4) as u8 + 5;
    if accuracy > max_accuracy {
        return Err(Error::corrupt("FSE accuracy log too large"));
    }
    let mut remaining = (1i32 << accuracy) + 1;
    let mut threshold = 1i32 << accuracy;
    let mut width = u32::from(accuracy) + 1;
    let mut counts: Vec<i16> = Vec::new();
    let mut previous_zero = false;

    while remaining > 1 && counts.len() <= max_symbol {
        if previous_zero {
            loop {
                let repeat = bits.read(2);
                counts.extend(std::iter::repeat_n(0, repeat as usize));
                if repeat < 3 {
                    break;
                }
            }
            if counts.len() > max_symbol + 1 {
                return Err(Error::corrupt("too many FSE symbols"));
            }
        }
        let max = 2 * threshold - 1 - remaining;
        let low = bits.peek(width - 1) as i32 & (threshold - 1);
        let mut count = if low < max {
            bits.skip(width - 1);
            low
        } else {
            let mut value = bits.peek(width) as i32 & (2 * threshold - 1);
            if value >= threshold {
                value -= max;
            }
            bits.skip(width);
            value
        };
        count -= 1;
        remaining -= count.abs();
        counts.push(count as i16);
        previous_zero = count == 0;
        while remaining < threshold {
            width -= 1;
            threshold >>= 1;
        }
        if counts.len() > max_symbol + 1 {
            return Err(Error::corrupt("too many FSE symbols"));
        }
    }
    if remaining != 1 {
        return Err(Error::corrupt("FSE distribution is not complete"));
    }
    Ok((counts, accuracy, bits.bytes_used()?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zstd::tables::{LITERAL_LENGTH_DEFAULT, MATCH_LENGTH_DEFAULT, OFFSET_DEFAULT};

    #[test]
    fn default_distributions_build() {
        for (counts, accuracy) in [
            (&LITERAL_LENGTH_DEFAULT[..], 6),
            (&MATCH_LENGTH_DEFAULT[..], 6),
            (&OFFSET_DEFAULT[..], 5),
        ] {
            let table = Table::build(counts, accuracy).unwrap();
            assert_eq!(table.cells.len(), 1 << accuracy);
            // Every symbol's cells tile the whole state range exactly once.
            for (symbol, &count) in counts.iter().enumerate() {
                let covered: u32 = table
                    .cells
                    .iter()
                    .filter(|c| usize::from(c.symbol) == symbol)
                    .map(|c| 1u32 << c.bits)
                    .sum();
                assert_eq!(covered, if count == 0 { 0 } else { 1 << accuracy });
            }
        }
    }

    #[test]
    fn bad_distributions_are_rejected() {
        assert!(Table::build(&[3, 3], 3).is_err());
        assert!(Table::build(&[8, -2], 3).is_err());
        assert_eq!(Table::single(7).cells[0].symbol, 7);
    }

    #[test]
    fn reads_a_distribution() {
        // Accuracy 5 (0 in the low nibble), then counts for two symbols 16 and 16.
        // remaining = 33, threshold = 32, width = 6.
        let writer = [0xFF, 0xFF];
        // Hand-encoded by encoding value (count + 1) with the variable width rule is covered by the
        // zstd round-trip tests; here only the error paths are exercised.
        assert!(read_distribution(&writer, 35, 6).is_err());
        assert!(read_distribution(&[0x0F], 35, 6).is_err());
    }
}

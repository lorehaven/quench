//! The bit streams zstd uses: forward (the FSE table description) and backward (everything else).

use crate::error::{Error, Result};

/// Reads little-endian bits from the start of a slice.
pub struct ForwardReader<'a> {
    data: &'a [u8],
    bit: usize,
}

impl<'a> ForwardReader<'a> {
    pub const fn new(data: &'a [u8]) -> Self {
        Self { data, bit: 0 }
    }

    /// The next `n` (≤ 32) bits without consuming them; bits past the end read as zero.
    pub fn peek(&self, n: u32) -> u32 {
        let mut value = 0u64;
        let first = self.bit / 8;
        for i in 0..5 {
            value |= u64::from(self.data.get(first + i).copied().unwrap_or(0)) << (8 * i);
        }
        ((value >> (self.bit % 8)) & ((1u64 << n) - 1)) as u32
    }

    pub const fn skip(&mut self, n: u32) {
        self.bit += n as usize;
    }

    pub fn read(&mut self, n: u32) -> u32 {
        let value = self.peek(n);
        self.skip(n);
        value
    }

    /// Whole bytes touched so far, or an error if the reader ran past the data.
    pub const fn bytes_used(&self) -> Result<usize> {
        let used = self.bit.div_ceil(8);
        if used > self.data.len() {
            Err(Error::Truncated)
        } else {
            Ok(used)
        }
    }
}

/// Reads a stream from its end: the last byte holds a 1-bit sentinel above the data, and bits are
/// consumed from the most significant downwards.
pub struct BackwardReader<'a> {
    data: &'a [u8],
    /// Data bits not yet consumed.
    remaining: usize,
}

impl<'a> BackwardReader<'a> {
    pub fn new(data: &'a [u8]) -> Result<Self> {
        let last = *data.last().ok_or(Error::Truncated)?;
        if last == 0 {
            return Err(Error::corrupt("bit stream has no end marker"));
        }
        let sentinel = 7 - last.leading_zeros() as usize;
        Ok(Self {
            data,
            remaining: (data.len() - 1) * 8 + sentinel,
        })
    }

    pub const fn remaining(&self) -> usize {
        self.remaining
    }

    /// The bits `[start, start + n)` of the stream, n ≤ 32.
    fn bits_at(&self, start: usize, n: u32) -> u32 {
        if n == 0 {
            return 0;
        }
        let first = start / 8;
        let mut value = 0u64;
        for i in 0..5 {
            value |= u64::from(self.data.get(first + i).copied().unwrap_or(0)) << (8 * i);
        }
        ((value >> (start % 8)) & ((1u64 << n) - 1)) as u32
    }

    /// Takes the next `n` bits; fails when fewer are left.
    pub fn read(&mut self, n: u32) -> Result<u32> {
        let n_bits = n as usize;
        if n_bits > self.remaining {
            return Err(Error::corrupt("bit stream ends early"));
        }
        self.remaining -= n_bits;
        Ok(self.bits_at(self.remaining, n))
    }

    /// The next `n` bits without consuming them; bits before the start of the stream read as zero.
    pub fn peek(&self, n: u32) -> u32 {
        let n_bits = n as usize;
        if n_bits <= self.remaining {
            self.bits_at(self.remaining - n_bits, n)
        } else if self.remaining == 0 {
            0
        } else {
            self.bits_at(0, self.remaining as u32) << (n_bits - self.remaining)
        }
    }

    /// Drops `n` bits that [`peek`](Self::peek) looked at.
    pub fn consume(&mut self, n: u32) -> Result<()> {
        let n_bits = n as usize;
        if n_bits > self.remaining {
            return Err(Error::corrupt("bit stream ends early"));
        }
        self.remaining -= n_bits;
        Ok(())
    }
}

/// Writes a stream for [`BackwardReader`]: bits go in at the bottom, and [`finish`](Self::finish) adds
/// the sentinel.
pub struct BackwardWriter {
    out: Vec<u8>,
    buffer: u64,
    count: u32,
}

impl BackwardWriter {
    pub const fn new() -> Self {
        Self {
            out: Vec::new(),
            buffer: 0,
            count: 0,
        }
    }

    /// Adds the low `n` (≤ 32) bits of `value`.
    pub fn add(&mut self, value: u32, n: u32) {
        debug_assert!(n <= 32 && (n == 32 || value < (1 << n)));
        self.buffer |= u64::from(value) << self.count;
        self.count += n;
        while self.count >= 8 {
            self.out.push(self.buffer as u8);
            self.buffer >>= 8;
            self.count -= 8;
        }
    }

    pub fn finish(mut self) -> Vec<u8> {
        self.add(1, 1);
        if self.count > 0 {
            self.out.push(self.buffer as u8);
        }
        self.out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[quench_test::test]
    fn backward_stream_round_trips() {
        let mut writer = BackwardWriter::new();
        let values = [
            (5u32, 3u32),
            (0, 1),
            (0xABCD, 16),
            (1, 1),
            (0x1F_FFFF, 21),
            (0, 7),
        ];
        for (v, n) in values {
            writer.add(v, n);
        }
        let bytes = writer.finish();
        let mut reader = BackwardReader::new(&bytes).unwrap();
        assert_eq!(reader.remaining(), 3 + 1 + 16 + 1 + 21 + 7);
        for (v, n) in values.iter().rev() {
            assert_eq!(reader.peek(*n), *v);
            assert_eq!(reader.read(*n).unwrap(), *v);
        }
        assert_eq!(reader.remaining(), 0);
        assert!(reader.read(1).is_err());
        assert!(reader.consume(1).is_err());
        assert_eq!(reader.peek(5), 0);
    }

    #[quench_test::test]
    fn peeking_past_the_start_pads_with_zeros() {
        let mut writer = BackwardWriter::new();
        writer.add(0b101, 3);
        let bytes = writer.finish();
        let reader = BackwardReader::new(&bytes).unwrap();
        assert_eq!(reader.peek(5), 0b10100);
    }

    #[quench_test::test]
    fn a_stream_needs_an_end_marker() {
        assert!(matches!(BackwardReader::new(&[]), Err(Error::Truncated)));
        assert!(matches!(
            BackwardReader::new(&[1, 0]),
            Err(Error::Corrupt(_))
        ));
    }

    #[quench_test::test]
    fn forward_reader_reads_little_endian() {
        let mut reader = ForwardReader::new(&[0b1010_0101, 0xFF]);
        assert_eq!(reader.read(4), 0b0101);
        assert_eq!(reader.peek(4), 0b1010);
        reader.skip(4);
        assert_eq!(reader.read(8), 0xFF);
        assert_eq!(reader.bytes_used().unwrap(), 2);
        reader.skip(1);
        assert!(reader.bytes_used().is_err());
        assert_eq!(reader.read(3), 0);
    }
}

//! Raw deflate (RFC 1951): a decoder, and an LZ77 encoder with fixed and dynamic Huffman blocks.

use crate::error::{Error, Result};
use crate::huffman::code_lengths;

const LENGTH_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
const LENGTH_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
const DISTANCE_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DISTANCE_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];
/// The order the code-length code's own lengths are stored in.
const CODE_LENGTH_ORDER: [usize; 19] = [
    16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
];

const END_OF_BLOCK: usize = 256;
const WINDOW: usize = 32 * 1024;
const MAX_MATCH: usize = 258;
const MIN_MATCH: usize = 3;

// ---------------------------------------------------------------- decoding

struct BitReader<'a> {
    data: &'a [u8],
    position: usize,
    buffer: u64,
    count: u32,
}

impl<'a> BitReader<'a> {
    const fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            position: 0,
            buffer: 0,
            count: 0,
        }
    }

    fn fill(&mut self) {
        while self.count <= 56 && self.position < self.data.len() {
            self.buffer |= u64::from(self.data[self.position]) << self.count;
            self.position += 1;
            self.count += 8;
        }
    }

    fn bits(&mut self, n: u32) -> Result<u32> {
        if n == 0 {
            return Ok(0);
        }
        if self.count < n {
            self.fill();
            if self.count < n {
                return Err(Error::Truncated);
            }
        }
        let value = (self.buffer & ((1u64 << n) - 1)) as u32;
        self.buffer >>= n;
        self.count -= n;
        Ok(value)
    }

    /// Drops the bits left in the current byte.
    const fn align(&mut self) {
        let drop = self.count % 8;
        self.buffer >>= drop;
        self.count -= drop;
    }

    /// Bytes consumed so far, counting a partly read byte as consumed.
    const fn consumed(&self) -> usize {
        self.position - (self.count / 8) as usize
    }

    fn bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        debug_assert_eq!(self.count % 8, 0);
        // Hand the whole bytes still in the bit buffer back to the slice.
        self.position -= (self.count / 8) as usize;
        self.buffer = 0;
        self.count = 0;
        let end = self.position.checked_add(n).ok_or(Error::Truncated)?;
        let slice = self.data.get(self.position..end).ok_or(Error::Truncated)?;
        self.position = end;
        Ok(slice)
    }
}

/// A canonical Huffman code, decoded a bit at a time.
struct Decoder {
    count: [u16; 16],
    symbol: Vec<u16>,
}

impl Decoder {
    fn new(lengths: &[u8]) -> Result<Self> {
        let mut count = [0u16; 16];
        for &length in lengths {
            count[usize::from(length)] += 1;
        }
        let used = lengths.len() - usize::from(count[0]);
        let mut left: i32 = 1;
        for &c in &count[1..] {
            left = (left << 1) - i32::from(c);
            if left < 0 {
                return Err(Error::corrupt("over-subscribed Huffman code"));
            }
        }
        // An incomplete code is only valid as the single one-bit code of an almost unused alphabet.
        if left > 0 && used > 0 && !(used == 1 && count[1] == 1) {
            return Err(Error::corrupt("incomplete Huffman code"));
        }
        let mut offsets = [0u16; 16];
        for length in 1..15 {
            offsets[length + 1] = offsets[length] + count[length];
        }
        let mut symbol = vec![0u16; used];
        for (s, &length) in lengths.iter().enumerate() {
            if length != 0 {
                symbol[usize::from(offsets[usize::from(length)])] = s as u16;
                offsets[usize::from(length)] += 1;
            }
        }
        Ok(Self { count, symbol })
    }

    fn decode(&self, reader: &mut BitReader<'_>) -> Result<usize> {
        let (mut code, mut first, mut index) = (0i32, 0i32, 0i32);
        for length in 1..16 {
            code |= reader.bits(1)? as i32;
            let count = i32::from(self.count[length]);
            if code - count < first {
                return Ok(usize::from(self.symbol[(index + (code - first)) as usize]));
            }
            index += count;
            first = (first + count) << 1;
            code <<= 1;
        }
        Err(Error::corrupt("invalid Huffman code"))
    }
}

fn fixed_lengths() -> (Vec<u8>, Vec<u8>) {
    let mut literal = vec![8u8; 288];
    literal[144..256].fill(9);
    literal[256..280].fill(7);
    (literal, vec![5u8; 32])
}

/// Decompresses a raw deflate stream, refusing to produce more than `limit` bytes. Returns the data and
/// how many input bytes the stream occupied (anything after is the caller's, such as a gzip trailer).
///
/// # Errors
///
/// [`Error::Corrupt`] or [`Error::Truncated`] for a malformed stream, [`Error::Limit`] past `limit`.
pub fn inflate(data: &[u8], limit: usize) -> Result<(Vec<u8>, usize)> {
    let mut reader = BitReader::new(data);
    let mut out: Vec<u8> = Vec::new();
    loop {
        let last = reader.bits(1)? == 1;
        match reader.bits(2)? {
            0 => {
                reader.align();
                let header = reader.bytes(4)?;
                let length = usize::from(u16::from_le_bytes([header[0], header[1]]));
                let inverse = u16::from_le_bytes([header[2], header[3]]);
                if inverse != !(length as u16) {
                    return Err(Error::corrupt("stored block length check failed"));
                }
                if out.len() + length > limit {
                    return Err(Error::Limit(format!("more than {limit} bytes")));
                }
                out.extend_from_slice(reader.bytes(length)?);
            }
            1 => {
                let (literal, distance) = fixed_lengths();
                block(
                    &mut reader,
                    &mut out,
                    limit,
                    &Decoder::new(&literal)?,
                    &Decoder::new(&distance)?,
                )?;
            }
            2 => {
                let (literal, distance) = dynamic_codes(&mut reader)?;
                block(&mut reader, &mut out, limit, &literal, &distance)?;
            }
            _ => return Err(Error::corrupt("reserved block type")),
        }
        if last {
            break;
        }
    }
    Ok((out, reader.consumed()))
}

fn dynamic_codes(reader: &mut BitReader<'_>) -> Result<(Decoder, Decoder)> {
    let literals = reader.bits(5)? as usize + 257;
    let distances = reader.bits(5)? as usize + 1;
    let code_lengths_count = reader.bits(4)? as usize + 4;
    if literals > 286 || distances > 30 {
        return Err(Error::corrupt("too many length or distance codes"));
    }

    let mut order = [0u8; 19];
    for &slot in &CODE_LENGTH_ORDER[..code_lengths_count] {
        order[slot] = reader.bits(3)? as u8;
    }
    let length_code = Decoder::new(&order)?;

    let mut lengths = vec![0u8; literals + distances];
    let mut index = 0;
    while index < lengths.len() {
        let symbol = length_code.decode(reader)?;
        let (value, repeat) = match symbol {
            0..=15 => (symbol as u8, 1),
            16 => {
                let previous = index
                    .checked_sub(1)
                    .map(|i| lengths[i])
                    .ok_or_else(|| Error::corrupt("repeat with nothing to repeat"))?;
                (previous, 3 + reader.bits(2)? as usize)
            }
            17 => (0, 3 + reader.bits(3)? as usize),
            _ => (0, 11 + reader.bits(7)? as usize),
        };
        if index + repeat > lengths.len() {
            return Err(Error::corrupt("code lengths overrun the table"));
        }
        lengths[index..index + repeat].fill(value);
        index += repeat;
    }
    if lengths[END_OF_BLOCK] == 0 {
        return Err(Error::corrupt("no end-of-block code"));
    }
    Ok((
        Decoder::new(&lengths[..literals])?,
        Decoder::new(&lengths[literals..])?,
    ))
}

fn block(
    reader: &mut BitReader<'_>,
    out: &mut Vec<u8>,
    limit: usize,
    literal: &Decoder,
    distance: &Decoder,
) -> Result<()> {
    loop {
        let symbol = literal.decode(reader)?;
        match symbol {
            0..END_OF_BLOCK => {
                if out.len() >= limit {
                    return Err(Error::Limit(format!("more than {limit} bytes")));
                }
                out.push(symbol as u8);
            }
            END_OF_BLOCK => return Ok(()),
            _ => {
                let slot = symbol - 257;
                if slot >= LENGTH_BASE.len() {
                    return Err(Error::corrupt("invalid length code"));
                }
                let length = usize::from(LENGTH_BASE[slot])
                    + reader.bits(u32::from(LENGTH_EXTRA[slot]))? as usize;
                let code = distance.decode(reader)?;
                if code >= DISTANCE_BASE.len() {
                    return Err(Error::corrupt("invalid distance code"));
                }
                let back = usize::from(DISTANCE_BASE[code])
                    + reader.bits(u32::from(DISTANCE_EXTRA[code]))? as usize;
                if back > out.len() {
                    return Err(Error::corrupt("distance reaches before the start"));
                }
                if out.len() + length > limit {
                    return Err(Error::Limit(format!("more than {limit} bytes")));
                }
                let start = out.len() - back;
                for i in 0..length {
                    out.push(out[start + i]);
                }
            }
        }
    }
}

/// Decompresses a raw deflate stream (see [`inflate`]), refusing to produce more than `limit` bytes.
///
/// # Errors
///
/// As [`inflate`].
pub fn decompress(data: &[u8], limit: usize) -> Result<Vec<u8>> {
    inflate(data, limit).map(|(out, _)| out)
}

// ---------------------------------------------------------------- encoding

struct BitWriter {
    out: Vec<u8>,
    buffer: u64,
    count: u32,
}

impl BitWriter {
    const fn new() -> Self {
        Self {
            out: Vec::new(),
            buffer: 0,
            count: 0,
        }
    }

    fn put(&mut self, value: u32, bits: u32) {
        self.buffer |= u64::from(value) << self.count;
        self.count += bits;
        while self.count >= 8 {
            self.out.push(self.buffer as u8);
            self.buffer >>= 8;
            self.count -= 8;
        }
    }

    fn align(&mut self) {
        if self.count > 0 {
            self.out.push(self.buffer as u8);
            self.buffer = 0;
            self.count = 0;
        }
    }
}

/// The codes for a set of lengths, bit-reversed because deflate packs Huffman codes MSB first into an
/// LSB-first stream.
fn codes(lengths: &[u8]) -> Vec<u16> {
    let mut count = [0u16; 16];
    for &l in lengths {
        count[usize::from(l)] += 1;
    }
    count[0] = 0;
    let mut next = [0u16; 16];
    let mut code = 0u16;
    for bits in 1..16 {
        code = (code + count[bits - 1]) << 1;
        next[bits] = code;
    }
    lengths
        .iter()
        .map(|&l| {
            if l == 0 {
                return 0;
            }
            let c = next[usize::from(l)];
            next[usize::from(l)] += 1;
            c.reverse_bits() >> (16 - l)
        })
        .collect()
}

#[derive(Clone, Copy)]
enum Token {
    Literal(u8),
    Match { length: u16, distance: u16 },
}

/// The length symbol (257..=285) and its extra bits for a match length.
fn length_symbol(length: u16) -> (usize, u32, u32) {
    let slot = LENGTH_BASE.partition_point(|&b| b <= length) - 1;
    (
        257 + slot,
        u32::from(length - LENGTH_BASE[slot]),
        u32::from(LENGTH_EXTRA[slot]),
    )
}

fn distance_symbol(distance: u16) -> (usize, u32, u32) {
    let slot = DISTANCE_BASE.partition_point(|&b| b <= distance) - 1;
    (
        slot,
        u32::from(distance - DISTANCE_BASE[slot]),
        u32::from(DISTANCE_EXTRA[slot]),
    )
}

const HASH_BITS: u32 = 15;

fn hash(data: &[u8]) -> usize {
    let value = u32::from(data[0]) | u32::from(data[1]) << 8 | u32::from(data[2]) << 16;
    (value.wrapping_mul(0x9E37_79B1) >> (32 - HASH_BITS)) as usize
}

/// How far down a hash chain the matcher looks.
const CHAIN: usize = 128;
/// A match this long is taken without looking for a better one.
const GOOD_ENOUGH: usize = 128;

struct Matcher<'a> {
    data: &'a [u8],
    head: Vec<u32>,
    prev: Vec<u32>,
}

impl<'a> Matcher<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            head: vec![u32::MAX; 1 << HASH_BITS],
            prev: vec![u32::MAX; WINDOW],
        }
    }

    fn insert(&mut self, position: usize) {
        if position + MIN_MATCH <= self.data.len() {
            let h = hash(&self.data[position..]);
            self.prev[position % WINDOW] = self.head[h];
            self.head[h] = position as u32;
        }
    }

    /// The longest earlier match for `position`, as `(length, distance)`.
    fn find(&self, position: usize) -> Option<(usize, usize)> {
        if position + MIN_MATCH > self.data.len() {
            return None;
        }
        let limit = MAX_MATCH.min(self.data.len() - position);
        let mut candidate = self.head[hash(&self.data[position..])];
        let (mut best_length, mut best_distance) = (MIN_MATCH - 1, 0);
        for _ in 0..CHAIN {
            if candidate == u32::MAX {
                break;
            }
            let at = candidate as usize;
            let distance = position - at;
            if distance == 0 || distance > WINDOW {
                break;
            }
            if self.data[at + best_length.min(limit - 1)]
                == self.data[position + best_length.min(limit - 1)]
            {
                let mut length = 0;
                while length < limit && self.data[at + length] == self.data[position + length] {
                    length += 1;
                }
                if length > best_length {
                    best_length = length;
                    best_distance = distance;
                    if length >= GOOD_ENOUGH.min(limit) {
                        break;
                    }
                }
            }
            let next = self.prev[at % WINDOW];
            if next != u32::MAX && next as usize >= at {
                break;
            }
            candidate = next;
        }
        (best_distance > 0 && best_length >= MIN_MATCH).then_some((best_length, best_distance))
    }
}

/// Greedy-with-one-step-lookahead LZ77.
fn tokenize(data: &[u8]) -> Vec<Token> {
    let mut matcher = Matcher::new(data);
    let mut tokens = Vec::with_capacity(data.len() / 2 + 16);
    let mut position = 0;
    while position < data.len() {
        let found = matcher.find(position);
        matcher.insert(position);
        let Some((length, distance)) = found else {
            tokens.push(Token::Literal(data[position]));
            position += 1;
            continue;
        };
        // Lazy matching: a longer match one byte on beats this one.
        if length < GOOD_ENOUGH
            && let Some((next_length, _)) = matcher.find(position + 1)
            && next_length > length + 1
        {
            tokens.push(Token::Literal(data[position]));
            position += 1;
            continue;
        }
        tokens.push(Token::Match {
            length: length as u16,
            distance: distance as u16,
        });
        for skipped in position + 1..position + length {
            matcher.insert(skipped);
        }
        position += length;
    }
    tokens
}

/// Tokens per block; each block picks its own representation.
const BLOCK_TOKENS: usize = 1 << 15;

struct Frequencies {
    literal: [u32; 286],
    distance: [u32; 30],
}

fn count(tokens: &[Token]) -> Frequencies {
    let mut f = Frequencies {
        literal: [0; 286],
        distance: [0; 30],
    };
    for token in tokens {
        match *token {
            Token::Literal(b) => f.literal[usize::from(b)] += 1,
            Token::Match { length, distance } => {
                f.literal[length_symbol(length).0] += 1;
                f.distance[distance_symbol(distance).0] += 1;
            }
        }
    }
    f.literal[END_OF_BLOCK] = 1;
    f
}

fn payload_bits(f: &Frequencies, literal: &[u8], distance: &[u8]) -> u64 {
    let mut bits = 0u64;
    for (symbol, &n) in f.literal.iter().enumerate() {
        bits += u64::from(n) * u64::from(literal[symbol]);
        if symbol >= 257 {
            bits += u64::from(n) * u64::from(LENGTH_EXTRA[symbol - 257]);
        }
    }
    for (symbol, &n) in f.distance.iter().enumerate() {
        bits += u64::from(n) * (u64::from(distance[symbol]) + u64::from(DISTANCE_EXTRA[symbol]));
    }
    bits
}

/// The run-length encoding of code lengths: `(symbol, extra value, extra bits)`.
fn rle_lengths(lengths: &[u8]) -> Vec<(u8, u32, u32)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < lengths.len() {
        let value = lengths[i];
        let mut run = 1;
        while i + run < lengths.len() && lengths[i + run] == value {
            run += 1;
        }
        i += run;
        if value == 0 {
            while run >= 11 {
                let n = run.min(138);
                out.push((18, (n - 11) as u32, 7));
                run -= n;
            }
            if run >= 3 {
                out.push((17, (run - 3) as u32, 3));
                run = 0;
            }
        } else {
            out.push((value, 0, 0));
            run -= 1;
            while run >= 3 {
                let n = run.min(6);
                out.push((16, (n - 3) as u32, 2));
                run -= n;
            }
        }
        for _ in 0..run {
            out.push((value, 0, 0));
        }
    }
    out
}

struct Dynamic {
    literal: Vec<u8>,
    distance: Vec<u8>,
    sequence: Vec<(u8, u32, u32)>,
    code_length: Vec<u8>,
    literal_count: usize,
    distance_count: usize,
    code_length_count: usize,
}

impl Dynamic {
    fn new(f: &Frequencies) -> Self {
        let literal = code_lengths(&f.literal, 15);
        let mut distance_freq = f.distance.to_vec();
        if distance_freq.iter().all(|&n| n == 0) {
            distance_freq[0] = 1;
        }
        let distance = code_lengths(&distance_freq, 15);

        let literal_count = (257..=286)
            .rev()
            .find(|&n| literal[n - 1] != 0)
            .unwrap_or(257)
            .max(257);
        let distance_count = (1..=30).rev().find(|&n| distance[n - 1] != 0).unwrap_or(1);

        let mut all = literal[..literal_count].to_vec();
        all.extend_from_slice(&distance[..distance_count]);
        let sequence = rle_lengths(&all);

        let mut freq = [0u32; 19];
        for &(symbol, _, _) in &sequence {
            freq[usize::from(symbol)] += 1;
        }
        // The code-length code must be complete, which needs two symbols.
        if freq.iter().filter(|&&n| n > 0).count() == 1 {
            let spare = usize::from(freq[0] > 0);
            freq[spare] = 1;
        }
        let code_length = code_lengths(&freq, 7);
        let code_length_count = (4..=19)
            .rev()
            .find(|&n| code_length[CODE_LENGTH_ORDER[n - 1]] != 0)
            .unwrap_or(4);

        Self {
            literal,
            distance,
            sequence,
            code_length,
            literal_count,
            distance_count,
            code_length_count,
        }
    }

    fn header_bits(&self) -> u64 {
        let mut bits = 14 + 3 * self.code_length_count as u64;
        for &(symbol, _, extra) in &self.sequence {
            bits += u64::from(self.code_length[usize::from(symbol)]) + u64::from(extra);
        }
        bits
    }

    fn write_header(&self, w: &mut BitWriter) {
        w.put((self.literal_count - 257) as u32, 5);
        w.put((self.distance_count - 1) as u32, 5);
        w.put((self.code_length_count - 4) as u32, 4);
        for &slot in &CODE_LENGTH_ORDER[..self.code_length_count] {
            w.put(u32::from(self.code_length[slot]), 3);
        }
        let table = codes(&self.code_length);
        for &(symbol, value, extra) in &self.sequence {
            w.put(
                u32::from(table[usize::from(symbol)]),
                u32::from(self.code_length[usize::from(symbol)]),
            );
            w.put(value, extra);
        }
    }
}

fn write_tokens(w: &mut BitWriter, tokens: &[Token], literal: &[u8], distance: &[u8]) {
    let literal_codes = codes(literal);
    let distance_codes = codes(distance);
    for token in tokens {
        match *token {
            Token::Literal(b) => {
                let s = usize::from(b);
                w.put(u32::from(literal_codes[s]), u32::from(literal[s]));
            }
            Token::Match {
                length,
                distance: back,
            } => {
                let (symbol, extra, bits) = length_symbol(length);
                w.put(u32::from(literal_codes[symbol]), u32::from(literal[symbol]));
                w.put(extra, bits);
                let (symbol, extra, bits) = distance_symbol(back);
                w.put(
                    u32::from(distance_codes[symbol]),
                    u32::from(distance[symbol]),
                );
                w.put(extra, bits);
            }
        }
    }
    w.put(
        u32::from(literal_codes[END_OF_BLOCK]),
        u32::from(literal[END_OF_BLOCK]),
    );
}

fn stored(w: &mut BitWriter, bytes: &[u8], last: bool) {
    let mut chunks: Vec<&[u8]> = bytes.chunks(65_535).collect();
    if chunks.is_empty() {
        chunks.push(&[]);
    }
    let total = chunks.len();
    for (i, chunk) in chunks.into_iter().enumerate() {
        w.put(u32::from(last && i + 1 == total), 1);
        w.put(0, 2);
        w.align();
        let length = chunk.len() as u16;
        w.out.extend_from_slice(&length.to_le_bytes());
        w.out.extend_from_slice(&(!length).to_le_bytes());
        w.out.extend_from_slice(chunk);
    }
}

fn token_bytes(tokens: &[Token]) -> usize {
    tokens
        .iter()
        .map(|t| match *t {
            Token::Literal(_) => 1,
            Token::Match { length, .. } => usize::from(length),
        })
        .sum()
}

/// Compresses `data` to a raw deflate stream. Each block is stored, fixed-Huffman or dynamic-Huffman,
/// whichever is smallest.
#[must_use]
pub fn compress(data: &[u8]) -> Vec<u8> {
    let tokens = tokenize(data);
    let mut w = BitWriter::new();
    let (fixed_literal, fixed_distance) = fixed_lengths();

    let mut offset = 0;
    let blocks: Vec<&[Token]> = if tokens.is_empty() {
        vec![&[]]
    } else {
        tokens.chunks(BLOCK_TOKENS).collect()
    };
    let total = blocks.len();
    for (i, block) in blocks.into_iter().enumerate() {
        let last = i + 1 == total;
        let size = token_bytes(block);
        let raw = &data[offset..offset + size];
        offset += size;

        let f = count(block);
        let dynamic = Dynamic::new(&f);
        let dynamic_bits =
            3 + dynamic.header_bits() + payload_bits(&f, &dynamic.literal, &dynamic.distance);
        let fixed_bits = 3 + payload_bits(&f, &fixed_literal, &fixed_distance);
        let stored_bits =
            (raw.len().div_ceil(65_535).max(1) as u64) * 40 + raw.len() as u64 * 8 + 7;

        if stored_bits < dynamic_bits.min(fixed_bits) {
            stored(&mut w, raw, last);
        } else if fixed_bits <= dynamic_bits {
            w.put(u32::from(last), 1);
            w.put(1, 2);
            write_tokens(&mut w, block, &fixed_literal, &fixed_distance);
        } else {
            w.put(u32::from(last), 1);
            w.put(2, 2);
            dynamic.write_header(&mut w);
            write_tokens(&mut w, block, &dynamic.literal, &dynamic.distance);
        }
    }
    w.align();
    w.out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(data: &[u8]) -> usize {
        let packed = compress(data);
        let (out, used) = inflate(&packed, usize::MAX).unwrap();
        assert_eq!(out, data);
        assert_eq!(used, packed.len());
        packed.len()
    }

    #[quench_test::test]
    fn round_trips_assorted_inputs() {
        round_trip(b"");
        round_trip(b"a");
        round_trip(b"abcabcabcabcabcabcabcabc");
        round_trip(&vec![0u8; 100_000]);
        let text: Vec<u8> = (0..50_000u32)
            .flat_map(|i| format!("line {} of the text, {}\n", i % 97, i % 13).into_bytes())
            .collect();
        assert!(round_trip(&text) < text.len() / 4);
        // Incompressible bytes fall back to stored blocks.
        let mut x = 12_345u32;
        let noise: Vec<u8> = (0..200_000)
            .map(|_| {
                x = x.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                (x >> 16) as u8
            })
            .collect();
        assert!(round_trip(&noise) <= noise.len() + noise.len() / 1000 + 16);
    }

    #[quench_test::test]
    fn rejects_malformed_streams() {
        assert!(matches!(inflate(&[], 10), Err(Error::Truncated)));
        // Reserved block type.
        assert!(matches!(inflate(&[0b111], 10), Err(Error::Corrupt(_))));
        // Stored block whose length check fails.
        assert!(matches!(
            inflate(&[1, 1, 0, 0, 0, 0], 10),
            Err(Error::Corrupt(_))
        ));
        // A back-reference before the start of the output.
        let (literal, distance) = fixed_lengths();
        let (lc, dc) = (codes(&literal), codes(&distance));
        let mut w = BitWriter::new();
        w.put(1, 1);
        w.put(1, 2);
        let (symbol, extra, bits) = length_symbol(3);
        w.put(u32::from(lc[symbol]), u32::from(literal[symbol]));
        w.put(extra, bits);
        w.put(u32::from(dc[0]), 5);
        w.align();
        assert!(matches!(inflate(&w.out, 100), Err(Error::Corrupt(_))));
    }

    #[quench_test::test]
    fn output_is_limited() {
        let packed = compress(&vec![7u8; 10_000]);
        assert!(matches!(decompress(&packed, 9_999), Err(Error::Limit(_))));
        assert_eq!(decompress(&packed, 10_000).unwrap().len(), 10_000);
        let stored = compress(&{
            let mut x = 1u32;
            (0..1000)
                .map(|_| {
                    x = x.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                    (x >> 16) as u8
                })
                .collect::<Vec<u8>>()
        });
        assert!(matches!(decompress(&stored, 10), Err(Error::Limit(_))));
    }
}

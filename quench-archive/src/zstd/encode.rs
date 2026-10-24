//! A zstd encoder: LZ77 sequences coded with the predefined FSE tables, Huffman-coded literals.

use super::bits::BackwardWriter;
use super::decode::BLOCK_MAX;
use super::fse::Table;
use super::huffman::Encoder as HuffmanEncoder;
use super::tables::{
    LITERAL_LENGTH_ACCURACY, LITERAL_LENGTH_CODES, LITERAL_LENGTH_DEFAULT, MATCH_LENGTH_ACCURACY,
    MATCH_LENGTH_CODES, MATCH_LENGTH_DEFAULT, OFFSET_ACCURACY, OFFSET_DEFAULT, literal_length_code,
    match_length_code,
};
use crate::checksum::xxh64;
use std::sync::OnceLock;

const MAGIC: u32 = 0xFD2F_B528;
/// Content per frame. A single-segment frame's window is its whole content, and decoders cap windows
/// (zstd's default is 128 MiB), so larger inputs are split into several frames.
const FRAME_MAX: usize = 64 << 20;
/// How far back a match may reach.
const WINDOW: usize = 1 << 22;
const MIN_MATCH: usize = 4;
const HASH_BITS: u32 = 17;
/// How far down a hash chain the matcher looks.
const CHAIN: usize = 48;
/// A match this long is taken without looking for a better one.
const GOOD_ENOUGH: usize = 96;

/// Compresses `data` into zstd frames (one per 64 MiB), each with a content checksum.
#[must_use]
pub fn compress(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    if data.is_empty() {
        frame(data, &mut out);
    }
    for chunk in data.chunks(FRAME_MAX) {
        frame(chunk, &mut out);
    }
    out
}

fn frame(data: &[u8], out: &mut Vec<u8>) {
    out.extend_from_slice(&MAGIC.to_le_bytes());
    let size = data.len() as u64;
    // Single segment (no window descriptor) with the content size and a checksum.
    let (flag, width) = match size {
        0..=255 => (0u8, 1),
        256..=65_791 => (1, 2),
        65_792..=0xFFFF_FFFF => (2, 4),
        _ => (3, 8),
    };
    out.push(flag << 6 | 0x20 | 0x04);
    let stored = if flag == 1 { size - 256 } else { size };
    out.extend_from_slice(&stored.to_le_bytes()[..width]);

    let mut matcher = Matcher::new(data);
    let mut start = 0;
    loop {
        let end = (start + BLOCK_MAX).min(data.len());
        block(data, start, end, end == data.len(), &mut matcher, out);
        if end == data.len() {
            break;
        }
        start = end;
    }
    out.extend_from_slice(&(xxh64(data, 0) as u32).to_le_bytes());
}

fn block_header(out: &mut Vec<u8>, last: bool, kind: u32, size: usize) {
    let header = u32::from(last) | kind << 1 | (size as u32) << 3;
    out.extend_from_slice(&header.to_le_bytes()[..3]);
}

fn block(
    data: &[u8],
    start: usize,
    end: usize,
    last: bool,
    matcher: &mut Matcher<'_>,
    out: &mut Vec<u8>,
) {
    let bytes = &data[start..end];
    if bytes.len() > 1 && bytes.iter().all(|&b| b == bytes[0]) {
        block_header(out, last, 1, bytes.len());
        out.push(bytes[0]);
        return;
    }
    let (literals, sequences) = matcher.sequences(start, end);
    let mut body = Vec::new();
    literals_section(&literals, &mut body);
    sequences_section(&sequences, &mut body);
    if body.len() < bytes.len() {
        block_header(out, last, 2, body.len());
        out.extend_from_slice(&body);
    } else {
        block_header(out, last, 0, bytes.len());
        out.extend_from_slice(bytes);
    }
}

// ---------------------------------------------------------------- matching

#[derive(Debug, Clone, Copy)]
struct Sequence {
    literals: usize,
    length: usize,
    distance: usize,
}

struct Matcher<'a> {
    data: &'a [u8],
    head: Vec<u32>,
    prev: Vec<u32>,
}

const fn hash(bytes: &[u8]) -> usize {
    let value = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    (value.wrapping_mul(0x9E37_79B1) >> (32 - HASH_BITS)) as usize
}

impl<'a> Matcher<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            head: vec![u32::MAX; 1 << HASH_BITS],
            prev: vec![u32::MAX; WINDOW.min(data.len().max(1))],
        }
    }

    fn insert(&mut self, position: usize) {
        if position + MIN_MATCH <= self.data.len() {
            let slot = hash(&self.data[position..]);
            let ring = self.prev.len();
            self.prev[position % ring] = self.head[slot];
            self.head[slot] = position as u32;
        }
    }

    /// The longest earlier match for `position` that ends by `end`: `(length, distance)`.
    fn find(&self, position: usize, end: usize) -> Option<(usize, usize)> {
        if position + MIN_MATCH > end {
            return None;
        }
        let limit = end - position;
        let ring = self.prev.len();
        let mut candidate = self.head[hash(&self.data[position..])];
        let (mut best_length, mut best_distance) = (MIN_MATCH - 1, 0);
        for _ in 0..CHAIN {
            if candidate == u32::MAX {
                break;
            }
            let at = candidate as usize;
            if at >= position || position - at > WINDOW.min(ring) {
                break;
            }
            let probe = best_length.min(limit - 1);
            if self.data[at + probe] == self.data[position + probe] {
                let mut length = 0;
                while length < limit && self.data[at + length] == self.data[position + length] {
                    length += 1;
                }
                if length > best_length {
                    best_length = length;
                    best_distance = position - at;
                    if length >= GOOD_ENOUGH.min(limit) {
                        break;
                    }
                }
            }
            let next = self.prev[at % ring];
            if next != u32::MAX && next as usize >= at {
                break;
            }
            candidate = next;
        }
        (best_distance > 0).then_some((best_length, best_distance))
    }

    /// The literals and sequences covering `start..end`; bytes after the last match stay literals.
    fn sequences(&mut self, start: usize, end: usize) -> (Vec<u8>, Vec<Sequence>) {
        let mut literals = Vec::new();
        let mut sequences = Vec::new();
        let (mut position, mut literal_start) = (start, start);
        while position < end {
            let found = self.find(position, end);
            self.insert(position);
            let Some((length, distance)) = found else {
                position += 1;
                continue;
            };
            // Lazy matching: a clearly longer match one byte on beats this one.
            if length < GOOD_ENOUGH
                && let Some((next_length, _)) = self.find(position + 1, end)
                && next_length > length + 1
            {
                position += 1;
                continue;
            }
            literals.extend_from_slice(&self.data[literal_start..position]);
            sequences.push(Sequence {
                literals: position - literal_start,
                length,
                distance,
            });
            for skipped in position + 1..position + length {
                self.insert(skipped);
            }
            position += length;
            literal_start = position;
        }
        literals.extend_from_slice(&self.data[literal_start..end]);
        (literals, sequences)
    }
}

// ---------------------------------------------------------------- literals

/// The size header shared by raw and run-length literals.
fn plain_header(kind: u8, size: usize, out: &mut Vec<u8>) {
    if size < 32 {
        out.push(kind | (size as u8) << 3);
    } else if size < 4096 {
        out.extend_from_slice(&(u16::from(kind) | 1 << 2 | (size as u16) << 4).to_le_bytes());
    } else {
        let word = u32::from(kind) | 3 << 2 | (size as u32) << 4;
        out.extend_from_slice(&word.to_le_bytes()[..3]);
    }
}

fn literals_section(literals: &[u8], out: &mut Vec<u8>) {
    let size = literals.len();
    if size > 1 && literals.iter().all(|&b| b == literals[0]) {
        plain_header(1, size, out);
        out.push(literals[0]);
        return;
    }
    if let Some(compressed) = huffman_literals(literals)
        && compressed.len() < size + 3
    {
        out.extend_from_slice(&compressed);
        return;
    }
    plain_header(0, size, out);
    out.extend_from_slice(literals);
}

/// A complete compressed-literals section, if the literals can be Huffman coded.
fn huffman_literals(literals: &[u8]) -> Option<Vec<u8>> {
    let size = literals.len();
    if size < 48 {
        return None;
    }
    let mut frequencies = [0u32; 256];
    for &b in literals {
        frequencies[usize::from(b)] += 1;
    }
    let encoder = HuffmanEncoder::new(&frequencies)?;

    let mut body = encoder.table();
    let single = size <= 1023;
    if single {
        body.extend_from_slice(&encoder.stream(literals));
    } else {
        let segment = size.div_ceil(4);
        let streams: Vec<Vec<u8>> = literals
            .chunks(segment)
            .map(|part| encoder.stream(part))
            .collect();
        if streams.len() != 4 || streams[..3].iter().any(|s| s.len() > 0xFFFF) {
            return None;
        }
        for stream in &streams[..3] {
            body.extend_from_slice(&(stream.len() as u16).to_le_bytes());
        }
        for stream in &streams {
            body.extend_from_slice(stream);
        }
    }

    let compressed = body.len();
    let mut out = Vec::with_capacity(compressed + 5);
    // Type 2 (compressed), then the size format: 0 one stream, 2 and 3 four streams.
    if single && compressed <= 1023 {
        let word = 2u32 | (size as u32) << 4 | (compressed as u32) << 14;
        out.extend_from_slice(&word.to_le_bytes()[..3]);
    } else if !single && size < 1 << 14 && compressed < 1 << 14 {
        let word = 2u32 | 2 << 2 | (size as u32) << 4 | (compressed as u32) << 18;
        out.extend_from_slice(&word.to_le_bytes());
    } else if !single && size < 1 << 18 && compressed < 1 << 18 {
        let word = 2u64 | 3 << 2 | (size as u64) << 4 | (compressed as u64) << 22;
        out.extend_from_slice(&word.to_le_bytes()[..5]);
    } else {
        return None;
    }
    out.extend_from_slice(&body);
    Some(out)
}

// ---------------------------------------------------------------- sequences

/// A predefined table seen from the encoder: for each symbol, the cells that decode to it.
struct Encoding {
    accuracy: u8,
    /// Per symbol: `(baseline, bits, state)` sorted by baseline.
    cells: Vec<Vec<(u16, u8, u16)>>,
}

impl Encoding {
    fn new(counts: &[i16], accuracy: u8) -> Self {
        let table = Table::build(counts, accuracy).expect("default distributions are valid");
        let mut cells: Vec<Vec<(u16, u8, u16)>> = vec![Vec::new(); counts.len()];
        for (state, cell) in table.cells.iter().enumerate() {
            cells[usize::from(cell.symbol)].push((cell.baseline, cell.bits, state as u16));
        }
        for list in &mut cells {
            list.sort_unstable();
        }
        Self { accuracy, cells }
    }

    /// Any state that decodes to `symbol`: where the last sequence's coding starts.
    fn start(&self, symbol: usize) -> usize {
        usize::from(self.cells[symbol][0].2)
    }

    /// Encodes `symbol` ahead of `next`, the state the decoder will be in afterwards: returns the state
    /// the decoder must be in before it, and the bits it reads to get to `next`.
    fn step(&self, symbol: usize, next: usize) -> (usize, u32, u32) {
        let &(baseline, bits, state) = self.cells[symbol]
            .iter()
            .find(|&&(baseline, bits, _)| {
                (usize::from(baseline)..usize::from(baseline) + (1 << bits)).contains(&next)
            })
            .expect("the cells of a symbol cover every state");
        (
            usize::from(state),
            (next - usize::from(baseline)) as u32,
            u32::from(bits),
        )
    }
}

fn encodings() -> &'static [Encoding; 3] {
    static ENCODINGS: OnceLock<[Encoding; 3]> = OnceLock::new();
    ENCODINGS.get_or_init(|| {
        [
            Encoding::new(&LITERAL_LENGTH_DEFAULT, LITERAL_LENGTH_ACCURACY),
            Encoding::new(&OFFSET_DEFAULT, OFFSET_ACCURACY),
            Encoding::new(&MATCH_LENGTH_DEFAULT, MATCH_LENGTH_ACCURACY),
        ]
    })
}

/// A sequence as `(code, extra value, extra bits)` for each of its three fields.
struct Coded {
    ll: (usize, u32, u32),
    of: (usize, u32, u32),
    ml: (usize, u32, u32),
}

fn sequences_section(sequences: &[Sequence], out: &mut Vec<u8>) {
    let count = sequences.len();
    match count {
        0 => {
            out.push(0);
            return;
        }
        1..=127 => out.push(count as u8),
        128..=0x7EFF => out.extend_from_slice(&[(count >> 8) as u8 + 128, count as u8]),
        _ => {
            out.push(255);
            out.extend_from_slice(&((count - 0x7F00) as u16).to_le_bytes());
        }
    }
    out.push(0); // all three tables predefined

    // Each sequence as the three codes and their extra bits.
    let coded: Vec<Coded> = sequences
        .iter()
        .map(|s| {
            let ll = literal_length_code(s.literals as u32);
            let ml = match_length_code(s.length as u32);
            let value = s.distance as u32 + 3;
            let of = value.ilog2();
            Coded {
                ll: (
                    ll,
                    s.literals as u32 - LITERAL_LENGTH_CODES[ll].0,
                    u32::from(LITERAL_LENGTH_CODES[ll].1),
                ),
                of: (of as usize, value - (1 << of), of),
                ml: (
                    ml,
                    s.length as u32 - MATCH_LENGTH_CODES[ml].0,
                    u32::from(MATCH_LENGTH_CODES[ml].1),
                ),
            }
        })
        .collect();

    let [ll_enc, of_enc, ml_enc] = encodings();
    let mut writer = BackwardWriter::new();
    let last = &coded[count - 1];
    let mut ll_state = ll_enc.start(last.ll.0);
    let mut of_state = of_enc.start(last.of.0);
    let mut ml_state = ml_enc.start(last.ml.0);
    writer.add(last.ll.1, last.ll.2);
    writer.add(last.ml.1, last.ml.2);
    writer.add(last.of.1, last.of.2);

    for item in coded[..count - 1].iter().rev() {
        let (state, value, bits) = of_enc.step(item.of.0, of_state);
        of_state = state;
        writer.add(value, bits);
        let (state, value, bits) = ml_enc.step(item.ml.0, ml_state);
        ml_state = state;
        writer.add(value, bits);
        let (state, value, bits) = ll_enc.step(item.ll.0, ll_state);
        ll_state = state;
        writer.add(value, bits);

        writer.add(item.ll.1, item.ll.2);
        writer.add(item.ml.1, item.ml.2);
        writer.add(item.of.1, item.of.2);
    }
    writer.add(ml_state as u32, u32::from(ml_enc.accuracy));
    writer.add(of_state as u32, u32::from(of_enc.accuracy));
    writer.add(ll_state as u32, u32::from(ll_enc.accuracy));
    out.extend_from_slice(&writer.finish());
}

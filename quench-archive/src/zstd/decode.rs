//! Decoding one compressed block: the literals section, then the sequences that interleave them with
//! copies from earlier output.

use super::bits::BackwardReader;
use super::fse::{self, Table};
use super::huffman::{self, DecodeTable};
use super::tables::{
    LITERAL_LENGTH_ACCURACY, LITERAL_LENGTH_CODES, LITERAL_LENGTH_DEFAULT,
    LITERAL_LENGTH_MAX_ACCURACY, MATCH_LENGTH_ACCURACY, MATCH_LENGTH_CODES, MATCH_LENGTH_DEFAULT,
    MATCH_LENGTH_MAX_ACCURACY, OFFSET_ACCURACY, OFFSET_DEFAULT, OFFSET_MAX_ACCURACY,
};
use crate::error::{Error, Result};

/// The largest block, and so the most one block may regenerate.
pub const BLOCK_MAX: usize = 128 * 1024;

/// What a frame carries from one block to the next.
#[derive(Debug, Clone)]
pub struct State {
    huffman: Option<DecodeTable>,
    literal_lengths: Option<Table>,
    offsets: Option<Table>,
    match_lengths: Option<Table>,
    /// The three most recent offsets.
    repeat: [usize; 3],
}

impl State {
    pub const fn new() -> Self {
        Self {
            huffman: None,
            literal_lengths: None,
            offsets: None,
            match_lengths: None,
            repeat: [1, 4, 8],
        }
    }
}

const fn low_bits(value: u64, bits: u32) -> u64 {
    value & ((1u64 << bits) - 1)
}

/// Decodes a compressed block, appending to `out`. `frame_start` is where this frame's output begins
/// (matches cannot reach before it) and `limit` the most `out` may hold.
pub fn block(
    data: &[u8],
    state: &mut State,
    out: &mut Vec<u8>,
    frame_start: usize,
    limit: usize,
) -> Result<()> {
    let (literals, used) = literals(data, state)?;
    sequences(&data[used..], &literals, state, out, frame_start, limit)
}

fn literals(data: &[u8], state: &mut State) -> Result<(Vec<u8>, usize)> {
    let first = *data.first().ok_or(Error::Truncated)?;
    let kind = first & 3;
    let format = (first >> 2) & 3;

    let word = |bytes: usize| -> Result<u64> {
        let slice = data.get(..bytes).ok_or(Error::Truncated)?;
        Ok(slice
            .iter()
            .enumerate()
            .fold(0u64, |acc, (i, &b)| acc | u64::from(b) << (8 * i)))
    };

    if kind < 2 {
        let (size, header) = match format {
            0 | 2 => (u64::from(first >> 3), 1),
            1 => (low_bits(word(2)? >> 4, 12), 2),
            _ => (low_bits(word(3)? >> 4, 20), 3),
        };
        let size = size as usize;
        if size > BLOCK_MAX {
            return Err(Error::corrupt("literals larger than a block"));
        }
        return if kind == 0 {
            let body = data.get(header..header + size).ok_or(Error::Truncated)?;
            Ok((body.to_vec(), header + size))
        } else {
            let byte = *data.get(header).ok_or(Error::Truncated)?;
            Ok((vec![byte; size], header + 1))
        };
    }

    let (header, size_bits, streams) = match format {
        0 => (3, 10, 1),
        1 => (3, 10, 4),
        2 => (4, 14, 4),
        _ => (5, 18, 4),
    };
    let value = word(header)?;
    let regenerated = low_bits(value >> 4, size_bits) as usize;
    let compressed = low_bits(value >> (4 + size_bits), size_bits) as usize;
    if regenerated > BLOCK_MAX {
        return Err(Error::corrupt("literals larger than a block"));
    }
    let body = data
        .get(header..header + compressed)
        .ok_or(Error::Truncated)?;

    let body = if kind == 2 {
        let (table, used) = huffman::read_table(body)?;
        state.huffman = Some(table);
        &body[used..]
    } else {
        body
    };
    let table = state
        .huffman
        .as_ref()
        .ok_or_else(|| Error::corrupt("literals reuse a Huffman table that was never sent"))?;

    let mut out = Vec::with_capacity(regenerated);
    if streams == 1 {
        table.decode(body, regenerated, &mut out)?;
    } else {
        let jump = body.get(..6).ok_or(Error::Truncated)?;
        let sizes = [
            usize::from(u16::from_le_bytes([jump[0], jump[1]])),
            usize::from(u16::from_le_bytes([jump[2], jump[3]])),
            usize::from(u16::from_le_bytes([jump[4], jump[5]])),
        ];
        let segment = regenerated.div_ceil(4);
        if segment * 3 > regenerated {
            return Err(Error::corrupt("literals too short for four streams"));
        }
        let mut at = 6;
        for (i, size) in sizes
            .iter()
            .copied()
            .chain([body.len().saturating_sub(6 + sizes.iter().sum::<usize>())])
            .enumerate()
        {
            let stream = body.get(at..at + size).ok_or(Error::Truncated)?;
            at += size;
            let count = if i < 3 {
                segment
            } else {
                regenerated - 3 * segment
            };
            table.decode(stream, count, &mut out)?;
        }
    }
    Ok((out, header + compressed))
}

/// The compression mode of one sequence table.
fn table(
    mode: u8,
    data: &[u8],
    at: &mut usize,
    previous: Option<&Table>,
    default: (&[i16], u8),
    max_symbol: usize,
    max_accuracy: u8,
) -> Result<Table> {
    match mode {
        0 => Table::build(default.0, default.1),
        1 => {
            let symbol = *data.get(*at).ok_or(Error::Truncated)?;
            *at += 1;
            if usize::from(symbol) > max_symbol {
                return Err(Error::corrupt("sequence code out of range"));
            }
            Ok(Table::single(symbol))
        }
        2 => {
            let (counts, accuracy, used) = fse::read_distribution(
                data.get(*at..).ok_or(Error::Truncated)?,
                max_symbol,
                max_accuracy,
            )?;
            *at += used;
            Table::build(&counts, accuracy)
        }
        _ => previous
            .cloned()
            .ok_or_else(|| Error::corrupt("sequences reuse a table that was never sent")),
    }
}

#[allow(clippy::too_many_lines)]
fn sequences(
    data: &[u8],
    literals: &[u8],
    state: &mut State,
    out: &mut Vec<u8>,
    frame_start: usize,
    limit: usize,
) -> Result<()> {
    let block_start = out.len();
    let first = *data.first().ok_or(Error::Truncated)?;
    let (count, mut at) = match first {
        0 => {
            if out.len() + literals.len() > limit {
                return Err(Error::Limit(format!("more than {limit} bytes")));
            }
            out.extend_from_slice(literals);
            return Ok(());
        }
        1..=127 => (usize::from(first), 1),
        128..=254 => {
            let second = *data.get(1).ok_or(Error::Truncated)?;
            (((usize::from(first) - 128) << 8) + usize::from(second), 2)
        }
        255 => {
            let rest = data.get(1..3).ok_or(Error::Truncated)?;
            (
                usize::from(u16::from_le_bytes([rest[0], rest[1]])) + 0x7F00,
                3,
            )
        }
    };

    let modes = *data.get(at).ok_or(Error::Truncated)?;
    at += 1;
    if modes & 3 != 0 {
        return Err(Error::corrupt("reserved sequence mode bits"));
    }
    let literal_lengths = table(
        modes >> 6,
        data,
        &mut at,
        state.literal_lengths.as_ref(),
        (&LITERAL_LENGTH_DEFAULT, LITERAL_LENGTH_ACCURACY),
        LITERAL_LENGTH_CODES.len() - 1,
        LITERAL_LENGTH_MAX_ACCURACY,
    )?;
    let offsets = table(
        (modes >> 4) & 3,
        data,
        &mut at,
        state.offsets.as_ref(),
        (&OFFSET_DEFAULT, OFFSET_ACCURACY),
        31,
        OFFSET_MAX_ACCURACY,
    )?;
    let match_lengths = table(
        (modes >> 2) & 3,
        data,
        &mut at,
        state.match_lengths.as_ref(),
        (&MATCH_LENGTH_DEFAULT, MATCH_LENGTH_ACCURACY),
        MATCH_LENGTH_CODES.len() - 1,
        MATCH_LENGTH_MAX_ACCURACY,
    )?;

    let mut reader = BackwardReader::new(data.get(at..).ok_or(Error::Truncated)?)?;
    let mut ll_state = reader.read(u32::from(literal_lengths.accuracy))? as usize;
    let mut of_state = reader.read(u32::from(offsets.accuracy))? as usize;
    let mut ml_state = reader.read(u32::from(match_lengths.accuracy))? as usize;

    let mut literal_at = 0usize;
    for index in 0..count {
        let ll_cell = literal_lengths.cells[ll_state];
        let of_cell = offsets.cells[of_state];
        let ml_cell = match_lengths.cells[ml_state];

        let of_code = u32::from(of_cell.symbol);
        let offset_value = (1usize << of_code) + reader.read(of_code)? as usize;
        let (ml_base, ml_bits) = MATCH_LENGTH_CODES[usize::from(ml_cell.symbol)];
        let match_length = ml_base as usize + reader.read(u32::from(ml_bits))? as usize;
        let (ll_base, ll_bits) = LITERAL_LENGTH_CODES[usize::from(ll_cell.symbol)];
        let literal_length = ll_base as usize + reader.read(u32::from(ll_bits))? as usize;

        let offset = resolve_offset(&mut state.repeat, offset_value, literal_length)?;

        let literal_end = literal_at
            .checked_add(literal_length)
            .filter(|&end| end <= literals.len())
            .ok_or_else(|| Error::corrupt("sequence reads past the literals"))?;
        let produced = out.len() - block_start + literal_length + match_length;
        if produced > BLOCK_MAX || out.len() + literal_length + match_length > limit {
            return Err(if produced > BLOCK_MAX {
                Error::corrupt("block regenerates more than 128 KiB")
            } else {
                Error::Limit(format!("more than {limit} bytes"))
            });
        }
        out.extend_from_slice(&literals[literal_at..literal_end]);
        literal_at = literal_end;

        if offset > out.len() - frame_start {
            return Err(Error::corrupt(
                "match reaches before the start of the frame",
            ));
        }
        let from = out.len() - offset;
        if offset >= match_length {
            out.extend_from_within(from..from + match_length);
        } else {
            for i in 0..match_length {
                out.push(out[from + i]);
            }
        }

        if index + 1 < count {
            ll_state =
                usize::from(ll_cell.baseline) + reader.read(u32::from(ll_cell.bits))? as usize;
            ml_state =
                usize::from(ml_cell.baseline) + reader.read(u32::from(ml_cell.bits))? as usize;
            of_state =
                usize::from(of_cell.baseline) + reader.read(u32::from(of_cell.bits))? as usize;
        }
    }
    if reader.remaining() != 0 {
        return Err(Error::corrupt("sequence stream has unused bits"));
    }

    let rest = &literals[literal_at..];
    if out.len() - block_start + rest.len() > BLOCK_MAX || out.len() + rest.len() > limit {
        return Err(Error::Limit(format!("more than {limit} bytes")));
    }
    out.extend_from_slice(rest);

    state.literal_lengths = Some(literal_lengths);
    state.offsets = Some(offsets);
    state.match_lengths = Some(match_lengths);
    Ok(())
}

/// Turns an offset value into a distance, keeping the history of recent offsets.
fn resolve_offset(repeat: &mut [usize; 3], value: usize, literal_length: usize) -> Result<usize> {
    if value > 3 {
        let offset = value - 3;
        *repeat = [offset, repeat[0], repeat[1]];
        return Ok(offset);
    }
    // With no literals before it, repeat code 1 means the second most recent offset, and so on.
    let slot = value - 1 + usize::from(literal_length == 0);
    let offset = match slot {
        0 => return Ok(repeat[0]),
        1 | 2 => repeat[slot],
        _ => repeat[0]
            .checked_sub(1)
            .filter(|&o| o > 0)
            .ok_or_else(|| Error::corrupt("repeat offset of zero"))?,
    };
    match slot {
        1 => *repeat = [offset, repeat[0], repeat[2]],
        _ => *repeat = [offset, repeat[0], repeat[1]],
    }
    Ok(offset)
}

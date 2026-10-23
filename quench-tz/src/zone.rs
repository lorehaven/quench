//! A zone's rules: local time types, the transitions between them, and the rule that continues after the
//! last transition.

use std::fmt;

use crate::posix::Rule;

/// One way a clock can read: an offset from UTC, whether it is daylight saving, and its abbreviation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalType {
    pub utc_offset: i32,
    pub is_dst: bool,
    pub abbreviation: String,
}

/// The rules of one zone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZoneData {
    pub types: Vec<LocalType>,
    /// Index into `types` for times before the first transition.
    pub initial: usize,
    /// (instant of the change in seconds since the epoch, index into `types`), ascending.
    pub transitions: Vec<(i64, usize)>,
    /// What applies after the last transition (and the whole story when there are none).
    pub rule: Option<Rule>,
    /// The text of `rule`, kept for packing.
    pub rule_text: String,
}

/// A `TZif` file that could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TzifError(pub &'static str);

impl fmt::Display for TzifError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid TZif data: {}", self.0)
    }
}

impl std::error::Error for TzifError {}

/// What a zone reads at one instant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reading<'a> {
    pub utc_offset: i32,
    pub is_dst: bool,
    pub abbreviation: &'a str,
}

impl ZoneData {
    /// A zone that never changes.
    #[must_use]
    pub fn fixed(utc_offset: i32, abbreviation: &str) -> Self {
        Self {
            types: vec![LocalType {
                utc_offset,
                is_dst: false,
                abbreviation: abbreviation.to_string(),
            }],
            initial: 0,
            transitions: Vec::new(),
            rule: None,
            rule_text: String::new(),
        }
    }

    /// What the zone reads at `t`, seconds since the epoch.
    #[must_use]
    pub fn reading_at(&self, t: i64) -> Reading<'_> {
        // The index of the first transition after `t`.
        let after = self.transitions.partition_point(|&(at, _)| at <= t);
        if after == self.transitions.len()
            && let Some(rule) = &self.rule
        {
            let r = rule.reading_at(t);
            return Reading {
                utc_offset: r.utc_offset,
                is_dst: r.is_dst,
                abbreviation: r.abbreviation,
            };
        }
        let kind = if after == 0 {
            self.initial
        } else {
            self.transitions[after - 1].1
        };
        self.type_reading(kind)
    }

    fn type_reading(&self, kind: usize) -> Reading<'_> {
        let t = self.types.get(kind).or_else(|| self.types.first());
        t.map_or(
            Reading {
                utc_offset: 0,
                is_dst: false,
                abbreviation: "UTC",
            },
            |t| Reading {
                utc_offset: t.utc_offset,
                is_dst: t.is_dst,
                abbreviation: &t.abbreviation,
            },
        )
    }

    /// Reads a `TZif` file (RFC 8536), using its 64-bit data and POSIX rule when it has them.
    ///
    /// # Errors
    ///
    /// [`TzifError`] naming what is wrong with the bytes.
    pub fn from_tzif(bytes: &[u8]) -> Result<Self, TzifError> {
        let first = Header::read(bytes, 0)?;
        let (header, body, wide) = if first.version >= b'2' {
            let skip = 44 + first.data_len(4);
            let second = Header::read(bytes, skip)?;
            (second, skip + 44, true)
        } else {
            (first, 44, false)
        };
        let time_size = if wide { 8 } else { 4 };
        let mut at = body;
        let mut take = |len: usize| -> Result<&[u8], TzifError> {
            let slice = bytes.get(at..at + len).ok_or(TzifError("truncated data"))?;
            at += len;
            Ok(slice)
        };
        let times = take(header.timecnt * time_size)?;
        let kinds = take(header.timecnt)?;
        let types_raw = take(header.typecnt * 6)?;
        let chars = take(header.charcnt)?;
        let _leaps = take(header.leapcnt * (time_size + 4))?;
        let _std = take(header.isstdcnt)?;
        let _ut = take(header.isutcnt)?;

        let mut types = Vec::with_capacity(header.typecnt);
        for entry in types_raw.as_chunks::<6>().0 {
            let utc_offset = i32::from_be_bytes([entry[0], entry[1], entry[2], entry[3]]);
            let start = usize::from(entry[5]);
            let name = chars
                .get(start..)
                .ok_or(TzifError("abbreviation out of range"))?;
            let end = name.iter().position(|&b| b == 0).unwrap_or(name.len());
            types.push(LocalType {
                utc_offset,
                is_dst: entry[4] != 0,
                abbreviation: String::from_utf8_lossy(&name[..end]).into_owned(),
            });
        }
        if types.is_empty() {
            return Err(TzifError("no local time types"));
        }
        let mut transitions = Vec::with_capacity(header.timecnt);
        for (index, chunk) in times.chunks_exact(time_size).enumerate() {
            let at = if wide {
                i64::from_be_bytes(chunk.try_into().map_err(|_| TzifError("bad time"))?)
            } else {
                i64::from(i32::from_be_bytes(
                    chunk.try_into().map_err(|_| TzifError("bad time"))?,
                ))
            };
            let kind = usize::from(kinds[index]);
            if kind >= types.len() {
                return Err(TzifError("transition to an unknown type"));
            }
            transitions.push((at, kind));
        }
        if transitions.windows(2).any(|w| w[0].0 >= w[1].0) {
            return Err(TzifError("transitions out of order"));
        }

        let rule_text = if wide {
            let rest = &bytes[at..];
            rest.strip_prefix(b"\n")
                .and_then(|r| r.iter().position(|&b| b == b'\n').map(|end| &r[..end]))
                .map(|r| String::from_utf8_lossy(r).into_owned())
                .unwrap_or_default()
        } else {
            String::new()
        };
        let rule = if rule_text.is_empty() {
            None
        } else {
            Some(Rule::parse(&rule_text).ok_or(TzifError("unreadable POSIX rule"))?)
        };
        // Before the first transition: the first standard-time type, as the reference readers do.
        let initial = types.iter().position(|t| !t.is_dst).unwrap_or(0);
        Ok(Self {
            types,
            initial,
            transitions,
            rule,
            rule_text,
        })
    }
}

struct Header {
    version: u8,
    isutcnt: usize,
    isstdcnt: usize,
    leapcnt: usize,
    timecnt: usize,
    typecnt: usize,
    charcnt: usize,
}

impl Header {
    fn read(bytes: &[u8], at: usize) -> Result<Self, TzifError> {
        let h = bytes
            .get(at..at + 44)
            .ok_or(TzifError("truncated header"))?;
        if &h[..4] != b"TZif" {
            return Err(TzifError("missing TZif magic"));
        }
        let count = |i: usize| {
            usize::try_from(u32::from_be_bytes([
                h[20 + 4 * i],
                h[21 + 4 * i],
                h[22 + 4 * i],
                h[23 + 4 * i],
            ]))
            .unwrap_or(usize::MAX)
        };
        Ok(Self {
            version: h[4],
            isutcnt: count(0),
            isstdcnt: count(1),
            leapcnt: count(2),
            timecnt: count(3),
            typecnt: count(4),
            charcnt: count(5),
        })
    }

    /// Bytes of the data block that follows the header, with times `time_size` bytes wide.
    const fn data_len(&self, time_size: usize) -> usize {
        self.timecnt * time_size
            + self.timecnt
            + self.typecnt * 6
            + self.charcnt
            + self.leapcnt * (time_size + 4)
            + self.isstdcnt
            + self.isutcnt
    }
}

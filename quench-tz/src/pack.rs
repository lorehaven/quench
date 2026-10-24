//! The compact form the zone database is stored in: every zone's rules, with identical zones (links) stored
//! once. `examples/pack.rs` writes it from a system `zoneinfo` directory; [`Database`] reads it.
//!
//! Layout: `QTZ1`, the database version, then a name table (name, index of the zone it uses), then the zones
//! (types, initial type, POSIX rule, transitions as zig-zag varint deltas). All integers are varints unless
//! noted, so the file stays a few hundred kilobytes.

use std::collections::HashMap;

use crate::posix::Rule;
use crate::zone::{LocalType, ZoneData};

const MAGIC: &[u8; 4] = b"QTZ1";

fn put(out: &mut Vec<u8>, mut value: u64) {
    loop {
        let byte = (value & 0x7f) as u8;
        value >>= 7;
        if value == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

const fn zigzag(value: i64) -> u64 {
    ((value << 1) ^ (value >> 63)).cast_unsigned()
}

const fn unzigzag(value: u64) -> i64 {
    (value >> 1).cast_signed() ^ -((value & 1).cast_signed())
}

fn put_str(out: &mut Vec<u8>, text: &str) {
    put(out, text.len() as u64);
    out.extend_from_slice(text.as_bytes());
}

/// Packs `zones` (name, rules) and the database `version` text.
#[must_use]
pub fn pack(version: &str, zones: &[(String, ZoneData)]) -> Vec<u8> {
    let mut index_of: HashMap<Vec<u8>, usize> = HashMap::new();
    let mut names: Vec<(&str, usize)> = Vec::new();
    let mut bodies: Vec<Vec<u8>> = Vec::new();
    for (name, data) in zones {
        let mut body = Vec::new();
        encode_zone(&mut body, data);
        let index = *index_of.entry(body.clone()).or_insert_with(|| {
            bodies.push(body);
            bodies.len() - 1
        });
        names.push((name, index));
    }
    names.sort_unstable();

    let mut out = Vec::new();
    out.extend_from_slice(MAGIC);
    put_str(&mut out, version);
    put(&mut out, names.len() as u64);
    for (name, index) in &names {
        put_str(&mut out, name);
        put(&mut out, *index as u64);
    }
    put(&mut out, bodies.len() as u64);
    // Zone lengths first, so a reader can jump to one without decoding the others.
    for body in &bodies {
        put(&mut out, body.len() as u64);
    }
    for body in &bodies {
        out.extend_from_slice(body);
    }
    out
}

fn encode_zone(out: &mut Vec<u8>, data: &ZoneData) {
    put_str(out, &data.rule_text);
    put(out, data.types.len() as u64);
    for t in &data.types {
        put(out, zigzag(i64::from(t.utc_offset)));
        out.push(u8::from(t.is_dst));
        put_str(out, &t.abbreviation);
    }
    put(out, data.initial as u64);
    put(out, data.transitions.len() as u64);
    let mut previous = 0i64;
    for &(at, kind) in &data.transitions {
        put(out, zigzag(at - previous));
        put(out, kind as u64);
        previous = at;
    }
}

/// A reader over packed bytes.
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn varint(&mut self) -> Option<u64> {
        let mut value = 0u64;
        let mut shift = 0;
        loop {
            let byte = *self.bytes.get(self.at)?;
            self.at += 1;
            value |= u64::from(byte & 0x7f).checked_shl(shift)?;
            if byte & 0x80 == 0 {
                return Some(value);
            }
            shift += 7;
            if shift > 63 {
                return None;
            }
        }
    }

    fn usize(&mut self) -> Option<usize> {
        usize::try_from(self.varint()?).ok()
    }

    fn text(&mut self) -> Option<&'a str> {
        let len = self.usize()?;
        let slice = self.bytes.get(self.at..self.at.checked_add(len)?)?;
        self.at += len;
        std::str::from_utf8(slice).ok()
    }

    fn byte(&mut self) -> Option<u8> {
        let b = *self.bytes.get(self.at)?;
        self.at += 1;
        Some(b)
    }
}

/// The packed zone database, indexed by name.
#[derive(Debug)]
pub struct Database {
    version: &'static str,
    /// (name, index of the zone it uses), sorted by name.
    names: Vec<(&'static str, usize)>,
    /// (start, end) of each zone's bytes.
    spans: Vec<(usize, usize)>,
    bytes: &'static [u8],
}

impl Database {
    /// Indexes `bytes`; `None` when they are not a packed database.
    #[must_use]
    pub fn open(bytes: &'static [u8]) -> Option<Self> {
        if bytes.get(..4)? != MAGIC {
            return None;
        }
        let mut r = Reader { bytes, at: 4 };
        let version = r.text()?;
        let count = r.usize()?;
        let mut names = Vec::with_capacity(count);
        for _ in 0..count {
            let name = r.text()?;
            names.push((name, r.usize()?));
        }
        let zones = r.usize()?;
        let mut lengths = Vec::with_capacity(zones);
        for _ in 0..zones {
            lengths.push(r.usize()?);
        }
        let mut start = r.at;
        let mut spans = Vec::with_capacity(zones);
        for len in lengths {
            let end = start.checked_add(len)?;
            if end > bytes.len() {
                return None;
            }
            spans.push((start, end));
            start = end;
        }
        Some(Self {
            version,
            names,
            spans,
            bytes,
        })
    }

    #[must_use]
    pub const fn version(&self) -> &'static str {
        self.version
    }

    /// Every zone name, sorted.
    pub fn names(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.names.iter().map(|(name, _)| *name)
    }

    /// The database's own spelling of `name`, if it has such a zone.
    #[must_use]
    pub fn canonical(&self, name: &str) -> Option<&'static str> {
        let at = self.names.binary_search_by(|(n, _)| (*n).cmp(name)).ok()?;
        Some(self.names[at].0)
    }

    /// The rules of `name` (exact spelling).
    #[must_use]
    pub fn zone(&self, name: &str) -> Option<ZoneData> {
        let at = self.names.binary_search_by(|(n, _)| (*n).cmp(name)).ok()?;
        let (start, end) = *self.spans.get(self.names[at].1)?;
        decode_zone(&self.bytes[start..end])
    }
}

fn decode_zone(bytes: &[u8]) -> Option<ZoneData> {
    let mut r = Reader { bytes, at: 0 };
    let rule_text = r.text()?.to_string();
    let type_count = r.usize()?;
    let mut types = Vec::with_capacity(type_count);
    for _ in 0..type_count {
        let utc_offset = i32::try_from(unzigzag(r.varint()?)).ok()?;
        let is_dst = r.byte()? != 0;
        types.push(LocalType {
            utc_offset,
            is_dst,
            abbreviation: r.text()?.to_string(),
        });
    }
    let initial = r.usize()?;
    let count = r.usize()?;
    let mut transitions = Vec::with_capacity(count);
    let mut at = 0i64;
    for _ in 0..count {
        at += unzigzag(r.varint()?);
        transitions.push((at, r.usize()?));
    }
    let rule = if rule_text.is_empty() {
        None
    } else {
        Some(Rule::parse(&rule_text)?)
    };
    Some(ZoneData {
        types,
        initial,
        transitions,
        rule,
        rule_text,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<(String, ZoneData)> {
        let fixed = ZoneData::fixed(3600, "XYZ");
        let mut moving = ZoneData::fixed(0, "GMT");
        moving.types.push(LocalType {
            utc_offset: 3600,
            is_dst: true,
            abbreviation: "BST".into(),
        });
        moving.transitions = vec![(-1_000_000_000, 1), (-999_000_000, 0), (1_700_000_000, 1)];
        moving.rule_text = "GMT0BST,M3.5.0/1,M10.5.0".into();
        moving.rule = Rule::parse(&moving.rule_text);
        vec![
            ("Fixed/A".into(), fixed.clone()),
            ("Fixed/B".into(), fixed),
            ("Moving".into(), moving),
        ]
    }

    #[quench_test::test]
    fn packing_round_trips_and_shares_identical_zones() {
        let zones = sample();
        let bytes: &'static [u8] = Box::leak(pack("2026x", &zones).into_boxed_slice());
        let db = Database::open(bytes).expect("a packed database");
        assert_eq!(db.version(), "2026x");
        assert_eq!(
            db.names().collect::<Vec<_>>(),
            ["Fixed/A", "Fixed/B", "Moving"]
        );
        for (name, data) in &zones {
            assert_eq!(&db.zone(name).expect("present"), data, "{name}");
        }
        assert!(db.zone("Nowhere").is_none());
        assert_eq!(db.spans.len(), 2, "the two identical zones are stored once");
        assert!(format!("{db:?}").contains("Database"));
    }

    #[quench_test::test]
    fn damaged_input_is_refused_not_trusted() {
        assert!(Database::open(b"").is_none());
        assert!(Database::open(b"NOPE").is_none());
        let bytes = pack("v", &sample());
        for cut in [4, 6, 12, bytes.len() / 2, bytes.len() - 1] {
            let truncated: &'static [u8] = Box::leak(bytes[..cut].to_vec().into_boxed_slice());
            if let Some(db) = Database::open(truncated) {
                // The index may survive a cut; reading a zone must then fail cleanly.
                for name in db.names() {
                    let _ = db.zone(name);
                }
            }
        }
        let mut corrupt = bytes;
        let last = corrupt.len() - 1;
        corrupt[last] = 0xff;
        let corrupt: &'static [u8] = Box::leak(corrupt.into_boxed_slice());
        if let Some(db) = Database::open(corrupt) {
            let _ = db.zone("Moving");
        }
    }

    #[quench_test::test]
    fn varints_and_zigzag() {
        let mut out = Vec::new();
        for v in [0u64, 1, 127, 128, 300, u64::from(u32::MAX), u64::MAX >> 1] {
            put(&mut out, v);
        }
        let mut r = Reader { bytes: &out, at: 0 };
        for v in [0u64, 1, 127, 128, 300, u64::from(u32::MAX), u64::MAX >> 1] {
            assert_eq!(r.varint(), Some(v));
        }
        assert_eq!(r.varint(), None);
        for v in [
            0i64,
            1,
            -1,
            1_000_000_000_000,
            -1_000_000_000_000,
            i64::MAX / 2,
            i64::MIN / 2,
        ] {
            assert_eq!(unzigzag(zigzag(v)), v);
        }
        let mut overlong = Reader {
            bytes: &[0x80; 12],
            at: 0,
        };
        assert_eq!(overlong.varint(), None);
    }
}

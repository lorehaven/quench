//! Pair kerning from `GPOS`: the `kern` feature's pair-adjustment lookups (types 2, and 9 wrapping them).
//!
//! The font's script and language lists are not consulted: a kerning pair names two glyph ids, which belong to
//! one script anyway, so every lookup the `kern` feature uses is applied to a pair in the order the font lists
//! them, the first matching subtable of each, and their adjustments are added.

use std::collections::BTreeSet;

use crate::font::{i16_at, u16_at, u32_at};

/// The most lookups and subtables read, whatever the file says.
const MAX_LOOKUPS: usize = 64;
const MAX_SUBTABLES: usize = 256;

/// The kerning lookups of a font: where each one's pair-adjustment subtables are.
#[derive(Debug, Default)]
pub struct Kerning {
    lookups: Vec<Vec<usize>>,
}

impl Kerning {
    /// Finds the `kern` feature's lookups in the `GPOS` table at `gpos`.
    #[must_use]
    pub fn parse(data: &[u8], gpos: usize, length: usize) -> Self {
        Self::try_parse(data, gpos, length).unwrap_or_default()
    }

    fn try_parse(data: &[u8], gpos: usize, length: usize) -> Option<Self> {
        let data = data.get(..gpos.checked_add(length)?)?;
        let features = gpos + usize::from(u16_at(data, gpos + 6)?);
        let lookups_at = gpos + usize::from(u16_at(data, gpos + 8)?);
        let mut wanted = BTreeSet::new();
        for i in 0..usize::from(u16_at(data, features)?) {
            let record = features + 2 + i * 6;
            if data.get(record..record + 4)? != b"kern" {
                continue;
            }
            let feature = features + usize::from(u16_at(data, record + 4)?);
            for k in 0..usize::from(u16_at(data, feature + 2)?) {
                wanted.insert(usize::from(u16_at(data, feature + 4 + k * 2)?));
            }
        }
        let mut lookups = Vec::new();
        for index in wanted.into_iter().take(MAX_LOOKUPS) {
            let lookup = lookups_at + usize::from(u16_at(data, lookups_at + 2 + index * 2)?);
            let (kind, subtables) = (
                u16_at(data, lookup)?,
                usize::from(u16_at(data, lookup + 4)?),
            );
            let mut found = Vec::new();
            for s in 0..subtables.min(MAX_SUBTABLES) {
                let sub = lookup + usize::from(u16_at(data, lookup + 6 + s * 2)?);
                match kind {
                    2 => found.push(sub),
                    // An extension: the real subtable is at a 32-bit offset, and must be a pair adjustment.
                    9 if u16_at(data, sub + 2)? == 2 => {
                        found.push(sub + u32_at(data, sub + 4)? as usize);
                    }
                    _ => {}
                }
            }
            if !found.is_empty() {
                lookups.push(found);
            }
        }
        Some(Self { lookups })
    }

    /// The adjustment to the pen after `left` when `right` follows, in font units.
    #[must_use]
    pub fn pair(&self, data: &[u8], left: u16, right: u16) -> i16 {
        let mut total = 0i32;
        for lookup in &self.lookups {
            for &sub in lookup {
                if let Some(adjustment) = pair_in(data, sub, left, right) {
                    total += i32::from(adjustment);
                    break;
                }
            }
        }
        total.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16
    }
}

/// Bytes in a value record of this format: two for each field it has.
const fn record_size(format: u16) -> usize {
    (format & 0x00FF).count_ones() as usize * 2
}

/// The `XAdvance` of the value record at `at`, if the format has one.
fn x_advance(data: &[u8], at: usize, format: u16) -> Option<i16> {
    if format & 0x0004 == 0 {
        return Some(0);
    }
    i16_at(data, at + (format & 0x0003).count_ones() as usize * 2)
}

fn pair_in(data: &[u8], sub: usize, left: u16, right: u16) -> Option<i16> {
    let format = u16_at(data, sub)?;
    let covered = coverage_index(data, sub + usize::from(u16_at(data, sub + 2)?), left)?;
    let (first, second) = (u16_at(data, sub + 4)?, u16_at(data, sub + 6)?);
    let (size1, size2) = (record_size(first), record_size(second));
    match format {
        1 => {
            let sets = usize::from(u16_at(data, sub + 8)?);
            if covered >= sets {
                return None;
            }
            let set = sub + usize::from(u16_at(data, sub + 10 + covered * 2)?);
            let count = usize::from(u16_at(data, set)?);
            let width = 2 + size1 + size2;
            let (mut low, mut high) = (0, count);
            while low < high {
                let mid = usize::midpoint(low, high);
                let glyph = u16_at(data, set + 2 + mid * width)?;
                match glyph.cmp(&right) {
                    std::cmp::Ordering::Less => low = mid + 1,
                    std::cmp::Ordering::Greater => high = mid,
                    std::cmp::Ordering::Equal => {
                        return x_advance(data, set + 2 + mid * width + 2, first);
                    }
                }
            }
            None
        }
        2 => {
            let class1 = class_of(data, sub + usize::from(u16_at(data, sub + 8)?), left)?;
            let class2 = class_of(data, sub + usize::from(u16_at(data, sub + 10)?), right)?;
            let (count1, count2) = (
                usize::from(u16_at(data, sub + 12)?),
                usize::from(u16_at(data, sub + 14)?),
            );
            if usize::from(class1) >= count1 || usize::from(class2) >= count2 {
                return None;
            }
            let record =
                sub + 16 + (usize::from(class1) * count2 + usize::from(class2)) * (size1 + size2);
            x_advance(data, record, first)
        }
        _ => None,
    }
}

/// The position of `glyph` in a coverage table, if it is covered.
fn coverage_index(data: &[u8], at: usize, glyph: u16) -> Option<usize> {
    let count = usize::from(u16_at(data, at + 2)?);
    match u16_at(data, at)? {
        1 => {
            let (mut low, mut high) = (0, count);
            while low < high {
                let mid = usize::midpoint(low, high);
                match u16_at(data, at + 4 + mid * 2)?.cmp(&glyph) {
                    std::cmp::Ordering::Less => low = mid + 1,
                    std::cmp::Ordering::Greater => high = mid,
                    std::cmp::Ordering::Equal => return Some(mid),
                }
            }
            None
        }
        2 => {
            let (mut low, mut high) = (0, count);
            while low < high {
                let mid = usize::midpoint(low, high);
                let record = at + 4 + mid * 6;
                let (start, end) = (u16_at(data, record)?, u16_at(data, record + 2)?);
                if glyph < start {
                    high = mid;
                } else if glyph > end {
                    low = mid + 1;
                } else {
                    return Some(
                        usize::from(u16_at(data, record + 4)?) + usize::from(glyph - start),
                    );
                }
            }
            None
        }
        _ => None,
    }
}

/// The class of `glyph` in a class definition; 0 for a glyph the table does not list.
fn class_of(data: &[u8], at: usize, glyph: u16) -> Option<u16> {
    match u16_at(data, at)? {
        1 => {
            let (start, count) = (u16_at(data, at + 2)?, u16_at(data, at + 4)?);
            if glyph < start || glyph - start >= count {
                return Some(0);
            }
            u16_at(data, at + 6 + usize::from(glyph - start) * 2)
        }
        2 => {
            let count = usize::from(u16_at(data, at + 2)?);
            let (mut low, mut high) = (0, count);
            while low < high {
                let mid = usize::midpoint(low, high);
                let record = at + 4 + mid * 6;
                let (start, end) = (u16_at(data, record)?, u16_at(data, record + 2)?);
                if glyph < start {
                    high = mid;
                } else if glyph > end {
                    low = mid + 1;
                } else {
                    return u16_at(data, record + 4);
                }
            }
            Some(0)
        }
        _ => None,
    }
}

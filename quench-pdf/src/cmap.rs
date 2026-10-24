//! Character maps: `ToUnicode` streams and embedded `CMap`s that say how many bytes make a code.

use crate::object::{Object, Parser};
use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct CMap {
    /// `(byte length, low, high)` of each code space range.
    pub codespaces: Vec<(usize, u32, u32)>,
    bf_chars: HashMap<u32, String>,
    /// `(low, high, text of the low code)`: later codes increment the last character.
    bf_ranges: Vec<(u32, u32, String)>,
    bf_arrays: Vec<(u32, Vec<String>)>,
    /// `(low, high, first CID)`.
    cid_ranges: Vec<(u32, u32, u32)>,
}

fn code(bytes: &[u8]) -> u32 {
    bytes.iter().fold(0u32, |acc, &b| acc << 8 | u32::from(b))
}

/// UTF-16BE bytes as text.
fn utf16(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes
        .chunks(2)
        .map(|c| u16::from(c[0]) << 8 | u16::from(c.get(1).copied().unwrap_or(0)))
        .collect();
    String::from_utf16_lossy(&units)
}

impl CMap {
    #[must_use]
    pub fn parse(data: &[u8]) -> Self {
        let mut map = Self::default();
        let mut parser = Parser::new(data, 0, false);
        let mut operands: Vec<Object> = Vec::new();
        while let Ok(Some(object)) = parser.parse() {
            let Object::Op(op) = &object else {
                operands.push(object);
                continue;
            };
            match op.as_str() {
                "endcodespacerange" => {
                    for pair in operands.as_chunks::<2>().0 {
                        if let (Object::String(lo), Object::String(hi)) = (&pair[0], &pair[1]) {
                            map.codespaces.push((lo.len(), code(lo), code(hi)));
                        }
                    }
                }
                "endbfchar" => {
                    for pair in operands.as_chunks::<2>().0 {
                        if let (Object::String(src), Object::String(dst)) = (&pair[0], &pair[1]) {
                            map.bf_chars.insert(code(src), utf16(dst));
                        }
                    }
                }
                "endbfrange" => {
                    for triple in operands.as_chunks::<3>().0 {
                        let (Object::String(lo), Object::String(hi)) = (&triple[0], &triple[1])
                        else {
                            continue;
                        };
                        match &triple[2] {
                            Object::String(dst) => {
                                map.bf_ranges.push((code(lo), code(hi), utf16(dst)));
                            }
                            Object::Array(items) => map.bf_arrays.push((
                                code(lo),
                                items
                                    .iter()
                                    .map(|o| match o {
                                        Object::String(s) => utf16(s),
                                        _ => String::new(),
                                    })
                                    .collect(),
                            )),
                            _ => {}
                        }
                    }
                }
                "endcidrange" => {
                    for triple in operands.as_chunks::<3>().0 {
                        if let (Object::String(lo), Object::String(hi), Object::Int(cid)) =
                            (&triple[0], &triple[1], &triple[2])
                        {
                            map.cid_ranges.push((
                                code(lo),
                                code(hi),
                                u32::try_from(*cid).unwrap_or(0),
                            ));
                        }
                    }
                }
                "endcidchar" => {
                    for pair in operands.as_chunks::<2>().0 {
                        if let (Object::String(src), Object::Int(cid)) = (&pair[0], &pair[1]) {
                            let c = code(src);
                            map.cid_ranges
                                .push((c, c, u32::try_from(*cid).unwrap_or(0)));
                        }
                    }
                }
                _ => {}
            }
            operands.clear();
        }
        map
    }

    /// The text a code stands for, when the map says.
    #[must_use]
    pub fn unicode(&self, code: u32) -> Option<String> {
        if let Some(s) = self.bf_chars.get(&code) {
            return Some(s.clone());
        }
        for (lo, hi, base) in &self.bf_ranges {
            if (*lo..=*hi).contains(&code) {
                let mut chars: Vec<char> = base.chars().collect();
                if let Some(last) = chars.last_mut() {
                    *last = char::from_u32(*last as u32 + (code - lo)).unwrap_or('\u{FFFD}');
                }
                return Some(chars.into_iter().collect());
            }
        }
        for (lo, items) in &self.bf_arrays {
            if code >= *lo
                && let Some(s) = items.get((code - lo) as usize)
            {
                return Some(s.clone());
            }
        }
        None
    }

    /// Whether the map has any code → text entries.
    #[must_use]
    pub fn has_unicode(&self) -> bool {
        !(self.bf_chars.is_empty() && self.bf_ranges.is_empty() && self.bf_arrays.is_empty())
    }

    /// The CID a code maps to (the code itself when there is no CID mapping).
    #[must_use]
    pub fn cid(&self, code: u32) -> u32 {
        self.cid_ranges
            .iter()
            .find(|(lo, hi, _)| (*lo..=*hi).contains(&code))
            .map_or(code, |(lo, _, first)| first + (code - lo))
    }

    /// Splits `bytes` into codes using the code space ranges (two bytes at a time when there are none).
    #[must_use]
    pub fn split(&self, bytes: &[u8]) -> Vec<(u32, usize)> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < bytes.len() {
            let mut length = 0;
            for n in 1..=4.min(bytes.len() - i) {
                let value = code(&bytes[i..i + n]);
                if self
                    .codespaces
                    .iter()
                    .any(|&(len, lo, hi)| len == n && (lo..=hi).contains(&value))
                {
                    length = n;
                    break;
                }
            }
            if length == 0 {
                length = self
                    .codespaces
                    .iter()
                    .map(|c| c.0)
                    .min()
                    .unwrap_or(2)
                    .min(bytes.len() - i);
            }
            out.push((code(&bytes[i..i + length]), length));
            i += length;
        }
        out
    }
}

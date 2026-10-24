//! Fonts: how the bytes of a string map to text and to advance widths.

use crate::cmap::CMap;
use crate::doc::Document;
use crate::glyphs::{GLYPHS, MAC_ROMAN, STANDARD, WIN_ANSI};
use crate::object::{Dict, Object};
use std::collections::HashMap;

/// One shown character code.
#[derive(Debug, Clone)]
pub struct Glyph {
    pub text: String,
    /// Advance in thousandths of the font size.
    pub width: f64,
    /// A single-byte code 32, which word spacing applies to.
    pub is_space: bool,
}

#[derive(Debug)]
pub struct Font {
    composite: bool,
    to_unicode: Option<CMap>,
    /// The `CMap` that splits composite strings into codes (and maps them to CIDs).
    code_map: CMap,
    encoding: Vec<Option<String>>,
    widths: HashMap<u32, f64>,
    default_width: f64,
}

/// A glyph name as text: the Adobe Glyph List, `uniXXXX`, `uXXXX` and `a_b` ligature names.
#[must_use]
pub fn glyph_text(name: &str) -> Option<String> {
    let base = name.split('.').next().unwrap_or(name);
    if base.is_empty() {
        return None;
    }
    if let Ok(i) = GLYPHS.binary_search_by(|(n, _)| (*n).cmp(base)) {
        return char::from_u32(GLYPHS[i].1).map(String::from);
    }
    if let Some(hex) = base.strip_prefix("uni")
        && hex.len() >= 4
        && hex.len() % 4 == 0
        && hex.bytes().all(|b| b.is_ascii_hexdigit())
    {
        let units: Vec<u16> = (0..hex.len() / 4)
            .filter_map(|i| u16::from_str_radix(&hex[i * 4..i * 4 + 4], 16).ok())
            .collect();
        return Some(String::from_utf16_lossy(&units));
    }
    if let Some(hex) = base.strip_prefix('u')
        && (4..=6).contains(&hex.len())
        && hex.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return u32::from_str_radix(hex, 16)
            .ok()
            .and_then(char::from_u32)
            .map(String::from);
    }
    if base.contains('_') {
        let parts: Option<Vec<String>> = base.split('_').map(glyph_text).collect();
        return parts.map(|p| p.concat());
    }
    None
}

fn base_encoding(name: &str) -> Option<&'static [u16; 256]> {
    match name {
        "WinAnsiEncoding" => Some(&WIN_ANSI),
        "MacRomanEncoding" => Some(&MAC_ROMAN),
        "StandardEncoding" | "MacExpertEncoding" => Some(&STANDARD),
        _ => None,
    }
}

fn table_to_encoding(table: &[u16; 256]) -> Vec<Option<String>> {
    table
        .iter()
        .map(|&u| {
            (u != 0)
                .then(|| char::from_u32(u32::from(u)).map(String::from))
                .flatten()
        })
        .collect()
}

impl Font {
    /// Reads a font dictionary.
    #[must_use]
    pub fn load(doc: &Document, dict: &Dict) -> Self {
        let subtype = doc
            .get(dict, "Subtype")
            .and_then(|o| o.as_name().map(String::from))
            .unwrap_or_default();
        let to_unicode = doc
            .get(dict, "ToUnicode")
            .and_then(|o| doc.stream_data(&o))
            .map(|data| CMap::parse(&data))
            .filter(CMap::has_unicode);

        if subtype == "Type0" {
            return Self::composite(doc, dict, to_unicode);
        }

        let truetype = subtype == "TrueType";
        let mut encoding = table_to_encoding(if truetype { &WIN_ANSI } else { &STANDARD });
        match doc.get(dict, "Encoding") {
            Some(Object::Name(n)) => {
                if let Some(t) = base_encoding(&n) {
                    encoding = table_to_encoding(t);
                }
            }
            Some(Object::Dict(d)) => {
                if let Some(Object::Name(n)) = doc.get(&d, "BaseEncoding")
                    && let Some(t) = base_encoding(&n)
                {
                    encoding = table_to_encoding(t);
                }
                if let Some(Object::Array(diffs)) = doc.get(&d, "Differences") {
                    let mut at = 0usize;
                    for item in &diffs {
                        match doc.resolve(item) {
                            Object::Int(n) => at = usize::try_from(n).unwrap_or(0),
                            Object::Name(name) => {
                                if at < 256 {
                                    encoding[at] = glyph_text(&name);
                                }
                                at += 1;
                            }
                            _ => {}
                        }
                    }
                }
            }
            _ => {}
        }

        let scale = if subtype == "Type3" {
            match doc.get(dict, "FontMatrix") {
                Some(Object::Array(m)) => {
                    m.first()
                        .and_then(|o| doc.resolve(o).as_f64())
                        .unwrap_or(0.001)
                        * 1000.0
                }
                _ => 1.0,
            }
        } else {
            1.0
        };
        let first = doc
            .get(dict, "FirstChar")
            .and_then(|o| o.as_int())
            .unwrap_or(0);
        let mut widths = HashMap::new();
        if let Some(Object::Array(list)) = doc.get(dict, "Widths") {
            for (i, w) in list.iter().enumerate() {
                if let Some(w) = doc.resolve(w).as_f64() {
                    widths.insert(u32::try_from(first + i as i64).unwrap_or(0), w * scale);
                }
            }
        }
        let missing = doc
            .get(dict, "FontDescriptor")
            .and_then(|d| d.as_dict().and_then(|d| doc.get(d, "MissingWidth")))
            .and_then(|o| o.as_f64());
        let base_font = doc
            .get(dict, "BaseFont")
            .and_then(|o| o.as_name().map(String::from))
            .unwrap_or_default();
        let default_width = missing.unwrap_or(if base_font.contains("Courier") {
            600.0
        } else {
            500.0
        });

        Self {
            composite: false,
            to_unicode,
            code_map: CMap::default(),
            encoding,
            widths,
            default_width,
        }
    }

    fn composite(doc: &Document, dict: &Dict, to_unicode: Option<CMap>) -> Self {
        let mut code_map = CMap::default();
        match doc.get(dict, "Encoding") {
            Some(Object::Name(_)) | None => {}
            Some(other) => {
                if let Some(data) = doc.stream_data(&other) {
                    code_map = CMap::parse(&data);
                }
            }
        }
        let descendant = match doc.get(dict, "DescendantFonts") {
            Some(Object::Array(a)) => a.first().map(|o| doc.resolve(o)),
            Some(o @ Object::Dict(_)) => Some(o),
            _ => None,
        };
        let mut widths = HashMap::new();
        let mut default_width = 1000.0;
        if let Some(Object::Dict(cid)) = descendant {
            default_width = doc
                .get(&cid, "DW")
                .and_then(|o| o.as_f64())
                .unwrap_or(1000.0);
            if let Some(Object::Array(w)) = doc.get(&cid, "W") {
                let items: Vec<Object> = w.iter().map(|o| doc.resolve(o)).collect();
                let mut i = 0;
                while i < items.len() {
                    let Some(first) = items[i].as_int() else {
                        i += 1;
                        continue;
                    };
                    match items.get(i + 1) {
                        Some(Object::Array(list)) => {
                            for (k, w) in list.iter().enumerate() {
                                if let Some(w) = doc.resolve(w).as_f64() {
                                    widths.insert(u32::try_from(first + k as i64).unwrap_or(0), w);
                                }
                            }
                            i += 2;
                        }
                        Some(last) if last.as_int().is_some() => {
                            let (last, w) = (
                                last.as_int().unwrap_or(first),
                                items.get(i + 2).and_then(Object::as_f64),
                            );
                            if let Some(w) = w {
                                for c in first..=last.min(first + 70_000) {
                                    widths.insert(u32::try_from(c).unwrap_or(0), w);
                                }
                            }
                            i += 3;
                        }
                        _ => i += 1,
                    }
                }
            }
        }
        Self {
            composite: true,
            to_unicode,
            code_map,
            encoding: Vec::new(),
            widths,
            default_width,
        }
    }

    /// The glyphs a shown string stands for.
    #[must_use]
    pub fn decode(&self, bytes: &[u8]) -> Vec<Glyph> {
        if self.composite {
            return self
                .code_map
                .split(bytes)
                .into_iter()
                .map(|(code, _)| {
                    let cid = self.code_map.cid(code);
                    let text = self
                        .to_unicode
                        .as_ref()
                        .and_then(|m| m.unicode(code))
                        .unwrap_or_else(|| {
                            char::from_u32(code).map(String::from).unwrap_or_default()
                        });
                    Glyph {
                        text,
                        width: self.widths.get(&cid).copied().unwrap_or(self.default_width),
                        is_space: false,
                    }
                })
                .collect();
        }
        bytes
            .iter()
            .map(|&b| {
                let code = u32::from(b);
                let text = self
                    .to_unicode
                    .as_ref()
                    .and_then(|m| m.unicode(code))
                    .or_else(|| self.encoding.get(usize::from(b)).cloned().flatten())
                    .unwrap_or_default();
                let width = self.widths.get(&code).copied().unwrap_or(
                    if b == 32 && self.widths.is_empty() {
                        278.0
                    } else {
                        self.default_width
                    },
                );
                Glyph {
                    text,
                    width,
                    is_space: b == 32,
                }
            })
            .collect()
    }
}

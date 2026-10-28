//! A TrueType font (`glyf` outlines): its tables, character map, metrics and glyph outlines.
//!
//! The parser reads what text needs and nothing else: `head`, `maxp`, `hhea`, `hmtx`, `cmap` (formats 4 and
//! 12), `loca`, `glyf` (simple and composite glyphs), `name`, and the pair kerning of `GPOS`. Every read is
//! bounds-checked, so a damaged or hostile file is an error or a missing glyph, never a panic or an overrun.

use std::borrow::Cow;
use std::fmt;

use quench_raster::Path;

use crate::gpos::Kerning;

/// Why bytes are not a font this crate can use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontError {
    /// Not an sfnt with TrueType outlines (`OTTO` fonts, with CFF outlines, are not read).
    NotTrueType,
    /// A table the font cannot work without is missing.
    Missing(&'static str),
    /// A table is cut short or points outside the file.
    Truncated(&'static str),
}

impl fmt::Display for FontError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotTrueType => f.write_str("not a TrueType font"),
            Self::Missing(table) => write!(f, "the `{table}` table is missing"),
            Self::Truncated(table) => write!(f, "the `{table}` table is cut short"),
        }
    }
}

impl std::error::Error for FontError {}

/// A point of a glyph outline, in font units with y pointing up.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
    /// On the curve; an off-curve point is a quadratic control point.
    pub on: bool,
}

/// A glyph's closed contours.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Outline {
    pub contours: Vec<Vec<Point>>,
}

impl Outline {
    /// The outline as a [`Path`] in pixels: scaled by `scale` (pixels per font unit), with the origin at
    /// `(x, y)` and y pointing down. Curves are flattened in pixel space, so the tolerance is in pixels.
    #[must_use]
    pub fn to_path(&self, scale: f32, x: f32, y: f32) -> Path {
        self.to_path_slanted(scale, x, y, 0.0)
    }

    /// [`to_path`](Self::to_path) leaning to the right: every point moves right by `slant` times its height above
    /// the baseline (0.2 is the lean of a synthetic italic).
    #[must_use]
    pub fn to_path_slanted(&self, scale: f32, x: f32, y: f32, slant: f32) -> Path {
        let at = |p: Point| (x + p.x * scale + slant * p.y * scale, y - p.y * scale);
        let mid = |a: (f32, f32), b: (f32, f32)| (f32::midpoint(a.0, b.0), f32::midpoint(a.1, b.1));
        let mut path = Path::new();
        for contour in &self.contours {
            if contour.len() < 2 {
                continue;
            }
            // Start at an on-curve point; with none, at the middle between the last and first control point.
            #[allow(clippy::option_if_let_else)]
            // two arms of different shapes read better than a closure pair
            let (start, rest): ((f32, f32), Vec<Point>) = match contour.iter().position(|p| p.on) {
                Some(i) => {
                    let rotated: Vec<Point> =
                        contour[i..].iter().chain(&contour[..i]).copied().collect();
                    (at(rotated[0]), rotated[1..].to_vec())
                }
                None => (
                    mid(at(contour[0]), at(contour[contour.len() - 1])),
                    contour.clone(),
                ),
            };
            path.move_to(start.0, start.1);
            let mut pending: Option<(f32, f32)> = None;
            for p in rest {
                let point = at(p);
                if p.on {
                    match pending.take() {
                        Some(c) => path.quad_to(c.0, c.1, point.0, point.1),
                        None => path.line_to(point.0, point.1),
                    };
                } else {
                    if let Some(c) = pending {
                        let m = mid(c, point);
                        path.quad_to(c.0, c.1, m.0, m.1);
                    }
                    pending = Some(point);
                }
            }
            if let Some(c) = pending {
                path.quad_to(c.0, c.1, start.0, start.1);
            }
            path.close();
        }
        path
    }
}

/// A parsed font. The bytes are kept (borrowed when they are `'static`, such as an embedded file) and read on
/// demand: nothing is copied out but the table positions.
pub struct Font {
    data: Cow<'static, [u8]>,
    units_per_em: u16,
    ascent: i16,
    descent: i16,
    line_gap: i16,
    num_glyphs: u16,
    num_h_metrics: u16,
    loca_long: bool,
    glyf: usize,
    loca: usize,
    hmtx: usize,
    name: Option<usize>,
    /// Where the chosen character map starts and its format (4 or 12).
    cmap: (usize, u16),
    kerning: Kerning,
}

impl fmt::Debug for Font {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Font")
            .field("family", &self.name(1))
            .field("glyphs", &self.num_glyphs)
            .field("units_per_em", &self.units_per_em)
            .finish_non_exhaustive()
    }
}

/// Big-endian reads that return `None` past the end.
pub fn u16_at(data: &[u8], at: usize) -> Option<u16> {
    let b = data.get(at..at.checked_add(2)?)?;
    Some(u16::from_be_bytes([b[0], b[1]]))
}

pub fn i16_at(data: &[u8], at: usize) -> Option<i16> {
    u16_at(data, at).map(|v| v as i16)
}

pub fn u32_at(data: &[u8], at: usize) -> Option<u32> {
    let b = data.get(at..at.checked_add(4)?)?;
    Some(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}

/// Components deeper than this, or more of them, are not followed (a loop or a bomb).
const MAX_DEPTH: usize = 8;
const MAX_COMPONENTS: usize = 64;
/// The most points one glyph may have after its components are put together.
const MAX_POINTS: usize = 20_000;

impl Font {
    /// Reads a font.
    ///
    /// # Errors
    /// [`FontError`] when the bytes are not a TrueType font with the tables text needs.
    pub fn parse(data: impl Into<Cow<'static, [u8]>>) -> Result<Self, FontError> {
        let data = data.into();
        let bytes: &[u8] = &data;
        // A TrueType file (`0x00010000` or `true`), or the first font of a collection is not read here.
        match bytes.get(..4) {
            Some([0, 1, 0, 0] | b"true") => {}
            _ => return Err(FontError::NotTrueType),
        }
        let count = usize::from(u16_at(bytes, 4).ok_or(FontError::Truncated("directory"))?);
        let table = |tag: &[u8; 4]| -> Option<(usize, usize)> {
            (0..count).find_map(|i| {
                let record = 12 + i * 16;
                (bytes.get(record..record + 4)? == tag).then_some(())?;
                let (offset, length) = (
                    u32_at(bytes, record + 8)? as usize,
                    u32_at(bytes, record + 12)? as usize,
                );
                // A table that runs past the end of the file is as good as missing.
                (offset.checked_add(length)? <= bytes.len()).then_some((offset, length))
            })
        };
        let need =
            |tag: &'static [u8; 4], name: &'static str| table(tag).ok_or(FontError::Missing(name));
        let (head, head_len) = need(b"head", "head")?;
        let (maxp, _) = need(b"maxp", "maxp")?;
        let (hhea, _) = need(b"hhea", "hhea")?;
        let (hmtx, _) = need(b"hmtx", "hmtx")?;
        let (loca, _) = need(b"loca", "loca")?;
        let (glyf, _) = need(b"glyf", "glyf")?;
        let (cmap, _) = need(b"cmap", "cmap")?;
        if head_len < 54 {
            return Err(FontError::Truncated("head"));
        }
        let units_per_em = u16_at(bytes, head + 18)
            .filter(|u| *u > 0)
            .ok_or(FontError::Truncated("head"))?;
        let loca_long = i16_at(bytes, head + 50).ok_or(FontError::Truncated("head"))? != 0;
        let num_glyphs = u16_at(bytes, maxp + 4).ok_or(FontError::Truncated("maxp"))?;
        let truncated = FontError::Truncated("hhea");
        let ascent = i16_at(bytes, hhea + 4).ok_or(truncated)?;
        let descent = i16_at(bytes, hhea + 6).ok_or(truncated)?;
        let line_gap = i16_at(bytes, hhea + 8).ok_or(truncated)?;
        let num_h_metrics = u16_at(bytes, hhea + 34).ok_or(truncated)?;
        let cmap = choose_cmap(bytes, cmap).ok_or(FontError::Missing("cmap (a Unicode table)"))?;
        let kerning = table(b"GPOS").map_or_else(Kerning::default, |(gpos, length)| {
            Kerning::parse(bytes, gpos, length)
        });
        Ok(Self {
            units_per_em,
            ascent,
            descent,
            line_gap,
            num_glyphs,
            num_h_metrics,
            loca_long,
            glyf,
            loca,
            hmtx,
            name: table(b"name").map(|t| t.0),
            cmap,
            kerning,
            data,
        })
    }

    #[must_use]
    pub const fn units_per_em(&self) -> u16 {
        self.units_per_em
    }

    /// Distance from the baseline up to the top of a line, in font units.
    #[must_use]
    pub const fn ascent(&self) -> i16 {
        self.ascent
    }

    /// Distance from the baseline down to the bottom of a line, in font units (negative).
    #[must_use]
    pub const fn descent(&self) -> i16 {
        self.descent
    }

    /// Extra space between lines, in font units.
    #[must_use]
    pub const fn line_gap(&self) -> i16 {
        self.line_gap
    }

    #[must_use]
    pub const fn glyph_count(&self) -> u16 {
        self.num_glyphs
    }

    /// The glyph for `c`, or `None` when the font has none (glyph 0, `.notdef`, is not an answer).
    #[must_use]
    pub fn glyph(&self, c: char) -> Option<u16> {
        let data: &[u8] = &self.data;
        let (at, format) = self.cmap;
        let code = c as u32;
        let id = match format {
            4 => cmap4(data, at, code)?,
            12 => cmap12(data, at, code)?,
            _ => return None,
        };
        (id != 0 && id < self.num_glyphs).then_some(id)
    }

    /// How far the pen moves after `glyph`, in font units.
    #[must_use]
    pub fn advance(&self, glyph: u16) -> u16 {
        let data: &[u8] = &self.data;
        if self.num_h_metrics == 0 {
            return 0;
        }
        let index = glyph.min(self.num_h_metrics - 1);
        u16_at(data, self.hmtx + usize::from(index) * 4).unwrap_or(0)
    }

    /// The adjustment to the pen between `left` and `right`, in font units (negative pulls them together).
    #[must_use]
    pub fn kerning(&self, left: u16, right: u16) -> i16 {
        self.kerning.pair(&self.data, left, right)
    }

    /// A name from the `name` table (1 family, 2 style, 0 copyright, 13 licence, 14 licence URL), in English.
    #[must_use]
    pub fn name(&self, id: u16) -> Option<String> {
        let data: &[u8] = &self.data;
        let table = self.name?;
        let count = usize::from(u16_at(data, table + 2)?);
        let strings = table + usize::from(u16_at(data, table + 4)?);
        let mut fallback = None;
        for i in 0..count {
            let record = table + 6 + i * 12;
            let (platform, encoding, name_id) = (
                u16_at(data, record)?,
                u16_at(data, record + 2)?,
                u16_at(data, record + 6)?,
            );
            if name_id != id {
                continue;
            }
            let (length, offset) = (
                usize::from(u16_at(data, record + 8)?),
                usize::from(u16_at(data, record + 10)?),
            );
            let raw = data.get(strings + offset..strings + offset + length)?;
            match (platform, encoding) {
                (3, 1 | 10) | (0, _) => {
                    let units: Vec<u16> = raw
                        .as_chunks::<2>()
                        .0
                        .iter()
                        .map(|c| u16::from_be_bytes(*c))
                        .collect();
                    return Some(String::from_utf16_lossy(&units));
                }
                (1, 0) => fallback = Some(raw.iter().map(|b| char::from(*b)).collect()),
                _ => {}
            }
        }
        fallback
    }

    /// The outline of `glyph`, or `None` for a glyph with no outline (a space) or one that is damaged.
    #[must_use]
    pub fn outline(&self, glyph: u16) -> Option<Outline> {
        let mut count = 0;
        let outline = self.outline_at(glyph, 0, &mut count)?;
        (!outline.contours.is_empty()).then_some(outline)
    }

    fn glyph_range(&self, glyph: u16) -> Option<(usize, usize)> {
        let data: &[u8] = &self.data;
        if glyph >= self.num_glyphs {
            return None;
        }
        let i = usize::from(glyph);
        let (start, end) = if self.loca_long {
            (
                u32_at(data, self.loca + i * 4)? as usize,
                u32_at(data, self.loca + i * 4 + 4)? as usize,
            )
        } else {
            (
                usize::from(u16_at(data, self.loca + i * 2)?) * 2,
                usize::from(u16_at(data, self.loca + i * 2 + 2)?) * 2,
            )
        };
        (end > start).then_some((self.glyf + start, self.glyf + end))
    }

    fn outline_at(&self, glyph: u16, depth: usize, components: &mut usize) -> Option<Outline> {
        let data: &[u8] = &self.data;
        let Some((start, end)) = self.glyph_range(glyph) else {
            return Some(Outline::default()); // no outline (a space), or out of range
        };
        let body = data.get(start..end)?;
        let contours = i16_at(body, 0)?;
        if contours >= 0 {
            simple_glyph(body, contours as usize)
        } else {
            if depth >= MAX_DEPTH {
                return None;
            }
            let mut result = Outline::default();
            let mut at = 10;
            loop {
                *components += 1;
                if *components > MAX_COMPONENTS {
                    return None;
                }
                let flags = u16_at(body, at)?;
                let child = u16_at(body, at + 2)?;
                at += 4;
                let (dx, dy) = if flags & 0x0001 != 0 {
                    let v = (i16_at(body, at)?, i16_at(body, at + 2)?);
                    at += 4;
                    v
                } else {
                    let v = (
                        i16::from(*body.get(at)? as i8),
                        i16::from(*body.get(at + 1)? as i8),
                    );
                    at += 2;
                    v
                };
                // Only offsets are supported; a component placed by matching points is left out.
                let placed = flags & 0x0002 != 0;
                let f2dot14 = |at: usize| i16_at(body, at).map(|v| f32::from(v) / 16384.0);
                let (mut a, mut b, mut c, mut d) = (1.0, 0.0, 0.0, 1.0);
                if flags & 0x0008 != 0 {
                    a = f2dot14(at)?;
                    d = a;
                    at += 2;
                } else if flags & 0x0040 != 0 {
                    a = f2dot14(at)?;
                    d = f2dot14(at + 2)?;
                    at += 4;
                } else if flags & 0x0080 != 0 {
                    a = f2dot14(at)?;
                    b = f2dot14(at + 2)?;
                    c = f2dot14(at + 4)?;
                    d = f2dot14(at + 6)?;
                    at += 8;
                }
                if placed {
                    let part = self.outline_at(child, depth + 1, components)?;
                    for contour in part.contours {
                        result.contours.push(
                            contour
                                .into_iter()
                                .map(|p| Point {
                                    // x' = a x + c y + dx, y' = b x + d y + dy
                                    x: a * p.x + c * p.y + f32::from(dx),
                                    y: b * p.x + d * p.y + f32::from(dy),
                                    on: p.on,
                                })
                                .collect(),
                        );
                    }
                    if result.contours.iter().map(Vec::len).sum::<usize>() > MAX_POINTS {
                        return None;
                    }
                }
                if flags & 0x0020 == 0 {
                    break;
                }
            }
            Some(result)
        }
    }
}

/// The ranges of code points the `cmap` table at `table` (inside `data`) covers, as `(first, last)` pairs; a
/// range may include codes the font leaves without a glyph, so it says where to look, not what is there.
pub fn cmap_ranges(data: &[u8], table: usize) -> Vec<(u32, u32)> {
    let Some((at, format)) = choose_cmap(data, table) else {
        return Vec::new();
    };
    let mut ranges = Vec::new();
    match format {
        4 => {
            let segments = usize::from(u16_at(data, at + 6).unwrap_or(0) / 2);
            for i in 0..segments {
                let end = u16_at(data, at + 14 + i * 2);
                let start = u16_at(data, at + 16 + segments * 2 + i * 2);
                if let (Some(start), Some(end)) = (start, end)
                    && start <= end
                    && end != 0xFFFF
                {
                    ranges.push((u32::from(start), u32::from(end)));
                }
            }
        }
        12 => {
            let groups = u32_at(data, at + 12).unwrap_or(0) as usize;
            for i in 0..groups.min(65_536) {
                if let (Some(start), Some(end)) = (
                    u32_at(data, at + 16 + i * 12),
                    u32_at(data, at + 20 + i * 12),
                ) && start <= end
                {
                    ranges.push((start, end));
                }
            }
        }
        _ => {}
    }
    ranges
}

/// The best Unicode character map of the `cmap` table at `table`: its position and format.
fn choose_cmap(data: &[u8], table: usize) -> Option<(usize, u16)> {
    let count = usize::from(u16_at(data, table + 2)?);
    let mut best: Option<(u8, usize, u16)> = None;
    for i in 0..count {
        let record = table + 4 + i * 8;
        let (platform, encoding) = (u16_at(data, record)?, u16_at(data, record + 2)?);
        let at = table + u32_at(data, record + 4)? as usize;
        let format = u16_at(data, at)?;
        // Full Unicode (format 12) over the Basic Multilingual Plane (format 4).
        let rank = match (platform, encoding, format) {
            (3, 10, 12) => 4,
            (0, 4 | 6, 12) => 3,
            (3, 1, 4) => 2,
            (0, _, 4) => 1,
            _ => continue,
        };
        if best.is_none_or(|b| rank > b.0) {
            best = Some((rank, at, format));
        }
    }
    best.map(|(_, at, format)| (at, format))
}

fn cmap4(data: &[u8], at: usize, code: u32) -> Option<u16> {
    let code = u16::try_from(code).ok()?;
    let segments = usize::from(u16_at(data, at + 6)? / 2);
    let ends = at + 14;
    let starts = ends + segments * 2 + 2;
    let deltas = starts + segments * 2;
    let offsets = deltas + segments * 2;
    // The first segment whose end is not below the code (segments are sorted by end).
    let (mut low, mut high) = (0, segments);
    while low < high {
        let mid = usize::midpoint(low, high);
        if u16_at(data, ends + mid * 2)? < code {
            low = mid + 1;
        } else {
            high = mid;
        }
    }
    if low == segments {
        return None;
    }
    let start = u16_at(data, starts + low * 2)?;
    if code < start {
        return None;
    }
    let delta = u16_at(data, deltas + low * 2)?;
    let range = u16_at(data, offsets + low * 2)?;
    if range == 0 {
        return Some(code.wrapping_add(delta));
    }
    let slot = offsets + low * 2 + usize::from(range) + usize::from(code - start) * 2;
    let glyph = u16_at(data, slot)?;
    (glyph != 0).then(|| glyph.wrapping_add(delta))
}

fn cmap12(data: &[u8], at: usize, code: u32) -> Option<u16> {
    let groups = u32_at(data, at + 12)? as usize;
    let (mut low, mut high) = (0, groups);
    while low < high {
        let mid = usize::midpoint(low, high);
        if u32_at(data, at + 16 + mid * 12 + 4)? < code {
            low = mid + 1;
        } else {
            high = mid;
        }
    }
    if low == groups {
        return None;
    }
    let group = at + 16 + low * 12;
    let (start, first) = (u32_at(data, group)?, u32_at(data, group + 8)?);
    if code < start {
        return None;
    }
    u16::try_from(first.checked_add(code - start)?).ok()
}

fn simple_glyph(body: &[u8], contours: usize) -> Option<Outline> {
    let mut at = 10;
    let mut ends = Vec::with_capacity(contours);
    for _ in 0..contours {
        ends.push(usize::from(u16_at(body, at)?));
        at += 2;
    }
    let points = ends.last().map_or(0, |e| e + 1);
    if points > MAX_POINTS || ends.windows(2).any(|w| w[1] < w[0]) {
        return None;
    }
    at += 2 + usize::from(u16_at(body, at)?); // the instructions are skipped
    let mut flags = Vec::with_capacity(points);
    while flags.len() < points {
        let flag = *body.get(at)?;
        at += 1;
        flags.push(flag);
        if flag & 0x08 != 0 {
            let repeats = usize::from(*body.get(at)?);
            at += 1;
            for _ in 0..repeats.min(points - flags.len()) {
                flags.push(flag);
            }
        }
    }
    let mut coordinates = |short: u8, same: u8| -> Option<Vec<f32>> {
        let mut values = Vec::with_capacity(points);
        let mut value = 0i32;
        for flag in &flags {
            if flag & short != 0 {
                let delta = i32::from(*body.get(at)?);
                at += 1;
                value += if flag & same != 0 { delta } else { -delta };
            } else if flag & same == 0 {
                value += i32::from(i16_at(body, at)?);
                at += 2;
            }
            values.push(value as f32);
        }
        Some(values)
    };
    let xs = coordinates(0x02, 0x10)?;
    let ys = coordinates(0x04, 0x20)?;
    let mut outline = Outline::default();
    let mut first = 0;
    for end in ends {
        outline.contours.push(
            (first..=end)
                .map(|i| Point {
                    x: xs[i],
                    y: ys[i],
                    on: flags[i] & 0x01 != 0,
                })
                .collect(),
        );
        first = end + 1;
    }
    Some(outline)
}

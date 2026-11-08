//! Boxes and glyphs: the pieces layout builds formulas from.
//!
//! A [`Bx`] is TeX's box: a width, a height above the baseline and a depth below it, with the items drawn in it
//! relative to its origin (left end of the baseline). Glyph boxes come from the font's metrics; a delimiter, a
//! radical sign or a wide accent is grown from the font's `MathVariants`: the first variant big enough, else the
//! glyph assembly stretched to the size wanted, else the largest variant.

use quench_raster::Color;
use quench_text::{GlyphAssembly, Math, MathConstants};

use super::{Error, Face, Fonts, Item};

/// A box: its metrics, in font units at the formula's size, and what is drawn in it.
#[derive(Debug, Clone, Default)]
pub struct Bx {
    pub width: f32,
    pub height: f32,
    pub depth: f32,
    /// The italic correction: how far the ink leans out past the width at the top.
    pub italic: f32,
    pub items: Vec<Item>,
}

impl Bx {
    /// An empty box `width` wide (a kern, a null delimiter).
    pub const fn space(width: f32) -> Self {
        Self {
            width,
            height: 0.0,
            depth: 0.0,
            italic: 0.0,
            items: Vec::new(),
        }
    }

    /// Puts `other` with its origin at `(x, y)`; the height and depth grow to hold it, the width does not change.
    pub fn add(&mut self, other: Self, x: f32, y: f32) {
        self.height = self.height.max(other.height + y);
        self.depth = self.depth.max(other.depth - y);
        self.items
            .extend(other.items.into_iter().map(|item| item.moved(x, y)));
    }

    /// Draws a rule with its bottom left corner at `(x, y)`; the height and depth grow to hold it.
    pub fn rule(&mut self, x: f32, y: f32, width: f32, height: f32, colour: Option<Color>) {
        self.height = self.height.max(y + height);
        self.depth = self.depth.max(-y);
        self.items.push(Item::Rule {
            x,
            y,
            width,
            height,
            colour,
        });
    }

    /// The box moved up by `dy` (down when negative), as one does to centre a delimiter on the axis.
    pub fn shifted(mut self, dy: f32) -> Self {
        self.height += dy;
        self.depth -= dy;
        for item in &mut self.items {
            *item = item.moved(0.0, dy);
        }
        self
    }
}

/// The ink extent of `items`: `(x_min, y_min, x_max, y_max)`.
pub fn ink(fonts: &Fonts, items: &[Item]) -> Option<(f32, f32, f32, f32)> {
    let mut extent: Option<(f32, f32, f32, f32)> = None;
    let mut grow = |x0: f32, y0: f32, x1: f32, y1: f32| {
        extent = Some(match extent {
            None => (x0, y0, x1, y1),
            Some((a, b, c, d)) => (a.min(x0), b.min(y0), c.max(x1), d.max(y1)),
        });
    };
    for item in items {
        match *item {
            Item::Glyph {
                font,
                glyph,
                x,
                y,
                size,
                ..
            } => {
                let face = fonts.face(font);
                if let Some(b) = face.bounds(glyph) {
                    let k = size / f32::from(face.units_per_em());
                    let slant = font.slant();
                    let (y0, y1) = (f32::from(b.y_min) * k, f32::from(b.y_max) * k);
                    grow(
                        x + f32::from(b.x_min) * k + slant * y0.min(0.0),
                        y + y0,
                        x + f32::from(b.x_max) * k + slant * y1.max(0.0),
                        y + y1,
                    );
                }
            }
            Item::Rule {
                x,
                y,
                width,
                height,
                ..
            } => grow(x, y, x + width, y + height),
        }
    }
    extent
}

/// What is known about a box that is one glyph (scripts, accents and italic corrections look at it).
#[derive(Debug, Clone, Copy)]
pub struct GlyphInfo {
    /// The glyph is an extended shape (a large operator, a grown delimiter): scripts treat it as a box.
    pub extended: bool,
    /// Where an accent goes, x from the origin.
    pub attach: Option<f32>,
}

/// The fonts and the math table, for building glyph boxes.
#[derive(Debug)]
pub struct Engine<'a> {
    pub fonts: &'a Fonts,
    pub math: &'a Math,
    pub k: MathConstants,
    /// Font units per em of the math font, as `f32`.
    pub em: f32,
}

impl<'a> Engine<'a> {
    pub fn new(fonts: &'a Fonts) -> Result<Self, Error> {
        let math = fonts.math.math().ok_or(Error::NotMathFont)?;
        let k = *math.constants().ok_or(Error::NotMathFont)?;
        Ok(Self {
            fonts,
            math,
            k,
            em: f32::from(fonts.math.units_per_em()),
        })
    }

    /// The math font's glyph for `ch`, or the error naming it.
    pub fn glyph_of(&self, ch: char) -> Result<u16, Error> {
        self.fonts.math.glyph(ch).ok_or(Error::MissingGlyph(ch))
    }

    /// A box of one glyph of `face` at `scale` (1 at text size), with its italic correction when it is from the
    /// math font.
    pub fn glyph_box(&self, face: Face, glyph: u16, scale: f32, colour: Option<Color>) -> Bx {
        let font = self.fonts.face(face);
        // Glyph positions are in math font units; a text face with another em is scaled to it.
        let k = scale * self.em / f32::from(font.units_per_em());
        let (height, depth) = font.bounds(glyph).map_or((0.0, 0.0), |b| {
            (
                (f32::from(b.y_max) * k).max(0.0),
                (-f32::from(b.y_min) * k).max(0.0),
            )
        });
        let italic = if face == Face::Math {
            self.math
                .italics_correction(glyph)
                .map_or(0.0, |v| f32::from(v) * k)
        } else {
            0.0
        };
        Bx {
            width: f32::from(font.advance(glyph)) * k,
            height,
            depth,
            italic,
            items: vec![Item::Glyph {
                font: face,
                glyph,
                x: 0.0,
                y: 0.0,
                size: self.em * scale,
                colour,
            }],
        }
    }

    /// What a math glyph tells scripts and accents.
    pub fn glyph_info(&self, glyph: u16, scale: f32) -> GlyphInfo {
        GlyphInfo {
            extended: self.math.is_extended_shape(glyph),
            attach: self
                .math
                .top_accent_attachment(glyph)
                .map(|v| f32::from(v) * scale),
        }
    }

    /// Where an accent is centred over `glyph`: its top accent attachment, else the middle of its ink.
    pub fn accent_attach(&self, glyph: u16, scale: f32, advance: f32) -> f32 {
        if let Some(v) = self.math.top_accent_attachment(glyph) {
            return f32::from(v) * scale;
        }
        self.fonts.math.bounds(glyph).map_or(advance / 2.0, |b| {
            f32::midpoint(f32::from(b.x_min), f32::from(b.x_max)) * scale
        })
    }

    /// `ch` grown vertically to at least `size` (height plus depth) at `scale`: a variant, else the assembly,
    /// else the largest variant. The box is where the font puts it (baseline at the glyph's origin).
    pub fn vertical(
        &self,
        ch: char,
        size: f32,
        scale: f32,
        colour: Option<Color>,
    ) -> Result<Bx, Error> {
        let glyph = self.glyph_of(ch)?;
        let Some(construction) = self.math.vertical_construction(glyph) else {
            return Ok(self.glyph_box(Face::Math, glyph, scale, colour));
        };
        if let Some(v) = construction
            .variants
            .iter()
            .find(|v| f32::from(v.advance) * scale >= size)
        {
            return Ok(self.glyph_box(Face::Math, v.glyph, scale, colour));
        }
        if let Some(assembly) = &construction.assembly {
            return Ok(self.assemble(assembly, size / scale, scale, true, colour));
        }
        let glyph = construction.variants.last().map_or(glyph, |v| v.glyph);
        Ok(self.glyph_box(Face::Math, glyph, scale, colour))
    }

    /// `ch` grown horizontally to `width` at `scale`, with where an accent made of it attaches (the middle of an
    /// assembly). With `cover` (braces, arrows) it is at least that wide; without (`\widehat`, as TeX's rule 12
    /// chooses accents) it is the widest variant no wider than that, or the narrowest, the assembly only past the
    /// widest variant.
    pub fn horizontal(
        &self,
        ch: char,
        width: f32,
        scale: f32,
        cover: bool,
        colour: Option<Color>,
    ) -> Result<(Bx, f32), Error> {
        let glyph = self.glyph_of(ch)?;
        let single = |g: u16| {
            let bx = self.glyph_box(Face::Math, g, scale, colour);
            let ink = self.fonts.math.bounds(g).map_or(bx.width, |b| {
                (f32::from(b.x_max) - f32::from(b.x_min)) * scale
            });
            let attach = self.accent_attach(g, scale, ink);
            (bx, attach)
        };
        let Some(construction) = self.math.horizontal_construction(glyph) else {
            return Ok(single(glyph));
        };
        let widest = construction
            .variants
            .last()
            .map_or(0.0, |v| f32::from(v.advance) * scale);
        if !cover
            && (widest >= width || construction.assembly.is_none())
            && let Some(v) = construction
                .variants
                .iter()
                .take_while(|v| f32::from(v.advance) * scale <= width)
                .last()
                .or_else(|| construction.variants.first())
        {
            return Ok(single(v.glyph));
        }
        if let Some(v) = construction
            .variants
            .iter()
            .find(|v| f32::from(v.advance) * scale >= width)
        {
            return Ok(single(v.glyph));
        }
        if let Some(assembly) = &construction.assembly {
            let bx = self.assemble(assembly, width / scale, scale, false, colour);
            let attach = bx.width / 2.0;
            return Ok((bx, attach));
        }
        Ok(single(
            construction.variants.last().map_or(glyph, |v| v.glyph),
        ))
    }

    /// The parts of `assembly` put together to `target` font units (unscaled) or just past it, as the OpenType
    /// specification describes: extenders repeated as often as needed, overlaps between neighbours at least the
    /// font's minimum connector overlap and at most the connectors (clamped to the parts' lengths), spread
    /// evenly. Vertical parts go bottom to top from the baseline, horizontal ones left to right from the origin.
    fn assemble(
        &self,
        assembly: &GlyphAssembly,
        target: f32,
        scale: f32,
        vertical: bool,
        colour: Option<Color>,
    ) -> Bx {
        let overlap = f32::from(self.math.min_connector_overlap());
        let build = |repeats: usize| {
            let mut parts = Vec::new();
            for part in &assembly.parts {
                let count = if part.extender { repeats } else { 1 };
                parts.extend(std::iter::repeat_n(*part, count));
            }
            parts
        };
        // Joints: the least and the most two neighbours may overlap.
        let joints = |parts: &[quench_text::GlyphPart]| -> Vec<(f32, f32)> {
            parts
                .windows(2)
                .map(|pair| {
                    let most = f32::from(
                        pair[0]
                            .end_connector_length
                            .min(pair[1].start_connector_length)
                            .min(pair[0].full_advance)
                            .min(pair[1].full_advance),
                    );
                    (overlap.min(most), most)
                })
                .collect()
        };
        let length = |parts: &[quench_text::GlyphPart]| -> f32 {
            parts.iter().map(|p| f32::from(p.full_advance)).sum()
        };
        let has_extender = assembly
            .parts
            .iter()
            .any(|p| p.extender && p.full_advance > 0);
        let mut repeats = 0;
        let mut parts = build(repeats);
        while has_extender && repeats < 1000 {
            let longest = length(&parts) - joints(&parts).iter().map(|j| j.0).sum::<f32>();
            if longest >= target {
                break;
            }
            repeats += 1;
            parts = build(repeats);
        }
        let joints = joints(&parts);
        let longest = length(&parts) - joints.iter().map(|j| j.0).sum::<f32>();
        let slack: f32 = joints.iter().map(|j| j.1 - j.0).sum();
        let lambda = if slack > 0.0 {
            ((longest - target) / slack).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let mut bx = Bx::default();
        let mut at = 0.0;
        for (i, part) in parts.iter().enumerate() {
            let mut piece = self.glyph_box(Face::Math, part.glyph, scale, colour);
            piece.italic = 0.0;
            let (breadth, ink_low, ink_high) = (piece.width, piece.depth, piece.height);
            if vertical {
                bx.width = bx.width.max(breadth);
                bx.add(piece, 0.0, at * scale);
            } else {
                bx.height = bx.height.max(ink_high);
                bx.depth = bx.depth.max(ink_low);
                bx.items.extend(
                    piece
                        .items
                        .into_iter()
                        .map(|item| item.moved(at * scale, 0.0)),
                );
            }
            at += f32::from(part.full_advance);
            if let Some((least, most)) = joints.get(i) {
                at -= least + lambda * (most - least);
            }
        }
        if !vertical {
            bx.width = at * scale;
        }
        bx.italic = f32::from(assembly.italics_correction) * scale;
        bx
    }
}

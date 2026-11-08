//! The OpenType `MATH` table: what a math typesetter needs from a font beyond its glyphs.
//!
//! TeX's layout rules for formulas take their distances from font parameters; an OpenType math font carries
//! them in `MATH`, with three subtables:
//!
//! * `MathConstants`: the ~56 distances and percentages ([`MathConstants`]);
//! * `MathGlyphInfo`: per glyph, the italic correction, the top accent attachment, whether it is an extended
//!   shape, and the kerns at its four corners ([`MathKernInfo`]);
//! * `MathVariants`: per glyph, larger versions of it, vertical or horizontal, and the assembly of parts that
//!   grows it to any size ([`GlyphConstruction`]).
//!
//! The table is read once into owned values. Every read is bounds-checked against the table's own length: a
//! damaged subtable reads as absent or empty, never as a panic. Device tables (pixel-size corrections) are not
//! read. All values are in font units; y points up.

use crate::font::{i16_at, u16_at};

/// The most glyphs a coverage table may name, whatever the file says (glyph ids are 16 bits).
const MAX_COVERED: usize = 65_536;

/// The `MathConstants` subtable: every field, named as the specification names it, in font units (percentages
/// are plain numbers: 60 means 60 %). Device-table corrections are ignored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MathConstants {
    pub script_percent_scale_down: i16,
    pub script_script_percent_scale_down: i16,
    pub delimited_sub_formula_min_height: u16,
    pub display_operator_min_height: u16,
    pub math_leading: i16,
    pub axis_height: i16,
    pub accent_base_height: i16,
    pub flattened_accent_base_height: i16,
    pub subscript_shift_down: i16,
    pub subscript_top_max: i16,
    pub subscript_baseline_drop_min: i16,
    pub superscript_shift_up: i16,
    pub superscript_shift_up_cramped: i16,
    pub superscript_bottom_min: i16,
    pub superscript_baseline_drop_max: i16,
    pub sub_superscript_gap_min: i16,
    pub superscript_bottom_max_with_subscript: i16,
    pub space_after_script: i16,
    pub upper_limit_gap_min: i16,
    pub upper_limit_baseline_rise_min: i16,
    pub lower_limit_gap_min: i16,
    pub lower_limit_baseline_drop_min: i16,
    pub stack_top_shift_up: i16,
    pub stack_top_display_style_shift_up: i16,
    pub stack_bottom_shift_down: i16,
    pub stack_bottom_display_style_shift_down: i16,
    pub stack_gap_min: i16,
    pub stack_display_style_gap_min: i16,
    pub stretch_stack_top_shift_up: i16,
    pub stretch_stack_bottom_shift_down: i16,
    pub stretch_stack_gap_above_min: i16,
    pub stretch_stack_gap_below_min: i16,
    pub fraction_numerator_shift_up: i16,
    pub fraction_numerator_display_style_shift_up: i16,
    pub fraction_denominator_shift_down: i16,
    pub fraction_denominator_display_style_shift_down: i16,
    pub fraction_numerator_gap_min: i16,
    pub fraction_num_display_style_gap_min: i16,
    pub fraction_rule_thickness: i16,
    pub fraction_denominator_gap_min: i16,
    pub fraction_denom_display_style_gap_min: i16,
    pub skewed_fraction_horizontal_gap: i16,
    pub skewed_fraction_vertical_gap: i16,
    pub overbar_vertical_gap: i16,
    pub overbar_rule_thickness: i16,
    pub overbar_extra_ascender: i16,
    pub underbar_vertical_gap: i16,
    pub underbar_rule_thickness: i16,
    pub underbar_extra_descender: i16,
    pub radical_vertical_gap: i16,
    pub radical_display_style_vertical_gap: i16,
    pub radical_rule_thickness: i16,
    pub radical_extra_ascender: i16,
    pub radical_kern_before_degree: i16,
    pub radical_kern_after_degree: i16,
    pub radical_degree_bottom_raise_percent: i16,
}

/// A corner of a glyph, for [`Math::kern`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MathKernCorner {
    TopRight,
    TopLeft,
    BottomRight,
    BottomLeft,
}

/// The kern at one corner of a glyph, by height bands.
///
/// `kern_values[i]` applies between `correction_heights[i - 1]` and `correction_heights[i]`, the first below the
/// lowest height, the last above the highest. A corner the font says nothing about has no values and kerns by 0.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MathKern {
    /// Heights (y, font units) at which the kern changes, ascending.
    pub correction_heights: Vec<i16>,
    /// One more than the heights (font units; negative moves the script closer).
    pub kern_values: Vec<i16>,
}

impl MathKern {
    /// The kern at `height`: the value of the band it falls in. A height equal to a correction height belongs to
    /// the band below it (as `HarfBuzz` reads the specification).
    #[must_use]
    pub fn at(&self, height: i16) -> i16 {
        let band = self.correction_heights.partition_point(|&h| h < height);
        self.kern_values.get(band).copied().unwrap_or(0)
    }
}

/// The math kerns of a glyph at its four corners.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MathKernInfo {
    pub top_right: MathKern,
    pub top_left: MathKern,
    pub bottom_right: MathKern,
    pub bottom_left: MathKern,
}

impl MathKernInfo {
    /// The kern of one corner.
    #[must_use]
    pub const fn corner(&self, corner: MathKernCorner) -> &MathKern {
        match corner {
            MathKernCorner::TopRight => &self.top_right,
            MathKernCorner::TopLeft => &self.top_left,
            MathKernCorner::BottomRight => &self.bottom_right,
            MathKernCorner::BottomLeft => &self.bottom_left,
        }
    }
}

/// A larger version of a glyph, and its size along the direction it grows (height plus depth for a vertical
/// variant, width for a horizontal one), in font units.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GlyphVariant {
    pub glyph: u16,
    pub advance: u16,
}

/// A part of a [`GlyphAssembly`], in font units along the direction of growth. Parts are listed bottom to top
/// (vertical) or left to right (horizontal).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GlyphPart {
    pub glyph: u16,
    /// How much of the part's start may overlap the previous part.
    pub start_connector_length: u16,
    /// How much of the part's end may overlap the next part.
    pub end_connector_length: u16,
    /// The part's whole length.
    pub full_advance: u16,
    /// The part may be repeated (or left out) to reach the size wanted.
    pub extender: bool,
}

/// How to build a glyph of any size from parts.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GlyphAssembly {
    /// The italic correction of the assembled glyph.
    pub italics_correction: i16,
    pub parts: Vec<GlyphPart>,
}

/// The ways one glyph grows in one direction: its variants, smallest first (the first is usually the glyph
/// itself), and an assembly for sizes past the largest.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GlyphConstruction {
    pub variants: Vec<GlyphVariant>,
    pub assembly: Option<GlyphAssembly>,
}

/// A font's `MATH` table, read.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Math {
    constants: Option<MathConstants>,
    italics: Vec<(u16, i16)>,
    top_accents: Vec<(u16, i16)>,
    extended: Vec<u16>,
    kerns: Vec<(u16, MathKernInfo)>,
    min_connector_overlap: u16,
    vertical: Vec<(u16, GlyphConstruction)>,
    horizontal: Vec<(u16, GlyphConstruction)>,
}

/// The value at `glyph` in a list sorted by glyph.
fn find<T>(list: &[(u16, T)], glyph: u16) -> Option<&T> {
    list.binary_search_by_key(&glyph, |e| e.0)
        .ok()
        .map(|i| &list[i].1)
}

impl Math {
    /// Reads the `MATH` table `table` (the table's bytes alone). `None` when its header cannot be read or its
    /// version is not 1; a damaged subtable is left out (absent constants, no entries).
    #[must_use]
    pub fn parse(table: &[u8]) -> Option<Self> {
        // Version 1.x, and the three subtable offsets there to be read.
        if u16_at(table, 0)? != 1 || table.len() < 10 {
            return None;
        }
        let sub = |at: usize| offset(table, 0, at);
        let mut math = Self {
            constants: sub(4).and_then(|at| constants(table, at)),
            ..Self::default()
        };
        if let Some(info) = sub(6) {
            let list = |at| offset(table, info, at);
            math.italics = list(0).and_then(|at| values(table, at)).unwrap_or_default();
            math.top_accents = list(2).and_then(|at| values(table, at)).unwrap_or_default();
            math.extended = list(4)
                .and_then(|at| coverage(table, at))
                .map(|covered| {
                    let mut glyphs: Vec<u16> = covered.into_iter().map(|e| e.0).collect();
                    glyphs.sort_unstable();
                    glyphs.dedup();
                    glyphs
                })
                .unwrap_or_default();
            math.kerns = list(6)
                .and_then(|at| kern_infos(table, at))
                .unwrap_or_default();
        }
        if let Some(variants) = sub(8) {
            math.min_connector_overlap = u16_at(table, variants).unwrap_or(0);
            // Each direction is read on its own: damage on one side leaves the other.
            let (vertical, horizontal) = constructions(table, variants);
            math.vertical = vertical.unwrap_or_default();
            math.horizontal = horizontal.unwrap_or_default();
        }
        Some(math)
    }

    /// The constants, `None` when the font has none (or they are cut short).
    #[must_use]
    pub const fn constants(&self) -> Option<&MathConstants> {
        self.constants.as_ref()
    }

    /// The italic correction of `glyph`: how far a superscript after it moves right, in font units.
    #[must_use]
    pub fn italics_correction(&self, glyph: u16) -> Option<i16> {
        find(&self.italics, glyph).copied()
    }

    /// Where an accent over `glyph` is centred, as x from the glyph's origin in font units. Without one, the
    /// specification says to centre on the glyph's ink (half its bounding box width, plus its left side).
    #[must_use]
    pub fn top_accent_attachment(&self, glyph: u16) -> Option<i16> {
        find(&self.top_accents, glyph).copied()
    }

    /// Whether `glyph` is an extended shape (a tall delimiter or large operator: the base of a script is placed
    /// by its ink rather than by the script rules for ordinary glyphs).
    #[must_use]
    pub fn is_extended_shape(&self, glyph: u16) -> bool {
        self.extended.binary_search(&glyph).is_ok()
    }

    /// The four corner kerns of `glyph`, when the font has them.
    #[must_use]
    pub fn kern_info(&self, glyph: u16) -> Option<&MathKernInfo> {
        find(&self.kerns, glyph)
    }

    /// The kern at `corner` of `glyph` at `height` (font units, y up); 0 when the font has none.
    #[must_use]
    pub fn kern(&self, glyph: u16, corner: MathKernCorner, height: i16) -> i16 {
        self.kern_info(glyph)
            .map_or(0, |info| info.corner(corner).at(height))
    }

    /// The glyphs that have corner kerns, ascending.
    pub fn kerned_glyphs(&self) -> impl Iterator<Item = u16> + '_ {
        self.kerns.iter().map(|e| e.0)
    }

    /// The least overlap between two parts of an assembly, in font units.
    #[must_use]
    pub const fn min_connector_overlap(&self) -> u16 {
        self.min_connector_overlap
    }

    /// How `glyph` grows taller (delimiters, radicals, integrals).
    #[must_use]
    pub fn vertical_construction(&self, glyph: u16) -> Option<&GlyphConstruction> {
        find(&self.vertical, glyph)
    }

    /// How `glyph` grows wider (accents, braces, arrows).
    #[must_use]
    pub fn horizontal_construction(&self, glyph: u16) -> Option<&GlyphConstruction> {
        find(&self.horizontal, glyph)
    }

    /// The glyphs with a vertical construction, ascending.
    pub fn vertical_glyphs(&self) -> impl Iterator<Item = u16> + '_ {
        self.vertical.iter().map(|e| e.0)
    }

    /// The glyphs with a horizontal construction, ascending.
    pub fn horizontal_glyphs(&self) -> impl Iterator<Item = u16> + '_ {
        self.horizontal.iter().map(|e| e.0)
    }
}

/// The table an `Offset16` at `base + at` points to, measured from `base`; `None` for a null offset.
fn offset(data: &[u8], base: usize, at: usize) -> Option<usize> {
    let value = u16_at(data, base.checked_add(at)?)?;
    (value != 0).then(|| base + usize::from(value))
}

/// Entries sorted by glyph, the first of each glyph kept.
fn sorted<T>(entries: impl Iterator<Item = (u16, T)>) -> Vec<(u16, T)> {
    let mut list: Vec<_> = entries.collect();
    list.sort_by_key(|e| e.0);
    list.dedup_by_key(|e| e.0);
    list
}

/// A sequential reader of big-endian fields.
struct Reader<'a> {
    data: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn u16(&mut self) -> Option<u16> {
        let v = u16_at(self.data, self.at)?;
        self.at += 2;
        Some(v)
    }

    fn i16(&mut self) -> Option<i16> {
        self.u16().map(|v| v as i16)
    }

    /// A `MathValueRecord`: the value, its device offset skipped.
    fn value(&mut self) -> Option<i16> {
        let v = self.i16()?;
        self.u16()?;
        Some(v)
    }
}

fn constants(data: &[u8], at: usize) -> Option<MathConstants> {
    let mut r = Reader { data, at };
    // Struct fields are evaluated in the order written, which is the order of the table.
    Some(MathConstants {
        script_percent_scale_down: r.i16()?,
        script_script_percent_scale_down: r.i16()?,
        delimited_sub_formula_min_height: r.u16()?,
        display_operator_min_height: r.u16()?,
        math_leading: r.value()?,
        axis_height: r.value()?,
        accent_base_height: r.value()?,
        flattened_accent_base_height: r.value()?,
        subscript_shift_down: r.value()?,
        subscript_top_max: r.value()?,
        subscript_baseline_drop_min: r.value()?,
        superscript_shift_up: r.value()?,
        superscript_shift_up_cramped: r.value()?,
        superscript_bottom_min: r.value()?,
        superscript_baseline_drop_max: r.value()?,
        sub_superscript_gap_min: r.value()?,
        superscript_bottom_max_with_subscript: r.value()?,
        space_after_script: r.value()?,
        upper_limit_gap_min: r.value()?,
        upper_limit_baseline_rise_min: r.value()?,
        lower_limit_gap_min: r.value()?,
        lower_limit_baseline_drop_min: r.value()?,
        stack_top_shift_up: r.value()?,
        stack_top_display_style_shift_up: r.value()?,
        stack_bottom_shift_down: r.value()?,
        stack_bottom_display_style_shift_down: r.value()?,
        stack_gap_min: r.value()?,
        stack_display_style_gap_min: r.value()?,
        stretch_stack_top_shift_up: r.value()?,
        stretch_stack_bottom_shift_down: r.value()?,
        stretch_stack_gap_above_min: r.value()?,
        stretch_stack_gap_below_min: r.value()?,
        fraction_numerator_shift_up: r.value()?,
        fraction_numerator_display_style_shift_up: r.value()?,
        fraction_denominator_shift_down: r.value()?,
        fraction_denominator_display_style_shift_down: r.value()?,
        fraction_numerator_gap_min: r.value()?,
        fraction_num_display_style_gap_min: r.value()?,
        fraction_rule_thickness: r.value()?,
        fraction_denominator_gap_min: r.value()?,
        fraction_denom_display_style_gap_min: r.value()?,
        skewed_fraction_horizontal_gap: r.value()?,
        skewed_fraction_vertical_gap: r.value()?,
        overbar_vertical_gap: r.value()?,
        overbar_rule_thickness: r.value()?,
        overbar_extra_ascender: r.value()?,
        underbar_vertical_gap: r.value()?,
        underbar_rule_thickness: r.value()?,
        underbar_extra_descender: r.value()?,
        radical_vertical_gap: r.value()?,
        radical_display_style_vertical_gap: r.value()?,
        radical_rule_thickness: r.value()?,
        radical_extra_ascender: r.value()?,
        radical_kern_before_degree: r.value()?,
        radical_kern_after_degree: r.value()?,
        radical_degree_bottom_raise_percent: r.i16()?,
    })
}

/// The glyphs of the coverage table at `at` with their coverage indices (formats 1 and 2).
fn coverage(data: &[u8], at: usize) -> Option<Vec<(u16, usize)>> {
    let count = usize::from(u16_at(data, at + 2)?);
    let mut covered = Vec::new();
    match u16_at(data, at)? {
        1 => {
            for i in 0..count {
                covered.push((u16_at(data, at + 4 + i * 2)?, i));
            }
        }
        2 => {
            for i in 0..count {
                let record = at + 4 + i * 6;
                let (start, end, first) = (
                    u16_at(data, record)?,
                    u16_at(data, record + 2)?,
                    usize::from(u16_at(data, record + 4)?),
                );
                if end < start || covered.len() + usize::from(end - start) >= MAX_COVERED {
                    return None;
                }
                covered.extend((start..=end).map(|g| (g, first + usize::from(g - start))));
            }
        }
        _ => return None,
    }
    Some(covered)
}

/// The records of a table laid out as a coverage offset, a count, and `count` records of `size` bytes: each
/// covered glyph with the position of its record. A coverage index past the count is left out.
fn records(data: &[u8], at: usize, size: usize) -> Option<Vec<(u16, usize)>> {
    let covered = coverage(data, offset(data, at, 0)?)?;
    let count = usize::from(u16_at(data, at + 2)?);
    let first = at + 4;
    // The last record must be inside the table, so every one read later is.
    data.get(first..first + count * size)?;
    Some(
        covered
            .into_iter()
            .filter(|&(_, index)| index < count)
            .map(|(glyph, index)| (glyph, first + index * size))
            .collect(),
    )
}

/// A `MathItalicsCorrectionInfo` or `MathTopAccentAttachment` table: one value record per covered glyph.
fn values(data: &[u8], at: usize) -> Option<Vec<(u16, i16)>> {
    let entries = records(data, at, 4)?
        .into_iter()
        .map(|(glyph, record)| Some((glyph, i16_at(data, record)?)))
        .collect::<Option<Vec<_>>>()?;
    Some(sorted(entries.into_iter()))
}

fn kern_infos(data: &[u8], at: usize) -> Option<Vec<(u16, MathKernInfo)>> {
    let entries = records(data, at, 8)?
        .into_iter()
        .map(|(glyph, record)| {
            let corner = |i: usize| {
                offset(data, at, record - at + i * 2).map_or_else(
                    || Some(MathKern::default()),
                    |kern_at| math_kern(data, kern_at),
                )
            };
            Some((
                glyph,
                MathKernInfo {
                    top_right: corner(0)?,
                    top_left: corner(1)?,
                    bottom_right: corner(2)?,
                    bottom_left: corner(3)?,
                },
            ))
        })
        .collect::<Option<Vec<_>>>()?;
    Some(sorted(entries.into_iter()))
}

fn math_kern(data: &[u8], at: usize) -> Option<MathKern> {
    let count = usize::from(u16_at(data, at)?);
    let mut r = Reader { data, at: at + 2 };
    let correction_heights = (0..count).map(|_| r.value()).collect::<Option<Vec<_>>>()?;
    let kern_values = (0..=count).map(|_| r.value()).collect::<Option<Vec<_>>>()?;
    Some(MathKern {
        correction_heights,
        kern_values,
    })
}

type Constructions = Vec<(u16, GlyphConstruction)>;

/// The vertical and horizontal constructions of the `MathVariants` table at `at`, each `None` when it cannot be
/// read. The horizontal offsets follow the vertical ones, so they need the vertical count, not the vertical
/// constructions.
fn constructions(data: &[u8], at: usize) -> (Option<Constructions>, Option<Constructions>) {
    let (Some(vertical_count), Some(horizontal_count)) =
        (u16_at(data, at + 6), u16_at(data, at + 8))
    else {
        return (None, None);
    };
    let (vertical_count, horizontal_count) =
        (usize::from(vertical_count), usize::from(horizontal_count));
    let read = |coverage_field: usize, offsets: usize, count: usize| -> Option<Constructions> {
        let Some(coverage_at) = offset(data, at, coverage_field) else {
            return Some(Vec::new());
        };
        let entries = coverage(data, coverage_at)?
            .into_iter()
            .filter(|&(_, index)| index < count)
            .map(|(glyph, index)| {
                let construction_at = offset(data, at, offsets + index * 2)?;
                Some((glyph, construction(data, construction_at)?))
            })
            .collect::<Option<Vec<_>>>()?;
        Some(sorted(entries.into_iter()))
    };
    let vertical = read(2, 10, vertical_count);
    let horizontal = read(4, 10 + vertical_count * 2, horizontal_count);
    (vertical, horizontal)
}

fn construction(data: &[u8], at: usize) -> Option<GlyphConstruction> {
    let count = usize::from(u16_at(data, at + 2)?);
    let mut r = Reader { data, at: at + 4 };
    let variants = (0..count)
        .map(|_| {
            Some(GlyphVariant {
                glyph: r.u16()?,
                advance: r.u16()?,
            })
        })
        .collect::<Option<Vec<_>>>()?;
    let assembly = match offset(data, at, 0) {
        Some(assembly_at) => Some(assembly(data, assembly_at)?),
        None => None,
    };
    Some(GlyphConstruction { variants, assembly })
}

fn assembly(data: &[u8], at: usize) -> Option<GlyphAssembly> {
    let mut r = Reader { data, at };
    let italics_correction = r.value()?;
    let count = usize::from(r.u16()?);
    let parts = (0..count)
        .map(|_| {
            Some(GlyphPart {
                glyph: r.u16()?,
                start_connector_length: r.u16()?,
                end_connector_length: r.u16()?,
                full_advance: r.u16()?,
                extender: r.u16()? & 0x0001 != 0,
            })
        })
        .collect::<Option<Vec<_>>>()?;
    Some(GlyphAssembly {
        italics_correction,
        parts,
    })
}

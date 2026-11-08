//! The typesetter: a parsed [`Formula`] laid out by TeX's rules with an OpenType `MATH` font, and drawn to PNG
//! or SVG. Only with the `typeset` feature.
//!
//! Layout follows *The `TeXbook`*, Appendix G, with the font's `MATH` constants in place of TeX's font parameters,
//! the way the OpenType MATH specification and `MathML` Core describe it: the four styles and their cramped forms,
//! the spacing table between atom classes, fractions, radicals, scripts, large operators with limits, delimiters
//! grown from the font's variants and assemblies, accents, braces, stacks and arrays. The result is a display
//! list ([`Typeset`]): glyphs and rules at positions in font units, with the baseline at `y = 0` and y pointing
//! up, independent of how it is drawn. [`to_png`], [`to_rgba`] and [`to_svg`] draw it.
//!
//! The font is Noto Sans Math, embedded by `quench-text` ([`Fonts::bundled`]); `\text` is set in Noto Sans. A
//! character the font has no glyph for is an [`Error`] naming it: a formula is typeset whole or not at all.
//!
//! Simplifications: the font's math kerns (`MathKernInfo`) are not used, scripts are placed with italic
//! corrections alone; the constants `MathLeading`, `DelimitedSubFormulaMinHeight`,
//! `FlattenedAccentBaseHeight`, `StretchStackTopShiftUp`, `StretchStackBottomShiftDown` and the two skewed
//! fraction gaps are not used; `\cfrac` is set as `\dfrac`; primes are set at the size of their base; script
//! glyphs are scaled outlines (no `ssty`); `\textit` is Noto Sans slanted.
//!
//! ```
//! use quench_tex::typeset::{RenderOptions, SvgOptions, tex_to_png, tex_to_svg, typeset};
//!
//! let formula = quench_tex::parse(r"\frac{a}{b}")?;
//! let set = typeset(&formula, true)?;
//! assert!(set.ascent > 278.0 && set.descent > 0.0);
//!
//! let png = tex_to_png(r"\sqrt{x^2+1}", false, &RenderOptions::default())?;
//! assert_eq!(&png[1..4], b"PNG");
//! let svg = tex_to_svg(r"e^{i\pi}", false, &SvgOptions::default())?;
//! assert!(svg.starts_with("<svg"));
//! # Ok::<(), quench_tex::typeset::Error>(())
//! ```

// Layout arithmetic: font units and pixels are small numbers held in `f32`, glyph ids 16 bits; every cast is of
// such a value, and the formulas read as the specifications write them.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::suboptimal_flops,
    clippy::similar_names,
    clippy::many_single_char_names,
    clippy::module_name_repetitions
)]

mod array;
mod boxes;
mod colour;
mod draw;
mod layout;
mod svg;

use std::fmt;
use std::sync::Arc;

pub use quench_raster::Color;
pub use quench_text::Font;

pub use draw::{Raster, RenderOptions, to_png, to_rgba};
pub use svg::{SvgOptions, to_svg};

use crate::Formula;

/// Why a formula could not be typeset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The TeX could not be read ([`tex_to_png`], [`tex_to_svg`]).
    Parse(crate::Error),
    /// A character the fonts have no glyph for.
    MissingGlyph(char),
    /// A colour name or value that is not known (`\color{…}`).
    UnknownColour(String),
    /// The math font has no `MATH` table (or no constants in it): it is not a math font.
    NotMathFont,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(error) => error.fmt(f),
            Self::MissingGlyph(ch) => write!(
                f,
                "the math font has no glyph for `{ch}` (U+{:04X})",
                u32::from(*ch)
            ),
            Self::UnknownColour(name) => write!(f, "unknown colour `{name}`"),
            Self::NotMathFont => f.write_str("the font has no MATH table: it is not a math font"),
        }
    }
}

impl std::error::Error for Error {}

impl From<crate::Error> for Error {
    fn from(error: crate::Error) -> Self {
        Self::Parse(error)
    }
}

/// Which font a glyph is drawn from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Face {
    /// The math font.
    Math,
    /// The text font (`\text`).
    Text,
    /// The bold text font (`\textbf`).
    TextBold,
    /// The text font slanted (`\textit`: Noto Sans has no italic embedded, so it leans).
    TextItalic,
}

impl Face {
    /// How far the face leans: every point moves right by this much times its height.
    #[must_use]
    pub const fn slant(self) -> f32 {
        match self {
            Self::TextItalic => 0.2,
            Self::Math | Self::Text | Self::TextBold => 0.0,
        }
    }
}

/// The fonts a formula is set in.
#[derive(Debug, Clone)]
pub struct Fonts {
    /// The math font, with a `MATH` table.
    pub math: Arc<Font>,
    /// The text font, for `\text`.
    pub text: Arc<Font>,
    /// The bold text font, for `\textbf`.
    pub text_bold: Arc<Font>,
}

impl Fonts {
    /// Noto Sans Math with Noto Sans for text, all embedded.
    #[must_use]
    pub fn bundled() -> Self {
        Self {
            math: quench_text::noto_sans_math(),
            text: quench_text::noto_sans(),
            text_bold: quench_text::noto_sans_bold(),
        }
    }

    /// Another math font, with Noto Sans for text.
    ///
    /// # Errors
    /// [`Error::NotMathFont`] when the font has no `MATH` table or no constants in it.
    pub fn with_math(math: Arc<Font>) -> Result<Self, Error> {
        if math.math().and_then(quench_text::Math::constants).is_none() {
            return Err(Error::NotMathFont);
        }
        Ok(Self {
            math,
            ..Self::bundled()
        })
    }

    /// The font of `face`.
    #[must_use]
    pub fn face(&self, face: Face) -> &Font {
        match face {
            Face::Math => &self.math,
            Face::Text | Face::TextItalic => &self.text,
            Face::TextBold => &self.text_bold,
        }
    }
}

/// One thing drawn: a glyph or a filled rectangle. Positions are in font units of the math font, y up, the
/// formula's baseline at `y = 0` and its left edge at `x = 0`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Item {
    /// A glyph with its origin at `(x, y)`.
    Glyph {
        /// The font it is from.
        font: Face,
        /// The glyph id in that font.
        glyph: u16,
        /// The x of the glyph's origin.
        x: f32,
        /// The y of the glyph's baseline.
        y: f32,
        /// The size of the glyph's em in the same units (1000 at text size for a 1000-unit font, 600 in a
        /// script).
        size: f32,
        /// Its colour; `None` for the colour the formula is drawn in.
        colour: Option<Color>,
    },
    /// A rule (fraction bar, radical overbar, array line): a rectangle with its bottom left corner at `(x, y)`.
    Rule {
        /// The left edge.
        x: f32,
        /// The bottom edge.
        y: f32,
        /// The width.
        width: f32,
        /// The height (thickness, for a horizontal rule).
        height: f32,
        /// Its colour; `None` for the colour the formula is drawn in.
        colour: Option<Color>,
    },
}

impl Item {
    /// The item moved by `(dx, dy)`.
    #[must_use]
    pub const fn moved(self, dx: f32, dy: f32) -> Self {
        match self {
            Self::Glyph {
                font,
                glyph,
                x,
                y,
                size,
                colour,
            } => Self::Glyph {
                font,
                glyph,
                x: x + dx,
                y: y + dy,
                size,
                colour,
            },
            Self::Rule {
                x,
                y,
                width,
                height,
                colour,
            } => Self::Rule {
                x: x + dx,
                y: y + dy,
                width,
                height,
                colour,
            },
        }
    }
}

/// A typeset formula: its box and what is drawn in it.
#[derive(Debug, Clone)]
pub struct Typeset {
    /// The advance width, font units.
    pub width: f32,
    /// How far the box reaches above the baseline.
    pub ascent: f32,
    /// How far the box reaches below the baseline (positive).
    pub descent: f32,
    /// Font units per em of the positions (the math font's units per em).
    pub units_per_em: f32,
    /// The glyphs and rules.
    pub items: Vec<Item>,
    /// The fonts the glyphs are from.
    pub fonts: Fonts,
}

impl Typeset {
    /// The extent of the ink, `(x_min, y_min, x_max, y_max)` in font units, from the glyphs' bounding boxes and
    /// the rules; `None` when nothing is drawn.
    #[must_use]
    pub fn ink(&self) -> Option<(f32, f32, f32, f32)> {
        boxes::ink(&self.fonts, &self.items)
    }
}

/// Typesets `formula` with the bundled fonts, in display style (`$$…$$`) or text style (`$…$`).
///
/// # Errors
/// [`Error::MissingGlyph`] for a character neither the math font nor (in `\text`) the text font has, and
/// [`Error::UnknownColour`] for a `\color` the typesetter does not know.
pub fn typeset(formula: &Formula, display: bool) -> Result<Typeset, Error> {
    typeset_with(formula, display, &Fonts::bundled())
}

/// [`typeset`] with other fonts.
///
/// # Errors
/// As [`typeset`], and [`Error::NotMathFont`] when `fonts.math` has no `MATH` table or constants.
pub fn typeset_with(formula: &Formula, display: bool, fonts: &Fonts) -> Result<Typeset, Error> {
    let bx = layout::formula(formula, display, fonts)?;
    Ok(Typeset {
        width: bx.width,
        ascent: bx.height,
        descent: bx.depth,
        units_per_em: f32::from(fonts.math.units_per_em()),
        items: bx.items,
        fonts: fonts.clone(),
    })
}

/// Reads and typesets `tex`, and draws it as a PNG.
///
/// # Errors
/// [`Error::Parse`] when `tex` cannot be read, then as [`typeset`].
pub fn tex_to_png(tex: &str, display: bool, options: &RenderOptions) -> Result<Vec<u8>, Error> {
    Ok(to_png(&typeset(&crate::parse(tex)?, display)?, options))
}

/// Reads and typesets `tex`, and writes it as SVG.
///
/// # Errors
/// [`Error::Parse`] when `tex` cannot be read, then as [`typeset`].
pub fn tex_to_svg(tex: &str, display: bool, options: &SvgOptions) -> Result<String, Error> {
    Ok(to_svg(&typeset(&crate::parse(tex)?, display)?, options))
}

//! Fonts and text with no dependencies: a TrueType parser, kerning, line breaking, Noto Sans embedded, and
//! text drawn through [`quench_raster`].
//!
//! ```
//! use quench_raster::{Canvas, Color};
//! use quench_text::FontSet;
//!
//! let fonts = FontSet::noto_sans();
//! let layout = fonts.layout("Zażółć gęślą jaźń: Привет", 16.0, Some(120.0));
//! let mut canvas = Canvas::new(140, 120);
//! fonts.draw(&mut canvas, &layout, 8.0, 8.0, Color::WHITE);
//! assert!(layout.lines.len() >= 2 && layout.width <= 120.0);
//! ```

// Glyph arithmetic: font units and pixels are small numbers (a glyph id is 16 bits, a coordinate 16 bits, an em is
// 1000 or 2048 units), every cast is of such a value, and the formulas read as the specification writes them.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::suboptimal_flops
)]

mod font;
mod gpos;
mod layout;
mod system;

pub use font::{Font, FontError, Outline, Point};
pub use layout::{
    FontSet, Glyph, Layout, Line, noto_sans, noto_sans_bold, noto_sans_light, noto_sans_medium,
};
pub use system::SystemFonts;

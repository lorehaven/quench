//! A software rasteriser with no dependencies: it draws anti-aliased shapes into a [`Canvas`].
//!
//! The canvas holds **premultiplied BGRA** bytes, the memory layout of Wayland's `ARGB8888` (and of a
//! little-endian `u32` `0xAARRGGBB`), so a frame can be handed to a compositor as it is. Drawing is
//! source-over compositing with an optional clip.
//!
//! - [`Canvas::fill_rect`]: whole-pixel rectangles, the fast path.
//! - [`Canvas::fill_rounded_rect`], [`Canvas::stroke_rounded_rect`]: anti-aliased from a signed distance.
//! - [`Canvas::fill_path`]: any [`Path`] of lines and curves, anti-aliased by exact signed-area coverage
//!   (the accumulation method: one pass over the edges, then a running sum per row).
//!
//! Coordinates are `f32` pixels with the origin at the top left; pixel `(x, y)` covers
//! `x..x+1, y..y+1`. The coverage rule for a path is **non-zero winding**, with overlapping regions of the
//! same direction counted once.

// Pixel arithmetic: coordinates and coverage are bounded by the canvas and by clamps before they are cast, and
// `x`, `y`, `w`, `h`, `r`, `g`, `b`, `a` are the domain's own names.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::many_single_char_names,
    clippy::too_many_arguments,
    clippy::similar_names,
    // The rasteriser is written as the formulas read; `mul_add` and `midpoint` would hide them, and an edge that is
    // exactly horizontal is the case `==` is for.
    clippy::suboptimal_flops,
    clippy::manual_midpoint,
    clippy::float_cmp
)]

mod canvas;
mod color;
mod path;

pub use canvas::{Canvas, Rounded};
pub use color::Color;
pub use path::Path;

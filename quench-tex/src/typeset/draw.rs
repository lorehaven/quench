//! Drawing a typeset formula to pixels: glyph outlines and rules filled antialiased through `quench-raster`,
//! read back as straight-alpha RGBA and encoded as PNG with `quench-image`.
//!
//! The image holds the formula's box and all of its ink (an italic letter or a large operator can reach past the
//! box), plus the padding; the baseline falls on a pixel boundary, and [`Raster::baseline`] says where, so a
//! caller can line the image up with text.

use quench_raster::{Canvas, Color, Path};

use super::{Item, Typeset};

/// How a formula is drawn.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderOptions {
    /// The size of the formula's em in pixels (the font size).
    pub pixels_per_em: f32,
    /// The colour of everything not coloured by `\color`.
    pub colour: Color,
    /// The background, transparent when `None`.
    pub background: Option<Color>,
    /// Pixels of space around the formula on every side.
    pub padding: u32,
}

impl Default for RenderOptions {
    /// 32 pixels to the em, black on transparent, 2 pixels of padding.
    fn default() -> Self {
        Self {
            pixels_per_em: 32.0,
            colour: Color::BLACK,
            background: None,
            padding: 2,
        }
    }
}

/// A formula drawn: RGBA pixels, straight alpha, `width * 4` bytes a row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Raster {
    pub width: u32,
    pub height: u32,
    /// The row the baseline is at, from the top: the formula's ascent is above it, its descent below.
    pub baseline: u32,
    /// The pixels.
    pub rgba: Vec<u8>,
}

/// Draws `set` as RGBA pixels.
#[must_use]
pub fn to_rgba(set: &Typeset, options: &RenderOptions) -> Raster {
    let px = options.pixels_per_em / set.units_per_em;
    let pad = options.padding as f32;
    let (mut x0, mut y0, mut x1, mut y1) = (0.0_f32, -set.descent, set.width, set.ascent);
    if let Some((a, b, c, d)) = set.ink() {
        (x0, y0, x1, y1) = (x0.min(a), y0.min(b), x1.max(c), y1.max(d));
    }
    let left = (x0 * px).floor() - pad;
    let top = (y1 * px).ceil() + pad;
    let width = ((x1 * px).ceil() + pad - left).max(1.0);
    let height = (top - ((y0 * px).floor() - pad)).max(1.0);
    let mut canvas = Canvas::new(width as usize, height as usize);
    if let Some(background) = options.background {
        canvas.clear(background);
    }
    let (ox, oy) = (-left, top);
    for item in &set.items {
        match *item {
            Item::Glyph {
                font,
                glyph,
                x,
                y,
                size,
                colour,
            } => {
                let face = set.fonts.face(font);
                if let Some(outline) = face.outline(glyph) {
                    let scale = size / f32::from(face.units_per_em()) * px;
                    let path =
                        outline.to_path_slanted(scale, ox + x * px, oy - y * px, font.slant());
                    canvas.fill_path(&path, 0.0, 0.0, colour.unwrap_or(options.colour));
                }
            }
            Item::Rule {
                x,
                y,
                width,
                height,
                colour,
            } => {
                let mut path = Path::new();
                path.rect(ox + x * px, oy - (y + height) * px, width * px, height * px);
                canvas.fill_path(&path, 0.0, 0.0, colour.unwrap_or(options.colour));
            }
        }
    }
    let (w, h) = (canvas.width(), canvas.height());
    let mut rgba = Vec::with_capacity(w * h * 4);
    for row in 0..h {
        for column in 0..w {
            let p = canvas.pixel(column, row);
            rgba.extend_from_slice(&[p.r, p.g, p.b, p.a]);
        }
    }
    Raster {
        width: w as u32,
        height: h as u32,
        baseline: top as u32,
        rgba,
    }
}

/// Draws `set` as a PNG file.
#[must_use]
pub fn to_png(set: &Typeset, options: &RenderOptions) -> Vec<u8> {
    let raster = to_rgba(set, options);
    quench_image::encode_png(raster.width, raster.height, &raster.rgba)
}

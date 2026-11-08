//! Writing a typeset formula as SVG: one `<path>` per glyph from its outline, one `<rect>` per rule, in a
//! `viewBox` measured in ems, so the picture scales with the font size of the page it is in.
//!
//! Everything is filled with `currentColor` unless a colour is given, so the formula takes the text colour of
//! the page; `\color` parts carry their own. The baseline is exposed as a `vertical-align` style: an inline
//! `<svg>` sits on the text's baseline as it is.

use std::fmt::Write as _;

use quench_raster::Color;
use quench_text::Outline;

use super::{Item, Typeset};

/// How a formula is written as SVG.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SvgOptions {
    /// The colour of everything not coloured by `\color`; `None` for `currentColor`.
    pub colour: Option<Color>,
    /// Space around the formula on every side, in ems.
    pub padding: f32,
}

/// A number in ems, short: at most four decimals, no trailing zeros.
fn num(value: f32) -> String {
    let rounded = (value * 10_000.0).round() / 10_000.0;
    let text = format!("{:.4}", if rounded == 0.0 { 0.0 } else { rounded });
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

fn fill(colour: Color) -> String {
    let mut out = format!("#{:02x}{:02x}{:02x}", colour.r, colour.g, colour.b);
    if colour.a < 255 {
        let _ = write!(
            out,
            "\" fill-opacity=\"{}",
            num(f32::from(colour.a) / 255.0)
        );
    }
    out
}

/// The path data of an outline, its points mapped by `at`: quadratic curves as TrueType draws them, an
/// off-curve point between two others implying the on-curve point halfway.
fn path_data(outline: &Outline, at: impl Fn(f32, f32) -> (f32, f32)) -> String {
    let mut d = String::new();
    let point = |d: &mut String, command: char, p: (f32, f32)| {
        let _ = write!(d, "{command}{} {}", num(p.0), num(p.1));
    };
    for contour in &outline.contours {
        if contour.len() < 2 {
            continue;
        }
        // Start at an on-curve point; with none, halfway between the last and the first.
        let (start, rest): ((f32, f32), Vec<_>) = contour.iter().position(|p| p.on).map_or_else(
            || {
                let (a, b) = (contour[0], contour[contour.len() - 1]);
                (
                    at(f32::midpoint(a.x, b.x), f32::midpoint(a.y, b.y)),
                    contour.clone(),
                )
            },
            |i| {
                let rotated: Vec<_> = contour[i..].iter().chain(&contour[..i]).copied().collect();
                (at(rotated[0].x, rotated[0].y), rotated[1..].to_vec())
            },
        );
        point(&mut d, 'M', start);
        let mut pending: Option<(f32, f32)> = None;
        for p in rest {
            let q = at(p.x, p.y);
            if p.on {
                if let Some(c) = pending.take() {
                    point(&mut d, 'Q', c);
                    point(&mut d, ' ', q);
                } else {
                    point(&mut d, 'L', q);
                }
            } else {
                if let Some(c) = pending {
                    point(&mut d, 'Q', c);
                    point(
                        &mut d,
                        ' ',
                        (f32::midpoint(c.0, q.0), f32::midpoint(c.1, q.1)),
                    );
                }
                pending = Some(q);
            }
        }
        if let Some(c) = pending {
            point(&mut d, 'Q', c);
            point(&mut d, ' ', start);
        }
        d.push('Z');
    }
    d
}

/// Writes `set` as an SVG document (an `<svg>` element, which can also be put inline in HTML).
#[must_use]
pub fn to_svg(set: &Typeset, options: &SvgOptions) -> String {
    let em = set.units_per_em;
    let pad = options.padding.max(0.0);
    let (mut x0, mut y0, mut x1, mut y1) = (0.0_f32, -set.descent, set.width, set.ascent);
    if let Some((a, b, c, d)) = set.ink() {
        (x0, y0, x1, y1) = (x0.min(a), y0.min(b), x1.max(c), y1.max(d));
    }
    let (left, top) = (x0 / em - pad, -(y1 / em) - pad);
    let (width, height) = ((x1 - x0) / em + 2.0 * pad, (y1 - y0) / em + 2.0 * pad);
    let bottom = -(y0 / em) + pad;
    let colour = options
        .colour
        .map_or_else(|| "currentColor".to_owned(), fill);
    let mut out = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}em\" height=\"{h}em\" \
         viewBox=\"{l} {t} {w} {h}\" style=\"vertical-align: {v}em\" fill=\"{colour}\">",
        w = num(width),
        h = num(height),
        l = num(left),
        t = num(top),
        v = num(-bottom),
    );
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
                let Some(outline) = face.outline(glyph) else {
                    continue;
                };
                let k = size / f32::from(face.units_per_em());
                let slant = font.slant();
                let d = path_data(&outline, |px, py| {
                    ((x + (px + slant * py) * k) / em, -(y + py * k) / em)
                });
                out.push_str("<path");
                if let Some(c) = colour {
                    let _ = write!(out, " fill=\"{}\"", fill(c));
                }
                let _ = write!(out, " d=\"{d}\"/>");
            }
            Item::Rule {
                x,
                y,
                width,
                height,
                colour,
            } => {
                let _ = write!(
                    out,
                    "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"",
                    num(x / em),
                    num(-(y + height) / em),
                    num(width / em),
                    num(height / em)
                );
                if let Some(c) = colour {
                    let _ = write!(out, " fill=\"{}\"", fill(c));
                }
                out.push_str("/>");
            }
        }
    }
    out.push_str("</svg>");
    out
}

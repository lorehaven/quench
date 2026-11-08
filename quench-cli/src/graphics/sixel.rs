//! A sixel encoder: RGBA pixels to a complete DCS sixel string, for terminals without the kitty protocol.
//!
//! Sixel has a palette of indexed colours and no alpha. So the image is first blended onto the
//! background the terminal shows (partly transparent pixels) or left out (fully transparent ones, which
//! with `P2=1` keep what is on the screen), then quantized to at most 256 colours. Palette colours are
//! given in percent, so the blended colours are reduced to what percent can express (101 levels per
//! channel) before counting them: a typeset formula, one colour with antialiased edges, then needs only a
//! few dozen entries and is reproduced exactly. Images with more colours go through median cut.
//!
//! The pixels are written in bands of six rows; in each band only the colours present in it are drawn,
//! runs of the same sixel are run-length encoded (`!n`), and empty sixels at the end of a row are left out.

use super::Rgb;
use std::collections::HashMap;
use std::fmt::{self, Write};

/// The most colours a sixel image gets.
pub const MAX_COLOURS: usize = 256;

/// Why pixels cannot be encoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SixelError {
    /// The pixel buffer is not `width × height × 4` bytes long.
    SizeMismatch { expected: usize, actual: usize },
}

impl fmt::Display for SixelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SizeMismatch { expected, actual } => {
                write!(f, "expected {expected} bytes of RGBA pixels, got {actual}")
            }
        }
    }
}

impl std::error::Error for SixelError {}

/// An image reduced to a palette.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Quantized {
    pub width: usize,
    pub height: usize,
    /// At most [`MAX_COLOURS`] colours, each one a percent value per channel can express exactly.
    pub palette: Vec<Rgb>,
    /// One entry per pixel, row by row: the palette index, `None` for a fully transparent pixel.
    pub pixels: Vec<Option<u8>>,
}

/// A colour in sixel's percent units, 0 to 100 per channel.
type Percent = [u8; 3];

fn to_percent(value: u8) -> u8 {
    // Rounded; the result is at most 100, so the cast cannot truncate.
    u8::try_from((u32::from(value) * 100 + 127) / 255).unwrap_or(100)
}

fn from_percent(value: u8) -> u8 {
    u8::try_from((u32::from(value) * 255 + 50) / 100).unwrap_or(255)
}

/// `colour` with `alpha` (0 to 255) over `background`, straight alpha, rounded.
#[must_use]
pub fn blend(colour: Rgb, alpha: u8, background: Rgb) -> Rgb {
    let mix = |c: u8, b: u8| {
        let a = u32::from(alpha);
        u8::try_from((u32::from(c) * a + u32::from(b) * (255 - a) + 127) / 255).unwrap_or(255)
    };
    Rgb::new(
        mix(colour.r, background.r),
        mix(colour.g, background.g),
        mix(colour.b, background.b),
    )
}

/// `colour` as sixel shows it: each channel rounded to a whole percent and back.
#[must_use]
pub fn as_shown(colour: Rgb) -> Rgb {
    let p = percent(colour);
    Rgb::new(from_percent(p[0]), from_percent(p[1]), from_percent(p[2]))
}

fn percent(colour: Rgb) -> Percent {
    [
        to_percent(colour.r),
        to_percent(colour.g),
        to_percent(colour.b),
    ]
}

const fn check(rgba: &[u8], width: usize, height: usize) -> Result<(), SixelError> {
    let expected = width.saturating_mul(height).saturating_mul(4);
    if rgba.len() == expected {
        Ok(())
    } else {
        Err(SixelError::SizeMismatch {
            expected,
            actual: rgba.len(),
        })
    }
}

/// Blends `rgba` (straight alpha) onto `background` and reduces it to at most `max_colours` colours
/// (clamped to 1..=[`MAX_COLOURS`]).
///
/// # Errors
///
/// [`SixelError::SizeMismatch`] when `rgba` is not `width × height × 4` bytes.
pub fn quantize(
    rgba: &[u8],
    width: usize,
    height: usize,
    background: Rgb,
    max_colours: usize,
) -> Result<Quantized, SixelError> {
    check(rgba, width, height)?;
    let max_colours = max_colours.clamp(1, MAX_COLOURS);
    let shown: Vec<Option<Percent>> = rgba
        .as_chunks::<4>()
        .0
        .iter()
        .map(|&[r, g, b, a]| (a != 0).then(|| percent(blend(Rgb::new(r, g, b), a, background))))
        .collect();
    let mut counts: HashMap<Percent, u32> = HashMap::new();
    for colour in shown.iter().flatten() {
        *counts.entry(*colour).or_insert(0) += 1;
    }
    let mut colours: Vec<(Percent, u32)> = counts.into_iter().collect();
    // Deterministic output: the most frequent colour first.
    colours.sort_unstable_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));

    let (palette, index): (Vec<Percent>, HashMap<Percent, u8>) = if colours.len() <= max_colours {
        let index = colours
            .iter()
            .enumerate()
            .map(|(n, (colour, _))| (*colour, u8::try_from(n).unwrap_or(u8::MAX)))
            .collect();
        (
            colours.into_iter().map(|(colour, _)| colour).collect(),
            index,
        )
    } else {
        median_cut(colours, max_colours)
    };
    Ok(Quantized {
        width,
        height,
        palette: palette
            .into_iter()
            .map(|p| Rgb::new(from_percent(p[0]), from_percent(p[1]), from_percent(p[2])))
            .collect(),
        pixels: shown
            .into_iter()
            .map(|colour| colour.and_then(|c| index.get(&c).copied()))
            .collect(),
    })
}

/// Splits the colours into `max_colours` boxes (the box with the widest channel range is cut at the
/// weighted median), each box's colour being its weighted average.
fn median_cut(
    colours: Vec<(Percent, u32)>,
    max_colours: usize,
) -> (Vec<Percent>, HashMap<Percent, u8>) {
    let range = |set: &[(Percent, u32)]| -> (usize, u8) {
        (0..3)
            .map(|channel| {
                let (lo, hi) = set.iter().fold((u8::MAX, 0), |(lo, hi), (c, _)| {
                    (lo.min(c[channel]), hi.max(c[channel]))
                });
                (channel, hi - lo)
            })
            .max_by_key(|&(channel, width)| (width, std::cmp::Reverse(channel)))
            .unwrap_or((0, 0))
    };
    let mut boxes = vec![colours];
    while boxes.len() < max_colours {
        let Some((at, channel)) = boxes
            .iter()
            .enumerate()
            .filter(|(_, set)| set.len() > 1)
            .map(|(n, set)| (n, range(set)))
            .max_by_key(|&(n, (_, width))| (width, std::cmp::Reverse(n)))
            .map(|(n, (channel, _))| (n, channel))
        else {
            break;
        };
        let mut set = boxes.swap_remove(at);
        set.sort_unstable_by_key(|(c, _)| (c[channel], *c));
        let total: u64 = set.iter().map(|(_, n)| u64::from(*n)).sum();
        let mut seen = 0;
        let mut cut = set.len() / 2;
        for (n, (_, count)) in set.iter().enumerate() {
            seen += u64::from(*count);
            if seen * 2 >= total {
                cut = n + 1;
                break;
            }
        }
        let cut = cut.clamp(1, set.len() - 1);
        let upper = set.split_off(cut);
        boxes.push(set);
        boxes.push(upper);
    }
    let mut palette = Vec::with_capacity(boxes.len());
    let mut index = HashMap::new();
    for (n, set) in boxes.iter().enumerate() {
        let total: u64 = set
            .iter()
            .map(|(_, count)| u64::from(*count))
            .sum::<u64>()
            .max(1);
        let mut average = [0u8; 3];
        for (channel, value) in average.iter_mut().enumerate() {
            let sum: u64 = set
                .iter()
                .map(|(c, count)| u64::from(c[channel]) * u64::from(*count))
                .sum();
            *value = u8::try_from((sum + total / 2) / total).unwrap_or(100);
        }
        palette.push(average);
        let n = u8::try_from(n).unwrap_or(u8::MAX);
        for (colour, _) in set {
            index.insert(*colour, n);
        }
    }
    (palette, index)
}

/// Encodes `rgba` (straight alpha, row by row, `width × height × 4` bytes) as a sixel image, blending
/// partly transparent pixels onto `background` and leaving fully transparent ones out.
///
/// # Errors
///
/// [`SixelError::SizeMismatch`] when `rgba` is not `width × height × 4` bytes.
pub fn encode(
    rgba: &[u8],
    width: usize,
    height: usize,
    background: Rgb,
) -> Result<String, SixelError> {
    Ok(encode_quantized(&quantize(
        rgba,
        width,
        height,
        background,
        MAX_COLOURS,
    )?))
}

/// Writes `count` copies of the sixel `sixel` (0 to 63), run-length encoded when that is shorter.
fn push_run(out: &mut String, sixel: u8, count: usize) {
    let c = char::from(0x3f + sixel);
    if count > 3 {
        let _ = write!(out, "!{count}{c}");
    } else {
        for _ in 0..count {
            out.push(c);
        }
    }
}

/// The DCS sixel string for an image already reduced to a palette.
#[must_use]
pub fn encode_quantized(image: &Quantized) -> String {
    let (width, height) = (image.width, image.height);
    // P2=1: pixels without a sixel bit set keep what the screen shows (transparency).
    let mut out = format!("\x1bP0;1;0q\"1;1;{width};{height}");
    for (n, colour) in image.palette.iter().enumerate() {
        let p = percent(*colour);
        let _ = write!(out, "#{n};2;{};{};{}", p[0], p[1], p[2]);
    }
    let mut rows: Vec<Option<Vec<u8>>> = vec![None; image.palette.len()];
    let mut present: Vec<usize> = Vec::new();
    for band in 0..height.div_ceil(6) {
        if band > 0 {
            out.push('-');
        }
        present.clear();
        for bit in 0..6 {
            let y = band * 6 + bit;
            if y >= height {
                break;
            }
            let line = image.pixels.get(y * width..(y + 1) * width).unwrap_or(&[]);
            for (x, pixel) in line.iter().enumerate() {
                let Some(colour) = pixel.map(usize::from).filter(|&c| c < rows.len()) else {
                    continue;
                };
                let row = rows[colour].get_or_insert_with(|| {
                    present.push(colour);
                    vec![0; width]
                });
                row[x] |= 1 << bit;
            }
        }
        present.sort_unstable();
        for (k, &colour) in present.iter().enumerate() {
            if k > 0 {
                out.push('$');
            }
            let _ = write!(out, "#{colour}");
            let Some(row) = rows[colour].take() else {
                continue;
            };
            let end = row.iter().rposition(|&s| s != 0).map_or(0, |at| at + 1);
            let mut x = 0;
            while x < end {
                let sixel = row[x];
                let run = row[x..end].iter().take_while(|&&s| s == sixel).count();
                push_run(&mut out, sixel, run);
                x += run;
            }
        }
    }
    out.push_str("\x1b\\");
    out
}

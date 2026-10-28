//! Colours: hex, `rgb()`/`rgba()`, `hsl()`, names, and `color-mix()` in sRGB.

use crate::value::{Value, split_commas};

/// An sRGB colour with straight alpha.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const TRANSPARENT: Self = Self {
        r: 0,
        g: 0,
        b: 0,
        a: 0,
    };
    pub const BLACK: Self = Self {
        r: 0,
        g: 0,
        b: 0,
        a: 255,
    };
    pub const WHITE: Self = Self {
        r: 255,
        g: 255,
        b: 255,
        a: 255,
    };

    #[must_use]
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// The colour with its alpha scaled by `opacity` (0 to 1).
    #[must_use]
    pub fn with_opacity(self, opacity: f32) -> Self {
        Self {
            a: (f32::from(self.a) * opacity.clamp(0.0, 1.0)).round() as u8,
            ..self
        }
    }
}

const NAMED: &[(&str, [u8; 3])] = &[
    ("black", [0, 0, 0]),
    ("silver", [192, 192, 192]),
    ("gray", [128, 128, 128]),
    ("grey", [128, 128, 128]),
    ("white", [255, 255, 255]),
    ("maroon", [128, 0, 0]),
    ("red", [255, 0, 0]),
    ("purple", [128, 0, 128]),
    ("fuchsia", [255, 0, 255]),
    ("green", [0, 128, 0]),
    ("lime", [0, 255, 0]),
    ("olive", [128, 128, 0]),
    ("yellow", [255, 255, 0]),
    ("navy", [0, 0, 128]),
    ("blue", [0, 0, 255]),
    ("teal", [0, 128, 128]),
    ("aqua", [0, 255, 255]),
    ("orange", [255, 165, 0]),
    ("pink", [255, 192, 203]),
    ("brown", [165, 42, 42]),
    ("gold", [255, 215, 0]),
    ("indigo", [75, 0, 130]),
    ("violet", [238, 130, 238]),
    ("crimson", [220, 20, 60]),
    ("tomato", [255, 99, 71]),
    ("coral", [255, 127, 80]),
    ("salmon", [250, 128, 114]),
    ("khaki", [240, 230, 140]),
    ("lightgray", [211, 211, 211]),
    ("lightgrey", [211, 211, 211]),
    ("darkgray", [169, 169, 169]),
    ("darkgrey", [169, 169, 169]),
    ("dimgray", [105, 105, 105]),
    ("whitesmoke", [245, 245, 245]),
    ("rebeccapurple", [102, 51, 153]),
    ("cyan", [0, 255, 255]),
    ("magenta", [255, 0, 255]),
    ("transparent", [0, 0, 0]),
];

/// What a colour value is: a colour, or `currentcolor` (the element's own `color`, known later).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorValue {
    Color(Color),
    CurrentColor,
}

impl ColorValue {
    #[must_use]
    pub const fn resolve(self, current: Color) -> Color {
        match self {
            Self::Color(c) => c,
            Self::CurrentColor => current,
        }
    }
}

/// A colour from one value (or `None` when it is not a colour).
#[must_use]
pub fn parse_color(value: &Value) -> Option<ColorValue> {
    match value {
        Value::Hash(hex) => parse_hex(hex).map(ColorValue::Color),
        Value::Ident(name) => {
            let name = name.to_ascii_lowercase();
            if name == "currentcolor" {
                return Some(ColorValue::CurrentColor);
            }
            if name == "transparent" {
                return Some(ColorValue::Color(Color::TRANSPARENT));
            }
            NAMED
                .iter()
                .find(|(n, _)| *n == name)
                .map(|(_, [r, g, b])| ColorValue::Color(Color::rgba(*r, *g, *b, 255)))
        }
        Value::Function(name, args) => match name.as_str() {
            "rgb" | "rgba" => parse_rgb(args).map(ColorValue::Color),
            "hsl" | "hsla" => parse_hsl(args).map(ColorValue::Color),
            "color-mix" => parse_mix(args),
            _ => None,
        },
        _ => None,
    }
}

fn parse_hex(hex: &str) -> Option<Color> {
    if !hex.is_ascii() {
        return None;
    }
    let digit = |i: usize| u8::from_str_radix(&hex[i..=i], 16).ok().map(|d| d * 17);
    let pair = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    match hex.len() {
        3 => Some(Color::rgba(digit(0)?, digit(1)?, digit(2)?, 255)),
        4 => Some(Color::rgba(digit(0)?, digit(1)?, digit(2)?, digit(3)?)),
        6 => Some(Color::rgba(pair(0)?, pair(2)?, pair(4)?, 255)),
        8 => Some(Color::rgba(pair(0)?, pair(2)?, pair(4)?, pair(6)?)),
        _ => None,
    }
}

/// The arguments of a colour function without its separators: `0 0 0 / 0.5` and `0, 0, 0, 0.5` alike.
fn components(args: &[Value]) -> Vec<&Value> {
    args.iter()
        .filter(|v| !matches!(v, Value::Comma | Value::Delim('/')))
        .collect()
}

fn channel(v: &Value) -> Option<f32> {
    match v {
        Value::Number(n) => Some(n.clamp(0.0, 255.0)),
        Value::Percent(p) => Some((p / 100.0 * 255.0).clamp(0.0, 255.0)),
        _ => None,
    }
}

fn alpha(v: Option<&&Value>) -> Option<f32> {
    match v {
        None => Some(1.0),
        Some(Value::Number(n)) => Some(n.clamp(0.0, 1.0)),
        Some(Value::Percent(p)) => Some((p / 100.0).clamp(0.0, 1.0)),
        _ => None,
    }
}

fn parse_rgb(args: &[Value]) -> Option<Color> {
    let c = components(args);
    if c.len() < 3 || c.len() > 4 {
        return None;
    }
    let (r, g, b) = (channel(c[0])?, channel(c[1])?, channel(c[2])?);
    Some(Color::rgba(
        r.round() as u8,
        g.round() as u8,
        b.round() as u8,
        (alpha(c.get(3))? * 255.0).round() as u8,
    ))
}

fn parse_hsl(args: &[Value]) -> Option<Color> {
    let c = components(args);
    if c.len() < 3 || c.len() > 4 {
        return None;
    }
    let hue = match c[0] {
        Value::Number(n) => *n,
        Value::Dimension(n, unit) if unit == "deg" => *n,
        _ => return None,
    };
    let percent = |v: &Value| match v {
        Value::Percent(p) => Some((p / 100.0).clamp(0.0, 1.0)),
        _ => None,
    };
    let (s, l) = (percent(c[1])?, percent(c[2])?);
    let k = |n: f32| (n + hue.rem_euclid(360.0) / 30.0) % 12.0;
    let a = s * l.min(1.0 - l);
    let f =
        |n: f32| ((l - a * (k(n) - 3.0).min(9.0 - k(n)).clamp(-1.0, 1.0)) * 255.0).round() as u8;
    Some(Color::rgba(
        f(0.0),
        f(8.0),
        f(4.0),
        (alpha(c.get(3))? * 255.0).round() as u8,
    ))
}

/// `color-mix(in srgb, A p%, B q%)`: a straight mix of the channels. Other colour spaces are mixed as sRGB too.
fn parse_mix(args: &[Value]) -> Option<ColorValue> {
    let parts = split_commas(args);
    // The first part names the space (`in srgb`); the next two are `colour [percentage]`.
    if parts.len() != 3
        || !matches!(parts[0].first(), Some(Value::Ident(w)) if w.eq_ignore_ascii_case("in"))
    {
        return None;
    }
    let side = |part: &[Value]| -> Option<(Color, Option<f32>)> {
        let color = part.iter().find_map(parse_color)?;
        let ColorValue::Color(color) = color else {
            return None;
        };
        let percent = part.iter().find_map(|v| {
            if let Value::Percent(p) = v {
                Some(*p)
            } else {
                None
            }
        });
        Some((color, percent))
    };
    let ((c1, p1), (c2, p2)) = (side(&parts[1])?, side(&parts[2])?);
    let (w1, w2) = match (p1, p2) {
        (Some(a), Some(b)) => (a, b),
        (Some(a), None) => (a, 100.0 - a),
        (None, Some(b)) => (100.0 - b, b),
        (None, None) => (50.0, 50.0),
    };
    let total = w1 + w2;
    if total <= 0.0 {
        return None;
    }
    let (w1, w2) = (w1 / total, w2 / total);
    let mix = |a: u8, b: u8| (f32::from(a) * w1 + f32::from(b) * w2).round() as u8;
    Some(ColorValue::Color(Color::rgba(
        mix(c1.r, c2.r),
        mix(c1.g, c2.g),
        mix(c1.b, c2.b),
        mix(c1.a, c2.a),
    )))
}

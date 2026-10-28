//! SVG rendering with no dependencies: a file in, RGBA pixels out.
//!
//! ```
//! let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10">
//!     <rect width="10" height="10" fill="#f00"/><circle cx="15" cy="5" r="4" fill="blue"/></svg>"##;
//! let image = quench_svg::render(svg, 1.0).unwrap();
//! assert_eq!((image.width, image.height), (20, 10));
//! ```
//!
//! What is drawn: `rect` (with rounded corners), `circle`, `ellipse`, `line`, `polyline`, `polygon`, `path` (every
//! command, arcs included), `g`, `svg` (nested too), `use` of an element with an `id`; `fill` and `stroke` colours
//! (hex, `rgb()`, names, `currentColor`, and `url(#gradient)` as the gradient's first stop), `*-opacity` and
//! `opacity`, `stroke-width`, `stroke-linecap`, `stroke-linejoin` and `miterlimit`, `transform`, `viewBox` with
//! `preserveAspectRatio` (meet, centred), and the `style` attribute. Not drawn: text, gradients and patterns as
//! such, filters, masks, clip paths, markers, dashes, `evenodd` fills and CSS in `<style>`; group `opacity` is
//! applied to each child (overlaps inside the group show through).

// Drawing arithmetic: small counts and coordinates move between integers and floats, formulas keep the notation of
// the SVG specification (`a`, `b`, `c`, `d`, `e`, `f`, `rx`, `ry`...), and plain `a * b + c` reads better than
// `mul_add`.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::many_single_char_names,
    clippy::suboptimal_flops,
    clippy::similar_names,
    clippy::too_many_lines,
    clippy::too_many_arguments,
    clippy::float_cmp,
    clippy::match_same_arms,
    clippy::manual_let_else,
    clippy::useless_let_if_seq,
    clippy::missing_const_for_fn,
    clippy::semicolon_if_nothing_returned,
    clippy::manual_midpoint,
    clippy::type_complexity,
    clippy::manual_checked_ops,
    clippy::chunks_exact_to_as_chunks
)]

mod xml;

use quench_css::{ColorValue, parse_color, parse_values, tokenize};
use quench_image::Image;
use quench_raster::{Canvas, Color, Path};
use xml::Element;

/// Why a file could not be drawn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The XML is not well formed (what is wrong with it).
    Invalid(&'static str),
    /// The root is not `svg`.
    NotSvg,
    /// The picture would be bigger than 16384 pixels a side or 64 million pixels.
    TooLarge,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(why) => write!(f, "invalid SVG: {why}"),
            Self::NotSvg => f.write_str("not an SVG file"),
            Self::TooLarge => f.write_str("the picture is too large"),
        }
    }
}

impl std::error::Error for Error {}

const MAX_SIDE: f32 = 16384.0;
const MAX_PIXELS: f32 = 64.0 * 1024.0 * 1024.0;
/// How many elements are drawn at most, and how deep `use` may go (a file that references itself).
const MAX_ELEMENTS: usize = 100_000;
const MAX_USE_DEPTH: usize = 8;

/// An affine transform: `x' = a x + c y + e`, `y' = b x + d y + f`.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Matrix([f32; 6]);

impl Matrix {
    const IDENTITY: Self = Self([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);

    /// The transform that applies `inner` first, then `self`.
    fn then_inside(self, inner: Self) -> Self {
        let (a, b) = (self.0, inner.0);
        Self([
            a[0] * b[0] + a[2] * b[1],
            a[1] * b[0] + a[3] * b[1],
            a[0] * b[2] + a[2] * b[3],
            a[1] * b[2] + a[3] * b[3],
            a[0] * b[4] + a[2] * b[5] + a[4],
            a[1] * b[4] + a[3] * b[5] + a[5],
        ])
    }

    fn apply(self, x: f32, y: f32) -> (f32, f32) {
        let m = self.0;
        (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5])
    }

    /// How much lengths grow, on average.
    fn scale(self) -> f32 {
        (self.0[0] * self.0[3] - self.0[1] * self.0[2]).abs().sqrt()
    }
}

/// Numbers in a list: separated by spaces and commas, signs and dots starting new ones.
fn numbers(text: &str) -> Vec<f32> {
    let mut out = Vec::new();
    let mut p = Scanner::new(text);
    while let Some(n) = p.number() {
        out.push(n);
    }
    out
}

struct Scanner<'a> {
    text: &'a [u8],
    at: usize,
}

impl<'a> Scanner<'a> {
    const fn new(text: &'a str) -> Self {
        Self {
            text: text.as_bytes(),
            at: 0,
        }
    }

    fn skip(&mut self) {
        while self
            .text
            .get(self.at)
            .is_some_and(|c| c.is_ascii_whitespace() || *c == b',')
        {
            self.at += 1;
        }
    }

    fn peek(&mut self) -> Option<u8> {
        self.skip();
        self.text.get(self.at).copied()
    }

    fn number(&mut self) -> Option<f32> {
        self.skip();
        let start = self.at;
        let mut i = self.at;
        let t = self.text;
        if matches!(t.get(i), Some(b'+' | b'-')) {
            i += 1;
        }
        let digits = |i: &mut usize| {
            let from = *i;
            while t.get(*i).is_some_and(u8::is_ascii_digit) {
                *i += 1;
            }
            *i > from
        };
        let whole = digits(&mut i);
        let mut fraction = false;
        if t.get(i) == Some(&b'.') {
            i += 1;
            fraction = digits(&mut i);
        }
        if !whole && !fraction {
            return None;
        }
        if matches!(t.get(i), Some(b'e' | b'E')) {
            let mut j = i + 1;
            if matches!(t.get(j), Some(b'+' | b'-')) {
                j += 1;
            }
            if digits(&mut j) {
                i = j;
            }
        }
        self.at = i;
        std::str::from_utf8(&t[start..i]).ok()?.parse().ok()
    }

    /// A single `0` or `1`, as an arc flag (which needs no separator after it).
    fn flag(&mut self) -> Option<bool> {
        self.skip();
        let c = *self.text.get(self.at)?;
        if c == b'0' || c == b'1' {
            self.at += 1;
            Some(c == b'1')
        } else {
            None
        }
    }
}

/// A length in user units (`px`, none; other units by their CSS size; percentages need the reference).
fn length(text: &str, reference: f32) -> Option<f32> {
    let text = text.trim();
    // The unit starts at the first letter that is not the `e` of an exponent (`1e3` is a number, `6em` is not).
    let bytes = text.as_bytes();
    let split = (0..bytes.len())
        .find(|&i| {
            let c = bytes[i];
            let exponent = (c == b'e' || c == b'E')
                && i > 0
                && bytes.get(i + 1).is_some_and(|n| {
                    n.is_ascii_digit()
                        || ((*n == b'-' || *n == b'+')
                            && bytes.get(i + 2).is_some_and(u8::is_ascii_digit))
                });
            (c.is_ascii_alphabetic() && !exponent) || c == b'%'
        })
        .unwrap_or(text.len());
    let (number, unit) = text.split_at(split);
    let n: f32 = number.trim().parse().ok()?;
    Some(match unit.trim() {
        "" | "px" => n,
        "pt" => n * 4.0 / 3.0,
        "pc" => n * 16.0,
        "mm" => n * 96.0 / 25.4,
        "cm" => n * 96.0 / 2.54,
        "in" => n * 96.0,
        "em" => n * 16.0,
        "%" => n * reference / 100.0,
        _ => return None,
    })
}

fn parse_transform(text: &str) -> Matrix {
    let mut m = Matrix::IDENTITY;
    let mut rest = text;
    while let Some(open) = rest.find('(') {
        let name = rest[..open]
            .trim()
            .trim_start_matches(|c: char| c == ',' || c.is_whitespace());
        let Some(close) = rest[open..].find(')') else {
            break;
        };
        let n = numbers(&rest[open + 1..open + close]);
        let step = match (name, n.as_slice()) {
            ("matrix", [a, b, c, d, e, f]) => Matrix([*a, *b, *c, *d, *e, *f]),
            ("translate", [x]) => Matrix([1.0, 0.0, 0.0, 1.0, *x, 0.0]),
            ("translate", [x, y]) => Matrix([1.0, 0.0, 0.0, 1.0, *x, *y]),
            ("scale", [s]) => Matrix([*s, 0.0, 0.0, *s, 0.0, 0.0]),
            ("scale", [x, y]) => Matrix([*x, 0.0, 0.0, *y, 0.0, 0.0]),
            ("rotate", [a, rest @ ..]) => {
                let (sin, cos) = a.to_radians().sin_cos();
                let rotation = Matrix([cos, sin, -sin, cos, 0.0, 0.0]);
                match rest {
                    [cx, cy] => Matrix([1.0, 0.0, 0.0, 1.0, *cx, *cy])
                        .then_inside(rotation)
                        .then_inside(Matrix([1.0, 0.0, 0.0, 1.0, -cx, -cy])),
                    _ => rotation,
                }
            }
            ("skewX", [a]) => Matrix([1.0, 0.0, a.to_radians().tan(), 1.0, 0.0, 0.0]),
            ("skewY", [a]) => Matrix([1.0, a.to_radians().tan(), 0.0, 1.0, 0.0, 0.0]),
            _ => Matrix::IDENTITY,
        };
        m = m.then_inside(step);
        rest = &rest[open + close + 1..];
    }
    m
}

/// The colour a property names; `None` for `none` and for what cannot be read.
fn paint(value: &str, current: Color, root: &Element) -> Option<Color> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("none") {
        return None;
    }
    if let Some(reference) = value.strip_prefix("url(") {
        // A gradient is drawn as its first stop.
        let id = reference
            .trim_start_matches(['#', '\'', '"', ' '])
            .split([')', '\'', '"'])
            .next()?;
        let target = find(root, id)?;
        let stop = target.children.iter().find(|c| c.name == "stop")?;
        let style = declarations(stop);
        let color = style
            .iter()
            .find(|(k, _)| k == "stop-color")
            .map(|(_, v)| v.clone())
            .or_else(|| stop.attr("stop-color").map(String::from))
            .unwrap_or_else(|| "black".into());
        return paint(&color, current, root);
    }
    let values = parse_values(&tokenize(value));
    match parse_color(values.first()?)? {
        ColorValue::Color(c) => Some(Color::rgba(c.r, c.g, c.b, c.a)),
        ColorValue::CurrentColor => Some(current),
    }
}

/// An element with this `id`.
fn find<'a>(e: &'a Element, id: &str) -> Option<&'a Element> {
    if e.attr("id") == Some(id) {
        return Some(e);
    }
    e.children.iter().find_map(|c| find(c, id))
}

/// The `style` attribute as name and value pairs.
fn declarations(e: &Element) -> Vec<(String, String)> {
    e.attr("style")
        .unwrap_or_default()
        .split(';')
        .filter_map(|d| d.split_once(':'))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect()
}

/// What is inherited down the tree.
#[derive(Clone)]
struct Style {
    fill: Option<Color>,
    stroke: Option<Color>,
    color: Color,
    stroke_width: f32,
    fill_opacity: f32,
    stroke_opacity: f32,
    opacity: f32,
    cap: Cap,
    join: Join,
    miter_limit: f32,
}

#[derive(Clone, Copy, PartialEq)]
enum Cap {
    Butt,
    Round,
    Square,
}

#[derive(Clone, Copy, PartialEq)]
enum Join {
    Miter,
    Round,
    Bevel,
}

impl Style {
    fn initial() -> Self {
        Self {
            fill: Some(Color::rgb(0, 0, 0)),
            stroke: None,
            color: Color::rgb(0, 0, 0),
            stroke_width: 1.0,
            fill_opacity: 1.0,
            stroke_opacity: 1.0,
            opacity: 1.0,
            cap: Cap::Butt,
            join: Join::Miter,
            miter_limit: 4.0,
        }
    }

    /// This style with the element's properties (attributes, then the `style` attribute) applied.
    fn with(&self, e: &Element, root: &Element) -> Self {
        let mut s = self.clone();
        // `opacity` is not inherited: the group's value is multiplied into its children's.
        let mut set = |name: &str, value: &str| match name {
            "color" => {
                if let Some(c) = paint(value, s.color, root) {
                    s.color = c;
                }
            }
            "fill" => s.fill = paint(value, s.color, root),
            "stroke" => s.stroke = paint(value, s.color, root),
            "stroke-width" => {
                s.stroke_width = length(value, 100.0).unwrap_or(s.stroke_width).max(0.0)
            }
            "fill-opacity" => s.fill_opacity = value.trim().parse().unwrap_or(1.0),
            "stroke-opacity" => s.stroke_opacity = value.trim().parse().unwrap_or(1.0),
            "opacity" => s.opacity *= value.trim().parse::<f32>().unwrap_or(1.0).clamp(0.0, 1.0),
            "stroke-linecap" => {
                s.cap = match value.trim() {
                    "round" => Cap::Round,
                    "square" => Cap::Square,
                    _ => Cap::Butt,
                };
            }
            "stroke-linejoin" => {
                s.join = match value.trim() {
                    "round" => Join::Round,
                    "bevel" => Join::Bevel,
                    _ => Join::Miter,
                };
            }
            "stroke-miterlimit" => s.miter_limit = value.trim().parse().unwrap_or(4.0),
            _ => {}
        };
        // `color` first, so `currentColor` in the same element sees it.
        if let Some(v) = e.attr("color") {
            set("color", v);
        }
        for (name, value) in &e.attrs {
            if name != "color" {
                set(name, value);
            }
        }
        for (name, value) in declarations(e) {
            set(&name, &value);
        }
        s
    }
}

/// Outlines in device space: polylines and whether each is closed.
#[derive(Default)]
struct Shape {
    lines: Vec<(Vec<(f32, f32)>, bool)>,
}

/// Builds a [`Shape`] from user-space drawing commands, transforming as it goes.
struct Builder {
    m: Matrix,
    shape: Shape,
    current: Vec<(f32, f32)>,
    /// The current point and the start of the sub-path, in user space.
    at: (f32, f32),
    start: (f32, f32),
}

impl Builder {
    fn new(m: Matrix) -> Self {
        Self {
            m,
            shape: Shape::default(),
            current: Vec::new(),
            at: (0.0, 0.0),
            start: (0.0, 0.0),
        }
    }

    fn flush(&mut self, closed: bool) {
        if self.current.len() > 1 || (closed && !self.current.is_empty()) {
            self.shape
                .lines
                .push((std::mem::take(&mut self.current), closed));
        } else {
            self.current.clear();
        }
    }

    fn move_to(&mut self, x: f32, y: f32) {
        self.flush(false);
        self.at = (x, y);
        self.start = (x, y);
        self.current.push(self.m.apply(x, y));
    }

    fn line_to(&mut self, x: f32, y: f32) {
        if self.current.is_empty() {
            let (ax, ay) = self.at;
            self.current.push(self.m.apply(ax, ay));
        }
        self.at = (x, y);
        self.current.push(self.m.apply(x, y));
    }

    fn cubic_to(&mut self, c1: (f32, f32), c2: (f32, f32), end: (f32, f32)) {
        let p0 = self.at;
        // Enough steps for the curve's size on the screen.
        let size = [c1, c2, end]
            .iter()
            .map(|p| {
                let (a, b) = (self.m.apply(p0.0, p0.1), self.m.apply(p.0, p.1));
                (a.0 - b.0).hypot(a.1 - b.1)
            })
            .fold(0.0f32, f32::max);
        let steps = (size.sqrt() * 2.0).ceil().clamp(4.0, 64.0) as usize;
        for i in 1..=steps {
            let t = i as f32 / steps as f32;
            let u = 1.0 - t;
            let x = u * u * u * p0.0
                + 3.0 * u * u * t * c1.0
                + 3.0 * u * t * t * c2.0
                + t * t * t * end.0;
            let y = u * u * u * p0.1
                + 3.0 * u * u * t * c1.1
                + 3.0 * u * t * t * c2.1
                + t * t * t * end.1;
            self.line_to(x, y);
        }
    }

    fn quad_to(&mut self, c: (f32, f32), end: (f32, f32)) {
        let p0 = self.at;
        let third = |a: f32, b: f32| a + 2.0 / 3.0 * (b - a);
        self.cubic_to(
            (third(p0.0, c.0), third(p0.1, c.1)),
            (third(end.0, c.0), third(end.1, c.1)),
            end,
        );
    }

    /// An elliptical arc from the current point (SVG's endpoint form).
    #[allow(
        clippy::many_single_char_names,
        reason = "the notation of the SVG specification"
    )]
    fn arc_to(
        &mut self,
        rx: f32,
        ry: f32,
        rotation: f32,
        large: bool,
        sweep: bool,
        end: (f32, f32),
    ) {
        let (x1, y1) = self.at;
        let (x2, y2) = end;
        let (mut rx, mut ry) = (rx.abs(), ry.abs());
        if rx == 0.0 || ry == 0.0 || (x1 == x2 && y1 == y2) {
            self.line_to(x2, y2);
            return;
        }
        let (sin, cos) = rotation.to_radians().sin_cos();
        let (dx, dy) = ((x1 - x2) / 2.0, (y1 - y2) / 2.0);
        let (x1p, y1p) = (cos * dx + sin * dy, -sin * dx + cos * dy);
        let lambda = (x1p / rx).powi(2) + (y1p / ry).powi(2);
        if lambda > 1.0 {
            rx *= lambda.sqrt();
            ry *= lambda.sqrt();
        }
        let numerator = rx * rx * ry * ry - rx * rx * y1p * y1p - ry * ry * x1p * x1p;
        let denominator = rx * rx * y1p * y1p + ry * ry * x1p * x1p;
        let mut factor = if denominator == 0.0 {
            0.0
        } else {
            (numerator / denominator).max(0.0).sqrt()
        };
        if large == sweep {
            factor = -factor;
        }
        let (cxp, cyp) = (factor * rx * y1p / ry, -factor * ry * x1p / rx);
        let (cx, cy) = (
            cos * cxp - sin * cyp + (x1 + x2) / 2.0,
            sin * cxp + cos * cyp + (y1 + y2) / 2.0,
        );
        let angle =
            |ux: f32, uy: f32, vx: f32, vy: f32| (ux * vy - uy * vx).atan2(ux * vx + uy * vy);
        let start = angle(1.0, 0.0, (x1p - cxp) / rx, (y1p - cyp) / ry);
        let mut sweep_angle = angle(
            (x1p - cxp) / rx,
            (y1p - cyp) / ry,
            (-x1p - cxp) / rx,
            (-y1p - cyp) / ry,
        );
        if !sweep && sweep_angle > 0.0 {
            sweep_angle -= std::f32::consts::TAU;
        } else if sweep && sweep_angle < 0.0 {
            sweep_angle += std::f32::consts::TAU;
        }
        let pieces = (sweep_angle.abs() / std::f32::consts::FRAC_PI_2)
            .ceil()
            .max(1.0) as usize;
        let step = sweep_angle / pieces as f32;
        let k = 4.0 / 3.0 * (step / 4.0).tan();
        let point = |t: f32| {
            let (s, c) = t.sin_cos();
            (
                cx + rx * c * cos - ry * s * sin,
                cy + rx * c * sin + ry * s * cos,
            )
        };
        let derivative = |t: f32| {
            let (s, c) = t.sin_cos();
            (-rx * s * cos - ry * c * sin, -rx * s * sin + ry * c * cos)
        };
        for i in 0..pieces {
            let (a, b) = (start + step * i as f32, start + step * (i + 1) as f32);
            let (pa, pb) = (point(a), point(b));
            let (da, db) = (derivative(a), derivative(b));
            let end = if i + 1 == pieces { (x2, y2) } else { pb };
            self.cubic_to(
                (pa.0 + k * da.0, pa.1 + k * da.1),
                (pb.0 - k * db.0, pb.1 - k * db.1),
                end,
            );
        }
    }

    fn close(&mut self) {
        self.flush(true);
        self.at = self.start;
        // The next command starts at the sub-path's start.
    }

    fn finish(mut self) -> Shape {
        self.flush(false);
        self.shape
    }

    fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, rx: f32, ry: f32) {
        let (rx, ry) = (rx.min(w / 2.0), ry.min(h / 2.0));
        if rx <= 0.0 || ry <= 0.0 {
            self.move_to(x, y);
            self.line_to(x + w, y);
            self.line_to(x + w, y + h);
            self.line_to(x, y + h);
            self.close();
            return;
        }
        self.move_to(x + rx, y);
        self.line_to(x + w - rx, y);
        self.arc_to(rx, ry, 0.0, false, true, (x + w, y + ry));
        self.line_to(x + w, y + h - ry);
        self.arc_to(rx, ry, 0.0, false, true, (x + w - rx, y + h));
        self.line_to(x + rx, y + h);
        self.arc_to(rx, ry, 0.0, false, true, (x, y + h - ry));
        self.line_to(x, y + ry);
        self.arc_to(rx, ry, 0.0, false, true, (x + rx, y));
        self.close();
    }

    fn ellipse(&mut self, cx: f32, cy: f32, rx: f32, ry: f32) {
        self.move_to(cx + rx, cy);
        self.arc_to(rx, ry, 0.0, false, true, (cx, cy + ry));
        self.arc_to(rx, ry, 0.0, false, true, (cx - rx, cy));
        self.arc_to(rx, ry, 0.0, false, true, (cx, cy - ry));
        self.arc_to(rx, ry, 0.0, false, true, (cx + rx, cy));
        self.close();
    }

    /// The commands of a `d` attribute.
    fn path_data(&mut self, d: &str) {
        let mut p = Scanner::new(d);
        let mut command = b' ';
        let (mut last_cubic, mut last_quad): (Option<(f32, f32)>, Option<(f32, f32)>) =
            (None, None);
        while let Some(c) = p.peek() {
            if c.is_ascii_alphabetic() {
                command = c;
                p.at += 1;
                if matches!(command, b'z' | b'Z') {
                    self.close();
                    (last_cubic, last_quad) = (None, None);
                    continue;
                }
            } else if command == b' ' || matches!(command, b'z' | b'Z') {
                return;
            }
            let relative = command.is_ascii_lowercase();
            let (ox, oy) = if relative { self.at } else { (0.0, 0.0) };
            macro_rules! n {
                () => {
                    match p.number() {
                        Some(n) => n,
                        None => return,
                    }
                };
            }
            let (mut next_cubic, mut next_quad) = (None, None);
            match command.to_ascii_uppercase() {
                b'M' => {
                    let (x, y) = (n!() + ox, n!() + oy);
                    self.move_to(x, y);
                    // More pairs after a move are lines.
                    command = if relative { b'l' } else { b'L' };
                }
                b'L' => {
                    let (x, y) = (n!() + ox, n!() + oy);
                    self.line_to(x, y);
                }
                b'H' => {
                    let x = n!() + ox;
                    self.line_to(x, self.at.1);
                }
                b'V' => {
                    let y = n!() + oy;
                    self.line_to(self.at.0, y);
                }
                b'C' => {
                    let c1 = (n!() + ox, n!() + oy);
                    let c2 = (n!() + ox, n!() + oy);
                    let end = (n!() + ox, n!() + oy);
                    self.cubic_to(c1, c2, end);
                    next_cubic = Some(c2);
                }
                b'S' => {
                    let c2 = (n!() + ox, n!() + oy);
                    let end = (n!() + ox, n!() + oy);
                    let c1 = last_cubic
                        .map_or(self.at, |(x, y)| (2.0 * self.at.0 - x, 2.0 * self.at.1 - y));
                    self.cubic_to(c1, c2, end);
                    next_cubic = Some(c2);
                }
                b'Q' => {
                    let c = (n!() + ox, n!() + oy);
                    let end = (n!() + ox, n!() + oy);
                    self.quad_to(c, end);
                    next_quad = Some(c);
                }
                b'T' => {
                    let end = (n!() + ox, n!() + oy);
                    let c = last_quad
                        .map_or(self.at, |(x, y)| (2.0 * self.at.0 - x, 2.0 * self.at.1 - y));
                    self.quad_to(c, end);
                    next_quad = Some(c);
                }
                b'A' => {
                    let (rx, ry, rotation) = (n!(), n!(), n!());
                    let (Some(large), Some(sweep)) = (p.flag(), p.flag()) else {
                        return;
                    };
                    let end = (n!() + ox, n!() + oy);
                    self.arc_to(rx, ry, rotation, large, sweep, end);
                }
                _ => return,
            }
            (last_cubic, last_quad) = (next_cubic, next_quad);
        }
    }
}

/// Adds the quadrilateral `a b c d` to `path` with a positive winding, so overlapping pieces add up.
fn polygon(path: &mut Path, points: &[(f32, f32)]) {
    let area: f32 = points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .map(|(a, b)| a.0 * b.1 - b.0 * a.1)
        .sum();
    let ordered: Vec<(f32, f32)> = if area < 0.0 {
        points.iter().rev().copied().collect()
    } else {
        points.to_vec()
    };
    path.move_to(ordered[0].0, ordered[0].1);
    for p in &ordered[1..] {
        path.line_to(p.0, p.1);
    }
    path.close();
}

/// The outline of the stroke of `lines` (device space), `width` wide.
fn stroke(shape: &Shape, width: f32, style: &Style) -> Path {
    let half = (width / 2.0).max(0.0);
    let mut path = Path::new();
    for (points, closed) in &shape.lines {
        let mut p: Vec<(f32, f32)> = Vec::with_capacity(points.len());
        for q in points {
            if p.last().is_none_or(|l| (l.0 - q.0).hypot(l.1 - q.1) > 1e-4) {
                p.push(*q);
            }
        }
        if *closed && p.len() > 1 && p.first() == p.last() {
            p.pop();
        }
        if p.len() == 1 {
            // A dot: a round or square cap shows, a butt cap does not.
            let (x, y) = p[0];
            match style.cap {
                Cap::Round => disc(&mut path, x, y, half),
                Cap::Square => polygon(
                    &mut path,
                    &[
                        (x - half, y - half),
                        (x + half, y - half),
                        (x + half, y + half),
                        (x - half, y + half),
                    ],
                ),
                Cap::Butt => {}
            }
            continue;
        }
        let count = p.len();
        let segments = if *closed { count } else { count - 1 };
        let direction = |i: usize| {
            let (a, b) = (p[i % count], p[(i + 1) % count]);
            let len = (b.0 - a.0).hypot(b.1 - a.1);
            ((b.0 - a.0) / len, (b.1 - a.1) / len)
        };
        for i in 0..segments {
            let (a, b) = (p[i % count], p[(i + 1) % count]);
            let (dx, dy) = direction(i);
            let (nx, ny) = (-dy * half, dx * half);
            let (mut a, mut b) = (a, b);
            // Square caps extend the ends of an open line.
            if !*closed && style.cap == Cap::Square {
                if i == 0 {
                    a = (a.0 - dx * half, a.1 - dy * half);
                }
                if i + 1 == segments {
                    b = (b.0 + dx * half, b.1 + dy * half);
                }
            }
            polygon(
                &mut path,
                &[
                    (a.0 + nx, a.1 + ny),
                    (b.0 + nx, b.1 + ny),
                    (b.0 - nx, b.1 - ny),
                    (a.0 - nx, a.1 - ny),
                ],
            );
        }
        if !*closed && style.cap == Cap::Round {
            disc(&mut path, p[0].0, p[0].1, half);
            disc(&mut path, p[count - 1].0, p[count - 1].1, half);
        }
        // Joins at every vertex between two segments.
        let joins = if *closed { 0..count } else { 1..count - 1 };
        for v in joins {
            let (d1, d2) = (direction((v + count - 1) % count), direction(v));
            let cross = d1.0 * d2.1 - d1.1 * d2.0;
            let dot = d1.0 * d2.0 + d1.1 * d2.1;
            if cross.abs() < 1e-4 && dot > 0.0 {
                continue;
            }
            let (x, y) = p[v];
            if style.join == Join::Round {
                disc(&mut path, x, y, half);
                continue;
            }
            // The outer side is the right of a left turn and the left of a right turn.
            let side = if cross > 0.0 { -1.0 } else { 1.0 };
            let (n1, n2) = ((-d1.1, d1.0), (-d2.1, d2.0));
            let a = (x + side * n1.0 * half, y + side * n1.1 * half);
            let b = (x + side * n2.0 * half, y + side * n2.1 * half);
            let n_dot = n1.0 * n2.0 + n1.1 * n2.1;
            let ratio = (2.0 / (1.0 + n_dot).max(1e-6)).sqrt();
            if style.join == Join::Miter && ratio <= style.miter_limit {
                let k = half * side / (1.0 + n_dot);
                let m = (x + (n1.0 + n2.0) * k, y + (n1.1 + n2.1) * k);
                polygon(&mut path, &[(x, y), a, m, b]);
            } else {
                polygon(&mut path, &[(x, y), a, b]);
            }
        }
    }
    path
}

fn disc(path: &mut Path, x: f32, y: f32, r: f32) {
    let n = ((r * 3.0).ceil() as usize).clamp(8, 48);
    let points: Vec<(f32, f32)> = (0..n)
        .map(|i| {
            let t = std::f32::consts::TAU * i as f32 / n as f32;
            (x + r * t.cos(), y + r * t.sin())
        })
        .collect();
    polygon(path, &points);
}

struct Painter<'a> {
    root: &'a Element,
    canvas: Canvas<Vec<u8>>,
    drawn: usize,
}

impl Painter<'_> {
    fn element(&mut self, e: &Element, parent: &Style, m: Matrix, depth: usize) {
        self.drawn += 1;
        if self.drawn > MAX_ELEMENTS || depth > 64 {
            return;
        }
        let hidden = e.attr("display") == Some("none")
            || declarations(e)
                .iter()
                .any(|(k, v)| k == "display" && v == "none")
            || e.attr("visibility") == Some("hidden");
        if hidden {
            return;
        }
        let style = parent.with(e, self.root);
        let m = e
            .attr("transform")
            .map_or(m, |t| m.then_inside(parse_transform(t)));
        let number = |name: &str| e.attr(name).and_then(|v| length(v, 100.0)).unwrap_or(0.0);
        let mut b = Builder::new(m);
        match e.name.as_str() {
            "g" | "a" | "switch" => {
                for child in &e.children {
                    self.element(child, &style, m, depth + 1);
                }
                return;
            }
            "svg" => {
                // A nested svg: its position, and its own viewBox if it has a size.
                let inner = m.then_inside(Matrix([1.0, 0.0, 0.0, 1.0, number("x"), number("y")]));
                for child in &e.children {
                    self.element(child, &style, inner, depth + 1);
                }
                return;
            }
            "use" => {
                let Some(target) = e
                    .attr("href")
                    .and_then(|h| h.strip_prefix('#'))
                    .and_then(|id| find(self.root, id))
                else {
                    return;
                };
                if depth < MAX_USE_DEPTH {
                    let moved =
                        m.then_inside(Matrix([1.0, 0.0, 0.0, 1.0, number("x"), number("y")]));
                    self.element(target, &style, moved, depth + 1);
                }
                return;
            }
            "rect" => {
                let (w, h) = (number("width"), number("height"));
                let (mut rx, mut ry) = (number("rx"), number("ry"));
                if e.attr("rx").is_none() {
                    rx = ry;
                }
                if e.attr("ry").is_none() {
                    ry = rx;
                }
                if w <= 0.0 || h <= 0.0 {
                    return;
                }
                b.rect(number("x"), number("y"), w, h, rx, ry);
            }
            "circle" => {
                let r = number("r");
                if r <= 0.0 {
                    return;
                }
                b.ellipse(number("cx"), number("cy"), r, r);
            }
            "ellipse" => {
                let (rx, ry) = (number("rx"), number("ry"));
                if rx <= 0.0 || ry <= 0.0 {
                    return;
                }
                b.ellipse(number("cx"), number("cy"), rx, ry);
            }
            "line" => {
                b.move_to(number("x1"), number("y1"));
                b.line_to(number("x2"), number("y2"));
            }
            "polyline" | "polygon" => {
                let n = numbers(e.attr("points").unwrap_or_default());
                for (i, xy) in n.chunks_exact(2).enumerate() {
                    if i == 0 {
                        b.move_to(xy[0], xy[1]);
                    } else {
                        b.line_to(xy[0], xy[1]);
                    }
                }
                if e.name == "polygon" {
                    b.close();
                }
            }
            "path" => b.path_data(e.attr("d").unwrap_or_default()),
            _ => return,
        }
        let shape = b.finish();
        if let Some(fill) = style.fill {
            let mut path = Path::new();
            for (points, _) in &shape.lines {
                if points.len() > 2 {
                    path.move_to(points[0].0, points[0].1);
                    for p in &points[1..] {
                        path.line_to(p.0, p.1);
                    }
                    path.close();
                }
            }
            self.canvas.fill_path(
                &path,
                0.0,
                0.0,
                fill.with_opacity(style.fill_opacity * style.opacity),
            );
        }
        if let Some(color) = style.stroke {
            let width = style.stroke_width * m.scale();
            if width > 0.0 {
                self.canvas.fill_path(
                    &stroke(&shape, width, &style),
                    0.0,
                    0.0,
                    color.with_opacity(style.stroke_opacity * style.opacity),
                );
            }
        }
    }
}

/// The picture's natural size in CSS pixels: `width` and `height`, else the `viewBox`, else 300 x 150.
fn natural_size(root: &Element) -> (f32, f32) {
    let view = root
        .attr("viewBox")
        .map(numbers)
        .filter(|v| v.len() == 4 && v[2] > 0.0 && v[3] > 0.0);
    let dimension = |name: &str, from_view: Option<f32>, default: f32| {
        root.attr(name)
            .filter(|v| !v.trim().ends_with('%'))
            .and_then(|v| length(v, 0.0))
            .filter(|v| *v > 0.0)
            .or(from_view)
            .unwrap_or(default)
    };
    (
        dimension("width", view.as_ref().map(|v| v[2]), 300.0),
        dimension("height", view.as_ref().map(|v| v[3]), 150.0),
    )
}

/// The size an SVG file wants, in CSS pixels.
///
/// # Errors
/// The file is not SVG.
pub fn size(svg: &str) -> Result<(f32, f32), Error> {
    let root = xml::parse(svg).map_err(Error::Invalid)?;
    if root.name != "svg" {
        return Err(Error::NotSvg);
    }
    Ok(natural_size(&root))
}

/// Draws an SVG file at `scale` device pixels per CSS pixel, on a transparent background.
///
/// # Errors
/// The XML is damaged, the root is not `svg`, or the picture is too large.
pub fn render(svg: &str, scale: f32) -> Result<Image, Error> {
    let root = xml::parse(svg).map_err(Error::Invalid)?;
    if root.name != "svg" {
        return Err(Error::NotSvg);
    }
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    let (w, h) = natural_size(&root);
    let (pw, ph) = ((w * scale).ceil().max(1.0), (h * scale).ceil().max(1.0));
    if pw > MAX_SIDE || ph > MAX_SIDE || pw * ph > MAX_PIXELS {
        return Err(Error::TooLarge);
    }
    // The view box is fitted into the picture, centred, keeping its proportions.
    let mut m = Matrix([scale, 0.0, 0.0, scale, 0.0, 0.0]);
    if let Some(v) = root
        .attr("viewBox")
        .map(numbers)
        .filter(|v| v.len() == 4 && v[2] > 0.0 && v[3] > 0.0)
    {
        let fit = (w / v[2]).min(h / v[3]);
        let (ox, oy) = ((w - v[2] * fit) / 2.0, (h - v[3] * fit) / 2.0);
        m = m.then_inside(Matrix([
            fit,
            0.0,
            0.0,
            fit,
            ox - v[0] * fit,
            oy - v[1] * fit,
        ]));
    }
    let mut painter = Painter {
        root: &root,
        canvas: Canvas::new(pw as usize, ph as usize),
        drawn: 0,
    };
    let initial = Style::initial().with(&root, &root);
    for child in &root.children {
        painter.element(child, &initial, m, 0);
    }
    // The canvas is premultiplied BGRA; an image is straight RGBA.
    let mut rgba = painter.canvas.bytes().to_vec();
    for px in rgba.chunks_exact_mut(4) {
        let a = u32::from(px[3]);
        let un = |c: u8| {
            if a == 0 {
                0
            } else {
                ((u32::from(c) * 255 + a / 2) / a).min(255) as u8
            }
        };
        let (b, g, r) = (un(px[0]), un(px[1]), un(px[2]));
        px.copy_from_slice(&[r, g, b, px[3]]);
    }
    Ok(Image {
        width: pw as u32,
        height: ph as u32,
        rgba,
    })
}

//! Drawing a page as an SVG picture: the paths, text and images of its content stream, with the colours they are painted in.
//!
//! The text is set in the viewer's own fonts (a PDF's embedded font programs are not read), each glyph placed where the file
//! says, so lines break and align as in the file but letter shapes are those of a similar font. Shadings, patterns, type 3
//! fonts and inline images are left out.

#![allow(
    clippy::many_single_char_names,
    clippy::use_self,
    clippy::missing_const_for_fn,
    clippy::match_same_arms,
    clippy::suboptimal_flops,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::float_cmp,
    clippy::useless_let_if_seq,
    clippy::tuple_array_conversions,
    clippy::needless_pass_by_ref_mut
)]

use crate::doc::{Document, Page};
use crate::font::Font;
use crate::object::{Dict, Object, Parser, Stream};
use std::collections::HashMap;
use std::fmt::Write as _;
use std::rc::Rc;

type Matrix = [f64; 6];

const IDENTITY: Matrix = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

/// The most pixels of an image that is drawn.
const MAX_PIXELS: usize = 16 << 20;
/// How deep forms may nest.
const MAX_DEPTH: usize = 8;

/// A piece of text on a page, for a viewer that lays real text over the picture (to select, copy and search it).
#[derive(Debug, Clone, PartialEq)]
pub struct TextRun {
    pub text: String,
    /// The start of its baseline, in points from the top left of the page as shown.
    pub x: f64,
    pub y: f64,
    /// The size of the letters, and how far the text reaches along the line, in points.
    pub size: f64,
    pub width: f64,
}

/// A page drawn: its size in points (as shown, after rotation), the SVG file, and the text on it. Pages that are rotated, and
/// text that is turned or mirrored, have no runs.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderedPage {
    pub width: f64,
    pub height: f64,
    pub svg: String,
    pub runs: Vec<TextRun>,
    /// The links on the page (none on rotated pages).
    pub links: Vec<crate::outline::PageLink>,
}

/// `a` applied first, then `b`.
fn multiply(a: &Matrix, b: &Matrix) -> Matrix {
    [
        a[1].mul_add(b[2], a[0] * b[0]),
        a[1].mul_add(b[3], a[0] * b[1]),
        a[3].mul_add(b[2], a[2] * b[0]),
        a[3].mul_add(b[3], a[2] * b[1]),
        a[5].mul_add(b[2], a[4] * b[0]) + b[4],
        a[5].mul_add(b[3], a[4] * b[1]) + b[5],
    ]
}

fn num(x: f64) -> String {
    if !x.is_finite() {
        return "0".into();
    }
    let mut s = format!("{x:.4}");
    if s.trim_start_matches('-')
        .trim_matches(['0', '.'])
        .is_empty()
        && x != 0.0
    {
        s = format!("{x:.8}");
    }
    if s.contains('.') {
        s = s.trim_end_matches('0').trim_end_matches('.').to_string();
    }
    if s == "-0" || s.is_empty() {
        s = "0".into();
    }
    s
}

fn matrix_attr(m: &Matrix) -> String {
    format!(
        "matrix({} {} {} {} {} {})",
        num(m[0]),
        num(m[1]),
        num(m[2]),
        num(m[3]),
        num(m[4]),
        num(m[5])
    )
}

fn xml_escape(text: &str, out: &mut String) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\u{0}'..='\u{8}'
            | '\u{b}'
            | '\u{c}'
            | '\u{e}'..='\u{1f}'
            | '\u{fffe}'
            | '\u{ffff}' => {}
            c => out.push(c),
        }
    }
}

fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        out.push(char::from(T[(n >> 18) as usize & 63]));
        out.push(char::from(T[(n >> 12) as usize & 63]));
        out.push(if chunk.len() > 1 {
            char::from(T[(n >> 6) as usize & 63])
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            char::from(T[n as usize & 63])
        } else {
            '='
        });
    }
    out
}

/// A PNG file of `rgba` pixels.
fn png(width: usize, height: usize, rgba: &[u8]) -> Vec<u8> {
    let row = width * 4;
    let mut raw = Vec::with_capacity((row + 1) * height);
    for line in rgba.chunks_exact(row.max(1)) {
        raw.push(0);
        raw.extend_from_slice(line);
    }
    let mut z = vec![0x78, 0x9c];
    z.extend(quench_archive::deflate::compress(&raw));
    z.extend(quench_archive::checksum::adler32(&raw).to_be_bytes());
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    let mut chunk = |kind: &[u8; 4], data: &[u8]| {
        out.extend((data.len() as u32).to_be_bytes());
        let mut body = kind.to_vec();
        body.extend_from_slice(data);
        out.extend_from_slice(&body);
        out.extend(quench_archive::checksum::crc32(&body).to_be_bytes());
    };
    let mut header = Vec::new();
    header.extend((width as u32).to_be_bytes());
    header.extend((height as u32).to_be_bytes());
    header.extend([8, 6, 0, 0, 0]);
    chunk(b"IHDR", &header);
    chunk(b"IDAT", &z);
    chunk(b"IEND", &[]);
    out
}

fn numbers(operands: &[Object]) -> Vec<f64> {
    operands.iter().filter_map(Object::as_f64).collect()
}

// ---------------------------------------------------------------- colours

/// A colour space, as far as turning components into RGB goes.
#[derive(Debug, Clone)]
enum Space {
    Gray,
    Rgb,
    Cmyk,
    Lab,
    /// One tint of an ink (`Separation`), or several (`DeviceN`): drawn as the darkness they add up to.
    Tint(usize),
    Indexed {
        base: Box<Space>,
        hival: usize,
        table: Vec<u8>,
    },
    Pattern,
}

impl Space {
    fn components(&self) -> usize {
        match self {
            Self::Gray | Self::Indexed { .. } => 1,
            Self::Rgb | Self::Lab => 3,
            Self::Cmyk => 4,
            Self::Tint(n) => *n,
            Self::Pattern => 0,
        }
    }

    fn initial(&self) -> Vec<f64> {
        match self {
            Self::Cmyk => vec![0.0, 0.0, 0.0, 1.0],
            Self::Tint(n) => vec![1.0; *n],
            other => vec![0.0; other.components()],
        }
    }

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn rgb(&self, c: &[f64]) -> (u8, u8, u8) {
        let byte = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        let at = |i: usize| c.get(i).copied().unwrap_or(0.0);
        match self {
            Self::Gray => {
                let g = byte(at(0));
                (g, g, g)
            }
            Self::Rgb => (byte(at(0)), byte(at(1)), byte(at(2))),
            Self::Cmyk => {
                let k = 1.0 - at(3).clamp(0.0, 1.0);
                let ch = |v: f64| byte((1.0 - v.clamp(0.0, 1.0)) * k);
                (ch(at(0)), ch(at(1)), ch(at(2)))
            }
            Self::Lab => {
                let g = byte(at(0) / 100.0);
                (g, g, g)
            }
            Self::Tint(n) => {
                let darkest = (0..*n).map(at).fold(0.0, f64::max);
                let g = byte(1.0 - darkest);
                (g, g, g)
            }
            Self::Indexed { base, hival, table } => {
                let i = (at(0).round().max(0.0) as usize).min(*hival);
                let n = base.components().max(1);
                let comps: Vec<f64> = (0..n)
                    .map(|j| {
                        let v = f64::from(*table.get(i * n + j).unwrap_or(&0)) / 255.0;
                        if matches!(**base, Space::Lab) && j == 0 {
                            v * 100.0
                        } else {
                            v
                        }
                    })
                    .collect();
                base.rgb(&comps)
            }
            Self::Pattern => (128, 128, 128),
        }
    }
}

fn hex((r, g, b): (u8, u8, u8)) -> String {
    format!("#{r:02x}{g:02x}{b:02x}")
}

// ---------------------------------------------------------------- fonts

/// How a font is drawn: a family and the weight and slant.
#[derive(Debug, Clone, Copy)]
struct Face {
    family: &'static str,
    bold: bool,
    italic: bool,
}

const SANS: &str = "'Liberation Sans', Arial, Helvetica, sans-serif";
const SERIF: &str = "'Liberation Serif', 'Times New Roman', Times, serif";
const MONO: &str = "'Liberation Mono', 'Courier New', Courier, monospace";

/// The width of a character in Helvetica, in thousandths of the size (a fixed 600 for a monospaced face): what a glyph is
/// given when the file does not say.
fn typical_width(c: char, mono: bool) -> f64 {
    const ASCII: [u16; 95] = [
        278, 278, 355, 556, 556, 889, 667, 222, 333, 333, 389, 584, 278, 333, 278, 278, 556, 556,
        556, 556, 556, 556, 556, 556, 556, 556, 278, 278, 584, 584, 584, 556, 1015, 667, 667, 722,
        722, 667, 611, 778, 722, 278, 500, 667, 556, 833, 722, 778, 667, 778, 722, 667, 611, 722,
        667, 944, 667, 667, 611, 278, 278, 278, 469, 556, 333, 556, 556, 500, 556, 556, 278, 556,
        556, 222, 222, 500, 222, 833, 556, 556, 556, 556, 333, 500, 278, 556, 500, 722, 500, 500,
        500, 334, 260, 334, 584,
    ];
    if mono {
        return 600.0;
    }
    match c {
        ' '..='~' => f64::from(ASCII[c as usize - 32]),
        '\u{a0}' => 278.0,
        _ => 556.0,
    }
}

fn face_of(doc: &Document, dict: &Dict) -> Face {
    let mut dict = dict.clone();
    if let Some(Object::Array(kids)) = doc.get(&dict, "DescendantFonts")
        && let Some(Object::Dict(d)) = kids.first().map(|k| doc.resolve(k))
    {
        let name = doc.get(&dict, "BaseFont");
        dict = d;
        if let Some(n) = name {
            dict.entry("BaseFont".to_string()).or_insert(n);
        }
    }
    let name = doc
        .get(&dict, "BaseFont")
        .and_then(|o| o.as_name().map(str::to_string))
        .unwrap_or_default();
    let name = name
        .split_once('+')
        .map_or(name.as_str(), |(_, n)| n)
        .to_string();
    let lower = name.to_lowercase();
    let flags = match doc.get(&dict, "FontDescriptor") {
        Some(Object::Dict(d)) => doc.get(&d, "Flags").and_then(|f| f.as_int()).unwrap_or(0),
        _ => 0,
    };
    let has = |words: &[&str]| words.iter().any(|w| lower.contains(w));
    let family = if has(&["courier", "mono", "consolas", "typewriter"]) || flags & 1 != 0 {
        MONO
    } else if has(&[
        "times", "serif", "roman", "georgia", "garamond", "palatino", "book", "minion", "cambria",
    ]) && !has(&["sans"])
    {
        SERIF
    } else if has(&["arial", "helvetica", "sans", "calibri", "verdana", "tahoma"]) {
        SANS
    } else if flags & 2 != 0 {
        SERIF
    } else {
        SANS
    };
    Face {
        family,
        bold: has(&["bold", "black", "heavy", "semibold", "demi"]) || flags & 0x40000 != 0,
        italic: has(&["italic", "oblique"]) || flags & 0x40 != 0,
    }
}

// ---------------------------------------------------------------- the drawing

#[derive(Clone)]
struct Gs {
    ctm: Matrix,
    fill: Space,
    stroke: Space,
    fill_color: (u8, u8, u8),
    stroke_color: (u8, u8, u8),
    fill_alpha: f64,
    stroke_alpha: f64,
    line_width: f64,
    cap: i64,
    join: i64,
    miter: f64,
    dash: Vec<f64>,
    dash_phase: f64,
    font: Option<Rc<Font>>,
    face: Face,
    size: f64,
    char_space: f64,
    word_space: f64,
    scale: f64,
    leading: f64,
    rise: f64,
    mode: i64,
}

impl Gs {
    fn new(ctm: Matrix) -> Self {
        Self {
            ctm,
            fill: Space::Gray,
            stroke: Space::Gray,
            fill_color: (0, 0, 0),
            stroke_color: (0, 0, 0),
            fill_alpha: 1.0,
            stroke_alpha: 1.0,
            line_width: 1.0,
            cap: 0,
            join: 0,
            miter: 10.0,
            dash: Vec::new(),
            dash_phase: 0.0,
            font: None,
            face: Face {
                family: SANS,
                bold: false,
                italic: false,
            },
            size: 1.0,
            char_space: 0.0,
            word_space: 0.0,
            scale: 1.0,
            leading: 0.0,
            rise: 0.0,
            mode: 0,
        }
    }
}

struct Renderer<'a> {
    doc: &'a Document,
    fonts: HashMap<String, (Rc<Font>, Face)>,
    spaces: HashMap<String, Space>,
    body: String,
    defs: String,
    next_id: usize,
    runs: Vec<TextRun>,
    /// The top left of the page's box, to turn page coordinates into ones from the top left.
    origin: (f64, f64),
}

impl Renderer<'_> {
    fn id(&mut self) -> usize {
        self.next_id += 1;
        self.next_id
    }

    fn font(&mut self, resources: &Dict, name: &str) -> Option<(Rc<Font>, Face)> {
        let Some(Object::Dict(fonts)) = self.doc.get(resources, "Font") else {
            return None;
        };
        let key = match fonts.get(name) {
            Some(Object::Ref(n, _)) => format!("#{n}"),
            _ => String::new(),
        };
        if !key.is_empty()
            && let Some(f) = self.fonts.get(&key)
        {
            return Some(f.clone());
        }
        let Some(Object::Dict(dict)) = self.doc.get(&fonts, name) else {
            return None;
        };
        let made = (
            Rc::new(Font::load(self.doc, &dict)),
            face_of(self.doc, &dict),
        );
        if !key.is_empty() {
            self.fonts.insert(key, made.clone());
        }
        Some(made)
    }

    fn space(&mut self, object: &Object, resources: &Dict, depth: usize) -> Space {
        if depth > 6 {
            return Space::Gray;
        }
        match object {
            Object::Name(n) => match n.as_str() {
                "DeviceGray" | "G" | "CalGray" => Space::Gray,
                "DeviceRGB" | "RGB" | "CalRGB" => Space::Rgb,
                "DeviceCMYK" | "CMYK" => Space::Cmyk,
                "Pattern" => Space::Pattern,
                other => {
                    if let Some(s) = self.spaces.get(other) {
                        return s.clone();
                    }
                    let found = match self.doc.get(resources, "ColorSpace") {
                        Some(Object::Dict(d)) => self.doc.get(&d, other),
                        _ => None,
                    };
                    let s = found.map_or(Space::Gray, |o| self.space(&o, resources, depth + 1));
                    self.spaces.insert(other.to_string(), s.clone());
                    s
                }
            },
            Object::Array(items) => {
                let first = items.first().map(|o| self.doc.resolve(o));
                match first.as_ref().and_then(Object::as_name) {
                    Some("ICCBased") => {
                        let n = items
                            .get(1)
                            .and_then(|o| match self.doc.resolve(o) {
                                Object::Stream(s) => {
                                    self.doc.get(&s.dict, "N").and_then(|n| n.as_int())
                                }
                                _ => None,
                            })
                            .unwrap_or(3);
                        match n {
                            1 => Space::Gray,
                            4 => Space::Cmyk,
                            _ => Space::Rgb,
                        }
                    }
                    Some("CalGray") => Space::Gray,
                    Some("CalRGB") => Space::Rgb,
                    Some("Lab") => Space::Lab,
                    Some("Separation") => Space::Tint(1),
                    Some("DeviceN") => {
                        let n = match items.get(1).map(|o| self.doc.resolve(o)) {
                            Some(Object::Array(a)) => a.len(),
                            _ => 1,
                        };
                        Space::Tint(n.max(1))
                    }
                    Some("Indexed" | "I") => {
                        let base = items.get(1).map_or(Space::Rgb, |o| {
                            self.space(&self.doc.resolve(o), resources, depth + 1)
                        });
                        let hival = items
                            .get(2)
                            .and_then(|o| self.doc.resolve(o).as_int())
                            .and_then(|v| usize::try_from(v).ok())
                            .unwrap_or(255)
                            .min(255);
                        let table = match items.get(3).map(|o| self.doc.resolve(o)) {
                            Some(Object::String(s)) => s,
                            Some(o @ Object::Stream(_)) => {
                                self.doc.stream_data(&o).unwrap_or_default()
                            }
                            _ => Vec::new(),
                        };
                        Space::Indexed {
                            base: Box::new(base),
                            hival,
                            table,
                        }
                    }
                    Some("Pattern") => Space::Pattern,
                    Some(other) => {
                        self.space(&Object::Name(other.to_string()), resources, depth + 1)
                    }
                    None => Space::Gray,
                }
            }
            _ => Space::Gray,
        }
    }

    fn state_params(&mut self, resources: &Dict, name: &str, state: &mut Gs) {
        let Some(Object::Dict(all)) = self.doc.get(resources, "ExtGState") else {
            return;
        };
        let Some(Object::Dict(d)) = self.doc.get(&all, name) else {
            return;
        };
        let f = |k: &str| self.doc.get(&d, k).and_then(|o| o.as_f64());
        if let Some(v) = f("LW") {
            state.line_width = v;
        }
        if let Some(v) = f("LC") {
            state.cap = v as i64;
        }
        if let Some(v) = f("LJ") {
            state.join = v as i64;
        }
        if let Some(v) = f("ML") {
            state.miter = v;
        }
        if let Some(v) = f("ca") {
            state.fill_alpha = v.clamp(0.0, 1.0);
        }
        if let Some(v) = f("CA") {
            state.stroke_alpha = v.clamp(0.0, 1.0);
        }
        if let Some(Object::Array(a)) = self.doc.get(&d, "D")
            && let (Some(Object::Array(dash)), Some(phase)) = (
                a.first().map(|o| self.doc.resolve(o)),
                a.get(1).map(|o| self.doc.resolve(o)),
            )
        {
            state.dash = dash
                .iter()
                .filter_map(|o| self.doc.resolve(o).as_f64())
                .collect();
            state.dash_phase = phase.as_f64().unwrap_or(0.0);
        }
    }

    #[allow(clippy::too_many_lines)]
    fn run(&mut self, content: &[u8], resources: &Dict, mut state: Gs, depth: usize) {
        let mut parser = Parser::new(content, 0, false);
        let mut operands: Vec<Object> = Vec::new();
        let mut stack: Vec<(Gs, usize)> = Vec::new();
        let (mut tm, mut tlm) = (IDENTITY, IDENTITY);
        let mut path = String::new();
        let (mut cx, mut cy, mut sx, mut sy) = (0.0, 0.0, 0.0, 0.0);
        let mut pending_clip: Option<bool> = None;
        let mut groups = 0_usize;
        let mut ops = 0_usize;

        while let Ok(Some(object)) = parser.parse() {
            let Object::Op(op) = object else {
                operands.push(object);
                if operands.len() > 512 {
                    operands.remove(0);
                }
                continue;
            };
            ops += 1;
            if ops > 4_000_000 {
                break;
            }
            let n = numbers(&operands);
            let paint = |this: &mut Self,
                         state: &Gs,
                         path: &mut String,
                         fill: Option<bool>,
                         stroke: bool| {
                if path.is_empty() {
                    return;
                }
                let mut el = format!(
                    "<path d=\"{path}\" transform=\"{}\"",
                    matrix_attr(&state.ctm)
                );
                match fill {
                    Some(even_odd) => {
                        let _ = write!(el, " fill=\"{}\"", hex(state.fill_color));
                        if state.fill_alpha < 1.0 {
                            let _ = write!(el, " fill-opacity=\"{}\"", num(state.fill_alpha));
                        }
                        if even_odd {
                            el.push_str(" fill-rule=\"evenodd\"");
                        }
                    }
                    None => el.push_str(" fill=\"none\""),
                }
                if stroke {
                    let det = (state.ctm[0] * state.ctm[3] - state.ctm[1] * state.ctm[2])
                        .abs()
                        .sqrt()
                        .max(1e-6);
                    let width = if state.line_width <= 0.0 {
                        0.5 / det
                    } else {
                        state.line_width
                    };
                    let _ = write!(
                        el,
                        " stroke=\"{}\" stroke-width=\"{}\"",
                        hex(state.stroke_color),
                        num(width)
                    );
                    if state.stroke_alpha < 1.0 {
                        let _ = write!(el, " stroke-opacity=\"{}\"", num(state.stroke_alpha));
                    }
                    el.push_str(match state.cap {
                        1 => " stroke-linecap=\"round\"",
                        2 => " stroke-linecap=\"square\"",
                        _ => "",
                    });
                    el.push_str(match state.join {
                        1 => " stroke-linejoin=\"round\"",
                        2 => " stroke-linejoin=\"bevel\"",
                        _ => "",
                    });
                    if (state.miter - 4.0).abs() > f64::EPSILON && state.join == 0 {
                        let _ = write!(el, " stroke-miterlimit=\"{}\"", num(state.miter));
                    }
                    if !state.dash.is_empty() && state.dash.iter().any(|d| *d > 0.0) {
                        let list: Vec<String> = state.dash.iter().map(|d| num(*d)).collect();
                        let _ = write!(
                            el,
                            " stroke-dasharray=\"{}\" stroke-dashoffset=\"{}\"",
                            list.join(" "),
                            num(state.dash_phase)
                        );
                    }
                }
                el.push_str("/>");
                this.body.push_str(&el);
            };
            match op.as_str() {
                "q" => stack.push((state.clone(), groups)),
                "Q" => {
                    if let Some((s, g)) = stack.pop() {
                        state = s;
                        while groups > g {
                            self.body.push_str("</g>");
                            groups -= 1;
                        }
                    }
                }
                "cm" if n.len() >= 6 => {
                    state.ctm = multiply(&[n[0], n[1], n[2], n[3], n[4], n[5]], &state.ctm);
                }
                "w" if !n.is_empty() => state.line_width = n[0],
                "J" if !n.is_empty() => state.cap = n[0] as i64,
                "j" if !n.is_empty() => state.join = n[0] as i64,
                "M" if !n.is_empty() => state.miter = n[0],
                "d" => {
                    if let Some(Object::Array(a)) = operands.first() {
                        state.dash = numbers(a);
                        state.dash_phase = n.last().copied().unwrap_or(0.0);
                        if operands.len() < 2 {
                            state.dash_phase = 0.0;
                        }
                    }
                }
                "gs" => {
                    if let Some(Object::Name(name)) = operands.first() {
                        self.state_params(resources, name, &mut state);
                    }
                }
                // ---- paths
                "m" if n.len() >= 2 => {
                    let _ = write!(path, "M{} {} ", num(n[0]), num(n[1]));
                    (cx, cy, sx, sy) = (n[0], n[1], n[0], n[1]);
                }
                "l" if n.len() >= 2 => {
                    let _ = write!(path, "L{} {} ", num(n[0]), num(n[1]));
                    (cx, cy) = (n[0], n[1]);
                }
                "c" if n.len() >= 6 => {
                    let _ = write!(
                        path,
                        "C{} {} {} {} {} {} ",
                        num(n[0]),
                        num(n[1]),
                        num(n[2]),
                        num(n[3]),
                        num(n[4]),
                        num(n[5])
                    );
                    (cx, cy) = (n[4], n[5]);
                }
                "v" if n.len() >= 4 => {
                    let _ = write!(
                        path,
                        "C{} {} {} {} {} {} ",
                        num(cx),
                        num(cy),
                        num(n[0]),
                        num(n[1]),
                        num(n[2]),
                        num(n[3])
                    );
                    (cx, cy) = (n[2], n[3]);
                }
                "y" if n.len() >= 4 => {
                    let _ = write!(
                        path,
                        "C{} {} {} {} {} {} ",
                        num(n[0]),
                        num(n[1]),
                        num(n[2]),
                        num(n[3]),
                        num(n[2]),
                        num(n[3])
                    );
                    (cx, cy) = (n[2], n[3]);
                }
                "h" => {
                    path.push_str("Z ");
                    (cx, cy) = (sx, sy);
                }
                "re" if n.len() >= 4 => {
                    let (x, y, w, h) = (n[0], n[1], n[2], n[3]);
                    let _ = write!(
                        path,
                        "M{} {} L{} {} L{} {} L{} {} Z ",
                        num(x),
                        num(y),
                        num(x + w),
                        num(y),
                        num(x + w),
                        num(y + h),
                        num(x),
                        num(y + h)
                    );
                    (cx, cy, sx, sy) = (x, y, x, y);
                }
                "W" => pending_clip = Some(false),
                "W*" => pending_clip = Some(true),
                "S" | "s" | "f" | "F" | "f*" | "B" | "B*" | "b" | "b*" | "n" => {
                    if matches!(op.as_str(), "s" | "b" | "b*") {
                        path.push_str("Z ");
                    }
                    let fill = match op.as_str() {
                        "f" | "F" | "B" | "b" => Some(false),
                        "f*" | "B*" | "b*" => Some(true),
                        _ => None,
                    };
                    let stroke = matches!(op.as_str(), "S" | "s" | "B" | "B*" | "b" | "b*");
                    if fill.is_some() || stroke {
                        paint(self, &state, &mut path, fill, stroke);
                    }
                    if let Some(even_odd) = pending_clip.take()
                        && !path.is_empty()
                    {
                        let id = self.id();
                        let _ = write!(
                            self.defs,
                            "<clipPath id=\"c{id}\"><path d=\"{path}\" transform=\"{}\" clip-rule=\"{}\"/></clipPath>",
                            matrix_attr(&state.ctm),
                            if even_odd { "evenodd" } else { "nonzero" }
                        );
                        let _ = write!(self.body, "<g clip-path=\"url(#c{id})\">");
                        groups += 1;
                    }
                    path.clear();
                }
                // ---- colours
                "g" if !n.is_empty() => {
                    state.fill = Space::Gray;
                    state.fill_color = state.fill.rgb(&n);
                }
                "G" if !n.is_empty() => {
                    state.stroke = Space::Gray;
                    state.stroke_color = state.stroke.rgb(&n);
                }
                "rg" if n.len() >= 3 => {
                    state.fill = Space::Rgb;
                    state.fill_color = state.fill.rgb(&n);
                }
                "RG" if n.len() >= 3 => {
                    state.stroke = Space::Rgb;
                    state.stroke_color = state.stroke.rgb(&n);
                }
                "k" if n.len() >= 4 => {
                    state.fill = Space::Cmyk;
                    state.fill_color = state.fill.rgb(&n);
                }
                "K" if n.len() >= 4 => {
                    state.stroke = Space::Cmyk;
                    state.stroke_color = state.stroke.rgb(&n);
                }
                "cs" | "CS" => {
                    if let Some(o) = operands.first() {
                        let space = self.space(o, resources, 0);
                        let color = space.rgb(&space.initial());
                        if op == "cs" {
                            (state.fill, state.fill_color) = (space, color);
                        } else {
                            (state.stroke, state.stroke_color) = (space, color);
                        }
                    }
                }
                "sc" | "scn" if !n.is_empty() => state.fill_color = state.fill.rgb(&n),
                "SC" | "SCN" if !n.is_empty() => state.stroke_color = state.stroke.rgb(&n),
                // ---- text
                "BT" => {
                    tm = IDENTITY;
                    tlm = IDENTITY;
                }
                "Tf" => {
                    if let (Some(Object::Name(name)), Some(size)) =
                        (operands.first(), operands.get(1).and_then(Object::as_f64))
                    {
                        if let Some((font, face)) = self.font(resources, name) {
                            state.font = Some(font);
                            state.face = face;
                        }
                        state.size = size;
                    }
                }
                "Tc" if !n.is_empty() => state.char_space = n[0],
                "Tw" if !n.is_empty() => state.word_space = n[0],
                "Tz" if !n.is_empty() => state.scale = n[0] / 100.0,
                "TL" if !n.is_empty() => state.leading = n[0],
                "Ts" if !n.is_empty() => state.rise = n[0],
                "Tr" if !n.is_empty() => state.mode = n[0] as i64,
                "Td" if n.len() >= 2 => {
                    tlm = multiply(&[1.0, 0.0, 0.0, 1.0, n[0], n[1]], &tlm);
                    tm = tlm;
                }
                "TD" if n.len() >= 2 => {
                    state.leading = -n[1];
                    tlm = multiply(&[1.0, 0.0, 0.0, 1.0, n[0], n[1]], &tlm);
                    tm = tlm;
                }
                "Tm" if n.len() >= 6 => {
                    tm = [n[0], n[1], n[2], n[3], n[4], n[5]];
                    tlm = tm;
                }
                "T*" => {
                    tlm = multiply(&[1.0, 0.0, 0.0, 1.0, 0.0, -state.leading], &tlm);
                    tm = tlm;
                }
                "Tj" => {
                    if let Some(Object::String(s)) = operands.first() {
                        self.show(&[Piece::Text(s)], &state, &mut tm);
                    }
                }
                "'" | "\"" => {
                    tlm = multiply(&[1.0, 0.0, 0.0, 1.0, 0.0, -state.leading], &tlm);
                    tm = tlm;
                    if op == "\"" && n.len() >= 2 {
                        state.word_space = n[0];
                        state.char_space = n[1];
                    }
                    if let Some(Object::String(s)) = operands.last() {
                        self.show(&[Piece::Text(s)], &state, &mut tm);
                    }
                }
                "TJ" => {
                    if let Some(Object::Array(items)) = operands.first() {
                        let pieces: Vec<Piece> = items
                            .iter()
                            .filter_map(|i| match i {
                                Object::String(s) => Some(Piece::Text(s)),
                                other => other.as_f64().map(Piece::Move),
                            })
                            .collect();
                        self.show(&pieces, &state, &mut tm);
                    }
                }
                // ---- things drawn
                "Do" => {
                    if let Some(Object::Name(name)) = operands.first() {
                        self.xobject(resources, name, &state, depth);
                    }
                }
                "BI" => {
                    // An inline image is skipped: its dictionary and its data.
                    while let Ok(Some(o)) = parser.parse() {
                        if matches!(&o, Object::Op(w) if w == "ID") {
                            break;
                        }
                    }
                    let mut at = parser.pos + 1;
                    while at + 2 <= content.len() {
                        if &content[at..at + 2] == b"EI"
                            && content
                                .get(at.wrapping_sub(1))
                                .is_some_and(u8::is_ascii_whitespace)
                            && content.get(at + 2).is_none_or(u8::is_ascii_whitespace)
                        {
                            break;
                        }
                        at += 1;
                    }
                    parser.pos = (at + 2).min(content.len());
                }
                _ => {}
            }
            operands.clear();
        }
        while groups > 0 {
            self.body.push_str("</g>");
            groups -= 1;
        }
    }

    /// Show the strings and spacing adjustments of one `Tj` or `TJ`.
    fn show(&mut self, pieces: &[Piece], state: &Gs, tm: &mut Matrix) {
        let Some(font) = state.font.clone() else {
            return;
        };
        // The glyphs and where each starts along the line, in text space before the horizontal scale.
        let mut glyphs: Vec<(String, f64, f64)> = Vec::new();
        let mut x = 0.0;
        for piece in pieces {
            match piece {
                Piece::Move(adjust) => x -= adjust / 1000.0 * state.size,
                Piece::Text(bytes) => {
                    for g in font.decode(bytes) {
                        // A glyph the file gives no width for (a font whose widths sit in its font program, which is not read) takes
                        // a typical one, so what follows is not drawn over it.
                        let width = if font.has_widths() && g.width > 0.0 {
                            g.width
                        } else {
                            typical_width(
                                g.text.chars().next().unwrap_or(' '),
                                state.face.family == MONO,
                            )
                        };
                        let advance = width / 1000.0 * state.size;
                        let space = if g.is_space { state.word_space } else { 0.0 };
                        glyphs.push((g.text, x, advance));
                        x += advance + state.char_space + space;
                    }
                }
            }
        }
        let start = *tm;
        *tm = multiply(&[1.0, 0.0, 0.0, 1.0, x * state.scale, 0.0], &start);
        // Invisible text (the layer of a scan's text) and clipping modes draw nothing.
        if matches!(state.mode, 3 | 7) || state.size == 0.0 {
            return;
        }
        let visible: Vec<&(String, f64, f64)> = glyphs
            .iter()
            .filter(|(t, ..)| !t.trim().is_empty())
            .collect();
        if visible.is_empty() {
            return;
        }
        // The page is drawn upside down (PDF's y goes up), so the text is turned over to stand up.
        let m = multiply(
            &[state.scale, 0.0, 0.0, -1.0, 0.0, 0.0],
            &multiply(&start, &state.ctm),
        );
        let fill = !matches!(state.mode, 1 | 5);
        let stroke = matches!(state.mode, 1 | 2 | 5 | 6);
        let mut el = format!(
            "<text transform=\"{}\" font-family=\"{}\" font-size=\"{}\"",
            matrix_attr(&m),
            state.face.family,
            num(state.size.abs())
        );
        if state.face.bold {
            el.push_str(" font-weight=\"bold\"");
        }
        if state.face.italic {
            el.push_str(" font-style=\"italic\"");
        }
        let _ = write!(
            el,
            " fill=\"{}\"",
            if fill {
                hex(state.fill_color)
            } else {
                "none".into()
            }
        );
        if fill && state.fill_alpha < 1.0 {
            let _ = write!(el, " fill-opacity=\"{}\"", num(state.fill_alpha));
        }
        if stroke {
            let _ = write!(
                el,
                " stroke=\"{}\" stroke-width=\"{}\"",
                hex(state.stroke_color),
                num(state.line_width)
            );
        }
        let _ = write!(el, " y=\"{}\" xml:space=\"preserve\">", num(-state.rise));
        // A font the file gives no widths for is set by the viewer's own (Helvetica, Times and Courier widths are those of the
        // fonts used instead), so the run flows as it comes; otherwise every glyph is put where the file says.
        if !font.has_widths() {
            let from = glyphs
                .iter()
                .position(|g| !g.0.trim().is_empty())
                .unwrap_or(0);
            let to = glyphs
                .iter()
                .rposition(|g| !g.0.trim().is_empty())
                .unwrap_or(0);
            let _ = write!(el, "<tspan x=\"{}\">", num(glyphs[from].1));
            for g in &glyphs[from..=to] {
                xml_escape(&g.0, &mut el);
            }
            el.push_str("</tspan>");
        } else if visible
            .iter()
            .all(|(t, ..)| t.chars().count() == 1 && t.chars().all(|c| c.len_utf16() == 1))
        {
            let list: Vec<String> = visible.iter().map(|(_, x, _)| num(*x)).collect();
            let _ = write!(el, "<tspan x=\"{}\">", list.join(" "));
            for (t, ..) in &visible {
                xml_escape(t, &mut el);
            }
            el.push_str("</tspan>");
        } else {
            for (t, x, _) in &visible {
                let _ = write!(el, "<tspan x=\"{}\">", num(*x));
                xml_escape(t, &mut el);
                el.push_str("</tspan>");
            }
        }
        el.push_str("</text>");
        self.body.push_str(&el);
        // The same text for a viewer to lay over the picture, when it runs left to right across the page.
        let placed = multiply(
            &multiply(&[state.scale, 0.0, 0.0, 1.0, 0.0, 0.0], &start),
            &state.ctm,
        );
        if placed[0] > 0.0
            && placed[1].abs() < placed[0] * 0.01
            && placed[2].abs() < placed[3].abs() * 0.01
        {
            let (first, last) = (visible[0], visible[visible.len() - 1]);
            let (lx, ly) = (first.1, state.rise);
            let (px, py) = (
                placed[0].mul_add(lx, placed[2] * ly) + placed[4],
                placed[1].mul_add(lx, placed[3] * ly) + placed[5],
            );
            let text: String = glyphs
                .iter()
                .skip_while(|g| g.0.trim().is_empty())
                .map(|g| g.0.as_str())
                .collect::<String>()
                .trim_end()
                .to_string();
            self.runs.push(TextRun {
                text,
                x: px - self.origin.0,
                y: self.origin.1 - py,
                size: state.size.abs() * placed[3].abs(),
                width: (last.1 + last.2 - first.1) * placed[0],
            });
        }
    }

    fn xobject(&mut self, resources: &Dict, name: &str, state: &Gs, depth: usize) {
        if depth >= MAX_DEPTH {
            return;
        }
        let Some(Object::Dict(xobjects)) = self.doc.get(resources, "XObject") else {
            return;
        };
        let Some(object) = xobjects.get(name) else {
            return;
        };
        let Object::Stream(stream) = self.doc.resolve(object) else {
            return;
        };
        match stream
            .dict
            .get("Subtype")
            .map(|o| self.doc.resolve(o))
            .as_ref()
            .and_then(Object::as_name)
        {
            Some("Image") => self.image(&stream, resources, state),
            Some("Form") => {
                let Some(data) = self.doc.stream_data(object) else {
                    return;
                };
                let floats = |key: &str| -> Option<Vec<f64>> {
                    match self.doc.get(&stream.dict, key) {
                        Some(Object::Array(m)) => Some(
                            m.iter()
                                .filter_map(|o| self.doc.resolve(o).as_f64())
                                .collect(),
                        ),
                        _ => None,
                    }
                };
                let matrix = floats("Matrix")
                    .filter(|v| v.len() >= 6)
                    .map_or(IDENTITY, |v| [v[0], v[1], v[2], v[3], v[4], v[5]]);
                let ctm = multiply(&matrix, &state.ctm);
                let form_resources = match self.doc.get(&stream.dict, "Resources") {
                    Some(Object::Dict(d)) => d,
                    _ => resources.clone(),
                };
                let mut opened = false;
                if let Some(b) = floats("BBox").filter(|v| v.len() >= 4) {
                    let id = self.id();
                    let (x, y) = (b[0].min(b[2]), b[1].min(b[3]));
                    let (w, h) = ((b[2] - b[0]).abs(), (b[3] - b[1]).abs());
                    let _ = write!(
                        self.defs,
                        "<clipPath id=\"c{id}\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" transform=\"{}\"/></clipPath>",
                        num(x),
                        num(y),
                        num(w),
                        num(h),
                        matrix_attr(&ctm)
                    );
                    let _ = write!(self.body, "<g clip-path=\"url(#c{id})\">");
                    opened = true;
                }
                let inner = Gs {
                    ctm,
                    ..state.clone()
                };
                self.run(&data, &form_resources, inner, depth + 1);
                if opened {
                    self.body.push_str("</g>");
                }
            }
            _ => {}
        }
    }

    /// An image: the unit square under the current matrix, filled with the picture.
    #[allow(
        clippy::too_many_lines,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    fn image(&mut self, stream: &Stream, resources: &Dict, state: &Gs) {
        let d = &stream.dict;
        let int = |k: &str, k2: &str| {
            self.doc
                .get(d, k)
                .or_else(|| self.doc.get(d, k2))
                .and_then(|o| o.as_int())
                .and_then(|v| usize::try_from(v).ok())
        };
        let (Some(w), Some(h)) = (int("Width", "W"), int("Height", "H")) else {
            return;
        };
        if w == 0 || h == 0 || w * h > MAX_PIXELS {
            return;
        }
        let Ok((data, jpeg)) = self.doc.decode_image(stream) else {
            return;
        };
        let place = |uri: String| {
            format!(
                "<image transform=\"{}\" width=\"1\" height=\"1\" preserveAspectRatio=\"none\" href=\"{uri}\"/>",
                matrix_attr(&multiply(&[1.0, 0.0, 0.0, -1.0, 0.0, 1.0], &state.ctm))
            )
        };
        let opacity = |el: String| {
            if state.fill_alpha < 1.0 {
                format!("<g opacity=\"{}\">{el}</g>", num(state.fill_alpha))
            } else {
                el
            }
        };
        if jpeg {
            let el = place(format!("data:image/jpeg;base64,{}", base64(&data)));
            self.body.push_str(&opacity(el));
            return;
        }
        let bpc = int("BitsPerComponent", "BPC").unwrap_or(8);
        let stencil = matches!(
            self.doc
                .get(d, "ImageMask")
                .or_else(|| self.doc.get(d, "IM")),
            Some(Object::Bool(true))
        );
        let decode: Vec<f64> = match self.doc.get(d, "Decode").or_else(|| self.doc.get(d, "D")) {
            Some(Object::Array(a)) => a
                .iter()
                .filter_map(|o| self.doc.resolve(o).as_f64())
                .collect(),
            _ => Vec::new(),
        };
        let mut rgba = vec![255_u8; w * h * 4];
        if stencil {
            // A stencil paints the fill colour where the sample is 0 (or 1 when decoded the other way round).
            let invert = decode.first().is_some_and(|v| *v > 0.5);
            let (r, g, b) = state.fill_color;
            let row = w.div_ceil(8);
            for y in 0..h {
                for x in 0..w {
                    let bit = data
                        .get(y * row + x / 8)
                        .map_or(1, |byte| (byte >> (7 - x % 8)) & 1);
                    let paint = (bit == 0) != invert;
                    let at = (y * w + x) * 4;
                    rgba[at..at + 4].copy_from_slice(&[r, g, b, if paint { 255 } else { 0 }]);
                }
            }
        } else {
            let space_object = self
                .doc
                .get(d, "ColorSpace")
                .or_else(|| self.doc.get(d, "CS"));
            let space = space_object.map_or(Space::Gray, |o| self.space(&o, resources, 0));
            let comps = space.components().max(1);
            if !matches!(bpc, 1 | 2 | 4 | 8 | 16) {
                return;
            }
            let row_bits = w * comps * bpc;
            let row_bytes = row_bits.div_ceil(8);
            let max = f64::from((1_u32 << bpc.min(16)) - 1);
            let indexed = matches!(space, Space::Indexed { .. });
            for y in 0..h {
                let line = data.get(y * row_bytes..).unwrap_or(&[]);
                for x in 0..w {
                    let mut c = [0.0_f64; 8];
                    for (i, slot) in c.iter_mut().enumerate().take(comps.min(8)) {
                        let sample = (x * comps + i) * bpc;
                        let value = match bpc {
                            8 => f64::from(*line.get(sample / 8).unwrap_or(&0)),
                            16 => {
                                f64::from(*line.get(sample / 8).unwrap_or(&0)) * 256.0
                                    + f64::from(*line.get(sample / 8 + 1).unwrap_or(&0))
                            }
                            _ => {
                                let byte = *line.get(sample / 8).unwrap_or(&0);
                                let shift = 8 - bpc - sample % 8;
                                f64::from((byte >> shift) & ((1 << bpc) - 1))
                            }
                        };
                        *slot = if indexed {
                            value
                        } else {
                            let (lo, hi) = (
                                decode.get(i * 2).copied().unwrap_or(0.0),
                                decode.get(i * 2 + 1).copied().unwrap_or(1.0),
                            );
                            lo + value / max * (hi - lo)
                        };
                    }
                    let (r, g, b) = space.rgb(&c[..comps.min(8)]);
                    let at = (y * w + x) * 4;
                    rgba[at..at + 3].copy_from_slice(&[r, g, b]);
                }
            }
        }
        // A soft mask is the alpha channel (taken at the picture's own size).
        if let Some(Object::Stream(mask)) = self.doc.get(d, "SMask") {
            let (mw, mh) = (
                self.doc
                    .get(&mask.dict, "Width")
                    .and_then(|o| o.as_int())
                    .and_then(|v| usize::try_from(v).ok())
                    .unwrap_or(0),
                self.doc
                    .get(&mask.dict, "Height")
                    .and_then(|o| o.as_int())
                    .and_then(|v| usize::try_from(v).ok())
                    .unwrap_or(0),
            );
            let mbpc = self
                .doc
                .get(&mask.dict, "BitsPerComponent")
                .and_then(|o| o.as_int())
                .unwrap_or(8);
            if let Ok((alpha, false)) = self.doc.decode_image(&mask)
                && mbpc == 8
                && mw > 0
                && mh > 0
                && alpha.len() >= mw * mh
            {
                for y in 0..h {
                    for x in 0..w {
                        let (mx, my) = (x * mw / w, y * mh / h);
                        rgba[(y * w + x) * 4 + 3] = alpha[my * mw + mx];
                    }
                }
            }
        }
        let el = place(format!(
            "data:image/png;base64,{}",
            base64(&png(w, h, &rgba))
        ));
        self.body.push_str(&opacity(el));
    }
}

enum Piece<'a> {
    Text(&'a [u8]),
    Move(f64),
}

fn rect_of(doc: &Document, page: &Page, key: &str) -> Option<[f64; 4]> {
    let Some(Object::Array(a)) = doc.get(&page.dict, key) else {
        return None;
    };
    let v: Vec<f64> = a.iter().filter_map(|o| doc.resolve(o).as_f64()).collect();
    (v.len() >= 4).then(|| {
        [
            v[0].min(v[2]),
            v[1].min(v[3]),
            v[0].max(v[2]),
            v[1].max(v[3]),
        ]
    })
}

impl Document {
    /// How many pages the file has.
    #[must_use]
    pub fn page_count(&self) -> usize {
        self.pages().len()
    }

    /// Draw page `index` (from 0) as an SVG picture, or `None` when there is no such page.
    #[must_use]
    pub fn render_page(&self, index: usize) -> Option<RenderedPage> {
        let pages = self.pages();
        let page = pages.get(index)?;
        let [x0, y0, x1, y1] = rect_of(self, page, "CropBox")
            .or_else(|| rect_of(self, page, "MediaBox"))
            .unwrap_or([0.0, 0.0, 612.0, 792.0]);
        let (w, h) = ((x1 - x0).max(1.0), (y1 - y0).max(1.0));
        let rotate = self
            .get(&page.dict, "Rotate")
            .and_then(|r| r.as_int())
            .unwrap_or(0)
            .rem_euclid(360);
        let resources = match self.get(&page.dict, "Resources") {
            Some(Object::Dict(d)) => d,
            _ => Dict::new(),
        };
        let content = self.page_contents(page);
        let mut r = Renderer {
            doc: self,
            fonts: HashMap::new(),
            spaces: HashMap::new(),
            body: String::new(),
            defs: String::new(),
            next_id: 0,
            runs: Vec::new(),
            origin: (x0, y1),
        };
        r.run(&content, &resources, Gs::new(IDENTITY), 0);
        let (width, height, turn) = match rotate {
            90 => (h, w, format!("matrix(0 1 -1 0 {} 0) ", num(h))),
            180 => (w, h, format!("matrix(-1 0 0 -1 {} {}) ", num(w), num(h))),
            270 => (h, w, format!("matrix(0 -1 1 0 0 {}) ", num(w))),
            _ => (w, h, String::new()),
        };
        let svg = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\"><defs>{defs}</defs><rect width=\"{w}\" height=\"{h}\" fill=\"#fff\"/><g transform=\"{turn}matrix(1 0 0 -1 {} {})\">{body}</g></svg>",
            num(-x0),
            num(y1),
            w = num(width),
            h = num(height),
            defs = r.defs,
            body = r.body,
        );
        let runs = if rotate == 0 { r.runs } else { Vec::new() };
        Some(RenderedPage {
            width,
            height,
            svg,
            runs,
            links: if rotate == 0 {
                self.links_of(index, (x0, y1))
            } else {
                Vec::new()
            },
        })
    }
}

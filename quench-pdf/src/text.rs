//! Running a page's content stream to collect the text it shows, in reading order.

use crate::doc::{Document, Page};
use crate::font::Font;
use crate::object::{Dict, Object, Parser};
use std::collections::HashMap;
use std::rc::Rc;

type Matrix = [f64; 6];

const IDENTITY: Matrix = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

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

const fn translate(tx: f64, ty: f64) -> Matrix {
    [1.0, 0.0, 0.0, 1.0, tx, ty]
}

/// A shown glyph in page space.
struct Placed {
    text: String,
    /// Position along the baseline, and across it (the line the glyph is on).
    along: f64,
    cross: f64,
    end: f64,
    size: f64,
    /// Whether the text runs up or down the page rather than across it.
    vertical: bool,
}

#[derive(Clone)]
struct State {
    ctm: Matrix,
    font: Option<Rc<Font>>,
    size: f64,
    char_space: f64,
    word_space: f64,
    scale: f64,
    leading: f64,
    rise: f64,
}

impl Default for State {
    fn default() -> Self {
        Self {
            ctm: IDENTITY,
            font: None,
            size: 1.0,
            char_space: 0.0,
            word_space: 0.0,
            scale: 1.0,
            leading: 0.0,
            rise: 0.0,
        }
    }
}

struct Interpreter<'a> {
    doc: &'a Document,
    fonts: HashMap<String, Rc<Font>>,
    placed: Vec<Placed>,
}

fn numbers(operands: &[Object]) -> Vec<f64> {
    operands.iter().filter_map(Object::as_f64).collect()
}

impl Interpreter<'_> {
    fn font(&mut self, resources: &Dict, name: &str) -> Option<Rc<Font>> {
        let Some(Object::Dict(fonts)) = self.doc.get(resources, "Font") else {
            return None;
        };
        // Fonts are shared between pages, so cache by the object they come from when there is one.
        let key = match fonts.get(name) {
            Some(Object::Ref(n, _)) => format!("#{n}"),
            _ => String::new(),
        };
        if !key.is_empty()
            && let Some(f) = self.fonts.get(&key)
        {
            return Some(Rc::clone(f));
        }
        let Some(Object::Dict(dict)) = self.doc.get(&fonts, name) else {
            return None;
        };
        let font = Rc::new(Font::load(self.doc, &dict));
        if !key.is_empty() {
            self.fonts.insert(key, Rc::clone(&font));
        }
        Some(font)
    }

    #[allow(clippy::too_many_lines)]
    fn run(&mut self, content: &[u8], resources: &Dict, base: Matrix, depth: usize) {
        let mut parser = Parser::new(content, 0, false);
        let mut operands: Vec<Object> = Vec::new();
        let mut state = State {
            ctm: base,
            ..State::default()
        };
        let mut stack: Vec<State> = Vec::new();
        let (mut tm, mut tlm) = (IDENTITY, IDENTITY);

        while let Ok(Some(object)) = parser.parse() {
            let Object::Op(op) = object else {
                operands.push(object);
                if operands.len() > 512 {
                    operands.remove(0);
                }
                continue;
            };
            let n = numbers(&operands);
            match op.as_str() {
                "q" => stack.push(state.clone()),
                "Q" => {
                    if let Some(s) = stack.pop() {
                        state = s;
                    }
                }
                "cm" if n.len() >= 6 => {
                    let m = [n[0], n[1], n[2], n[3], n[4], n[5]];
                    state.ctm = multiply(&m, &state.ctm);
                }
                "BT" => {
                    tm = IDENTITY;
                    tlm = IDENTITY;
                }
                "Tf" => {
                    if let (Some(Object::Name(name)), Some(size)) =
                        (operands.first(), operands.get(1).and_then(Object::as_f64))
                    {
                        state.font = self.font(resources, name);
                        state.size = size;
                    }
                }
                "Tc" if !n.is_empty() => state.char_space = n[0],
                "Tw" if !n.is_empty() => state.word_space = n[0],
                "Tz" if !n.is_empty() => state.scale = n[0] / 100.0,
                "TL" if !n.is_empty() => state.leading = n[0],
                "Ts" if !n.is_empty() => state.rise = n[0],
                "Td" if n.len() >= 2 => {
                    tlm = multiply(&translate(n[0], n[1]), &tlm);
                    tm = tlm;
                }
                "TD" if n.len() >= 2 => {
                    state.leading = -n[1];
                    tlm = multiply(&translate(n[0], n[1]), &tlm);
                    tm = tlm;
                }
                "Tm" if n.len() >= 6 => {
                    tm = [n[0], n[1], n[2], n[3], n[4], n[5]];
                    tlm = tm;
                }
                "T*" => {
                    tlm = multiply(&translate(0.0, -state.leading), &tlm);
                    tm = tlm;
                }
                "Tj" => {
                    if let Some(Object::String(s)) = operands.first() {
                        self.show(s, &state, &mut tm);
                    }
                }
                "'" | "\"" => {
                    tlm = multiply(&translate(0.0, -state.leading), &tlm);
                    tm = tlm;
                    if op == "\"" && n.len() >= 2 {
                        state.word_space = n[0];
                        state.char_space = n[1];
                    }
                    if let Some(Object::String(s)) = operands.last() {
                        self.show(s, &state, &mut tm);
                    }
                }
                "TJ" => {
                    if let Some(Object::Array(items)) = operands.first() {
                        for item in items {
                            match item {
                                Object::String(s) => self.show(s, &state, &mut tm),
                                other => {
                                    if let Some(adjust) = other.as_f64() {
                                        let tx = -adjust / 1000.0 * state.size * state.scale;
                                        tm = multiply(&translate(tx, 0.0), &tm);
                                    }
                                }
                            }
                        }
                    }
                }
                "Do" => {
                    if depth < 8
                        && let Some(Object::Name(name)) = operands.first()
                    {
                        self.form(resources, name, &state, depth);
                    }
                }
                "BI" => {
                    // Inline image: skip its dictionary and data.
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
    }

    fn form(&mut self, resources: &Dict, name: &str, state: &State, depth: usize) {
        let Some(Object::Dict(xobjects)) = self.doc.get(resources, "XObject") else {
            return;
        };
        let Some(object) = xobjects.get(name) else {
            return;
        };
        let Object::Stream(stream) = self.doc.resolve(object) else {
            return;
        };
        if stream.dict.get("Subtype").and_then(Object::as_name) != Some("Form") {
            return;
        }
        let Some(data) = self.doc.stream_data(object) else {
            return;
        };
        let matrix = match self.doc.get(&stream.dict, "Matrix") {
            Some(Object::Array(m)) if m.len() >= 6 => {
                let v: Vec<f64> = m
                    .iter()
                    .filter_map(|o| self.doc.resolve(o).as_f64())
                    .collect();
                if v.len() >= 6 {
                    [v[0], v[1], v[2], v[3], v[4], v[5]]
                } else {
                    IDENTITY
                }
            }
            _ => IDENTITY,
        };
        let form_resources = match self.doc.get(&stream.dict, "Resources") {
            Some(Object::Dict(d)) => d,
            _ => resources.clone(),
        };
        self.run(
            &data,
            &form_resources,
            multiply(&matrix, &state.ctm),
            depth + 1,
        );
    }

    fn show(&mut self, bytes: &[u8], state: &State, tm: &mut Matrix) {
        let Some(font) = state.font.clone() else {
            return;
        };
        for glyph in font.decode(bytes) {
            let trm = multiply(
                &multiply(
                    &[
                        state.size * state.scale,
                        0.0,
                        0.0,
                        state.size,
                        0.0,
                        state.rise,
                    ],
                    tm,
                ),
                &state.ctm,
            );
            let advance = glyph.width / 1000.0 * state.size;
            let scale_x = trm[0].hypot(trm[1]) / (state.size * state.scale).abs().max(1e-9);
            let size = (trm[2].hypot(trm[3])).max(1e-6);
            let length = advance * state.scale * scale_x;
            let vertical = trm[1].abs() > trm[0].abs();
            let (along, cross, end) = if vertical {
                let sign = trm[1].signum();
                (trm[5] * sign, trm[4], trm[5].mul_add(sign, length))
            } else {
                let sign = trm[0].signum();
                (trm[4] * sign, trm[5], trm[4].mul_add(sign, length))
            };
            self.placed.push(Placed {
                text: glyph.text,
                along,
                cross,
                end,
                size,
                vertical,
            });
            let space = if glyph.is_space {
                state.word_space
            } else {
                0.0
            };
            let tx = (advance + state.char_space + space) * state.scale;
            *tm = multiply(&translate(tx, 0.0), tm);
        }
    }
}

fn clean(text: &str, out: &mut String) {
    for c in text.chars() {
        match c {
            '\u{FB00}' => out.push_str("ff"),
            '\u{FB01}' => out.push_str("fi"),
            '\u{FB02}' => out.push_str("fl"),
            '\u{FB03}' => out.push_str("ffi"),
            '\u{FB04}' => out.push_str("ffl"),
            '\u{FB05}' | '\u{FB06}' => out.push_str("st"),
            '\u{00AD}' | '\0' => {}
            '\u{00A0}' => out.push(' '),
            c if c.is_control() && c != '\t' => {}
            c => out.push(c),
        }
    }
}

/// The text of one page.
#[must_use]
pub fn page_text(doc: &Document, page: &Page, fonts: &mut HashMap<String, Rc<Font>>) -> String {
    let resources = match doc.get(&page.dict, "Resources") {
        Some(Object::Dict(d)) => d,
        _ => Dict::new(),
    };
    let content = doc.page_contents(page);
    let mut interpreter = Interpreter {
        doc,
        fonts: std::mem::take(fonts),
        placed: Vec::new(),
    };
    interpreter.run(&content, &resources, IDENTITY, 0);
    *fonts = std::mem::take(&mut interpreter.fonts);

    let mut out = String::new();
    let mut last: Option<(f64, f64, f64, bool)> = None; // (end along, cross, size, vertical)
    for glyph in &interpreter.placed {
        if glyph.text.is_empty() {
            continue;
        }
        if let Some((end, cross, size, vertical)) = last {
            let reference = glyph.size.max(size) * 0.5;
            if vertical != glyph.vertical
                || (glyph.cross - cross).abs() > glyph.size.min(size) * 0.6
            {
                trim_end(&mut out);
                out.push('\n');
            } else if (glyph.along - end).abs() > reference * 0.35
                && !out.ends_with(' ')
                && !glyph.text.starts_with(' ')
            {
                out.push(' ');
            }
        }
        clean(&glyph.text, &mut out);
        last = Some((glyph.end, glyph.cross, glyph.size, glyph.vertical));
    }
    out.lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

fn trim_end(out: &mut String) {
    while out.ends_with(' ') {
        out.pop();
    }
}

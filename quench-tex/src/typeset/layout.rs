//! Math lists into boxes, by *The `TeXbook`*'s Appendix G with OpenType `MATH` constants.
//!
//! A list is turned into atoms (each a box with its class), the binary operators that have nothing to operate on
//! become ordinary (rules 5 and 6), and the atoms are put side by side with the spacing table between them
//! (chapter 18). Each kind of node has its rule: fractions (15), radicals (11), accents (12), operators and their
//! limits (13), scripts (18), `\left…\right` (19); the distances are the font's `MathConstants` at the size of
//! the style, which is how the OpenType MATH specification and `MathML` Core restate TeX's font parameters.

use quench_raster::Color;

use super::boxes::{Bx, Engine, GlyphInfo};
use super::{Error, Face, Fonts, colour};
use crate::{
    Accent, Class, Formula, Limits, MathChar, MathFont, Node, OpName, Space, Style, TextFont,
    TextPart, math_alphanumeric,
};

/// The width of an empty delimiter (`\left.`), TeX's `\nulldelimiterspace` (1.2 pt at 10 pt), per em.
const NULL_DELIMITER: f32 = 0.12;
/// TeX's `\delimiterfactor`: a grown delimiter covers at least 901/1000 of the content.
const DELIMITER_FACTOR: f32 = 0.901;
/// TeX's `\delimitershortfall` (5 pt at 10 pt), per em: or falls short of it by at most this much.
const DELIMITER_SHORTFALL: f32 = 0.5;

/// A math style: D, T, S or SS (0 to 3), cramped or not (*The `TeXbook`*, chapter 17).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct St {
    pub level: u8,
    pub cramped: bool,
}

impl St {
    pub const fn of(style: Style) -> Self {
        let level = match style {
            Style::Display => 0,
            Style::Text => 1,
            Style::Script => 2,
            Style::ScriptScript => 3,
        };
        Self {
            level,
            cramped: false,
        }
    }

    /// The style of a superscript.
    pub const fn sup(self) -> Self {
        Self {
            level: if self.level <= 1 { 2 } else { 3 },
            cramped: self.cramped,
        }
    }

    /// The style of a subscript: the superscript's, cramped.
    pub const fn sub(self) -> Self {
        Self {
            cramped: true,
            ..self.sup()
        }
    }

    /// The style of a numerator.
    pub const fn num(self) -> Self {
        Self {
            level: match self.level {
                0 => 1,
                1 => 2,
                _ => 3,
            },
            cramped: self.cramped,
        }
    }

    /// The style of a denominator: the numerator's, cramped.
    pub const fn den(self) -> Self {
        Self {
            cramped: true,
            ..self.num()
        }
    }

    pub const fn cramp(self) -> Self {
        Self {
            cramped: true,
            ..self
        }
    }

    pub const fn display(self) -> bool {
        self.level == 0
    }

    pub const fn script(self) -> bool {
        self.level >= 2
    }
}

/// What a node is laid out in: the style, the alphabet and the colour.
#[derive(Debug, Clone, Copy)]
pub struct Ctx {
    pub style: St,
    pub font: Option<MathFont>,
    pub colour: Option<Color>,
}

impl Ctx {
    pub const fn with(self, style: St) -> Self {
        Self { style, ..self }
    }
}

/// An atom: a box, its class as its left and right neighbours see it (`\middle` is a closing delimiter to what
/// is before it and an opening one to what follows), and what scripts need to know about it.
#[derive(Debug)]
pub struct Atom {
    pub bx: Bx,
    pub class: (Class, Class),
    /// Set when the atom is one glyph (TeX's character box: scripts are not dropped to it).
    pub glyph: Option<GlyphInfo>,
    /// A large operator's italic correction: its superscript goes that much further right than its subscript.
    pub op_italic: Option<f32>,
}

impl Atom {
    pub const fn boxed(bx: Bx, class: Class) -> Self {
        Self {
            bx,
            class: (class, class),
            glyph: None,
            op_italic: None,
        }
    }
}

/// An element of a list before it is put together.
#[derive(Debug)]
pub enum Entry {
    Atom(Atom),
    Kern(f32),
    /// A `\middle` delimiter, sized once the list it is in is measured.
    Middle(Option<char>, Option<Color>),
}

/// Lays out a whole formula in display or text style.
pub fn formula(formula: &Formula, display: bool, fonts: &Fonts) -> Result<Bx, Error> {
    let engine = Engine::new(fonts)?;
    let ctx = Ctx {
        style: St::of(if display { Style::Display } else { Style::Text }),
        font: None,
        colour: None,
    };
    engine.list(&formula.body, ctx)
}

/// TeX's spacing between atom classes, in mu: 3 thin, 4 medium, 5 thick; negative for a space only in display
/// and text styles (*The `TeXbook`*, chapter 18).
const SPACING: [[i8; 8]; 8] = [
    // Ord Op  Bin Rel Open Close Punct Inner
    [0, 3, -4, -5, 0, 0, 0, -3],     // Ord
    [3, 3, 0, -5, 0, 0, 0, -3],      // Op
    [-4, -4, 0, 0, -4, 0, 0, -4],    // Bin
    [-5, -5, 0, 0, -5, 0, 0, -5],    // Rel
    [0, 0, 0, 0, 0, 0, 0, 0],        // Open
    [0, 3, -4, -5, 0, 0, 0, -3],     // Close
    [-3, -3, 0, -3, -3, -3, -3, -3], // Punct
    [-3, 3, -4, -5, -3, 0, -3, -3],  // Inner
];

const fn class_index(class: Class) -> usize {
    match class {
        Class::Ord => 0,
        Class::Op => 1,
        Class::Bin => 2,
        Class::Rel => 3,
        Class::Open => 4,
        Class::Close => 5,
        Class::Punct => 6,
        Class::Inner => 7,
    }
}

/// The space between two neighbouring atoms, in mu.
pub const fn spacing_mu(left: Class, right: Class, script: bool) -> u8 {
    let v = SPACING[class_index(left)][class_index(right)];
    if v >= 0 {
        v as u8
    } else if script {
        0
    } else {
        (-v) as u8
    }
}

/// A letter in the math italic alphabet, as TeX sets letters by default (and `\mathit`, with capital Greek too).
fn italic(ch: char, capital_greek: bool) -> char {
    let code = u32::from(ch);
    let mapped = match ch {
        'h' => Some(0x210E),
        'A'..='Z' => Some(0x1D434 + code - 0x41),
        'a'..='z' => Some(0x1D44E + code - 0x61),
        'α'..='ω' => Some(0x1D6FC + code - 0x3B1),
        'Α'..='Ω' if capital_greek && code != 0x3A2 => Some(0x1D6E2 + code - 0x391),
        '∂' => Some(0x1D715),
        'ϵ' => Some(0x1D716),
        'ϑ' => Some(0x1D717),
        'ϰ' => Some(0x1D718),
        'ϕ' => Some(0x1D719),
        'ϱ' => Some(0x1D71A),
        'ϖ' => Some(0x1D71B),
        _ => None,
    };
    mapped.and_then(char::from_u32).unwrap_or(ch)
}

/// A letter in bold italic (`\boldsymbol`), Latin and Greek.
fn bold_italic(ch: char) -> Option<char> {
    let code = u32::from(ch);
    let mapped = match ch {
        'A'..='Z' => 0x1D468 + code - 0x41,
        'a'..='z' => 0x1D482 + code - 0x61,
        'α'..='ω' => 0x1D736 + code - 0x3B1,
        'Α'..='Ω' if code != 0x3A2 => 0x1D71C + code - 0x391,
        '∂' => 0x1D74F,
        'ϵ' => 0x1D750,
        'ϑ' => 0x1D751,
        'ϰ' => 0x1D752,
        'ϕ' => 0x1D753,
        'ϱ' => 0x1D754,
        'ϖ' => 0x1D755,
        _ => return None,
    };
    char::from_u32(mapped)
}

/// The character a math character is drawn as in an alphabet.
fn math_char(ch: char, font: Option<MathFont>) -> char {
    match font {
        None => italic(ch, false),
        Some(MathFont::Italic) => italic(ch, true),
        Some(MathFont::Roman) => ch,
        Some(MathFont::BoldSymbol) => {
            bold_italic(ch).unwrap_or_else(|| math_alphanumeric(ch, MathFont::Bold))
        }
        Some(font) => math_alphanumeric(ch, font),
    }
}

/// Primes at the start of a superscript list, and the rest of it.
fn split_primes(sup: Option<&[Node]>) -> (Vec<char>, Option<&[Node]>) {
    let Some(sup) = sup else {
        return (Vec::new(), None);
    };
    let count = sup
        .iter()
        .take_while(|n| {
            matches!(
                n,
                Node::Char(MathChar {
                    ch: '′' | '″' | '‴' | '⁗',
                    ..
                })
            )
        })
        .count();
    let primes = sup[..count]
        .iter()
        .filter_map(|n| match n {
            Node::Char(c) => Some(c.ch),
            _ => None,
        })
        .collect();
    let rest = &sup[count..];
    (primes, (!rest.is_empty()).then_some(rest))
}

impl Engine<'_> {
    /// The size of a style relative to text size (the font's script percentages).
    pub fn scale(&self, style: St) -> f32 {
        let percent = |v: i16, default: f32| {
            if v > 0 { f32::from(v) / 100.0 } else { default }
        };
        match style.level {
            0 | 1 => 1.0,
            2 => percent(self.k.script_percent_scale_down, 0.7),
            _ => percent(self.k.script_script_percent_scale_down, 0.5),
        }
    }

    /// A constant at the size of `style`.
    fn c(&self, value: i16, style: St) -> f32 {
        f32::from(value) * self.scale(style)
    }

    /// The math axis at the size of `style`.
    pub fn axis(&self, style: St) -> f32 {
        self.c(self.k.axis_height, style)
    }

    /// A list laid out as a box.
    pub fn list(&self, nodes: &[Node], ctx: Ctx) -> Result<Bx, Error> {
        let mut entries = Vec::new();
        self.entries(nodes, ctx, &mut entries)?;
        self.join(entries, ctx, None)
    }

    /// The entries of a list: style and colour changes apply to what they hold, spaces are kerns.
    pub fn entries(&self, nodes: &[Node], ctx: Ctx, out: &mut Vec<Entry>) -> Result<(), Error> {
        for node in nodes {
            match node {
                Node::Style { style, body } => self.entries(body, ctx.with(St::of(*style)), out)?,
                Node::Color { color, body } => {
                    let colour =
                        colour::parse(color).ok_or_else(|| Error::UnknownColour(color.clone()))?;
                    self.entries(
                        body,
                        Ctx {
                            colour: Some(colour),
                            ..ctx
                        },
                        out,
                    )?;
                }
                Node::Space(space) => out.push(Entry::Kern(self.space(*space, ctx.style))),
                Node::Middle(delim) => out.push(Entry::Middle(*delim, ctx.colour)),
                _ => out.push(Entry::Atom(self.atom(node, ctx)?)),
            }
        }
        Ok(())
    }

    /// An explicit space at the size of `style` (18 mu to the em).
    fn space(&self, space: Space, style: St) -> f32 {
        space.em() * self.em * self.scale(style)
    }

    /// Puts entries side by side, with TeX's spacing between atoms. `\middle` delimiters are grown to
    /// `middle` (they keep their normal size without one).
    pub fn join(&self, entries: Vec<Entry>, ctx: Ctx, middle: Option<f32>) -> Result<Bx, Error> {
        let mut list = Vec::with_capacity(entries.len());
        for entry in entries {
            list.push(match entry {
                Entry::Middle(delim, colour) => {
                    let size = middle.unwrap_or(0.0);
                    let bx = self.delimiter(delim, size, Ctx { colour, ..ctx })?;
                    Entry::Atom(Atom {
                        bx,
                        class: (Class::Close, Class::Open),
                        glyph: None,
                        op_italic: None,
                    })
                }
                other => other,
            });
        }
        demote_binaries(&mut list);
        let mu = self.em * self.scale(ctx.style) / 18.0;
        let mut out = Bx::default();
        let mut x = 0.0;
        let mut previous: Option<Class> = None;
        for entry in list {
            match entry {
                Entry::Kern(k) => x += k,
                Entry::Atom(atom) => {
                    if let Some(left) = previous {
                        x += f32::from(spacing_mu(left, atom.class.0, ctx.style.script())) * mu;
                    }
                    previous = Some(atom.class.1);
                    let advance = atom.bx.width + atom.bx.italic;
                    out.add(atom.bx, x, 0.0);
                    x += advance;
                }
                Entry::Middle(..) => {}
            }
        }
        out.width = x;
        Ok(out)
    }

    /// One node as an atom.
    fn atom(&self, node: &Node, ctx: Ctx) -> Result<Atom, Error> {
        Ok(match node {
            Node::Char(c) => self.char_atom(*c, ctx)?,
            Node::Op { name, .. } => self.op(name, ctx)?,
            // TeX makes a group of one ordinary character that character (`{x}^2` is `x^2`).
            Node::Group(body) => match body.as_slice() {
                [Node::Char(c)] if c.class == Class::Ord => self.char_atom(*c, ctx)?,
                _ => Atom::boxed(self.list(body, ctx)?, Class::Ord),
            },
            Node::Font { font, body } => {
                let ctx = Ctx {
                    font: Some(*font),
                    ..ctx
                };
                match body.as_slice() {
                    [Node::Char(c)] if c.class == Class::Ord => self.char_atom(*c, ctx)?,
                    _ => Atom::boxed(self.list(body, ctx)?, Class::Ord),
                }
            }
            Node::Scripts { base, sub, sup } => {
                self.scripts(base, sub.as_deref(), sup.as_deref(), ctx)?
            }
            Node::Frac {
                num,
                den,
                bar,
                open,
                close,
                style,
            } => Atom::boxed(
                self.fraction(num, den, *bar, (*open, *close), *style, ctx)?,
                Class::Inner,
            ),
            Node::Sqrt { degree, body } => {
                Atom::boxed(self.radical(degree.as_deref(), body, ctx)?, Class::Ord)
            }
            Node::Fenced { open, body, close } => {
                Atom::boxed(self.fenced(*open, body, *close, ctx)?, Class::Inner)
            }
            Node::Middle(delim) => Atom::boxed(self.delimiter(*delim, 0.0, ctx)?, Class::Ord),
            Node::Big { delim, size, class } => {
                let factor = [1.2, 1.8, 2.4, 3.0][usize::from(size.clamp(&1, &4) - 1)];
                let size = DELIMITER_FACTOR * factor * self.em * self.scale(ctx.style);
                Atom::boxed(self.delimiter(*delim, size, ctx)?, *class)
            }
            Node::Accent { accent, base } => {
                Atom::boxed(self.accent(*accent, base, ctx)?, Class::Ord)
            }
            Node::Overline(body) => Atom::boxed(self.overline(body, ctx)?, Class::Ord),
            Node::Underline(body) => Atom::boxed(self.underline(body, ctx)?, Class::Ord),
            Node::Brace { over, body, label } => {
                Atom::boxed(self.brace(*over, body, label.as_deref(), ctx)?, Class::Op)
            }
            Node::Stack {
                base,
                over,
                under,
                class,
            } => {
                let base = Atom::boxed(self.list(base, ctx)?, *class);
                let bx = self.limits(base, under.as_deref(), over.as_deref(), ctx)?;
                Atom::boxed(bx, *class)
            }
            Node::Text { font, body } => Atom::boxed(self.text(body, *font, ctx)?, Class::Ord),
            Node::Class { class, body, .. } => Atom::boxed(self.list(body, ctx)?, *class),
            Node::Array(array) => Atom::boxed(super::array::array(self, array, ctx)?, Class::Ord),
            Node::Style { .. } | Node::Color { .. } | Node::Space(_) => Atom::boxed(
                self.list(std::slice::from_ref(node), ctx)?,
                node.class().unwrap_or(Class::Ord),
            ),
        })
    }

    /// A character in the alphabet of the context (letters in math italic by default).
    fn char_atom(&self, c: MathChar, ctx: Ctx) -> Result<Atom, Error> {
        let mapped = math_char(c.ch, ctx.font);
        let font = &self.fonts.math;
        let glyph = font
            .glyph(mapped)
            .or_else(|| font.glyph(c.ch))
            .ok_or(Error::MissingGlyph(c.ch))?;
        let scale = self.scale(ctx.style);
        Ok(Atom {
            bx: self.glyph_box(Face::Math, glyph, scale, ctx.colour),
            class: (c.class, c.class),
            // A character is a character to the script rules even when the font calls it an extended shape
            // (a parenthesis): TeX looks at what the nucleus is, not at the glyph.
            glyph: Some(GlyphInfo {
                extended: false,
                ..self.glyph_info(glyph, scale)
            }),
            op_italic: None,
        })
    }

    /// A large operator (bigger in display style, centred on the axis) or a named one (upright letters).
    fn op(&self, name: &OpName, ctx: Ctx) -> Result<Atom, Error> {
        let scale = self.scale(ctx.style);
        match name {
            OpName::Symbol(ch) => {
                let mut glyph = self.glyph_of(*ch)?;
                if ctx.style.display()
                    && let Some(construction) = self.math.vertical_construction(glyph)
                {
                    let least = f32::from(self.k.display_operator_min_height);
                    glyph = construction
                        .variants
                        .iter()
                        .find(|v| f32::from(v.advance) >= least)
                        .or_else(|| construction.variants.last())
                        .map_or(glyph, |v| v.glyph);
                }
                let mut bx = self.glyph_box(Face::Math, glyph, scale, ctx.colour);
                let italic = std::mem::take(&mut bx.italic);
                let shift = self.axis(ctx.style) - (bx.height - bx.depth) / 2.0;
                Ok(Atom {
                    bx: bx.shifted(shift),
                    class: (Class::Op, Class::Op),
                    glyph: Some(GlyphInfo {
                        extended: true,
                        attach: None,
                    }),
                    op_italic: Some(italic),
                })
            }
            OpName::Named(name) => {
                let mut bx = Bx::default();
                let mut x = 0.0;
                for ch in name.chars() {
                    if ch == ' ' {
                        x += self.space(Space::Mu(3.0), ctx.style);
                        continue;
                    }
                    let glyph = self.glyph_of(ch)?;
                    let piece = self.glyph_box(Face::Math, glyph, scale, ctx.colour);
                    let advance = piece.width;
                    bx.add(piece, x, 0.0);
                    x += advance;
                }
                bx.width = x;
                Ok(Atom::boxed(bx, Class::Op))
            }
        }
    }

    /// A base with scripts: limits above and below an operator that takes them in this style, beside otherwise.
    fn scripts(
        &self,
        base: &Node,
        sub: Option<&[Node]>,
        sup: Option<&[Node]>,
        ctx: Ctx,
    ) -> Result<Atom, Error> {
        let (primes, sup) = split_primes(sup);
        let atom = self.atom(base, ctx)?;
        let limits = match base {
            Node::Op { limits, .. }
            | Node::Class {
                limits: Some(limits),
                ..
            } => match limits {
                Limits::Display => ctx.style.display(),
                Limits::Always => true,
                Limits::Never => false,
            },
            _ => false,
        };
        let primes = self.primes(&primes, ctx)?;
        if sub.is_none() && sup.is_none() && primes.is_none() {
            return Ok(atom);
        }
        let class = atom.class;
        let bx = if limits && primes.is_none() {
            self.limits(atom, sub, sup, ctx)?
        } else {
            self.beside(atom, sub, sup, primes, ctx)?
        };
        Ok(Atom {
            class,
            ..Atom::boxed(bx, class.0)
        })
    }

    /// Primes at the size of their base: the font draws its prime glyphs raised already, so they take the
    /// superscript's place without its shift.
    fn primes(&self, primes: &[char], ctx: Ctx) -> Result<Option<Bx>, Error> {
        if primes.is_empty() {
            return Ok(None);
        }
        let mut bx = Bx::default();
        let mut x = 0.0;
        for &ch in primes {
            let prime = self.char_atom(
                MathChar {
                    ch,
                    class: Class::Ord,
                },
                ctx,
            )?;
            let advance = prime.bx.width + prime.bx.italic;
            bx.add(prime.bx, x, 0.0);
            x += advance;
        }
        bx.width = x;
        Ok(Some(bx))
    }

    /// Rule 18: sub- and superscripts beside the base, primes first in the superscript's place.
    fn beside(
        &self,
        atom: Atom,
        sub: Option<&[Node]>,
        sup: Option<&[Node]>,
        primes: Option<Bx>,
        ctx: Ctx,
    ) -> Result<Bx, Error> {
        let st = ctx.style;
        let k = &self.k;
        let c = |v: i16| self.c(v, st);
        let sup = sup.map(|n| self.list(n, ctx.with(st.sup()))).transpose()?;
        let sub = sub.map(|n| self.list(n, ctx.with(st.sub()))).transpose()?;
        let base = atom.bx;
        // A character's scripts are placed by the constants alone; a box's hang from its top and bottom.
        let character = atom.glyph.is_some_and(|g| !g.extended);
        let (mut up, mut down) = if character {
            (0.0, 0.0)
        } else {
            (
                base.height - c(k.superscript_baseline_drop_max),
                base.depth + c(k.subscript_baseline_drop_min),
            )
        };
        let shift_up = if st.cramped {
            k.superscript_shift_up_cramped
        } else {
            k.superscript_shift_up
        };
        if let Some(p) = &sup {
            up = up
                .max(c(shift_up))
                .max(p.depth + c(k.superscript_bottom_min));
        }
        match (&sub, &sup) {
            (Some(b), None) => {
                down = down
                    .max(c(k.subscript_shift_down))
                    .max(b.height - c(k.subscript_top_max));
            }
            (Some(b), Some(p)) => {
                down = down.max(c(k.subscript_shift_down));
                let least = c(k.sub_superscript_gap_min);
                let gap = (up - p.depth) - (b.height - down);
                if gap < least {
                    // Raise the superscript as far as its bottom may go, then lower the subscript for the rest.
                    let raise = (c(k.superscript_bottom_max_with_subscript) - (up - p.depth))
                        .min(least - gap)
                        .max(0.0);
                    up += raise;
                    let gap = gap + raise;
                    if gap < least {
                        down += least - gap;
                    }
                }
            }
            _ => {}
        }
        // A large operator's italic correction moves its subscript left; a letter's moves its superscript right.
        let (sub_x, sup_x) = atom
            .op_italic
            .map_or((base.width, base.width + base.italic), |italic| {
                (base.width - italic, base.width)
            });
        let mut width = base.width + if sub.is_none() { base.italic } else { 0.0 };
        let mut out = Bx::default();
        out.add(
            Bx {
                italic: 0.0,
                ..base
            },
            0.0,
            0.0,
        );
        let mut sup_x = sup_x;
        if let Some(q) = primes {
            let x = sup_x;
            sup_x += q.width;
            width = width.max(sup_x);
            out.add(q, x, 0.0);
        }
        if let Some(p) = sup {
            width = width.max(sup_x + p.width);
            out.add(p, sup_x, up);
        }
        if let Some(b) = sub {
            width = width.max(sub_x + b.width);
            out.add(b, sub_x, -down);
        }
        out.width = width + c(k.space_after_script);
        Ok(out)
    }

    /// Rule 13a: limits centred above and below an operator (and `\overset`, `\underset`), the superscript
    /// half the italic correction right, the subscript half left.
    fn limits(
        &self,
        atom: Atom,
        sub: Option<&[Node]>,
        sup: Option<&[Node]>,
        ctx: Ctx,
    ) -> Result<Bx, Error> {
        let st = ctx.style;
        let k = &self.k;
        let c = |v: i16| self.c(v, st);
        let sup = sup.map(|n| self.list(n, ctx.with(st.sup()))).transpose()?;
        let sub = sub.map(|n| self.list(n, ctx.with(st.sub()))).transpose()?;
        let italic = atom.op_italic.unwrap_or(0.0);
        let base = atom.bx;
        let width = base
            .width
            .max(sup.as_ref().map_or(0.0, |b| b.width))
            .max(sub.as_ref().map_or(0.0, |b| b.width));
        let (base_height, base_depth) = (base.height, base.depth);
        let mut out = Bx::default();
        out.add(base.clone(), (width - base.width) / 2.0, 0.0);
        if let Some(p) = sup {
            let y = base_height
                + (c(k.upper_limit_gap_min) + p.depth).max(c(k.upper_limit_baseline_rise_min));
            let x = f32::midpoint(width - p.width, italic);
            out.add(p, x, y);
        }
        if let Some(b) = sub {
            let y = base_depth
                + (c(k.lower_limit_gap_min) + b.height).max(c(k.lower_limit_baseline_drop_min));
            let x = (width - b.width - italic) / 2.0;
            out.add(b, x, -y);
        }
        out.width = width;
        Ok(out)
    }

    /// Rule 15: a fraction, with its rule on the axis, or a `\binom`-like stack without one.
    fn fraction(
        &self,
        num: &[Node],
        den: &[Node],
        bar: bool,
        delims: (Option<char>, Option<char>),
        style: Option<Style>,
        ctx: Ctx,
    ) -> Result<Bx, Error> {
        let st = style.map_or(ctx.style, St::of);
        let ctx = ctx.with(st);
        let k = &self.k;
        let c = |v: i16| self.c(v, st);
        let display = st.display();
        let n = self.list(num, ctx.with(st.num()))?;
        let d = self.list(den, ctx.with(st.den()))?;
        let axis = self.axis(st);
        let thickness = c(k.fraction_rule_thickness);
        let (mut up, mut down);
        if bar {
            let (shift_up, shift_down, gap_num, gap_den) = if display {
                (
                    k.fraction_numerator_display_style_shift_up,
                    k.fraction_denominator_display_style_shift_down,
                    k.fraction_num_display_style_gap_min,
                    k.fraction_denom_display_style_gap_min,
                )
            } else {
                (
                    k.fraction_numerator_shift_up,
                    k.fraction_denominator_shift_down,
                    k.fraction_numerator_gap_min,
                    k.fraction_denominator_gap_min,
                )
            };
            up = c(shift_up).max(c(gap_num) + axis + thickness / 2.0 + n.depth);
            down = c(shift_down).max(c(gap_den) - axis + thickness / 2.0 + d.height);
        } else {
            let (shift_up, shift_down, least) = if display {
                (
                    k.stack_top_display_style_shift_up,
                    k.stack_bottom_display_style_shift_down,
                    k.stack_display_style_gap_min,
                )
            } else {
                (
                    k.stack_top_shift_up,
                    k.stack_bottom_shift_down,
                    k.stack_gap_min,
                )
            };
            up = c(shift_up);
            down = c(shift_down);
            let gap = (up - n.depth) - (d.height - down);
            if gap < c(least) {
                up += (c(least) - gap) / 2.0;
                down += (c(least) - gap) / 2.0;
            }
        }
        let width = n.width.max(d.width);
        let mut frac = Bx::default();
        let (nw, dw) = (n.width, d.width);
        frac.add(n, (width - nw) / 2.0, up);
        frac.add(d, (width - dw) / 2.0, -down);
        if bar {
            frac.rule(0.0, axis - thickness / 2.0, width, thickness, ctx.colour);
        }
        frac.width = width;
        self.delimited(delims.0, frac, delims.1, ctx)
    }

    /// A box between two delimiters grown to cover it (`\binom`, the matrices).
    pub fn delimited(
        &self,
        open: Option<char>,
        inner: Bx,
        close: Option<char>,
        ctx: Ctx,
    ) -> Result<Bx, Error> {
        let size = self.delimiter_size(inner.height, inner.depth, ctx.style);
        let entries = vec![
            Entry::Atom(Atom::boxed(self.delimiter(open, size, ctx)?, Class::Open)),
            Entry::Atom(Atom::boxed(inner, Class::Ord)),
            Entry::Atom(Atom::boxed(self.delimiter(close, size, ctx)?, Class::Close)),
        ];
        self.join(entries, ctx, None)
    }

    /// Rule 19: how big delimiters around content of this height and depth are: they cover at least
    /// `\delimiterfactor` of it, or fall short by at most `\delimitershortfall`, measured from the axis.
    pub fn delimiter_size(&self, height: f32, depth: f32, style: St) -> f32 {
        let axis = self.axis(style);
        let half = (height - axis).max(depth + axis);
        (2.0 * half * DELIMITER_FACTOR)
            .max(2.0 * half - DELIMITER_SHORTFALL * self.em * self.scale(style))
    }

    /// A delimiter grown to `size` and centred on the axis; an empty one (`.`) is a null delimiter space.
    pub fn delimiter(&self, delim: Option<char>, size: f32, ctx: Ctx) -> Result<Bx, Error> {
        let scale = self.scale(ctx.style);
        let Some(ch) = delim else {
            return Ok(Bx::space(NULL_DELIMITER * self.em * scale));
        };
        let mut bx = self.vertical(ch, size, scale, ctx.colour)?;
        bx.italic = 0.0;
        let shift = self.axis(ctx.style) - (bx.height - bx.depth) / 2.0;
        Ok(bx.shifted(shift))
    }

    /// `\left … \middle … \right`.
    fn fenced(
        &self,
        open: Option<char>,
        body: &[Node],
        close: Option<char>,
        ctx: Ctx,
    ) -> Result<Bx, Error> {
        let mut inner = Vec::new();
        self.entries(body, ctx, &mut inner)?;
        let (height, depth) = inner.iter().fold((0.0_f32, 0.0_f32), |(h, d), e| match e {
            Entry::Atom(a) => (h.max(a.bx.height), d.max(a.bx.depth)),
            _ => (h, d),
        });
        let size = self.delimiter_size(height, depth, ctx.style);
        let mut entries = vec![Entry::Atom(Atom::boxed(
            self.delimiter(open, size, ctx)?,
            Class::Open,
        ))];
        entries.extend(inner);
        entries.push(Entry::Atom(Atom::boxed(
            self.delimiter(close, size, ctx)?,
            Class::Close,
        )));
        self.join(entries, ctx, Some(size))
    }

    /// Rule 11: a radical sign grown to cover the radicand and the gap above it, an overbar of the font's
    /// thickness, and the degree raised on the sign's left.
    fn radical(&self, degree: Option<&[Node]>, body: &[Node], ctx: Ctx) -> Result<Bx, Error> {
        let st = ctx.style;
        let k = &self.k;
        let c = |v: i16| self.c(v, st);
        let scale = self.scale(st);
        let radicand = self.list(body, ctx.with(st.cramp()))?;
        let mut gap = c(if st.display() {
            k.radical_display_style_vertical_gap
        } else {
            k.radical_vertical_gap
        });
        let thickness = c(k.radical_rule_thickness);
        let wanted = radicand.height + radicand.depth + gap + thickness;
        let mut sign = self.vertical('√', wanted, scale, ctx.colour)?;
        sign.italic = 0.0;
        let total = sign.height + sign.depth;
        if total > wanted {
            gap += (total - wanted) / 2.0;
        }
        let bar_bottom = radicand.height + gap;
        let bar_top = bar_bottom + thickness;
        let sign_shift = bar_top - sign.height;
        let sign = sign.shifted(sign_shift);
        let sign_bottom = -sign.depth;

        let mut out = Bx::default();
        let mut sign_x = 0.0;
        if let Some(degree) = degree {
            let index = self.list(
                degree,
                ctx.with(St {
                    level: 3,
                    cramped: false,
                }),
            )?;
            let raise = f32::from(k.radical_degree_bottom_raise_percent) / 100.0;
            let baseline = sign_bottom + raise * total + index.depth;
            let before = c(k.radical_kern_before_degree);
            sign_x = (before + index.width + c(k.radical_kern_after_degree)).max(0.0);
            out.add(index, before, baseline);
        }
        let body_x = sign_x + sign.width;
        let body_width = radicand.width;
        out.add(sign, sign_x, 0.0);
        out.add(radicand, body_x, 0.0);
        out.rule(body_x, bar_bottom, body_width, thickness, ctx.colour);
        out.height = out.height.max(bar_top + c(k.radical_extra_ascender));
        out.width = body_x + body_width;
        Ok(out)
    }

    /// Rule 12: an accent over its base, on the base's top accent attachment, raised by what the base has above
    /// the accent base height; a wide accent grown to the base's width.
    fn accent(&self, accent: Accent, base: &[Node], ctx: Ctx) -> Result<Bx, Error> {
        let st = ctx.style;
        let scale = self.scale(st);
        let base_ctx = ctx.with(st.cramp());
        let atom = match base {
            [Node::Char(c)] => self.char_atom(*c, base_ctx)?,
            _ => Atom::boxed(self.list(base, base_ctx)?, Class::Ord),
        };
        let base = atom.bx;
        let base_attach = atom
            .glyph
            .and_then(|g| g.attach)
            .unwrap_or_else(|| f32::midpoint(base.width, base.italic));
        let ch = accent.combining();
        let (mark, mark_attach) = if accent.is_wide() {
            let arrow = matches!(
                accent,
                Accent::OverRightArrow | Accent::OverLeftArrow | Accent::OverLeftRightArrow
            );
            self.horizontal(ch, base.width, scale, arrow, ctx.colour)?
        } else {
            let glyph = self.glyph_of(ch)?;
            let bx = self.glyph_box(Face::Math, glyph, scale, ctx.colour);
            let attach = self.accent_attach(glyph, scale, bx.width);
            (bx, attach)
        };
        let raise = (base.height - self.c(self.k.accent_base_height, st)).max(0.0);
        let mut out = Bx::default();
        let width = base.width;
        out.add(
            Bx {
                italic: 0.0,
                ..base
            },
            0.0,
            0.0,
        );
        out.add(mark, base_attach - mark_attach, raise);
        out.width = width;
        Ok(out)
    }

    /// `\overline`: a rule over the (cramped) base, the font's gap and thickness, and room above it.
    fn overline(&self, body: &[Node], ctx: Ctx) -> Result<Bx, Error> {
        let st = ctx.style;
        let k = &self.k;
        let base = self.list(body, ctx.with(st.cramp()))?;
        let thickness = self.c(k.overbar_rule_thickness, st);
        let y = base.height + self.c(k.overbar_vertical_gap, st);
        let width = base.width;
        let mut out = Bx::default();
        out.add(base, 0.0, 0.0);
        out.rule(0.0, y, width, thickness, ctx.colour);
        out.height += self.c(k.overbar_extra_ascender, st);
        out.width = width;
        Ok(out)
    }

    /// `\underline`: a rule under the base.
    fn underline(&self, body: &[Node], ctx: Ctx) -> Result<Bx, Error> {
        let st = ctx.style;
        let k = &self.k;
        let base = self.list(body, ctx)?;
        let thickness = self.c(k.underbar_rule_thickness, st);
        let y = -(base.depth + self.c(k.underbar_vertical_gap, st) + thickness);
        let width = base.width;
        let mut out = Bx::default();
        out.add(base, 0.0, 0.0);
        out.rule(0.0, y, width, thickness, ctx.colour);
        out.depth += self.c(k.underbar_extra_descender, st);
        out.width = width;
        Ok(out)
    }

    /// `\overbrace`, `\underbrace`: the brace grown to the body's width, the stretch stack gaps between body,
    /// brace and label.
    fn brace(
        &self,
        over: bool,
        body: &[Node],
        label: Option<&[Node]>,
        ctx: Ctx,
    ) -> Result<Bx, Error> {
        let st = ctx.style;
        let k = &self.k;
        let c = |v: i16| self.c(v, st);
        let base = self.list(body, ctx)?;
        let (brace, _) = self.horizontal(
            if over { '⏞' } else { '⏟' },
            base.width,
            self.scale(st),
            true,
            ctx.colour,
        )?;
        let (_, ink_bottom, _, ink_top) =
            super::boxes::ink(self.fonts, &brace.items).unwrap_or((0.0, 0.0, 0.0, 0.0));
        let label = label
            .map(|n| self.list(n, ctx.with(if over { st.sup() } else { st.sub() })))
            .transpose()?;
        let width = base
            .width
            .max(brace.width)
            .max(label.as_ref().map_or(0.0, |b| b.width));
        let mut out = Bx::default();
        let (base_width, base_height, base_depth, brace_width) =
            (base.width, base.height, base.depth, brace.width);
        out.add(base, (width - base_width) / 2.0, 0.0);
        let (brace_y, label_y) = if over {
            let y = base_height + c(k.stretch_stack_gap_below_min) - ink_bottom;
            let top = ink_top + y;
            let label_y = label
                .as_ref()
                .map_or(0.0, |l| top + c(k.stretch_stack_gap_above_min) + l.depth);
            (y, label_y)
        } else {
            let y = -(base_depth + c(k.stretch_stack_gap_above_min)) - ink_top;
            let bottom = ink_bottom + y;
            let label_y = label.as_ref().map_or(0.0, |l| {
                bottom - c(k.stretch_stack_gap_below_min) - l.height
            });
            (y, label_y)
        };
        out.add(brace, (width - brace_width) / 2.0, brace_y);
        if let Some(l) = label {
            let lw = l.width;
            out.add(l, (width - lw) / 2.0, label_y);
        }
        out.width = width;
        Ok(out)
    }

    /// A text run in the text font, at the size of the style.
    fn text(&self, parts: &[TextPart], font: TextFont, ctx: Ctx) -> Result<Bx, Error> {
        let mut out = Bx::default();
        let mut x = 0.0;
        self.text_parts(parts, font, ctx, &mut out, &mut x)?;
        out.width = x;
        Ok(out)
    }

    fn text_parts(
        &self,
        parts: &[TextPart],
        font: TextFont,
        ctx: Ctx,
        out: &mut Bx,
        x: &mut f32,
    ) -> Result<(), Error> {
        let scale = self.scale(ctx.style);
        let face = match font {
            TextFont::Bold => Face::TextBold,
            TextFont::Italic => Face::TextItalic,
            TextFont::Normal | TextFont::SansSerif | TextFont::Monospace => Face::Text,
        };
        for part in parts {
            match part {
                TextPart::Str(text) => {
                    let text_font = self.fonts.face(face);
                    let em_scale = scale * self.em / f32::from(text_font.units_per_em());
                    let mut previous = None;
                    for ch in text.chars() {
                        if font == TextFont::Monospace && ch.is_ascii_alphanumeric() {
                            let glyph =
                                self.glyph_of(math_alphanumeric(ch, MathFont::Monospace))?;
                            let bx = self.glyph_box(Face::Math, glyph, scale, ctx.colour);
                            let advance = bx.width;
                            out.add(bx, *x, 0.0);
                            *x += advance;
                            previous = None;
                            continue;
                        }
                        let (glyph_face, glyph) = match text_font.glyph(ch) {
                            Some(g) => (face, g),
                            None => (Face::Math, self.glyph_of(ch)?),
                        };
                        if glyph_face == face
                            && let Some(p) = previous
                        {
                            *x += f32::from(text_font.kerning(p, glyph)) * em_scale;
                        }
                        previous = (glyph_face == face).then_some(glyph);
                        let bx = self.glyph_box(glyph_face, glyph, scale, ctx.colour);
                        let advance = bx.width;
                        if !ch.is_whitespace() {
                            out.add(bx, *x, 0.0);
                        }
                        *x += advance;
                    }
                }
                TextPart::Math(nodes) => {
                    let st = if ctx.style.display() {
                        St {
                            level: 1,
                            ..ctx.style
                        }
                    } else {
                        ctx.style
                    };
                    let bx = self.list(nodes, ctx.with(st))?;
                    let advance = bx.width;
                    out.add(bx, *x, 0.0);
                    *x += advance;
                }
                TextPart::Styled(inner, parts) => self.text_parts(parts, *inner, ctx, out, x)?,
                TextPart::Space(space) => *x += self.space(*space, ctx.style),
            }
        }
        Ok(())
    }
}

/// Rules 5 and 6: a binary operator after nothing, another binary operator, an operator, a relation, an opening
/// delimiter or punctuation, or before a relation, a closing delimiter or punctuation, or last, is ordinary.
fn demote_binaries(list: &mut [Entry]) {
    let atoms: Vec<usize> = list
        .iter()
        .enumerate()
        .filter_map(|(i, e)| matches!(e, Entry::Atom(_)).then_some(i))
        .collect();
    let class = |list: &[Entry], i: usize| match &list[i] {
        Entry::Atom(a) => a.class,
        _ => (Class::Ord, Class::Ord),
    };
    let set_ord = |list: &mut [Entry], i: usize| {
        if let Entry::Atom(a) = &mut list[i] {
            a.class = (Class::Ord, Class::Ord);
        }
    };
    for (n, &i) in atoms.iter().enumerate() {
        let before = n.checked_sub(1).map(|p| class(list, atoms[p]).1);
        let (left, _) = class(list, i);
        if left == Class::Bin
            && matches!(
                before,
                None | Some(Class::Bin | Class::Op | Class::Rel | Class::Open | Class::Punct)
            )
        {
            set_ord(list, i);
        }
        let (left, _) = class(list, i);
        if matches!(left, Class::Rel | Class::Close | Class::Punct)
            && let Some(p) = n.checked_sub(1)
            && class(list, atoms[p]).1 == Class::Bin
        {
            set_ord(list, atoms[p]);
        }
    }
    if let Some(&last) = atoms.last()
        && class(list, last).1 == Class::Bin
    {
        set_ord(list, last);
    }
}

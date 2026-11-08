//! The syntax tree of a formula: what [`crate::parse`] produces and every output reads.
//!
//! The tree keeps what TeX's layout (*The `TeXbook`*, Appendix G) needs, not only what the text output uses: the
//! class of every atom, the limits behaviour of operators, style changes, delimiters with their sizes, accents,
//! arrays with their column alignment, text runs, alphabets and explicit spacing. Characters are stored as the
//! Unicode characters they stand for (`\alpha` is `α`, `-` is `−`); letters stay plain Latin letters, the math
//! italic being a matter of layout, and alphabet changes are [`Node::Font`] around them.

/// A parsed formula.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Formula {
    /// The math list of the formula, in order.
    pub body: Vec<Node>,
}

/// TeX's atom classes, which decide the spacing between neighbours (*The `TeXbook`*, chapter 18).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Class {
    /// Ordinary: letters, digits, most symbols.
    Ord,
    /// A large or named operator (`\sum`, `\sin`).
    Op,
    /// A binary operator (`+`, `\times`).
    Bin,
    /// A relation (`=`, `\le`, arrows).
    Rel,
    /// An opening delimiter (`(`, `\langle`).
    Open,
    /// A closing delimiter (`)`, `!`, `?`).
    Close,
    /// Punctuation (`,`, `;`).
    Punct,
    /// An inner atom (`\left…\right`, fractions, `\cdots`).
    Inner,
}

/// One math character with its class.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MathChar {
    /// The Unicode character (`α`, `≤`, `−`, a plain `x`).
    pub ch: char,
    /// Its class.
    pub class: Class,
}

/// Where an operator's scripts go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Limits {
    /// Above and below in display style, beside otherwise: `\sum`, `\lim` (TeX's `\displaylimits`).
    Display,
    /// Always above and below (`\limits`).
    Always,
    /// Always beside (`\nolimits`; integrals and `\sin`-like operators by default).
    Never,
}

/// What an operator shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpName {
    /// A large symbol, drawn bigger in display style: `∑`, `∫`.
    Symbol(char),
    /// A name set upright in the roman font: `sin`, `lim sup`, `\operatorname{rank}`.
    Named(String),
}

/// A math style (*The `TeXbook`*, chapter 17).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    /// `\displaystyle`.
    Display,
    /// `\textstyle`.
    Text,
    /// `\scriptstyle`.
    Script,
    /// `\scriptscriptstyle`.
    ScriptScript,
}

/// An alphabet change in math (`\mathbb` and friends).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MathFont {
    /// `\mathrm`: upright roman.
    Roman,
    /// `\mathit`: italic (letters set as text italic).
    Italic,
    /// `\mathbf`: bold upright.
    Bold,
    /// `\boldsymbol` / `\bm`: bold, symbols and Greek included.
    BoldSymbol,
    /// `\mathbb`: double-struck.
    Blackboard,
    /// `\mathcal`: calligraphic (Unicode's script letters).
    Calligraphic,
    /// `\mathscr`: script.
    Script,
    /// `\mathfrak`: Fraktur.
    Fraktur,
    /// `\mathsf`: sans-serif.
    SansSerif,
    /// `\mathtt`: monospace.
    Monospace,
}

/// The font of a text run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextFont {
    /// `\text`, `\mbox`, `\textrm`, `\textnormal`, `\textup`: the body font.
    Normal,
    /// `\textbf`.
    Bold,
    /// `\textit`, `\emph`.
    Italic,
    /// `\textsf`.
    SansSerif,
    /// `\texttt`.
    Monospace,
}

/// A piece of a text run.
#[derive(Debug, Clone, PartialEq)]
pub enum TextPart {
    /// Text, spaces kept (runs of spaces are one space, as in TeX).
    Str(String),
    /// Math inside the text (`\text{for $x > 0$}`).
    Math(Vec<Node>),
    /// A nested change of font (`\text{a \textbf{b}}`).
    Styled(TextFont, Vec<Self>),
    /// Explicit spacing (`\quad`, `\ `).
    Space(Space),
}

/// An explicit space, in the unit it was written in.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Space {
    /// Math units: 1/18 em of the math font (`\,` is 3, `\:` 4, `\;` 5, `\!` −3).
    Mu(f32),
    /// Ems of the current font (`\quad` is 1, `\qquad` 2; `\hspace` lengths are converted, 10 pt to the em).
    Em(f32),
}

impl Space {
    /// The amount in ems (18 mu to the em).
    #[must_use]
    pub fn em(self) -> f32 {
        match self {
            Self::Mu(mu) => mu / 18.0,
            Self::Em(em) => em,
        }
    }
}

/// An accent over a base.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Accent {
    /// `\hat`.
    Hat,
    /// `\check`.
    Check,
    /// `\tilde`.
    Tilde,
    /// `\acute`.
    Acute,
    /// `\grave`.
    Grave,
    /// `\dot`.
    Dot,
    /// `\ddot`.
    Ddot,
    /// `\dddot`.
    Dddot,
    /// `\breve`.
    Breve,
    /// `\bar`.
    Bar,
    /// `\vec`.
    Vec,
    /// `\mathring`.
    Ring,
    /// `\widehat`: stretches over its base.
    WideHat,
    /// `\widetilde`: stretches over its base.
    WideTilde,
    /// `\widecheck`: stretches over its base.
    WideCheck,
    /// `\overrightarrow`: an arrow as wide as its base.
    OverRightArrow,
    /// `\overleftarrow`: an arrow as wide as its base.
    OverLeftArrow,
    /// `\overleftrightarrow`: an arrow as wide as its base.
    OverLeftRightArrow,
}

impl Accent {
    /// The combining character of the accent (U+0302 for a hat, U+20D7 for a vector arrow, ...).
    #[must_use]
    pub const fn combining(self) -> char {
        match self {
            Self::Hat | Self::WideHat => '\u{302}',
            Self::Check | Self::WideCheck => '\u{30C}',
            Self::Tilde | Self::WideTilde => '\u{303}',
            Self::Acute => '\u{301}',
            Self::Grave => '\u{300}',
            Self::Dot => '\u{307}',
            Self::Ddot => '\u{308}',
            Self::Dddot => '\u{20DB}',
            Self::Breve => '\u{306}',
            Self::Bar => '\u{304}',
            Self::Vec | Self::OverRightArrow => '\u{20D7}',
            Self::Ring => '\u{30A}',
            Self::OverLeftArrow => '\u{20D6}',
            Self::OverLeftRightArrow => '\u{20E1}',
        }
    }

    /// The spacing (non-combining) form of the accent, as a math font has it (`^`, `˜`, `→`): what the text
    /// output draws on its own row over a base wider than one cell. Unicode has no spacing triple dot, so
    /// `Dddot` gives the horizontal ellipsis `…` (U+2026), which shows three dots in one cell as the other
    /// spacing forms show their accent.
    #[must_use]
    pub const fn spacing(self) -> char {
        match self {
            Self::Hat | Self::WideHat => '^',
            Self::Check | Self::WideCheck => 'ˇ',
            Self::Tilde | Self::WideTilde => '~',
            Self::Acute => '´',
            Self::Grave => '`',
            Self::Dot => '˙',
            Self::Ddot => '¨',
            Self::Dddot => '…',
            Self::Breve => '˘',
            Self::Bar => '¯',
            Self::Vec | Self::OverRightArrow => '→',
            Self::Ring => '˚',
            Self::OverLeftArrow => '←',
            Self::OverLeftRightArrow => '↔',
        }
    }

    /// Whether the accent stretches to the width of its base (`\widehat`, `\overrightarrow`) rather than being
    /// one fixed-size glyph centred on it.
    #[must_use]
    pub const fn is_wide(self) -> bool {
        matches!(
            self,
            Self::WideHat
                | Self::WideTilde
                | Self::WideCheck
                | Self::OverRightArrow
                | Self::OverLeftArrow
                | Self::OverLeftRightArrow
        )
    }
}

/// The kind of an array environment, which decides its default columns, delimiters and spacing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArrayKind {
    /// `matrix` and the delimited `pmatrix`, `bmatrix`, `Bmatrix`, `vmatrix`, `Vmatrix`: centred columns.
    Matrix,
    /// `smallmatrix`: a matrix in script size.
    SmallMatrix,
    /// `cases`: two left-aligned columns after a left brace.
    Cases,
    /// `aligned`, `align*`, `split`: columns aligned right, left, right, left, ... (`a &= b`).
    Aligned,
    /// `gathered`, `gather*`, `equation`: one centred column.
    Gathered,
    /// `array{…}`: columns from the specification.
    Array,
}

/// The alignment of a column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    /// `l`.
    Left,
    /// `c`.
    Center,
    /// `r`.
    Right,
}

/// An entry of an array's column specification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Column {
    /// A column with its alignment.
    Align(Align),
    /// A vertical rule (`|`) between columns.
    Rule,
}

/// An array environment.
#[derive(Debug, Clone, PartialEq)]
pub struct Array {
    /// Which environment it was.
    pub kind: ArrayKind,
    /// The column specification: as written for `array`, the kind's default for the others (centred for
    /// matrices, left for `cases`, right and left alternating for `Aligned`). It always has an alignment for
    /// every cell of the widest row: cells beyond an `array`'s specification get centred columns.
    pub columns: Vec<Column>,
    /// The rows, each a list of cells, each cell a math list.
    pub rows: Vec<Vec<Vec<Node>>>,
    /// Horizontal rules (`\hline`): the row boundaries they are drawn at, `0` above the first row, `rows.len()`
    /// below the last; a boundary may appear twice for a double rule.
    pub hlines: Vec<usize>,
    /// The delimiters around it (`pmatrix` is `(` and `)`, `cases` is `{` and nothing); `None` for none.
    pub open: Option<char>,
    /// The closing delimiter; `None` for none.
    pub close: Option<char>,
}

/// A node of a math list.
#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    /// A single character with its class: a letter, digit or symbol.
    Char(MathChar),
    /// A large or named operator.
    Op {
        /// The symbol or the name.
        name: OpName,
        /// Where its scripts go.
        limits: Limits,
    },
    /// A braced group `{…}`: an ordinary atom in TeX.
    Group(Vec<Self>),
    /// A base with a subscript, a superscript or both. Primes (`f'`) are a superscript of `′`, `″`, `‴` or `⁗`
    /// first in the superscript list. The base is an empty [`Node::Group`] when there was none (`{}^2`, `^2`).
    Scripts {
        /// The base.
        base: Box<Self>,
        /// The subscript.
        sub: Option<Vec<Self>>,
        /// The superscript.
        sup: Option<Vec<Self>>,
    },
    /// A generalised fraction: `\frac` and `\binom` with their `d`/`t` forms.
    Frac {
        /// The numerator.
        num: Vec<Self>,
        /// The denominator.
        den: Vec<Self>,
        /// Whether the fraction rule is drawn (`false` for `\binom`).
        bar: bool,
        /// The delimiters around it (`(` and `)` for `\binom`), `None` for none.
        open: Option<char>,
        /// The closing delimiter, `None` for none.
        close: Option<char>,
        /// A forced style (`\dfrac` is `Display`, `\tfrac` is `Text`), `None` for the usual one.
        style: Option<Style>,
    },
    /// A radical: `\sqrt{…}` or `\sqrt[n]{…}`.
    Sqrt {
        /// The degree, if given.
        degree: Option<Vec<Self>>,
        /// The radicand.
        body: Vec<Self>,
    },
    /// `\left … \right`: delimiters grown to the height of what they enclose. `None` is `.`, no delimiter.
    /// `\middle` delimiters are [`Node::Middle`] directly in `body`.
    Fenced {
        /// The left delimiter.
        open: Option<char>,
        /// The content, with its `\middle` delimiters.
        body: Vec<Self>,
        /// The right delimiter.
        close: Option<char>,
    },
    /// A `\middle` delimiter; only directly inside a [`Node::Fenced`] body. `None` is `.`.
    Middle(Option<char>),
    /// A delimiter at a fixed size: `\big`, `\Big`, `\bigg`, `\Bigg` and their `l`/`r`/`m` forms.
    Big {
        /// The delimiter (`None` for `.`).
        delim: Option<char>,
        /// 1 for `\big` (1.2 times the normal size in TeX), 2 for `\Big` (1.8), 3 for `\bigg` (2.4), 4 for
        /// `\Bigg` (3.0).
        size: u8,
        /// The class: `Open` for `\bigl`, `Close` for `\bigr`, `Rel` for `\bigm`, `Ord` for plain `\big`.
        class: Class,
    },
    /// An accent over a base.
    Accent {
        /// Which accent.
        accent: Accent,
        /// The base.
        base: Vec<Self>,
    },
    /// `\overline{…}`: a rule over the base.
    Overline(Vec<Self>),
    /// `\underline{…}`: a rule under the base.
    Underline(Vec<Self>),
    /// `\overbrace{…}^{label}` or `\underbrace{…}_{label}`; the label (taken from the script right after the
    /// brace) is set above an over-brace, below an under-brace.
    Brace {
        /// `true` for `\overbrace`, `false` for `\underbrace`.
        over: bool,
        /// What the brace spans.
        body: Vec<Self>,
        /// The label.
        label: Option<Vec<Self>>,
    },
    /// `\overset`, `\underset`, `\stackrel`: a base with something set (in script style) above or below it.
    Stack {
        /// The base.
        base: Vec<Self>,
        /// What goes above.
        over: Option<Vec<Self>>,
        /// What goes below.
        under: Option<Vec<Self>>,
        /// The class of the whole: `Rel` for `\stackrel`, the base's class when it is a single atom otherwise.
        class: Class,
    },
    /// An alphabet change (`\mathbb{R}`; also the old `\rm`, `\bf`... switches, which hold the rest of the group).
    Font {
        /// The alphabet.
        font: MathFont,
        /// What it applies to.
        body: Vec<Self>,
    },
    /// A text run (`\text{…}`), an ordinary atom.
    Text {
        /// Its font.
        font: TextFont,
        /// The text, with any math inside it.
        body: Vec<TextPart>,
    },
    /// A style change (`\displaystyle`), holding the rest of its group.
    Style {
        /// The style.
        style: Style,
        /// The rest of the group.
        body: Vec<Self>,
    },
    /// A colour (`\color{red}` holding the rest of its group, or `\textcolor{red}{…}`); the name or `#rrggbb` as
    /// written.
    Color {
        /// The colour, as written.
        color: String,
        /// What it applies to.
        body: Vec<Self>,
    },
    /// A list forced to a class (`\mathrel{…}`, `\mathop{…}`, ...).
    Class {
        /// The class.
        class: Class,
        /// The list.
        body: Vec<Self>,
        /// Where scripts go when the class is [`Class::Op`] (`\mathop{…}`): [`Limits::Display`] unless
        /// `\limits`, `\nolimits` or `\displaylimits` follow, as for a built-in operator. `None` for the
        /// other classes.
        limits: Option<Limits>,
    },
    /// Explicit spacing (`\,`, `\quad`, `\hspace{…}`, `~`).
    Space(Space),
    /// An array environment (`pmatrix`, `cases`, `aligned`, `array`, ...).
    Array(Array),
}

impl Node {
    /// The class of the node as an atom, as TeX's spacing sees it (a fraction and `\left…\right` are inner,
    /// a group and a text run ordinary, a script node takes its base's). `None` for nodes that are not atoms
    /// (spacing, a `\middle`) and for style or colour changes, whose content counts instead.
    #[must_use]
    pub fn class(&self) -> Option<Class> {
        match self {
            Self::Char(c) => Some(c.class),
            Self::Op { .. } | Self::Brace { .. } => Some(Class::Op),
            Self::Scripts { base, .. } => base.class(),
            Self::Frac { .. } | Self::Fenced { .. } => Some(Class::Inner),
            Self::Big { class, .. } | Self::Stack { class, .. } | Self::Class { class, .. } => {
                Some(*class)
            }
            Self::Middle(_) | Self::Space(_) | Self::Style { .. } | Self::Color { .. } => None,
            Self::Group(_)
            | Self::Sqrt { .. }
            | Self::Accent { .. }
            | Self::Overline(_)
            | Self::Underline(_)
            | Self::Font { .. }
            | Self::Text { .. }
            | Self::Array(_) => Some(Class::Ord),
        }
    }
}

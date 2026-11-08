---
title: API reference
parent: quench-tex
grand_parent: Crates
nav_order: 2
---

The crate root (`src/lib.rs`) re-exports:

```rust
pub use ast::{
    Accent, Align, Array, ArrayKind, Class, Column, Formula, Limits, MathChar, MathFont, Node,
    OpName, Space, Style, TextFont, TextPart,
};
pub use macros::Macros;
pub use symbols::{large_op, named_op, symbol};
pub use text::{TextBlock, to_text_block, to_unicode};
pub use unicode::{display_width, math_alphanumeric};

#[cfg(feature = "typeset")]
pub mod typeset;
```

and defines `Error`, `parse` and `parse_with` itself. The modules `ast`, `lexer`, `macros`, `parse`, `symbols`, `text` and `unicode` are private; `typeset` is the only public module. No `unsafe`. Every type is plain data (`Send + Sync`); nothing has interior mutability or global state.

## Errors

### `Error`

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// A command or environment the reader does not know (or a form of one it does not support).
    Unsupported {
        command: String,   // as written, with its backslash: `\foo`, `\begin{tabular}`
        at: usize,         // byte offset in the source
    },
    /// The formula is not well-formed TeX.
    Syntax {
        at: usize,         // byte offset where the problem was found
        message: String,   // what is wrong
    },
}

impl Error {
    #[must_use] pub const fn at(&self) -> usize   // the byte offset of either variant
}
impl fmt::Display for Error
impl std::error::Error for Error
```

| Variant | `Display` |
|---|---|
| `Unsupported { command, at }` | ``unsupported TeX command `{command}` at byte {at}`` |
| `Syntax { at, message }` | `TeX syntax error at byte {at}: {message}` |

`at` is a **byte** offset into the string given to `parse`, so `&tex[at..]` is where the problem starts. Offsets of tokens that came from a macro expansion are the offset of the macro call. Which input gives which variant, and the message texts, are listed in [supported TeX](supported.md#errors).

## Reading

### `parse`

```rust
pub fn parse(tex: &str) -> Result<Formula, Error>
```

Reads a formula: the TeX between the `$` signs, without them. Macros defined inside it apply to the rest of that formula only. Errors: `Error::Unsupported` for an unknown command or environment, `Error::Syntax` for malformed TeX, a nesting deeper than 150 groups or arguments, or a runaway macro.

### `parse_with`

```rust
pub fn parse_with(tex: &str, macros: &mut Macros) -> Result<Formula, Error>
```

As `parse`, with the definitions in `macros` in force; the definitions the formula makes (`\newcommand`, `\renewcommand`, `\providecommand`, `\def`, `\gdef`, `\DeclareMathOperator`) are kept in `macros` for the formulas after it. The formula is read against a clone: **a formula that fails changes nothing** in `macros`.

### `Macros`

```rust
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Macros { /* private: name (without backslash) -> definition */ }
```

| Method | Signature | Behaviour |
|---|---|---|
| `new` | `#[must_use] pub fn new() -> Self` | No definitions (same as `Default`). |
| `define` | `pub fn define(&mut self, name: &str, params: u8, body: &str) -> Result<(), Error>` | Defines `\name` (given without the backslash) as `body`, which uses `#1`…`#params` for its arguments, as `\newcommand{\name}[params]{body}` does. Redefines an existing macro. Errors (all `Error::Syntax`; nothing is defined then): `params` over 9 (at byte 0: `` `\ten` takes 0 to 9 arguments, not `10` ``), the body does not tokenize (``a lone `\` at the end``), or it uses a parameter above `params` (``the body uses #2 but the macro takes 1 arguments``). |
| `contains` | `#[must_use] pub fn contains(&self, name: &str) -> bool` | Whether `\name` (without the backslash) is defined. |
| `len` | `#[must_use] pub fn len(&self) -> usize` | How many macros are defined. |
| `is_empty` | `#[must_use] pub fn is_empty(&self) -> bool` | Whether none is. |

A user macro shadows a built-in command of the same name (`\renewcommand{\frac}[2]{#1:#2}` works). Expansion limits: 10 000 expansions per formula and 100 000 pending tokens ([internals](internals.md#macros)).

## The syntax tree (`ast.rs`)

Characters are stored as the Unicode characters they stand for (`\alpha` is `α`, `-` is `−`); letters stay plain Latin letters (the math italic is a matter of layout), and alphabet changes are `Node::Font` around them.

### `Formula`

```rust
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Formula {
    pub body: Vec<Node>,   // the math list, in order
}
```

A formula with `\\` at the top level (outside any environment) is one `Node::Array` of kind `Gathered` with one centred column, as KaTeX does; a trailing empty line is dropped.

### `Class`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Class { Ord, Op, Bin, Rel, Open, Close, Punct, Inner }
```

TeX's atom classes (*The TeXbook*, chapter 18), which decide the spacing between neighbours: ordinary (letters, digits, most symbols), large or named operator, binary operator, relation, opening, closing (`)`, `!`, `?`), punctuation (`,`, `;`), inner (`\left…\right`, fractions, `\cdots`). The tree keeps the class as written: a `-` at the start of a list is `Bin` in the tree and becomes ordinary only in the outputs (TeX's rules 5 and 6).

### `MathChar`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MathChar {
    pub ch: char,      // the Unicode character (`α`, `≤`, `−`, a plain `x`)
    pub class: Class,
}
```

### `Limits` and `OpName`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Limits {
    Display,   // above and below in display style, beside otherwise (`\sum`, `\lim`; TeX's `\displaylimits`)
    Always,    // always above and below (`\limits`)
    Never,     // always beside (`\nolimits`; integrals and `\sin`-like operators by default)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpName {
    Symbol(char),    // a large symbol, drawn bigger in display style: `∑`, `∫`
    Named(String),   // a name set upright in the roman font: `sin`, `lim sup`, `\operatorname{rank}`
}
```

### `Style`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style { Display, Text, Script, ScriptScript }   // `\displaystyle` … `\scriptscriptstyle`
```

### `MathFont` and `TextFont`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MathFont {
    Roman,          // \mathrm (also \mathup, \rm)
    Italic,         // \mathit (\it)
    Bold,           // \mathbf (\bf)
    BoldSymbol,     // \boldsymbol, \bm, \pmb: bold, symbols and Greek included
    Blackboard,     // \mathbb, \Bbb
    Calligraphic,   // \mathcal (\cal): Unicode's script letters
    Script,         // \mathscr
    Fraktur,        // \mathfrak
    SansSerif,      // \mathsf (\sf)
    Monospace,      // \mathtt (\tt)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextFont {
    Normal,      // \text, \mbox, \hbox, \textrm, \textnormal, \textup
    Bold,        // \textbf
    Italic,      // \textit, \emph
    SansSerif,   // \textsf
    Monospace,   // \texttt
}
```

Unicode has one script alphabet, so `Calligraphic` and `Script` give the same characters (`\mathcal{L}` and `\mathscr{L}` are both `ℒ`).

### `TextPart`

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum TextPart {
    Str(String),                    // text, spaces kept (a run of spaces is one space, as in TeX)
    Math(Vec<Node>),                // math inside the text: `\text{for $x > 0$}`
    Styled(TextFont, Vec<Self>),    // a nested change of font: `\text{a \textbf{b}}`
    Space(Space),                   // explicit spacing (`\quad`, `\ `)
}
```

### `Space`

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Space {
    Mu(f32),   // math units, 1/18 em of the math font: `\,` 3, `\:` 4, `\;` 5, `\!` -3
    Em(f32),   // ems of the current font: `\quad` 1, `\qquad` 2; `\hspace` lengths converted, 10 pt to the em
}

impl Space {
    #[must_use] pub fn em(self) -> f32   // the amount in ems (18 mu to the em)
}
```

### `Accent`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Accent {
    Hat, Check, Tilde, Acute, Grave, Dot, Ddot, Dddot, Breve, Bar, Vec, Ring,
    WideHat, WideTilde, WideCheck, OverRightArrow, OverLeftArrow, OverLeftRightArrow,
}
```

| Method | Signature | Returns |
|---|---|---|
| `combining` | `#[must_use] pub const fn combining(self) -> char` | The combining character: U+0302 hat and wide hat, U+030C check, U+0303 tilde, U+0301 acute, U+0300 grave, U+0307 dot, U+0308 ddot, U+20DB dddot, U+0306 breve, U+0304 bar, U+20D7 vec and `\overrightarrow`, U+030A ring, U+20D6 `\overleftarrow`, U+20E1 `\overleftrightarrow`. |
| `spacing` | `#[must_use] pub const fn spacing(self) -> char` | The spacing (non-combining) form: `^`, `ˇ`, `~`, `´`, `` ` ``, `˙`, `¨`, `…` (U+2026 for `Dddot`: Unicode has no spacing triple dot, so the ellipsis stands in), `˘`, `¯`, `→`, `˚`, `←`, `↔`. |
| `is_wide` | `#[must_use] pub const fn is_wide(self) -> bool` | `true` for `WideHat`, `WideTilde`, `WideCheck` and the three arrows: they stretch to the base's width. |

### Arrays: `ArrayKind`, `Align`, `Column`, `Array`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArrayKind {
    Matrix,        // matrix, pmatrix, bmatrix, Bmatrix, vmatrix, Vmatrix: centred columns
    SmallMatrix,   // smallmatrix: a matrix in script size
    Cases,         // cases, rcases: two left-aligned columns after a left brace (before a right one)
    Aligned,       // aligned, align, align*, split, alignedat: columns right, left, right, left, ...
    Gathered,      // gathered, gather, gather*, equation, equation*: one centred column
    Array,         // array{…}: columns from the specification
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align { Left, Center, Right }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Column { Align(Align), Rule }   // Rule: a `|` between columns

#[derive(Debug, Clone, PartialEq)]
pub struct Array {
    pub kind: ArrayKind,
    pub columns: Vec<Column>,          // as written for `array`, the kind's default for the others
    pub rows: Vec<Vec<Vec<Node>>>,     // rows of cells, each cell a math list
    pub hlines: Vec<usize>,            // row boundaries with an `\hline`: 0 above the first row, rows.len() below the last
    pub open: Option<char>,            // `pmatrix` is '(' … ')', `cases` is '{' and None
    pub close: Option<char>,
}
```

`columns` always has an alignment for every cell of the widest row: cells beyond an `array`'s specification get centred columns. A boundary may appear twice in `hlines` for a double rule. A final row that is one empty cell (after a trailing `\\`) is dropped.

### `Node`

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Char(MathChar),
    Op { name: OpName, limits: Limits },
    Group(Vec<Self>),
    Scripts { base: Box<Self>, sub: Option<Vec<Self>>, sup: Option<Vec<Self>> },
    Frac { num: Vec<Self>, den: Vec<Self>, bar: bool, open: Option<char>, close: Option<char>, style: Option<Style> },
    Sqrt { degree: Option<Vec<Self>>, body: Vec<Self> },
    Fenced { open: Option<char>, body: Vec<Self>, close: Option<char> },
    Middle(Option<char>),
    Big { delim: Option<char>, size: u8, class: Class },
    Accent { accent: Accent, base: Vec<Self> },
    Overline(Vec<Self>),
    Underline(Vec<Self>),
    Brace { over: bool, body: Vec<Self>, label: Option<Vec<Self>> },
    Stack { base: Vec<Self>, over: Option<Vec<Self>>, under: Option<Vec<Self>>, class: Class },
    Font { font: MathFont, body: Vec<Self> },
    Text { font: TextFont, body: Vec<TextPart> },
    Style { style: Style, body: Vec<Self> },
    Color { color: String, body: Vec<Self> },
    Class { class: Class, body: Vec<Self>, limits: Option<Limits> },
    Space(Space),
    Array(Array),
}
```

| Variant | Made by | Notes |
|---|---|---|
| `Char` | a character, a symbol command | `'` alone is `′` (ordinary) |
| `Op` | `\sum`, `∑` typed, `\sin`, `\operatorname`, `\DeclareMathOperator` | `\limits`/`\nolimits`/`\displaylimits` right after change `limits` (also of a `\mathop` `Class`) |
| `Group` | `{…}` | an ordinary atom |
| `Scripts` | `^`, `_`, primes | Primes (`f'`) are a superscript of `′`, `″`, `‴` or `⁗` first in the superscript list (four primes per `⁗`). The base is an empty `Group` when there was none (`{}^2`, `^2`). A script after something that is not an atom (a space, a style change) also gets an empty base. |
| `Frac` | `\frac`, `\dfrac`, `\tfrac`, `\cfrac`, `\binom`, `\dbinom`, `\tbinom` | `bar` is `false` for the binomials, which have `open: Some('(')`, `close: Some(')')`; `style` is `Some(Display)` for the `d` forms and `\cfrac`, `Some(Text)` for the `t` forms |
| `Sqrt` | `\sqrt`, `\sqrt[n]` | `degree` is `Some(vec![])` for `\sqrt[]{x}` |
| `Fenced` | `\left … \right` | `None` is `.`; the `\middle`s are `Middle` nodes directly in `body` |
| `Middle` | `\middle` | only directly inside a `Fenced` body |
| `Big` | `\big`, `\Big`, `\bigg`, `\Bigg` and their `l`/`r`/`m` forms | `size` 1 to 4 (1.2, 1.8, 2.4, 3.0 times the normal size in TeX); `class` `Open` for `l`, `Close` for `r`, `Rel` for `m`, `Ord` plain |
| `Accent` | `\hat` … `\overleftrightarrow` | |
| `Overline`, `Underline` | `\overline`, `\underline` | |
| `Brace` | `\overbrace{…}^{label}`, `\underbrace{…}_{label}` | the label is the script right after the brace (`^` for over, `_` for under) |
| `Stack` | `\overset`, `\underset`, `\stackrel` | `class` is `Rel` for `\stackrel`, the base's class when it is a single atom otherwise, else `Ord` |
| `Font` | `\mathbb{…}` etc., and `\rm`, `\bf`, `\it`, `\sf`, `\tt`, `\cal`, which hold the rest of the group | |
| `Text` | `\text{…}` and friends | |
| `Style` | `\displaystyle` etc., holding the rest of the group | |
| `Color` | `\color{red}` (rest of the group) or `\textcolor{red}{…}` | the colour as written; with a model, `model:value` (`\color[rgb]{1,0,0}` is `"rgb:1,0,0"`) |
| `Class` | `\mathord`, `\mathop`, `\mathbin`, `\mathrel`, `\mathopen`, `\mathclose`, `\mathpunct`, `\mathinner` | `limits` is `Some(Limits::Display)` for `\mathop` (changed by a following `\limits`, `\nolimits` or `\displaylimits`, as for `Op`) and `None` for the other classes |
| `Space` | `\,`, `\quad`, `\hspace{…}`, `~`, `\ ` | |
| `Array` | the environments, top-level `\\` | |

```rust
impl Node {
    #[must_use] pub fn class(&self) -> Option<Class>
}
```

The node's class as TeX's spacing sees it: `Char` its own; `Op` and `Brace` are `Op`; `Scripts` its base's; `Frac` and `Fenced` are `Inner`; `Big`, `Stack` and `Class` their `class`; `Group`, `Sqrt`, `Accent`, `Overline`, `Underline`, `Font`, `Text` and `Array` are `Ord`; `None` for `Middle`, `Space`, `Style` and `Color` (whose content counts instead).

## Symbol tables (`symbols.rs`)

| Function | Signature | Returns |
|---|---|---|
| `symbol` | `#[must_use] pub fn symbol(command: &str) -> Option<MathChar>` | The character and class a symbol command stands for, the name given without backslash: `symbol("le")` is `≤`, `Rel`. Large and named operators are not symbols. |
| `large_op` | `#[must_use] pub fn large_op(command: &str) -> Option<(char, Limits)>` | `large_op("sum")` is `('∑', Limits::Display)`; `large_op("int")` is `('∫', Limits::Never)`. |
| `named_op` | `#[must_use] pub fn named_op(command: &str) -> Option<(&'static str, Limits)>` | `named_op("limsup")` is `("lim sup", Limits::Display)`; `named_op("sin")` is `("sin", Limits::Never)`. |

The full tables are on the [supported TeX](supported.md) page.

## Text output (`text.rs`, `unicode.rs`)

### `to_unicode`

```rust
#[must_use] pub fn to_unicode(formula: &Formula) -> String
```

The formula as one line of Unicode text, trimmed: `\frac{a+b}{2} \le \sqrt{x}` is `(a + b)/2 ≤ √x`. Fractions are `a/b` (parenthesised parts when they are more than one term), scripts are Unicode super- and subscript forms when every character has one, else `^x`/`_x` for one character and `^(…)`/`_(…)` for more; colours are dropped. Rules in [internals](internals.md#text-output).

### `to_text_block` and `TextBlock`

```rust
#[must_use] pub fn to_text_block(formula: &Formula) -> TextBlock

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextBlock {
    pub lines: Vec<String>,   // the rows, top to bottom, without trailing spaces
    pub baseline: usize,      // the row the formula's baseline is on (where a one-line neighbour would sit)
    pub width: usize,         // the width in terminal cells of the widest row
}
```

The formula laid out in two dimensions for display math: stacked fractions, raised scripts, limits above and below, grown delimiters, aligned arrays. The common left indent of the non-empty rows is removed. **The caller centres it.** `width` is measured with `display_width`, so it counts cells, not `char`s.

### `display_width`

```rust
#[must_use] pub fn display_width(text: &str) -> usize
```

The width of `text` in terminal cells: combining marks, zero-width spaces and joiners and variation selectors take none (U+0300–036F, U+0483–0489, U+0591–05BD, U+0610–061A, U+064B–065F, U+1AB0–1AFF, U+1DC0–1DFF, U+200B–200F, U+2060–2064, U+20D0–20FF, U+FE00–FE0F, U+FE20–FE2F, U+E0100–E01EF); East Asian wide characters take two (Hangul Jamo, CJK, Hangul syllables, compatibility ideographs, full-width forms, the emoji blocks U+1F300–1F64F and U+1F900–1F9FF, planes 2 and 3); every other character one. Emoji sequences count as one picture, by the same rules as quench-markdown-term's `display_width`, so a formula measured here fits there:

* a visible character other than whitespace followed by U+FE0F (emoji presentation) takes two cells; U+FE0E and U+FE0F themselves take none;
* skin-tone modifiers (U+1F3FB–1F3FF) take none;
* after a two-cell cluster, U+200D (ZWJ) joins the next character when that is two cells wide or in U+2000–2BFF, and the joined character adds nothing (👨‍👩‍👧 is 2); a ZWJ anywhere else takes none and joins nothing (`a‍b` is 2);
* two regional indicators (a flag) take two cells for the pair, a lone one takes one;
* other zero-width characters are absorbed into the cluster before them.

`display_width("x\u{302}y\u{305}")` is 2, `display_width("数学")` is 4, `display_width("❤\u{fe0f}")` is 2.

### `math_alphanumeric`

```rust
#[must_use] pub fn math_alphanumeric(ch: char, font: MathFont) -> char
```

`ch` in the alphabet of `font`, from the Mathematical Alphanumeric Symbols block: `math_alphanumeric('R', MathFont::Blackboard)` is `ℝ`. The letters Unicode encoded before the block fill its holes (`ℂ ℍ ℕ ℙ ℚ ℝ ℤ` double-struck; `ℬ ℰ ℱ ℋ ℐ ℒ ℳ ℛ ℯ ℊ ℴ` script; `ℭ ℌ ℑ ℜ ℨ` Fraktur). Digits exist for bold, double-struck, sans-serif and monospace. `Bold` and `BoldSymbol` also map Greek (capitals except U+03A2, small letters, `∇`, `∂` and the variant forms `ϵ ϑ ϰ ϕ ϱ ϖ`). Characters the alphabet does not have, and every character for `Roman` and `Italic`, come back unchanged.

## The typesetter (`typeset`, feature `typeset`)

```rust
pub use quench_raster::Color;
pub use quench_text::Font;
pub use draw::{Raster, RenderOptions, to_png, to_rgba};
pub use svg::{SvgOptions, to_svg};
```

Every position in the display list is in **font units of the math font** (Noto Sans Math has 1000 to the em), with **y pointing up**, the formula's **baseline at y = 0** and its left edge at x = 0. Drawing converts to pixels (y down) or to ems.

### `typeset::Error`

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Parse(crate::Error),     // the TeX could not be read (tex_to_png, tex_to_svg)
    MissingGlyph(char),      // a character the fonts have no glyph for
    UnknownColour(String),   // a colour name or value that is not known (`\color{…}`)
    NotMathFont,             // the math font has no `MATH` table (or no constants in it)
}
impl fmt::Display for Error
impl std::error::Error for Error
impl From<crate::Error> for Error   // so `?` works on `parse` inside a function returning typeset::Error
```

| Variant | `Display` |
|---|---|
| `Parse(e)` | `e`'s own message |
| `MissingGlyph(ch)` | ``the math font has no glyph for `{ch}` (U+{:04X})``, e.g. ``the math font has no glyph for `漢` (U+6F22)`` |
| `UnknownColour(name)` | ``unknown colour `{name}` `` |
| `NotMathFont` | `the font has no MATH table: it is not a math font` |

`MissingGlyph` is raised when neither the math font nor (inside `\text`) the text face has the character; a formula is typeset whole or not at all.

### `Face`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Face {
    Math,         // the math font
    Text,         // the text font (`\text`, also `\textsf` and non-alphanumerics of `\texttt`)
    TextBold,     // the bold text font (`\textbf`)
    TextItalic,   // the text font slanted (`\textit`: Noto Sans has no italic embedded, so it leans)
}

impl Face {
    #[must_use] pub const fn slant(self) -> f32   // 0.2 for TextItalic, 0.0 otherwise
}
```

`slant` is how far the face leans: every point moves right by this much times its height.

### `Fonts`

```rust
#[derive(Debug, Clone)]
pub struct Fonts {
    pub math: Arc<Font>,        // the math font, with a `MATH` table
    pub text: Arc<Font>,        // for `\text`
    pub text_bold: Arc<Font>,   // for `\textbf`
}
```

| Method | Signature | Behaviour |
|---|---|---|
| `bundled` | `#[must_use] pub fn bundled() -> Self` | `quench_text::noto_sans_math()`, `noto_sans()`, `noto_sans_bold()`: all embedded, shared `Arc`s. |
| `with_math` | `pub fn with_math(math: Arc<Font>) -> Result<Self, Error>` | Another math font with Noto Sans for text. `Error::NotMathFont` when `math.math()` has no constants. |
| `face` | `#[must_use] pub fn face(&self, face: Face) -> &Font` | `Math` → `math`; `Text` and `TextItalic` → `text`; `TextBold` → `text_bold`. |

### `Item`

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Item {
    Glyph {
        font: Face,              // the font it is from
        glyph: u16,              // the glyph id in that font
        x: f32,                  // the x of the glyph's origin
        y: f32,                  // the y of the glyph's baseline
        size: f32,               // the size of the glyph's em in the same units: 1000 at text size, 600 in a script, 500 in a scriptscript (Noto Sans Math)
        colour: Option<Color>,   // None: the colour the formula is drawn in
    },
    Rule {                       // fraction bar, radical overbar, array line: a filled rectangle
        x: f32,                  // the left edge
        y: f32,                  // the bottom edge
        width: f32,
        height: f32,             // the thickness, for a horizontal rule
        colour: Option<Color>,
    },
}

impl Item {
    #[must_use] pub const fn moved(self, dx: f32, dy: f32) -> Self   // the item moved by (dx, dy)
}
```

A glyph is drawn from its outline scaled by `size / units_per_em` of its own face, so a text face with another em size lines up. A `Rule`'s `(x, y)` is its bottom left corner (y up).

### `Typeset`

```rust
#[derive(Debug, Clone)]
pub struct Typeset {
    pub width: f32,          // the advance width, font units
    pub ascent: f32,         // how far the box reaches above the baseline
    pub descent: f32,        // how far the box reaches below the baseline (positive)
    pub units_per_em: f32,   // font units per em of the positions (the math font's)
    pub items: Vec<Item>,    // the glyphs and rules
    pub fonts: Fonts,        // the fonts the glyphs are from
}

impl Typeset {
    #[must_use] pub fn ink(&self) -> Option<(f32, f32, f32, f32)>
}
```

`ink` is the extent of what is drawn, `(x_min, y_min, x_max, y_max)` in font units, from each glyph's `Font::bounds` (scaled, slant included) and the rules; `None` when nothing is drawn. The ink can reach past the box (an italic letter's overhang, a large operator), which is why the drawing functions use the union of the box and the ink. `\frac{a}{b}` in display style has `ascent` 1249, `descent` 683 and `width` 819 with Noto Sans Math.

### `typeset`, `typeset_with`

```rust
pub fn typeset(formula: &Formula, display: bool) -> Result<Typeset, Error>
pub fn typeset_with(formula: &Formula, display: bool, fonts: &Fonts) -> Result<Typeset, Error>
```

Lay a parsed formula out in display style (`display: true`, `$$…$$`) or text style (`$…$`); `typeset` uses `Fonts::bundled()`. Errors: `MissingGlyph`, `UnknownColour`, `NotMathFont` (only `typeset_with`, for a font whose `MATH` table or constants are missing).

### `tex_to_png`, `tex_to_svg`

```rust
pub fn tex_to_png(tex: &str, display: bool, options: &RenderOptions) -> Result<Vec<u8>, Error>
pub fn tex_to_svg(tex: &str, display: bool, options: &SvgOptions) -> Result<String, Error>
```

`parse` (errors as `Error::Parse`), `typeset` with the bundled fonts, then `to_png` or `to_svg`. No macros: use `parse_with` and `typeset` for a page with definitions.

### `RenderOptions`, `Raster`, `to_rgba`, `to_png`

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderOptions {
    pub pixels_per_em: f32,          // the size of the formula's em in pixels (the font size)
    pub colour: Color,               // the colour of everything not coloured by `\color`
    pub background: Option<Color>,   // transparent when None
    pub padding: u32,                // pixels of space on every side
}
impl Default for RenderOptions      // 32.0 px per em, Color::BLACK, None, 2

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Raster {
    pub width: u32,
    pub height: u32,
    pub baseline: u32,   // the row the baseline is at, from the top
    pub rgba: Vec<u8>,   // straight alpha, `width * 4` bytes a row
}

#[must_use] pub fn to_rgba(set: &Typeset, options: &RenderOptions) -> Raster
#[must_use] pub fn to_png(set: &Typeset, options: &RenderOptions) -> Vec<u8>
```

`to_rgba` fills every glyph outline (`Outline::to_path_slanted` with the face's slant) and rule antialiased on a `quench_raster::Canvas`, cleared to `background` first when one is given, and reads the pixels back as straight-alpha RGBA. The image holds the union of the box and the ink, rounded outwards to whole pixels, plus `padding`, and is at least 1×1; the baseline falls on a pixel boundary, `baseline` rows from the top (the larger of the ascent and the ink's top, in pixels rounded up, plus the padding), so an image can be lined up with text. `to_png` is `to_rgba` then `quench_image::encode_png`. Neither can fail.

`x^2` in text style at 24 px per em is a 27×24 raster with its baseline at row 22.

### `SvgOptions`, `to_svg`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SvgOptions {
    pub colour: Option<Color>,   // None: `currentColor`
    pub padding: f32,            // space on every side, in ems (negative is treated as 0)
}

#[must_use] pub fn to_svg(set: &Typeset, options: &SvgOptions) -> String
```

An `<svg>` element (no XML declaration, so it can be put inline in HTML): `width` and `height` in `em`, a `viewBox` in ems with y down and the baseline at 0, `style="vertical-align: …em"` set to minus what lies below the baseline (the larger of the descent and the ink's bottom, plus the padding), so an inline `<svg>` sits on the text's baseline, and `fill` set to `currentColor` or `options.colour` as `#rrggbb` (plus `fill-opacity` when not opaque). Inside, one `<path d="…"/>` per glyph that has an outline (quadratic curves as TrueType draws them) and one `<rect/>` per rule; items coloured by `\color` carry their own `fill`. Numbers have at most four decimals. Example head for `\int_0^1 x^2\,dx` in text style:

```text
<svg xmlns="http://www.w3.org/2000/svg" width="3.6867em" height="1.4654em" viewBox="0 -1.0319 3.6867 1.4654" style="vertical-align: -0.4335em" fill="currentColor">
```

### Colours

`\color` and `\textcolor` values are resolved when typesetting (`UnknownColour` otherwise): the xcolor base names `black`, `white`, `red` (255, 0, 0), `green` (0, 255, 0), `blue`, `cyan`, `magenta`, `yellow`, `gray`/`grey` (128), `darkgray`/`darkgrey` (64), `lightgray`/`lightgrey` (191), `brown` (191, 128, 64), `lime` (191, 255, 0), `olive` (128, 128, 0), `orange` (255, 128, 0), `pink` (255, 191, 191), `purple` (191, 0, 64), `teal` (0, 128, 128), `violet` (128, 0, 128), case-insensitive; `#` followed by hex digits (`Color::from_hex`: `#rgb`, `#rrggbb`, `#rrggbbaa`); and the models `[HTML]{FF8000}` (also `html`; exactly 6 digits), `[rgb]{1,0.5,0}` (0 to 1, clamped), `[RGB]{255,128,0}` (0 to 255, clamped) and `[gray]{0.5}`. The reader keeps the argument as written, so every hex form works, whether it starts with a letter or a digit (`#1a2b3c`, `#123`). The text output ignores colours.

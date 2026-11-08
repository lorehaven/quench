//! Text output: a formula as one line of Unicode ([`to_unicode`]), or as a two-dimensional block of terminal
//! cells for display math ([`to_text_block`]).
//!
//! Both are the same walk over the tree. Each node becomes an item (a block of lines with a baseline, and the
//! TeX class it has for spacing); a list is its items side by side on their baselines, with a space where TeX
//! puts a medium or thick space (around binary operators and relations), after punctuation, and around named
//! operators. Inline, every block is one line: fractions are `a/b`, scripts Unicode forms or `^(…)`. In a block,
//! fractions stack over a rule, scripts that are not short Unicode forms are raised or lowered a line, limits go
//! above and below operators in display style, and delimiters grow from Unicode's bracket pieces.
//!
//! The choices where TeX's look cannot be had in cells: `∑` and `∏` are a single character with their limits
//! above and below (a three-line `⎲`/`⎳` reads worse in most terminal fonts); integrals are one `∫` with their
//! limits beside it; a script stays inline in a block when it is at most two characters that all have Unicode
//! forms (`x²`, `aᵢⱼ`, `x⁻¹`), and is raised a line otherwise; a radical is `√` with a row of `_` over the
//! radicand.

use crate::ast::{
    Accent, Align, Array, ArrayKind, Class, Column, Formula, Limits, MathFont, Node, OpName, Space,
    Style, TextPart,
};
use crate::unicode::{display_width, is_zero_width, math_alphanumeric, scripted};

/// A formula laid out in terminal cells.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextBlock {
    /// The rows, top to bottom, without trailing spaces.
    pub lines: Vec<String>,
    /// The row the formula's baseline is on (where a single-line neighbour would sit).
    pub baseline: usize,
    /// The width in cells of the widest row.
    pub width: usize,
}

/// The formula as one line of Unicode text: `\frac{a+b}{2} \le \sqrt{x}` is `(a + b)/2 ≤ √x`.
#[must_use]
pub fn to_unicode(formula: &Formula) -> String {
    let block = list(&formula.body, Ctx::INLINE);
    block
        .lines
        .first()
        .map_or_else(String::new, |line| line.trim().to_owned())
}

/// The formula laid out in two dimensions for display math: stacked fractions, raised scripts, limits above
/// and below, grown delimiters, aligned arrays. The caller centres it.
#[must_use]
pub fn to_text_block(formula: &Formula) -> TextBlock {
    let block = list(&formula.body, Ctx::DISPLAY);
    let lines: Vec<String> = block
        .lines
        .iter()
        .map(|line| line.trim_end().to_owned())
        .collect();
    let indent = lines
        .iter()
        .filter(|line| !line.is_empty())
        .map(|line| line.len() - line.trim_start_matches(' ').len())
        .min()
        .unwrap_or(0);
    let lines: Vec<String> = lines
        .iter()
        .map(|line| line.get(indent..).unwrap_or("").to_owned())
        .collect();
    let width = lines
        .iter()
        .map(|line| display_width(line))
        .max()
        .unwrap_or(0);
    TextBlock {
        lines,
        baseline: block.baseline,
        width,
    }
}

// ---- Blocks.

/// Lines of text, each padded to `width` cells, with a baseline row.
#[derive(Debug, Clone)]
struct Block {
    lines: Vec<String>,
    baseline: usize,
    width: usize,
}

impl Block {
    fn text(text: impl Into<String>) -> Self {
        let text = text.into();
        let width = display_width(&text);
        Self {
            lines: vec![text],
            baseline: 0,
            width,
        }
    }

    fn empty() -> Self {
        Self::text("")
    }

    const fn height(&self) -> usize {
        self.lines.len()
    }

    const fn descent(&self) -> usize {
        self.lines.len() - 1 - self.baseline
    }

    const fn is_line(&self) -> bool {
        self.lines.len() == 1
    }

    fn line(&self) -> &str {
        self.lines.first().map_or("", String::as_str)
    }

    /// The blocks side by side, on their baselines.
    fn hcat(blocks: &[Self]) -> Self {
        let ascent = blocks.iter().map(|b| b.baseline).max().unwrap_or(0);
        let descent = blocks.iter().map(Self::descent).max().unwrap_or(0);
        let lines = (0..=ascent + descent)
            .map(|row| {
                let mut line = String::new();
                for block in blocks {
                    match (row + block.baseline)
                        .checked_sub(ascent)
                        .and_then(|r| block.lines.get(r))
                    {
                        Some(text) => line.push_str(text),
                        None => line.push_str(&" ".repeat(block.width)),
                    }
                }
                line
            })
            .collect();
        Self {
            lines,
            baseline: ascent,
            width: blocks.iter().map(|b| b.width).sum(),
        }
    }

    /// The blocks one under the other, aligned in the widest's width, the baseline on row `baseline`.
    fn vstack(blocks: &[Self], align: Align, baseline: usize) -> Self {
        let width = blocks.iter().map(|b| b.width).max().unwrap_or(0);
        let lines: Vec<String> = blocks
            .iter()
            .flat_map(|b| b.lines.iter().map(move |line| pad(line, width, align)))
            .collect();
        let baseline = baseline.min(lines.len().saturating_sub(1));
        Self {
            lines,
            baseline,
            width,
        }
    }

    /// The block grown with blank rows to `ascent` rows above its baseline and `descent` below.
    fn extend(&self, ascent: usize, descent: usize) -> Self {
        let blank = " ".repeat(self.width);
        let mut lines = vec![blank.clone(); ascent.saturating_sub(self.baseline)];
        lines.extend(self.lines.iter().cloned());
        lines.extend(std::iter::repeat_n(
            blank,
            descent.saturating_sub(self.descent()),
        ));
        Self {
            lines,
            baseline: ascent.max(self.baseline),
            width: self.width,
        }
    }

    fn padded(&self, width: usize, align: Align) -> Self {
        Self {
            lines: self
                .lines
                .iter()
                .map(|line| pad(line, width, align))
                .collect(),
            baseline: self.baseline,
            width: width.max(self.width),
        }
    }

    fn parenthesised(&self) -> Self {
        Self::hcat(&[
            delimiter(Some('('), self.baseline, self.descent()),
            self.clone(),
            delimiter(Some(')'), self.baseline, self.descent()),
        ])
    }
}

fn pad(line: &str, width: usize, align: Align) -> String {
    let extra = width.saturating_sub(display_width(line));
    let left = match align {
        Align::Left => 0,
        Align::Center => extra / 2,
        Align::Right => extra,
    };
    format!("{}{line}{}", " ".repeat(left), " ".repeat(extra - left))
}

// ---- Context and items.

/// How a list is being set.
#[derive(Debug, Clone, Copy)]
struct Ctx {
    /// One line (`to_unicode`), or a block.
    inline: bool,
    /// Display style: limits above and below.
    display: bool,
    /// Script style: no spaces around binary operators and relations.
    script: bool,
    /// The alphabet letters are set in.
    font: Option<MathFont>,
}

impl Ctx {
    const INLINE: Self = Self {
        inline: true,
        display: false,
        script: false,
        font: None,
    };
    const DISPLAY: Self = Self {
        inline: false,
        display: true,
        script: false,
        font: None,
    };

    const fn script(self) -> Self {
        Self {
            display: false,
            script: true,
            ..self
        }
    }

    const fn text(self) -> Self {
        Self {
            display: false,
            ..self
        }
    }

    const fn style(self, style: Style) -> Self {
        match style {
            Style::Display => Self {
                display: true,
                script: false,
                ..self
            },
            Style::Text => Self {
                display: false,
                script: false,
                ..self
            },
            Style::Script | Style::ScriptScript => self.script(),
        }
    }
}

/// A `\middle` delimiter waiting for its height.
#[derive(Debug, Clone, Copy)]
struct Middle(Option<char>);

/// A node set: its block and what spacing needs to know about it.
#[derive(Debug, Clone)]
struct Item {
    block: Block,
    /// The class; `None` for explicit spacing.
    class: Option<Class>,
    /// An operator that wants a space after it (and before it when named): `sin`, `∑` with scripts.
    spaced_op: bool,
    /// An inline `a/b` fraction, parenthesised when something touches it.
    frac: bool,
    /// A `\middle` delimiter, grown once the height of its `\left…\right` is known.
    middle: Option<Middle>,
}

impl Item {
    const fn new(block: Block, class: Option<Class>) -> Self {
        Self {
            block,
            class,
            spaced_op: false,
            frac: false,
            middle: None,
        }
    }
}

fn list(nodes: &[Node], ctx: Ctx) -> Block {
    join(items(nodes, ctx), ctx)
}

fn items(nodes: &[Node], ctx: Ctx) -> Vec<Item> {
    let mut out = Vec::new();
    for node in nodes {
        match node {
            Node::Style { style, body } => out.extend(items(body, ctx.style(*style))),
            Node::Color { body, .. } => out.extend(items(body, ctx)),
            _ => out.push(item(node, ctx)),
        }
    }
    out
}

/// The classes TeX spaces by: a binary operator with nothing to operate on is ordinary (`-x`, `(+1)`).
fn effective_classes(items: &[Item]) -> Vec<Option<Class>> {
    let mut classes: Vec<Option<Class>> = items.iter().map(|i| i.class).collect();
    let mut previous: Option<usize> = None;
    for i in 0..classes.len() {
        let Some(class) = classes[i] else { continue };
        let before = previous.and_then(|p| classes[p]);
        if class == Class::Bin
            && matches!(
                before,
                None | Some(Class::Bin | Class::Op | Class::Rel | Class::Open | Class::Punct)
            )
        {
            classes[i] = Some(Class::Ord);
        }
        if matches!(class, Class::Rel | Class::Close | Class::Punct)
            && let Some(p) = previous
            && classes[p] == Some(Class::Bin)
        {
            classes[p] = Some(Class::Ord);
        }
        previous = Some(i);
    }
    if let Some(p) = previous
        && classes[p] == Some(Class::Bin)
    {
        classes[p] = Some(Class::Ord);
    }
    classes
}

/// Cells between two neighbouring atoms: TeX's spacing table (*The `TeXbook`*, chapter 18) with medium and thick
/// spaces as one cell, thin spaces as one cell after punctuation and next to operators, none otherwise.
fn gap(left: (Class, &Item), right: (Class, &Item), ctx: Ctx) -> usize {
    use Class::{Bin, Close, Inner, Op, Open, Ord, Punct, Rel};
    let (a, b) = (left.0, right.0);
    // 0 none, 1 thin, 2 medium, 3 thick; `true` when only in display and text style.
    let (space, styles_only) = match (a, b) {
        (Ord | Close | Inner, Op) | (Op, Ord | Op) => (1, false),
        (Ord | Close | Op, Inner) | (Inner, Ord | Open | Punct | Inner) | (Punct, _) => (1, true),
        (Ord | Close | Inner, Bin) | (Bin, Ord | Op | Open | Inner) => (2, true),
        (Ord | Op | Close | Inner, Rel) | (Rel, Ord | Op | Open | Inner) => (3, true),
        _ => (0, false),
    };
    if space == 0 || (styles_only && ctx.script) {
        return 0;
    }
    if space > 1 || a == Punct {
        return 1;
    }
    let named_right = b == Op
        && right.1.spaced_op
        && matches!(right.1.block.line().chars().next(), Some(c) if c.is_alphabetic());
    let after_op = a == Op && (left.1.spaced_op || b != Op);
    usize::from(after_op || named_right)
}

fn join(mut items: Vec<Item>, ctx: Ctx) -> Block {
    let classes = effective_classes(&items);
    if ctx.inline {
        parenthesise_touching_fractions(&mut items, &classes);
    }
    let mut blocks = Vec::with_capacity(items.len() * 2);
    let mut previous: Option<(Class, usize)> = None;
    for (i, item) in items.iter().enumerate() {
        match classes[i] {
            Some(class) => {
                if let Some((left, p)) = previous {
                    let cells = gap((left, &items[p]), (class, item), ctx);
                    if cells > 0 {
                        blocks.push(Block::text(" ".repeat(cells)));
                    }
                }
                previous = Some((class, i));
            }
            None => previous = None,
        }
        blocks.push(item.block.clone());
    }
    if blocks.is_empty() {
        return Block::empty();
    }
    Block::hcat(&blocks)
}

/// Inline, `1/2x` would read as `1/(2x)`: a fraction with an atom right against it is parenthesised.
fn parenthesise_touching_fractions(items: &mut [Item], classes: &[Option<Class>]) {
    for (i, item) in items.iter_mut().enumerate() {
        if !item.frac {
            continue;
        }
        let before = i.checked_sub(1).and_then(|p| classes[p]);
        let after = classes.get(i + 1).copied().flatten();
        if matches!(before, Some(Class::Ord | Class::Close | Class::Inner))
            || matches!(after, Some(Class::Ord | Class::Open | Class::Inner))
        {
            let text = format!("({})", item.block.line());
            item.block = Block::text(text);
            item.frac = false;
        }
    }
}

// ---- Nodes.

fn item(node: &Node, ctx: Ctx) -> Item {
    match node {
        Node::Char(c) => Item::new(Block::text(char_text(c.ch, ctx.font)), Some(c.class)),
        Node::Op { name, .. } => {
            let text = match name {
                OpName::Symbol(ch) => ch.to_string(),
                OpName::Named(name) => name.clone(),
            };
            Item {
                spaced_op: matches!(name, OpName::Named(_)),
                ..Item::new(Block::text(text), Some(Class::Op))
            }
        }
        Node::Group(body) => group(body, ctx),
        Node::Scripts { base, sub, sup } => scripts(base, sub.as_deref(), sup.as_deref(), ctx),
        Node::Frac {
            num,
            den,
            bar,
            open,
            close,
            style,
        } => fraction(num, den, *bar, (*open, *close), *style, ctx),
        Node::Sqrt { degree, body } => radical(degree.as_deref(), body, ctx),
        Node::Fenced { open, body, close } => fenced(*open, body, *close, ctx),
        Node::Middle(delim) => Item {
            middle: Some(Middle(*delim)),
            ..Item::new(Block::text(delim_text(*delim)), Some(Class::Rel))
        },
        Node::Big { delim, size, class } => {
            let block = if *size >= 3 && !ctx.inline {
                delimiter(*delim, 1, 1)
            } else {
                Block::text(delim_text(*delim))
            };
            Item::new(block, Some(*class))
        }
        Node::Accent { accent, base } => decorated(base, Decoration::Accent(*accent), ctx),
        Node::Overline(base) => decorated(base, Decoration::Overline, ctx),
        Node::Underline(base) => decorated(base, Decoration::Underline, ctx),
        Node::Brace { over, body, label } => brace(*over, body, label.as_deref(), ctx),
        Node::Stack {
            base,
            over,
            under,
            class,
        } => stack(base, over.as_deref(), under.as_deref(), *class, ctx),
        Node::Font { font, body } => {
            let font = match font {
                MathFont::Roman | MathFont::Italic => None,
                other => Some(*other),
            };
            group(body, Ctx { font, ..ctx })
        }
        Node::Text { body, .. } => Item::new(Block::text(text_parts(body)), Some(Class::Ord)),
        Node::Style { style, body } => Item::new(list(body, ctx.style(*style)), Some(Class::Ord)),
        Node::Color { body, .. } => Item::new(list(body, ctx), Some(Class::Ord)),
        Node::Class { class, body, .. } => Item {
            spaced_op: *class == Class::Op,
            ..Item::new(list(body, ctx), Some(*class))
        },
        Node::Space(space) => Item::new(Block::text(" ".repeat(space_cells(*space))), None),
        Node::Array(array) => Item::new(array_block(array, ctx), Some(Class::Ord)),
    }
}

fn char_text(ch: char, font: Option<MathFont>) -> String {
    let ch = font.map_or(ch, |font| math_alphanumeric(ch, font));
    // The dot operator U+22C5 is missing from many terminal fonts; the middle dot looks the same.
    let ch = if ch == '⋅' { '·' } else { ch };
    ch.to_string()
}

fn delim_text(delim: Option<char>) -> String {
    delim.map(String::from).unwrap_or_default()
}

/// A group is an ordinary atom; a group of one item keeps that item's look (`{\frac12}` is still a fraction).
fn group(body: &[Node], ctx: Ctx) -> Item {
    let mut parts = items(body, ctx);
    if parts.len() == 1 && parts[0].class.is_some() {
        let mut only = parts.remove(0);
        only.class = Some(Class::Ord);
        only.spaced_op = false;
        return only;
    }
    Item::new(join(parts, ctx), Some(Class::Ord))
}

fn space_cells(space: Space) -> usize {
    let cells = space.em() * 2.0;
    if cells.is_nan() || cells <= 0.0 {
        return 0;
    }
    (1_u8..=40)
        .filter(|&n| f32::from(n) - 0.5 <= cells)
        .count()
        .max(1)
}

fn text_parts(parts: &[TextPart]) -> String {
    let mut out = String::new();
    for part in parts {
        match part {
            TextPart::Str(text) => out.push_str(text),
            TextPart::Styled(_, parts) => out.push_str(&text_parts(parts)),
            TextPart::Space(space) => out.push_str(&" ".repeat(space_cells(*space))),
            TextPart::Math(body) => out.push_str(list(body, Ctx::INLINE).line()),
        }
    }
    out
}

// ---- Scripts.

/// Where an operator puts its limits: `Some(true)` above and below.
fn op_limits(node: &Node, ctx: Ctx) -> Option<bool> {
    let limits = match node {
        Node::Op { limits, .. }
        | Node::Class {
            limits: Some(limits),
            ..
        } => *limits,
        _ => return None,
    };
    Some(!ctx.inline && (limits == Limits::Always || (limits == Limits::Display && ctx.display)))
}

/// Whether a script base needs parentheses to show what the script is on (`(a + b)²`).
fn base_needs_parens(base: &Node, ctx: Ctx) -> bool {
    match base {
        Node::Group(body) => {
            body.len() > 1
                && !body
                    .iter()
                    .all(|n| matches!(n, Node::Char(c) if c.ch.is_ascii_digit() || c.ch == '.'))
        }
        Node::Frac { bar: true, .. } | Node::Sqrt { .. } => ctx.inline,
        _ => false,
    }
}

/// The primes at the start of a superscript, and the rest of it.
fn split_primes(sup: Option<&[Node]>) -> (String, Option<&[Node]>) {
    let Some(sup) = sup else {
        return (String::new(), None);
    };
    let count = sup
        .iter()
        .take_while(|n| matches!(n, Node::Char(c) if matches!(c.ch, '′' | '″' | '‴' | '⁗')))
        .count();
    let primes = sup[..count]
        .iter()
        .filter_map(|n| {
            if let Node::Char(c) = n {
                Some(c.ch)
            } else {
                None
            }
        })
        .collect();
    let rest = &sup[count..];
    (primes, (!rest.is_empty()).then_some(rest))
}

/// A script inline: its Unicode forms, `^c` for one character without a form, `^(…)` otherwise.
fn inline_script(nodes: &[Node], sup: bool, ctx: Ctx) -> String {
    let text = list(nodes, ctx.script()).line().trim().to_owned();
    if text.is_empty() {
        return text;
    }
    if let Some(forms) = scripted(&text, sup) {
        return forms;
    }
    let mark = if sup { '^' } else { '_' };
    if text.chars().filter(|&c| !is_zero_width(c)).count() == 1 {
        format!("{mark}{text}")
    } else {
        format!("{mark}({text})")
    }
}

/// A script that may stay inline in a block: short, one line, every character with a Unicode form.
fn short_forms(block: &Block, sup: bool) -> Option<String> {
    let text = block.line().trim();
    if !block.is_line() || display_width(text) > 2 {
        return None;
    }
    scripted(text, sup)
}

fn scripts(base: &Node, sub: Option<&[Node]>, sup: Option<&[Node]>, ctx: Ctx) -> Item {
    let base_item = item(base, ctx);
    let class = base_item.class.or(Some(Class::Ord));
    let op = op_limits(base, ctx);
    let mut base_block = base_item.block;
    if base_needs_parens(base, ctx) {
        base_block = if ctx.inline {
            Block::text(format!("({})", base_block.line()))
        } else {
            base_block.parenthesised()
        };
    }
    let (primes, sup) = split_primes(sup);
    let block = if ctx.inline {
        let mut text = format!("{}{primes}", base_block.line());
        if let Some(sub) = sub {
            text.push_str(&inline_script(sub, false, ctx));
        }
        if let Some(sup) = sup {
            text.push_str(&inline_script(sup, true, ctx));
        }
        Block::text(text)
    } else {
        if !primes.is_empty() {
            base_block = Block::hcat(&[base_block, Block::text(primes)]);
        }
        let sub = sub.map(|s| list(s, ctx.script()));
        let sup = sup.map(|s| list(s, ctx.script()));
        if op == Some(true) {
            limits_block(&base_block, sub.as_ref(), sup.as_ref())
        } else {
            beside(base_block, sub.as_ref(), sup.as_ref(), op.is_some())
        }
    };
    Item {
        spaced_op: op.is_some() || base_item.spaced_op,
        ..Item::new(block, class)
    }
}

/// Limits centred above and below an operator.
fn limits_block(op: &Block, sub: Option<&Block>, sup: Option<&Block>) -> Block {
    let mut parts = Vec::new();
    let mut baseline = op.baseline;
    if let Some(sup) = sup {
        baseline += sup.height();
        parts.push(sup.clone());
    }
    parts.push(op.clone());
    if let Some(sub) = sub {
        parts.push(sub.clone());
    }
    Block::vstack(&parts, Align::Center, baseline)
}

/// Scripts beside a base: as Unicode forms when both are short, else raised and lowered a line.
fn beside(base: Block, sub: Option<&Block>, sup: Option<&Block>, op: bool) -> Block {
    if base.is_line() && !op {
        let lower_forms = sub.map(|s| short_forms(s, false));
        let upper_forms = sup.map(|s| short_forms(s, true));
        if !matches!(lower_forms, Some(None)) && !matches!(upper_forms, Some(None)) {
            let text = format!(
                "{}{}{}",
                base.line(),
                lower_forms.flatten().unwrap_or_default(),
                upper_forms.flatten().unwrap_or_default()
            );
            return Block {
                lines: vec![text],
                baseline: 0,
                width: 0,
            }
            .measured();
        }
    }
    if sub.is_none() && sup.is_none() {
        return base;
    }
    // Rows relative to the base's top: a superscript ends on the row above a one-line base, on the top row of
    // a taller one; a subscript starts on the row below, or on the bottom row.
    let height = base.height();
    let above_bottom: isize = if height == 1 { -1 } else { 0 };
    let below_top = if height == 1 { 1 } else { height - 1 };
    let above_top = sup.map_or(0, |s| above_bottom - signed(s.height()) + 1);
    let top = above_top.min(0);
    let bottom = sub.map_or(height - 1, |s| (below_top + s.height() - 1).max(height - 1));
    let width = sub.map_or(0, |s| s.width).max(sup.map_or(0, |s| s.width));
    let rows = usize::try_from(signed(bottom) - top + 1).unwrap_or(1);
    let mut column = vec![" ".repeat(width); rows];
    let mut place = |block: &Block, first: isize| {
        for (i, line) in block.lines.iter().enumerate() {
            if let Ok(row) = usize::try_from(first - top + signed(i))
                && let Some(slot) = column.get_mut(row)
            {
                *slot = pad(line, width, Align::Left);
            }
        }
    };
    if let Some(sup) = sup {
        place(sup, above_top);
    }
    if let Some(sub) = sub {
        place(sub, signed(below_top));
    }
    let baseline = usize::try_from(signed(base.baseline) - top).unwrap_or(0);
    let column = Block {
        lines: column,
        baseline,
        width,
    };
    Block::hcat(&[base, column])
}

fn signed(n: usize) -> isize {
    isize::try_from(n).unwrap_or(isize::MAX)
}

impl Block {
    fn measured(mut self) -> Self {
        self.width = self
            .lines
            .iter()
            .map(|l| display_width(l))
            .max()
            .unwrap_or(0);
        self
    }
}

// ---- Fractions and radicals.

/// Whether an inline fraction side is one term (`2x`, `x²`) and needs no parentheses.
fn one_term(items: &[Item]) -> bool {
    items
        .iter()
        .all(|i| matches!(i.class, Some(Class::Ord | Class::Open | Class::Close)) && !i.frac)
        && !items.iter().any(|i| i.class.is_none())
}

/// Whether a list is one atom (or a number) that a radical sign can sit on without parentheses.
fn single(items: &[Item]) -> bool {
    match items {
        [only] => !only.frac && only.class.is_some(),
        _ => {
            items.iter().all(|i| {
                i.block
                    .line()
                    .chars()
                    .all(|c| c.is_ascii_digit() || c == '.')
            }) || wrapped(items)
        }
    }
}

/// Whether a list is already wholly inside one pair of delimiters (`(a + b)`).
fn wrapped(items: &[Item]) -> bool {
    let (Some(first), Some(last)) = (items.first(), items.last()) else {
        return false;
    };
    if first.class != Some(Class::Open) || last.class != Some(Class::Close) || items.len() < 2 {
        return false;
    }
    let mut depth = 0_i32;
    for (i, item) in items.iter().enumerate() {
        match item.class {
            Some(Class::Open) => depth += 1,
            Some(Class::Close) => depth -= 1,
            _ => {}
        }
        if depth == 0 && i + 1 < items.len() {
            return false;
        }
    }
    true
}

fn fraction(
    num: &[Node],
    den: &[Node],
    bar: bool,
    (open, close): (Option<char>, Option<char>),
    style: Option<Style>,
    ctx: Ctx,
) -> Item {
    let inner = match style {
        Some(Style::Display) => ctx.style(Style::Display),
        _ => ctx.text(),
    };
    if ctx.inline {
        let num_items = items(num, inner);
        let den_items = items(den, inner);
        let side = |parts: Vec<Item>| {
            let term = one_term(&parts);
            let text = join(parts, inner).line().to_owned();
            if term && !text.is_empty() {
                text
            } else {
                format!("({text})")
            }
        };
        if !bar {
            let text = format!(
                "{}{} {}{}",
                delim_text(open),
                join(num_items, inner).line(),
                join(den_items, inner).line(),
                delim_text(close)
            );
            return Item::new(Block::text(text), Some(Class::Inner));
        }
        let text = format!("{}/{}", side(num_items), side(den_items));
        return Item {
            frac: true,
            ..Item::new(Block::text(text), Some(Class::Inner))
        };
    }
    let num = list(num, inner);
    let den = list(den, inner);
    let width = num.width.max(den.width);
    let middle = Block::text(if bar {
        "─".repeat(width)
    } else {
        " ".repeat(width)
    });
    let stacked = Block::vstack(&[num.clone(), middle, den], Align::Center, num.height());
    let block = if open.is_some() || close.is_some() {
        let (ascent, descent) = (stacked.baseline, stacked.descent());
        Block::hcat(&[
            delimiter(open, ascent, descent),
            stacked,
            delimiter(close, ascent, descent),
        ])
    } else {
        stacked
    };
    Item::new(block, Some(Class::Inner))
}

/// The radical sign for a degree: `√`, `∛`, `∜`, or the degree in superscript before `√`.
fn radical_sign(degree: Option<&[Node]>) -> String {
    let Some(degree) = degree else {
        return "√".to_owned();
    };
    let text = list(degree, Ctx::INLINE.script()).line().trim().to_owned();
    match text.as_str() {
        "" | "2" => "√".to_owned(),
        "3" => "∛".to_owned(),
        "4" => "∜".to_owned(),
        _ => {
            scripted(&text, true).map_or_else(|| format!("({text})√"), |forms| format!("{forms}√"))
        }
    }
}

fn radical(degree: Option<&[Node]>, body: &[Node], ctx: Ctx) -> Item {
    let sign = radical_sign(degree);
    if ctx.inline {
        let parts = items(body, ctx);
        let simple = single(&parts) && !parts.is_empty();
        let text = join(parts, ctx).line().to_owned();
        let text = if simple {
            format!("{sign}{text}")
        } else {
            format!("{sign}({text})")
        };
        return Item::new(Block::text(text), Some(Class::Ord));
    }
    let body = list(body, ctx.text());
    let sign_width = display_width(&sign);
    let mut lines = vec![format!(
        "{}{}",
        " ".repeat(sign_width),
        "_".repeat(body.width)
    )];
    let last = body.height() - 1;
    for (row, line) in body.lines.iter().enumerate() {
        let prefix = if row == last {
            sign.clone()
        } else {
            format!("{}│", " ".repeat(sign_width - 1))
        };
        lines.push(format!("{prefix}{line}"));
    }
    let block = Block {
        lines,
        baseline: body.baseline + 1,
        width: sign_width + body.width,
    };
    Item::new(block, Some(Class::Ord))
}

// ---- Delimiters.

/// A delimiter `ascent + descent + 1` rows tall, built from Unicode's bracket pieces; one character when one row.
fn delimiter(delim: Option<char>, ascent: usize, descent: usize) -> Block {
    let height = ascent + descent + 1;
    let Some(ch) = delim else {
        return Block {
            lines: vec![String::new(); height],
            baseline: ascent,
            width: 0,
        };
    };
    if height == 1 {
        return Block::text(ch.to_string());
    }
    let middle = if ascent > 0 && ascent < height - 1 {
        ascent
    } else {
        height / 2
    };
    let lines = (0..height)
        .map(|row| piece(ch, row, height, middle).to_string())
        .collect();
    Block {
        lines,
        baseline: ascent,
        width: 1,
    }
}

/// The piece of a tall delimiter on `row` of `height` (`middle` is where a brace's point goes).
const fn piece(ch: char, row: usize, height: usize, middle: usize) -> char {
    let (top, bottom) = (row == 0, row == height - 1);
    match ch {
        '(' => pick(top, bottom, '⎛', '⎜', '⎝'),
        ')' => pick(top, bottom, '⎞', '⎟', '⎠'),
        '[' => pick(top, bottom, '⎡', '⎢', '⎣'),
        ']' => pick(top, bottom, '⎤', '⎥', '⎦'),
        '⌈' => pick(top, false, '⎡', '⎢', '⎢'),
        '⌉' => pick(top, false, '⎤', '⎥', '⎥'),
        '⌊' => pick(false, bottom, '⎢', '⎢', '⎣'),
        '⌋' => pick(false, bottom, '⎥', '⎥', '⎦'),
        '{' if height == 2 => pick(top, bottom, '⎰', '⎪', '⎱'),
        '}' if height == 2 => pick(top, bottom, '⎱', '⎪', '⎰'),
        '{' if row == middle && !top && !bottom => '⎨',
        '}' if row == middle && !top && !bottom => '⎬',
        '{' => pick(top, bottom, '⎧', '⎪', '⎩'),
        '}' => pick(top, bottom, '⎫', '⎪', '⎭'),
        '|' | '∣' => '│',
        '‖' | '∥' => '‖',
        '⟨' | '⟩' => {
            let opening = matches!(ch, '⟨');
            if height % 2 == 1 && row == height / 2 {
                ch
            } else if (row < height / 2) == opening {
                '╱'
            } else {
                '╲'
            }
        }
        _ if row == middle => ch,
        _ => ' ',
    }
}

const fn pick(top: bool, bottom: bool, first: char, extension: char, last: char) -> char {
    if top {
        first
    } else if bottom {
        last
    } else {
        extension
    }
}

fn fenced(open: Option<char>, body: &[Node], close: Option<char>, ctx: Ctx) -> Item {
    let mut parts = items(body, ctx);
    if ctx.inline {
        let text = format!(
            "{}{}{}",
            delim_text(open),
            join(parts, ctx).line(),
            delim_text(close)
        );
        return Item::new(Block::text(text), Some(Class::Inner));
    }
    let content = parts.iter().filter(|p| p.middle.is_none());
    let ascent = content.clone().map(|p| p.block.baseline).max().unwrap_or(0);
    let descent = content.map(|p| p.block.descent()).max().unwrap_or(0);
    for part in &mut parts {
        if let Some(Middle(delim)) = part.middle {
            part.block = delimiter(delim, ascent, descent);
        }
    }
    let inner = join(parts, ctx);
    let block = Block::hcat(&[
        delimiter(open, ascent, descent),
        inner,
        delimiter(close, ascent, descent),
    ]);
    Item::new(block, Some(Class::Inner))
}

// ---- Accents, braces, stacks.

#[derive(Debug, Clone, Copy)]
enum Decoration {
    Accent(Accent),
    Overline,
    Underline,
}

impl Decoration {
    const fn combining(self) -> char {
        match self {
            Self::Accent(accent) => accent.combining(),
            Self::Overline => '\u{305}',
            Self::Underline => '\u{332}',
        }
    }
}

/// `text` with a combining mark after each visible character (`A̅B̅`).
fn combine_each(text: &str, mark: char) -> String {
    let mut out = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        out.push(c);
        if c == ' ' || is_zero_width(c) {
            continue;
        }
        // Keep marks already on the character together with it.
        while let Some(&next) = chars.peek() {
            if !is_zero_width(next) {
                break;
            }
            out.push(next);
            chars.next();
        }
        out.push(mark);
    }
    out
}

fn decorated(base: &[Node], decoration: Decoration, ctx: Ctx) -> Item {
    let block = list(base, ctx);
    if ctx.inline || (block.is_line() && block.width <= 1) {
        return Item::new(
            Block::text(combine_each(block.line(), decoration.combining())),
            Some(Class::Ord),
        );
    }
    let width = block.width;
    let row = match decoration {
        Decoration::Overline | Decoration::Accent(Accent::Bar) => "_".repeat(width),
        Decoration::Underline => "‾".repeat(width),
        Decoration::Accent(Accent::Vec | Accent::OverRightArrow) => {
            format!("{}→", "─".repeat(width - 1))
        }
        Decoration::Accent(Accent::OverLeftArrow) => format!("←{}", "─".repeat(width - 1)),
        Decoration::Accent(Accent::OverLeftRightArrow) => {
            format!("←{}→", "─".repeat(width.saturating_sub(2)))
        }
        Decoration::Accent(accent) => pad(&accent.spacing().to_string(), width, Align::Center),
    };
    let block = if matches!(decoration, Decoration::Underline) {
        let baseline = block.baseline;
        Block::vstack(&[block, Block::text(row)], Align::Left, baseline)
    } else {
        let baseline = block.baseline + 1;
        Block::vstack(&[Block::text(row), block], Align::Left, baseline)
    };
    Item::new(block, Some(Class::Ord))
}

fn brace(over: bool, body: &[Node], label: Option<&[Node]>, ctx: Ctx) -> Item {
    if ctx.inline {
        let parts = items(body, ctx);
        let term = parts.len() <= 1;
        let text = join(parts, ctx).line().to_owned();
        let mut text = if term { text } else { format!("({text})") };
        if let Some(label) = label {
            text.push_str(&inline_script(label, over, ctx));
        }
        return Item::new(Block::text(text), Some(Class::Ord));
    }
    let block = list(body, ctx);
    let width = block.width;
    let (left, tip, right) = if over {
        ('╭', '┴', '╮')
    } else {
        ('╰', '┬', '╯')
    };
    let row = match width {
        0 => String::new(),
        1 => "─".to_owned(),
        2 => format!("{left}{right}"),
        _ => {
            let half = (width - 3) / 2;
            format!(
                "{left}{}{tip}{}{right}",
                "─".repeat(half),
                "─".repeat(width - 3 - half)
            )
        }
    };
    let label = label.map(|l| list(l, ctx.script()));
    let mut parts = Vec::new();
    let baseline;
    if over {
        baseline = label.as_ref().map_or(0, Block::height) + 1 + block.baseline;
        parts.extend(label);
        parts.push(Block::text(row));
        parts.push(block);
    } else {
        baseline = block.baseline;
        parts.push(block);
        parts.push(Block::text(row));
        parts.extend(label);
    }
    Item::new(
        Block::vstack(&parts, Align::Center, baseline),
        Some(Class::Ord),
    )
}

fn stack(
    base: &[Node],
    over: Option<&[Node]>,
    under: Option<&[Node]>,
    class: Class,
    ctx: Ctx,
) -> Item {
    if ctx.inline {
        let parts = items(base, ctx);
        let term = parts.len() <= 1;
        let text = join(parts, ctx).line().to_owned();
        let mut text = if term { text } else { format!("({text})") };
        if let Some(under) = under {
            text.push_str(&inline_script(under, false, ctx));
        }
        if let Some(over) = over {
            text.push_str(&inline_script(over, true, ctx));
        }
        return Item::new(Block::text(text), Some(class));
    }
    let base = list(base, ctx);
    let over = over.map(|o| list(o, ctx.script()));
    let under = under.map(|u| list(u, ctx.script()));
    Item::new(
        limits_block(&base, under.as_ref(), over.as_ref()),
        Some(class),
    )
}

// ---- Arrays.

fn array_block(array: &Array, ctx: Ctx) -> Block {
    if ctx.inline {
        return Block::text(array_inline(array, ctx));
    }
    let cell_ctx = match array.kind {
        ArrayKind::Aligned | ArrayKind::Gathered => ctx.style(Style::Display),
        ArrayKind::SmallMatrix => ctx.script(),
        ArrayKind::Matrix | ArrayKind::Cases | ArrayKind::Array => ctx.text(),
    };
    let cells: Vec<Vec<Block>> = array
        .rows
        .iter()
        .map(|row| {
            row.iter()
                .enumerate()
                .map(|(c, cell)| cell_block(array.kind, c, cell, cell_ctx))
                .collect()
        })
        .collect();
    let aligns: Vec<Align> = array
        .columns
        .iter()
        .filter_map(|c| {
            if let Column::Align(a) = c {
                Some(*a)
            } else {
                None
            }
        })
        .collect();
    let count = cells.iter().map(Vec::len).max().unwrap_or(0);
    let widths: Vec<usize> = (0..count)
        .map(|c| {
            cells
                .iter()
                .filter_map(|row| row.get(c))
                .map(|b| b.width)
                .max()
                .unwrap_or(0)
        })
        .collect();
    let layout = column_layout(array, count);
    let tall = cells.iter().flatten().any(|b| !b.is_line());
    let total: usize = layout
        .iter()
        .map(|part| match part {
            Part::Cell(c) => widths[*c],
            Part::Gap(n) => *n,
            Part::Rule => 1,
        })
        .sum();
    let mut lines = Vec::new();
    for (r, row) in cells.iter().enumerate() {
        push_hlines(&mut lines, array, r, &layout, &widths);
        if r > 0 && tall && !array.hlines.contains(&r) {
            lines.push(" ".repeat(total));
        }
        let ascent = row.iter().map(|b| b.baseline).max().unwrap_or(0);
        let descent = row.iter().map(Block::descent).max().unwrap_or(0);
        let empty = Block::empty();
        let blocks: Vec<Block> = layout
            .iter()
            .map(|part| match part {
                Part::Cell(c) => {
                    let align = aligns.get(*c).copied().unwrap_or(Align::Center);
                    row.get(*c)
                        .unwrap_or(&empty)
                        .padded(widths[*c], align)
                        .extend(ascent, descent)
                }
                Part::Gap(n) => Block::text(" ".repeat(*n)).extend(ascent, descent),
                Part::Rule => Block {
                    lines: vec!["│".to_owned(); ascent + descent + 1],
                    baseline: ascent,
                    width: 1,
                },
            })
            .collect();
        let joined = if blocks.is_empty() {
            Block::empty().extend(ascent, descent)
        } else {
            Block::hcat(&blocks)
        };
        lines.extend(
            joined
                .lines
                .into_iter()
                .map(|line| pad(&line, total, Align::Left)),
        );
    }
    push_hlines(&mut lines, array, cells.len(), &layout, &widths);
    if lines.is_empty() {
        lines.push(String::new());
    }
    let baseline = (lines.len() - 1) / 2;
    let block = Block {
        lines,
        baseline,
        width: total,
    };
    with_delimiters(array, block)
}

/// The array's delimiters around its block (`cases` keeps a space between its brace and the rows).
fn with_delimiters(array: &Array, block: Block) -> Block {
    if array.open.is_none() && array.close.is_none() {
        return block;
    }
    let (ascent, descent) = (block.baseline, block.descent());
    let gap = Block::text(" ").extend(ascent, descent);
    let mut parts = vec![delimiter(array.open, ascent, descent)];
    if array.kind == ArrayKind::Cases && array.open.is_some() {
        parts.push(gap.clone());
    }
    parts.push(block);
    if array.kind == ArrayKind::Cases && array.close.is_some() {
        parts.push(gap);
    }
    parts.push(delimiter(array.close, ascent, descent));
    Block::hcat(&parts)
}

/// A cell; in an aligned environment the cells after an `&` start as if something were before them, so a
/// relation there is spaced (`&= b` is ` = b`), as amsmath does.
fn cell_block(kind: ArrayKind, column: usize, cell: &[Node], ctx: Ctx) -> Block {
    if kind == ArrayKind::Aligned && column % 2 == 1 {
        let mut nodes = vec![Node::Group(Vec::new())];
        nodes.extend(cell.iter().cloned());
        return list(&nodes, ctx);
    }
    list(cell, ctx)
}

#[derive(Debug, Clone, Copy)]
enum Part {
    Cell(usize),
    Gap(usize),
    Rule,
}

/// The columns of an array left to right: cells, the gaps between them and the vertical rules.
fn column_layout(array: &Array, count: usize) -> Vec<Part> {
    let gap_after = |c: usize| match array.kind {
        ArrayKind::Aligned if c.is_multiple_of(2) => 0,
        ArrayKind::Aligned => 3,
        _ => 2,
    };
    let mut layout = Vec::new();
    let mut cell = 0;
    let mut previous_cell = false;
    for column in &array.columns {
        match column {
            Column::Rule => {
                if previous_cell {
                    layout.push(Part::Gap(1));
                }
                layout.push(Part::Rule);
                previous_cell = false;
            }
            Column::Align(_) if cell < count => {
                if previous_cell {
                    layout.push(Part::Gap(gap_after(cell - 1)));
                } else if matches!(layout.last(), Some(Part::Rule)) {
                    layout.push(Part::Gap(1));
                }
                layout.push(Part::Cell(cell));
                cell += 1;
                previous_cell = true;
            }
            Column::Align(_) => {}
        }
    }
    layout.retain(|part| !matches!(part, Part::Gap(0)));
    layout
}

fn push_hlines(
    lines: &mut Vec<String>,
    array: &Array,
    boundary: usize,
    layout: &[Part],
    widths: &[usize],
) {
    for _ in array.hlines.iter().filter(|&&b| b == boundary) {
        let line: String = layout
            .iter()
            .map(|part| match part {
                Part::Cell(c) => "─".repeat(widths[*c]),
                Part::Gap(n) => "─".repeat(*n),
                Part::Rule => "┼".to_owned(),
            })
            .collect();
        lines.push(line);
    }
}

fn array_inline(array: &Array, ctx: Ctx) -> String {
    let separator = match array.kind {
        ArrayKind::Cases => ", ",
        _ => " ",
    };
    let rows: Vec<String> = array
        .rows
        .iter()
        .map(|row| {
            if array.kind == ArrayKind::Aligned {
                let mut nodes = Vec::new();
                for cell in row {
                    nodes.push(Node::Group(Vec::new()));
                    nodes.extend(cell.iter().cloned());
                }
                return list(&nodes, ctx).line().trim().to_owned();
            }
            let cells: Vec<String> = row
                .iter()
                .map(|cell| list(cell, ctx).line().trim().to_owned())
                .collect();
            cells.join(separator)
        })
        .collect();
    // Inline, the one-sided brace of `cases` is closed, so the line does not read as unbalanced.
    let (open, close) = match (array.kind, array.open, array.close) {
        (ArrayKind::Cases, Some('{'), None) | (ArrayKind::Cases, None, Some('}')) => {
            (Some('{'), Some('}'))
        }
        _ => (array.open, array.close),
    };
    format!(
        "{}{}{}",
        delim_text(open),
        rows.join("; "),
        delim_text(close)
    )
}

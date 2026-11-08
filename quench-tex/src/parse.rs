//! The reader: tokens to the syntax tree, expanding user macros on the way.
//!
//! It is a recursive descent over TeX's math lists. A list is read until a token that ends it (`}`, `&`, `\\`,
//! `\end`, `\right`, `\middle`, a closing `$` or the end), and the construct that asked for the list decides
//! whether that end is right where it is: a `&` ends a cell in an array and is an error anywhere else. Anything
//! unknown stops the whole formula with [`Error::Unsupported`], so no output is ever half a formula.

use crate::Error;
use crate::ast::{
    Accent, Align, Array, ArrayKind, Class, Column, Formula, Limits, MathChar, MathFont, Node,
    OpName, Space, Style, TextFont, TextPart,
};
use crate::lexer::{Tok, Token, tokenize};
use crate::macros::{MAX_EXPANSIONS, MAX_PENDING, Macro, Macros, check_params, substitute};
use crate::symbols;

/// How deeply groups and arguments may nest (deeper is a syntax error, not a stack overflow).
const MAX_DEPTH: usize = 150;

/// Parses `source` with `macros`, which gains the definitions the formula makes.
pub fn parse(source: &str, macros: &mut Macros) -> Result<Formula, Error> {
    let mut parser = Parser {
        tokens: tokenize(source)?,
        next: 0,
        pending: Vec::new(),
        macros,
        expansions: 0,
        depth: 0,
        end: source.len(),
    };
    parser.formula()
}

/// What ended a math list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stop {
    Eof,
    Close,
    Align,
    Row,
    EndEnv,
    Right,
    Middle,
    Bracket,
    Shift,
}

/// The cells of an array row.
type Row = Vec<Vec<Node>>;

/// A math list and what ended it, with the position of that end.
type Ended = (Vec<Node>, Stop, usize);

struct Parser<'m> {
    tokens: Vec<Token>,
    next: usize,
    /// Tokens from macro expansion, to be read before `tokens[next..]`; the last is read first.
    pending: Vec<Token>,
    macros: &'m mut Macros,
    expansions: usize,
    depth: usize,
    end: usize,
}

impl Parser<'_> {
    // ---- Reading tokens.

    fn raw_peek(&self) -> Option<&Token> {
        self.pending.last().or_else(|| self.tokens.get(self.next))
    }

    fn raw_next(&mut self) -> Option<Token> {
        if let Some(token) = self.pending.pop() {
            return Some(token);
        }
        let token = self.tokens.get(self.next).cloned();
        self.next += 1;
        token
    }

    fn raw_skip_spaces(&mut self) {
        while self.raw_peek().is_some_and(|t| t.tok == Tok::Space) {
            self.raw_next();
        }
    }

    /// The next token, after expanding any macro calls in front of it.
    fn peek(&mut self) -> Result<Option<Token>, Error> {
        loop {
            let Some(token) = self.raw_peek() else {
                return Ok(None);
            };
            if let Tok::Command(name) = &token.tok
                && let Some(definition) = self.macros.get(name)
            {
                let (definition, name, at) = (definition.clone(), name.clone(), token.at);
                self.raw_next();
                self.expand(&definition, &name, at)?;
                continue;
            }
            return Ok(Some(token.clone()));
        }
    }

    fn next(&mut self) -> Result<Option<Token>, Error> {
        let token = self.peek()?;
        self.raw_next();
        Ok(token)
    }

    fn skip_spaces(&mut self) -> Result<(), Error> {
        while self.peek()?.is_some_and(|t| t.tok == Tok::Space) {
            self.raw_next();
        }
        Ok(())
    }

    /// Whether the next token (after spaces) is the command `\name`.
    fn at_command(&mut self, name: &str) -> Result<bool, Error> {
        self.skip_spaces()?;
        Ok(self
            .peek()?
            .is_some_and(|t| matches!(&t.tok, Tok::Command(c) if c == name)))
    }

    fn at_char(&mut self, ch: char) -> Result<bool, Error> {
        Ok(self.peek()?.is_some_and(|t| t.tok == Tok::Char(ch)))
    }

    fn expand(&mut self, definition: &Macro, name: &str, at: usize) -> Result<(), Error> {
        self.expansions += 1;
        if self.expansions > MAX_EXPANSIONS {
            return Err(Error::syntax(
                at,
                format!("macro expansion too deep (is `\\{name}` defined in terms of itself?)"),
            ));
        }
        let mut args = Vec::with_capacity(usize::from(definition.params));
        for i in 0..definition.params {
            match &definition.default {
                Some(default) if i == 0 => {
                    self.raw_skip_spaces();
                    if self.raw_peek().is_some_and(|t| t.tok == Tok::Char('[')) {
                        self.raw_next();
                        args.push(self.raw_until_bracket(at)?);
                    } else {
                        args.push(default.clone());
                    }
                }
                _ => args.push(self.raw_arg(at, name)?),
            }
        }
        let body = substitute(definition, &args, at);
        if self.pending.len() + body.len() > MAX_PENDING {
            return Err(Error::syntax(
                at,
                format!("macro expansion too large (in `\\{name}`)"),
            ));
        }
        self.pending.extend(body.into_iter().rev());
        Ok(())
    }

    /// One argument as tokens, unexpanded: a braced group's content or a single token.
    fn raw_arg(&mut self, at: usize, command: &str) -> Result<Vec<Token>, Error> {
        self.raw_skip_spaces();
        match self.raw_next() {
            Some(Token {
                tok: Tok::Begin,
                at: open,
            }) => self.raw_group_rest(open),
            Some(token) if !matches!(token.tok, Tok::End) => Ok(vec![token]),
            _ => Err(missing_argument(at, command)),
        }
    }

    /// The tokens up to the `}` matching an already read `{` at `open`.
    fn raw_group_rest(&mut self, open: usize) -> Result<Vec<Token>, Error> {
        let mut depth = 0_usize;
        let mut out = Vec::new();
        loop {
            let Some(token) = self.raw_next() else {
                return Err(Error::syntax(open, "unbalanced braces: `{` without `}`"));
            };
            match token.tok {
                Tok::Begin => depth += 1,
                Tok::End if depth == 0 => return Ok(out),
                Tok::End => depth -= 1,
                _ => {}
            }
            out.push(token);
        }
    }

    /// The tokens up to a `]` outside braces, the `[` already read.
    fn raw_until_bracket(&mut self, at: usize) -> Result<Vec<Token>, Error> {
        let mut depth = 0_usize;
        let mut out = Vec::new();
        loop {
            let Some(token) = self.raw_next() else {
                return Err(Error::syntax(at, "`[` without `]`"));
            };
            match token.tok {
                Tok::Begin => depth += 1,
                Tok::End if depth == 0 => return Err(Error::syntax(token.at, "`[` without `]`")),
                Tok::End => depth -= 1,
                Tok::Char(']') if depth == 0 => return Ok(out),
                _ => {}
            }
            out.push(token);
        }
    }

    /// A braced group of plain characters, as a string (an environment's name, a colour, a dimension).
    fn raw_word(&mut self, at: usize, command: &str) -> Result<String, Error> {
        let tokens = self.raw_arg(at, command)?;
        let mut word = String::new();
        for token in tokens {
            match token.tok {
                Tok::Char(c) => word.push(c),
                Tok::Space => {}
                _ => {
                    return Err(Error::syntax(
                        token.at,
                        format!("`\\{command}` takes plain characters"),
                    ));
                }
            }
        }
        Ok(word)
    }

    fn enter(&mut self, at: usize) -> Result<(), Error> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(Error::syntax(at, "the formula is nested too deeply"));
        }
        Ok(())
    }

    const fn leave(&mut self) {
        self.depth -= 1;
    }

    // ---- Lists.

    fn formula(&mut self) -> Result<Formula, Error> {
        let (first, stop, at) = self.list(false)?;
        match stop {
            Stop::Eof => Ok(Formula { body: first }),
            Stop::Row => {
                // Lines broken with `\\` outside any environment: shown as `gathered`, as KaTeX does.
                let mut rows = vec![vec![first]];
                loop {
                    self.raw_next();
                    self.skip_row_space()?;
                    let (line, stop, at) = self.list(false)?;
                    rows.push(vec![line]);
                    match stop {
                        Stop::Eof => break,
                        Stop::Row => {}
                        other => return Err(misplaced(other, at)),
                    }
                }
                if rows.last().is_some_and(|row| row[0].is_empty()) {
                    rows.pop();
                }
                let array = Array {
                    kind: ArrayKind::Gathered,
                    columns: vec![Column::Align(Align::Center)],
                    rows,
                    hlines: Vec::new(),
                    open: None,
                    close: None,
                };
                Ok(Formula {
                    body: vec![Node::Array(array)],
                })
            }
            other => Err(misplaced(other, at)),
        }
    }

    /// A math list, up to what ends it (not read). `bracket`: a `]` ends it too (`\sqrt`'s degree).
    fn list(&mut self, bracket: bool) -> Result<Ended, Error> {
        let at = self.raw_peek().map_or(self.end, |t| t.at);
        self.enter(at)?;
        let ended = self.list_items(bracket)?;
        self.leave();
        Ok(ended)
    }

    fn list_items(&mut self, bracket: bool) -> Result<Ended, Error> {
        let mut list = Vec::new();
        loop {
            let Some(token) = self.peek()? else {
                return Ok((list, Stop::Eof, self.end));
            };
            let at = token.at;
            if let Some(stop) = stop_of(&token.tok, bracket) {
                return Ok((list, stop, at));
            }
            match token.tok {
                Tok::Space => {
                    self.raw_next();
                }
                Tok::Sup | Tok::Sub => {
                    self.raw_next();
                    let script = self.script_arg(at)?;
                    attach(&mut list, token.tok == Tok::Sup, script, at)?;
                }
                Tok::Char('\'') => {
                    let script = self.primes()?;
                    attach(&mut list, true, script, at)?;
                }
                Tok::Command(name) => {
                    if let Some(ended) = self.list_command(&name, at, &mut list, bracket)? {
                        return Ok(ended);
                    }
                }
                _ => list.push(self.atom()?),
            }
        }
    }

    /// Commands that act on the list rather than make an atom: definitions, `\limits`, and switches that take
    /// the rest of the list (`\displaystyle`, `\color`, `\bf`), which end it.
    fn list_command(
        &mut self,
        name: &str,
        at: usize,
        list: &mut Vec<Node>,
        bracket: bool,
    ) -> Result<Option<Ended>, Error> {
        let style = match name {
            "displaystyle" => Some(Style::Display),
            "textstyle" => Some(Style::Text),
            "scriptstyle" => Some(Style::Script),
            "scriptscriptstyle" => Some(Style::ScriptScript),
            _ => None,
        };
        let font = match name {
            "rm" => Some(MathFont::Roman),
            "bf" => Some(MathFont::Bold),
            "it" => Some(MathFont::Italic),
            "sf" => Some(MathFont::SansSerif),
            "tt" => Some(MathFont::Monospace),
            "cal" => Some(MathFont::Calligraphic),
            _ => None,
        };
        if style.is_some() || font.is_some() || name == "color" {
            self.raw_next();
            let color = if name == "color" {
                Some(self.color(at)?)
            } else {
                None
            };
            let (body, stop, stop_at) = self.list(bracket)?;
            list.push(match (style, font, color) {
                (Some(style), ..) => Node::Style { style, body },
                (_, Some(font), _) => Node::Font { font, body },
                (.., color) => Node::Color {
                    color: color.unwrap_or_default(),
                    body,
                },
            });
            return Ok(Some((std::mem::take(list), stop, stop_at)));
        }
        match name {
            "newcommand" | "renewcommand" | "providecommand" => {
                self.raw_next();
                self.newcommand(name, at)?;
            }
            "DeclareMathOperator" => {
                self.raw_next();
                self.declare_operator(at)?;
            }
            "def" | "gdef" => {
                self.raw_next();
                self.def(at)?;
            }
            "limits" | "nolimits" | "displaylimits" => {
                self.raw_next();
                let new = match name {
                    "limits" => Limits::Always,
                    "nolimits" => Limits::Never,
                    _ => Limits::Display,
                };
                match list.last_mut() {
                    Some(
                        Node::Op { limits, .. }
                        | Node::Class {
                            limits: Some(limits),
                            ..
                        },
                    ) => *limits = new,
                    _ => {
                        return Err(Error::syntax(
                            at,
                            format!("`\\{name}` must follow an operator"),
                        ));
                    }
                }
            }
            _ => list.push(self.atom()?),
        }
        Ok(None)
    }

    /// `'`, `''`, ... and a superscript right after them: one superscript list, the primes first.
    fn primes(&mut self) -> Result<Vec<Node>, Error> {
        let mut count = 0;
        while self.at_char('\'')? {
            self.raw_next();
            count += 1;
        }
        let mut script = Vec::new();
        let full = [('⁗', 4), ('‴', 3), ('″', 2), ('′', 1)];
        while count > 0 {
            let &(ch, n) = full.iter().find(|&&(_, n)| n <= count).unwrap_or(&('′', 1));
            script.push(Node::Char(MathChar {
                ch,
                class: Class::Ord,
            }));
            count -= n;
        }
        if self.peek()?.is_some_and(|t| t.tok == Tok::Sup) {
            let at = self.raw_next().map_or(self.end, |t| t.at);
            script.extend(self.script_arg(at)?);
        }
        Ok(script)
    }

    /// The argument of `^` or `_`.
    fn script_arg(&mut self, at: usize) -> Result<Vec<Node>, Error> {
        self.skip_spaces()?;
        match self.peek()?.map(|t| t.tok) {
            Some(Tok::Sup | Tok::Sub) => Err(Error::syntax(at, "a script right after a script")),
            _ => self.arg(at, "^"),
        }
    }

    /// A command's argument: a braced group's list or one atom.
    fn arg(&mut self, at: usize, command: &str) -> Result<Vec<Node>, Error> {
        self.skip_spaces()?;
        let Some(token) = self.peek()? else {
            return Err(missing_argument(at, command));
        };
        if stop_of(&token.tok, false).is_some() || matches!(token.tok, Tok::Sup | Tok::Sub) {
            return Err(missing_argument(at, command));
        }
        if token.tok == Tok::Begin {
            self.group()
        } else {
            Ok(vec![self.atom()?])
        }
    }

    /// A braced group's list, from its `{`.
    fn group(&mut self) -> Result<Vec<Node>, Error> {
        let open = self.raw_next().map_or(self.end, |t| t.at);
        let (list, stop, at) = self.list(false)?;
        match stop {
            Stop::Close => {
                self.raw_next();
                Ok(list)
            }
            Stop::Eof => Err(Error::syntax(open, "unbalanced braces: `{` without `}`")),
            other => Err(misplaced(other, at)),
        }
    }

    /// One atom: a character, a group or a command with its arguments.
    fn atom(&mut self) -> Result<Node, Error> {
        let Some(token) = self.peek()? else {
            return Err(Error::syntax(self.end, "unexpected end of the formula"));
        };
        self.enter(token.at)?;
        let node = self.atom_inner(token);
        self.leave();
        node
    }

    fn atom_inner(&mut self, token: Token) -> Result<Node, Error> {
        let at = token.at;
        match token.tok {
            Tok::Begin => Ok(Node::Group(self.group()?)),
            Tok::Command(name) => {
                self.raw_next();
                self.command(&name, at)
            }
            Tok::Char('\'') => {
                self.raw_next();
                Ok(Node::Char(MathChar {
                    ch: '′',
                    class: Class::Ord,
                }))
            }
            Tok::Char('#') => Err(Error::syntax(
                at,
                "a `#` that is not a macro parameter (`#1` to `#9`)",
            )),
            Tok::Char(c) => {
                self.raw_next();
                Ok(symbols::large_op_char(c).map_or_else(
                    || Node::Char(symbols::char_atom(c)),
                    |limits| Node::Op {
                        name: OpName::Symbol(c),
                        limits,
                    },
                ))
            }
            Tok::Tie => {
                self.raw_next();
                Ok(Node::Space(Space::Em(1.0 / 3.0)))
            }
            Tok::Param(_) => Err(Error::syntax(at, "a macro parameter outside a definition")),
            Tok::Shift => Err(Error::syntax(at, "a `$` inside math")),
            Tok::Space | Tok::Sup | Tok::Sub | Tok::End | Tok::Align => {
                Err(Error::syntax(at, "unexpected token"))
            }
        }
    }

    // ---- Commands.

    fn command(&mut self, name: &str, at: usize) -> Result<Node, Error> {
        if let Some(node) = self.construct(name, at)? {
            return Ok(node);
        }
        if let Some(node) = self.styled(name, at)? {
            return Ok(node);
        }
        if let Some(space) = self.spacing(name, at)? {
            return Ok(Node::Space(space));
        }
        if let Some(node) = self.delimiter_command(name, at)? {
            return Ok(node);
        }
        if let Some(c) = symbols::symbol(name) {
            return Ok(Node::Char(c));
        }
        if let Some((ch, limits)) = symbols::large_op(name) {
            return Ok(Node::Op {
                name: OpName::Symbol(ch),
                limits,
            });
        }
        if let Some((text, limits)) = symbols::named_op(name) {
            return Ok(Node::Op {
                name: OpName::Named(text.to_owned()),
                limits,
            });
        }
        Err(Error::Unsupported {
            command: format!("\\{name}"),
            at,
        })
    }

    /// Fractions, radicals, accents, braces, stacks, `\begin`, `\not`, `\operatorname`.
    fn construct(&mut self, name: &str, at: usize) -> Result<Option<Node>, Error> {
        let fraction = match name {
            "frac" => Some((true, None)),
            "dfrac" | "cfrac" => Some((true, Some(Style::Display))),
            "tfrac" => Some((true, Some(Style::Text))),
            "binom" => Some((false, None)),
            "dbinom" => Some((false, Some(Style::Display))),
            "tbinom" => Some((false, Some(Style::Text))),
            _ => None,
        };
        if let Some((bar, style)) = fraction {
            let num = self.arg(at, name)?;
            let den = self.arg(at, name)?;
            let (open, close) = if bar {
                (None, None)
            } else {
                (Some('('), Some(')'))
            };
            return Ok(Some(Node::Frac {
                num,
                den,
                bar,
                open,
                close,
                style,
            }));
        }
        if let Some(accent) = accent(name) {
            let base = self.arg(at, name)?;
            return Ok(Some(Node::Accent { accent, base }));
        }
        let node = match name {
            "sqrt" => self.sqrt(at)?,
            "overline" => Node::Overline(self.arg(at, name)?),
            "underline" => Node::Underline(self.arg(at, name)?),
            "overbrace" | "underbrace" => self.brace(name == "overbrace", at)?,
            "stackrel" | "overset" | "underset" => {
                let decoration = self.arg(at, name)?;
                let base = self.arg(at, name)?;
                let class = if name == "stackrel" {
                    Class::Rel
                } else {
                    single_class(&base)
                };
                let (over, under) = if name == "underset" {
                    (None, Some(decoration))
                } else {
                    (Some(decoration), None)
                };
                Node::Stack {
                    base,
                    over,
                    under,
                    class,
                }
            }
            "begin" => self.environment(at)?,
            "not" => self.not(at)?,
            "operatorname" => self.operatorname(at)?,
            "left" => self.fenced(at)?,
            "hline" => return Err(Error::syntax(at, "`\\hline` outside an array")),
            _ => return Ok(None),
        };
        Ok(Some(node))
    }

    /// Fonts, text, colour and class changes.
    fn styled(&mut self, name: &str, at: usize) -> Result<Option<Node>, Error> {
        let font = match name {
            "mathbb" | "Bbb" => Some(MathFont::Blackboard),
            "mathcal" => Some(MathFont::Calligraphic),
            "mathscr" => Some(MathFont::Script),
            "mathfrak" => Some(MathFont::Fraktur),
            "mathbf" => Some(MathFont::Bold),
            "mathit" => Some(MathFont::Italic),
            "mathrm" | "mathup" => Some(MathFont::Roman),
            "mathsf" => Some(MathFont::SansSerif),
            "mathtt" => Some(MathFont::Monospace),
            "boldsymbol" | "bm" | "pmb" => Some(MathFont::BoldSymbol),
            _ => None,
        };
        if let Some(font) = font {
            return Ok(Some(Node::Font {
                font,
                body: self.arg(at, name)?,
            }));
        }
        if let Some(font) = text_font(name) {
            return Ok(Some(Node::Text {
                font,
                body: self.text_arg(at, name)?,
            }));
        }
        let class = match name {
            "mathord" => Some(Class::Ord),
            "mathop" => Some(Class::Op),
            "mathbin" => Some(Class::Bin),
            "mathrel" => Some(Class::Rel),
            "mathopen" => Some(Class::Open),
            "mathclose" => Some(Class::Close),
            "mathpunct" => Some(Class::Punct),
            "mathinner" => Some(Class::Inner),
            _ => None,
        };
        if let Some(class) = class {
            return Ok(Some(Node::Class {
                class,
                body: self.arg(at, name)?,
                limits: (class == Class::Op).then_some(Limits::Display),
            }));
        }
        if name == "textcolor" {
            let color = self.color(at)?;
            return Ok(Some(Node::Color {
                color,
                body: self.arg(at, name)?,
            }));
        }
        Ok(None)
    }

    /// `\big(` and its sizes and classes.
    fn delimiter_command(&mut self, name: &str, at: usize) -> Result<Option<Node>, Error> {
        let sizes = [("Bigg", 4), ("bigg", 3), ("Big", 2), ("big", 1)];
        let Some(&(base, size)) = sizes.iter().find(|(base, _)| name.starts_with(base)) else {
            return Ok(None);
        };
        let class = match &name[base.len()..] {
            "" => Class::Ord,
            "l" => Class::Open,
            "r" => Class::Close,
            "m" => Class::Rel,
            _ => return Ok(None),
        };
        let delim = self.delimiter(at, name)?;
        Ok(Some(Node::Big { delim, size, class }))
    }

    fn sqrt(&mut self, at: usize) -> Result<Node, Error> {
        self.skip_spaces()?;
        let degree = if self.at_char('[')? {
            self.raw_next();
            let (degree, stop, stop_at) = self.list(true)?;
            match stop {
                Stop::Bracket => {
                    self.raw_next();
                }
                Stop::Eof => return Err(Error::syntax(at, "`\\sqrt[` without `]`")),
                other => return Err(misplaced(other, stop_at)),
            }
            Some(degree)
        } else {
            None
        };
        let body = self.arg(at, "sqrt")?;
        Ok(Node::Sqrt { degree, body })
    }

    fn brace(&mut self, over: bool, at: usize) -> Result<Node, Error> {
        let body = self.arg(at, if over { "overbrace" } else { "underbrace" })?;
        self.skip_spaces()?;
        let script = if over { Tok::Sup } else { Tok::Sub };
        let label = if self.peek()?.is_some_and(|t| t.tok == script) {
            let at = self.raw_next().map_or(self.end, |t| t.at);
            Some(self.script_arg(at)?)
        } else {
            None
        };
        Ok(Node::Brace { over, body, label })
    }

    fn not(&mut self, at: usize) -> Result<Node, Error> {
        self.skip_spaces()?;
        let target = match self.next()?.map(|t| t.tok) {
            Some(Tok::Char(c)) => Some(symbols::char_atom(c)),
            Some(Tok::Command(name)) => symbols::symbol(&name),
            _ => None,
        };
        target
            .and_then(|c| symbols::negated(c.ch).map(|ch| MathChar { ch, class: c.class }))
            .map(Node::Char)
            .ok_or_else(|| Error::Unsupported {
                command: "\\not".to_owned(),
                at,
            })
    }

    fn operatorname(&mut self, at: usize) -> Result<Node, Error> {
        let star = self.at_char('*')?;
        if star {
            self.raw_next();
        }
        let body = self.arg(at, "operatorname")?;
        let mut name = String::new();
        if !flatten_name(&body, &mut name) {
            return Err(Error::syntax(
                at,
                "`\\operatorname` takes a name of letters",
            ));
        }
        let limits = if star { Limits::Display } else { Limits::Never };
        Ok(Node::Op {
            name: OpName::Named(name),
            limits,
        })
    }

    /// `\left`, its `\middle`s and `\right`.
    fn fenced(&mut self, at: usize) -> Result<Node, Error> {
        let open = self.delimiter(at, "left")?;
        let mut body = Vec::new();
        loop {
            let (list, stop, stop_at) = self.list(false)?;
            body.extend(list);
            match stop {
                Stop::Middle => {
                    self.raw_next();
                    body.push(Node::Middle(self.delimiter(stop_at, "middle")?));
                }
                Stop::Right => {
                    self.raw_next();
                    let close = self.delimiter(stop_at, "right")?;
                    return Ok(Node::Fenced { open, body, close });
                }
                Stop::Eof => return Err(Error::syntax(at, "`\\left` without `\\right`")),
                other => return Err(misplaced(other, stop_at)),
            }
        }
    }

    /// The delimiter after `\left`, `\right`, `\middle` or `\big`: `None` for `.`.
    fn delimiter(&mut self, at: usize, command: &str) -> Result<Option<char>, Error> {
        self.skip_spaces()?;
        let token = self.next()?;
        let delim = match token.as_ref().map(|t| &t.tok) {
            Some(Tok::Char('.')) => return Ok(None),
            Some(Tok::Char('<')) => Some('⟨'),
            Some(Tok::Char('>')) => Some('⟩'),
            Some(&Tok::Char(c)) if symbols::is_delimiter_char(c) => Some(c),
            Some(Tok::Command(name)) => symbols::delimiter_command(name),
            _ => None,
        };
        delim.map(Some).ok_or_else(|| {
            Error::syntax(
                token.map_or(at, |t| t.at),
                format!("`\\{command}` needs a delimiter after it"),
            )
        })
    }

    /// Explicit spacing commands.
    fn spacing(&mut self, name: &str, at: usize) -> Result<Option<Space>, Error> {
        let space = match name {
            "," | "thinspace" => Space::Mu(3.0),
            ":" | ">" | "medspace" => Space::Mu(4.0),
            ";" | "thickspace" => Space::Mu(5.0),
            "!" | "negthinspace" => Space::Mu(-3.0),
            "negmedspace" => Space::Mu(-4.0),
            "negthickspace" => Space::Mu(-5.0),
            " " | "space" => Space::Em(1.0 / 3.0),
            "enspace" => Space::Em(0.5),
            "quad" => Space::Em(1.0),
            "qquad" => Space::Em(2.0),
            "hspace" | "hskip" | "kern" | "mkern" | "mskip" | "mspace" => {
                if name == "hspace" && self.at_char('*')? {
                    self.raw_next();
                }
                self.dimension(at, name)?
            }
            _ => return Ok(None),
        };
        Ok(Some(space))
    }

    /// A length: `{2em}` or, for `\kern` and friends, `3mu` written straight after.
    fn dimension(&mut self, at: usize, command: &str) -> Result<Space, Error> {
        self.raw_skip_spaces();
        let text = if self.raw_peek().is_some_and(|t| t.tok == Tok::Begin) {
            self.raw_word(at, command)?
        } else {
            let mut text = String::new();
            let mut letters = 0;
            while let Some(Token {
                tok: Tok::Char(c), ..
            }) = self.raw_peek()
            {
                let c = *c;
                if c.is_ascii_alphabetic() {
                    letters += 1;
                } else if letters > 0 || !(c.is_ascii_digit() || matches!(c, '.' | '-' | '+')) {
                    break;
                }
                text.push(c);
                self.raw_next();
                if letters == 2 {
                    break;
                }
            }
            text
        };
        parse_dimension(&text)
            .ok_or_else(|| Error::syntax(at, format!("`\\{command}` needs a length, not `{text}`")))
    }

    fn color(&mut self, at: usize) -> Result<String, Error> {
        self.raw_skip_spaces();
        let model = if self.raw_peek().is_some_and(|t| t.tok == Tok::Char('[')) {
            self.raw_next();
            let tokens = self.raw_until_bracket(at)?;
            let model: String = tokens
                .iter()
                .filter_map(|t| {
                    if let Tok::Char(c) = t.tok {
                        Some(c)
                    } else {
                        None
                    }
                })
                .collect();
            Some(model)
        } else {
            None
        };
        let value = self.colour_word(at)?;
        Ok(model.map_or_else(|| value.clone(), |model| format!("{model}:{value}")))
    }

    /// A colour's braced argument as written. The lexer reads `#` and a digit as a macro parameter; a colour
    /// is not in a definition here (a definition's parameters are substituted before the colour is read), so
    /// such a token is the `#` and the digit of a hex colour (`#1a2b3c`).
    fn colour_word(&mut self, at: usize) -> Result<String, Error> {
        let tokens = self.raw_arg(at, "color")?;
        let mut word = String::new();
        for token in tokens {
            match token.tok {
                Tok::Char(c) => word.push(c),
                Tok::Param(n) => {
                    word.push('#');
                    word.push(char::from(b'0' + n));
                }
                Tok::Space => {}
                _ => {
                    return Err(Error::syntax(token.at, "`\\color` takes plain characters"));
                }
            }
        }
        Ok(word)
    }

    // ---- Text.

    /// The argument of `\text` and friends: text, spaces kept, with `$…$` math.
    fn text_arg(&mut self, at: usize, command: &str) -> Result<Vec<TextPart>, Error> {
        self.skip_spaces()?;
        match self.peek()? {
            Some(Token {
                tok: Tok::Begin,
                at: open,
            }) => {
                self.raw_next();
                self.enter(open)?;
                let parts = self.text_rest(open)?;
                self.leave();
                Ok(parts)
            }
            Some(Token {
                tok: Tok::Char(c), ..
            }) => {
                self.raw_next();
                Ok(vec![TextPart::Str(c.to_string())])
            }
            _ => Err(missing_argument(at, command)),
        }
    }

    /// Text up to the `}` closing a `{` at `open`.
    fn text_rest(&mut self, open: usize) -> Result<Vec<TextPart>, Error> {
        let mut parts = Vec::new();
        let mut text = String::new();
        loop {
            let Some(token) = self.next()? else {
                return Err(Error::syntax(open, "unbalanced braces: `{` without `}`"));
            };
            match token.tok {
                Tok::End => break,
                Tok::Char(c) => text.push(c),
                Tok::Space | Tok::Tie => text.push(' '),
                Tok::Begin => {
                    flush(&mut text, &mut parts);
                    self.enter(token.at)?;
                    parts.extend(self.text_rest(token.at)?);
                    self.leave();
                }
                Tok::Shift => {
                    flush(&mut text, &mut parts);
                    let (math, stop, at) = self.list(false)?;
                    match stop {
                        Stop::Shift => {
                            self.raw_next();
                        }
                        Stop::Eof | Stop::Close => {
                            return Err(Error::syntax(token.at, "a `$` without its closing `$`"));
                        }
                        other => return Err(misplaced(other, at)),
                    }
                    parts.push(TextPart::Math(math));
                }
                Tok::Command(name) => self.text_command(&name, token.at, &mut text, &mut parts)?,
                Tok::Sup | Tok::Sub => {
                    return Err(Error::syntax(token.at, "a script in text (use `$…$`)"));
                }
                Tok::Align => return Err(misplaced(Stop::Align, token.at)),
                Tok::Param(_) => {
                    return Err(Error::syntax(
                        token.at,
                        "a macro parameter outside a definition",
                    ));
                }
            }
        }
        flush(&mut text, &mut parts);
        Ok(parts)
    }

    fn text_command(
        &mut self,
        name: &str,
        at: usize,
        text: &mut String,
        parts: &mut Vec<TextPart>,
    ) -> Result<(), Error> {
        if let Some(font) = text_font(name) {
            flush(text, parts);
            let inner = self.text_arg(at, name)?;
            parts.push(TextPart::Styled(font, inner));
            return Ok(());
        }
        let escaped = match name {
            "{" | "}" | "$" | "%" | "&" | "#" | "_" => name.chars().next(),
            "textbackslash" => Some('\\'),
            "textasciitilde" => Some('~'),
            "textasciicircum" => Some('^'),
            _ => None,
        };
        if let Some(c) = escaped {
            text.push(c);
            return Ok(());
        }
        if let Some(space) = self.spacing(name, at)? {
            flush(text, parts);
            parts.push(TextPart::Space(space));
            return Ok(());
        }
        Err(Error::Unsupported {
            command: format!("\\{name}"),
            at,
        })
    }

    // ---- Environments.

    fn environment(&mut self, at: usize) -> Result<Node, Error> {
        let name = self.raw_word(at, "begin")?;
        let (kind, open, close) = match name.as_str() {
            "matrix" => (ArrayKind::Matrix, None, None),
            "pmatrix" => (ArrayKind::Matrix, Some('('), Some(')')),
            "bmatrix" => (ArrayKind::Matrix, Some('['), Some(']')),
            "Bmatrix" => (ArrayKind::Matrix, Some('{'), Some('}')),
            "vmatrix" => (ArrayKind::Matrix, Some('|'), Some('|')),
            "Vmatrix" => (ArrayKind::Matrix, Some('‖'), Some('‖')),
            "smallmatrix" => (ArrayKind::SmallMatrix, None, None),
            "cases" => (ArrayKind::Cases, Some('{'), None),
            "rcases" => (ArrayKind::Cases, None, Some('}')),
            "aligned" | "align" | "align*" | "split" | "alignedat" => {
                (ArrayKind::Aligned, None, None)
            }
            "gathered" | "gather" | "gather*" | "equation" | "equation*" => {
                (ArrayKind::Gathered, None, None)
            }
            "array" => (ArrayKind::Array, None, None),
            _ => {
                return Err(Error::Unsupported {
                    command: format!("\\begin{{{name}}}"),
                    at,
                });
            }
        };
        if name == "alignedat" {
            // The column count is only a hint for TeX's spacing.
            self.raw_arg(at, "begin")?;
        }
        let columns = if kind == ArrayKind::Array {
            self.column_spec(at)?
        } else {
            Vec::new()
        };
        let (rows, hlines) = self.rows(&name, at)?;
        let mut array = Array {
            kind,
            columns,
            rows,
            hlines,
            open,
            close,
        };
        complete_columns(&mut array);
        Ok(Node::Array(array))
    }

    fn column_spec(&mut self, at: usize) -> Result<Vec<Column>, Error> {
        let tokens = self.raw_arg(at, "begin{array}")?;
        let mut columns = Vec::new();
        for token in tokens {
            columns.push(match token.tok {
                Tok::Char('l') => Column::Align(Align::Left),
                Tok::Char('c') => Column::Align(Align::Center),
                Tok::Char('r') => Column::Align(Align::Right),
                Tok::Char('|') => Column::Rule,
                Tok::Space => continue,
                _ => {
                    return Err(Error::syntax(
                        token.at,
                        "array columns other than `l`, `c`, `r` and `|` are not supported",
                    ));
                }
            });
        }
        Ok(columns)
    }

    /// The rows of an environment, up to its `\end`, with the positions of its `\hline`s.
    fn rows(&mut self, name: &str, at: usize) -> Result<(Vec<Row>, Vec<usize>), Error> {
        let mut rows = Vec::new();
        let mut hlines = Vec::new();
        let mut row = Vec::new();
        loop {
            if row.is_empty() {
                while self.at_command("hline")? {
                    self.raw_next();
                    hlines.push(rows.len());
                }
            }
            let (cell, stop, stop_at) = self.list(false)?;
            row.push(cell);
            match stop {
                Stop::Align => {
                    self.raw_next();
                }
                Stop::Row => {
                    self.raw_next();
                    self.skip_row_space()?;
                    rows.push(std::mem::take(&mut row));
                }
                Stop::EndEnv => {
                    self.raw_next();
                    let end = self.raw_word(stop_at, "end")?;
                    if end != name {
                        return Err(Error::syntax(
                            stop_at,
                            format!("`\\begin{{{name}}}` ended by `\\end{{{end}}}`"),
                        ));
                    }
                    if !(row.len() == 1 && row[0].is_empty()) {
                        rows.push(row);
                    }
                    return Ok((rows, hlines));
                }
                Stop::Eof => {
                    return Err(Error::syntax(
                        at,
                        format!("`\\begin{{{name}}}` without `\\end{{{name}}}`"),
                    ));
                }
                other => return Err(misplaced(other, stop_at)),
            }
        }
    }

    /// The optional `[length]` or `*` straight after `\\`.
    fn skip_row_space(&mut self) -> Result<(), Error> {
        match self.raw_peek().map(|t| (&t.tok, t.at)) {
            Some((Tok::Char('['), at)) => {
                self.raw_next();
                self.raw_until_bracket(at)?;
            }
            Some((Tok::Char('*'), _)) => {
                self.raw_next();
            }
            _ => {}
        }
        Ok(())
    }

    // ---- Definitions.

    /// The `\name` being defined: `{\name}` or `\name`.
    fn defined_name(&mut self, at: usize, command: &str) -> Result<String, Error> {
        self.raw_skip_spaces();
        let tokens = self.raw_arg(at, command)?;
        let mut names = tokens.into_iter().filter(|t| t.tok != Tok::Space);
        match (names.next().map(|t| t.tok), names.next()) {
            (Some(Tok::Command(name)), None) => Ok(name),
            _ => Err(Error::syntax(
                at,
                format!("`\\{command}` needs the `\\name` it defines"),
            )),
        }
    }

    fn newcommand(&mut self, command: &str, at: usize) -> Result<(), Error> {
        self.raw_skip_spaces();
        if self.raw_peek().is_some_and(|t| t.tok == Tok::Char('*')) {
            self.raw_next();
        }
        let name = self.defined_name(at, command)?;
        let mut params = 0;
        self.raw_skip_spaces();
        if self.raw_peek().is_some_and(|t| t.tok == Tok::Char('[')) {
            self.raw_next();
            let count: String = self
                .raw_until_bracket(at)?
                .iter()
                .filter_map(|t| {
                    if let Tok::Char(c) = t.tok {
                        Some(c)
                    } else {
                        None
                    }
                })
                .collect();
            params = match count.trim().parse::<u8>() {
                Ok(n @ 0..=9) => n,
                _ => {
                    return Err(Error::syntax(
                        at,
                        format!("`\\{command}` takes 0 to 9 arguments, not `{count}`"),
                    ));
                }
            };
        }
        self.raw_skip_spaces();
        let default = if self.raw_peek().is_some_and(|t| t.tok == Tok::Char('[')) {
            self.raw_next();
            Some(self.raw_until_bracket(at)?)
        } else {
            None
        };
        if default.is_some() && params == 0 {
            return Err(Error::syntax(
                at,
                "an optional argument's default for a macro without arguments",
            ));
        }
        let body = self.raw_arg(at, command)?;
        check_params(&body, params, at)?;
        if command == "providecommand" && self.macros.contains(&name) {
            return Ok(());
        }
        self.macros.insert(
            name,
            Macro {
                params,
                default,
                body,
            },
        );
        Ok(())
    }

    fn declare_operator(&mut self, at: usize) -> Result<(), Error> {
        self.raw_skip_spaces();
        let star = self.raw_peek().is_some_and(|t| t.tok == Tok::Char('*'));
        if star {
            self.raw_next();
        }
        let name = self.defined_name(at, "DeclareMathOperator")?;
        let text = self.raw_arg(at, "DeclareMathOperator")?;
        let token = |tok| Token { tok, at };
        let mut body = vec![token(Tok::Command("operatorname".to_owned()))];
        if star {
            body.push(token(Tok::Char('*')));
        }
        body.push(token(Tok::Begin));
        body.extend(text);
        body.push(token(Tok::End));
        self.macros.insert(
            name,
            Macro {
                params: 0,
                default: None,
                body,
            },
        );
        Ok(())
    }

    fn def(&mut self, at: usize) -> Result<(), Error> {
        self.raw_skip_spaces();
        let Some(Token {
            tok: Tok::Command(name),
            ..
        }) = self.raw_next()
        else {
            return Err(Error::syntax(at, "`\\def` needs the `\\name` it defines"));
        };
        let mut params = 0;
        loop {
            match self.raw_peek().map(|t| &t.tok) {
                Some(Tok::Param(n)) if *n == params + 1 => {
                    params += 1;
                    self.raw_next();
                }
                Some(Tok::Begin) => break,
                _ => {
                    // Delimited parameters (`\def\a#1.{…}`) and the like.
                    return Err(Error::Unsupported {
                        command: "\\def".to_owned(),
                        at,
                    });
                }
            }
        }
        let body = self.raw_arg(at, "def")?;
        check_params(&body, params, at)?;
        self.macros.insert(
            name,
            Macro {
                params,
                default: None,
                body,
            },
        );
        Ok(())
    }
}

/// What a token ends, if it ends a list.
fn stop_of(tok: &Tok, bracket: bool) -> Option<Stop> {
    match tok {
        Tok::End => Some(Stop::Close),
        Tok::Align => Some(Stop::Align),
        Tok::Shift => Some(Stop::Shift),
        Tok::Char(']') if bracket => Some(Stop::Bracket),
        Tok::Command(name) => match name.as_str() {
            "\\" | "cr" => Some(Stop::Row),
            "end" => Some(Stop::EndEnv),
            "right" => Some(Stop::Right),
            "middle" => Some(Stop::Middle),
            _ => None,
        },
        _ => None,
    }
}

/// The error for a list ended where that end does not belong.
fn misplaced(stop: Stop, at: usize) -> Error {
    let message = match stop {
        Stop::Eof => "unexpected end of the formula",
        Stop::Close => "unbalanced braces: `}` without `{`",
        Stop::Align => "`&` outside an array",
        Stop::Row => "`\\\\` outside an array",
        Stop::EndEnv => "`\\end` without `\\begin`",
        Stop::Right => "`\\right` without `\\left`",
        Stop::Middle => "`\\middle` outside `\\left`…`\\right`",
        Stop::Bracket => "unexpected `]`",
        Stop::Shift => "a `$` inside math",
    };
    Error::syntax(at, message)
}

fn missing_argument(at: usize, command: &str) -> Error {
    Error::syntax(at, format!("missing argument for `\\{command}`"))
}

/// Puts `script` on the atom before it, starting a script node with an empty base when there is none.
fn attach(list: &mut Vec<Node>, sup: bool, script: Vec<Node>, at: usize) -> Result<(), Error> {
    let (base, mut lower, mut upper) = match list.pop() {
        Some(Node::Scripts { base, sub, sup }) => (base, sub, sup),
        Some(node) if node.class().is_some() => (Box::new(node), None, None),
        Some(node) => {
            list.push(node);
            (Box::new(Node::Group(Vec::new())), None, None)
        }
        None => (Box::new(Node::Group(Vec::new())), None, None),
    };
    let slot = if sup { &mut upper } else { &mut lower };
    if slot.is_some() {
        let what = if sup {
            "double superscript"
        } else {
            "double subscript"
        };
        return Err(Error::syntax(at, what));
    }
    *slot = Some(script);
    list.push(Node::Scripts {
        base,
        sub: lower,
        sup: upper,
    });
    Ok(())
}

fn accent(name: &str) -> Option<Accent> {
    Some(match name {
        "hat" => Accent::Hat,
        "check" => Accent::Check,
        "tilde" => Accent::Tilde,
        "acute" => Accent::Acute,
        "grave" => Accent::Grave,
        "dot" => Accent::Dot,
        "ddot" => Accent::Ddot,
        "dddot" => Accent::Dddot,
        "breve" => Accent::Breve,
        "bar" => Accent::Bar,
        "vec" => Accent::Vec,
        "mathring" => Accent::Ring,
        "widehat" => Accent::WideHat,
        "widetilde" => Accent::WideTilde,
        "widecheck" => Accent::WideCheck,
        "overrightarrow" => Accent::OverRightArrow,
        "overleftarrow" => Accent::OverLeftArrow,
        "overleftrightarrow" => Accent::OverLeftRightArrow,
        _ => return None,
    })
}

fn text_font(name: &str) -> Option<TextFont> {
    Some(match name {
        "text" | "textrm" | "textnormal" | "textup" | "mbox" | "hbox" => TextFont::Normal,
        "textbf" => TextFont::Bold,
        "textit" | "emph" => TextFont::Italic,
        "textsf" => TextFont::SansSerif,
        "texttt" => TextFont::Monospace,
        _ => return None,
    })
}

/// The class of a list that is one atom, ordinary otherwise.
fn single_class(list: &[Node]) -> Class {
    match list {
        [node] => node.class().unwrap_or(Class::Ord),
        _ => Class::Ord,
    }
}

/// Fills in the column specification so it covers every cell: the kind's defaults, and for `array` centred
/// columns for cells beyond the specification.
fn complete_columns(array: &mut Array) {
    let cells = array.rows.iter().map(Vec::len).max().unwrap_or(0);
    let mut have = array
        .columns
        .iter()
        .filter(|c| matches!(c, Column::Align(_)))
        .count();
    while have < cells {
        let align = match array.kind {
            ArrayKind::Aligned if have % 2 == 0 => Align::Right,
            ArrayKind::Cases | ArrayKind::Aligned => Align::Left,
            ArrayKind::Matrix | ArrayKind::SmallMatrix | ArrayKind::Gathered | ArrayKind::Array => {
                Align::Center
            }
        };
        array.columns.push(Column::Align(align));
        have += 1;
    }
}

/// The name of an operator from its parsed argument: letters, digits and spacing.
fn flatten_name(list: &[Node], name: &mut String) -> bool {
    list.iter().all(|node| match node {
        Node::Char(c) => {
            name.push(match c.ch {
                '−' => '-',
                ch => ch,
            });
            true
        }
        Node::Space(space) => {
            if space.em() > 0.0 {
                name.push(' ');
            }
            true
        }
        Node::Group(body) | Node::Font { body, .. } | Node::Class { body, .. } => {
            flatten_name(body, name)
        }
        Node::Text { body, .. } => flatten_text(body, name),
        _ => false,
    })
}

fn flatten_text(parts: &[TextPart], name: &mut String) -> bool {
    parts.iter().all(|part| match part {
        TextPart::Str(s) => {
            name.push_str(s);
            true
        }
        TextPart::Styled(_, parts) => flatten_text(parts, name),
        TextPart::Space(_) => {
            name.push(' ');
            true
        }
        TextPart::Math(list) => flatten_name(list, name),
    })
}

fn flush(text: &mut String, parts: &mut Vec<TextPart>) {
    if !text.is_empty() {
        parts.push(TextPart::Str(std::mem::take(text)));
    }
}

/// A TeX length, in ems (10 pt to the em) or mu.
fn parse_dimension(text: &str) -> Option<Space> {
    let text = text.trim();
    let split = text.find(|c: char| c.is_ascii_alphabetic())?;
    let (number, unit) = text.split_at(split);
    let number = number.trim();
    let value: f32 = if number.is_empty() || number == "-" || number == "+" {
        let sign = if number == "-" { -1.0 } else { 1.0 };
        sign * 1.0
    } else {
        number.parse().ok()?
    };
    let points = match unit.trim() {
        "mu" => return Some(Space::Mu(value)),
        "em" => return Some(Space::Em(value)),
        "ex" => return Some(Space::Em(value * 0.43)),
        "pt" => value,
        "bp" => value * 1.00375,
        "pc" => value * 12.0,
        "px" => value * 0.75,
        "in" => value * 72.27,
        "cm" => value * 28.45,
        "mm" => value * 2.845,
        _ => return None,
    };
    value.is_finite().then_some(Space::Em(points / 10.0))
}

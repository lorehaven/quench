//! Splitting template text into tags, and parsing tags into a tree.

use crate::error::{Error, Result};
use quench_serde::Value;

// ---------------------------------------------------------------- tree

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    FloorDiv,
    Rem,
    Pow,
    Concat,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    In,
    NotIn,
}

#[derive(Debug, Clone)]
pub enum Expr {
    Const(Value),
    Var(String),
    List(Vec<Self>),
    Dict(Vec<(Self, Self)>),
    Attr(Box<Self>, String),
    Index(Box<Self>, Box<Self>),
    Slice(Box<Self>, [Option<Box<Self>>; 3]),
    Call(Box<Self>, Vec<Self>, Vec<(String, Self)>),
    Filter(Box<Self>, String, Vec<Self>, Vec<(String, Self)>),
    Test(Box<Self>, String, Vec<Self>, bool),
    Neg(Box<Self>),
    Not(Box<Self>),
    Binary(BinOp, Box<Self>, Box<Self>),
    And(Box<Self>, Box<Self>),
    Or(Box<Self>, Box<Self>),
    Cond(Box<Self>, Box<Self>, Option<Box<Self>>),
}

#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub default: Option<Expr>,
}

#[derive(Debug)]
pub struct MacroDef {
    pub name: String,
    pub params: Vec<Param>,
    pub body: Vec<Stmt>,
}

#[derive(Debug)]
pub enum Stmt {
    Text(String),
    Output(Expr, usize),
    If(Vec<(Expr, Vec<Self>)>, Vec<Self>),
    For {
        targets: Vec<String>,
        iter: Expr,
        filter: Option<Expr>,
        body: Vec<Self>,
        otherwise: Vec<Self>,
        line: usize,
    },
    Set(Vec<String>, Expr, usize),
    SetBlock(String, Vec<Self>),
    Macro(std::sync::Arc<MacroDef>),
    Import(Expr, String, usize),
    FromImport(Expr, Vec<(String, String)>, usize),
    Include(Expr, bool, usize),
    With(Vec<(String, Expr)>, Vec<Self>),
}

// ---------------------------------------------------------------- splitting

enum Piece {
    Text(String),
    Output(String, usize),
    Block(String, usize),
}

fn line_of(source: &str, at: usize) -> usize {
    1 + source[..at].bytes().filter(|&b| b == b'\n').count()
}

/// The end of a `{{ }}` / `{% %}` tag (the index of `}}`/`%}`), skipping string literals.
fn tag_end(source: &str, from: usize, close: &str) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut i = from;
    while i < bytes.len() {
        match bytes[i] {
            quote @ (b'"' | b'\'') => {
                i += 1;
                while i < bytes.len() && bytes[i] != quote {
                    if bytes[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
            }
            _ if source[i..].starts_with(close) => return Some(i),
            _ => {}
        }
        i += 1;
    }
    None
}

#[allow(clippy::too_many_lines)]
fn split(source: &str) -> Result<Vec<Piece>> {
    let mut pieces = Vec::new();
    let mut position = 0;
    let mut trim_next = false;
    while position < source.len() {
        let rest = &source[position..];
        let next = ["{{", "{%", "{#"]
            .iter()
            .filter_map(|open| rest.find(open).map(|i| (i, *open)))
            .min_by_key(|&(i, _)| i);
        let Some((offset, open)) = next else {
            let text = if trim_next { rest.trim_start() } else { rest };
            if !text.is_empty() {
                pieces.push(Piece::Text(text.to_string()));
            }
            break;
        };
        let start = position + offset;
        let line = line_of(source, start);
        let body_start = start + 2;
        let trim_before = source[body_start..].starts_with('-');
        let mut text = &source[position..start];
        if trim_next {
            text = text.trim_start();
        }
        if trim_before {
            text = text.trim_end();
        }
        if !text.is_empty() {
            pieces.push(Piece::Text(text.to_string()));
        }
        let content_start =
            body_start + usize::from(trim_before || source[body_start..].starts_with('+'));

        let close = match open {
            "{{" => "}}",
            "{%" => "%}",
            _ => "#}",
        };
        let end = if open == "{#" {
            source[content_start..]
                .find(close)
                .map(|i| content_start + i)
        } else {
            tag_end(source, content_start, close)
        }
        .ok_or_else(|| Error::syntax(format!("unclosed `{open}`"), line))?;
        let trim_after = end > content_start && source.as_bytes()[end - 1] == b'-';
        let content = &source[content_start..end - usize::from(trim_after)];
        position = end + 2;
        trim_next = trim_after;

        match open {
            "{{" => pieces.push(Piece::Output(content.to_string(), line)),
            "{%" => {
                if content.trim() == "raw" {
                    // Everything up to `endraw` is text.
                    let mut search = position;
                    let (raw_end, after, trim_end_tag, trim_after_tag) = loop {
                        let found = source[search..]
                            .find("{%")
                            .map(|i| search + i)
                            .ok_or_else(|| Error::syntax("unclosed `raw`", line))?;
                        let tag_close = source[found..]
                            .find("%}")
                            .map(|i| found + i)
                            .ok_or_else(|| Error::syntax("unclosed `raw`", line))?;
                        let inner = &source[found + 2..tag_close];
                        let left = inner.starts_with('-');
                        let right = inner.ends_with('-');
                        let word = inner.trim_matches('-').trim();
                        if word == "endraw" {
                            break (found, tag_close + 2, left, right);
                        }
                        search = tag_close + 2;
                    };
                    let mut raw = &source[position..raw_end];
                    if trim_next {
                        raw = raw.trim_start();
                    }
                    if trim_end_tag {
                        raw = raw.trim_end();
                    }
                    if !raw.is_empty() {
                        pieces.push(Piece::Text(raw.to_string()));
                    }
                    position = after;
                    trim_next = trim_after_tag;
                } else {
                    pieces.push(Piece::Block(content.to_string(), line));
                }
            }
            _ => {}
        }
    }
    // Like Jinja, a single trailing newline of the template is not output.
    if let Some(Piece::Text(text)) = pieces.last_mut() {
        if text.ends_with("\r\n") {
            text.truncate(text.len() - 2);
        } else if text.ends_with('\n') {
            text.pop();
        }
    }
    Ok(pieces)
}

// ---------------------------------------------------------------- tokens

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Ident(String),
    Int(i64),
    Float(f64),
    Str(String),
    Op(&'static str),
}

fn tokenize(content: &str, line: usize) -> Result<Vec<Tok>> {
    const OPS: [&str; 24] = [
        "==", "!=", "<=", ">=", "//", "**", "+", "-", "*", "/", "%", "~", "<", ">", "=", "(", ")",
        "[", "]", "{", "}", ",", ":", ".",
    ];
    let chars: Vec<char> = content.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
        } else if c.is_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            tokens.push(Tok::Ident(chars[start..i].iter().collect()));
        } else if c.is_ascii_digit() {
            let start = i;
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
            }
            let mut float = false;
            if i + 1 < chars.len() && chars[i] == '.' && chars[i + 1].is_ascii_digit() {
                float = true;
                i += 1;
                while i < chars.len() && chars[i].is_ascii_digit() {
                    i += 1;
                }
            }
            if i < chars.len() && (chars[i] == 'e' || chars[i] == 'E') {
                let mut j = i + 1;
                if j < chars.len() && (chars[j] == '+' || chars[j] == '-') {
                    j += 1;
                }
                if j < chars.len() && chars[j].is_ascii_digit() {
                    float = true;
                    while j < chars.len() && chars[j].is_ascii_digit() {
                        j += 1;
                    }
                    i = j;
                }
            }
            let text: String = chars[start..i].iter().collect();
            let bad = || Error::syntax(format!("bad number `{text}`"), line);
            tokens.push(if float {
                Tok::Float(text.parse().map_err(|_| bad())?)
            } else {
                Tok::Int(text.parse().map_err(|_| bad())?)
            });
        } else if c == '"' || c == '\'' {
            i += 1;
            let mut text = String::new();
            loop {
                let Some(&ch) = chars.get(i) else {
                    return Err(Error::syntax("unterminated string", line));
                };
                i += 1;
                if ch == c {
                    break;
                }
                if ch == '\\' {
                    let escaped = *chars
                        .get(i)
                        .ok_or_else(|| Error::syntax("unterminated string", line))?;
                    i += 1;
                    text.push(match escaped {
                        'n' => '\n',
                        't' => '\t',
                        'r' => '\r',
                        other => other,
                    });
                } else {
                    text.push(ch);
                }
            }
            tokens.push(Tok::Str(text));
        } else if c == '|' {
            tokens.push(Tok::Op("|"));
            i += 1;
        } else {
            let rest: String = chars[i..chars.len().min(i + 2)].iter().collect();
            let op = OPS
                .iter()
                .find(|op| rest.starts_with(**op))
                .ok_or_else(|| Error::syntax(format!("unexpected `{c}`"), line))?;
            tokens.push(Tok::Op(op));
            i += op.len();
        }
    }
    Ok(tokens)
}

// ---------------------------------------------------------------- expressions

struct Tokens {
    tokens: Vec<Tok>,
    at: usize,
    line: usize,
}

impl Tokens {
    fn peek(&self) -> Option<&Tok> {
        self.tokens.get(self.at)
    }

    fn peek_at(&self, offset: usize) -> Option<&Tok> {
        self.tokens.get(self.at + offset)
    }

    fn next(&mut self) -> Option<Tok> {
        let token = self.tokens.get(self.at).cloned();
        self.at += 1;
        token
    }

    fn is_op(&self, op: &str) -> bool {
        matches!(self.peek(), Some(Tok::Op(o)) if *o == op)
    }

    fn is_word(&self, word: &str) -> bool {
        matches!(self.peek(), Some(Tok::Ident(w)) if w == word)
    }

    fn eat_op(&mut self, op: &str) -> bool {
        let found = self.is_op(op);
        if found {
            self.at += 1;
        }
        found
    }

    fn eat_word(&mut self, word: &str) -> bool {
        let found = self.is_word(word);
        if found {
            self.at += 1;
        }
        found
    }

    fn expect_op(&mut self, op: &str) -> Result<()> {
        if self.eat_op(op) {
            Ok(())
        } else {
            Err(self.error(format!("expected `{op}`")))
        }
    }

    fn ident(&mut self) -> Result<String> {
        match self.next() {
            Some(Tok::Ident(name)) => Ok(name),
            other => Err(self.error(format!("expected a name, found {other:?}"))),
        }
    }

    fn error(&self, message: impl Into<String>) -> Error {
        Error::syntax(message, self.line)
    }

    const fn done(&self) -> bool {
        self.at >= self.tokens.len()
    }

    fn finish(&self) -> Result<()> {
        if self.done() {
            Ok(())
        } else {
            Err(self.error(format!("unexpected {:?}", self.tokens[self.at])))
        }
    }
}

fn bx(expr: Expr) -> Box<Expr> {
    Box::new(expr)
}

fn parse_expr(t: &mut Tokens) -> Result<Expr> {
    let value = parse_or(t)?;
    if t.eat_word("if") {
        let test = parse_or(t)?;
        let otherwise = if t.eat_word("else") {
            Some(bx(parse_expr(t)?))
        } else {
            None
        };
        return Ok(Expr::Cond(bx(test), bx(value), otherwise));
    }
    Ok(value)
}

fn parse_or(t: &mut Tokens) -> Result<Expr> {
    let mut left = parse_and(t)?;
    while t.eat_word("or") {
        left = Expr::Or(bx(left), bx(parse_and(t)?));
    }
    Ok(left)
}

fn parse_and(t: &mut Tokens) -> Result<Expr> {
    let mut left = parse_not(t)?;
    while t.eat_word("and") {
        left = Expr::And(bx(left), bx(parse_not(t)?));
    }
    Ok(left)
}

fn parse_not(t: &mut Tokens) -> Result<Expr> {
    if t.eat_word("not") {
        return Ok(Expr::Not(bx(parse_not(t)?)));
    }
    parse_compare(t)
}

fn parse_compare(t: &mut Tokens) -> Result<Expr> {
    let mut left = parse_sum(t)?;
    loop {
        let op = if t.eat_op("==") {
            BinOp::Eq
        } else if t.eat_op("!=") {
            BinOp::Ne
        } else if t.eat_op("<=") {
            BinOp::Le
        } else if t.eat_op(">=") {
            BinOp::Ge
        } else if t.eat_op("<") {
            BinOp::Lt
        } else if t.eat_op(">") {
            BinOp::Gt
        } else if t.eat_word("in") {
            BinOp::In
        } else if t.is_word("not") && matches!(t.peek_at(1), Some(Tok::Ident(w)) if w == "in") {
            t.at += 2;
            BinOp::NotIn
        } else {
            return Ok(left);
        };
        left = Expr::Binary(op, bx(left), bx(parse_sum(t)?));
    }
}

fn parse_sum(t: &mut Tokens) -> Result<Expr> {
    let mut left = parse_concat(t)?;
    loop {
        let op = if t.eat_op("+") {
            BinOp::Add
        } else if t.eat_op("-") {
            BinOp::Sub
        } else {
            return Ok(left);
        };
        left = Expr::Binary(op, bx(left), bx(parse_concat(t)?));
    }
}

fn parse_concat(t: &mut Tokens) -> Result<Expr> {
    let mut left = parse_product(t)?;
    while t.eat_op("~") {
        left = Expr::Binary(BinOp::Concat, bx(left), bx(parse_product(t)?));
    }
    Ok(left)
}

fn parse_product(t: &mut Tokens) -> Result<Expr> {
    let mut left = parse_power(t)?;
    loop {
        let op = if t.eat_op("*") {
            BinOp::Mul
        } else if t.eat_op("//") {
            BinOp::FloorDiv
        } else if t.eat_op("/") {
            BinOp::Div
        } else if t.eat_op("%") {
            BinOp::Rem
        } else {
            return Ok(left);
        };
        left = Expr::Binary(op, bx(left), bx(parse_power(t)?));
    }
}

fn parse_power(t: &mut Tokens) -> Result<Expr> {
    let base = parse_unary(t)?;
    if t.eat_op("**") {
        return Ok(Expr::Binary(BinOp::Pow, bx(base), bx(parse_power(t)?)));
    }
    Ok(base)
}

fn parse_unary(t: &mut Tokens) -> Result<Expr> {
    if t.eat_op("-") {
        return Ok(Expr::Neg(bx(parse_unary(t)?)));
    }
    if t.eat_op("+") {
        return parse_unary(t);
    }
    parse_postfix(t)
}

type Arguments = (Vec<Expr>, Vec<(String, Expr)>);

fn parse_arguments(t: &mut Tokens) -> Result<Arguments> {
    let (mut args, mut kwargs) = (Vec::new(), Vec::new());
    while !t.is_op(")") {
        if let (Some(Tok::Ident(name)), Some(Tok::Op("="))) = (t.peek(), t.peek_at(1)) {
            let name = name.clone();
            t.at += 2;
            kwargs.push((name, parse_expr(t)?));
        } else {
            if !kwargs.is_empty() {
                return Err(t.error("positional argument after a keyword argument"));
            }
            args.push(parse_expr(t)?);
        }
        if !t.eat_op(",") {
            break;
        }
    }
    t.expect_op(")")?;
    Ok((args, kwargs))
}

fn parse_postfix(t: &mut Tokens) -> Result<Expr> {
    let mut expr = parse_primary(t)?;
    loop {
        if t.eat_op(".") {
            match t.next() {
                Some(Tok::Ident(name)) => expr = Expr::Attr(bx(expr), name),
                Some(Tok::Int(n)) => expr = Expr::Index(bx(expr), bx(Expr::Const(Value::from(n)))),
                other => {
                    return Err(t.error(format!("expected an attribute name, found {other:?}")));
                }
            }
        } else if t.eat_op("[") {
            expr = parse_subscript(t, expr)?;
        } else if t.eat_op("(") {
            let (args, kwargs) = parse_arguments(t)?;
            expr = Expr::Call(bx(expr), args, kwargs);
        } else if t.eat_op("|") {
            let name = t.ident()?;
            let (args, kwargs) = if t.eat_op("(") {
                parse_arguments(t)?
            } else {
                (Vec::new(), Vec::new())
            };
            expr = Expr::Filter(bx(expr), name, args, kwargs);
        } else if t.is_word("is") {
            t.at += 1;
            let negate = t.eat_word("not");
            let name = t.ident()?;
            let args = if t.eat_op("(") {
                parse_arguments(t)?.0
            } else if matches!(t.peek(), Some(Tok::Int(_) | Tok::Float(_) | Tok::Str(_))) {
                vec![parse_primary(t)?]
            } else {
                Vec::new()
            };
            expr = Expr::Test(bx(expr), name, args, negate);
        } else {
            return Ok(expr);
        }
    }
}

fn parse_subscript(t: &mut Tokens, base: Expr) -> Result<Expr> {
    let mut parts: [Option<Box<Expr>>; 3] = [None, None, None];
    let mut slot = 0;
    let mut sliced = false;
    loop {
        if t.is_op(":") {
            t.at += 1;
            sliced = true;
            slot += 1;
            if slot > 2 {
                return Err(t.error("too many `:` in a slice"));
            }
        } else if t.eat_op("]") {
            break;
        } else {
            parts[slot] = Some(bx(parse_expr(t)?));
            if !t.is_op(":") && !t.is_op("]") {
                return Err(t.error("expected `]`"));
            }
        }
    }
    if sliced {
        Ok(Expr::Slice(bx(base), parts))
    } else {
        let index = parts[0].take().ok_or_else(|| t.error("empty subscript"))?;
        Ok(Expr::Index(bx(base), index))
    }
}

fn parse_primary(t: &mut Tokens) -> Result<Expr> {
    match t.next() {
        Some(Tok::Int(n)) => Ok(Expr::Const(Value::from(n))),
        Some(Tok::Float(f)) => Ok(Expr::Const(Value::Float(f))),
        Some(Tok::Str(mut s)) => {
            // Adjacent string literals concatenate, as in Python.
            while let Some(Tok::Str(more)) = t.peek() {
                s.push_str(more);
                t.at += 1;
            }
            Ok(Expr::Const(Value::String(s)))
        }
        Some(Tok::Ident(name)) => Ok(match name.as_str() {
            "true" | "True" => Expr::Const(Value::Bool(true)),
            "false" | "False" => Expr::Const(Value::Bool(false)),
            "none" | "None" => Expr::Const(Value::Null),
            _ => Expr::Var(name),
        }),
        Some(Tok::Op("(")) => {
            let first = parse_expr(t)?;
            if t.is_op(",") {
                let mut items = vec![first];
                while t.eat_op(",") {
                    if t.is_op(")") {
                        break;
                    }
                    items.push(parse_expr(t)?);
                }
                t.expect_op(")")?;
                return Ok(Expr::List(items));
            }
            t.expect_op(")")?;
            Ok(first)
        }
        Some(Tok::Op("[")) => {
            let mut items = Vec::new();
            while !t.is_op("]") {
                items.push(parse_expr(t)?);
                if !t.eat_op(",") {
                    break;
                }
            }
            t.expect_op("]")?;
            Ok(Expr::List(items))
        }
        Some(Tok::Op("{")) => {
            let mut pairs = Vec::new();
            while !t.is_op("}") {
                let key = parse_expr(t)?;
                t.expect_op(":")?;
                pairs.push((key, parse_expr(t)?));
                if !t.eat_op(",") {
                    break;
                }
            }
            t.expect_op("}")?;
            Ok(Expr::Dict(pairs))
        }
        other => Err(t.error(format!("unexpected {other:?} in an expression"))),
    }
}

fn expression(content: &str, line: usize) -> Result<Expr> {
    let mut t = Tokens {
        tokens: tokenize(content, line)?,
        at: 0,
        line,
    };
    let expr = parse_expr(&mut t)?;
    t.finish()?;
    Ok(expr)
}

// ---------------------------------------------------------------- statements

struct Parser {
    pieces: std::vec::IntoIter<Piece>,
}

/// Where a nested body ended: the keyword that closed it and the rest of that tag.
struct Closer {
    word: String,
    tokens: Tokens,
}

impl Parser {
    /// Parses statements until a block whose keyword is one of `ends`; `None` at the end of the template.
    fn body(&mut self, ends: &[&str]) -> Result<(Vec<Stmt>, Option<Closer>)> {
        let mut out = Vec::new();
        while let Some(piece) = self.pieces.next() {
            match piece {
                Piece::Text(text) => out.push(Stmt::Text(text)),
                Piece::Output(content, line) => {
                    out.push(Stmt::Output(expression(&content, line)?, line));
                }
                Piece::Block(content, line) => {
                    let mut t = Tokens {
                        tokens: tokenize(&content, line)?,
                        at: 0,
                        line,
                    };
                    let word = t.ident()?;
                    if ends.contains(&word.as_str()) {
                        return Ok((out, Some(Closer { word, tokens: t })));
                    }
                    out.push(self.statement(&word, t)?);
                }
            }
        }
        if ends.is_empty() {
            Ok((out, None))
        } else {
            // The last keyword is the one that really closes the block (`endif`, not `elif`).
            let closer = ends.last().copied().unwrap_or_default();
            Err(Error::syntax(format!("missing `{closer}`"), 0))
        }
    }

    fn statement(&mut self, word: &str, mut t: Tokens) -> Result<Stmt> {
        let line = t.line;
        match word {
            "if" => self.if_statement(t),
            "for" => {
                let targets = targets(&mut t, "in")?;
                if !t.eat_word("in") {
                    return Err(t.error("expected `in`"));
                }
                let iter = parse_or(&mut t)?;
                let filter = if t.eat_word("if") {
                    Some(parse_or(&mut t)?)
                } else {
                    None
                };
                t.finish()?;
                let (body, closer) = self.body(&["else", "endfor"])?;
                let mut otherwise = Vec::new();
                if closer.is_some_and(|c| c.word == "else") {
                    otherwise = self.body(&["endfor"])?.0;
                }
                Ok(Stmt::For {
                    targets,
                    iter,
                    filter,
                    body,
                    otherwise,
                    line,
                })
            }
            "set" => {
                let names = targets(&mut t, "=")?;
                if t.eat_op("=") {
                    let value = parse_expr(&mut t)?;
                    t.finish()?;
                    Ok(Stmt::Set(names, value, line))
                } else {
                    t.finish()?;
                    let name = names
                        .into_iter()
                        .next()
                        .ok_or_else(|| t.error("`set` needs a name"))?;
                    let (body, _) = self.body(&["endset"])?;
                    Ok(Stmt::SetBlock(name, body))
                }
            }
            "macro" => {
                let name = t.ident()?;
                t.expect_op("(")?;
                let mut params = Vec::new();
                while !t.is_op(")") {
                    let name = t.ident()?;
                    let default = if t.eat_op("=") {
                        Some(parse_expr(&mut t)?)
                    } else {
                        None
                    };
                    params.push(Param { name, default });
                    if !t.eat_op(",") {
                        break;
                    }
                }
                t.expect_op(")")?;
                t.finish()?;
                let (body, _) = self.body(&["endmacro"])?;
                Ok(Stmt::Macro(std::sync::Arc::new(MacroDef {
                    name,
                    params,
                    body,
                })))
            }
            "import" => {
                let template = parse_expr(&mut t)?;
                if !t.eat_word("as") {
                    return Err(t.error("expected `as`"));
                }
                let alias = t.ident()?;
                t.finish()?;
                Ok(Stmt::Import(template, alias, line))
            }
            "from" => {
                let template = parse_expr(&mut t)?;
                if !t.eat_word("import") {
                    return Err(t.error("expected `import`"));
                }
                let mut names = Vec::new();
                loop {
                    let name = t.ident()?;
                    let alias = if t.eat_word("as") {
                        t.ident()?
                    } else {
                        name.clone()
                    };
                    names.push((name, alias));
                    if !t.eat_op(",") || t.done() {
                        break;
                    }
                }
                t.finish()?;
                Ok(Stmt::FromImport(template, names, line))
            }
            "include" => {
                let template = parse_expr(&mut t)?;
                let mut ignore_missing = false;
                if t.eat_word("ignore") {
                    if !t.eat_word("missing") {
                        return Err(t.error("expected `missing`"));
                    }
                    ignore_missing = true;
                }
                if t.eat_word("with") || t.eat_word("without") {
                    t.at = t.tokens.len(); // context is always passed
                }
                t.finish()?;
                Ok(Stmt::Include(template, ignore_missing, line))
            }
            "with" => {
                let mut bindings = Vec::new();
                while !t.done() {
                    let name = t.ident()?;
                    t.expect_op("=")?;
                    bindings.push((name, parse_expr(&mut t)?));
                    if !t.eat_op(",") {
                        break;
                    }
                }
                t.finish()?;
                let (body, _) = self.body(&["endwith"])?;
                Ok(Stmt::With(bindings, body))
            }
            other => Err(Error::syntax(format!("unknown tag `{other}`"), line)),
        }
    }

    fn if_statement(&mut self, mut t: Tokens) -> Result<Stmt> {
        let mut branches = Vec::new();
        let mut condition = parse_expr(&mut t)?;
        t.finish()?;
        loop {
            let (body, closer) = self.body(&["elif", "else", "endif"])?;
            branches.push((condition, body));
            let Some(mut closer) = closer else {
                return Err(Error::syntax("missing `endif`", t.line));
            };
            match closer.word.as_str() {
                "elif" => {
                    condition = parse_expr(&mut closer.tokens)?;
                    closer.tokens.finish()?;
                }
                "else" => {
                    let otherwise = self.body(&["endif"])?.0;
                    return Ok(Stmt::If(branches, otherwise));
                }
                _ => return Ok(Stmt::If(branches, Vec::new())),
            }
        }
    }
}

/// `a`, `a, b` or `(a, b)` up to the `stop` word or operator.
fn targets(t: &mut Tokens, stop: &str) -> Result<Vec<String>> {
    let parenthesised = t.eat_op("(");
    let mut names = Vec::new();
    loop {
        names.push(t.ident()?);
        if !t.eat_op(",") || t.is_word(stop) || t.is_op(stop) {
            break;
        }
    }
    if parenthesised {
        t.expect_op(")")?;
    }
    Ok(names)
}

/// Parses a template's text into statements.
pub fn parse(source: &str) -> Result<Vec<Stmt>> {
    let pieces = split(source)?;
    let mut parser = Parser {
        pieces: pieces.into_iter(),
    };
    Ok(parser.body(&[])?.0)
}

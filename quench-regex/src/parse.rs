//! Pattern text to a syntax tree.

use crate::class::{self, ClassSet, orbit_of};
use crate::error::Error;

/// A zero-width test.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Assertion {
    StartText,
    EndText,
    StartLine,
    EndLine,
    WordBoundary,
    NotWordBoundary,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Empty,
    Char(char),
    Class(ClassSet),
    /// `.`; `true` when it also matches a newline.
    Any(bool),
    Assert(Assertion),
    Concat(Vec<Self>),
    Alternate(Vec<Self>),
    Repeat {
        node: Box<Self>,
        min: u32,
        max: Option<u32>,
        greedy: bool,
    },
    /// A capturing group (index from 1).
    Group(Box<Self>, usize),
}

#[derive(Clone, Copy)]
#[allow(clippy::struct_excessive_bools)]
struct Flags {
    case_insensitive: bool,
    multi_line: bool,
    dot_all: bool,
    verbose: bool,
    swap_greed: bool,
}

/// The tree, the number of capture groups (not counting the whole match) and their names.
pub struct Parsed {
    pub node: Node,
    pub groups: usize,
    pub names: Vec<(String, usize)>,
}

/// Options set by the caller before the pattern's own flags apply.
#[derive(Clone, Copy, Default)]
#[allow(clippy::struct_excessive_bools)]
pub struct Options {
    pub case_insensitive: bool,
    pub multi_line: bool,
    pub dot_matches_new_line: bool,
    pub ignore_whitespace: bool,
    pub swap_greed: bool,
}

/// Most repetitions a counted repeat may ask for.
const MAX_REPEAT: u32 = 1000;
/// Deepest nesting of groups.
const MAX_DEPTH: usize = 250;

struct Parser {
    chars: Vec<char>,
    pos: usize,
    flags: Flags,
    groups: usize,
    names: Vec<(String, usize)>,
    depth: usize,
}

pub fn parse(pattern: &str, options: Options) -> Result<Parsed, Error> {
    let mut parser = Parser {
        chars: pattern.chars().collect(),
        pos: 0,
        flags: Flags {
            case_insensitive: options.case_insensitive,
            multi_line: options.multi_line,
            dot_all: options.dot_matches_new_line,
            verbose: options.ignore_whitespace,
            swap_greed: options.swap_greed,
        },
        groups: 0,
        names: Vec::new(),
        depth: 0,
    };
    let node = parser.alternation()?;
    if parser.pos < parser.chars.len() {
        // Only an unmatched `)` stops the top level early.
        return Err(parser.error("unopened group"));
    }
    Ok(Parsed {
        node,
        groups: parser.groups,
        names: parser.names,
    })
}

impl Parser {
    fn error(&self, message: &str) -> Error {
        Error::syntax(message, self.pos.min(self.chars.len()))
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn eat(&mut self, c: char) -> bool {
        if self.peek() == Some(c) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn alternation(&mut self) -> Result<Node, Error> {
        let mut branches = vec![self.concatenation()?];
        while self.eat('|') {
            branches.push(self.concatenation()?);
        }
        Ok(if branches.len() == 1 {
            branches.remove(0)
        } else {
            Node::Alternate(branches)
        })
    }

    fn concatenation(&mut self) -> Result<Node, Error> {
        let mut items = Vec::new();
        loop {
            self.skip_verbose();
            match self.peek() {
                None | Some('|' | ')') => break,
                Some(_) => {
                    // A flags-only group such as `(?i)` is not something a quantifier can repeat.
                    let Some(atom) = self.atom()? else { continue };
                    let repeated = self.repetition(atom)?;
                    items.push(repeated);
                }
            }
        }
        Ok(match items.len() {
            0 => Node::Empty,
            1 => items.remove(0),
            _ => Node::Concat(items),
        })
    }

    /// In verbose mode: spaces, and `#` comments to the end of the line, mean nothing.
    fn skip_verbose(&mut self) {
        if !self.flags.verbose {
            return;
        }
        while let Some(c) = self.peek() {
            if c.is_whitespace() {
                self.pos += 1;
            } else if c == '#' {
                while self.peek().is_some_and(|c| c != '\n') {
                    self.pos += 1;
                }
            } else {
                break;
            }
        }
    }

    fn repetition(&mut self, atom: Node) -> Result<Node, Error> {
        let mut node = atom;
        loop {
            self.skip_verbose();
            let mut counted = false;
            let (min, max) = match self.peek() {
                Some('*') => {
                    self.pos += 1;
                    (0, None)
                }
                Some('+') => {
                    self.pos += 1;
                    (1, None)
                }
                Some('?') => {
                    self.pos += 1;
                    (0, Some(1))
                }
                Some('{') => {
                    self.pos += 1;
                    counted = true;
                    self.counted()?
                }
                _ => return Ok(node),
            };
            let mut greedy = true;
            // In verbose mode only a counted repetition lets whitespace come before its lazy `?`.
            if counted {
                self.skip_verbose();
            }
            if self.eat('?') {
                greedy = false;
            }
            if self.flags.swap_greed {
                greedy = !greedy;
            }
            if let Some(max) = max
                && max < min
            {
                return Err(self.error("repetition quantifier has its minimum above its maximum"));
            }
            node = Node::Repeat {
                node: Box::new(node),
                min,
                max,
                greedy,
            };
        }
    }

    /// After `{`: `n}`, `n,}` or `n,m}`.
    fn counted(&mut self) -> Result<(u32, Option<u32>), Error> {
        let min = self
            .number()
            .ok_or_else(|| self.error("repetition quantifier expects a decimal number"))?;
        let max =
            if self.eat(',') {
                if self.peek() == Some('}') {
                    None
                } else {
                    Some(self.number().ok_or_else(|| {
                        self.error("repetition quantifier expects a decimal number")
                    })?)
                }
            } else {
                Some(min)
            };
        if !self.eat('}') {
            return Err(self.error("unclosed counted repetition"));
        }
        if min > MAX_REPEAT || max.is_some_and(|m| m > MAX_REPEAT) {
            return Err(self.error("repetition count is too large"));
        }
        Ok((min, max))
    }

    fn number(&mut self) -> Option<u32> {
        let start = self.pos;
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.pos += 1;
        }
        if self.pos == start {
            return None;
        }
        self.chars[start..self.pos]
            .iter()
            .collect::<String>()
            .parse()
            .ok()
    }

    /// The next atom; `None` for a flags-only group like `(?i)`, which changes the parser and matches nothing.
    fn atom(&mut self) -> Result<Option<Node>, Error> {
        let c = self.peek().ok_or_else(|| self.error("unexpected end"))?;
        match c {
            '(' => self.group(),
            '[' => {
                self.pos += 1;
                let set = self.class()?;
                Ok(Some(Node::Class(set)))
            }
            '.' => {
                self.pos += 1;
                Ok(Some(Node::Any(self.flags.dot_all)))
            }
            '^' => {
                self.pos += 1;
                Ok(Some(Node::Assert(if self.flags.multi_line {
                    Assertion::StartLine
                } else {
                    Assertion::StartText
                })))
            }
            '$' => {
                self.pos += 1;
                Ok(Some(Node::Assert(if self.flags.multi_line {
                    Assertion::EndLine
                } else {
                    Assertion::EndText
                })))
            }
            '\\' => {
                self.pos += 1;
                self.escape().map(Some)
            }
            '*' | '+' | '?' | '{' => Err(self.error("repetition operator missing its expression")),
            _ => {
                self.pos += 1;
                Ok(Some(self.literal(c)))
            }
        }
    }

    fn literal(&self, c: char) -> Node {
        if self.flags.case_insensitive {
            let orbit = orbit_of(c);
            if orbit.len() > 1 {
                let mut set = ClassSet::new();
                for member in orbit {
                    set.push(member as u32, member as u32);
                }
                return Node::Class(set);
            }
        }
        Node::Char(c)
    }

    fn group(&mut self) -> Result<Option<Node>, Error> {
        self.pos += 1; // (
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(self.error("groups are nested too deeply"));
        }
        let saved = self.flags;
        let mut capture: Option<(usize, Option<String>)> = None;
        if self.eat('?') {
            match self.peek() {
                Some(':') => self.pos += 1,
                Some('P' | '<') => {
                    if self.eat('P') && self.peek() != Some('<') {
                        return Err(self.error("expected `<` after `(?P`"));
                    }
                    self.pos += 1; // <
                    let name = self.group_name()?;
                    self.groups += 1;
                    capture = Some((self.groups, Some(name)));
                }
                _ => {
                    // Flags: `(?i)` for the rest of the enclosing group, `(?i:...)` for just this one.
                    let scoped = self.flag_list()?;
                    if !scoped {
                        self.depth -= 1;
                        return Ok(None);
                    }
                }
            }
        } else {
            self.groups += 1;
            capture = Some((self.groups, None));
        }
        if let Some((index, Some(name))) = &capture {
            if self.names.iter().any(|(n, _)| n == name) {
                return Err(self.error("duplicate capture group name"));
            }
            self.names.push((name.clone(), *index));
        }
        let inner = self.alternation()?;
        if !self.eat(')') {
            return Err(self.error("unclosed group"));
        }
        self.flags = saved;
        self.depth -= 1;
        Ok(Some(match capture {
            Some((index, _)) => Node::Group(Box::new(inner), index),
            None => inner,
        }))
    }

    fn group_name(&mut self) -> Result<String, Error> {
        let mut name = String::new();
        loop {
            match self.peek() {
                Some('>') => {
                    self.pos += 1;
                    break;
                }
                Some(c) if c.is_alphanumeric() || c == '_' => {
                    name.push(c);
                    self.pos += 1;
                }
                _ => return Err(self.error("invalid capture group name")),
            }
        }
        if name.is_empty() || name.starts_with(|c: char| c.is_ascii_digit()) {
            return Err(self.error("invalid capture group name"));
        }
        Ok(name)
    }

    /// After `(?`: reads `imsxU-imsxU` then `)` (returns `false`) or `:` (returns `true`).
    fn flag_list(&mut self) -> Result<bool, Error> {
        let mut on = true;
        loop {
            let c = self.peek().ok_or_else(|| self.error("unclosed group"))?;
            self.pos += 1;
            match c {
                'i' => self.flags.case_insensitive = on,
                'm' => self.flags.multi_line = on,
                's' => self.flags.dot_all = on,
                'x' => self.flags.verbose = on,
                'U' => self.flags.swap_greed = on,
                '-' if on => on = false,
                ')' => return Ok(false),
                ':' => return Ok(true),
                _ => return Err(self.error("unrecognized flag")),
            }
        }
    }

    fn escape(&mut self) -> Result<Node, Error> {
        let c = self.peek().ok_or_else(|| self.error("a lone `\\`"))?;
        self.pos += 1;
        Ok(match c {
            'd' => Node::Class(class::digit()),
            'D' => Node::Class(class::digit().negated()),
            's' => Node::Class(class::space()),
            'S' => Node::Class(class::space().negated()),
            'w' => Node::Class(class::word()),
            'W' => Node::Class(class::word().negated()),
            'b' => Node::Assert(Assertion::WordBoundary),
            'B' => Node::Assert(Assertion::NotWordBoundary),
            'A' => Node::Assert(Assertion::StartText),
            'z' => Node::Assert(Assertion::EndText),
            _ => {
                self.pos -= 1;
                let literal = self.escaped_char()?;
                self.literal(literal)
            }
        })
    }

    /// A single escaped character: `\n`, `\x41`, `\u{263A}`, `\.`.
    fn escaped_char(&mut self) -> Result<char, Error> {
        let c = self.peek().ok_or_else(|| self.error("a lone `\\`"))?;
        self.pos += 1;
        Ok(match c {
            'n' => '\n',
            't' => '\t',
            'r' => '\r',
            'f' => '\u{0C}',
            'v' => '\u{0B}',
            'a' => '\u{07}',
            'x' => self.hex_escape(2)?,
            'u' => self.hex_escape(4)?,
            'U' => self.hex_escape(8)?,
            c if c.is_ascii_punctuation() || c == ' ' => c,
            c if c.is_ascii_alphanumeric() => {
                return Err(self.error("unrecognized escape sequence"));
            }
            c => c,
        })
    }

    /// `\xHH`, `\x{H..}` and the same for `\u` / `\U`.
    fn hex_escape(&mut self, digits: usize) -> Result<char, Error> {
        let mut text = String::new();
        if self.eat('{') {
            loop {
                match self.peek() {
                    Some('}') => {
                        self.pos += 1;
                        break;
                    }
                    Some(c) if c.is_ascii_hexdigit() => {
                        text.push(c);
                        self.pos += 1;
                    }
                    _ => return Err(self.error("invalid hexadecimal escape")),
                }
            }
        } else {
            for _ in 0..digits {
                match self.peek() {
                    Some(c) if c.is_ascii_hexdigit() => {
                        text.push(c);
                        self.pos += 1;
                    }
                    _ => return Err(self.error("invalid hexadecimal escape")),
                }
            }
        }
        u32::from_str_radix(&text, 16)
            .ok()
            .and_then(char::from_u32)
            .ok_or_else(|| self.error("invalid hexadecimal escape"))
    }

    /// After `[`: a bracketed class, up to and including `]`.
    fn class(&mut self) -> Result<ClassSet, Error> {
        let negated = self.eat('^');
        let mut set = ClassSet::new();
        let mut first = true;
        loop {
            let c = self
                .peek()
                .ok_or_else(|| self.error("unclosed character class"))?;
            if c == ']' && !first {
                self.pos += 1;
                break;
            }
            first = false;
            if c == '[' {
                if self.chars.get(self.pos + 1) == Some(&':') {
                    set.push_set(&self.posix_class()?);
                } else {
                    self.pos += 1;
                    let nested = self.class()?;
                    set.push_set(&nested);
                }
                continue;
            }
            if c == '&' && self.chars.get(self.pos + 1) == Some(&'&')
                || c == '~' && self.chars.get(self.pos + 1) == Some(&'~')
                || c == '-' && self.chars.get(self.pos + 1) == Some(&'-')
            {
                return Err(self.error("set operations in a class are not supported"));
            }
            let low = self.class_atom(&mut set)?;
            let Some(low) = low else { continue };
            // A range `a-z`, unless the `-` is the last thing in the class.
            if self.peek() == Some('-') && self.chars.get(self.pos + 1).is_some_and(|c| *c != ']') {
                self.pos += 1;
                let mut scratch = ClassSet::new();
                let high = self.class_atom(&mut scratch)?;
                let Some(high) = high else {
                    return Err(self.error("a class shorthand cannot end a range"));
                };
                if high < low {
                    return Err(self.error("class range is out of order"));
                }
                set.push(low as u32, high as u32);
            } else {
                set.push(low as u32, low as u32);
            }
        }
        if self.flags.case_insensitive {
            set.fold_case();
        }
        Ok(if negated { set.negated() } else { set })
    }

    /// One class member: `Some(char)` for a character (the caller adds or ranges it), `None` after adding a
    /// shorthand such as `\d` straight into `set`.
    fn class_atom(&mut self, set: &mut ClassSet) -> Result<Option<char>, Error> {
        let c = self
            .peek()
            .ok_or_else(|| self.error("unclosed character class"))?;
        self.pos += 1;
        if c != '\\' {
            return Ok(Some(c));
        }
        let e = self.peek().ok_or_else(|| self.error("a lone `\\`"))?;
        match e {
            'd' => set.push_set(&class::digit()),
            'D' => set.push_set(&class::digit().negated()),
            's' => set.push_set(&class::space()),
            'S' => set.push_set(&class::space().negated()),
            'w' => set.push_set(&class::word()),
            'W' => set.push_set(&class::word().negated()),
            _ => return self.escaped_char().map(Some),
        }
        self.pos += 1;
        Ok(None)
    }

    /// `[:alpha:]` and friends (ASCII), `[:^alpha:]` negated.
    fn posix_class(&mut self) -> Result<ClassSet, Error> {
        let start = self.pos;
        self.pos += 2; // [:
        let negated = self.eat('^');
        let mut name = String::new();
        while self.peek().is_some_and(|c| c.is_ascii_lowercase()) {
            name.push(self.chars[self.pos]);
            self.pos += 1;
        }
        if !(self.eat(':') && self.eat(']')) {
            self.pos = start;
            return Err(self.error("malformed POSIX class"));
        }
        let ranges: &[(u32, u32)] = match name.as_str() {
            "alnum" => &[(0x30, 0x39), (0x41, 0x5A), (0x61, 0x7A)],
            "alpha" => &[(0x41, 0x5A), (0x61, 0x7A)],
            "ascii" => &[(0, 0x7F)],
            "blank" => &[(0x09, 0x09), (0x20, 0x20)],
            "cntrl" => &[(0, 0x1F), (0x7F, 0x7F)],
            "digit" => &[(0x30, 0x39)],
            "graph" => &[(0x21, 0x7E)],
            "lower" => &[(0x61, 0x7A)],
            "print" => &[(0x20, 0x7E)],
            "punct" => &[(0x21, 0x2F), (0x3A, 0x40), (0x5B, 0x60), (0x7B, 0x7E)],
            "space" => &[(0x09, 0x0D), (0x20, 0x20)],
            "upper" => &[(0x41, 0x5A)],
            "word" => &[(0x30, 0x39), (0x41, 0x5A), (0x5F, 0x5F), (0x61, 0x7A)],
            "xdigit" => &[(0x30, 0x39), (0x41, 0x46), (0x61, 0x66)],
            _ => {
                self.pos = start;
                return Err(self.error("unknown POSIX class"));
            }
        };
        let set = ClassSet::from_table(ranges);
        Ok(if negated { set.negated() } else { set })
    }
}

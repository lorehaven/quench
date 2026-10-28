//! Stylesheets: rules with nesting resolved, media and supports conditions, and keyframes.

use std::rc::Rc;

use crate::media::{MediaQuery, parse_media_queries};
use crate::properties;
use crate::selector::{Complex, Specificity, parse_selector_list, resolve_nesting};
use crate::token::{Token, tokenize};
use crate::value::{Value, parse_values, split_commas};

/// `name: value`.
#[derive(Debug, Clone, PartialEq)]
pub struct Declaration {
    /// Lowercase, except a custom property (`--x`), whose case is kept.
    pub name: String,
    pub value: Vec<Value>,
    pub important: bool,
}

/// One selector with its declarations: a rule with a selector list is one `Rule` per selector.
#[derive(Debug, Clone)]
pub struct Rule {
    pub selector: Complex,
    pub specificity: Specificity,
    pub declarations: Rc<Vec<Declaration>>,
    /// The `@media` conditions around the rule, outermost first: every level must hold.
    pub media: Vec<Vec<MediaQuery>>,
    /// Source order: a later rule wins a tie of specificity.
    pub order: usize,
}

/// A named animation: where in its course each set of declarations applies (0 to 1).
#[derive(Debug, Clone, PartialEq)]
pub struct Keyframes {
    pub name: String,
    pub frames: Vec<(f32, Vec<Declaration>)>,
}

/// A parsed stylesheet.
#[derive(Debug, Clone, Default)]
pub struct Stylesheet {
    pub rules: Vec<Rule>,
    pub keyframes: Vec<Keyframes>,
    /// What was not understood and left out, for tests and diagnostics: a selector, an at-rule, a property.
    pub ignored: Vec<String>,
}

struct Parser {
    sheet: Stylesheet,
    order: usize,
}

/// Parses `text`. Nothing here fails: what cannot be understood is skipped (and listed in
/// [`Stylesheet::ignored`]), as a browser does.
#[must_use]
pub fn parse(text: &str) -> Stylesheet {
    let tokens = tokenize(text);
    let mut parser = Parser {
        sheet: Stylesheet::default(),
        order: 0,
    };
    parser.block(&tokens, None, &[]);
    parser.sheet.rules.sort_by_key(|r| r.order);
    parser.sheet
}

/// Parses the declarations of a `style` attribute (`color: red; width: 10px`).
#[must_use]
pub fn parse_declarations(text: &str) -> Vec<Declaration> {
    let tokens = tokenize(text);
    let mut parser = Parser {
        sheet: Stylesheet::default(),
        order: 0,
    };
    parser.block(&tokens, None, &[])
}

/// Where a token slice at `at` ends: the first `;`, `{` or `}` outside parentheses and brackets.
const fn find_end(tokens: &[Token], mut at: usize) -> (usize, Option<Token>) {
    let mut depth = 0i32;
    while at < tokens.len() {
        match &tokens[at] {
            Token::Open('{') if depth == 0 => return (at, Some(Token::Open('{'))),
            Token::Open(_) | Token::Function(_) => depth += 1,
            Token::Close('}') if depth == 0 => return (at, Some(Token::Close('}'))),
            Token::Close(_) => depth -= 1,
            Token::Semicolon if depth == 0 => return (at, Some(Token::Semicolon)),
            _ => {}
        }
        at += 1;
    }
    (at, None)
}

/// The tokens of the `{ ... }` block that opens at `open`, and the index after its closing brace.
fn block_at(tokens: &[Token], open: usize) -> (&[Token], usize) {
    let mut depth = 0i32;
    let mut at = open;
    while at < tokens.len() {
        match tokens[at] {
            Token::Open(_) | Token::Function(_) => depth += 1,
            Token::Close(_) => {
                depth -= 1;
                if depth == 0 {
                    return (&tokens[open + 1..at], at + 1);
                }
            }
            _ => {}
        }
        at += 1;
    }
    (&tokens[(open + 1).min(tokens.len())..], tokens.len())
}

fn trimmed(tokens: &[Token]) -> &[Token] {
    let start = tokens
        .iter()
        .position(|t| *t != Token::Whitespace)
        .unwrap_or(tokens.len());
    let end = tokens
        .iter()
        .rposition(|t| *t != Token::Whitespace)
        .map_or(start, |e| e + 1);
    &tokens[start..end]
}

impl Parser {
    /// The contents of a stylesheet or of a rule's block: nested rules and at-rules are added to the sheet; the
    /// declarations that stand directly in it are returned (for a rule block, they are the rule's own).
    fn block(
        &mut self,
        tokens: &[Token],
        parents: Option<&[Complex]>,
        media: &[Vec<MediaQuery>],
    ) -> Vec<Declaration> {
        let mut declarations = Vec::new();
        let mut i = 0;
        while i < tokens.len() {
            match &tokens[i] {
                Token::Whitespace | Token::Semicolon | Token::Close('}') => i += 1,
                Token::AtKeyword(name) => {
                    let (end, terminator) = find_end(tokens, i + 1);
                    let prelude = trimmed(&tokens[i + 1..end]);
                    let name = name.to_ascii_lowercase();
                    if terminator == Some(Token::Open('{')) {
                        let (body, next) = block_at(tokens, end);
                        self.at_rule(&name, prelude, body, parents, media, &mut declarations);
                        i = next;
                    } else {
                        // `@import ...;`, `@charset ...;`: not used here.
                        self.sheet.ignored.push(format!("@{name}"));
                        i = end + 1;
                    }
                }
                _ => {
                    let (end, terminator) = find_end(tokens, i);
                    if terminator == Some(Token::Open('{')) {
                        let selectors = trimmed(&tokens[i..end]);
                        let (body, next) = block_at(tokens, end);
                        self.rule(selectors, body, parents, media);
                        i = next;
                    } else {
                        if let Some(d) = self.declaration(&tokens[i..end]) {
                            declarations.push(d);
                        }
                        i = end + 1;
                    }
                }
            }
        }
        declarations
    }

    fn rule(
        &mut self,
        selector_tokens: &[Token],
        body: &[Token],
        parents: Option<&[Complex]>,
        media: &[Vec<MediaQuery>],
    ) {
        let order = self.next_order();
        let parsed = parse_selector_list(selector_tokens);
        let expected = selector_tokens
            .iter()
            .filter(|t| **t == Token::Comma)
            .count()
            + 1;
        if parsed.len() < expected {
            self.sheet
                .ignored
                .push(format!("selector in `{}`", describe(selector_tokens)));
        }
        let mut selectors = Vec::new();
        for s in parsed {
            match parents {
                Some(parents) => {
                    for p in parents {
                        match resolve_nesting(p, &s) {
                            Some(resolved) => selectors.push(resolved),
                            None => self
                                .sheet
                                .ignored
                                .push(format!("nesting in `{}`", describe(selector_tokens))),
                        }
                    }
                }
                None if s.has_nesting() => self.sheet.ignored.push(format!(
                    "`&` outside a rule in `{}`",
                    describe(selector_tokens)
                )),
                None => selectors.push(s),
            }
        }
        let declarations = self.block(body, Some(&selectors), media);
        self.push_rules(&selectors, declarations, media, order);
    }

    fn push_rules(
        &mut self,
        selectors: &[Complex],
        declarations: Vec<Declaration>,
        media: &[Vec<MediaQuery>],
        order: usize,
    ) {
        if declarations.is_empty() {
            return;
        }
        let declarations = Rc::new(declarations);
        for selector in selectors {
            self.sheet.rules.push(Rule {
                specificity: selector.specificity(),
                selector: selector.clone(),
                declarations: Rc::clone(&declarations),
                media: media.to_vec(),
                order,
            });
        }
    }

    const fn next_order(&mut self) -> usize {
        self.order += 1;
        self.order
    }

    fn at_rule(
        &mut self,
        name: &str,
        prelude: &[Token],
        body: &[Token],
        parents: Option<&[Complex]>,
        media: &[Vec<MediaQuery>],
        declarations: &mut Vec<Declaration>,
    ) {
        match name {
            "media" => {
                let mut levels = media.to_vec();
                levels.push(parse_media_queries(&parse_values(prelude)));
                let order = self.next_order();
                let own = self.block(body, parents, &levels);
                if let Some(parents) = parents {
                    self.push_rules(parents, own, &levels, order);
                }
            }
            "supports" => {
                if supports(&parse_values(prelude)) {
                    declarations.extend(self.block(body, parents, media));
                }
            }
            "keyframes" | "-webkit-keyframes" => self.keyframes(prelude, body),
            "layer" | "container" if body.contains(&Token::Open('{')) || !body.is_empty() => {
                declarations.extend(self.block(body, parents, media));
            }
            other => self.sheet.ignored.push(format!("@{other}")),
        }
    }

    fn keyframes(&mut self, prelude: &[Token], body: &[Token]) {
        let name = match parse_values(prelude).first() {
            Some(Value::Ident(n) | Value::Str(n)) => n.clone(),
            _ => return,
        };
        let mut frames = Vec::new();
        let mut i = 0;
        while i < body.len() {
            if matches!(body[i], Token::Whitespace) {
                i += 1;
                continue;
            }
            let (end, terminator) = find_end(body, i);
            if terminator != Some(Token::Open('{')) {
                break;
            }
            let selector = parse_values(&body[i..end]);
            let (inner, next) = block_at(body, end);
            let mut decls = Vec::new();
            for statement in inner.split(|t| *t == Token::Semicolon) {
                if let Some(d) = self.declaration(statement) {
                    decls.push(d);
                }
            }
            for part in split_commas(&selector) {
                let offset = match part.as_slice() {
                    [Value::Ident(w)] if w.eq_ignore_ascii_case("from") => Some(0.0),
                    [Value::Ident(w)] if w.eq_ignore_ascii_case("to") => Some(1.0),
                    [Value::Percent(p)] => Some(p / 100.0),
                    _ => None,
                };
                if let Some(offset) = offset {
                    frames.push((offset, decls.clone()));
                }
            }
            i = next;
        }
        frames.sort_by(|a, b| a.0.total_cmp(&b.0));
        self.sheet.keyframes.push(Keyframes { name, frames });
    }

    fn declaration(&mut self, tokens: &[Token]) -> Option<Declaration> {
        let tokens = trimmed(tokens);
        let [Token::Ident(name), rest @ ..] = tokens else {
            if !tokens.is_empty() {
                self.sheet
                    .ignored
                    .push(format!("declaration `{}`", describe(tokens)));
            }
            return None;
        };
        let rest = trimmed(rest);
        let [Token::Colon, value @ ..] = rest else {
            return None;
        };
        let mut value = parse_values(trimmed(value));
        let important = matches!(value.as_slice(), [.., Value::Delim('!'), Value::Ident(word)] if word.eq_ignore_ascii_case("important"));
        if important {
            value.truncate(value.len() - 2);
        }
        let name = if name.starts_with("--") {
            name.clone()
        } else {
            name.to_ascii_lowercase()
        };
        if !properties::is_known(&name) {
            self.sheet.ignored.push(format!("property {name}"));
        }
        Some(Declaration {
            name,
            value,
            important,
        })
    }
}

fn describe(tokens: &[Token]) -> String {
    tokens
        .iter()
        .map(|t| match t {
            Token::Ident(s) | Token::Function(s) | Token::Hash(s) | Token::Str(s) => s.clone(),
            Token::Delim(c) | Token::Open(c) | Token::Close(c) => c.to_string(),
            Token::Colon => ":".into(),
            Token::Comma => ",".into(),
            Token::Whitespace => " ".into(),
            Token::Number { value, unit } => format!("{value}{unit}"),
            Token::AtKeyword(s) => format!("@{s}"),
            Token::Semicolon => ";".into(),
        })
        .collect()
}

/// Whether an `@supports` condition holds: `(property: value)` is true for a property this engine knows,
/// combined with `not`, `and` and `or`.
fn supports(prelude: &[Value]) -> bool {
    let mut result: Option<bool> = None;
    let mut operator = "and";
    let mut negate = false;
    for v in prelude {
        match v {
            Value::Ident(w) if w.eq_ignore_ascii_case("not") => negate = true,
            Value::Ident(w) if w.eq_ignore_ascii_case("and") => operator = "and",
            Value::Ident(w) if w.eq_ignore_ascii_case("or") => operator = "or",
            Value::Block('(', inner) => {
                let holds = match inner.as_slice() {
                    [Value::Ident(prop), Value::Delim(':'), ..] => {
                        properties::is_known(&prop.to_ascii_lowercase())
                    }
                    nested => supports(nested),
                } != negate;
                negate = false;
                result = Some(match (result, operator) {
                    (None, _) => holds,
                    (Some(r), "or") => r || holds,
                    (Some(r), _) => r && holds,
                });
            }
            _ => return false,
        }
    }
    result.unwrap_or(false)
}

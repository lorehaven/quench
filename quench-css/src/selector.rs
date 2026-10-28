//! Selectors: parsing, specificity, and matching against any tree that can say what an element is.

use crate::token::Token;

/// What a selector needs to know about an element of the tree it is matched against.
pub trait Element: Sized {
    /// The tag name, lowercase.
    fn tag(&self) -> String;
    fn id(&self) -> Option<String>;
    fn has_class(&self, class: &str) -> bool;
    fn attribute(&self, name: &str) -> Option<String>;
    fn parent(&self) -> Option<Self>;
    /// The element just before this one among its parent's children.
    fn previous_sibling(&self) -> Option<Self>;
    /// Whether it is the root of the document.
    fn is_root(&self) -> bool {
        self.parent().is_none()
    }
    /// A state that comes from interaction, not from the markup.
    fn state(&self, state: State) -> bool;
}

/// The interaction states a selector can ask about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Hover,
    Focus,
    FocusVisible,
    Active,
    Disabled,
    Checked,
}

/// How two compounds are related.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Combinator {
    Descendant,
    Child,
    NextSibling,
    LaterSibling,
}

/// How an attribute is compared.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttrOp {
    Exists,
    Equals,
    Includes,
    Prefix,
    Suffix,
    Substring,
    DashMatch,
}

/// One condition of a compound selector.
#[derive(Debug, Clone, PartialEq)]
pub enum Simple {
    Id(String),
    Class(String),
    Attribute {
        name: String,
        op: AttrOp,
        value: String,
        ignore_case: bool,
    },
    /// `:hover`, `:root`, `:first-child` ... by name.
    Pseudo(String),
    Not(Vec<Complex>),
    /// A pseudo-class or pseudo-element this engine does not know: it never matches.
    Unknown,
    /// `&`: the parent rule's selector (replaced when nesting is resolved).
    Nesting,
}

/// A tag (or `*`) and conditions: `button.q-chip[aria-pressed="true"]`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Compound {
    /// `None` for any element.
    pub tag: Option<String>,
    pub conditions: Vec<Simple>,
}

/// Compounds joined by combinators, left to right: `a > b c` is `[(_, a), (Child, b), (Descendant, c)]`; the
/// combinator of the first is unused.
#[derive(Debug, Clone, PartialEq)]
pub struct Complex {
    pub parts: Vec<(Combinator, Compound)>,
}

/// How strongly a selector claims an element: ids, then classes/attributes/pseudo-classes, then tags.
pub type Specificity = u32;

impl Complex {
    #[must_use]
    pub fn specificity(&self) -> Specificity {
        let (mut ids, mut classes, mut tags) = (0u32, 0u32, 0u32);
        for (_, compound) in &self.parts {
            tags += u32::from(compound.tag.is_some());
            for c in &compound.conditions {
                match c {
                    Simple::Id(_) => ids += 1,
                    Simple::Class(_) | Simple::Attribute { .. } | Simple::Pseudo(_) => classes += 1,
                    Simple::Not(list) => {
                        // `:not()` counts as its most specific argument.
                        let top = list.iter().map(Self::specificity).max().unwrap_or(0);
                        ids += top >> 16;
                        classes += (top >> 8) & 0xFF;
                        tags += top & 0xFF;
                    }
                    Simple::Unknown | Simple::Nesting => {}
                }
            }
        }
        (ids.min(255) << 16) | (classes.min(255) << 8) | tags.min(255)
    }

    /// Whether `element` is matched by this selector.
    #[must_use]
    pub fn matches<E: Element>(&self, element: &E) -> bool {
        matches_from(&self.parts, self.parts.len() - 1, element)
    }

    /// Whether any compound still holds a `&` that was not resolved.
    #[must_use]
    pub fn has_nesting(&self) -> bool {
        self.parts
            .iter()
            .any(|(_, c)| c.conditions.contains(&Simple::Nesting))
    }
}

fn matches_from<E: Element>(parts: &[(Combinator, Compound)], at: usize, element: &E) -> bool {
    if !compound_matches(&parts[at].1, element) {
        return false;
    }
    if at == 0 {
        return true;
    }
    let combinator = parts[at].0;
    match combinator {
        Combinator::Child => element
            .parent()
            .is_some_and(|p| matches_from(parts, at - 1, &p)),
        Combinator::Descendant => {
            let mut up = element.parent();
            while let Some(ancestor) = up {
                if matches_from(parts, at - 1, &ancestor) {
                    return true;
                }
                up = ancestor.parent();
            }
            false
        }
        Combinator::NextSibling => element
            .previous_sibling()
            .is_some_and(|s| matches_from(parts, at - 1, &s)),
        Combinator::LaterSibling => {
            let mut before = element.previous_sibling();
            while let Some(sibling) = before {
                if matches_from(parts, at - 1, &sibling) {
                    return true;
                }
                before = sibling.previous_sibling();
            }
            false
        }
    }
}

fn compound_matches<E: Element>(compound: &Compound, element: &E) -> bool {
    if let Some(tag) = &compound.tag
        && !tag.eq_ignore_ascii_case(&element.tag())
    {
        return false;
    }
    compound.conditions.iter().all(|c| match c {
        Simple::Id(id) => element.id().as_deref() == Some(id.as_str()),
        Simple::Class(class) => element.has_class(class),
        Simple::Attribute {
            name,
            op,
            value,
            ignore_case,
        } => element
            .attribute(name)
            .is_some_and(|actual| attribute_matches(*op, &actual, value, *ignore_case)),
        Simple::Pseudo(name) => pseudo_matches(name, element),
        Simple::Not(list) => !list.iter().any(|s| s.matches(element)),
        Simple::Unknown | Simple::Nesting => false,
    })
}

fn attribute_matches(op: AttrOp, actual: &str, wanted: &str, ignore_case: bool) -> bool {
    let (a, w) = if ignore_case {
        (actual.to_ascii_lowercase(), wanted.to_ascii_lowercase())
    } else {
        (actual.to_string(), wanted.to_string())
    };
    match op {
        AttrOp::Exists => true,
        AttrOp::Equals => a == w,
        AttrOp::Includes => a.split_whitespace().any(|part| part == w),
        AttrOp::Prefix => !w.is_empty() && a.starts_with(&w),
        AttrOp::Suffix => !w.is_empty() && a.ends_with(&w),
        AttrOp::Substring => !w.is_empty() && a.contains(&w),
        AttrOp::DashMatch => a == w || a.strip_prefix(&w).is_some_and(|rest| rest.starts_with('-')),
    }
}

fn pseudo_matches<E: Element>(name: &str, element: &E) -> bool {
    match name {
        "root" => element.is_root(),
        "hover" => element.state(State::Hover),
        "focus" => element.state(State::Focus),
        "focus-visible" => element.state(State::FocusVisible),
        "active" => element.state(State::Active),
        "disabled" => element.state(State::Disabled) || element.attribute("disabled").is_some(),
        "enabled" => !(element.state(State::Disabled) || element.attribute("disabled").is_some()),
        "checked" => element.state(State::Checked) || element.attribute("checked").is_some(),
        "first-child" => element.previous_sibling().is_none(),
        _ => false,
    }
}

/// Parses a selector list (the prelude of a rule) from its tokens. A selector that cannot be parsed is left out,
/// as a browser drops an invalid one.
#[must_use]
pub fn parse_selector_list(tokens: &[Token]) -> Vec<Complex> {
    split_top_level(tokens)
        .iter()
        .filter_map(|part| parse_complex(part))
        .collect()
}

/// Splits tokens at commas that are not inside parentheses or brackets.
fn split_top_level(tokens: &[Token]) -> Vec<Vec<Token>> {
    let mut parts = vec![Vec::new()];
    let mut depth = 0i32;
    for t in tokens {
        match t {
            Token::Open(_) | Token::Function(_) => depth += 1,
            Token::Close(_) => depth -= 1,
            Token::Comma if depth == 0 => {
                parts.push(Vec::new());
                continue;
            }
            _ => {}
        }
        parts.last_mut().expect("at least one part").push(t.clone());
    }
    parts
}

fn parse_complex(tokens: &[Token]) -> Option<Complex> {
    let mut parts: Vec<(Combinator, Compound)> = Vec::new();
    let mut compound = Compound::default();
    let mut has_content = false;
    let mut combinator = Combinator::Descendant;
    let mut pending_space = false;
    let mut i = 0;
    let flush = |compound: &mut Compound,
                 has: &mut bool,
                 combinator: &mut Combinator,
                 parts: &mut Vec<(Combinator, Compound)>| {
        if *has {
            parts.push((*combinator, std::mem::take(compound)));
            *has = false;
            *combinator = Combinator::Descendant;
        }
    };
    while i < tokens.len() {
        match &tokens[i] {
            Token::Whitespace => {
                pending_space = true;
                i += 1;
                continue;
            }
            Token::Delim(c @ ('>' | '+' | '~')) => {
                flush(&mut compound, &mut has_content, &mut combinator, &mut parts);
                combinator = match c {
                    '>' => Combinator::Child,
                    '+' => Combinator::NextSibling,
                    _ => Combinator::LaterSibling,
                };
                pending_space = false;
                i += 1;
                continue;
            }
            _ => {}
        }
        // A space between two compounds is a descendant combinator.
        if pending_space && has_content {
            flush(&mut compound, &mut has_content, &mut combinator, &mut parts);
        }
        pending_space = false;
        match &tokens[i] {
            Token::Ident(name) if !has_content => {
                compound.tag = Some(name.to_ascii_lowercase());
                has_content = true;
                i += 1;
            }
            Token::Delim('*') if !has_content => {
                has_content = true;
                i += 1;
            }
            Token::Delim('&') => {
                compound.conditions.push(Simple::Nesting);
                has_content = true;
                i += 1;
            }
            Token::Hash(id) => {
                compound.conditions.push(Simple::Id(id.clone()));
                has_content = true;
                i += 1;
            }
            Token::Delim('.') => {
                let Some(Token::Ident(class)) = tokens.get(i + 1) else {
                    return None;
                };
                compound.conditions.push(Simple::Class(class.clone()));
                has_content = true;
                i += 2;
            }
            Token::Open('[') => {
                let end = tokens[i..].iter().position(|t| *t == Token::Close(']'))? + i;
                compound
                    .conditions
                    .push(parse_attribute(&tokens[i + 1..end])?);
                has_content = true;
                i = end + 1;
            }
            Token::Colon => {
                let double = tokens.get(i + 1) == Some(&Token::Colon);
                let at = i + if double { 2 } else { 1 };
                match tokens.get(at) {
                    Some(Token::Ident(name)) => {
                        compound.conditions.push(if double {
                            Simple::Unknown
                        } else {
                            Simple::Pseudo(name.to_ascii_lowercase())
                        });
                        i = at + 1;
                    }
                    Some(Token::Function(name)) => {
                        let mut depth = 1;
                        let mut end = at + 1;
                        while end < tokens.len() && depth > 0 {
                            match tokens[end] {
                                Token::Open(_) | Token::Function(_) => depth += 1,
                                Token::Close(_) => depth -= 1,
                                _ => {}
                            }
                            end += 1;
                        }
                        let inner = &tokens[at + 1..end.saturating_sub(1)];
                        compound
                            .conditions
                            .push(match name.to_ascii_lowercase().as_str() {
                                "not" if !double => Simple::Not(parse_selector_list(inner)),
                                "is" | "where" if !double => {
                                    // `:is(a, b)` as `:not(:not(a, b))` would lose specificity; the kit never uses it.
                                    Simple::Unknown
                                }
                                _ => Simple::Unknown,
                            });
                        i = end;
                    }
                    _ => return None,
                }
                has_content = true;
            }
            _ => return None,
        }
    }
    flush(&mut compound, &mut has_content, &mut combinator, &mut parts);
    if parts.is_empty() {
        return None;
    }
    Some(Complex { parts })
}

fn parse_attribute(tokens: &[Token]) -> Option<Simple> {
    let significant: Vec<&Token> = tokens.iter().filter(|t| **t != Token::Whitespace).collect();
    let Some(Token::Ident(name)) = significant.first() else {
        return None;
    };
    let name = name.to_ascii_lowercase();
    if significant.len() == 1 {
        return Some(Simple::Attribute {
            name,
            op: AttrOp::Exists,
            value: String::new(),
            ignore_case: false,
        });
    }
    let (op, rest) = match (significant.get(1), significant.get(2)) {
        (Some(Token::Delim('=')), _) => (AttrOp::Equals, 2),
        (Some(Token::Delim(c @ ('~' | '^' | '$' | '*' | '|'))), Some(Token::Delim('='))) => (
            match c {
                '~' => AttrOp::Includes,
                '^' => AttrOp::Prefix,
                '$' => AttrOp::Suffix,
                '*' => AttrOp::Substring,
                _ => AttrOp::DashMatch,
            },
            3,
        ),
        _ => return None,
    };
    let value = match significant.get(rest)? {
        Token::Ident(v) | Token::Str(v) => v.clone(),
        Token::Number { value, unit } => format!("{value}{unit}"),
        _ => return None,
    };
    let ignore_case =
        matches!(significant.get(rest + 1), Some(Token::Ident(f)) if f.eq_ignore_ascii_case("i"));
    Some(Simple::Attribute {
        name,
        op,
        value,
        ignore_case,
    })
}

/// Resolves the `&` of a nested selector against its parent. A selector without `&` is a descendant of the
/// parent. `None` when `&` is somewhere this engine does not handle (not in the first compound).
#[must_use]
pub fn resolve_nesting(parent: &Complex, nested: &Complex) -> Option<Complex> {
    let first_has_nesting = nested.parts[0].1.conditions.contains(&Simple::Nesting);
    let later_has_nesting = nested.parts[1..]
        .iter()
        .any(|(_, c)| c.conditions.contains(&Simple::Nesting));
    if later_has_nesting {
        return None;
    }
    let mut parts = parent.parts.clone();
    if first_has_nesting {
        let first = &nested.parts[0].1;
        let last = &mut parts.last_mut().expect("a selector has a compound").1;
        if first.tag.is_some() && last.tag.is_some() && first.tag != last.tag {
            return None; // `a&` against `b`: can never match
        }
        if last.tag.is_none() {
            last.tag.clone_from(&first.tag);
        }
        last.conditions.extend(
            first
                .conditions
                .iter()
                .filter(|s| **s != Simple::Nesting)
                .cloned(),
        );
    } else {
        parts.push((nested.parts[0].0, nested.parts[0].1.clone()));
    }
    parts.extend(nested.parts[1..].iter().cloned());
    Some(Complex { parts })
}

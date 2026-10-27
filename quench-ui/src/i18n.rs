//! Translations at run time, from Fluent (`.ftl`) files, with plural forms.
//!
//! The files the server-side tools already use can be embedded in the app (`include_str!`) and
//! switched without a reload. The subset of Fluent understood is what apps need: messages, `{ $var }`,
//! references to other messages, number and string literals, `NUMBER($n)`, and selectors
//! (`{ $n -> [one] ... *[other] ... }`) on plural categories or exact numbers. Attributes, terms and
//! other functions are skipped.
//!
//! ```
//! use quench_ui::i18n::I18n;
//!
//! let i18n = I18n::new(
//!     &[("en", "files = { $n ->\n    [one] { $n } file\n   *[other] { $n } files\n}")],
//!     "en",
//! ).unwrap();
//! assert_eq!(i18n.tr("files", &[("n", 1.into())]), "1 file");
//! assert_eq!(i18n.tr("files", &[("n", 5.into())]), "5 files");
//! ```

use crate::reactive::{Signal, create_signal};
use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;

/// A value a message can use.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Number(f64),
    Text(String),
}

impl From<i32> for Value {
    fn from(n: i32) -> Self {
        Self::Number(f64::from(n))
    }
}
impl From<u32> for Value {
    fn from(n: u32) -> Self {
        Self::Number(f64::from(n))
    }
}
impl From<usize> for Value {
    #[allow(clippy::cast_precision_loss)]
    fn from(n: usize) -> Self {
        Self::Number(n as f64)
    }
}
impl From<f64> for Value {
    fn from(n: f64) -> Self {
        Self::Number(n)
    }
}
impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Self::Text(s.to_string())
    }
}
impl From<String> for Value {
    fn from(s: String) -> Self {
        Self::Text(s)
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Number(n) => write!(f, "{n}"),
            Self::Text(s) => f.write_str(s),
        }
    }
}

/// The plural categories of CLDR.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    Zero,
    One,
    Two,
    Few,
    Many,
    Other,
}

impl Category {
    const fn name(self) -> &'static str {
        match self {
            Self::Zero => "zero",
            Self::One => "one",
            Self::Two => "two",
            Self::Few => "few",
            Self::Many => "many",
            Self::Other => "other",
        }
    }
}

/// The plural category of `n` in `language` (`en`, `pl-PL`, ...).
///
/// Languages without rules here count like English. A number is judged by its written form, so a
/// number with decimals is not "one" in English, as CLDR has it.
#[must_use]
pub fn plural_category(language: &str, n: f64) -> Category {
    let text = format!("{}", n.abs());
    let (int, fraction) = text
        .split_once('.')
        .map_or((text.as_str(), ""), |(i, f)| (i, f));
    let i: u64 = int.parse().unwrap_or(0);
    let has_fraction = !fraction.is_empty();
    match language.split(['-', '_']).next().unwrap_or("") {
        "pl" => {
            if has_fraction {
                Category::Other
            } else if i == 1 {
                Category::One
            } else if (2..=4).contains(&(i % 10)) && !(12..=14).contains(&(i % 100)) {
                Category::Few
            } else {
                Category::Many
            }
        }
        "ja" | "zh" | "ko" | "vi" | "th" => Category::Other,
        "fr" | "pt" => {
            if i == 0 || (i == 1 && !has_fraction) {
                Category::One
            } else {
                Category::Other
            }
        }
        _ => {
            if i == 1 && !has_fraction {
                Category::One
            } else {
                Category::Other
            }
        }
    }
}

/// The `.ftl` text is not valid.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    pub line: usize,
    pub message: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for ParseError {}

#[derive(Clone, Debug, PartialEq)]
enum Part {
    Text(String),
    Var(String),
    Number(String),
    Reference(String),
    Select {
        var: String,
        variants: Vec<(String, Pattern)>,
        default: usize,
    },
}

type Pattern = Vec<Part>;

/// The messages of one language.
#[derive(Clone, Debug, Default)]
pub struct Catalog {
    messages: HashMap<String, Pattern>,
}

impl Catalog {
    /// Reads a `.ftl` file.
    ///
    /// # Errors
    /// On a message whose braces do not balance or whose selector has no default variant.
    pub fn parse(source: &str) -> Result<Self, ParseError> {
        let source = source.replace("\r\n", "\n");
        let lines: Vec<&str> = source.split('\n').collect();
        let mut messages = HashMap::new();
        let mut i = 0;
        while i < lines.len() {
            let line = lines[i];
            let starts_message = line.starts_with(|c: char| c.is_ascii_alphabetic());
            if !starts_message && !line.starts_with('-') {
                i += 1; // blank lines, comments, and stray text
                continue;
            }
            let (end, text) = entry(&lines, i);
            if starts_message && let Some((id, value)) = text.split_once('=') {
                let pattern = parse_pattern(value.trim_start_matches(' '), i + 1)?;
                if !pattern.is_empty() {
                    messages.insert(id.trim().to_string(), pattern);
                }
            }
            i = end;
        }
        Ok(Self { messages })
    }

    #[must_use]
    pub fn contains(&self, id: &str) -> bool {
        self.messages.contains_key(id)
    }

    /// The text of message `id` for `language` with `args`, or `None` if there is no such message.
    #[must_use]
    pub fn format(&self, language: &str, id: &str, args: &[(&str, Value)]) -> Option<String> {
        let pattern = self.messages.get(id)?;
        let mut out = String::new();
        self.write(language, pattern, args, &mut out, 0);
        Some(out)
    }

    fn write(
        &self,
        language: &str,
        pattern: &Pattern,
        args: &[(&str, Value)],
        out: &mut String,
        depth: usize,
    ) {
        let arg = |name: &str| args.iter().find(|(n, _)| *n == name).map(|(_, v)| v);
        for part in pattern {
            match part {
                Part::Text(t) => out.push_str(t),
                Part::Var(name) | Part::Number(name) => {
                    if let Some(v) = arg(name) {
                        out.push_str(&v.to_string());
                    }
                }
                Part::Reference(id) if depth < 8 => {
                    if let Some(p) = self.messages.get(id) {
                        self.write(language, p, args, out, depth + 1);
                    }
                }
                Part::Reference(_) => {}
                Part::Select {
                    var,
                    variants,
                    default,
                } => {
                    let chosen = select(language, arg(var), variants).unwrap_or(*default);
                    self.write(language, &variants[chosen].1, args, out, depth);
                }
            }
        }
    }
}

/// The variant a selector picks: an exact number first, then the plural category, or a text key.
fn select(language: &str, value: Option<&Value>, variants: &[(String, Pattern)]) -> Option<usize> {
    match value? {
        Value::Text(t) => variants.iter().position(|(k, _)| k == t),
        Value::Number(n) => {
            let exact = variants
                .iter()
                .position(|(k, _)| k.parse::<f64>().is_ok_and(|x| (x - n).abs() < f64::EPSILON));
            exact.or_else(|| {
                let category = plural_category(language, *n).name();
                variants.iter().position(|(k, _)| k == category)
            })
        }
    }
}

/// The lines of the entry that starts at `start`, joined with the indentation taken off, and the
/// index of the line after it. Attributes (`.name = ...` lines outside braces) are dropped.
fn entry(lines: &[&str], start: usize) -> (usize, String) {
    let mut text = lines[start].to_string();
    let mut depth = brace_balance(lines[start]);
    let mut end = start + 1;
    let mut pending_blank = 0;
    while end < lines.len() {
        let line = lines[end];
        if depth > 0 || line.starts_with([' ', '\t']) && !line.trim().is_empty() {
            let trimmed = line.trim_start();
            if depth <= 0 && trimmed.starts_with('.') && trimmed.contains('=') {
                // an attribute: the value ends here, and what follows belongs to it
                end += 1;
                while end < lines.len()
                    && (lines[end].starts_with([' ', '\t']) || lines[end].trim().is_empty())
                {
                    end += 1;
                }
                break;
            }
            for _ in 0..pending_blank {
                text.push('\n');
            }
            pending_blank = 0;
            text.push('\n');
            text.push_str(trimmed);
            depth += brace_balance(line);
            end += 1;
        } else if line.trim().is_empty() {
            pending_blank += 1;
            end += 1;
        } else {
            break;
        }
    }
    (end, text)
}

fn brace_balance(line: &str) -> i32 {
    let mut balance = 0;
    let mut in_string = false;
    for c in line.chars() {
        match c {
            '"' => in_string = !in_string,
            '{' if !in_string => balance += 1,
            '}' if !in_string => balance -= 1,
            _ => {}
        }
    }
    balance
}

fn parse_pattern(text: &str, line: usize) -> Result<Pattern, ParseError> {
    let chars: Vec<char> = text.chars().collect();
    let mut at = 0;
    let pattern = pattern_until(&chars, &mut at, line, false)?;
    if at < chars.len() {
        return Err(ParseError {
            line,
            message: "unbalanced `}`".into(),
        });
    }
    Ok(pattern)
}

/// Reads text and placeables up to the end, or (in a variant) up to the next variant or the
/// closing brace of the selector.
fn pattern_until(
    chars: &[char],
    at: &mut usize,
    line: usize,
    in_variant: bool,
) -> Result<Pattern, ParseError> {
    let mut parts = Vec::new();
    let mut text = String::new();
    while *at < chars.len() {
        let c = chars[*at];
        if in_variant && (c == '}' || (c == '\n' && next_is_variant(chars, *at + 1))) {
            break;
        }
        match c {
            '{' => {
                if !text.is_empty() {
                    parts.push(Part::Text(std::mem::take(&mut text)));
                }
                *at += 1;
                if let Some(part) = placeable(chars, at, line)? {
                    parts.push(part);
                }
            }
            '}' => {
                return Err(ParseError {
                    line,
                    message: "unbalanced `}`".into(),
                });
            }
            _ => {
                text.push(c);
                *at += 1;
            }
        }
    }
    let trimmed = text.trim_end_matches([' ', '\n']);
    if in_variant {
        let end = trimmed.len();
        text.truncate(end);
    }
    if !text.is_empty() {
        parts.push(Part::Text(text));
    }
    Ok(parts)
}

/// Whether the line starting at `from` begins (after blanks) a variant: `[key]` or `*[key]`.
fn next_is_variant(chars: &[char], from: usize) -> bool {
    let mut i = from;
    while i < chars.len() && chars[i] == ' ' {
        i += 1;
    }
    if chars.get(i) == Some(&'*') {
        i += 1;
    }
    chars.get(i) == Some(&'[')
}

#[allow(clippy::missing_const_for_fn)] // `char::is_whitespace` is not const
fn skip_blanks(chars: &[char], at: &mut usize) {
    while *at < chars.len() && chars[*at].is_whitespace() {
        *at += 1;
    }
}

fn word(chars: &[char], at: &mut usize) -> String {
    let start = *at;
    while *at < chars.len()
        && (chars[*at].is_alphanumeric() || matches!(chars[*at], '_' | '-' | '.'))
    {
        *at += 1;
    }
    chars[start..*at].iter().collect()
}

/// After a `{`: one placeable, up to and including its `}`.
fn placeable(chars: &[char], at: &mut usize, line: usize) -> Result<Option<Part>, ParseError> {
    let fail = |m: &str| {
        Err(ParseError {
            line,
            message: m.into(),
        })
    };
    skip_blanks(chars, at);
    let part = match chars.get(*at) {
        Some('$') => {
            *at += 1;
            let name = word(chars, at);
            skip_blanks(chars, at);
            if chars.get(*at) == Some(&'-') && chars.get(*at + 1) == Some(&'>') {
                *at += 2;
                return select_body(chars, at, line, name).map(Some);
            }
            Some(Part::Var(name))
        }
        Some('"') => {
            *at += 1;
            let start = *at;
            while *at < chars.len() && chars[*at] != '"' {
                *at += 1;
            }
            let literal: String = chars[start..*at].iter().collect();
            *at += 1;
            Some(Part::Text(literal))
        }
        Some('-') => {
            word(chars, at); // a term: not supported, contributes nothing
            None
        }
        Some(c) if c.is_ascii_digit() => {
            let n = word(chars, at);
            Some(Part::Text(n))
        }
        Some(_) => {
            let name = word(chars, at);
            skip_blanks(chars, at);
            if chars.get(*at) == Some(&'(') {
                // NUMBER($n) and friends: only NUMBER is understood, as the plain value
                *at += 1;
                skip_blanks(chars, at);
                let arg = if chars.get(*at) == Some(&'$') {
                    *at += 1;
                    Some(word(chars, at))
                } else {
                    None
                };
                while *at < chars.len() && chars[*at] != ')' {
                    *at += 1;
                }
                *at += 1;
                skip_blanks(chars, at);
                if chars.get(*at) == Some(&'-') && chars.get(*at + 1) == Some(&'>') {
                    *at += 2;
                    return select_body(chars, at, line, arg.unwrap_or_default()).map(Some);
                }
                (name == "NUMBER").then(|| arg.map(Part::Number)).flatten()
            } else {
                Some(Part::Reference(name))
            }
        }
        None => return fail("unclosed `{`"),
    };
    skip_blanks(chars, at);
    if chars.get(*at) != Some(&'}') {
        return fail("expected `}`");
    }
    *at += 1;
    Ok(part)
}

/// After `->`: the variants and the closing `}`.
fn select_body(
    chars: &[char],
    at: &mut usize,
    line: usize,
    var: String,
) -> Result<Part, ParseError> {
    let mut variants: Vec<(String, Pattern)> = Vec::new();
    let mut default = None;
    loop {
        skip_blanks(chars, at);
        match chars.get(*at) {
            Some('}') => {
                *at += 1;
                break;
            }
            None => {
                return Err(ParseError {
                    line,
                    message: "unclosed selector".into(),
                });
            }
            _ => {}
        }
        let star = chars.get(*at) == Some(&'*');
        if star {
            *at += 1;
        }
        if chars.get(*at) != Some(&'[') {
            return Err(ParseError {
                line,
                message: "expected a variant `[key]`".into(),
            });
        }
        *at += 1;
        let start = *at;
        while *at < chars.len() && chars[*at] != ']' {
            *at += 1;
        }
        let key: String = chars[start..*at]
            .iter()
            .collect::<String>()
            .trim()
            .to_string();
        *at += 1;
        while chars.get(*at) == Some(&' ') {
            *at += 1;
        }
        let body = pattern_until(chars, at, line, true)?;
        if star {
            default = Some(variants.len());
        }
        variants.push((key, body));
    }
    let default = default.ok_or_else(|| ParseError {
        line,
        message: "a selector needs a default variant `*[other]`".into(),
    })?;
    Ok(Part::Select {
        var,
        variants,
        default,
    })
}

/// The app's translations and the language in use. Cheap to clone; clones share the language.
#[derive(Clone)]
pub struct I18n {
    language: Signal<String>,
    catalogs: Rc<Vec<(String, Catalog)>>,
    fallback: Rc<str>,
}

impl fmt::Debug for I18n {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("I18n")
            .field("language", &self.language.get_untracked())
            .finish_non_exhaustive()
    }
}

impl I18n {
    /// Catalogs as `(language, ftl text)`, starting in `initial`. Messages missing from a language
    /// fall back to the first one listed.
    ///
    /// # Errors
    /// If a file does not parse; the error names the language.
    pub fn new(files: &[(&str, &str)], initial: &str) -> Result<Self, ParseError> {
        let mut catalogs = Vec::new();
        for (language, text) in files {
            let catalog = Catalog::parse(text).map_err(|e| ParseError {
                line: e.line,
                message: format!("{language}: {}", e.message),
            })?;
            catalogs.push(((*language).to_string(), catalog));
        }
        let fallback = catalogs
            .first()
            .map_or_else(String::new, |(l, _)| l.clone());
        Ok(Self {
            language: create_signal(initial.to_string()),
            catalogs: Rc::new(catalogs),
            fallback: fallback.into(),
        })
    }

    /// The language in use; reading it in a computation follows changes.
    #[must_use]
    pub fn language(&self) -> String {
        self.language.get()
    }

    /// Switches language; everything that read it through [`I18n::tr`] updates.
    pub fn set_language(&self, language: &str) {
        self.language.set_if_changed(language.to_string());
    }

    /// The languages that were given.
    #[must_use]
    pub fn languages(&self) -> Vec<String> {
        self.catalogs.iter().map(|(l, _)| l.clone()).collect()
    }

    /// Message `id` with no arguments.
    #[must_use]
    pub fn t(&self, id: &str) -> String {
        self.tr(id, &[])
    }

    /// Message `id` in the current language (or the first one, if that lacks it). An unknown id
    /// comes back as itself, so a missing translation shows up on the page instead of failing.
    #[must_use]
    pub fn tr(&self, id: &str, args: &[(&str, Value)]) -> String {
        let language = self.language.get();
        let ordered = std::iter::once(language.as_str()).chain(std::iter::once(&*self.fallback));
        for l in ordered {
            let catalog = self
                .catalogs
                .iter()
                .find(|(name, _)| name == l)
                .or_else(|| {
                    // `pl-PL` finds `pl`
                    self.catalogs
                        .iter()
                        .find(|(name, _)| l.split(['-', '_']).next() == Some(name.as_str()))
                });
            if let Some((name, c)) = catalog
                && let Some(text) = c.format(name, id, args)
            {
                return text;
            }
        }
        id.to_string()
    }
}

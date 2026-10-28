//! Media queries: `@media (hover: hover) and (pointer: fine)`, `(prefers-reduced-motion: reduce)`, widths.

use crate::value::{Value, split_commas};

/// A light or dark preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorScheme {
    Light,
    Dark,
}

/// What the main pointing device is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pointer {
    None,
    Coarse,
    Fine,
}

/// The environment a query is asked about.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MediaEnv {
    pub width: f32,
    pub height: f32,
    pub color_scheme: ColorScheme,
    pub reduced_motion: bool,
    /// Whether the primary pointer can hover.
    pub hover: bool,
    pub pointer: Pointer,
}

impl Default for MediaEnv {
    /// A 1280 x 800 desktop with a mouse, a dark scheme and motion allowed.
    fn default() -> Self {
        Self {
            width: 1280.0,
            height: 800.0,
            color_scheme: ColorScheme::Dark,
            reduced_motion: false,
            hover: true,
            pointer: Pointer::Fine,
        }
    }
}

/// One query of a list: a type and conditions that must all hold, possibly negated.
#[derive(Debug, Clone, PartialEq)]
pub struct MediaQuery {
    negated: bool,
    /// `all`, `screen` or `print`; `None` is `all`.
    media_type: Option<String>,
    conditions: Vec<Condition>,
}

#[derive(Debug, Clone, PartialEq)]
enum Condition {
    /// `(name)` or `(name: value)`.
    Feature(String, Option<Value>),
    /// `(width >= 600px)`: a feature, a comparison and a length.
    Range(String, Comparison, f32),
    /// Something this engine cannot evaluate: the query does not match.
    Unknown,
    Not(Box<Condition>),
    /// `(100px < width <= 200px)`: both comparisons hold.
    All(Vec<Condition>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Comparison {
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    Equal,
}

/// A length in a media query, in pixels (`rem` and `em` are 16 px there, as the spec says).
fn px(value: &Value) -> Option<f32> {
    match value {
        Value::Dimension(n, unit) => match unit.as_str() {
            "px" => Some(*n),
            "em" | "rem" => Some(n * 16.0),
            _ => None,
        },
        Value::Number(n) if *n == 0.0 => Some(0.0),
        _ => None,
    }
}

/// `<`, `<=`, `>`, `>=` or `=` at the start of `values`, and what follows it.
const fn comparison(values: &[Value]) -> Option<(Comparison, &[Value])> {
    let [Value::Delim(op), rest @ ..] = values else {
        return None;
    };
    let (equal, rest) = match rest {
        [Value::Delim('='), tail @ ..] => (true, tail),
        tail => (false, tail),
    };
    let comparison = match (op, equal) {
        ('<', false) => Comparison::Less,
        ('<', true) => Comparison::LessEqual,
        ('>', false) => Comparison::Greater,
        ('>', true) => Comparison::GreaterEqual,
        ('=', false) => Comparison::Equal,
        _ => return None,
    };
    Some((comparison, rest))
}

/// The comparison seen from the other side: `100px < width` is `width > 100px`.
const fn flipped(c: Comparison) -> Comparison {
    match c {
        Comparison::Less => Comparison::Greater,
        Comparison::LessEqual => Comparison::GreaterEqual,
        Comparison::Greater => Comparison::Less,
        Comparison::GreaterEqual => Comparison::LessEqual,
        Comparison::Equal => Comparison::Equal,
    }
}

fn parse_condition(values: &[Value]) -> Condition {
    if let [low, rest @ ..] = values
        && let Some(low) = px(low)
        && let Some((first, rest)) = comparison(rest)
        && let [Value::Ident(name), rest @ ..] = rest
    {
        let name = name.to_ascii_lowercase();
        let mut all = vec![Condition::Range(name.clone(), flipped(first), low)];
        if !rest.is_empty() {
            let Some((second, tail)) = comparison(rest) else {
                return Condition::Unknown;
            };
            let Some(high) = tail.first().and_then(px) else {
                return Condition::Unknown;
            };
            all.push(Condition::Range(name, second, high));
        }
        return Condition::All(all);
    }
    match values {
        [Value::Ident(name)] => Condition::Feature(name.to_ascii_lowercase(), None),
        [Value::Ident(name), Value::Delim(':'), value] => {
            Condition::Feature(name.to_ascii_lowercase(), Some(value.clone()))
        }
        [Value::Ident(name), rest @ ..] if comparison(rest).is_some() => {
            let (comparison, tail) = comparison(rest).expect("checked above");
            tail.first().and_then(px).map_or(Condition::Unknown, |v| {
                Condition::Range(name.to_ascii_lowercase(), comparison, v)
            })
        }
        [Value::Ident(not), Value::Block('(', inner)] if not.eq_ignore_ascii_case("not") => {
            Condition::Not(Box::new(parse_condition(inner)))
        }
        _ => Condition::Unknown,
    }
}

/// Parses the prelude of an `@media` rule.
#[must_use]
pub fn parse_media_queries(prelude: &[Value]) -> Vec<MediaQuery> {
    let mut queries = Vec::new();
    for part in split_commas(prelude) {
        let mut query = MediaQuery {
            negated: false,
            media_type: None,
            conditions: Vec::new(),
        };
        let mut ok = true;
        for (i, value) in part.iter().enumerate() {
            match value {
                Value::Ident(word) => match word.to_ascii_lowercase().as_str() {
                    "not" if i == 0 => query.negated = true,
                    "only" | "and" => {}
                    kind @ ("all" | "screen" | "print" | "speech") => {
                        query.media_type = Some(kind.to_string());
                    }
                    _ => ok = false,
                },
                Value::Block('(', inner) => query.conditions.push(parse_condition(inner)),
                _ => ok = false,
            }
        }
        if ok {
            queries.push(query);
        } else {
            // A malformed query matches nothing (it is `not all`).
            queries.push(MediaQuery {
                negated: true,
                media_type: None,
                conditions: Vec::new(),
            });
        }
    }
    queries
}

impl MediaQuery {
    /// Whether the query holds in `env`.
    #[must_use]
    pub fn matches(&self, env: &MediaEnv) -> bool {
        let type_ok = self
            .media_type
            .as_deref()
            .is_none_or(|t| matches!(t, "all" | "screen"));
        let all = type_ok && self.conditions.iter().all(|c| condition_holds(c, env));
        all != self.negated
    }
}

/// Whether every query level of a rule holds; one level matches when any of its queries does.
#[must_use]
pub fn all_match(levels: &[Vec<MediaQuery>], env: &MediaEnv) -> bool {
    levels
        .iter()
        .all(|queries| queries.iter().any(|q| q.matches(env)))
}

fn condition_holds(condition: &Condition, env: &MediaEnv) -> bool {
    match condition {
        Condition::Unknown => false,
        Condition::Not(inner) => !condition_holds(inner, env),
        Condition::All(all) => all.iter().all(|c| condition_holds(c, env)),
        Condition::Range(name, comparison, value) => {
            let actual = match name.as_str() {
                "width" => env.width,
                "height" => env.height,
                _ => return false,
            };
            match comparison {
                Comparison::Less => actual < *value,
                Comparison::LessEqual => actual <= *value,
                Comparison::Greater => actual > *value,
                Comparison::GreaterEqual => actual >= *value,
                Comparison::Equal => (actual - value).abs() < 0.01,
            }
        }
        Condition::Feature(name, value) => feature_holds(name, value.as_ref(), env),
    }
}

fn feature_holds(name: &str, value: Option<&Value>, env: &MediaEnv) -> bool {
    let word = value.and_then(Value::ident);
    let length = value.and_then(px);
    match name {
        "width" | "min-width" | "max-width" | "height" | "min-height" | "max-height" => {
            let (actual, wanted) = match (name.ends_with("height"), length) {
                (true, Some(w)) => (env.height, w),
                (false, Some(w)) => (env.width, w),
                _ => return false,
            };
            match name.split('-').next() {
                Some("min") => actual >= wanted,
                Some("max") => actual <= wanted,
                _ => (actual - wanted).abs() < 0.01,
            }
        }
        "prefers-color-scheme" => match word.as_deref() {
            Some("dark") => env.color_scheme == ColorScheme::Dark,
            Some("light") => env.color_scheme == ColorScheme::Light,
            _ => false,
        },
        "prefers-reduced-motion" => match word.as_deref() {
            Some("reduce") => env.reduced_motion,
            Some("no-preference") => !env.reduced_motion,
            _ => false,
        },
        "hover" | "any-hover" => match word.as_deref() {
            Some("hover") | None => env.hover,
            Some("none") => !env.hover,
            _ => false,
        },
        "pointer" | "any-pointer" => match word.as_deref() {
            Some("fine") => env.pointer == Pointer::Fine,
            Some("coarse") => env.pointer == Pointer::Coarse,
            Some("none") => env.pointer == Pointer::None,
            None => env.pointer != Pointer::None,
            _ => false,
        },
        "orientation" => match word.as_deref() {
            Some("portrait") => env.height >= env.width,
            Some("landscape") => env.width > env.height,
            _ => false,
        },
        _ => false,
    }
}

//! Lengths: pixels, percentages, and `calc()`/`min()`/`max()` of them, resolved once the box they refer to is known.

use std::rc::Rc;

use crate::value::Value;

/// What a length unit is measured against while a style is computed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Units {
    /// The font size of the element (what `em` is), in pixels.
    pub font_size: f32,
    /// The font size of the root (what `rem` is).
    pub root_font_size: f32,
    pub viewport_width: f32,
    pub viewport_height: f32,
    /// How many device pixels a CSS pixel is (1 on a desktop, 2.6 on a phone): absolute units are multiplied by it.
    pub zoom: f32,
}

impl Default for Units {
    fn default() -> Self {
        Self {
            font_size: 16.0,
            root_font_size: 16.0,
            viewport_width: 1280.0,
            viewport_height: 800.0,
            zoom: 1.0,
        }
    }
}

/// A length as the cascade leaves it: percentages (and what mixes them with pixels) wait for the layout.
#[derive(Debug, Clone, PartialEq)]
pub enum Length {
    Px(f32),
    /// A percentage of whatever the property refers to (the containing block's width, usually).
    Percent(f32),
    Calc(Rc<Calc>),
}

/// An expression over lengths.
#[derive(Debug, Clone, PartialEq)]
pub enum Calc {
    Px(f32),
    Percent(f32),
    Add(Rc<Calc>, Rc<Calc>),
    Sub(Rc<Calc>, Rc<Calc>),
    Mul(Rc<Calc>, f32),
    Min(Vec<Calc>),
    Max(Vec<Calc>),
}

impl Length {
    pub const ZERO: Self = Self::Px(0.0);

    /// The length in pixels, given what 100% is.
    #[must_use]
    pub fn resolve(&self, basis: f32) -> f32 {
        match self {
            Self::Px(px) => *px,
            Self::Percent(p) => basis * p / 100.0,
            Self::Calc(c) => c.resolve(basis),
        }
    }

    /// The length when it has no percentage in it (`None` otherwise).
    #[must_use]
    pub fn as_px(&self) -> Option<f32> {
        match self {
            Self::Px(px) => Some(*px),
            Self::Percent(_) => None,
            Self::Calc(c) => c.constant(),
        }
    }

    /// Whether it depends on a percentage.
    #[must_use]
    pub fn is_relative(&self) -> bool {
        self.as_px().is_none()
    }

    fn from_calc(c: Calc) -> Self {
        match c {
            Calc::Px(px) => Self::Px(px),
            Calc::Percent(p) => Self::Percent(p),
            other => other
                .constant()
                .map_or_else(|| Self::Calc(Rc::new(other)), Self::Px),
        }
    }
}

impl Calc {
    #[must_use]
    pub fn resolve(&self, basis: f32) -> f32 {
        match self {
            Self::Px(px) => *px,
            Self::Percent(p) => basis * p / 100.0,
            Self::Add(a, b) => a.resolve(basis) + b.resolve(basis),
            Self::Sub(a, b) => a.resolve(basis) - b.resolve(basis),
            Self::Mul(a, k) => a.resolve(basis) * k,
            Self::Min(items) => items
                .iter()
                .map(|c| c.resolve(basis))
                .fold(f32::INFINITY, f32::min),
            Self::Max(items) => items
                .iter()
                .map(|c| c.resolve(basis))
                .fold(f32::NEG_INFINITY, f32::max),
        }
    }

    /// The value when no percentage is involved.
    #[must_use]
    pub fn constant(&self) -> Option<f32> {
        match self {
            Self::Px(px) => Some(*px),
            Self::Percent(_) => None,
            Self::Add(a, b) => Some(a.constant()? + b.constant()?),
            Self::Sub(a, b) => Some(a.constant()? - b.constant()?),
            Self::Mul(a, k) => Some(a.constant()? * k),
            Self::Min(items) => items
                .iter()
                .map(Self::constant)
                .try_fold(f32::INFINITY, |m, v| Some(m.min(v?))),
            Self::Max(items) => items
                .iter()
                .map(Self::constant)
                .try_fold(f32::NEG_INFINITY, |m, v| Some(m.max(v?))),
        }
    }
}

/// A dimension in pixels: `None` for a unit that is not a length.
fn unit_to_px(value: f32, unit: &str, units: &Units) -> Option<f32> {
    Some(match unit {
        "px" => value * units.zoom,
        "em" => value * units.font_size,
        "rem" => value * units.root_font_size,
        "ex" | "ch" => value * units.font_size * 0.5,
        "vw" | "svw" | "lvw" | "dvw" => value * units.viewport_width / 100.0,
        "vh" | "svh" | "lvh" | "dvh" => value * units.viewport_height / 100.0,
        "vmin" | "svmin" | "lvmin" | "dvmin" => {
            value * units.viewport_width.min(units.viewport_height) / 100.0
        }
        "vmax" | "svmax" | "lvmax" | "dvmax" => {
            value * units.viewport_width.max(units.viewport_height) / 100.0
        }
        "pt" => value * 96.0 / 72.0 * units.zoom,
        "pc" => value * 16.0 * units.zoom,
        "in" => value * 96.0 * units.zoom,
        "cm" => value * 96.0 / 2.54 * units.zoom,
        "mm" => value * 96.0 / 25.4 * units.zoom,
        "q" => value * 96.0 / 101.6 * units.zoom,
        _ => return None,
    })
}

/// A single length value: `16px`, `2rem`, `50%`, `0`, or a function (`calc()`, `min()`, `max()`, `clamp()`).
#[must_use]
pub fn parse_length(value: &Value, units: &Units) -> Option<Length> {
    match value {
        Value::Dimension(n, unit) => unit_to_px(*n, unit, units).map(Length::Px),
        Value::Percent(p) => Some(Length::Percent(*p)),
        Value::Number(n) if *n == 0.0 => Some(Length::ZERO),
        Value::Function(name, args)
            if matches!(name.as_str(), "calc" | "min" | "max" | "clamp") =>
        {
            parse_function(name, args, units).map(Length::from_calc)
        }
        _ => None,
    }
}

fn parse_function(name: &str, args: &[Value], units: &Units) -> Option<Calc> {
    match name {
        "calc" => parse_sum(args, units),
        "min" | "max" | "clamp" => {
            let parts = crate::value::split_commas(args);
            let items: Option<Vec<Calc>> = parts.iter().map(|p| parse_sum(p, units)).collect();
            let items = items?;
            match name {
                "min" => (!items.is_empty()).then_some(Calc::Min(items)),
                "max" => (!items.is_empty()).then_some(Calc::Max(items)),
                _ => {
                    let [low, value, high] = <[Calc; 3]>::try_from(items).ok()?;
                    Some(Calc::Max(vec![low, Calc::Min(vec![value, high])]))
                }
            }
        }
        _ => None,
    }
}

/// `a + b - c`, with `*` and `/` binding tighter and parentheses grouping.
fn parse_sum(values: &[Value], units: &Units) -> Option<Calc> {
    match sum(values, units)? {
        Term::Length(c) => Some(c),
        // `calc(2)` is a number, not a length.
        Term::Number(_) => None,
    }
}

/// A value in a calculation: a plain number, or a length (so `2 * 10px` and `10px * 2` both work, and so
/// does `(3 + 4) * 1px`).
enum Term {
    Number(f32),
    Length(Calc),
}

/// `a + b - c` over terms: numbers with numbers, lengths with lengths.
fn sum(values: &[Value], units: &Units) -> Option<Term> {
    let mut at = 0;
    let mut left = product(values, &mut at, units)?;
    while let Some(Value::Delim(op @ ('+' | '-'))) = values.get(at) {
        at += 1;
        let right = product(values, &mut at, units)?;
        let plus = *op == '+';
        left = match (left, right) {
            (Term::Number(a), Term::Number(b)) => Term::Number(if plus { a + b } else { a - b }),
            (Term::Length(a), Term::Length(b)) => Term::Length(if plus {
                Calc::Add(Rc::new(a), Rc::new(b))
            } else {
                Calc::Sub(Rc::new(a), Rc::new(b))
            }),
            _ => return None,
        };
    }
    (at == values.len()).then_some(left)
}

fn term(values: &[Value], at: &mut usize, units: &Units) -> Option<Term> {
    let value = values.get(*at)?;
    *at += 1;
    Some(match value {
        Value::Number(n) => Term::Number(*n),
        Value::Block('(', inner) => sum(inner, units)?,
        Value::Function(name, args)
            if matches!(name.as_str(), "calc" | "min" | "max" | "clamp") =>
        {
            Term::Length(parse_function(name, args, units)?)
        }
        other => match parse_length(other, units)? {
            Length::Px(px) => Term::Length(Calc::Px(px)),
            Length::Percent(p) => Term::Length(Calc::Percent(p)),
            Length::Calc(c) => Term::Length((*c).clone()),
        },
    })
}

fn product(values: &[Value], at: &mut usize, units: &Units) -> Option<Term> {
    let mut left = term(values, at, units)?;
    while let Some(Value::Delim(op @ ('*' | '/'))) = values.get(*at) {
        *at += 1;
        let right = term(values, at, units)?;
        left = match (left, right, *op) {
            (Term::Number(a), Term::Number(b), '*') => Term::Number(a * b),
            (Term::Number(a), Term::Number(b), _) if b != 0.0 => Term::Number(a / b),
            (Term::Length(l), Term::Number(k), '*') | (Term::Number(k), Term::Length(l), '*') => {
                Term::Length(Calc::Mul(Rc::new(l), k))
            }
            (Term::Length(l), Term::Number(k), _) if k != 0.0 => {
                Term::Length(Calc::Mul(Rc::new(l), 1.0 / k))
            }
            _ => return None,
        };
    }
    Some(left)
}
